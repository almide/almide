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
                span: None,
            })
        }
        ExprKind::Float { value } => Some(IrExpr {
            kind: IrExprKind::LitFloat { value: *value },
            ty: Ty::Float,
            span: None,
        }),
        ExprKind::String { value } => Some(IrExpr {
            kind: IrExprKind::LitStr { value: value.clone() },
            ty: Ty::String,
            span: None,
        }),
        ExprKind::Bool { value } => Some(IrExpr {
            kind: IrExprKind::LitBool { value: *value },
            ty: Ty::Bool,
            span: None,
        }),
        ExprKind::Ident { name } if name.as_str() == "none" => Some(IrExpr {
            kind: IrExprKind::OptionNone,
            ty: Ty::option(Ty::Unknown),
            span: None,
        }),
        _ => None,
    }
}

impl NanoPass for StdlibLoweringPass {
    fn name(&self) -> &str { "StdlibLowering" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    fn depends_on(&self) -> Vec<&'static str> { vec!["EffectInference"] }
    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        // Every fn in every bundled stdlib IR module is bundled-only
        // now: the TOML-backed runtime table was retired with the
        // Stdlib Declarative Unification arc, so no overlap check
        // is needed.
        let bundled_fns: HashSet<(Sym, Sym)> = program.modules.iter()
            .filter(|m| almide_lang::stdlib_info::is_bundled_module(m.name.as_str()))
            .flat_map(|m| {
                let mname = m.name;
                m.functions.iter().map(move |f| (mname, f.name))
            })
            .collect();
        BUNDLED_FNS.with(|s| *s.borrow_mut() = bundled_fns);

        // Build the @inline_rust dispatch table. Two sources feed it:
        //
        // 1. `program.modules` — the frontend-loaded bundled stdlib
        //    modules (the normal compile path routes through
        //    `resolve.rs` which parses + lowers each bundled `.almd`).
        //    These land as full `IrModule` entries; we read the
        //    `@inline_rust` attribute directly off `IrFunction.attrs`.
        //
        // 2. Bundled source fallback — code paths that bypass
        //    `resolve.rs` (unit tests using `canonicalize_program` +
        //    `lower_program` directly, e.g. the snapshot test suite)
        //    never get bundled modules into `program.modules`. For
        //    those, re-parse the embedded `stdlib/<m>.almd` source so
        //    every @inline_rust fn is reachable. Skipping this would
        //    make pass_stdlib_lowering silently fall back to the
        //    legacy `arg_transforms::lookup` path for migrated fns,
        //    which now returns `None` and emits invalid Rust.
        //
        // The merge policy is "IR wins" — if a bundled module is
        // fully loaded, we take its IR-level signature (param names,
        // attribute values). Source fallback only fills in modules
        // that IR didn't provide.
        let mut inline_rust: HashMap<(Sym, Sym), InlineRustSpec> = program.modules.iter()
            .filter(|m| almide_lang::stdlib_info::is_bundled_module(m.name.as_str()))
            .flat_map(|m| {
                let mname = m.name;
                m.functions.iter().filter_map(move |f| {
                    find_inline_rust_template(f).map(|template| {
                        let param_names = f.params.iter().map(|p| p.name).collect();
                        let defaults = f.params.iter()
                            .map(|p| p.default.as_ref().map(|d| (**d).clone()))
                            .collect();
                        ((mname, f.name), InlineRustSpec { template, param_names, defaults })
                    })
                })
            })
            .collect();
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
        // Resolve remaining bare UFCS calls in module bodies (checker doesn't fully type them)
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
        // Build versioned name mapping: original module name → versioned name
        // e.g., "json" → "json_v2" when versioned_name is set
        let version_map: std::collections::HashMap<String, String> = program.modules.iter()
            .filter_map(|m| m.versioned_name.map(|v| (m.name.to_string(), v.to_string())))
            .collect();
        // Rewrite CallTarget::Module names to versioned names in all function bodies
        if !version_map.is_empty() {
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
        // Prefix intra-module Named calls to match renamed definitions
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
        PassResult { program, changed: true }
    }
}

fn rewrite_expr(expr: IrExpr) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func }, args, type_args } => {
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
                    span,
                };
            }

            // Non-stdlib modules (bundled .almd + user packages): leave as Module calls.
            // They are rendered by the walker directly, not converted to Named calls.
            // Bundled-only stdlib fns (defined in stdlib/<module>.almd with no TOML
            // runtime template) take the same path — there is no almide_rt_*
            // function to call, so the walker renders them as a normal user-fn call.
            let is_stdlib = almide_lang::stdlib_info::is_stdlib_module(&module);
            if !is_stdlib || is_bundled_only(module, func) {
                let args: Vec<IrExpr> = args.into_iter().map(|a| rewrite_expr(a)).collect();
                return IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module { module, func },
                        args,
                        type_args,
                    },
                    ty,
                    span,
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
            return IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module { module, func },
                    args,
                    type_args,
                },
                ty,
                span,
            };
        }

        // Recurse into all sub-expressions (same as before)
        IrExprKind::Call { target, args, type_args } => {
            let args = args.into_iter().map(|a| rewrite_expr(a)).collect();
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
                                    target: CallTarget::Module { module: module.to_string().into(), func: method },
                                    args: call_args, type_args,
                                },
                                ty: ty.clone(), span,
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
                                }, ty, span };
                            }
                        let mut call_args = vec![*object];
                        call_args.extend(args);
                        let module_call = IrExpr {
                            kind: IrExprKind::Call {
                                target: CallTarget::Module { module: mod_name.into(), func: func_name.into() },
                                args: call_args, type_args,
                            },
                            ty: ty.clone(), span,
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
            IrExprKind::Call { target, args, type_args }
        }
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
        other => other,
    };

    IrExpr { kind, ty, span }
}

