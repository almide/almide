//! IR Walker: traverses typed IR and renders using templates.
//!
//! This is the shared engine for all targets. It walks each IrExprKind,
//! recursively renders sub-expressions, and uses TemplateSet for output.
//!
//! The walker does NOT make semantic decisions — those are handled by
//! Nanopass passes (Layer 2) which annotate the IR before the walker runs.

mod declarations;
mod expressions;
pub mod helpers;
mod statements;
mod types;

// Re-exports for external consumers
pub use helpers::ty_contains_name;
pub use types::render_type;
pub use expressions::render_expr;
pub use statements::{render_stmt, render_pattern};

use almide_ir::*;
use almide_lang::types::Ty;
use super::annotations::CodegenAnnotations;
use super::pass::Target;
use super::template::TemplateSet;

use types::render_type as render_type_fn;
use expressions::render_expr as render_expr_fn;
use statements::render_stmt as render_stmt_fn;
use helpers::{terminate_stmt, indent_lines};
use declarations::{render_type_decl, collect_named_records, collect_anon_records, collect_record_field_counts};

/// Render context: carries the variable table, target, and annotations.
/// The walker NEVER checks types — all codegen decisions come from annotations.
pub struct RenderContext<'a> {
    pub templates: &'a TemplateSet,
    pub var_table: &'a VarTable,
    pub indent: usize,
    pub target: Target,
    pub auto_unwrap: bool,
    pub is_test: bool,
    pub ann: CodegenAnnotations,
    pub type_aliases: std::collections::HashMap<almide_base::intern::Sym, almide_lang::types::Ty>,
    /// Type names that have generic parameters (for erasing to `_` when args are missing)
    pub generic_types: std::collections::HashSet<almide_base::intern::Sym>,
    /// Use minimal generic bounds (Clone only) for bundled .almd module functions.
    pub minimal_generic_bounds: bool,
    /// Emit `#[repr(C)]` on structs/enums for stable C ABI layout.
    pub repr_c: bool,
    /// VarIds of current fn params emitted as Rust references (`&T`,
    /// `&[T]`, `&str`). Used by the Borrow walker to avoid
    /// double-borrowing an already-reference binding.
    pub ref_params: std::collections::HashSet<VarId>,
    /// VarIds of current fn params emitted as `&mut T`. Used by the
    /// Borrow walker to skip the outer `&mut` wrap when forwarding a
    /// `RefMut` param into another `RefMut` callee slot (Rust
    /// auto-reborrows).
    pub ref_mut_params: std::collections::HashSet<VarId>,
    /// Names of user-defined record/variant types that have a generated
    /// `AlmideRepr` impl (see `render_repr_impl`). A `Ty::Named` in a compound
    /// interpolation part routes through `almide_repr` only when it is in this
    /// set, so a value with the impl renders to its literal form while opaque
    /// `Named` references (e.g. runtime newtypes) stay on the Display path.
    pub repr_named_types: std::collections::HashSet<almide_base::intern::Sym>,
    /// Error type `E` of the enclosing fn's declared return `Result[_, E]`,
    /// or `None` if the fn does not return a `Result`. The `!` (Unwrap)
    /// renderer compares a propagated source error against this: when they
    /// match, `?` propagates directly with no `map_err` coercion (so a custom
    /// variant error type is preserved rather than stringified via Debug).
    pub fn_err_ty: Option<almide_lang::types::Ty>,
}

impl<'a> RenderContext<'a> {
    pub fn new(templates: &'a TemplateSet, var_table: &'a VarTable) -> Self {
        Self { templates, var_table, indent: 0, target: Target::Rust, auto_unwrap: false, is_test: false, ann: CodegenAnnotations::default(), type_aliases: std::collections::HashMap::new(), generic_types: std::collections::HashSet::new(), minimal_generic_bounds: false, repr_c: false, ref_params: std::collections::HashSet::new(), ref_mut_params: std::collections::HashSet::new(), repr_named_types: std::collections::HashSet::new(), fn_err_ty: None }
    }

    pub fn with_target(mut self, target: Target) -> Self {
        self.target = target;
        self
    }

    pub fn with_annotations(mut self, ann: CodegenAnnotations) -> Self {
        self.ann = ann;
        self
    }



    pub(crate) fn var_name(&self, id: VarId) -> String {
        let name = &self.var_table.get(id).name;
        let kw = [
            // Rust keywords
            "as", "async", "await", "break", "const", "continue", "crate",
            "dyn", "else", "enum", "extern", "false", "fn", "for", "if",
            "impl", "in", "let", "loop", "match", "mod", "move", "mut",
            "pub", "ref", "return", "self", "Self", "static", "struct",
            "super", "trait", "true", "type", "unsafe", "use", "where", "while",
            "abstract", "become", "box", "do", "final", "macro", "override",
            "priv", "try", "typeof", "unsized", "virtual", "yield",
        ];
        if kw.contains(&name.as_str()) {
            self.templates.render_with("keyword_escape", None, &[], &[("name", name.as_str())])
                .unwrap_or_else(|| name.to_string())
        } else {
            name.to_string()
        }
    }

    /// The thread_local static name for a global var, matching exactly what the
    /// `render_program` top_let emission declares. Built from the RAW (un-escaped)
    /// var name: `var_name(id)` raw-escapes Rust keywords (`box` → `r#box`), and
    /// uppercasing that yields the invalid `R#BOX`, which mismatches the `BOX` the
    /// declaration emits. Every read/write of a global must route through here.
    pub(crate) fn global_static_name(&self, id: VarId) -> String {
        let vi = self.var_table.get(id);
        match &vi.module_origin {
            Some(origin) => format!("ALMIDE_RT_{}_{}", origin.to_uppercase(), vi.name.to_uppercase()),
            None => vi.name.to_uppercase(),
        }
    }
}

// ── Function rendering ──

