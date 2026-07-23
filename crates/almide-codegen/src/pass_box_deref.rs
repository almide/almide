//! BoxDerefPass: mark pattern-bound variables from Box'd fields for *deref.
//!
//! When a recursive enum has Box'd fields (e.g., Node(Box<Tree>, Box<Tree>)),
//! variables bound in match patterns from those fields need *deref when used.

use std::collections::{HashSet, HashMap};
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
        // Step 1: Collect deref vars and insert Deref IR nodes.
        //         Post `UnifyVarTablesPass` every VarId indexes into
        //         `program.var_table`; the module walk below reuses the
        //         same table rather than the now-empty `module.var_table`.
        let (deref_ids, recursive) = collect_deref_vars(&program);
        insert_deref_nodes(&mut program, &deref_ids);

        // Step 2: Process module-level box deref using the unified table.
        let all_type_decls: Vec<_> = program.type_decls.iter()
            .chain(program.modules.iter().flat_map(|m| m.type_decls.iter()))
            .cloned().collect();
        // Cloning the program-level table is cheap vs. re-collecting per
        // module and keeps the `&module` read-only for collect_deref_vars_module.
        let shared_vt = program.var_table.clone();
        for module in &mut program.modules {
            let mod_deref_ids = collect_module_deref_vars_with_vt(module, &shared_vt, &all_type_decls);
            insert_module_deref_nodes(module, &mod_deref_ids);
        }

        // Step 3: Populate codegen annotations
        program.codegen_annotations.recursive_enums = recursive.clone();

        // Build boxed_fields: for each recursive enum, find which variant fields
        // reference ANY cycle member (mutual recursion, not only self) (#656).
        let rec_ref = &recursive;
        program.codegen_annotations.boxed_fields = program.type_decls.iter()
            .chain(program.modules.iter().flat_map(|m| m.type_decls.iter()))
            .filter(|td| rec_ref.contains(&*td.name))
            .filter_map(|td| match &td.kind {
                IrTypeDeclKind::Variant { cases, .. } => Some(cases),
                _ => None,
            })
            .flat_map(|cases| {
                cases.iter().flat_map(move |c| {
                    match &c.kind {
                        IrVariantKind::Record { fields } => fields.iter()
                            .filter(|f| walker::ty_contains_any_recursive(&f.ty, rec_ref))
                            .map(|f| (c.name.to_string(), f.name.to_string()))
                            .collect::<Vec<_>>(),
                        IrVariantKind::Tuple { fields } => fields.iter().enumerate()
                            .filter(|(_, t)| walker::ty_contains_any_recursive(t, rec_ref))
                            .map(|(i, _)| (c.name.to_string(), format!("{}", i)))
                            .collect::<Vec<_>>(),
                        _ => vec![],
                    }
                })
            })
            .collect();

        // Build default_fields: for each variant/record constructor with default field values.
        // Chain module type_decls so types declared in submodules also fill defaults at
        // construction sites in cross-module callers.
        // Keys use BOTH bare name and module-qualified name so lookups from
        // either form succeed (same-module uses bare, cross-module uses qualified).
        let mut defaults = std::collections::HashMap::new();
        for (mod_prefix, td_iter) in std::iter::once((None, program.type_decls.iter()))
            .chain(program.modules.iter().map(|m| (Some(m.name.as_str()), m.type_decls.iter())))
        {
            for td in td_iter {
                let entries: Vec<((String, String), IrExpr)> = match &td.kind {
                    IrTypeDeclKind::Variant { cases, .. } => cases.iter()
                        .filter_map(|c| match &c.kind {
                            IrVariantKind::Record { fields } => Some(fields.iter()
                                .filter_map(|f| f.default.as_ref().map(|def| ((c.name.to_string(), f.name.to_string()), def.clone())))
                                .collect::<Vec<_>>()),
                            _ => None,
                        })
                        .flatten()
                        .collect(),
                    IrTypeDeclKind::Record { fields } => fields.iter()
                        .filter_map(|f| f.default.as_ref().map(|def| ((td.name.to_string(), f.name.to_string()), def.clone())))
                        .collect(),
                    _ => vec![],
                };
                for ((type_name, field_name), expr) in entries {
                    // Register module-qualified name first (needs clone)
                    if let Some(prefix) = mod_prefix {
                        defaults.insert((format!("{}.{}", prefix, &type_name), field_name.clone()), expr.clone());
                    }
                    // Bare name (move, no clone)
                    defaults.insert((type_name, field_name), expr);
                }
            }
        }
        program.codegen_annotations.default_fields = defaults;

        PassResult { program, changed: true }
    }
}

