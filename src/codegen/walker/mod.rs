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
    pub ann: CodegenAnnotations,
}

impl<'a> RenderContext<'a> {
    pub fn new(templates: &'a TemplateSet, var_table: &'a VarTable) -> Self {
        Self { templates, var_table, indent: 0, target: Target::Rust, auto_unwrap: false, ann: CodegenAnnotations::default() }
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
        let kw = ["default", "switch", "case", "class", "new", "delete",
            "typeof", "void", "with", "yield", "export", "import",
            "try", "catch", "finally", "throw", "eval", "arguments"];
        if kw.contains(&name.as_str()) {
            self.templates.render_with("keyword_escape", None, &[], &[("name", name.as_str())])
                .unwrap_or_else(|| name.clone())
        } else {
            name.clone()
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
        ann: ctx.ann.clone(),
    };

    // Extern fn: emit import/use via template
    if !func.extern_attrs.is_empty() {
        let target_str = match ctx.target {
            Target::Rust => "rs",
            Target::TypeScript | Target::JavaScript => "ts",
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
            let mut param_name = p.name.clone();
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

    // Function body: render Block/DoBlock contents directly (no IIFE wrapper)
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
            let params = generics.iter().map(|g| {
                ctx.templates.render_with("generic_bound_full", None, &[], &[("name", g.name.as_str())])
                    .unwrap_or_else(|| g.name.clone())
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
        func.name.clone()
    };
    let mut safe_name = raw_name.replace(' ', "_").replace('-', "_").replace('.', "_")
        .replace('+', "_plus_").replace('/', "_div_").replace('*', "_mul_")
        .replace('(', "").replace(')', "").replace(',', "_").replace(':', "_")
        .replace('=', "_eq_").replace('!', "_bang_").replace('?', "_q_")
        .replace('<', "_lt_").replace('>', "_gt_").replace('[', "_").replace(']', "_")
        .replace('|', "_pipe_").replace('&', "_amp_").replace('%', "_mod_");
    // Escape target-specific keywords via template
    let target_keywords = ["while", "for", "if", "else", "match", "loop", "break", "continue",
        "return", "fn", "let", "mut", "use", "mod", "pub", "struct", "enum", "impl", "trait",
        "type", "where", "as", "in", "ref", "self", "super", "crate", "const", "static",
        "unsafe", "async", "await", "dyn", "move", "true", "false",
        "default", "switch", "case", "class", "extends", "new", "delete", "typeof",
        "void", "with", "yield", "export", "import", "try", "catch", "finally", "throw",
        "eval", "arguments"];
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
    let mut ctx = RenderContext {
        templates: ctx.templates,
        var_table: ctx.var_table,
        indent: ctx.indent,
        target: ctx.target,
        auto_unwrap: ctx.auto_unwrap,
        ann: ctx.ann.clone(),
    };
    for td in &program.type_decls {
        if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
            for c in cases {
                ctx.ann.ctor_to_enum.insert(c.name.clone(), td.name.clone());
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
                        .unwrap_or_else(|| format!("{}: T{}", name, i))
                })
                .collect();
            let fields_str = fields.join("\n");
            let full_name = format!("{}<{}>", struct_name, generics.join(", "));
            parts.push(ctx.templates.render_with("struct_decl", None, &[], &[("name", full_name.as_str()), ("fields", fields_str.as_str())])
                .unwrap_or_else(|| format!("struct {} {{ {} }}", struct_name, fields_str)));
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
        let mod_ctx = RenderContext {
            templates: ctx.templates,
            var_table: &module.var_table,
            indent: ctx.indent,
            target: ctx.target,
            auto_unwrap: false,
            ann: ctx.ann.clone(),
        };
        // Module type decls
        for td in &module.type_decls {
            parts.push(render_type_decl(&mod_ctx, td));
        }
        // Module functions (prefixed with module name)
        for func in &module.functions {
            let rendered = render_function(&mod_ctx, func);
            // Rename: fn name → fn modulename_name (to match almide_rt_module_func naming)
            let prefixed = rendered.replacen(
                &format!("fn {}", func.name.replace(' ', "_").replace('-', "_").replace('.', "_")),
                &format!("fn almide_rt_{}_{}", module.name, func.name.replace(' ', "_").replace('-', "_").replace('.', "_")),
                1
            );
            parts.push(prefixed);
        }
    }

    parts.join("\n\n")
}
