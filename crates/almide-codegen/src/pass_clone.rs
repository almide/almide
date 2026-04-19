//! ClonePass: insert Clone IR nodes for heap-type variables in Rust.
//!
//! **Last-use optimization**: tracks remaining uses per variable.
//! At the final use of a variable, ownership is transferred (move) instead of cloning.
//! Inside loops, clones are always inserted (the body executes multiple times).
//! At branches (if/match), remaining counts are merged conservatively (min).

use std::collections::{HashSet, HashMap};
use almide_ir::*;
use almide_base::Span;
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct CloneInsertionPass;

impl NanoPass for CloneInsertionPass {
    fn name(&self) -> &str { "CloneInsertion" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }

    fn depends_on(&self) -> Vec<&'static str> { vec!["BorrowInsertion"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        compute_use_counts(&mut program);
        let top_let_vars: HashSet<VarId> = program.top_lets.iter().map(|tl| tl.var).collect();

        // Compute syntactic counts (no loop/lambda bumps) for remaining tracking
        let syntactic = compute_syntactic_counts_program(&program);

        let (always, eligible) = split_clone_ids(&program.var_table, &top_let_vars, &syntactic);
        let mut remaining = build_remaining(&eligible, &syntactic);

        for func in &mut program.functions {
            // Reset remaining for each function (vars are function-scoped)
            reset_remaining(&mut remaining, &eligible, &syntactic);
            func.body = insert_clones_live(std::mem::take(&mut func.body), &always, &eligible, &mut remaining, false);
        }
        for tl in &mut program.top_lets {
            reset_remaining(&mut remaining, &eligible, &syntactic);
            tl.value = insert_clones_live(std::mem::take(&mut tl.value), &always, &eligible, &mut remaining, false);
        }

        let IrProgram { modules, var_table, .. } = &mut program;
        for module in modules.iter_mut() {
            let module_top_lets: HashSet<VarId> = module.top_lets.iter().map(|tl| tl.var).collect();
            let module_syntactic = compute_syntactic_counts_module(module);
            let (m_always, m_eligible) = split_clone_ids(var_table, &module_top_lets, &module_syntactic);
            let mut m_remaining = build_remaining(&m_eligible, &module_syntactic);

            for func in module.functions.iter_mut() {
                reset_remaining(&mut m_remaining, &m_eligible, &module_syntactic);
                func.body = insert_clones_live(std::mem::take(&mut func.body), &m_always, &m_eligible, &mut m_remaining, false);
            }
            for tl in module.top_lets.iter_mut() {
                reset_remaining(&mut m_remaining, &m_eligible, &module_syntactic);
                tl.value = insert_clones_live(std::mem::take(&mut tl.value), &m_always, &m_eligible, &mut m_remaining, false);
            }
        }
        PassResult { program, changed: true }
    }
}

// ── Syntactic use-count (no loop/lambda bumps) ─────────────────────

fn compute_syntactic_counts_program(program: &IrProgram) -> HashMap<VarId, u32> {
    let mut counts = HashMap::new();
    for func in &program.functions {
        count_syntactic(&func.body, &mut counts);
    }
    for tl in &program.top_lets {
        count_syntactic(&tl.value, &mut counts);
    }
    counts
}

fn compute_syntactic_counts_module(module: &IrModule) -> HashMap<VarId, u32> {
    let mut counts = HashMap::new();
    for func in &module.functions {
        count_syntactic(&func.body, &mut counts);
    }
    for tl in &module.top_lets {
        count_syntactic(&tl.value, &mut counts);
    }
    counts
}