pub fn render_function(ctx: &RenderContext, func: &IrFunction) -> String {
    // Collect VarIds of fn params that will be emitted as references
    // (`&T` / `&[T]` / `&str`). The Borrow walker uses this to skip
    // outer `&` wrap on already-borrowed bindings.
    let mut ref_params: std::collections::HashSet<VarId> =
        std::collections::HashSet::new();
    let mut ref_mut_params: std::collections::HashSet<VarId> =
        std::collections::HashSet::new();
    for p in &func.params {
        use almide_ir::ParamBorrow;
        match p.borrow {
            ParamBorrow::Ref | ParamBorrow::RefSlice | ParamBorrow::RefStr => {
                ref_params.insert(p.var);
            }
            ParamBorrow::RefMut => {
                ref_mut_params.insert(p.var);
            }
            _ => {}
        }
    }

    // Error type that the enclosing fn's `?`/auto-? propagates into. A fn
    // declared `Result[_, E]` propagates into `E`; an effect fn declared with
    // a non-Result type is auto-wrapped to `Result<_, String>`, so it
    // propagates into `String`. A plain fn with no Result return propagates
    // into nothing (None). The Unwrap renderer uses this to skip the Debug
    // `map_err` coercion when the source error type already matches.
    let fn_err_ty = if let Some((_, err_ty)) = func.ret_ty.inner2() {
        Some(err_ty.clone())
    } else if func.is_effect && !func.is_test {
        Some(almide_lang::types::Ty::String)
    } else {
        None
    };

    // Set effect fn context for auto-? insertion
    let fn_ctx = RenderContext {
        templates: ctx.templates,
        var_table: ctx.var_table,
        indent: ctx.indent,
        target: ctx.target,
        auto_unwrap: func.is_effect && !func.is_test,
        is_test: func.is_test,
        ann: ctx.ann.clone(),
        type_aliases: ctx.type_aliases.clone(),
        generic_types: ctx.generic_types.clone(),
        minimal_generic_bounds: ctx.minimal_generic_bounds,
        repr_c: ctx.repr_c,
        ref_params,
        ref_mut_params,
        repr_named_types: ctx.repr_named_types.clone(),
        fn_err_ty,
    };

    // Dispatch-only fns (body is Hole): `@inline_rust` / `@intrinsic`
    // templates are inlined at call sites. No Rust fn emitted.
    // Package fns with `@inline_rust` + real fallback body: emit the body
    // so same-module calls (tests, internal) have a callable function.
    let has_codegen_attr = func.attrs.iter().any(|a|
        matches!(a.name.as_str(), "inline_rust" | "intrinsic"));
    let body_is_dispatch_only = matches!(func.body.kind, IrExprKind::Hole | IrExprKind::Todo { .. })
        || matches!(&func.body.kind, IrExprKind::ResultOk { expr } if matches!(expr.kind, IrExprKind::Hole | IrExprKind::Todo { .. }));
    if matches!(ctx.target, Target::Rust) && has_codegen_attr && body_is_dispatch_only {
        return String::new();
    }

    // Extern fn dispatch:
    //   @extern(rust, "mod", "fn") → native module call (render_native_call)
    //   @extern(wasm, "env", "fn") → WASM host import (future)
    //   @extern(rs, "mod", "fn")   → template-based rendering (legacy)
    //   @extern(c, "lib", "fn")    → C FFI with extern "C" block
    if !func.extern_attrs.is_empty() {
        let target_str = match ctx.target {
            Target::Rust => "rs",
            _ => "",
        };
        let native_target = match ctx.target {
            Target::Rust => "rust",
            _ => "wasm",
        };
        for attr in &func.extern_attrs {
            // @extern(rust, ...) / @extern(wasm, ...) — native module binding
            if attr.target == native_target {
                return render_native_call(ctx, func, attr);
            }
            if attr.target == target_str {
                return ctx.templates.render_with("extern_fn", None, &[], &[("module", attr.module.as_str()), ("function", attr.function.as_str()), ("name", func.name.as_str())])
                    .unwrap_or_else(|| format!("// extern: {}.{}", attr.module, attr.function));
            }
            // @extern(c, "lib", "func") — generate extern "C" block + safe wrapper
            if attr.target == "c" && matches!(ctx.target, Target::Rust) {
                return render_extern_c(ctx, func, attr);
            }
        }
    }

    // Export fn: render body normally, then wrap with #[no_mangle] pub extern "C"
    if !func.export_attrs.is_empty() {
        for attr in &func.export_attrs {
            if attr.target == "c" && matches!(ctx.target, Target::Rust) {
                return render_export_c(ctx, func, attr);
            }
        }
    }

    let params_str = func.params.iter()
        .map(|p| {
            let mut param_name = p.name.to_string();
            // Escape target-specific keywords in param names
            let kw_list = ["default", "switch", "case", "class", "new", "delete",
                "typeof", "void", "with", "yield", "export", "import",
                "try", "catch", "finally", "throw"];
            if kw_list.contains(&param_name.as_str()) {
                param_name = fn_ctx.templates.render_with("keyword_escape", None, &[], &[("name", param_name.as_str())])
                    .unwrap_or(param_name);
            }
            // Mutable params (e.g. from TCO pass) — let the template decide
            // whether to emit a `mut` prefix via the {mut_prefix} variable.
            let mut_prefix = if fn_ctx.var_table.get(p.var).mutability == Mutability::Var {
                fn_ctx.templates.render_with("mut_param_prefix", None, &[], &[])
                    .unwrap_or_default()
            } else {
                String::new()
            };
            if !mut_prefix.is_empty() {
                param_name = format!("{}{}", mut_prefix, param_name);
            }
            let type_s = match p.borrow {
                ParamBorrow::Own => render_type_fn(&fn_ctx, &p.ty),
                ParamBorrow::Ref => format!("&{}", render_type_fn(&fn_ctx, &p.ty)),
                ParamBorrow::RefMut => format!("&mut {}", render_type_fn(&fn_ctx, &p.ty)),
                ParamBorrow::RefStr => "&str".to_string(),
                ParamBorrow::RefSlice => {
                    if let almide_lang::types::Ty::Applied(_, args) = &p.ty {
                        if let Some(inner) = args.first() {
                            format!("&[{}]", render_type_fn(&fn_ctx, inner))
                        } else { format!("&{}", render_type_fn(&fn_ctx, &p.ty)) }
                    } else { format!("&{}", render_type_fn(&fn_ctx, &p.ty)) }
                }
            };
            fn_ctx.templates.render_with("fn_param", None, &[], &[("name", param_name.as_str()), ("type", type_s.as_str())])
                .unwrap_or_else(|| format!("{}: {}", p.name, type_s))
        })
        .collect::<Vec<_>>()
        .join(", ");

    // Function body: render Block contents directly (no IIFE wrapper)
    let body_raw = match &func.body.kind {
        IrExprKind::Block { stmts, expr } => {
            let mut parts: Vec<String> = stmts.iter()
                .map(|s| terminate_stmt(&fn_ctx, render_stmt_fn(&fn_ctx, s)))
                .collect();
            if let Some(e) = expr {
                let mut expr_str = render_expr_fn(&fn_ctx, e);
                // Val-wrapped var at function return: unwrap to plain T.
                // For simple Var: append .into_inner().
                // For ResultOk { Var }: the Var inside Ok needs unwrapping.
                if let IrExprKind::Var { id } = &e.kind {
                    if fn_ctx.ann.is_rc_cow(id) {
                        expr_str = format!("{}.into_inner()", expr_str);
                    }
                } else if let IrExprKind::ResultOk { expr: inner } = &e.kind {
                    if let IrExprKind::Var { id } = &inner.kind {
                        if fn_ctx.ann.is_rc_cow(id) {
                            // Re-render with unwrap: Ok(var.into_inner())
                            let var_name = fn_ctx.var_table.get(*id).name.to_string();
                            expr_str = format!("Ok({}.into_inner())", var_name);
                        }
                    }
                }
                let is_control = matches!(&e.kind, IrExprKind::Break | IrExprKind::Continue);
                if is_control {
                    parts.push(expr_str);
                } else {
                    parts.push(fn_ctx.templates.render_with("block_result_expr", None, &[], &[("expr", expr_str.as_str())])
                        .unwrap_or_else(|| expr_str.clone()));
                }
            }
            parts.join("\n")
        }
        _ => {
            let mut raw = render_expr_fn(&fn_ctx, &func.body);
            if let IrExprKind::Var { id } = &func.body.kind {
                if fn_ctx.ann.is_rc_cow(id) {
                    raw = format!("{}.into_inner()", raw);
                }
            }
            let is_control = matches!(&func.body.kind, IrExprKind::Break | IrExprKind::Continue);
            if is_control {
                raw
            } else {
                fn_ctx.templates.render_with("block_result_expr", None, &[], &[("expr", raw.as_str())])
                    .unwrap_or_else(|| raw.clone())
            }
        }
    };
    let body_str = indent_lines(&body_raw, 4);
    let ret_str = render_type_fn(ctx, &func.ret_ty);

    // Build generics string for functions
    let fn_generics = if let Some(generics) = &func.generics {
        if generics.is_empty() {
            String::new()
        } else {
            let bound_template = if fn_ctx.minimal_generic_bounds { "generic_bound_minimal" } else { "generic_bound_full" };
            let params = generics.iter().map(|g| {
                ctx.templates.render_with(bound_template, None, &[], &[("name", g.name.as_str())])
                    .unwrap_or_else(|| g.name.to_string())
            }).collect::<Vec<_>>().join(", ");
            format!("<{}>", params)
        }
    } else {
        String::new()
    };

    // `effect fn main` is renamed to `__almide_main` and given a thin `fn main`
    // wrapper (below) that reports an unhandled `Err` via Display (`Error: <msg>`)
    // and exits 1 — instead of Rust's default `Termination`, which prints the
    // Debug form (`Error: "<msg>"`, with quotes). This keeps the error format an
    // intentional Almide decision and lets the WASM target match it byte-for-byte.
    let is_rust_effect_main = matches!(ctx.target, Target::Rust)
        && func.name.as_str() == "main" && func.is_effect && !func.is_test;
    // A plain (non-effect) `fn main` also needs the wrapper when there are
    // abortable lazy top-lets to force at startup (wasm-eager parity).
    let is_rust_plain_main_with_forces = matches!(ctx.target, Target::Rust)
        && func.name.as_str() == "main" && !func.is_effect && !func.is_test
        && ctx.ann.global_init_order.iter().any(|v| matches!(
            ctx.ann.globals.get(v).map(|i| i.storage),
            Some(almide_ir::top_let_storage::TopLetStorage::Lazy { eager_force: true })
        ));

    // Sanitize function name: spaces/dots/hyphens → underscores.
    // Test blocks already carry `TEST_NAME_PREFIX` from lowering so they
    // cannot collide with user fns here — no conditional prefixing needed.
    let raw_name = if is_rust_effect_main || is_rust_plain_main_with_forces {
        "__almide_main".to_string()
    } else {
        func.name.to_string()
    };
    let mut safe_name = raw_name.replace(' ', "_").replace('-', "_").replace('.', "_")
        .replace('+', "_plus_").replace('/', "_div_").replace('*', "_mul_")
        .replace('(', "").replace(')', "").replace(',', "_").replace(':', "_")
        .replace('=', "_eq_").replace('!', "_bang_").replace('?', "_q_")
        .replace('<', "_lt_").replace('>', "_gt_").replace('[', "_").replace(']', "_")
        .replace('|', "_pipe_").replace('&', "_amp_").replace('%', "_mod_");
    // Strip any remaining non-ASCII characters (e.g., →, ★, etc.)
    safe_name = safe_name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    // Escape target-specific keywords via template
    let target_keywords = ["while", "for", "if", "else", "match", "loop", "break", "continue",
        "return", "fn", "let", "mut", "use", "mod", "pub", "struct", "enum", "impl", "trait",
        "type", "where", "as", "in", "ref", "self", "super", "crate", "const", "static",
        "unsafe", "async", "await", "dyn", "move", "true", "false", "try", "yield"];
    if target_keywords.contains(&safe_name.as_str()) {
        safe_name = ctx.templates.render_with("keyword_escape", None, &[], &[("name", safe_name.as_str())])
            .unwrap_or(safe_name);
    }
    // Emit-time prefix: module_origin → "almide_rt_{origin}_{name}"
    // IR name stays clean; prefix is a rendering concern.
    if let Some(ref origin) = func.module_origin {
        // A qualified-method fn (e.g. a Codec `Type.encode` whose type is now the
        // namespaced `mod.Type`) flattens to `mod_Type_method`; the emit prefix
        // already adds `almide_rt_<origin>_`, so strip a leading `<origin>_` to
        // avoid doubling the module (#433 × #411-B). Mirrors the call-site strip;
        // gated on a dotted IR name so plain module fns are unaffected.
        let base: String = if func.name.as_str().contains('.') {
            safe_name.strip_prefix(&format!("{}_", origin)).unwrap_or(&safe_name).to_string()
        } else {
            safe_name.clone()
        };
        safe_name = format!("almide_rt_{}_{}", origin, base);
    }
    let safe_name = format!("{}{}", safe_name, fn_generics);

    let construct = if func.is_test {
        "test_block"
    } else if func.is_effect {
        "effect_fn_decl"
    } else {
        "fn_decl"
    };
    let fn_code = fn_ctx.templates.render_with(construct, None, &[], &[("name", safe_name.as_str()), ("params", params_str.as_str()), ("return_type", ret_str.as_str()), ("body", body_str.as_str())])
        .unwrap_or_else(|| format!("fn {}() {{ }}", func.name));

    // Wrap `effect fn main`: report an unhandled error via Display + exit 1.
    // Both wrapper shapes first FORCE the abortable lazy top-lets in declaration
    // order, so an aborting initializer (integer `/`/`%`) fires at startup —
    // byte-identical to wasm's eager top-let evaluation in `_start`.
    let force_lines: String = ctx.ann.global_init_order.iter()
        .filter_map(|v| ctx.ann.globals.get(v))
        .filter(|i| matches!(i.storage, almide_ir::top_let_storage::TopLetStorage::Lazy { eager_force: true }))
        .map(|i| format!("    std::sync::LazyLock::force(&{});\n", i.static_name))
        .collect();
    let fn_code = if is_rust_effect_main {
        format!("{}\n\nfn main() {{\n{}    if let Err(__almide_err) = __almide_main() {{\n        eprintln!(\"Error: {{}}\", __almide_err);\n        std::process::exit(1);\n    }}\n}}", fn_code, force_lines)
    } else if is_rust_plain_main_with_forces {
        format!("{}\n\nfn main() {{\n{}    __almide_main();\n}}", fn_code, force_lines)
    } else {
        fn_code
    };

    // Prepend doc comment if present
    if let Some(ref doc) = func.doc {
        let doc_lines: String = doc.lines()
            .map(|line| if line.is_empty() { "///".to_string() } else { format!("/// {}", line) })
            .collect::<Vec<_>>()
            .join("\n");
        format!("{}\n{}", doc_lines, fn_code)
    } else {
        fn_code
    }
}

