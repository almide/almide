//! BoxDerefPass: mark pattern-bound variables from Box'd fields for *deref.
//!
//! When a recursive enum has Box'd fields (e.g., Node(Box<Tree>, Box<Tree>)),
//! variables bound in match patterns from those fields need *deref when used.

use std::collections::HashSet;
use almide_ir::*;
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Target};
use super::walker;

#[derive(Debug)]
pub struct BoxDerefPass;

impl NanoPass for BoxDerefPass {
    fn name(&self) -> &str { "BoxDeref" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        // Step 1: Collect deref vars and insert Deref IR nodes
        let (deref_ids, recursive) = collect_deref_vars(&program);
        insert_deref_nodes(&mut program, &deref_ids);

        // Step 2: Process module-level box deref (separate VarId namespace per module)
        let all_type_decls: Vec<_> = program.type_decls.iter()
            .chain(program.modules.iter().flat_map(|m| m.type_decls.iter()))
            .cloned().collect();
        for module in &mut program.modules {
            let mod_deref_ids = collect_module_deref_vars(module, &all_type_decls);
            insert_module_deref_nodes(module, &mod_deref_ids);
        }

        // Step 3: Populate codegen annotations
        program.codegen_annotations.recursive_enums = recursive.clone();

        // Build boxed_fields: for each recursive enum, find which variant fields reference the enum
        program.codegen_annotations.boxed_fields = program.type_decls.iter()
            .chain(program.modules.iter().flat_map(|m| m.type_decls.iter()))
            .filter(|td| recursive.contains(&*td.name))
            .filter_map(|td| match &td.kind {
                IrTypeDeclKind::Variant { cases, .. } => Some((td, cases)),
                _ => None,
            })
            .flat_map(|(td, cases)| {
                cases.iter().flat_map(move |c| {
                    let name = &td.name;
                    match &c.kind {
                        IrVariantKind::Record { fields } => fields.iter()
                            .filter(|f| walker::ty_contains_name(&f.ty, name))
                            .map(|f| (c.name.to_string(), f.name.to_string()))
                            .collect::<Vec<_>>(),
                        IrVariantKind::Tuple { fields } => fields.iter().enumerate()
                            .filter(|(_, t)| walker::ty_contains_name(t, name))
                            .map(|(i, _)| (c.name.to_string(), format!("{}", i)))
                            .collect::<Vec<_>>(),
                        _ => vec![],
                    }
                })
            })
            .collect();

        // Build default_fields: for each variant/record constructor with default field values
        program.codegen_annotations.default_fields = program.type_decls.iter()
            .flat_map(|td| match &td.kind {
                IrTypeDeclKind::Variant { cases, .. } => cases.iter()
                    .filter_map(|c| match &c.kind {
                        IrVariantKind::Record { fields } => Some(fields.iter()
                            .filter_map(|f| f.default.as_ref().map(|def| ((c.name.to_string(), f.name.to_string()), def.clone())))
                            .collect::<Vec<_>>()),
                        _ => None,
                    })
                    .flatten()
                    .collect::<Vec<_>>(),
                IrTypeDeclKind::Record { fields } => fields.iter()
                    .filter_map(|f| f.default.as_ref().map(|def| ((td.name.to_string(), f.name.to_string()), def.clone())))
                    .collect(),
                _ => vec![],
            })
            .collect();

        PassResult { program, changed: true }
    }
}

/// Find recursive enums from a set of type declarations.
fn find_recursive_enums<'a>(type_decls: impl Iterator<Item = &'a IrTypeDecl>) -> HashSet<String> {
    let mut recursive = HashSet::new();
    for td in type_decls {
        if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
            let is_recursive = cases.iter().any(|case| match &case.kind {
                IrVariantKind::Tuple { fields } => fields.iter().any(|f| ty_contains_name(f, &td.name)),
                IrVariantKind::Record { fields } => fields.iter().any(|f| ty_contains_name(&f.ty, &td.name)),
                _ => false,
            });
            if is_recursive {
                recursive.insert(td.name.to_string());
            }
        }
    }
    recursive
}

/// Find which enums have recursive variants and collect
/// VarIds that are bound from Box'd fields in match patterns.
pub fn collect_deref_vars(program: &IrProgram) -> (HashSet<VarId>, HashSet<String>) {
    // Build name → VarId reverse map for shorthand record pattern lookup
    let mut name_to_var: std::collections::HashMap<String, Vec<VarId>> = std::collections::HashMap::new();
    for i in 0..program.var_table.len() {
        let id = VarId(i as u32);
        let info = program.var_table.get(id);
        name_to_var.entry(info.name.to_string()).or_default().push(id);
    }
    let mut deref_vars = HashSet::new();

    // Step 1: Find recursive enums (type declarations that reference themselves)
    let recursive_enums = find_recursive_enums(
        program.type_decls.iter().chain(program.modules.iter().flat_map(|m| m.type_decls.iter()))
    );

    // Step 2: Walk all match expressions and find Bind vars in recursive positions
    for func in &program.functions {
        collect_from_expr(&func.body, &recursive_enums, &program.type_decls, &name_to_var, &mut deref_vars);
    }

    (deref_vars, recursive_enums)
}

