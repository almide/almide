//! StdlibLoweringPass: dispatch Module calls into per-target IR nodes.
//!
//! Post Stdlib Declarative Unification, every stdlib module lives in
//! `stdlib/<m>.almd` with `@inline_rust` / `@wasm_intrinsic` templates.
//! This pass parses those attrs (once per run via the `INLINE_RUST`
//! thread-local) and rewrites `CallTarget::Module { module, func }`
//! into `IrExprKind::InlineRust { template, args }` for the Rust
//! target. The WASM emitter keeps its own dispatch in
//! `emit_wasm/calls_*.rs`; this pass skips WASM emission.

use std::cell::RefCell;
use std::collections::{HashSet, HashMap};
use almide_base::intern::Sym;
use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct StdlibLoweringPass;

/// Decoded `@inline_rust(...)` metadata for a bundled stdlib fn.
/// Populated once at the start of `StdlibLoweringPass::run` and
/// consumed during `rewrite_expr` on every matching call site.
#[derive(Debug, Clone)]
struct InlineRustSpec {
    /// The literal template string from `@inline_rust("...")`.
    template: String,
    /// Parameter names in order. Used to pair positional call args
    /// with their `{name}` placeholders in `template`.
    param_names: Vec<Sym>,
    /// Per-param default value, if declared in the bundled source
    /// (`fn slice(s: String, start: Int, end: Int = 9223372036854775807)`).
    /// Used to fill positional args that the caller omitted — e.g.
    /// `string.slice(s, 3)` binds `{end}` to the stored default so
    /// the emitted template renders as a 3-arg runtime call.
    defaults: Vec<Option<IrExpr>>,
}

thread_local! {
    /// (module, func) pairs that come from bundled .almd stdlib sources and
    /// have NO matching TOML/Rust runtime fn. Populated at the start of
    /// `StdlibLoweringPass::run`. For these, the rt_ rewrite is suppressed
    /// and the call stays as a `CallTarget::Module` so the walker emits a
    /// normal user-fn call.
    static BUNDLED_FNS: RefCell<HashSet<(Sym, Sym)>> = RefCell::new(HashSet::new());

    /// (module, func) → `@inline_rust` metadata. Populated once at the
    /// start of `run`; `rewrite_expr` intercepts matching module calls
    /// and emits `IrExprKind::InlineRust` instead of the legacy
    /// `arg_transforms`-backed `Named` call. This lets pure-Almide
    /// stdlib modules override the per-TOML dispatch on a fn-by-fn
    /// basis (Stage 1 of the Stdlib Declarative Unification arc).
    static INLINE_RUST: RefCell<HashMap<(Sym, Sym), InlineRustSpec>> = RefCell::new(HashMap::new());
}

fn is_bundled_only(module: Sym, func: Sym) -> bool {
    BUNDLED_FNS.with(|s| s.borrow().contains(&(module, func)))
}

fn inline_rust_spec(module: Sym, func: Sym) -> Option<InlineRustSpec> {
    INLINE_RUST.with(|s| s.borrow().get(&(module, func)).cloned())
}

/// Replace whole-identifier occurrences of `from` with `to` in raw Rust
/// template text: a match is skipped when it abuts an identifier character
/// (so `Cfb8State` never clips `MyCfb8State`) or a `.` (already-qualified
/// `aes.Cfb8State` and field accesses stay untouched).
fn replace_ident_token(text: &str, from: &str, to: &str) -> String {
    let bytes = text.as_bytes();
    let is_ident = |b: u8| b == b'_' || b.is_ascii_alphanumeric();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(from.as_bytes()) {
            let before_ok = i == 0 || (!is_ident(bytes[i - 1]) && bytes[i - 1] != b'.');
            let after = i + from.len();
            let after_ok = after >= bytes.len() || !is_ident(bytes[after]);
            if before_ok && after_ok {
                out.push_str(to);
                i = after;
                continue;
            }
        }
        // Advance one full UTF-8 char (templates may hold non-ASCII strings).
        let ch_len = text[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        out.push_str(&text[i..i + ch_len]);
        i += ch_len;
    }
    out
}