fn count_syntactic(expr: &IrExpr, counts: &mut HashMap<VarId, u32>) {
    match &expr.kind {
        IrExprKind::Var { id } => { *counts.entry(*id).or_insert(0) += 1; }
        IrExprKind::BinOp { left, right, .. } => {
            count_syntactic(left, counts); count_syntactic(right, counts);
        }
        IrExprKind::UnOp { operand, .. } => count_syntactic(operand, counts),
        IrExprKind::If { cond, then, else_ } => {
            count_syntactic(cond, counts); count_syntactic(then, counts); count_syntactic(else_, counts);
        }
        IrExprKind::Match { subject, arms } => {
            count_syntactic(subject, counts);
            for arm in arms {
                if let Some(g) = &arm.guard { count_syntactic(g, counts); }
                count_syntactic(&arm.body, counts);
            }
        }
        IrExprKind::Block { stmts, expr } => {
            for s in stmts { count_syntactic_stmt(s, counts); }
            if let Some(e) = expr { count_syntactic(e, counts); }
        }
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Method { object, .. } => count_syntactic(object, counts),
                CallTarget::Computed { callee } => count_syntactic(callee, counts),
                _ => {}
            }
            for a in args { count_syntactic(a, counts); }
        }
        IrExprKind::RuntimeCall { args, .. } => {
            for a in args { count_syntactic(a, counts); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements }
        | IrExprKind::Fan { exprs: elements } => {
            for e in elements { count_syntactic(e, counts); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, e) in fields { count_syntactic(e, counts); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            count_syntactic(base, counts);
            for (_, e) in fields { count_syntactic(e, counts); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries { count_syntactic(k, counts); count_syntactic(v, counts); }
        }
        IrExprKind::Range { start, end, .. } => {
            count_syntactic(start, counts); count_syntactic(end, counts);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => count_syntactic(object, counts),
        IrExprKind::IndexAccess { object, index } => {
            count_syntactic(object, counts); count_syntactic(index, counts);
        }
        IrExprKind::MapAccess { object, key } => {
            count_syntactic(object, counts); count_syntactic(key, counts);
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            count_syntactic(iterable, counts);
            for s in body { count_syntactic_stmt(s, counts); }
        }
        IrExprKind::While { cond, body } => {
            count_syntactic(cond, counts);
            for s in body { count_syntactic_stmt(s, counts); }
        }
        IrExprKind::Lambda { body, .. } => count_syntactic(body, counts),
        IrExprKind::StringInterp { parts } => {
            for p in parts { if let IrStringPart::Expr { expr } = p { count_syntactic(expr, counts); } }
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Unwrap { expr } | IrExprKind::ToOption { expr }
        | IrExprKind::Await { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } => count_syntactic(expr, counts),
        IrExprKind::UnwrapOr { expr, fallback } => {
            count_syntactic(expr, counts); count_syntactic(fallback, counts);
        }
        IrExprKind::RustMacro { args, .. } => {
            for a in args { count_syntactic(a, counts); }
        }
        _ => {}
    }
}

fn count_syntactic_stmt(stmt: &IrStmt, counts: &mut HashMap<VarId, u32>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } => count_syntactic(value, counts),
        IrStmtKind::IndexAssign { index, value, .. } => {
            count_syntactic(index, counts); count_syntactic(value, counts);
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            count_syntactic(key, counts); count_syntactic(value, counts);
        }
        IrStmtKind::FieldAssign { value, .. } => count_syntactic(value, counts),
        IrStmtKind::ListSwap { a, b, .. } => {
            count_syntactic(a, counts); count_syntactic(b, counts);
        }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
            count_syntactic(end, counts);
        }
        IrStmtKind::ListCopySlice { len, .. } => count_syntactic(len, counts),
        IrStmtKind::Expr { expr } => count_syntactic(expr, counts),
        IrStmtKind::Guard { cond, else_ } => {
            count_syntactic(cond, counts); count_syntactic(else_, counts);
        }
        IrStmtKind::Comment { .. } => {}
    }
}

// ── Clone ID classification ────────────────────────────────────────

fn needs_clone(ty: &Ty) -> bool {
    match ty {
        Ty::String | Ty::Applied(_, _) |
        Ty::Record { .. } | Ty::OpenRecord { .. } |
        Ty::Named(_, _) | Ty::Matrix | Ty::Bytes |
        Ty::Variant { .. } | Ty::Fn { .. } |
        Ty::TypeVar(_) => true,
        // A tuple needs cloning when any element needs cloning. Pure numeric
        // tuples like `(Int, Int)` are Copy in the Rust target and can be
        // moved out of an index access directly.
        Ty::Tuple(elements) => elements.iter().any(needs_clone),
        _ => false,
    }
}