/// Find recursive enums from a set of type declarations.
/// Collect every Named/Variant type name appearing anywhere inside `ty`.
fn collect_type_names(ty: &Ty, out: &mut HashSet<String>) {
    let names = std::cell::RefCell::new(out);
    ty.any_child_recursive(&|t| {
        match t {
            Ty::Named(n, _) => { names.borrow_mut().insert(n.to_string()); }
            Ty::Variant { name, .. } => { names.borrow_mut().insert(name.to_string()); }
            _ => {}
        }
        false // visit every child, never short-circuit
    });
}

/// A variant type needs a Box indirection iff it participates in a recursion
/// CYCLE — it can reach itself by following variant-field type references. This
/// catches mutual recursion (`type A = A(B); type B = B(A)`), not only direct
/// self-recursion, so neither member is emitted as an infinitely-sized native
/// enum (E0072) (#656).
fn find_recursive_enums<'a>(type_decls: impl Iterator<Item = &'a IrTypeDecl>) -> HashSet<String> {
    let graph = build_type_ref_graph(type_decls);
    find_cycle_reachable_types(&graph)
}

/// Phase 1 of `find_recursive_enums`, extracted verbatim (cog>30
/// decomposition, sequential-phase pattern — phase 2 only reads this
/// phase's output, never mutates it back). Reference graph: type name →
/// names it mentions in any variant field type.
fn build_type_ref_graph<'a>(type_decls: impl Iterator<Item = &'a IrTypeDecl>) -> HashMap<String, HashSet<String>> {
    let mut graph: HashMap<String, HashSet<String>> = HashMap::new();
    for td in type_decls {
        if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
            let mut refs = HashSet::new();
            for case in cases { collect_variant_case_refs(case, &mut refs); }
            graph.insert(td.name.to_string(), refs);
        }
    }
    graph
}

/// Per-case body of `build_type_ref_graph`'s inner loop, extracted verbatim
/// (further split of the same decomposition — `refs` is a write-only
/// accumulator).
fn collect_variant_case_refs(case: &IrVariantDecl, refs: &mut HashSet<String>) {
    match &case.kind {
        IrVariantKind::Tuple { fields } => {
            for f in fields { collect_type_names(f, refs); }
        }
        IrVariantKind::Record { fields } => {
            for f in fields { collect_type_names(&f.ty, refs); }
        }
        _ => {}
    }
}

/// Phase 2 of `find_recursive_enums`, extracted verbatim (cog>30
/// decomposition) — a type is recursive iff it can reach itself through
/// ≥1 edge of the phase-1 reference graph (DFS per node).
fn find_cycle_reachable_types(graph: &HashMap<String, HashSet<String>>) -> HashSet<String> {
    let mut recursive = HashSet::new();
    for start in graph.keys().cloned().collect::<Vec<_>>() {
        let mut stack: Vec<String> = graph[&start].iter().cloned().collect();
        let mut visited: HashSet<String> = HashSet::new();
        while let Some(n) = stack.pop() {
            if n == start { recursive.insert(start.clone()); break; }
            if !visited.insert(n.clone()) { continue; }
            if let Some(next) = graph.get(&n) {
                stack.extend(next.iter().cloned());
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

/// Collect deref vars for a single module scope using the supplied
/// `VarTable`. Post-unification the supplied table is the
/// program-level one; callers pass it explicitly to avoid cloning per
/// module.
pub fn collect_module_deref_vars_with_vt(
    module: &IrModule,
    vt: &VarTable,
    all_type_decls: &[IrTypeDecl],
) -> HashSet<VarId> {
    let mut name_to_var: std::collections::HashMap<String, Vec<VarId>> = std::collections::HashMap::new();
    for i in 0..vt.len() {
        let id = VarId(i as u32);
        let info = vt.get(id);
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
                    expr: Box::new(IrExpr { kind: IrExprKind::Var { id }, ty: ty.clone(), span, def_id: None }),
                },
                ty, span, def_id: None,
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
                other @ (CallTarget::Named { .. } | CallTarget::Module { .. }) => other,
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
                lit @ IrStringPart::Lit { .. } => lit,
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
        // Any other kind: recurse into every child (total by construction).
        other => return IrExpr { kind: other, ty, span, def_id: None }
            .map_children(&mut |e| insert_derefs(e, deref_ids)),
    };

    IrExpr { kind, ty, span, def_id: None }
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
            other => return IrStmt { kind: other, span: s.span }
                .map_exprs(&mut |e| insert_derefs(e, deref_ids)),
        };
        IrStmt { kind, span: s.span }
    }).collect()
}

fn collect_from_expr(expr: &IrExpr, recursive_enums: &HashSet<String>, type_decls: &[IrTypeDecl], name_to_var: &std::collections::HashMap<String, Vec<VarId>>, deref_vars: &mut HashSet<VarId>) {
    DerefCollector { recursive_enums, type_decls, name_to_var, deref_vars }.visit_expr(expr);
}