/// Strip owning / borrowing / cloning decorations from an
/// `@inline_rust` arg before it lands in an `InlineRust` node.
///
/// Upstream passes (`CloneInsertionPass`, `BorrowInsertionPass`, ...)
/// wrap args in `Clone` / `Borrow` / `ToVec` / `RcWrap` / `BoxNew`
/// based on the callee signature. But the `@inline_rust` template is
/// authoritative for Rust-level reference semantics — when the user
/// writes `&mut {b}`, they mean the VARIABLE `b`, not a clone.
/// Passing `b.clone()` into the template produces `&mut b.clone()`
/// which operates on a disposable temp and silently loses the
/// mutation.
///
/// Stripping one layer of these wrappers aligns the rendered arg with
/// the template's stated intent. Users who actually want a clone
/// should spell it out (`.clone()`) inside the template string.
fn strip_arg_decorations(expr: IrExpr) -> IrExpr {
    match expr.kind {
        IrExprKind::Clone { expr: inner } => *inner,
        IrExprKind::Borrow { expr: inner, .. } => *inner,
        IrExprKind::ToVec { expr: inner } => *inner,
        IrExprKind::RcWrap { expr: inner, .. } => *inner,
        IrExprKind::BoxNew { expr: inner } => *inner,
        _ => expr,
    }
}

/// Does the template reference `{name}` preceded by an explicit
/// borrow sigil (`&`, `&*`, `&mut`, `&mut *`)? If so the template is
/// declaring that the RENDERED arg is a place-expression, not a
/// value-expression; we strip any Clone wrapper that upstream passes
/// inserted so the mutation / borrow lands on the caller's variable
/// instead of a disposable clone.
fn template_wants_reference(template: &str, name: &str) -> bool {
    let needle = format!("{{{}}}", name);
    let Some(idx) = template.find(&needle) else { return false };
    // Walk back over horizontal whitespace + the last sigil.
    let prefix = &template[..idx];
    let trimmed = prefix.trim_end();
    // Borrow forms: `&`, `&mut`, `&*`, `&mut *`. Keyword `mut` may
    // carry trailing whitespace (`&mut `), star may come right after
    // ampersand with no space, and the `&` itself always closes the
    // immediately-preceding token.
    trimmed.ends_with('&')
        || trimmed.ends_with("&mut")
        || trimmed.ends_with("&*")
        || trimmed.ends_with("&mut *")
}

/// Extract the `@inline_rust("...")` template from an IrFunction's
/// attrs, if present. Returns None if the attribute is missing or
/// malformed (no string first-arg).
fn find_inline_rust_template(f: &IrFunction) -> Option<String> {
    use almide_lang::ast::AttrValue;
    let attr = f.attrs.iter().find(|a| a.name.as_str() == "inline_rust")?;
    let first = attr.args.first()?;
    match &first.value {
        AttrValue::String { value } => Some(value.clone()),
        _ => None,
    }
}

/// Fallback entry point: pull every `@inline_rust` template from a
/// bundled stdlib source. Needed when a consumer (notably the codegen
/// snapshot tests in `tests/`) invokes codegen without going through
/// `resolve.rs`, so `program.modules` never contains the bundled
/// `IrModule`.
///
/// The parse is delegated to `almide_lang::parse_cached`, the shared
/// process-wide AST cache used by `almide-frontend::bundled_sigs` to
/// extract type signatures. Both views derive from a single parse so
/// they cannot drift as bundled `.almd` sources evolve.
fn parse_bundled_inline_rust(module: &str) -> Vec<(Sym, InlineRustSpec)> {
    use almide_lang::ast::{AttrValue, Decl};
    let Some(source) = almide_lang::stdlib_info::bundled_source(module) else {
        return Vec::new();
    };
    let Some(program) = almide_lang::parse_cached(source) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for decl in &program.decls {
        let Decl::Fn { name, params, attrs, .. } = decl else { continue };
        let Some(attr) = attrs.iter().find(|a| a.name.as_str() == "inline_rust") else {
            continue;
        };
        let Some(first) = attr.args.first() else { continue };
        let template = match &first.value {
            AttrValue::String { value } => value.clone(),
            _ => continue,
        };
        let param_names = params.iter().map(|p| p.name).collect();
        let defaults = params.iter()
            .map(|p| p.default.as_deref().and_then(ast_expr_to_ir_literal))
            .collect();
        out.push((*name, InlineRustSpec { template, param_names, defaults }));
    }
    out
}