/// Whether an expression contains an integer `/` or `%` anywhere — the ops that
/// render as the aborting totality macros (`Error: <msg>` + exit 1). Uses the
/// exhaustive `IrVisitor` walk so new IR nodes are traversed automatically.
fn contains_aborting_int_div(expr: &IrExpr) -> bool {
    use almide_ir::visit::{IrVisitor, walk_expr};
    struct Finder { found: bool }
    impl IrVisitor for Finder {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.found { return; }
            if matches!(&e.kind, IrExprKind::BinOp { op: BinOp::DivInt | BinOp::ModInt, .. }) {
                self.found = true;
                return;
            }
            walk_expr(self, e);
        }
    }
    let mut f = Finder { found: false };
    f.visit_expr(expr);
    f.found
}

// ── Full program rendering ──

pub fn render_program(ctx: &RenderContext, program: &IrProgram) -> String {
    // Build constructor → enum name map
    // Build type alias map for transparent expansion
    let mut type_aliases = std::collections::HashMap::new();
    let mut generic_types = std::collections::HashSet::new();
    // Collect type aliases and generic types from ALL sources
    // (top-level + modules) so render_type can expand them everywhere.
    let all_type_decls = program.type_decls.iter()
        .chain(program.modules.iter().flat_map(|m| m.type_decls.iter()));
    for td in all_type_decls {
        match &td.kind {
            IrTypeDeclKind::Alias { target } => {
                // Opaque (mod/local) aliases are newtypes — don't expand transparently
                if matches!(td.visibility, IrVisibility::Public) {
                    type_aliases.insert(td.name, target.clone());
                }
            }
            _ => {}
        }
        // Track types with generic parameters
        if td.generics.as_ref().map_or(false, |g| !g.is_empty()) {
            generic_types.insert(td.name);
        }
    }
    // Record/variant types that get a generated `AlmideRepr` impl — a value of
    // such a type interpolated in a string renders to its literal form. Gate
    // mirrors `render_repr_impl` (closure-bearing types are excluded).
    let mut repr_named_types = std::collections::HashSet::new();
    for td in program.type_decls.iter()
        .chain(program.modules.iter().flat_map(|m| m.type_decls.iter()))
    {
        if declarations::type_has_repr_impl(td) {
            repr_named_types.insert(td.name);
        }
    }
    let mut ann = ctx.ann.clone();
    // Compute which user types cannot derive PartialEq (contain Matrix,
    // Fn, or a field whose type itself blocks equality). Must consider
    // type decls from every module, not just the top-level program,
    // because user programs reference types defined in other modules.
    let all_type_decls: Vec<IrTypeDecl> = program.type_decls.iter()
        .chain(program.modules.iter().flat_map(|m| m.type_decls.iter()))
        .cloned()
        .collect();
    ann.eq_blocked_types = super::walker::declarations::compute_eq_blocked_types(&all_type_decls);
    ann.phantom_param_structs = super::walker::declarations::compute_phantom_param_structs(&all_type_decls);
    // §4 endgame: the legacy pre-index (lazy_top_let_names /
    // eager_force_top_lets / const_top_let_vars) and the mutable-storage
    // register block are GONE — every consumer reads the TopLetStorage
    // attribute computed by TopLetStoragePass, and the agreement verifier
    // that soaked the flip (v0.27.2) retired with the predicates it
    // compared. One rule, one place.
    // Classify function-local `var` bindings:
    //   LocalMut (let mut T)  — not captured by closures, no RcCow overhead
    //   RcCow                 — captured by a lambda, needs COW semantics
    //
    // Scan IR Bind statements for `var` of non-Copy types, then check if
    // any lambda in the same function captures that var.
    {
        use almide_ir::annotations::VarStorage;
        let mut exclude: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for tl in &program.top_lets {
            if tl.mutable { exclude.insert(tl.var.0); }
        }
        for module in &program.modules {
            for tl in &module.top_lets {
                if tl.mutable { exclude.insert(tl.var.0); }
            }
        }
        for func in &program.functions {
            for p in &func.params { exclude.insert(p.var.0); }
        }
        for module in &program.modules {
            for func in &module.functions {
                for p in &func.params { exclude.insert(p.var.0); }
            }
        }

        // Phase 1: Collect all non-Copy `var` bindings
        struct VarBindCollector { vars: std::collections::HashSet<u32> }
        impl almide_ir::visit::IrVisitor for VarBindCollector {
            fn visit_stmt(&mut self, stmt: &IrStmt) {
                if let IrStmtKind::Bind { var, mutability: almide_ir::Mutability::Var, ty, .. } = &stmt.kind {
                    // §4 stage 2c (#531): derived from THE copy-ness
                    // classifier (projection table in top_let_storage).
                    if !almide_ir::top_let_storage::rccow_copyish(ty) {
                        self.vars.insert(var.0);
                    }
                }
                almide_ir::visit::walk_stmt(self, stmt);
            }
            fn visit_expr(&mut self, expr: &IrExpr) {
                almide_ir::visit::walk_expr(self, expr);
            }
        }
        let mut collector = VarBindCollector { vars: std::collections::HashSet::new() };
        use almide_ir::visit::IrVisitor;
        for func in &program.functions {
            collector.visit_expr(&func.body);
        }
        for module in &program.modules {
            for func in &module.functions {
                collector.visit_expr(&func.body);
            }
        }

        // Phase 2: Find vars captured by any lambda — via the single shared
        // free-variable analysis (`almide_ir::free_vars`), the same one the WASM
        // closure path uses. A lambda's captures are the free vars of its body
        // relative to its params; the union over every lambda is the full captured
        // set. `free_vars` tracks all binders (block lets incl. destructure, match
        // arms, for-in vars, nested lambdas), so this is strictly more accurate than
        // the old hand-rolled lambda-depth walker. (Closure v2, P4: one capture
        // analysis for both targets.)
        struct CaptureUnion { captured: std::collections::HashSet<u32> }
        impl almide_ir::visit::IrVisitor for CaptureUnion {
            fn visit_expr(&mut self, expr: &IrExpr) {
                if let IrExprKind::Lambda { params, body, .. } = &expr.kind {
                    let param_set: std::collections::HashSet<VarId> =
                        params.iter().map(|(v, _)| *v).collect();
                    for v in almide_ir::free_vars::free_vars(body, &param_set) {
                        self.captured.insert(v.0);
                    }
                }
                almide_ir::visit::walk_expr(self, expr);
            }
        }
        let mut cap = CaptureUnion { captured: std::collections::HashSet::new() };
        for func in &program.functions { cap.visit_expr(&func.body); }
        for module in &program.modules {
            for func in &module.functions { cap.visit_expr(&func.body); }
        }

        // Phase 3: Only vars captured by lambdas get RcCow; rest are LocalMut (let mut)
        for var_id in collector.vars {
            if exclude.contains(&var_id) { continue; }
            // Captured mutable vars that became shared cells (`Rc<Cell>` for Copy via
            // P3, `SharedMut` for non-Copy via P6) are driven by the shared-mut path,
            // NOT RcCow — RcCow's copy-on-write would lose a mutation made through the
            // closure. (Closure v2 P6.)
            if ann.is_shared_mut(&VarId(var_id)) { continue; }
            if cap.captured.contains(&var_id) {
                ann.var_storage.insert(VarId(var_id), VarStorage::RcCow);
            }
            // LocalMut: no entry in var_storage → walker emits plain `let mut T`
        }
        // Note: captured Copy-type mutable vars (Int/Float/Bool) are classified as
        // `shared_mut_vars` (→ `Rc<Cell<T>>`) by CaptureClonePass, which runs before
        // it must decide whether to clone-wrap the (now non-Copy) capture. Those
        // flow in via `program.codegen_annotations` → `ctx.ann`. (Closure v2, P3.)
    }
    let mut ctx = RenderContext {
        templates: ctx.templates,
        var_table: ctx.var_table,
        indent: ctx.indent,
        target: ctx.target,
        auto_unwrap: ctx.auto_unwrap,
        is_test: ctx.is_test,
        ann,
        type_aliases,
        generic_types,
        minimal_generic_bounds: false,
        repr_c: ctx.repr_c,
        ref_params: std::collections::HashSet::new(),
        ref_mut_params: std::collections::HashSet::new(),
        repr_named_types,
        fn_err_ty: None,
    };
    for td in &program.type_decls {
        if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
            for c in cases {
                ctx.ann.ctor_to_enum.insert(c.name.to_string(), td.name.to_string());
            }
        }
    }
    // Also register constructors from imported modules
    for module in &program.modules {
        for td in &module.type_decls {
            if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
                for c in cases {
                    ctx.ann.ctor_to_enum.insert(c.name.to_string(), td.name.to_string());
                }
            }
        }
    }

    // Build anonymous record maps (populated by target-specific pipeline)
    ctx.ann.named_records = collect_named_records(program);
    ctx.ann.anon_records = collect_anon_records(program, &ctx.ann.named_records);
    ctx.ann.anon_records_with_fn = declarations::take_anon_fn_keys();
    ctx.ann.record_field_counts = collect_record_field_counts(program);

    let mut parts = Vec::new();

    // Anonymous record struct definitions (only if anon_records is populated).
    // SORTED iteration: anon_records is a HashMap, and emitting in its raw
    // iteration order made the generated Rust SOURCE nondeterministic
    // run-to-run (three runs, three different bytes) — semantically neutral
    // for rustc but fatal for reproducible builds and any byte-diff gate.
    if !ctx.ann.anon_records.is_empty() {
        let mut sorted_anon: Vec<(&Vec<String>, &String)> = ctx.ann.anon_records.iter().collect();
        sorted_anon.sort_by(|a, b| a.1.cmp(b.1));
        for (field_names, struct_name) in sorted_anon {
            // A closure field can't be Debug/PartialEq, so such a struct derives
            // Clone only (the `has_fn_fields` struct_decl) and drops the
            // `Debug + PartialEq` generic bounds — derive(Clone) re-adds `T: Clone`
            // itself. Mirrors the `type`-declared record path. (Cross-target gaps.)
            let has_fn = ctx.ann.anon_records_with_fn.contains(field_names);
            let generics: Vec<String> = (0..field_names.len())
                .map(|i| {
                    let name_s = format!("T{}", i);
                    if has_fn {
                        name_s
                    } else {
                        ctx.templates.render_with("generic_bound_full", None, &[], &[("name", name_s.as_str())])
                            .unwrap_or_else(|| format!("T{}", i))
                    }
                })
                .collect();
            let fields: Vec<String> = field_names.iter().enumerate()
                .map(|(i, name)| {
                    let type_s = format!("T{}", i);
                    ctx.templates.render_with("struct_field", None, &[], &[("name", name.as_str()), ("type", type_s.as_str())])
                        .unwrap_or_else(|| format!("    pub {}: T{}", name, i))
                })
                .collect();
            let fields_str = fields.join("\n");
            let full_name = format!("{}<{}>", struct_name, generics.join(", "));
            let mut decl_attrs: Vec<&str> = if ctx.repr_c { vec!["repr_c"] } else { vec![] };
            if has_fn { decl_attrs.push("has_fn_fields"); }
            let repr_prefix = if ctx.repr_c { "#[repr(C)]\n" } else { "" };
            parts.push(ctx.templates.render_with("struct_decl", None, &decl_attrs, &[("name", full_name.as_str()), ("fields", fields_str.as_str())])
                .unwrap_or_else(|| format!("{}pub struct {} {{\n{}\n}}", repr_prefix, struct_name, fields_str)));

            // `AlmideRepr` impl for the anonymous struct: `"${rec}"` renders an
            // anonymous record to `{ x: 1, y: 2 }` — NO type name, because it HAS
            // none — byte-identically with the WASM anon-record walk. A
            // closure-bearing anon record (`has_fn`) is not `AlmideRepr`, so it is
            // skipped (it never reaches compound interp). Fields render in the
            // struct's own field order, which is the SORTED field-name list (this
            // `field_names` is the sorted key from `collect_anon_records`); the
            // WASM walk sorts to match (see `emit_repr_record`).
            if !has_fn {
                let bare_generics: Vec<String> = (0..field_names.len())
                    .map(|i| format!("T{}", i)).collect();
                // The anon struct declares each param via `generic_bound_full`
                // (`T: Clone + Debug + PartialEq`); the impl must satisfy those
                // same bounds, plus `AlmideRepr` so the field reprs compose.
                // Reuse the template so the bounds stay in lock-step with the decl.
                let impl_bounds = bare_generics.iter()
                    .map(|t| {
                        let own = ctx.templates.render_with("generic_bound_full", None, &[], &[("name", t.as_str())])
                            .unwrap_or_else(|| format!("{}: Clone + std::fmt::Debug + PartialEq", t));
                        match own.split_once(':') {
                            Some((name, rest)) => format!("{}: AlmideRepr +{}", name.trim_end(), rest),
                            None => format!("{}: AlmideRepr", t),
                        }
                    })
                    .collect::<Vec<_>>().join(", ");
                let target = format!("{}<{}>", struct_name, bare_generics.join(", "));
                let fmt = field_names.iter().enumerate()
                    .map(|(i, name)| format!("{}{}: {{}}", if i > 0 { ", " } else { "" }, name))
                    .collect::<Vec<_>>().join("");
                let args = field_names.iter()
                    .map(|name| format!("self.{}.almide_repr()", name))
                    .collect::<Vec<_>>().join(", ");
                parts.push(format!(
                    "impl<{}> AlmideRepr for {} {{ fn almide_repr(&self) -> String {{ format!(\"{{{{ {} }}}}\", {}) }} }}",
                    impl_bounds, target, fmt, args
                ));
            }
        }
    }

    // Type declarations — track emitted names to deduplicate across modules
    let mut emitted_types: std::collections::HashSet<String> = std::collections::HashSet::new();
    for td in &program.type_decls {
        emitted_types.insert(td.name.as_str().to_string());
        let mut rendered = render_type_decl(&ctx, td);
        if let Some(ref doc) = td.doc {
            let doc_lines: String = doc.lines()
                .map(|line| if line.is_empty() { "///".to_string() } else { format!("/// {}", line) })
                .collect::<Vec<_>>()
                .join("\n");
            rendered = format!("{}\n{}", doc_lines, rendered);
        }
        parts.push(rendered);
    }

    // Top-level lets and vars — §4 Stage 2: the declaration consumes the
    // SAME GlobalInfo every reference site dispatches on (storage class and
    // static name decided once, in the attribute pass). The former
    // `lazy_vars` mid-emission write is gone — no reader remains.
    for tl in &program.top_lets {
        let ty_str = render_type_fn(&ctx, &tl.ty);
        let val_str = render_expr_fn(&ctx, &tl.value);
        let info = ctx.ann.globals.get(&tl.var).unwrap_or_else(|| panic!(
            "[COMPILER BUG] top-let `{}` missing from the storage attribute",
            ctx.var_table.get(tl.var).name.as_str()
        ));
        use almide_ir::top_let_storage::TopLetStorage as Tls;
        let name_upper = info.static_name.as_str();
        let mut rendered = match info.storage {
            Tls::Cell =>
                format!("thread_local! {{ static {}: std::cell::Cell<{}> = std::cell::Cell::new({}); }}", name_upper, ty_str, val_str),
            Tls::RcRefCell =>
                format!("thread_local! {{ static {}: std::cell::RefCell<std::rc::Rc<{}>> = std::cell::RefCell::new(std::rc::Rc::new({})); }}", name_upper, ty_str, val_str),
            Tls::Const | Tls::Lazy { .. } => {
                let construct = match info.storage {
                    Tls::Const => "top_let_const",
                    _ => "top_let_lazy",
                };
                ctx.templates.render_with(construct, None, &[], &[("name", name_upper), ("type", ty_str.as_str()), ("value", val_str.as_str())])
                    .unwrap_or_else(|| format!("const {} = {};", name_upper, val_str))
            }
        };
        if let Some(ref doc) = tl.doc {
            let doc_lines: String = doc.lines()
                .map(|line| if line.is_empty() { "///".to_string() } else { format!("/// {}", line) })
                .collect::<Vec<_>>()
                .join("\n");
            rendered = format!("{}\n{}", doc_lines, rendered);
        }
        parts.push(rendered);
    }

    // Functions (non-test): separate extern fn imports from regular functions
    let mut import_parts = Vec::new();
    let mut fn_parts = Vec::new();
    for func in program.functions.iter().filter(|f| !f.is_test) {
        let rendered = render_function(&ctx, func);
        if !func.extern_attrs.is_empty() {
            import_parts.push(rendered);
        } else {
            fn_parts.push(rendered);
        }
    }
    // Emit imports first as a group, then functions
    if !import_parts.is_empty() {
        parts.push(import_parts.join("\n"));
    }
    parts.extend(fn_parts);

    // Test functions
    let test_fns: Vec<&IrFunction> = program.functions.iter().filter(|f| f.is_test).collect();
    if !test_fns.is_empty() {
        let test_parts: Vec<String> = test_fns.iter()
            .map(|f| render_function(&ctx, f))
            .collect();
        let tests_s = test_parts.join("\n\n");
        let indented_tests = indent_lines(&tests_s, 4);
        let wrapped = ctx.templates.render_with("test_module", None, &[], &[("tests", indented_tests.as_str())])
            .unwrap_or_else(|| test_parts.join("\n\n"));
        parts.push(wrapped);
    }

    // Modules are flattened by ir_link_flatten into root functions/types/top_lets.
    // No per-module iteration needed.
    debug_assert!(program.modules.is_empty(), "ir_link_flatten should have emptied modules");

    parts.join("\n\n")
}

