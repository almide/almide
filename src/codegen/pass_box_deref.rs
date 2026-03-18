//! BoxDerefPass: mark pattern-bound variables from Box'd fields for *deref.
//!
//! When a recursive enum has Box'd fields (e.g., Node(Box<Tree>, Box<Tree>)),
//! variables bound in match patterns from those fields need *deref when used.

use std::collections::HashSet;
use crate::ir::*;
use crate::types::Ty;
use super::annotations::CodegenAnnotations;
use super::pass::{NanoPass, Target};

#[derive(Debug)]
pub struct BoxDerefPass;

impl NanoPass for BoxDerefPass {
    fn name(&self) -> &str { "BoxDeref" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }

    fn run(&self, program: &mut IrProgram, _target: Target) {
        // Populates annotations via collect_deref_vars()
    }
}

/// Find which enums have recursive variants and collect
/// VarIds that are bound from Box'd fields in match patterns.
pub fn collect_deref_vars(program: &IrProgram) -> (HashSet<VarId>, HashSet<String>) {
    // Build name → VarId reverse map for shorthand record pattern lookup
    let mut name_to_var: std::collections::HashMap<String, Vec<VarId>> = std::collections::HashMap::new();
    for i in 0..program.var_table.len() {
        let id = VarId(i as u32);
        let info = program.var_table.get(id);
        name_to_var.entry(info.name.clone()).or_default().push(id);
    }
    let mut deref_vars = HashSet::new();
    let mut recursive_enums = HashSet::new();

    // Step 1: Find recursive enums
    for td in &program.type_decls {
        if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
            for case in cases {
                match &case.kind {
                    IrVariantKind::Tuple { fields } => {
                        for f in fields {
                            if ty_contains_name(f, &td.name) {
                                recursive_enums.insert(td.name.clone());
                            }
                        }
                    }
                    IrVariantKind::Record { fields } => {
                        for f in fields {
                            if ty_contains_name(&f.ty, &td.name) {
                                recursive_enums.insert(td.name.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Step 2: Walk all match expressions and find Bind vars in recursive positions
    for func in &program.functions {
        collect_from_expr(&func.body, &recursive_enums, &program.type_decls, &name_to_var, &mut deref_vars);
    }

    (deref_vars, recursive_enums)
}

fn collect_from_expr(expr: &IrExpr, recursive_enums: &HashSet<String>, type_decls: &[IrTypeDecl], name_to_var: &std::collections::HashMap<String, Vec<VarId>>, deref_vars: &mut HashSet<VarId>) {
    match &expr.kind {
        IrExprKind::Match { subject, arms } => {
            // Check if subject type is a recursive enum
            let enum_name = match &subject.ty {
                Ty::Named(n, _) => Some(n.clone()),
                Ty::Variant { name, .. } => Some(name.clone()),
                _ => None,
            };

            if let Some(ref ename) = enum_name {
                if recursive_enums.contains(ename) {
                    // Find the type decl to know which fields are recursive
                    let td = type_decls.iter().find(|td| &td.name == ename);
                    for arm in arms {
                        collect_deref_from_pattern(&arm.pattern, ename, td, name_to_var, deref_vars);
                        collect_from_expr(&arm.body, recursive_enums, type_decls, name_to_var, deref_vars);
                    }
                } else {
                    for arm in arms {
                        collect_from_expr(&arm.body, recursive_enums, type_decls, name_to_var, deref_vars);
                    }
                }
            } else {
                for arm in arms {
                    collect_from_expr(&arm.body, recursive_enums, type_decls, name_to_var, deref_vars);
                }
            }
            collect_from_expr(subject, recursive_enums, type_decls, name_to_var, deref_vars);
        }
        IrExprKind::Block { stmts, expr: e } | IrExprKind::DoBlock { stmts, expr: e } => {
            for s in stmts { collect_from_stmt(s, recursive_enums, type_decls, name_to_var, deref_vars); }
            if let Some(e) = e { collect_from_expr(e, recursive_enums, type_decls, name_to_var, deref_vars); }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_from_expr(cond, recursive_enums, type_decls, name_to_var, deref_vars);
            collect_from_expr(then, recursive_enums, type_decls, name_to_var, deref_vars);
            collect_from_expr(else_, recursive_enums, type_decls, name_to_var, deref_vars);
        }
        IrExprKind::Call { args, .. } => {
            for a in args { collect_from_expr(a, recursive_enums, type_decls, name_to_var, deref_vars); }
        }
        IrExprKind::BinOp { left, right, .. } => {
            collect_from_expr(left, recursive_enums, type_decls, name_to_var, deref_vars);
            collect_from_expr(right, recursive_enums, type_decls, name_to_var, deref_vars);
        }
        IrExprKind::Lambda { body, .. } => collect_from_expr(body, recursive_enums, type_decls, name_to_var, deref_vars),
        _ => {}
    }
}

fn collect_from_stmt(stmt: &IrStmt, recursive_enums: &HashSet<String>, type_decls: &[IrTypeDecl], name_to_var: &std::collections::HashMap<String, Vec<VarId>>, deref_vars: &mut HashSet<VarId>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => {
            collect_from_expr(value, recursive_enums, type_decls, name_to_var, deref_vars);
        }
        IrStmtKind::Expr { expr } => collect_from_expr(expr, recursive_enums, type_decls, name_to_var, deref_vars),
        _ => {}
    }
}

/// Given a pattern matching a recursive enum, find Bind vars in recursive positions.
fn collect_deref_from_pattern(pattern: &IrPattern, enum_name: &str, td: Option<&IrTypeDecl>, name_to_var: &std::collections::HashMap<String, Vec<VarId>>, deref_vars: &mut HashSet<VarId>) {
    match pattern {
        IrPattern::Constructor { name, args } => {
            // Find the variant in the type decl
            if let Some(td) = td {
                if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
                    if let Some(case) = cases.iter().find(|c| &c.name == name) {
                        if let IrVariantKind::Tuple { fields } = &case.kind {
                            for (i, arg) in args.iter().enumerate() {
                                if let Some(field_ty) = fields.get(i) {
                                    if ty_contains_name(field_ty, enum_name) {
                                        // This field is Box'd — any Bind in this pattern position needs deref
                                        collect_bind_vars(arg, deref_vars);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        IrPattern::RecordPattern { name, fields, .. } => {
            if let Some(td) = td {
                if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
                    if let Some(case) = cases.iter().find(|c| &c.name == name) {
                        if let IrVariantKind::Record { fields: case_fields } = &case.kind {
                            for field_pat in fields {
                                if let Some(case_field) = case_fields.iter().find(|f| f.name == field_pat.name) {
                                    if ty_contains_name(&case_field.ty, enum_name) {
                                        if let Some(ref p) = field_pat.pattern {
                                            collect_bind_vars(p, deref_vars);
                                        }
                                        // Shorthand: skip global name lookup — too imprecise.
                                        // The walker handles Box deref for shorthand record patterns
                                        // by inserting *deref at the pattern site if needed.
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn collect_bind_vars(pattern: &IrPattern, deref_vars: &mut HashSet<VarId>) {
    match pattern {
        IrPattern::Bind { var } => { deref_vars.insert(*var); }
        IrPattern::Constructor { args, .. } => {
            for a in args { collect_bind_vars(a, deref_vars); }
        }
        _ => {}
    }
}

fn ty_contains_name(ty: &Ty, name: &str) -> bool {
    match ty {
        Ty::Named(n, args) => n == name || args.iter().any(|a| ty_contains_name(a, name)),
        Ty::Variant { name: vn, .. } => vn == name,
        Ty::List(inner) | Ty::Option(inner) => ty_contains_name(inner, name),
        Ty::Result(a, b) | Ty::Map(a, b) => ty_contains_name(a, name) || ty_contains_name(b, name),
        Ty::Tuple(elems) => elems.iter().any(|e| ty_contains_name(e, name)),
        _ => false,
    }
}