/// Convert an AST literal default-value expression to an `IrExpr`.
///
/// Scope: simple literals only (int / float / string / bool / `none`)
/// — the cases that show up as bundled-stdlib default parameters.
/// Anything else returns `None` and the default effectively doesn't
/// exist from this fallback path's point of view; the IR-population
/// path (which runs through the full lowering pipeline) still carries
/// the complete `IrExpr`, so the fallback only matters for tooling
/// that bypasses resolve/lower (codegen snapshot tests, ...).
fn ast_expr_to_ir_literal(expr: &almide_lang::ast::Expr) -> Option<IrExpr> {
    use almide_lang::ast::ExprKind;
    match &expr.kind {
        ExprKind::Int { value, .. } => {
            let n = value.as_i64()?;
            Some(IrExpr {
                kind: IrExprKind::LitInt { value: n },
                ty: Ty::Int,
                span: None, def_id: None,
            })
        }
        ExprKind::Float { value } => Some(IrExpr {
            kind: IrExprKind::LitFloat { value: *value },
            ty: Ty::Float,
            span: None, def_id: None,
        }),
        ExprKind::String { value } => Some(IrExpr {
            kind: IrExprKind::LitStr { value: value.clone() },
            ty: Ty::String,
            span: None, def_id: None,
        }),
        ExprKind::Bool { value } => Some(IrExpr {
            kind: IrExprKind::LitBool { value: *value },
            ty: Ty::Bool,
            span: None, def_id: None,
        }),
        ExprKind::Ident { name } if name.as_str() == "none" => Some(IrExpr {
            kind: IrExprKind::OptionNone,
            ty: Ty::option(Ty::Unknown),
            span: None, def_id: None,
        }),
        _ => None,
    }
}

impl NanoPass for StdlibLoweringPass {
    fn name(&self) -> &str { "StdlibLowering" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    fn depends_on(&self) -> Vec<&'static str> { vec!["EffectInference"] }
    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        seed_bundled_fns(&program);
        seed_inline_rust_table(&program);
        rewrite_all_bodies(&mut program);
        resolve_module_ufcs(&mut program);
        rewrite_versioned_module_names(&mut program);
        prefix_intra_module_named_calls(&mut program);
        PassResult { program, changed: true }
    }
}

// ── `run` step extraction (cog>100 decomposition, pattern 2) ──
//
// Each of these is a 1:1 text-move of one independent step from the
// original `run` body. The steps only ever run in this fixed order and
// none reads a value a LATER step produces, so splitting them into
// top-level functions called in the same order changes nothing observable.

/// Every fn in every bundled stdlib IR module is bundled-only now: the
/// TOML-backed runtime table was retired with the Stdlib Declarative
/// Unification arc, so no overlap check is needed.
fn seed_bundled_fns(program: &IrProgram) {
    let bundled_fns: HashSet<(Sym, Sym)> = program.modules.iter()
        .filter(|m| almide_lang::stdlib_info::is_bundled_module(m.name.as_str()))
        .flat_map(|m| {
            let mname = m.name;
            m.functions.iter().map(move |f| (mname, f.name))
        })
        .collect();
    BUNDLED_FNS.with(|s| *s.borrow_mut() = bundled_fns);
}