/// Split clone candidates into "always clone" and "eligible for last-use move".
fn split_clone_ids(
    vt: &VarTable,
    top_let_vars: &HashSet<VarId>,
    syntactic: &HashMap<VarId, u32>,
) -> (HashSet<VarId>, HashSet<VarId>) {
    let mut always = HashSet::new();
    let mut eligible = HashSet::new();

    for i in 0..vt.len() {
        let id = VarId(i as u32);
        let info = vt.get(id);
        if !needs_clone(&info.ty) { continue; }

        let name = almide_base::intern::resolve(info.name);
        if top_let_vars.contains(&id) || matches!(&info.ty, Ty::Fn { .. } | Ty::TypeVar(_))
            || name.starts_with("__cap_") || name.starts_with("__licm")
        {
            always.insert(id);
        } else {
            let syn = syntactic.get(&id).copied().unwrap_or(0);
            if syn > 1 {
                // Multiple syntactic uses → eligible for last-use optimization
                eligible.insert(id);
            } else if info.use_count > 1 {
                // Single syntactic use but bumped (loop/lambda) → always clone
                always.insert(id);
            }
            // syn <= 1 && use_count <= 1: single use, no loop → move by default
        }
    }
    (always, eligible)
}

fn build_remaining(eligible: &HashSet<VarId>, syntactic: &HashMap<VarId, u32>) -> HashMap<VarId, u32> {
    eligible.iter().map(|&id| (id, syntactic.get(&id).copied().unwrap_or(0))).collect()
}

fn reset_remaining(remaining: &mut HashMap<VarId, u32>, eligible: &HashSet<VarId>, syntactic: &HashMap<VarId, u32>) {
    for &id in eligible {
        remaining.insert(id, syntactic.get(&id).copied().unwrap_or(0));
    }
}

// ── Clone insertion with last-use tracking ─────────────────────────

fn make_clone(id: VarId, ty: Ty, span: Option<Span>) -> IrExpr {
    IrExpr {
        kind: IrExprKind::Clone {
            expr: Box::new(IrExpr { kind: IrExprKind::Var { id }, ty: ty.clone(), span }),
        },
        ty, span,
    }
}