/// Resolve a stdlib module from the receiver/arg type and method name.
/// Only resolves when the type is known (not Unknown).
fn resolve_module_from_ty(ty: &Ty, method: &str) -> Option<&'static str> {
    let candidates = almide_lang::stdlib_info::resolve_ufcs_candidates(method);
    if candidates.is_empty() { return None; }
    let module = match ty {
        Ty::Applied(TypeConstructorId::List, _) => Some("list"),
        Ty::Applied(TypeConstructorId::Map, _) => Some("map"),
        Ty::Applied(TypeConstructorId::Set, _) => Some("set"),
        Ty::String => Some("string"),
        Ty::Int => Some("int"),
        Ty::Float => Some("float"),
        // Sized numeric types (Stage 3 of the sized-numeric-types arc).
        // Each hosts its own UFCS conversion / `.to_string()` module.
        Ty::Int8 => Some("int8"),
        Ty::Int16 => Some("int16"),
        Ty::Int32 => Some("int32"),
        Ty::UInt8 => Some("uint8"),
        Ty::UInt16 => Some("uint16"),
        Ty::UInt32 => Some("uint32"),
        Ty::UInt64 => Some("uint64"),
        Ty::Float32 => Some("float32"),
        Ty::Applied(TypeConstructorId::Option, _) => Some("option"),
        Ty::Applied(TypeConstructorId::Result, _) => Some("result"),
        _ => None,
    };
    if let Some(m) = module {
        if candidates.contains(&m) { return Some(m); }
    }
    None
}

fn rewrite_stmts(stmts: Vec<IrStmt>) -> Vec<IrStmt> {
    stmts.into_iter().map(|s| {
        let kind = match s.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
                var, mutability, ty, value: rewrite_expr(value),
            },
            IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: rewrite_expr(value) },
            IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: rewrite_expr(expr) },
            IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
                cond: rewrite_expr(cond), else_: rewrite_expr(else_),
            },
            IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
                pattern, value: rewrite_expr(value),
            },
            IrStmtKind::IndexAssign { target, index, value } => IrStmtKind::IndexAssign {
                target, index: rewrite_expr(index), value: rewrite_expr(value),
            },
            IrStmtKind::FieldAssign { target, field, value } => IrStmtKind::FieldAssign {
                target, field, value: rewrite_expr(value),
            },
            IrStmtKind::MapInsert { target, key, value } => IrStmtKind::MapInsert {
                target, key: rewrite_expr(key), value: rewrite_expr(value),
            },
            other => other,
        };
        IrStmt { kind, span: s.span }
    }).collect()
}