/// Build the @inline_rust dispatch table. Two sources feed it:
///
/// 1. `program.modules` — the frontend-loaded bundled stdlib
///    modules (the normal compile path routes through
///    `resolve.rs` which parses + lowers each bundled `.almd`).
///    These land as full `IrModule` entries; we read the
///    `@inline_rust` attribute directly off `IrFunction.attrs`.
///
/// 2. Bundled source fallback — code paths that bypass
///    `resolve.rs` (unit tests using `canonicalize_program` +
///    `lower_program` directly, e.g. the snapshot test suite)
///    never get bundled modules into `program.modules`. For
///    those, re-parse the embedded `stdlib/<m>.almd` source so
///    every @inline_rust fn is reachable. Skipping this would
///    make pass_stdlib_lowering silently fall back to the
///    legacy `arg_transforms::lookup` path for migrated fns,
///    which now returns `None` and emits invalid Rust.
///
/// The merge policy is "IR wins" — if a bundled module is
/// fully loaded, we take its IR-level signature (param names,
/// attribute values). Source fallback only fills in modules
/// that IR didn't provide.
fn seed_inline_rust_table(program: &IrProgram) {
    // Scan ALL modules (bundled + packages) for @inline_rust templates.
    let mut inline_rust: HashMap<(Sym, Sym), InlineRustSpec> = HashMap::new();
    for m in &program.modules {
        let mname = m.name;
        // A USER package's template references its OWN structs by their
        // package-local bare names (`Cfb8State { .. }`), but post-flatten
        // those structs are mangled (`almide_rt_aes_Cfb8State`) — the
        // pasted text failed as E0422 when the call site was in ANOTHER
        // module (aes cfb8 via `import self`). Requalify bare type tokens
        // to the decl's canonical dotted name while the owning module is
        // still known; the flatten pass rewrites dotted → mangled inside
        // templates like every other reference. Bundled stdlib types stay
        // bare — they are never mangled.
        let own_types: Vec<(String, &str)> = if almide_lang::stdlib_info::is_bundled_module(mname.as_str()) {
            Vec::new()
        } else {
            m.type_decls.iter().filter_map(|td| {
                let full = td.name.as_str();
                full.rsplit_once('.').map(|(_, base)| (base.to_string(), full))
            }).collect()
        };
        for f in &m.functions {
            let Some(mut template) = find_inline_rust_template(f) else { continue };
            for (base, full) in &own_types {
                template = replace_ident_token(&template, base, full);
            }
            let param_names = f.params.iter().map(|p| p.name).collect();
            let defaults = f.params.iter()
                .map(|p| p.default.as_ref().map(|d| (**d).clone()))
                .collect();
            inline_rust.insert((mname, f.name), InlineRustSpec { template, param_names, defaults });
        }
    }
    let loaded_bundled_modules: std::collections::HashSet<Sym> = program.modules.iter()
        .map(|m| m.name)
        .collect();
    for name in almide_lang::stdlib_info::BUNDLED_MODULES {
        let mname = almide_base::intern::sym(name);
        if loaded_bundled_modules.contains(&mname) {
            continue;
        }
        for (fname, spec) in parse_bundled_inline_rust(name) {
            inline_rust.entry((mname, fname)).or_insert(spec);
        }
    }
    INLINE_RUST.with(|s| *s.borrow_mut() = inline_rust);
}

fn rewrite_all_bodies(program: &mut IrProgram) {
    for func in &mut program.functions {
        func.body = rewrite_expr(std::mem::take(&mut func.body));
    }
    for tl in &mut program.top_lets {
        tl.value = rewrite_expr(std::mem::take(&mut tl.value));
    }
    // Process module functions and top_lets
    for module in &mut program.modules {
        for func in &mut module.functions {
            func.body = rewrite_expr(std::mem::take(&mut func.body));
        }
        for tl in &mut module.top_lets {
            tl.value = rewrite_expr(std::mem::take(&mut tl.value));
        }
    }
}

