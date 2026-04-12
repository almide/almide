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
}

impl<'a> RenderContext<'a> {
    pub fn new(templates: &'a TemplateSet, var_table: &'a VarTable) -> Self {
        Self { templates, var_table, indent: 0, target: Target::Rust, auto_unwrap: false, is_test: false, ann: CodegenAnnotations::default(), type_aliases: std::collections::HashMap::new(), generic_types: std::collections::HashSet::new(), minimal_generic_bounds: false, repr_c: false }
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
}

// ── Function rendering ──

pub fn render_function(ctx: &RenderContext, func: &IrFunction) -> String {
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
    };

    // Extern fn: emit import/use via template (rs) or extern "C" block (c)
    if !func.extern_attrs.is_empty() {
        let target_str = match ctx.target {
            Target::Rust => "rs",
            Target::TypeScript => "ts",
            _ => "",
        };
        for attr in &func.extern_attrs {
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
                let expr_str = render_expr_fn(&fn_ctx, e);
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
            let raw = render_expr_fn(&fn_ctx, &func.body);
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

    // Sanitize function name: spaces/dots/hyphens → underscores
    // Prefix test functions to avoid name collision with real functions
    let raw_name = if func.is_test {
        format!("__test_almd_{}", func.name)
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

// ── Full program rendering ──

pub fn render_program(ctx: &RenderContext, program: &IrProgram) -> String {
    // Build constructor → enum name map
    // Build type alias map for transparent expansion
    let mut type_aliases = std::collections::HashMap::new();
    let mut generic_types = std::collections::HashSet::new();
    for td in &program.type_decls {
        match &td.kind {
            IrTypeDeclKind::Alias { target } => { type_aliases.insert(td.name, target.clone()); }
            _ => {}
        }
        // Track types with generic parameters
        if td.generics.as_ref().map_or(false, |g| !g.is_empty()) {
            generic_types.insert(td.name);
        }
    }
    let mut ctx = RenderContext {
        templates: ctx.templates,
        var_table: ctx.var_table,
        indent: ctx.indent,
        target: ctx.target,
        auto_unwrap: ctx.auto_unwrap,
        is_test: ctx.is_test,
        ann: ctx.ann.clone(),
        type_aliases,
        generic_types,
        minimal_generic_bounds: false,
        repr_c: ctx.repr_c,
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
    ctx.ann.record_field_counts = collect_record_field_counts(program);

    let mut parts = Vec::new();

    // Anonymous record struct definitions (only if anon_records is populated)
    if !ctx.ann.anon_records.is_empty() {
        for (field_names, struct_name) in &ctx.ann.anon_records {
            let generics: Vec<String> = (0..field_names.len())
                .map(|i| {
                    let name_s = format!("T{}", i);
                    ctx.templates.render_with("generic_bound_full", None, &[], &[("name", name_s.as_str())])
                        .unwrap_or_else(|| format!("T{}", i))
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
            let decl_attrs: Vec<&str> = if ctx.repr_c { vec!["repr_c"] } else { vec![] };
            let repr_prefix = if ctx.repr_c { "#[repr(C)]\n" } else { "" };
            parts.push(ctx.templates.render_with("struct_decl", None, &decl_attrs, &[("name", full_name.as_str()), ("fields", fields_str.as_str())])
                .unwrap_or_else(|| format!("{}pub struct {} {{\n{}\n}}", repr_prefix, struct_name, fields_str)));
        }
    }

    // Type declarations
    for td in &program.type_decls {
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

    // Top-level lets
    for tl in &program.top_lets {
        let name = ctx.var_table.get(tl.var).name.clone();
        let ty_str = render_type_fn(&ctx, &tl.ty);
        let val_str = render_expr_fn(&ctx, &tl.value);
        if matches!(tl.kind, TopLetKind::Lazy) {
            ctx.ann.lazy_vars.insert(tl.var);
        }
        let construct = match tl.kind {
            TopLetKind::Const => "top_let_const",
            TopLetKind::Lazy => "top_let_lazy",
        };
        let name_upper = name.to_uppercase();
        let mut rendered = ctx.templates.render_with(construct, None, &[], &[("name", name_upper.as_str()), ("type", ty_str.as_str()), ("value", val_str.as_str())])
            .unwrap_or_else(|| format!("const {} = {};", name, val_str));
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

    // Imported modules: render their type decls and functions
    for module in &program.modules {
        // Bundled .almd modules use minimal generic bounds (Clone only)
        // because their functions don't require PartialEq/PartialOrd/Debug.
        let is_bundled = almide_lang::stdlib_info::is_bundled_module(&module.name);
        let mut mod_ann = ctx.ann.clone();
        // Each module has its own VarTable, so VarIds from the parent's
        // lazy_vars would collide with module-local variables (parameters,
        // match bindings) that share the same numeric id.
        // Clear inherited lazy_vars; module-specific ones are added below.
        mod_ann.lazy_vars.clear();
        let mut mod_ctx = RenderContext {
            templates: ctx.templates,
            var_table: &module.var_table,
            indent: ctx.indent,
            target: ctx.target,
            auto_unwrap: false,
            is_test: false,
            ann: mod_ann,
            type_aliases: ctx.type_aliases.clone(),
            generic_types: ctx.generic_types.clone(),
            minimal_generic_bounds: is_bundled,
            repr_c: ctx.repr_c,
        };
        // Module type decls
        for td in &module.type_decls {
            parts.push(render_type_decl(&mod_ctx, td));
        }
        // Module functions (prefixed with module name or versioned name)
        let mod_ident = module.versioned_name
            .map(|v| v.replace('.', "_"))
            .unwrap_or_else(|| module.name.replace('.', "_"));
        // Module top-level lets (names already prefixed during lowering)
        for tl in &module.top_lets {
            let name = mod_ctx.var_table.get(tl.var).name.to_string();
            let ty_str = render_type_fn(&mod_ctx, &tl.ty);
            let val_str = render_expr_fn(&mod_ctx, &tl.value);
            if matches!(tl.kind, TopLetKind::Lazy) {
                mod_ctx.ann.lazy_vars.insert(tl.var);
            }
            let construct = match tl.kind {
                TopLetKind::Const => "top_let_const",
                TopLetKind::Lazy => "top_let_lazy",
            };
            let rendered = mod_ctx.templates.render_with(construct, None, &[], &[("name", name.as_str()), ("type", ty_str.as_str()), ("value", val_str.as_str())])
                .unwrap_or_else(|| format!("const {} = {};", name, val_str));
            parts.push(rendered);
        }
        for func in &module.functions {
            let rendered = render_function(&mod_ctx, func);
            let clean_name = func.name.replace(' ', "_").replace('-', "_").replace('.', "_");
            let prefixed_name = format!("almide_rt_{}_{}", mod_ident, clean_name);
            let prefixed = rendered
                .replacen(
                    &format!("fn {}", clean_name),
                    &format!("fn {}", prefixed_name),
                    1
                )
                // Also rename @extern(rs) use-as aliases to match the prefixed name
                .replacen(
                    &format!(" as {};", clean_name),
                    &format!(" as {};", prefixed_name),
                    1
                );
            parts.push(prefixed);
        }
    }

    parts.join("\n\n")
}

// ── C FFI extern codegen ──────────────────────────────────────

/// Render @extern(c, "lib", "func") as: extern "C" block + safe Almide wrapper.
///
/// Type mapping (Almide → C extern → safe wrapper):
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
/// ```rust
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