/// Resolve bare UFCS calls in module function bodies where the checker
/// couldn't fully resolve types. Only converts Named/Method calls that
/// match known stdlib functions and DON'T match sibling module functions.
fn resolve_unresolved_ufcs(expr: IrExpr, siblings: &[String]) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    // Special cases: Named calls and Method calls that resolve to stdlib
    match &expr.kind {
        // Named call: sort(xs) → list.sort(xs) when "sort" is a stdlib function
        // and NOT a sibling module function
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
            if !args.is_empty()
            && !siblings.iter().any(|s| s == &**name)
            && !almide_lang::stdlib_info::resolve_ufcs_candidates(name).is_empty() =>
        {
            let IrExprKind::Call { target: CallTarget::Named { name }, args, type_args } = expr.kind else { unreachable!() };
            let args: Vec<IrExpr> = args.into_iter().map(|a| resolve_unresolved_ufcs(a, siblings)).collect();
            let module = resolve_module_from_ty(&args[0].ty, &name)
                .or_else(|| almide_lang::stdlib_info::resolve_ufcs_module(&name));
            if let Some(module) = module {
                return rewrite_expr(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module { module: module.to_string().into(), func: name },
                        args, type_args,
                    },
                    ty, span,
                });
            }
            return IrExpr {
                kind: IrExprKind::Call { target: CallTarget::Named { name }, args, type_args },
                ty, span,
            };
        }
        // Method call: xs.map(fn) → list.map(xs, fn) when type is known
        IrExprKind::Call { target: CallTarget::Method { method, .. }, .. }
            if !method.contains('.')
            && !almide_lang::stdlib_info::resolve_ufcs_candidates(method).is_empty() =>
        {
            let IrExprKind::Call { target: CallTarget::Method { object, method }, args, type_args } = expr.kind else { unreachable!() };
            let object = Box::new(resolve_unresolved_ufcs(*object, siblings));
            let args: Vec<IrExpr> = args.into_iter().map(|a| resolve_unresolved_ufcs(a, siblings)).collect();
            let module = resolve_module_from_ty(&object.ty, &method)
                .or_else(|| almide_lang::stdlib_info::resolve_ufcs_module(&method));
            if let Some(module) = module {
                let mut call_args = vec![*object];
                call_args.extend(args);
                return rewrite_expr(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module { module: module.to_string().into(), func: method },
                        args: call_args, type_args,
                    },
                    ty, span,
                });
            }
            return IrExpr {
                kind: IrExprKind::Call { target: CallTarget::Method { object, method }, args, type_args },
                ty, span,
            };
        }
        _ => {}
    }
    // Default: recurse into all children
    expr.map_children(&mut |e| resolve_unresolved_ufcs(e, siblings))
}

// Kept for backward compatibility — resolve_ufcs_stmts callers in the pass
fn resolve_ufcs_stmts(stmts: Vec<IrStmt>, siblings: &[String]) -> Vec<IrStmt> {
    stmts.into_iter().map(|s| s.map_exprs(&mut |e| resolve_unresolved_ufcs(e, siblings))).collect()
}

// ── Iterator chain lowering ────────────────────────────────────────

