//! Type declaration rendering and anonymous record collection.

use std::collections::{HashMap, HashSet};
use crate::ir::*;
use crate::types::Ty;
use super::RenderContext;
use super::types::render_type;
use super::helpers::{template_or, ty_contains_name};

pub fn render_type_decl(ctx: &RenderContext, td: &IrTypeDecl) -> String {
    // Build generics string e.g. "<T>" or "<T, U>"
    let generics_str = if let Some(generics) = &td.generics {
        if generics.is_empty() {
            String::new()
        } else {
            let params = generics.iter().map(|g| {
                ctx.templates.render_with("generic_bound", None, &[], &[("name", g.name.as_str())])
                    .unwrap_or_else(|| g.name.clone())
            }).collect::<Vec<_>>().join(", ");
            format!("<{}>", params)
        }
    } else {
        String::new()
    };

    match &td.kind {
        IrTypeDeclKind::Record { fields } => {
            let fields_str = fields.iter()
                .map(|f| {
                    let type_s = render_type(ctx, &f.ty);
                    ctx.templates.render_with("struct_field", None, &[], &[("name", f.name.as_str()), ("type", type_s.as_str())])
                        .unwrap_or_else(|| format!("{}: {},", f.name, render_type(ctx, &f.ty)))
                })
                .collect::<Vec<_>>()
                .join("\n");
            let full_name = format!("{}{}", td.name, generics_str);
            let fallback = format!("struct {} {{ {} }}", full_name, &fields_str);
            ctx.templates.render_with("struct_decl", None, &[], &[("name", full_name.as_str()), ("fields", fields_str.as_str())])
                .unwrap_or(fallback)
        }
        IrTypeDeclKind::Variant { cases, .. } => {
            let variants_parts: Vec<String> = cases.iter()
                .map(|v| match &v.kind {
                    IrVariantKind::Unit => {
                        ctx.templates.render_with("enum_variant_unit", None, &[], &[("name", v.name.as_str())])
                            .unwrap_or_else(|| v.name.to_string())
                    }
                    IrVariantKind::Tuple { fields } => {
                        let is_recursive = ctx.ann.recursive_enums.contains(&*td.name);
                        let types: Vec<String> = fields.iter().map(|t| {
                            let rendered = render_type(ctx, t);
                            if is_recursive && ty_contains_name(t, &td.name) { format!("Box<{}>", rendered) } else { rendered }
                        }).collect();
                        let fields_str = types.join(", ");
                        // Named params via fn_param template (respects JS/TS)
                        let params_str = types.iter().enumerate()
                            .map(|(i, t)| {
                                let name = format!("v{}", i);
                                ctx.templates.render_with("fn_param", None, &[], &[("name", name.as_str()), ("type", t.as_str())])
                                    .unwrap_or(name)
                            })
                            .collect::<Vec<_>>().join(", ");
                        let param_names = (0..types.len()).map(|i| format!("v{}", i))
                            .collect::<Vec<_>>().join(", ");
                        let fallback = format!("{}({})", v.name, &fields_str);
                        ctx.templates.render_with("enum_variant", None, &[], &[("name", v.name.as_str()), ("fields", fields_str.as_str()), ("params", params_str.as_str()), ("param_names", param_names.as_str())])
                            .unwrap_or(fallback)
                    }
                    IrVariantKind::Record { fields } => {
                        let fields_str = fields.iter()
                            .map(|f| {
                                let rendered = render_type(ctx, &f.ty);
                                let boxed = if ctx.ann.recursive_enums.contains(&*td.name) && ty_contains_name(&f.ty, &td.name) {
                                    format!("Box<{}>", rendered)
                                } else {
                                    rendered
                                };
                                ctx.templates.render_with("fn_param", None, &[], &[("name", f.name.as_str()), ("type", boxed.as_str())])
                                    .unwrap_or_else(|| format!("{}: {}", f.name, boxed))
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        let field_names = fields.iter().map(|f| f.name.to_string()).collect::<Vec<_>>().join(", ");
                        ctx.templates.render_with("enum_variant_record", None, &[], &[("name", v.name.as_str()), ("fields", fields_str.as_str()), ("field_names", field_names.as_str())])
                            .unwrap_or_else(|| format!("{} {{ {} }}", v.name, fields_str))
                    }
                })
                .collect::<Vec<_>>();
            let sep = template_or(ctx, "enum_variant_sep", &[], ",\n");
            let variants_str = variants_parts.join(&sep);
            let full_name = format!("{}{}", td.name, generics_str);
            let fallback = format!("enum {} {{ {} }}", full_name, &variants_str);
            ctx.templates.render_with("enum_decl", None, &[], &[("name", full_name.as_str()), ("variants", variants_str.as_str())])
                .unwrap_or(fallback)
        }
        IrTypeDeclKind::Alias { target } => {
            let type_s = render_type(ctx, target);
            ctx.templates.render_with("type_alias", None, &[], &[("name", td.name.as_str()), ("type", type_s.as_str())])
                .unwrap_or_else(|| format!("type {} = {};", td.name, render_type(ctx, target)))
        }
    }
}

// ── Anonymous record collection ──
// Simplified version of emit_rust::lower_types logic, directly in codegen.

pub fn collect_named_records(program: &IrProgram) -> HashMap<Vec<String>, String> {
    let mut map = HashMap::new();
    for td in &program.type_decls {
        if let IrTypeDeclKind::Record { fields } = &td.kind {
            let mut names: Vec<String> = fields.iter().map(|f| f.name.to_string()).collect();
            names.sort();
            map.insert(names, td.name.to_string());
        }
    }
    // Also collect from module type declarations
    for module in &program.modules {
        for td in &module.type_decls {
            if let IrTypeDeclKind::Record { fields } = &td.kind {
                let mut names: Vec<String> = fields.iter().map(|f| f.name.to_string()).collect();
                names.sort();
                map.insert(names, td.name.to_string());
            }
        }
    }
    map
}

pub fn collect_anon_records(program: &IrProgram, named: &HashMap<Vec<String>, String>) -> HashMap<Vec<String>, String> {
    let named_set: HashSet<Vec<String>> = named.keys().cloned().collect();
    let mut seen: HashSet<Vec<String>> = HashSet::new();

    // Collect from all types AND expressions in the program
    for func in &program.functions {
        for p in &func.params { collect_anon_from_ty(&p.ty, &named_set, &mut seen); }
        collect_anon_from_ty(&func.ret_ty, &named_set, &mut seen);
        collect_anon_from_expr(&func.body, &named_set, &mut seen);
    }
    for tl in &program.top_lets {
        collect_anon_from_ty(&tl.ty, &named_set, &mut seen);
        collect_anon_from_expr(&tl.value, &named_set, &mut seen);
    }
    // Also collect from module functions and top_lets
    for module in &program.modules {
        for func in &module.functions {
            for p in &func.params { collect_anon_from_ty(&p.ty, &named_set, &mut seen); }
            collect_anon_from_ty(&func.ret_ty, &named_set, &mut seen);
            collect_anon_from_expr(&func.body, &named_set, &mut seen);
        }
        for tl in &module.top_lets {
            collect_anon_from_ty(&tl.ty, &named_set, &mut seen);
            collect_anon_from_expr(&tl.value, &named_set, &mut seen);
        }
    }

    let mut map = HashMap::new();
    let mut keys: Vec<Vec<String>> = seen.into_iter().collect();
    keys.sort();
    for (i, key) in keys.into_iter().enumerate() {
        map.insert(key, format!("AlmdRec{}", i));
    }
    map
}

fn collect_anon_from_expr(expr: &IrExpr, named: &HashSet<Vec<String>>, seen: &mut HashSet<Vec<String>>) {
    collect_anon_from_ty(&expr.ty, named, seen);
    match &expr.kind {
        IrExprKind::Block { stmts, expr: e } | IrExprKind::DoBlock { stmts, expr: e } => {
            for s in stmts { collect_anon_from_stmt(s, named, seen); }
            if let Some(e) = e { collect_anon_from_expr(e, named, seen); }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_anon_from_expr(cond, named, seen);
            collect_anon_from_expr(then, named, seen);
            collect_anon_from_expr(else_, named, seen);
        }
        IrExprKind::Match { subject, arms } => {
            collect_anon_from_expr(subject, named, seen);
            for arm in arms { collect_anon_from_expr(&arm.body, named, seen); }
        }
        IrExprKind::Call { args, target, .. } => {
            if let CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } = target {
                collect_anon_from_expr(object, named, seen);
            }
            for a in args { collect_anon_from_expr(a, named, seen); }
        }
        IrExprKind::BinOp { left, right, .. } => {
            collect_anon_from_expr(left, named, seen);
            collect_anon_from_expr(right, named, seen);
        }
        IrExprKind::UnOp { operand, .. } => collect_anon_from_expr(operand, named, seen),
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { collect_anon_from_expr(e, named, seen); }
        }
        IrExprKind::Lambda { body, .. } => collect_anon_from_expr(body, named, seen),
        IrExprKind::Record { fields, .. } | IrExprKind::SpreadRecord { fields, .. } => {
            for (_, v) in fields { collect_anon_from_expr(v, named, seen); }
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            collect_anon_from_expr(object, named, seen);
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            collect_anon_from_expr(iterable, named, seen);
            for s in body { collect_anon_from_stmt(s, named, seen); }
        }
        IrExprKind::While { cond, body } => {
            collect_anon_from_expr(cond, named, seen);
            for s in body { collect_anon_from_stmt(s, named, seen); }
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr } => {
            collect_anon_from_expr(expr, named, seen);
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr } = p { collect_anon_from_expr(expr, named, seen); }
            }
        }
        // Codegen-specific nodes
        IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } | IrExprKind::Await { expr } => {
            collect_anon_from_expr(expr, named, seen);
        }
        IrExprKind::RustMacro { args, .. } => {
            for a in args { collect_anon_from_expr(a, named, seen); }
        }
        _ => {}
    }
}