/// Insert Deref IR nodes for box'd pattern variables
pub fn insert_deref_nodes(program: &mut IrProgram, deref_ids: &HashSet<VarId>) {
    if deref_ids.is_empty() { return; }
    for func in &mut program.functions {
        func.body = insert_derefs(std::mem::take(&mut func.body), deref_ids);
    }
    for tl in &mut program.top_lets {
        tl.value = insert_derefs(std::mem::take(&mut tl.value), deref_ids);
    }
}

/// Collect deref vars for a single module scope (separate VarId namespace).
pub fn collect_module_deref_vars(module: &IrModule, all_type_decls: &[IrTypeDecl]) -> HashSet<VarId> {
    let mut name_to_var: std::collections::HashMap<String, Vec<VarId>> = std::collections::HashMap::new();
    for i in 0..module.var_table.len() {
        let id = VarId(i as u32);
        let info = module.var_table.get(id);
        name_to_var.entry(info.name.to_string()).or_default().push(id);
    }
    let mut deref_vars = HashSet::new();
    let recursive_enums = find_recursive_enums(all_type_decls.iter());
    for func in &module.functions {
        collect_from_expr(&func.body, &recursive_enums, all_type_decls, &name_to_var, &mut deref_vars);
    }
    deref_vars
}

/// Insert Deref IR nodes for a single module's functions and top_lets.
pub fn insert_module_deref_nodes(module: &mut IrModule, deref_ids: &HashSet<VarId>) {
    if deref_ids.is_empty() { return; }
    for func in &mut module.functions {
        func.body = insert_derefs(std::mem::take(&mut func.body), deref_ids);
    }
    for tl in &mut module.top_lets {
        tl.value = insert_derefs(std::mem::take(&mut tl.value), deref_ids);
    }
}

fn insert_derefs(expr: IrExpr, deref_ids: &HashSet<VarId>) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        IrExprKind::Var { id } if deref_ids.contains(&id) => {
            return IrExpr {
                kind: IrExprKind::Deref {
                    expr: Box::new(IrExpr { kind: IrExprKind::Var { id }, ty: ty.clone(), span }),
                },
                ty, span,
            };
        }
        // Recurse
        IrExprKind::Call { target, args, type_args } => {
            let args = args.into_iter().map(|a| insert_derefs(a, deref_ids)).collect();
            let target = match target {
                CallTarget::Method { object, method } => CallTarget::Method {
                    object: Box::new(insert_derefs(*object, deref_ids)), method,
                },
                CallTarget::Computed { callee } => CallTarget::Computed {
                    callee: Box::new(insert_derefs(*callee, deref_ids)),
                },
                other => other,
            };
            IrExprKind::Call { target, args, type_args }
        }
        IrExprKind::RuntimeCall { symbol, args } => IrExprKind::RuntimeCall {
            symbol,
            args: args.into_iter().map(|a| insert_derefs(a, deref_ids)).collect(),
        },
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(insert_derefs(*cond, deref_ids)),
            then: Box::new(insert_derefs(*then, deref_ids)),
            else_: Box::new(insert_derefs(*else_, deref_ids)),
        },
        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: insert_deref_stmts(stmts, deref_ids),
            expr: expr.map(|e| Box::new(insert_derefs(*e, deref_ids))),
        },

        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(insert_derefs(*subject, deref_ids)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(|g| insert_derefs(g, deref_ids)),
                body: insert_derefs(arm.body, deref_ids),
            }).collect(),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op, left: Box::new(insert_derefs(*left, deref_ids)), right: Box::new(insert_derefs(*right, deref_ids)),
        },
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params, body: Box::new(insert_derefs(*body, deref_ids)), lambda_id,
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| insert_derefs(e, deref_ids)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: fields.into_iter().map(|(k, v)| (k, insert_derefs(v, deref_ids))).collect(),
        },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(insert_derefs(*object, deref_ids)), field,
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple, iterable: Box::new(insert_derefs(*iterable, deref_ids)),
            body: insert_deref_stmts(body, deref_ids),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: insert_derefs(expr, deref_ids) },
                other => other,
            }).collect(),
        },
        IrExprKind::Unwrap { expr } => IrExprKind::Unwrap { expr: Box::new(insert_derefs(*expr, deref_ids)) },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(insert_derefs(*expr, deref_ids)) },
        IrExprKind::UnwrapOr { expr, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(insert_derefs(*expr, deref_ids)),
            fallback: Box::new(insert_derefs(*fallback, deref_ids)),
        },
        IrExprKind::ToOption { expr } => IrExprKind::ToOption { expr: Box::new(insert_derefs(*expr, deref_ids)) },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(insert_derefs(*expr, deref_ids)) },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(insert_derefs(*expr, deref_ids)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(insert_derefs(*expr, deref_ids)) },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(|e| insert_derefs(e, deref_ids)).collect(),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(insert_derefs(*operand, deref_ids)),
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| insert_derefs(e, deref_ids)).collect(),
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(insert_derefs(*object, deref_ids)),
            index: Box::new(insert_derefs(*index, deref_ids)),
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(insert_derefs(*start, deref_ids)),
            end: Box::new(insert_derefs(*end, deref_ids)),
            inclusive,
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(insert_derefs(*cond, deref_ids)),
            body: insert_deref_stmts(body, deref_ids),
        },
        other => other,
    };

    IrExpr { kind, ty, span }
}