/// Inline math/float/int intrinsics as native Rust expressions.
/// Eliminates runtime function call overhead for hot-path numeric operations.
fn try_inline_intrinsic(module: &str, func: &str, args: &[IrExpr], ty: &Ty, span: Option<almide_base::Span>) -> Option<IrExpr> {
    let mk = |kind: IrExprKind| IrExpr { kind, ty: ty.clone(), span };

    // NOTE: `float` entries that used to live here (sqrt/abs/floor/
    // ceil/round/is_nan/is_infinite) have been deleted. The bundled
    // `stdlib/float.almd` now owns those dispatches via `@inline_rust`
    // templates that emit the same Method-call form — the intercept
    // fires earlier in `rewrite_expr`, so this code would be dead even
    // if left in place. Kept `math.*` entries because the `math`
    // module has not been migrated to bundled yet.
    match (module, func) {
        // ── math.sqrt(x) → x.sqrt() via RenderedCall ──
        // These are the highest-impact: called in tight loops (nbody, spectralnorm)
        ("math", "sqrt") if args.len() >= 1 => {
            Some(mk(IrExprKind::Call {
                target: CallTarget::Method {
                    object: Box::new(args[0].clone()),
                    method: almide_base::intern::sym("sqrt"),
                },
                args: vec![],
                type_args: vec![],
            }))
        }
        ("math", "abs") if args.len() >= 1 => {
            Some(mk(IrExprKind::Call {
                target: CallTarget::Method {
                    object: Box::new(args[0].clone()),
                    method: almide_base::intern::sym("abs"),
                },
                args: vec![],
                type_args: vec![],
            }))
        }
        ("math", "floor") if args.len() >= 1 => {
            Some(mk(IrExprKind::Call {
                target: CallTarget::Method {
                    object: Box::new(args[0].clone()),
                    method: almide_base::intern::sym("floor"),
                },
                args: vec![],
                type_args: vec![],
            }))
        }
        ("math", "ceil") if args.len() >= 1 => {
            Some(mk(IrExprKind::Call {
                target: CallTarget::Method {
                    object: Box::new(args[0].clone()),
                    method: almide_base::intern::sym("ceil"),
                },
                args: vec![],
                type_args: vec![],
            }))
        }
        ("math", "round") if args.len() >= 1 => {
            Some(mk(IrExprKind::Call {
                target: CallTarget::Method {
                    object: Box::new(args[0].clone()),
                    method: almide_base::intern::sym("round"),
                },
                args: vec![],
                type_args: vec![],
            }))
        }
        ("math", "sin") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("sin") },
            args: vec![], type_args: vec![],
        })),
        ("math", "cos") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("cos") },
            args: vec![], type_args: vec![],
        })),
        ("math", "tan") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("tan") },
            args: vec![], type_args: vec![],
        })),
        ("math", "asin") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("asin") },
            args: vec![], type_args: vec![],
        })),
        ("math", "acos") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("acos") },
            args: vec![], type_args: vec![],
        })),
        ("math", "atan") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("atan") },
            args: vec![], type_args: vec![],
        })),
        ("math", "atan2") if args.len() >= 2 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("atan2") },
            args: vec![args[1].clone()], type_args: vec![],
        })),
        ("math", "exp") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("exp") },
            args: vec![], type_args: vec![],
        })),
        ("math", "log") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("ln") },
            args: vec![], type_args: vec![],
        })),
        ("math", "log2") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("log2") },
            args: vec![], type_args: vec![],
        })),
        ("math", "log10") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("log10") },
            args: vec![], type_args: vec![],
        })),
        // float.from_int / int.to_float / float.to_int: walker handles inline cast
        // math.pow: Int exponentiation — keep as runtime call (i64.pow needs u32 cast)
        // ── math.fpow(base, exp) → base.powf(exp) ──
        ("math", "fpow") if args.len() >= 2 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("powf") },
            args: vec![args[1].clone()], type_args: vec![],
        })),
        // ── Constants ──
        ("math", "pi") => Some(mk(IrExprKind::LitFloat { value: std::f64::consts::PI })),
        ("math", "e") => Some(mk(IrExprKind::LitFloat { value: std::f64::consts::E })),
        ("math", "inf") => Some(mk(IrExprKind::LitFloat { value: f64::INFINITY })),
        // `float.is_nan` / `float.is_infinite` deleted — owned by
        // `stdlib/float.almd` via `@inline_rust`.
        ("math", "is_nan") if args.len() >= 1 => Some(mk(IrExprKind::Call {
            target: CallTarget::Method { object: Box::new(args[0].clone()), method: almide_base::intern::sym("is_nan") },
            args: vec![], type_args: vec![],
        })),
        _ => None,
    }
}

