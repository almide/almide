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
pub use helpers::ty_contains_any_recursive;
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

/// Is `name` a Rust keyword (strict, reserved, or weak) that must be raw-escaped
/// (`r#name`) when used as an identifier? Single source of truth for every
/// emission site — a var name, a fn parameter, a fn DEFINITION, and a fn CALL
/// site — so a user fn named `box`/`move`/`dyn`/… escapes identically on both
/// sides of the call (#659). An incomplete per-site list previously let the
/// definition or the call slip through unescaped, producing invalid Rust.
pub(crate) fn is_rust_keyword(name: &str) -> bool {
    matches!(name,
        "as" | "async" | "await" | "break" | "const" | "continue" | "crate"
        | "dyn" | "else" | "enum" | "extern" | "false" | "fn" | "for" | "if"
        | "impl" | "in" | "let" | "loop" | "match" | "mod" | "move" | "mut"
        | "pub" | "ref" | "return" | "self" | "Self" | "static" | "struct"
        | "super" | "trait" | "true" | "type" | "unsafe" | "use" | "where" | "while"
        | "abstract" | "become" | "box" | "do" | "final" | "macro" | "override"
        | "priv" | "try" | "typeof" | "unsized" | "virtual" | "yield"
    )
}

/// Escape `name` for use as a Rust identifier (definition or reference).
/// Single source of truth for every emission site (`var_name`, fn param, fn
/// DEFINITION, fn call site) — see `is_rust_keyword`.
///
/// `self`/`Self`/`super`/`crate` are Rust keywords rustc explicitly refuses
/// to raw-escape ("`self` cannot be a raw identifier") — they're rejected as
/// `r#self` too, unlike every other keyword here. Since every convention
/// method (`fn Type.method(...)`) compiles to a plain free function, not a
/// real Rust method, a literal `self`/bare-self-sugar param can't be a raw
/// identifier; it's renamed outright instead.
pub(crate) fn escape_rust_ident(name: &str, templates: &TemplateSet) -> String {
    match name {
        "self" | "Self" | "super" | "crate" => format!("almide_{}", name.to_lowercase()),
        _ if is_rust_keyword(name) => templates
            .render_with("keyword_escape", None, &[], &[("name", name)])
            .unwrap_or_else(|| name.to_string()),
        _ => name.to_string(),
    }
}

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
        escape_rust_ident(name.as_str(), self.templates)
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
        // A module fn's call sites all render the flatten prefix
        // (`almide_rt_<origin>_<name>`), so an extern binding must be emitted
        // under that same prefixed name — a bare `use bridge::f as f;` defines
        // a symbol nobody calls (porta wasm_rt: E0425 on almide_rt_wasm_rt_*).
        let emit_name = match &func.module_origin {
            Some(origin) => format!("almide_rt_{}_{}", origin,
                func.name.replace(' ', "_").replace('-', "_").replace('.', "_")),
            None => func.name.to_string(),
        };
        for attr in &func.extern_attrs {
            // @extern(rust, ...) / @extern(wasm, ...) — native module binding
            if attr.target == native_target {
                return render_native_call(ctx, func, attr, &emit_name);
            }
            if attr.target == target_str {
                return ctx.templates.render_with("extern_fn", None, &[], &[("module", attr.module.as_str()), ("function", attr.function.as_str()), ("name", emit_name.as_str())])
                    .unwrap_or_else(|| format!("// extern: {}.{}", attr.module, attr.function));
            }
            // @extern(c, "lib", "func") — generate extern "C" block + safe wrapper
            if attr.target == "c" && matches!(ctx.target, Target::Rust) {
                return render_extern_c(ctx, func, attr, &emit_name);
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
            // Escape Rust keywords in param names — same helper as every
            // other emission site (var_name, fn call, fn definition), so a
            // param named e.g. `self`/`box`/`move` matches at its binding and
            // every use within the body (#659's rule, applied here too).
            let mut param_name = escape_rust_ident(p.name.as_str(), fn_ctx.templates);
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
    // Escape a Rust-keyword fn name (`box` → `r#box`) so the DEFINITION
    // matches the CALL site exactly (#659).
    safe_name = escape_rust_ident(&safe_name, ctx.templates);
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

include!("mod_p2.rs");
include!("mod_p3.rs");