/// Walks every expression collecting Box'd recursive-enum bindings that need a
/// Deref. Riding the exhaustive `IrVisitor` means a recursive-enum Match nested
/// inside any node kind (list/tuple/record/…) is found too — matching the now-total
/// `insert_derefs` side. Collecting an extra deref var is rustc-checked, never silent.
struct DerefCollector<'a> {
    recursive_enums: &'a HashSet<String>,
    type_decls: &'a [IrTypeDecl],
    name_to_var: &'a std::collections::HashMap<String, Vec<VarId>>,
    deref_vars: &'a mut HashSet<VarId>,
}

impl IrVisitor for DerefCollector<'_> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        if let IrExprKind::Match { subject, arms } = &expr.kind {
            // A recursive-enum match binds Box'd fields — collect their deref vars.
            let enum_name = match &subject.ty {
                Ty::Named(n, _) => Some(n.clone()),
                Ty::Variant { name, .. } => Some(name.clone()),
                _ => None,
            };
            if let Some(ref ename) = enum_name {
                if self.recursive_enums.contains(ename.as_str()) {
                    for arm in arms {
                        collect_deref_from_pattern(&arm.pattern, self.recursive_enums, self.type_decls, self.name_to_var, self.deref_vars);
                    }
                }
            }
        }
        walk_expr(self, expr); // recurse subject, arm bodies, and every other kind
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

/// Find the variant type-decl that DECLARES constructor `ctor` — lets a nested
/// constructor resolve to its OWN enum (which may differ from the outer one under
/// mutual recursion), so each field's box-ness is judged against the right decl.
fn find_td_for_ctor<'a>(type_decls: &'a [IrTypeDecl], ctor: &str) -> Option<&'a IrTypeDecl> {
    type_decls.iter().find(|td| matches!(&td.kind,
        IrTypeDeclKind::Variant { cases, .. } if cases.iter().any(|c| c.name == ctor)))
}

fn collect_deref_from_pattern(pattern: &IrPattern, recursive: &HashSet<String>, type_decls: &[IrTypeDecl], name_to_var: &std::collections::HashMap<String, Vec<VarId>>, deref_vars: &mut HashSet<VarId>) {
    match pattern {
        IrPattern::Constructor { name, args } => {
            let td = find_td_for_ctor(type_decls, name.as_str());
            let Some(IrVariantKind::Tuple { fields }) = find_variant_case(td, name) else { return };
            // A field bound here is Box'd iff it references ANY cycle member, so
            // the deref must use the SAME predicate as the Box-rendering site (#656).
            args.iter().enumerate()
                .filter(|(i, _)| fields.get(*i).map_or(false, |ft| walker::ty_contains_any_recursive(ft, recursive)))
                .for_each(|(_, arg)| mark_boxed_field_pattern(arg, recursive, type_decls, name_to_var, deref_vars));
        }
        IrPattern::RecordPattern { name, fields, .. } => {
            let td = find_td_for_ctor(type_decls, name.as_str());
            let Some(IrVariantKind::Record { fields: case_fields }) = find_variant_case(td, name) else { return };
            for field_pat in fields {
                let is_recursive = case_fields.iter()
                    .find(|f| f.name == field_pat.name)
                    .map_or(false, |f| walker::ty_contains_any_recursive(&f.ty, recursive));
                if !is_recursive { continue; }

                if let Some(ref p) = field_pat.pattern {
                    mark_boxed_field_pattern(p, recursive, type_decls, name_to_var, deref_vars);
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

/// A pattern occupying a BOXED field position. A var bound here aliases the box
/// itself, so it derefs on use. But a NESTED constructor/record matches THROUGH
/// the box — its OWN inner fields must be re-gated by their declared types, never
/// blanket-marked: `Node(Leaf(a), _)`'s `a: Int` is not boxed and must not get a
/// spurious `*a` (the #610 E0614). Recursing via `collect_deref_from_pattern`
/// re-applies the box predicate at each level (and re-resolves the enum decl).
fn mark_boxed_field_pattern(pat: &IrPattern, recursive: &HashSet<String>, type_decls: &[IrTypeDecl], name_to_var: &std::collections::HashMap<String, Vec<VarId>>, deref_vars: &mut HashSet<VarId>) {
    match pat {
        IrPattern::Bind { var, .. } => { deref_vars.insert(*var); }
        IrPattern::Constructor { .. } | IrPattern::RecordPattern { .. } =>
            collect_deref_from_pattern(pat, recursive, type_decls, name_to_var, deref_vars),
        _ => {}
    }
}