/// Try to lower a list.* call into an IterChain IR node.
/// Returns None if the operation isn't iterator-eligible.
fn try_lower_to_iter_chain(func: &str, mut args: Vec<IrExpr>, ty: &Ty, span: Option<almide_base::Span>) -> Option<IrExpr> {
    match func {
        // ── Consuming operations (into_iter) → produce Vec ──
        "map" if args.len() >= 2 => {
            let lambda = prepare_lambda(args.remove(1));
            let source = args.remove(0);
            Some(IrExpr {
                kind: IrExprKind::IterChain {
                    source: Box::new(source),
                    consume: true,
                    steps: vec![IterStep::Map { lambda: Box::new(lambda) }],
                    collector: IterCollector::Collect,
                },
                ty: ty.clone(), span,
            })
        }
        "filter" if args.len() >= 2 && matches!(args[1].kind, IrExprKind::Lambda { .. }) => {
            let lambda = prepare_lambda_borrowed(args.remove(1));
            let source = args.remove(0);
            Some(IrExpr {
                kind: IrExprKind::IterChain {
                    source: Box::new(source),
                    consume: true,
                    steps: vec![IterStep::Filter { lambda: Box::new(lambda) }],
                    collector: IterCollector::Collect,
                },
                ty: ty.clone(), span,
            })
        }
        "flat_map" if args.len() >= 2 => {
            let lambda = prepare_lambda(args.remove(1));
            let source = args.remove(0);
            Some(IrExpr {
                kind: IrExprKind::IterChain {
                    source: Box::new(source),
                    consume: true,
                    steps: vec![IterStep::FlatMap { lambda: Box::new(lambda) }],
                    collector: IterCollector::Collect,
                },
                ty: ty.clone(), span,
            })
        }
        "filter_map" if args.len() >= 2 => {
            let lambda = prepare_lambda(args.remove(1));
            let source = args.remove(0);
            Some(IrExpr {
                kind: IrExprKind::IterChain {
                    source: Box::new(source),
                    consume: true,
                    steps: vec![IterStep::FilterMap { lambda: Box::new(lambda) }],
                    collector: IterCollector::Collect,
                },
                ty: ty.clone(), span,
            })
        }
        "fold" if args.len() >= 3 => {
            let lambda = prepare_lambda(args.remove(2));
            let init = args.remove(1);
            let source = args.remove(0);
            Some(IrExpr {
                kind: IrExprKind::IterChain {
                    source: Box::new(source),
                    consume: true,
                    steps: vec![],
                    collector: IterCollector::Fold { init: Box::new(init), lambda: Box::new(lambda) },
                },
                ty: ty.clone(), span,
            })
        }
        "find" if args.len() >= 2 && matches!(args[1].kind, IrExprKind::Lambda { .. }) => {
            let lambda = prepare_lambda_borrowed(args.remove(1));
            let source = args.remove(0);
            Some(IrExpr {
                kind: IrExprKind::IterChain {
                    source: Box::new(source),
                    consume: true,
                    steps: vec![],
                    collector: IterCollector::Find { lambda: Box::new(lambda) },
                },
                ty: ty.clone(), span,
            })
        }
        // ── Borrowing operations (iter) → produce scalar ──
        "any" if args.len() >= 2 => {
            let lambda = prepare_lambda(args.remove(1));
            let source = args.remove(0);
            Some(IrExpr {
                kind: IrExprKind::IterChain {
                    source: Box::new(source),
                    consume: true,
                    steps: vec![],
                    collector: IterCollector::Any { lambda: Box::new(lambda) },
                },
                ty: ty.clone(), span,
            })
        }
        "all" if args.len() >= 2 => {
            let lambda = prepare_lambda(args.remove(1));
            let source = args.remove(0);
            Some(IrExpr {
                kind: IrExprKind::IterChain {
                    source: Box::new(source),
                    consume: true,
                    steps: vec![],
                    collector: IterCollector::All { lambda: Box::new(lambda) },
                },
                ty: ty.clone(), span,
            })
        }
        "count" if args.len() >= 2 && matches!(args[1].kind, IrExprKind::Lambda { .. }) => {
            let lambda = prepare_lambda_borrowed(args.remove(1));
            let source = args.remove(0);
            Some(IrExpr {
                kind: IrExprKind::IterChain {
                    source: Box::new(source),
                    consume: true,
                    steps: vec![],
                    collector: IterCollector::Count { lambda: Box::new(lambda) },
                },
                ty: ty.clone(), span,
            })
        }
        _ => None,
    }
}

/// Prepare a lambda for consuming iterator ops (map, fold, flat_map, filter_map).
/// Callback gets `T` (owned) — apply LambdaClone with smart single-use skip.
fn prepare_lambda(arg: IrExpr) -> IrExpr {
    let ty = arg.ty.clone();
    let span = arg.span;
    match arg.kind {
        IrExprKind::Lambda { params, body, lambda_id } => {
            let clone_stmts = build_clone_stmts_for_lambda(&params, &body);
            let wrapped_body = if clone_stmts.is_empty() {
                *body
            } else {
                let body_ty = body.ty.clone();
                let body_span = body.span;
                IrExpr {
                    kind: IrExprKind::Block { stmts: clone_stmts, expr: Some(body) },
                    ty: body_ty, span: body_span,
                }
            };
            IrExpr {
                kind: IrExprKind::Lambda { params, body: Box::new(wrapped_body), lambda_id },
                ty, span,
            }
        }
        _ => arg,
    }
}