// ── C FFI extern codegen ──────────────────────────────────────

/// Render @extern(c, "lib", "func") as: extern "C" block + safe Almide wrapper.
///
/// Type mapping (Almide → C extern → safe wrapper):
/// Render @native("target", "module", "function") — delegates to module::function().
/// Parameters use reference types (&str, &[T]) matching native Rust conventions.
fn render_native_call(ctx: &RenderContext, func: &IrFunction, attr: &almide_lang::ast::ExternAttr) -> String {
    use types::render_type;
    use almide_lang::types::{Ty, TypeConstructorId};
    let mod_name = attr.module.as_str();
    let fn_name = attr.function.as_str();

    // Wrapper params: use reference types for String/List to match native Rust conventions
    let params: Vec<String> = func.params.iter().map(|p| {
        let ty = match &p.ty {
            Ty::String => "&str".to_string(),
            Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
                format!("&[{}]", render_type(ctx, &args[0]))
            }
            _ => render_type(ctx, &p.ty),
        };
        format!("{}: {}", p.name, ty)
    }).collect();

    // Call args: pass through directly (wrapper already uses reference types)
    let args: Vec<String> = func.params.iter().map(|p| {
        p.name.to_string()
    }).collect();

    let ret = render_type(ctx, &func.ret_ty);
    format!("fn {}({}) -> {} {{\n    {}::{}({})\n}}",
        func.name, params.join(", "), ret,
        mod_name, fn_name, args.join(", "))
}