fn collect_anon_from_stmt(stmt: &IrStmt, named: &HashSet<Vec<String>>, seen: &mut HashSet<Vec<String>>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, ty, .. } => {
            collect_anon_from_ty(ty, named, seen);
            collect_anon_from_expr(value, named, seen);
        }
        IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            collect_anon_from_expr(value, named, seen);
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            collect_anon_from_expr(index, named, seen);
            collect_anon_from_expr(value, named, seen);
        }
        IrStmtKind::Guard { cond, else_ } => {
            collect_anon_from_expr(cond, named, seen);
            collect_anon_from_expr(else_, named, seen);
        }
        IrStmtKind::Expr { expr } => collect_anon_from_expr(expr, named, seen),
        _ => {}
    }
}

fn collect_anon_from_ty(ty: &Ty, named: &HashSet<Vec<String>>, seen: &mut HashSet<Vec<String>>) {
    // Record/OpenRecord: register anonymous record fields
    if let Ty::Record { fields } | Ty::OpenRecord { fields } = ty {
        let mut names: Vec<String> = fields.iter().map(|(n, _)| n.to_string()).collect();
        names.sort();
        if !named.contains(&names) { seen.insert(names); }
    }
    // Recurse into all children uniformly
    for child in ty.children() {
        collect_anon_from_ty(child, named, seen);
    }
}