/// Prepare a lambda for borrowing iterator ops (filter, find, any, all, count).
/// Callback gets `&T` — need deref/clone bindings to convert to owned `T`.
fn prepare_lambda_borrowed(arg: IrExpr) -> IrExpr {
    let ty = arg.ty.clone();
    let span = arg.span;
    match arg.kind {
        IrExprKind::Lambda { params, body, lambda_id } => {
            // For &T params, always add binding: Copy types get `let x = *x;`, heap types get `let x = x.clone();`
            let deref_stmts: Vec<IrStmt> = params.iter()
                .map(|(id, param_ty)| {
                    let is_copy = matches!(param_ty, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit);
                    let value = if is_copy {
                        // *x (deref the reference)
                        IrExpr {
                            kind: IrExprKind::Deref {
                                expr: Box::new(IrExpr { kind: IrExprKind::Var { id: *id }, ty: param_ty.clone(), span: None }),
                            },
                            ty: param_ty.clone(), span: None,
                        }
                    } else {
                        // x.clone() (clone from reference)
                        IrExpr {
                            kind: IrExprKind::Clone {
                                expr: Box::new(IrExpr { kind: IrExprKind::Var { id: *id }, ty: param_ty.clone(), span: None }),
                            },
                            ty: param_ty.clone(), span: None,
                        }
                    };
                    IrStmt {
                        kind: IrStmtKind::Bind { var: *id, mutability: Mutability::Let, ty: param_ty.clone(), value },
                        span: None,
                    }
                }).collect();

            let wrapped_body = if deref_stmts.is_empty() {
                *body
            } else {
                let body_ty = body.ty.clone();
                let body_span = body.span;
                IrExpr {
                    kind: IrExprKind::Block { stmts: deref_stmts, expr: Some(body) },
                    ty: body_ty, span: body_span,
                }
            };
            IrExpr {
                kind: IrExprKind::Lambda { params, body: Box::new(wrapped_body), lambda_id },
                ty, span,
            }
        }
        _ => arg,
    }
}

/// Rewrite intra-module `CallTarget::Named` calls that match a sibling function
/// to use the `almide_rt_{module}_{func}` prefix (matching the walker's definition rename).
fn prefix_intra_module_calls(expr: IrExpr, mod_name: &str, siblings: &[String]) -> IrExpr {
    // Special cases: Named calls and FnRef to sibling functions get prefixed
    match &expr.kind {
        IrExprKind::Call { target: CallTarget::Named { name }, .. }
            if siblings.iter().any(|s| s == &**name) =>
        {
            let IrExprKind::Call { target: CallTarget::Named { name }, args, type_args } = expr.kind else { unreachable!() };
            let sanitized = name.replace(' ', "_").replace('-', "_").replace('.', "_");
            let mod_ident = mod_name.replace('.', "_");
            let prefixed = format!("almide_rt_{}_{}", mod_ident, sanitized);
            let args = args.into_iter().map(|a| prefix_intra_module_calls(a, mod_name, siblings)).collect();
            return IrExpr {
                kind: IrExprKind::Call { target: CallTarget::Named { name: prefixed.into() }, args, type_args },
                ty: expr.ty, span: expr.span,
            };
        }
        IrExprKind::FnRef { name } if siblings.iter().any(|s| s == &**name) => {
            let sanitized = name.replace(' ', "_").replace('-', "_").replace('.', "_");
            let mod_ident = mod_name.replace('.', "_");
            return IrExpr {
                kind: IrExprKind::FnRef { name: format!("almide_rt_{}_{}", mod_ident, sanitized).into() },
                ty: expr.ty, span: expr.span,
            };
        }
        _ => {}
    }
    // Default: recurse into all children
    expr.map_children(&mut |e| prefix_intra_module_calls(e, mod_name, siblings))
}