///   Int     → i32 in extern, i64 in wrapper (cast)
///   Float   → f64 (same)
///   Bool    → i32 in extern, bool in wrapper (cast)
///   RawPtr  → *mut u8 (same)
fn render_extern_c(ctx: &RenderContext, func: &IrFunction, attr: &almide_lang::ast::ExternAttr) -> String {
    use almide_lang::types::Ty;

    let lib = attr.module.as_str();
    let c_func = attr.function.as_str();
    let almide_name = func.name.as_str();

    // Build C parameter list and Almide parameter list
    let mut c_params = Vec::new();
    let mut almide_params = Vec::new();
    let mut call_args = Vec::new();

    for p in &func.params {
        let name = p.name.as_str();
        let (c_ty, almide_ty, to_c) = extern_c_type_mapping(ctx, &p.ty, name);
        c_params.push(format!("{}: {}", name, c_ty));
        almide_params.push(format!("{}: {}", name, almide_ty));
        call_args.push(to_c);
    }

    let (c_ret, almide_ret, from_c) = extern_c_return_mapping(ctx, &func.ret_ty);

    let c_params_str = c_params.join(", ");
    let almide_params_str = almide_params.join(", ");
    let call_args_str = call_args.join(", ");

    format!(
        "#[link(name = \"{lib}\")]\nextern \"C\" {{ fn {c_func}({c_params_str}) -> {c_ret}; }}\n\
         pub fn {almide_name}({almide_params_str}) -> {almide_ret} {{ {from_c} }}",
        lib = lib,
        c_func = c_func,
        c_params_str = c_params_str,
        c_ret = c_ret,
        almide_name = almide_name,
        almide_params_str = almide_params_str,
        almide_ret = almide_ret,
        from_c = format!("unsafe {{ {} }}", wrap_return(&from_c, c_func, &call_args_str)),
    )
}

