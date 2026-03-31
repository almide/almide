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

use crate::ir::*;
use super::annotations::CodegenAnnotations;
use super::pass::Target;
use super::template::TemplateSet;

use types::render_type as render_type_fn;
use expressions::render_expr as render_expr_fn;
use statements::render_stmt as render_stmt_fn;
use helpers::terminate_stmt;
use declarations::{render_type_decl, collect_named_records, collect_anon_records};

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
    pub type_aliases: std::collections::HashMap<crate::intern::Sym, crate::types::Ty>,
    /// Use minimal generic bounds (Clone only) for bundled .almd module functions.
    pub minimal_generic_bounds: bool,
    /// Emit `#[repr(C)]` on structs/enums for stable C ABI layout.
    pub repr_c: bool,
}

impl<'a> RenderContext<'a> {
    pub fn new(templates: &'a TemplateSet, var_table: &'a VarTable) -> Self {
        Self { templates, var_table, indent: 0, target: Target::Rust, auto_unwrap: false, is_test: false, ann: CodegenAnnotations::default(), type_aliases: std::collections::HashMap::new(), minimal_generic_bounds: false, repr_c: false }
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
        minimal_generic_bounds: ctx.minimal_generic_bounds,
        repr_c: ctx.repr_c,
    };

    // Extern fn: emit import/use via template
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
            // Rust: add `mut` for params marked Var (e.g. by TCO pass)
            if fn_ctx.target == Target::Rust && fn_ctx.var_table.get(p.var).mutability == Mutability::Var {
                param_name = format!("mut {}", param_name);
            }
            let type_s = render_type_fn(&fn_ctx, &p.ty);
            fn_ctx.templates.render_with("fn_param", None, &[], &[("name", param_name.as_str()), ("type", type_s.as_str())])
                .unwrap_or_else(|| format!("{}: {}", p.name, render_type_fn(&fn_ctx, &p.ty)))
        })
        .collect::<Vec<_>>()
        .join(", ");

    // Function body: render Block contents directly (no IIFE wrapper)
    let body_str = match &func.body.kind {
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
            let body_raw = render_expr_fn(&fn_ctx, &func.body);
            let is_control = matches!(&func.body.kind, IrExprKind::Break | IrExprKind::Continue);
            if is_control {
                body_raw
            } else {
                fn_ctx.templates.render_with("block_result_expr", None, &[], &[("expr", body_raw.as_str())])
                    .unwrap_or_else(|| body_raw.clone())
            }
        }
    };
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
    fn_ctx.templates.render_with(construct, None, &[], &[("name", safe_name.as_str()), ("params", params_str.as_str()), ("return_type", ret_str.as_str()), ("body", body_str.as_str())])
        .unwrap_or_else(|| format!("fn {}() {{ }}", func.name))
}

// ── Full program rendering ──

pub fn render_program(ctx: &RenderContext, program: &IrProgram) -> String {
    // Build constructor → enum name map
    // Build type alias map for transparent expansion
    let mut type_aliases = std::collections::HashMap::new();
    for td in &program.type_decls {
        if let IrTypeDeclKind::Alias { target } = &td.kind {
            type_aliases.insert(td.name, target.clone());
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

    // Build anonymous record maps (populated by target-specific pipeline)
    ctx.ann.named_records = collect_named_records(program);
    ctx.ann.anon_records = collect_anon_records(program, &ctx.ann.named_records);

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
        parts.push(render_type_decl(&ctx, td));
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
        let rendered = ctx.templates.render_with(construct, None, &[], &[("name", name_upper.as_str()), ("type", ty_str.as_str()), ("value", val_str.as_str())])
            .unwrap_or_else(|| format!("const {} = {};", name, val_str));
        parts.push(rendered);
    }

    // Functions (non-test)
    for func in program.functions.iter().filter(|f| !f.is_test) {
        parts.push(render_function(&ctx, func));
    }

    // Test functions
    let test_fns: Vec<&IrFunction> = program.functions.iter().filter(|f| f.is_test).collect();
    if !test_fns.is_empty() {
        let test_parts: Vec<String> = test_fns.iter()
            .map(|f| render_function(&ctx, f))
            .collect();
        let tests_s = test_parts.join("\n\n");
        let wrapped = ctx.templates.render_with("test_module", None, &[], &[("tests", tests_s.as_str())])
            .unwrap_or_else(|| test_parts.join("\n\n"));
        parts.push(wrapped);
    }

    // Imported modules: render their type decls and functions
    for module in &program.modules {
        // Bundled .almd modules use minimal generic bounds (Clone only)
        // because their functions don't require PartialEq/PartialOrd/Debug.
        let is_bundled = crate::stdlib::get_bundled_source(&module.name).is_some();
        let mut mod_ctx = RenderContext {
            templates: ctx.templates,
            var_table: &module.var_table,
            indent: ctx.indent,
            target: ctx.target,
            auto_unwrap: false,
            is_test: false,
            ann: ctx.ann.clone(),
            type_aliases: ctx.type_aliases.clone(),
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
            let prefixed = rendered.replacen(
                &format!("fn {}", func.name.replace(' ', "_").replace('-', "_").replace('.', "_")),
                &format!("fn almide_rt_{}_{}", mod_ident, func.name.replace(' ', "_").replace('-', "_").replace('.', "_")),
                1
            );
            parts.push(prefixed);
        }
    }

    parts.join("\n\n")
}