/// Rewrite CallTarget::Module names using versioned name mapping.
/// e.g., CallTarget::Module { module: "json" } → CallTarget::Module { module: "json_v2" }
fn rewrite_module_names(expr: IrExpr, map: &std::collections::HashMap<String, String>) -> IrExpr {
    use almide_base::intern::sym;
    // Only CallTarget::Module needs special handling; everything else just recurses.
    if let IrExprKind::Call { target: CallTarget::Module { module, .. }, .. } = &expr.kind {
        if map.contains_key(&**module) {
            let IrExprKind::Call { target: CallTarget::Module { module, func }, args, type_args } = expr.kind else { unreachable!() };
            let new_module = map.get(&*module).map(|v| sym(v)).unwrap_or(module);
            let args = args.into_iter().map(|a| rewrite_module_names(a, map)).collect();
            return IrExpr {
                kind: IrExprKind::Call { target: CallTarget::Module { module: new_module, func }, args, type_args },
                ty: expr.ty, span: expr.span,
            };
        }
    }
    expr.map_children(&mut |e| rewrite_module_names(e, map))
}

// ── Lambda clone optimization: only clone multi-use params ─────────

/// Types that need explicit annotation in lambda rebinding to help Rust type inference.
fn needs_type_annotation(ty: &Ty) -> bool {
    matches!(ty, Ty::Applied(_, _) | Ty::Named(_, _) | Ty::Record { .. } | Ty::OpenRecord { .. }
        | Ty::Variant { .. } | Ty::TypeVar(_))
}

/// Build clone stmts for lambda params, skipping single-use params (they can move).
fn build_clone_stmts_for_lambda(params: &[(VarId, Ty)], body: &IrExpr) -> Vec<IrStmt> {
    let non_copy: HashSet<VarId> = params.iter()
        .filter(|(_, t)| !matches!(t, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit))
        .map(|(id, _)| *id)
        .collect();
    if non_copy.is_empty() { return Vec::new(); }

    let uses = count_lambda_body_uses(body, &non_copy);

    params.iter()
        .filter(|(_, t)| !matches!(t, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit))
        .filter_map(|(id, param_ty)| {
            let count = uses.get(id).copied().unwrap_or(0);
            if count > 1 {
                // Multi-use: clone binding (let x: T = x.clone())
                Some(IrStmt {
                    kind: IrStmtKind::Bind {
                        var: *id,
                        mutability: Mutability::Let,
                        ty: param_ty.clone(),
                        value: IrExpr {
                            kind: IrExprKind::Clone {
                                expr: Box::new(IrExpr {
                                    kind: IrExprKind::Var { id: *id },
                                    ty: param_ty.clone(),
                                    span: None,
                                }),
                            },
                            ty: param_ty.clone(),
                            span: None,
                        },
                    },
                    span: None,
                })
            } else if count == 1 && needs_type_annotation(param_ty) {
                // Single-use but complex type: rebind for type annotation (let x: T = x)
                Some(IrStmt {
                    kind: IrStmtKind::Bind {
                        var: *id,
                        mutability: Mutability::Let,
                        ty: param_ty.clone(),
                        value: IrExpr {
                            kind: IrExprKind::Var { id: *id },
                            ty: param_ty.clone(),
                            span: None,
                        },
                    },
                    span: None,
                })
            } else {
                None
            }
        }).collect()
}

/// Count uses of target VarIds within a lambda body.
/// Uses inside loops or nested lambdas are counted as 2 (conservative: forces clone).
fn count_lambda_body_uses(expr: &IrExpr, targets: &HashSet<VarId>) -> HashMap<VarId, u32> {
    let mut counts = HashMap::new();
    count_lbu_expr(expr, targets, &mut counts, false);
    counts
}