/// Resolve remaining bare UFCS calls in module bodies (checker doesn't fully type them).
fn resolve_module_ufcs(program: &mut IrProgram) {
    for module in &mut program.modules {
        let sibling_names: Vec<String> = module.functions.iter()
            .map(|f| f.name.to_string())
            .collect();
        for func in &mut module.functions {
            func.body = resolve_unresolved_ufcs(std::mem::take(&mut func.body), &sibling_names);
        }
        for tl in &mut module.top_lets {
            tl.value = resolve_unresolved_ufcs(std::mem::take(&mut tl.value), &sibling_names);
        }
    }
}

/// Build versioned name mapping (original module name → versioned name,
/// e.g. "json" → "json_v2" when `versioned_name` is set) and rewrite
/// `CallTarget::Module` names to versioned names in all function bodies.
fn rewrite_versioned_module_names(program: &mut IrProgram) {
    let version_map: std::collections::HashMap<String, String> = program.modules.iter()
        .filter_map(|m| m.versioned_name.map(|v| (m.name.to_string(), v.to_string())))
        .collect();
    if version_map.is_empty() {
        return;
    }
    for func in &mut program.functions {
        func.body = rewrite_module_names(std::mem::take(&mut func.body), &version_map);
    }
    for tl in &mut program.top_lets {
        tl.value = rewrite_module_names(std::mem::take(&mut tl.value), &version_map);
    }
    for module in &mut program.modules {
        for func in &mut module.functions {
            func.body = rewrite_module_names(std::mem::take(&mut func.body), &version_map);
        }
        for tl in &mut module.top_lets {
            tl.value = rewrite_module_names(std::mem::take(&mut tl.value), &version_map);
        }
    }
}

/// Prefix intra-module Named calls to match renamed definitions.
fn prefix_intra_module_named_calls(program: &mut IrProgram) {
    for module in &mut program.modules {
        let sibling_names: Vec<String> = module.functions.iter()
            .map(|f| f.name.to_string())
            .collect();
        let mod_name = module.versioned_name
            .map(|v| v.to_string())
            .unwrap_or_else(|| module.name.to_string());
        for func in &mut module.functions {
            func.body = prefix_intra_module_calls(std::mem::take(&mut func.body), &mod_name, &sibling_names);
        }
        for tl in &mut module.top_lets {
            tl.value = prefix_intra_module_calls(std::mem::take(&mut tl.value), &mod_name, &sibling_names);
        }
    }
}