/// Map an Almide param type to (C type, Almide type, call expression).
fn extern_c_type_mapping(_ctx: &RenderContext, ty: &almide_lang::types::Ty, name: &str) -> (String, String, String) {
    use almide_lang::types::Ty;
    match ty {
        Ty::Int    => ("i32".into(), "i64".into(), format!("{} as i32", name)),
        Ty::Float  => ("f64".into(), "f64".into(), name.into()),
        Ty::Bool   => ("i32".into(), "bool".into(), format!("if {} {{ 1 }} else {{ 0 }}", name)),
        Ty::RawPtr => ("*mut u8".into(), "*mut u8".into(), name.into()),
        Ty::String => ("*const u8".into(), "String".into(), format!("{}.as_ptr()", name)),
        other      => {
            let s = format!("{:?}", other);
            (s.clone(), s.clone(), name.into())
        }
    }
}

/// Map an Almide return type to (C type, Almide type, conversion wrapper template).
fn extern_c_return_mapping(_ctx: &RenderContext, ty: &almide_lang::types::Ty) -> (String, String, String) {
    use almide_lang::types::Ty;
    match ty {
        Ty::Int    => ("i32".into(), "i64".into(), "as_i64".into()),
        Ty::Float  => ("f64".into(), "f64".into(), "direct".into()),
        Ty::Bool   => ("i32".into(), "bool".into(), "ne_zero".into()),
        Ty::RawPtr => ("*mut u8".into(), "*mut u8".into(), "direct".into()),
        Ty::Unit   => ("()".into(), "()".into(), "direct".into()),
        _other     => ("i32".into(), "i64".into(), "as_i64".into()),
    }
}