fn count_lbu_expr(expr: &IrExpr, targets: &HashSet<VarId>, counts: &mut HashMap<VarId, u32>, in_multi: bool) {
    match &expr.kind {
        IrExprKind::Var { id } if targets.contains(id) => {
            *counts.entry(*id).or_insert(0) += if in_multi { 2 } else { 1 };
        }
        IrExprKind::Block { stmts, expr } => {
            for s in stmts { count_lbu_stmt(s, targets, counts, in_multi); }
            if let Some(e) = expr { count_lbu_expr(e, targets, counts, in_multi); }
        }
        IrExprKind::If { cond, then, else_ } => {
            count_lbu_expr(cond, targets, counts, in_multi);
            count_lbu_expr(then, targets, counts, in_multi);
            count_lbu_expr(else_, targets, counts, in_multi);
        }
        IrExprKind::Match { subject, arms } => {
            count_lbu_expr(subject, targets, counts, in_multi);
            for arm in arms {
                if let Some(g) = &arm.guard { count_lbu_expr(g, targets, counts, in_multi); }
                count_lbu_expr(&arm.body, targets, counts, in_multi);
            }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            count_lbu_expr(iterable, targets, counts, in_multi);
            for s in body { count_lbu_stmt(s, targets, counts, true); }
        }
        IrExprKind::While { cond, body } => {
            count_lbu_expr(cond, targets, counts, true);
            for s in body { count_lbu_stmt(s, targets, counts, true); }
        }
        IrExprKind::Lambda { body, .. } => {
            count_lbu_expr(body, targets, counts, true);
        }
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Method { object, .. } => count_lbu_expr(object, targets, counts, in_multi),
                CallTarget::Computed { callee } => count_lbu_expr(callee, targets, counts, in_multi),
                _ => {}
            }
            for a in args { count_lbu_expr(a, targets, counts, in_multi); }
        }
        IrExprKind::BinOp { left, right, .. } => {
            count_lbu_expr(left, targets, counts, in_multi);
            count_lbu_expr(right, targets, counts, in_multi);
        }
        IrExprKind::UnOp { operand, .. } => count_lbu_expr(operand, targets, counts, in_multi),
        IrExprKind::List { elements } | IrExprKind::Tuple { elements }
        | IrExprKind::Fan { exprs: elements } => {
            for e in elements { count_lbu_expr(e, targets, counts, in_multi); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, e) in fields { count_lbu_expr(e, targets, counts, in_multi); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            count_lbu_expr(base, targets, counts, in_multi);
            for (_, e) in fields { count_lbu_expr(e, targets, counts, in_multi); }
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => count_lbu_expr(object, targets, counts, in_multi),
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            count_lbu_expr(object, targets, counts, in_multi);
            count_lbu_expr(index, targets, counts, in_multi);
        }
        IrExprKind::Range { start, end, .. } => {
            count_lbu_expr(start, targets, counts, in_multi);
            count_lbu_expr(end, targets, counts, in_multi);
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts { if let IrStringPart::Expr { expr } = p { count_lbu_expr(expr, targets, counts, in_multi); } }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries { count_lbu_expr(k, targets, counts, in_multi); count_lbu_expr(v, targets, counts, in_multi); }
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Unwrap { expr } | IrExprKind::ToOption { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } | IrExprKind::Await { expr } => {
            count_lbu_expr(expr, targets, counts, in_multi);
        }
        IrExprKind::UnwrapOr { expr, fallback } => {
            count_lbu_expr(expr, targets, counts, in_multi);
            count_lbu_expr(fallback, targets, counts, in_multi);
        }
        IrExprKind::RustMacro { args, .. } => {
            for a in args { count_lbu_expr(a, targets, counts, in_multi); }
        }
        _ => {}
    }
}

fn count_lbu_stmt(stmt: &IrStmt, targets: &HashSet<VarId>, counts: &mut HashMap<VarId, u32>, in_multi: bool) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            count_lbu_expr(value, targets, counts, in_multi);
        }
        IrStmtKind::IndexAssign { index, value, .. } | IrStmtKind::MapInsert { key: index, value, .. } => {
            count_lbu_expr(index, targets, counts, in_multi);
            count_lbu_expr(value, targets, counts, in_multi);
        }
        IrStmtKind::Expr { expr } => count_lbu_expr(expr, targets, counts, in_multi),
        IrStmtKind::Guard { cond, else_ } => {
            count_lbu_expr(cond, targets, counts, in_multi);
            count_lbu_expr(else_, targets, counts, in_multi);
        }
        IrStmtKind::ListSwap { a, b, .. } => {
            count_lbu_expr(a, targets, counts, in_multi);
            count_lbu_expr(b, targets, counts, in_multi);
        }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
            count_lbu_expr(end, targets, counts, in_multi);
        }
        IrStmtKind::ListCopySlice { len, .. } => count_lbu_expr(len, targets, counts, in_multi),
        IrStmtKind::Comment { .. } => {}
    }
}