/// `Call { target: Module { module, func, .. }, args, type_args }` arm of
/// [`rewrite_expr`]. Always returns early — every branch either lowers the
/// call to a different node or leaves it as a `Module` call — so it never
/// falls through to the generic `kind = ...` tail.
fn rewrite_expr_call_module(module: Sym, func: Sym, args: Vec<IrExpr>, type_args: Vec<Ty>, ty: Ty, span: Option<almide_base::Span>) -> IrExpr {
    // Stage 3c: list operations migrate to bundled `@inline_rust`
    // like every other module, BUT the Rust target needs the
    // fused-iterator lowering (`IterChain`) for isolated closure
    // ops (`list.map(xs, f)` outside a pipe) to stay zero-copy.
    // `StreamFusionPass` (runs earlier, pipeline-level) already
    // handles pipe chains; `try_lower_to_iter_chain` is the
    // fallback for single-call shape. Putting it BEFORE the
    // `inline_rust_spec` intercept keeps the perf win — if it
    // declines (non-closure ops like `len`, `push`), we fall
    // through to the declarative bundled dispatch below.
    if module.as_ref() == "list" {
        let args_for_fusion: Vec<IrExpr> = args.iter().cloned()
            .map(|a| rewrite_expr(a))
            .collect();
        if let Some(iter_expr) = try_lower_to_iter_chain(
            &func, args_for_fusion, &ty, span,
        ) {
            return iter_expr;
        }
    }
    // Stdlib Unification Stage 1: if a bundled stdlib fn
    // declares `@inline_rust("template")`, produce an
    // InlineRust IR node with the template + param-keyed args.
    // Bundled wins over the legacy TOML/arg_transforms path.
    if let Some(spec) = inline_rust_spec(module, func) {
        let mut rewritten_args: Vec<IrExpr> = args.into_iter()
            .map(|a| rewrite_expr(a))
            .collect();
        // Fill trailing positional args from declared defaults.
        // Bundled fns like `string.slice(s, start, end = <sentinel>)`
        // let the caller omit `end`; without this the template
        // would render with an unreplaced `{end}` placeholder.
        // The IR-population path carries full `IrExpr` defaults
        // from the lowered bundled module; the source-parse
        // fallback only supports simple literals — anything
        // else is `None` and leaves the arg unfilled (the same
        // failure mode as before this carve-out).
        if rewritten_args.len() < spec.param_names.len() {
            for default in spec.defaults.iter().skip(rewritten_args.len()) {
                if let Some(d) = default {
                    rewritten_args.push(d.clone());
                } else {
                    break;
                }
            }
        }
        // Pair each rewritten arg with the matching param name.
        // Strip pre-inserted Clone / Borrow / ToVec / BoxNew /
        // RcWrap wrappers ONLY when the template references
        // the parameter with an explicit borrow sigil
        // (`&{b}`, `&mut {b}`, `&*{b}`). In that case the
        // template owns reference semantics and the wrapper
        // would produce `&mut b.clone()` — mutation on a
        // temp, silently dropped. Elsewhere (by-value params
        // like `value.field(v, key)` that take `Value` owned
        // and reuse `v` across several `?`-propagating calls
        // after the codec derive), keep the wrappers so the
        // clone-insertion pass's ownership plumbing works.
        let paired: Vec<(Sym, IrExpr)> = spec.param_names.iter()
            .zip(rewritten_args.into_iter())
            .map(|(n, a)| {
                let a = if template_wants_reference(&spec.template, n.as_str()) {
                    strip_arg_decorations(a)
                } else {
                    a
                };
                // Stage 3: a literal Lambda arg to a bundled
                // stdlib fn needs the same clone-binding
                // treatment as the TOML path's `ArgTransform::
                // LambdaClone` — otherwise a captured var used
                // twice inside the lambda body produces a
                // move-after-move in the generated closure.
                // `prepare_lambda` is a no-op on non-Lambda
                // args, so this is safe for non-closure params.
                let a = prepare_lambda(a);
                (*n, a)
            })
            .collect();
        return IrExpr {
            kind: IrExprKind::InlineRust { template: spec.template, args: paired },
            ty,
            span, def_id: None,
        };
    }

    // Modules without @inline_rust (user packages without native,
    // bundled-only stdlib fns): leave as Module calls, rendered by
    // walker directly.
    let is_stdlib = almide_lang::stdlib_info::is_stdlib_module(&module);
    let has_inline = inline_rust_spec(module, func).is_some();
    if (!is_stdlib && !has_inline) || is_bundled_only(module, func) {
        let args: Vec<IrExpr> = args.into_iter().map(|a| rewrite_expr(a)).collect();
        return IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module { module, func, def_id: None },
                args,
                type_args,
            },
            ty,
            span, def_id: None,
        };
    }

    // Recurse into args first (fan auto-try is handled by FanLoweringPass)
    let args: Vec<IrExpr> = args.into_iter().map(|a| rewrite_expr(a)).collect();

    // Try to lower list operations to iterator chains (Rust-only optimization)
    if module.as_ref() == "list" {
        if let Some(iter_expr) = try_lower_to_iter_chain(&func, args.clone(), &ty, span) {
            return iter_expr;
        }
    }

    // Inline math/float/int intrinsics as native Rust expressions
    if let Some(inlined) = try_inline_intrinsic(&module, &func, &args, &ty, span) {
        return inlined;
    }

    // Post Stdlib Declarative Unification every stdlib module
    // flows through the `@inline_rust` dispatch above. Any
    // Module call that reaches here is either a user module
    // (already returned at the `is_bundled_only` branch) or a
    // stale alias that should remain visible to the walker.
    IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module, func, def_id: None },
            args,
            type_args,
        },
        ty,
        span, def_id: None,
    }
}