fn insert_clones_live(
    expr: IrExpr,
    always: &HashSet<VarId>,
    eligible: &HashSet<VarId>,
    remaining: &mut HashMap<VarId, u32>,
    in_loop: bool,
) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        // ── Var: the core decision point ───────────────────────────
        IrExprKind::Var { id } if always.contains(&id) => {
            return make_clone(id, ty, span);
        }
        IrExprKind::Var { id } if eligible.contains(&id) => {
            if let Some(r) = remaining.get_mut(&id) {
                *r = r.saturating_sub(1);
                if *r == 0 && !in_loop {
                    // Last use outside a loop → move (no clone)
                    return IrExpr { kind: IrExprKind::Var { id }, ty, span };
                }
            }
            return make_clone(id, ty, span);
        }

        // ── Block: sequential statements ───────────────────────────
        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: insert_clone_stmts_live(stmts, always, eligible, remaining, in_loop),
            expr: expr.map(|e| Box::new(insert_clones_live(*e, always, eligible, remaining, in_loop))),
        },

        // ── If: save/restore/min for branches ──────────────────────
        IrExprKind::If { cond, then, else_ } => {
            let new_cond = insert_clones_live(*cond, always, eligible, remaining, in_loop);
            let saved = remaining.clone();
            let new_then = insert_clones_live(*then, always, eligible, remaining, in_loop);
            let then_remaining = std::mem::replace(remaining, saved);
            let new_else = insert_clones_live(*else_, always, eligible, remaining, in_loop);
            // Merge: take min (conservative — the branch that consumed more wins)
            for &id in eligible.iter() {
                let t = then_remaining.get(&id).copied().unwrap_or(0);
                let e = remaining.get(&id).copied().unwrap_or(0);
                remaining.insert(id, t.min(e));
            }
            IrExprKind::If {
                cond: Box::new(new_cond),
                then: Box::new(new_then),
                else_: Box::new(new_else),
            }
        }

        // ── Match: same strategy as If but N arms ──────────────────
        IrExprKind::Match { subject, arms } => {
            let new_subject = insert_clones_live(*subject, always, eligible, remaining, in_loop);
            let saved = remaining.clone();
            let mut min_remaining = HashMap::new();
            let mut new_arms = Vec::with_capacity(arms.len());

            for (i, arm) in arms.into_iter().enumerate() {
                *remaining = saved.clone();
                let new_guard = arm.guard.map(|g| insert_clones_live(g, always, eligible, remaining, in_loop));
                let new_body = insert_clones_live(arm.body, always, eligible, remaining, in_loop);
                new_arms.push(IrMatchArm { pattern: arm.pattern, guard: new_guard, body: new_body });

                if i == 0 {
                    min_remaining = remaining.clone();
                } else {
                    for &id in eligible.iter() {
                        let cur = remaining.get(&id).copied().unwrap_or(0);
                        let prev = min_remaining.get(&id).copied().unwrap_or(0);
                        min_remaining.insert(id, cur.min(prev));
                    }
                }
            }
            *remaining = min_remaining;
            IrExprKind::Match { subject: Box::new(new_subject), arms: new_arms }
        }

        // ── ForIn: iterable is NOT in loop, body IS ────────────────
        IrExprKind::ForIn { var, var_tuple, iterable, body } => {
            let new_iterable = insert_clones_live(*iterable, always, eligible, remaining, in_loop);
            let new_body = insert_clone_stmts_live(body, always, eligible, remaining, true);
            IrExprKind::ForIn { var, var_tuple, iterable: Box::new(new_iterable), body: new_body }
        }

        // ── While: cond and body are in loop ───────────────────────
        IrExprKind::While { cond, body } => {
            let new_cond = insert_clones_live(*cond, always, eligible, remaining, true);
            let new_body = insert_clone_stmts_live(body, always, eligible, remaining, true);
            IrExprKind::While { cond: Box::new(new_cond), body: new_body }
        }

        // ── Lambda: body recurses normally ─────────────────────────
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params, body: Box::new(insert_clones_live(*body, always, eligible, remaining, in_loop)), lambda_id,
        },

        // ── Call ───────────────────────────────────────────────────
        IrExprKind::Call { target, args, type_args } => {
            let args = args.into_iter().map(|a| insert_clones_live(a, always, eligible, remaining, in_loop)).collect();
            let target = match target {
                CallTarget::Method { object, method } => CallTarget::Method {
                    object: Box::new(insert_clones_live(*object, always, eligible, remaining, in_loop)), method,
                },
                CallTarget::Computed { callee } => CallTarget::Computed {
                    callee: Box::new(insert_clones_live(*callee, always, eligible, remaining, in_loop)),
                },
                other => other,
            };
            IrExprKind::Call { target, args, type_args }
        }
        IrExprKind::RuntimeCall { symbol, args } => {
            let args = args.into_iter().map(|a| insert_clones_live(a, always, eligible, remaining, in_loop)).collect();
            IrExprKind::RuntimeCall { symbol, args }
        }

        // ── IndexAccess: borrow container, clone element ───────────
        IrExprKind::IndexAccess { object, index } => {
            let mut processed_object = insert_clones_live(*object, always, eligible, remaining, in_loop);
            // Strip top-level Clone from container (indexing borrows)
            if let IrExprKind::Clone { expr } = processed_object.kind {
                processed_object = *expr;
            }
            let processed_index = insert_clones_live(*index, always, eligible, remaining, in_loop);
            let access = IrExpr {
                kind: IrExprKind::IndexAccess {
                    object: Box::new(processed_object),
                    index: Box::new(processed_index),
                },
                ty: ty.clone(), span,
            };
            if needs_clone(&ty) {
                return IrExpr { kind: IrExprKind::Clone { expr: Box::new(access) }, ty, span };
            }
            return access;
        }

        // ── MapAccess: borrow container, clone element ─────────────
        IrExprKind::MapAccess { object, key } => {
            let mut processed_object = insert_clones_live(*object, always, eligible, remaining, in_loop);
            if let IrExprKind::Clone { expr } = processed_object.kind {
                processed_object = *expr;
            }
            let processed_key = insert_clones_live(*key, always, eligible, remaining, in_loop);
            let access = IrExpr {
                kind: IrExprKind::MapAccess {
                    object: Box::new(processed_object),
                    key: Box::new(processed_key),
                },
                ty: ty.clone(), span,
            };
            if needs_clone(&ty) {
                return IrExpr { kind: IrExprKind::Clone { expr: Box::new(access) }, ty, span };
            }
            return access;
        }

        // ── Simple recursion cases ─────────────────────────────────
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op,
            left: Box::new(insert_clones_live(*left, always, eligible, remaining, in_loop)),
            right: Box::new(insert_clones_live(*right, always, eligible, remaining, in_loop)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(insert_clones_live(*operand, always, eligible, remaining, in_loop)),
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| insert_clones_live(e, always, eligible, remaining, in_loop)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: fields.into_iter().map(|(k, v)| (k, insert_clones_live(v, always, eligible, remaining, in_loop))).collect(),
        },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(insert_clones_live(*object, always, eligible, remaining, in_loop)), field,
        },
        IrExprKind::OptionalChain { expr, field } => IrExprKind::OptionalChain {
            expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)), field,
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: insert_clones_live(expr, always, eligible, remaining, in_loop) },
                other => other,
            }).collect(),
        },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)) },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)) },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)) },
        IrExprKind::Unwrap { expr } => IrExprKind::Unwrap { expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)) },
        IrExprKind::Deref { expr } => IrExprKind::Deref { expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)) },
        IrExprKind::UnwrapOr { expr, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)),
            fallback: Box::new(insert_clones_live(*fallback, always, eligible, remaining, in_loop)),
        },
        IrExprKind::ToOption { expr } => IrExprKind::ToOption { expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)) },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| insert_clones_live(e, always, eligible, remaining, in_loop)).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => {
            // Fields are evaluated before the spread base in Rust struct literals
            let new_fields: Vec<_> = fields.into_iter().map(|(k, v)| (k, insert_clones_live(v, always, eligible, remaining, in_loop))).collect();
            let new_base = insert_clones_live(*base, always, eligible, remaining, in_loop);
            IrExprKind::SpreadRecord { base: Box::new(new_base), fields: new_fields }
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(insert_clones_live(*start, always, eligible, remaining, in_loop)),
            end: Box::new(insert_clones_live(*end, always, eligible, remaining, in_loop)),
            inclusive,
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(|e| insert_clones_live(e, always, eligible, remaining, in_loop)).collect(),
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (insert_clones_live(k, always, eligible, remaining, in_loop), insert_clones_live(v, always, eligible, remaining, in_loop))).collect(),
        },
        IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex {
            object: Box::new(insert_clones_live(*object, always, eligible, remaining, in_loop)), index,
        },
        IrExprKind::Borrow { expr, as_str, mutable } => {
            let mut inner = insert_clones_live(*expr, always, eligible, remaining, in_loop);
            // Strip clone inside borrow: &x.clone() → &x (borrow doesn't consume ownership)
            if let IrExprKind::Clone { expr: unwrapped } = inner.kind {
                inner = *unwrapped;
            }
            IrExprKind::Borrow { expr: Box::new(inner), as_str, mutable }
        },
        IrExprKind::BoxNew { expr } => IrExprKind::BoxNew {
            expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)),
        },
        IrExprKind::ToVec { expr } => IrExprKind::ToVec {
            expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)),
        },
        IrExprKind::Await { expr } => IrExprKind::Await {
            expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)),
        },
        IrExprKind::RustMacro { name, args } => IrExprKind::RustMacro {
            name, args: args.into_iter().map(|a| insert_clones_live(a, always, eligible, remaining, in_loop)).collect(),
        },
        other => other,
    };

    IrExpr { kind, ty, span }
}