fn insert_deref_stmts(stmts: Vec<IrStmt>, deref_ids: &HashSet<VarId>) -> Vec<IrStmt> {
    stmts.into_iter().map(|s| {
        let kind = match s.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
                var, mutability, ty, value: insert_derefs(value, deref_ids),
            },
            IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: insert_derefs(value, deref_ids) },
            IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: insert_derefs(expr, deref_ids) },
            IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
                cond: insert_derefs(cond, deref_ids), else_: insert_derefs(else_, deref_ids),
            },
            other => other,
        };
        IrStmt { kind, span: s.span }
    }).collect()
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
                if recursive_enums.contains(ename.as_str()) {
                    // Find the type decl to know which fields are recursive
                    let td = type_decls.iter().find(|td| *ename == td.name);
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
        IrExprKind::Block { stmts, expr: e } => {
            for s in stmts { collect_from_stmt(s, recursive_enums, type_decls, name_to_var, deref_vars); }
            if let Some(e) = e { collect_from_expr(e, recursive_enums, type_decls, name_to_var, deref_vars); }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_from_expr(cond, recursive_enums, type_decls, name_to_var, deref_vars);
            collect_from_expr(then, recursive_enums, type_decls, name_to_var, deref_vars);
            collect_from_expr(else_, recursive_enums, type_decls, name_to_var, deref_vars);
        }
        IrExprKind::Call { args, .. }
        | IrExprKind::RuntimeCall { args, .. } => {
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
/// Look up a variant case from a type decl by constructor name.
fn find_variant_case<'a>(td: Option<&'a IrTypeDecl>, ctor_name: &str) -> Option<&'a IrVariantKind> {
    let td = td?;
    let cases = match &td.kind {
        IrTypeDeclKind::Variant { cases, .. } => cases,
        _ => return None,
    };
    cases.iter().find(|c| c.name == ctor_name).map(|c| &c.kind)
}

fn collect_deref_from_pattern(pattern: &IrPattern, enum_name: &str, td: Option<&IrTypeDecl>, name_to_var: &std::collections::HashMap<String, Vec<VarId>>, deref_vars: &mut HashSet<VarId>) {
    match pattern {
        IrPattern::Constructor { name, args } => {
            let Some(IrVariantKind::Tuple { fields }) = find_variant_case(td, name) else { return };
            args.iter().enumerate()
                .filter(|(i, _)| fields.get(*i).map_or(false, |ft| ty_contains_name(ft, enum_name)))
                .for_each(|(_, arg)| collect_bind_vars(arg, deref_vars));
        }
        IrPattern::RecordPattern { name, fields, .. } => {
            let Some(IrVariantKind::Record { fields: case_fields }) = find_variant_case(td, name) else { return };
            for field_pat in fields {
                let is_recursive = case_fields.iter()
                    .find(|f| f.name == field_pat.name)
                    .map_or(false, |f| ty_contains_name(&f.ty, enum_name));
                if !is_recursive { continue; }

                if let Some(ref p) = field_pat.pattern {
                    collect_bind_vars(p, deref_vars);
                } else if let Some(var_ids) = name_to_var.get(&field_pat.name) {
                    // Shorthand: lookup VarId by name, only if unambiguous
                    if var_ids.len() == 1 {
                        deref_vars.insert(var_ids[0]);
                    }
                }
            }
        }
        _ => {}
    }
}

fn collect_bind_vars(pattern: &IrPattern, deref_vars: &mut HashSet<VarId>) {
    match pattern {
        IrPattern::Bind { var, .. } => { deref_vars.insert(*var); }
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
        Ty::Applied(_, args) => args.iter().any(|a| ty_contains_name(a, name)),
        Ty::Tuple(elems) => elems.iter().any(|e| ty_contains_name(e, name)),
        _ => false,
    }
}