/// `Call { target, args, type_args }` arm of [`rewrite_expr`] (any target
/// other than `Module`). Builds and returns the full [`IrExpr`] itself
/// (rather than yielding an `IrExprKind` for the caller to wrap) because the
/// UFCS branches need to bypass the generic wrap and return an already
/// fully-rewritten node.
fn rewrite_expr_call_other(target: CallTarget, args: Vec<IrExpr>, type_args: Vec<Ty>, ty: Ty, span: Option<almide_base::Span>) -> IrExpr {
    let args: Vec<IrExpr> = args.into_iter().map(|a| rewrite_expr(a)).collect();
    let target = match target {
        CallTarget::Method { object, method } => {
            let object = Box::new(rewrite_expr(*object));
            // Fallback: bare method (no dot) on known type → convert to Module call
            if !method.contains('.') {
                if let Some(module) = resolve_module_from_ty(&object.ty, &method) {
                    let mut call_args = vec![*object];
                    call_args.extend(args);
                    let module_call = IrExpr {
                        kind: IrExprKind::Call {
                            target: CallTarget::Module { module: module.to_string().into(), func: method, def_id: None },
                            args: call_args, type_args,
                        },
                        ty: ty.clone(), span, def_id: None,
                    };
                    return rewrite_expr(module_call);
                }
            }
            // UFCS: "module.func" method → convert to Module call and process
            // Accept stdlib fns from two sources:
            //   1. TOML-backed `arg_transforms` table (legacy path)
            //   2. Bundled `@inline_rust` stdlib fns (Stdlib Declarative
            //      Unification Stage 2+) — the INLINE_RUST registry built
            //      at the top of `run`. Without this branch, deleting a
            //      fn's TOML entry after migrating it to bundled would
            //      drop UFCS dispatch (`42.to_string()`) back into the
            //      BuiltinLoweringPass Method fallback.
            if method.contains('.') && !method.ends_with(".encode") && !method.ends_with(".decode") {
                let dot_pos = method.find('.').unwrap();
                let (mod_name, func_name) = (&method[..dot_pos], &method[dot_pos+1..]);
                let mod_sym = almide_base::intern::sym(mod_name);
                let func_sym = almide_base::intern::sym(func_name);
                let is_bundled_inline_rust = INLINE_RUST.with(|s| s.borrow().contains_key(&(mod_sym, func_sym)));
                if !is_bundled_inline_rust {
                        // Not a stdlib function — leave as Method call for BuiltinLoweringPass
                        return IrExpr { kind: IrExprKind::Call {
                            target: CallTarget::Method { object, method },
                            args, type_args,
                        }, ty, span, def_id: None };
                    }
                let mut call_args = vec![*object];
                call_args.extend(args);
                let module_call = IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module { module: mod_name.into(), func: func_name.into(), def_id: None },
                        args: call_args, type_args,
                    },
                    ty: ty.clone(), span, def_id: None,
                };
                return rewrite_expr(module_call);
            }
            CallTarget::Method { object, method }
        }
        CallTarget::Computed { callee } => CallTarget::Computed {
            callee: Box::new(rewrite_expr(*callee)),
        },
        other => other,
    };
    IrExpr { kind: IrExprKind::Call { target, args, type_args }, ty, span, def_id: None }
}