fn wrap_return(mode: &str, c_func: &str, call_args: &str) -> String {
    match mode {
        "as_i64"  => format!("{}({}) as i64", c_func, call_args),
        "ne_zero" => format!("{}({}) != 0", c_func, call_args),
        _         => format!("{}({})", c_func, call_args),
    }
}

/// Render @export(c, "symbol") — emits normal Almide fn + thin extern "C" wrapper.
///
/// ```text
/// pub fn my_add(a: i64, b: i64) -> i64 { (a + b) }
///
/// #[export_name = "my_add"]
/// pub extern "C" fn __c_my_add(a: i32, b: i32) -> i32 {
///     my_add(a as i64, b as i64) as i32
/// }
/// ```
fn render_export_c(ctx: &RenderContext, func: &IrFunction, attr: &almide_lang::ast::ExportAttr) -> String {
    use almide_lang::types::Ty;

    let symbol = attr.symbol.as_str();
    let fn_name = func.name.as_str();

    // 1. Render the normal Almide function (strip export_attrs to avoid recursion)
    let mut clean_func = func.clone();
    clean_func.export_attrs.clear();
    let almide_fn = render_function(ctx, &clean_func);

    // 2. Build C wrapper
    let mut c_params = Vec::new();
    let mut call_args = Vec::new();

    for p in &func.params {
        let name = p.name.as_str();
        match &p.ty {
            Ty::Int    => { c_params.push(format!("{}: i32", name)); call_args.push(format!("{} as i64", name)); }
            Ty::Float  => { c_params.push(format!("{}: f64", name)); call_args.push(name.into()); }
            Ty::Bool   => { c_params.push(format!("{}: i32", name)); call_args.push(format!("{} != 0", name)); }
            Ty::RawPtr => { c_params.push(format!("{}: *mut u8", name)); call_args.push(name.into()); }
            _          => { let t = render_type_fn(ctx, &p.ty); c_params.push(format!("{}: {}", name, t)); call_args.push(name.into()); }
        }
    }

    let (c_ret, wrap_open, wrap_close) = match &func.ret_ty {
        Ty::Int    => ("i32", "(", ") as i32"),
        Ty::Bool   => ("i32", "if ", " { 1 } else { 0 }"),
        Ty::RawPtr => ("*mut u8", "", ""),
        Ty::Float  => ("f64", "", ""),
        Ty::Unit   => ("()", "", ""),
        _          => ("i32", "(", ") as i32"),
    };

    let c_params_str = c_params.join(", ");
    let call_args_str = call_args.join(", ");

    let wrapper = format!(
        "#[export_name = \"{symbol}\"]\npub extern \"C\" fn __c_{fn_name}({c_params_str}) -> {c_ret} {{ {wo}{fn_name}({args}){wc} }}",
        symbol = symbol, fn_name = fn_name,
        c_params_str = c_params_str, c_ret = c_ret,
        wo = wrap_open, args = call_args_str, wc = wrap_close,
    );

    format!("{}\n\n{}", almide_fn, wrapper)
}