fn insert_clone_stmts_live(
    stmts: Vec<IrStmt>,
    always: &HashSet<VarId>,
    eligible: &HashSet<VarId>,
    remaining: &mut HashMap<VarId, u32>,
    in_loop: bool,
) -> Vec<IrStmt> {
    stmts.into_iter().map(|s| {
        let kind = match s.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
                var, mutability, ty, value: insert_clones_live(value, always, eligible, remaining, in_loop),
            },
            IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: insert_clones_live(value, always, eligible, remaining, in_loop) },
            IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: insert_clones_live(expr, always, eligible, remaining, in_loop) },
            IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
                cond: insert_clones_live(cond, always, eligible, remaining, in_loop), else_: insert_clones_live(else_, always, eligible, remaining, in_loop),
            },
            IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
                pattern, value: insert_clones_live(value, always, eligible, remaining, in_loop),
            },
            IrStmtKind::IndexAssign { target, index, value } => IrStmtKind::IndexAssign {
                target, index: insert_clones_live(index, always, eligible, remaining, in_loop), value: insert_clones_live(value, always, eligible, remaining, in_loop),
            },
            IrStmtKind::FieldAssign { target, field, value } => IrStmtKind::FieldAssign {
                target, field, value: insert_clones_live(value, always, eligible, remaining, in_loop),
            },
            IrStmtKind::MapInsert { target, key, value } => IrStmtKind::MapInsert {
                target, key: insert_clones_live(key, always, eligible, remaining, in_loop), value: insert_clones_live(value, always, eligible, remaining, in_loop),
            },
            other => other,
        };
        IrStmt { kind, span: s.span }
    }).collect()
}