fn rewrite_expr(expr: IrExpr) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, type_args } =>
            return rewrite_expr_call_module(module, func, args, type_args, ty, span),

        // Recurse into all sub-expressions (same as before)
        IrExprKind::Call { target, args, type_args } =>
            return rewrite_expr_call_other(target, args, type_args, ty, span),
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(rewrite_expr(*cond)),
            then: Box::new(rewrite_expr(*then)),
            else_: Box::new(rewrite_expr(*else_)),
        },
        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: rewrite_stmts(stmts),
            expr: expr.map(|e| Box::new(rewrite_expr(*e))),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(rewrite_expr(*subject)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(|g| rewrite_expr(g)),
                body: rewrite_expr(arm.body),
            }).collect(),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op, left: Box::new(rewrite_expr(*left)), right: Box::new(rewrite_expr(*right)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(rewrite_expr(*operand)),
        },
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params, body: Box::new(rewrite_expr(*body)), lambda_id,
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| rewrite_expr(e)).collect(),
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(|e| rewrite_expr(e)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: fields.into_iter().map(|(k, v)| (k, rewrite_expr(v))).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(rewrite_expr(*base)),
            fields: fields.into_iter().map(|(k, v)| (k, rewrite_expr(v))).collect(),
        },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(rewrite_expr(*object)), field,
        },
        IrExprKind::OptionalChain { expr, field } => IrExprKind::OptionalChain {
            expr: Box::new(rewrite_expr(*expr)), field,
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(rewrite_expr(*iterable)),
            body: rewrite_stmts(body),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(rewrite_expr(*cond)),
            body: rewrite_stmts(body),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: rewrite_expr(expr) },
                other => other,
            }).collect(),
        },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::Unwrap { expr } => IrExprKind::Unwrap { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::ToOption { expr } => IrExprKind::ToOption { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::UnwrapOr { expr, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(rewrite_expr(*expr)),
            fallback: Box::new(rewrite_expr(*fallback)),
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (rewrite_expr(k), rewrite_expr(v))).collect(),
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(rewrite_expr(*start)),
            end: Box::new(rewrite_expr(*end)),
            inclusive,
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(rewrite_expr(*object)),
            index: Box::new(rewrite_expr(*index)),
        },
        IrExprKind::MapAccess { object, key } => IrExprKind::MapAccess {
            object: Box::new(rewrite_expr(*object)),
            key: Box::new(rewrite_expr(*key)),
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            // FanLoweringPass will strip auto-try from these later
            exprs: exprs.into_iter().map(|e| rewrite_expr(e)).collect(),
        },
        // Codegen wrapper nodes — must recurse into inner expressions
        IrExprKind::Clone { expr } => IrExprKind::Clone { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::Borrow { expr, as_str, mutable } => IrExprKind::Borrow { expr: Box::new(rewrite_expr(*expr)), as_str, mutable },
        IrExprKind::Deref { expr } => IrExprKind::Deref { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::BoxNew { expr } => IrExprKind::BoxNew { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::ToVec { expr } => IrExprKind::ToVec { expr: Box::new(rewrite_expr(*expr)) },
        IrExprKind::RustMacro { name, args } => IrExprKind::RustMacro {
            name, args: args.into_iter().map(|a| rewrite_expr(a)).collect(),
        },
        // Phase 1e-2: traverse RuntimeCall args so inner `@inline_rust`
        // calls (e.g. `almide_rt_int_to_string(list.len(xs))`) still get
        // their template substitution. Without this, Module calls nested
        // inside a RuntimeCall fall through `other => other` untouched
        // and emit as bare Module calls, losing the `&{xs}` borrow.
        IrExprKind::RuntimeCall { symbol, args } => IrExprKind::RuntimeCall {
            symbol,
            args: args.into_iter().map(|a| rewrite_expr(a)).collect(),
        },
        // Default: recurse every child via the exhaustive map_children chokepoint
        // so no un-listed node kind silently drops its subtree.
        other => {
            let e = IrExpr { kind: other, ty: ty.clone(), span, def_id: None };
            return e.map_children(&mut |c| rewrite_expr(c));
        }
    };

    IrExpr { kind, ty, span, def_id: None }
}

include!("pass_stdlib_lowering_p2.rs");
