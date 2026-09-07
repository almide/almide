//! RETURNS-FRESH fns (#1990): the table fns whose every return value is a
//! block allocated INSIDE the call — a literal, a constructor, a
//! `prim.alloc_*` result, or another returns-fresh call — never a view of
//! a parameter, a global, or a raw address derived from one.
//!
//! Why the set exists: a module-space fn keeps every RC release on its
//! epilogue (the raw-address rule, #1988) because a prim-tier callee may
//! return a VIEW into one of the caller's params (the regex engine's
//! capture buffer) — releasing that param at a `return_call` site freed
//! the block under the view. The `bytes.append_*` wrappers are the cost:
//! `bytes_append_u16_le(b, v) = __bam(b, v, 2, true)` never released `b`
//! at its tail site, so the OLD buffer stayed at rc 1 on every append —
//! a quadratic leak over a loop. When the tail callee is returns-fresh
//! its result cannot alias `b`, so the wrapper's tail release is exactly
//! the epilogue release moved before the jump: the callee still owns its
//! copy (rc_arg_guard's +1) and decs it at ITS epilogue.
//!
//! Least fixpoint, conservative: a fn joins only when its tail is fresh
//! under the current set, and a `Var` tail is fresh only when it was bound
//! to a fresh rhs and is never re-assigned in the body.

use std::collections::HashSet;

use almide_ir::{CallTarget, IrExpr, IrExprKind, IrFunction, IrStmt, IrStmtKind, VarId};

use crate::rc_ownership::rc_certainly_fresh;
use crate::FnTable;

pub(crate) type ReturnsFresh = std::cell::RefCell<HashSet<usize>>;

pub(crate) fn returns_fresh_fns(
    program_fns: &[(&IrFunction, Option<String>, u32)],
    table: &FnTable,
) -> HashSet<usize> {
    let mut fresh: HashSet<usize> = HashSet::new();
    loop {
        let snapshot = fresh.clone();
        for (i, (f, qual, _)) in program_fns.iter().enumerate() {
            if snapshot.contains(&i) || table.infos[i].refuse.is_some() {
                continue;
            }
            let cur_module = qual.as_deref().and_then(|q| q.split('.').next());
            let assigned = assigned_vars(&f.body);
            let cx = Cx { table, cur_module, fresh: &snapshot, assigned: &assigned };
            let mut bound = HashSet::new();
            if expr_fresh(&f.body, &cx, &mut bound) {
                fresh.insert(i);
            }
        }
        if fresh.len() == snapshot.len() {
            if std::env::var_os("ALMIDE_FRESH_DEBUG").is_some() {
                let mut names: Vec<String> = fresh
                    .iter()
                    .map(|&i| {
                        let (f, q, _) = &program_fns[i];
                        q.clone().unwrap_or_else(|| f.name.as_str().to_string())
                    })
                    .collect();
                names.sort_unstable();
                eprintln!("[fresh] returns-fresh fns: {}", names.join(" "));
            }
            return fresh;
        }
    }
}

struct Cx<'a> {
    table: &'a FnTable,
    cur_module: Option<&'a str>,
    fresh: &'a HashSet<usize>,
    assigned: &'a HashSet<VarId>,
}

/// Every var the body re-assigns anywhere (a `Var` tail bound fresh but
/// later assigned a view is not fresh).
fn assigned_vars(body: &IrExpr) -> HashSet<VarId> {
    struct Scan(HashSet<VarId>);
    impl almide_ir::visit::IrVisitor for Scan {
        fn visit_stmt(&mut self, s: &IrStmt) {
            if let IrStmtKind::Assign { var, .. } = &s.kind {
                self.0.insert(*var);
            }
            almide_ir::visit::walk_stmt(self, s);
        }
    }
    let mut s = Scan(HashSet::new());
    almide_ir::visit::IrVisitor::visit_expr(&mut s, body);
    s.0
}

fn call_fresh(target: &CallTarget, cx: &Cx) -> bool {
    match target {
        CallTarget::Named { name } => {
            let name = name.as_str();
            cx.cur_module
                .and_then(|m| cx.table.by_name.get(&format!("{m}.{name}")))
                .or_else(|| cx.table.by_name.get(name))
                .is_some_and(|i| cx.fresh.contains(i))
        }
        CallTarget::Module { module, func, .. } => {
            (module.as_str() == "prim" && func.as_str().starts_with("alloc_"))
                || cx
                    .table
                    .by_name
                    .get(&format!("{}.{}", module.as_str(), func.as_str()))
                    .is_some_and(|i| cx.fresh.contains(i))
        }
        _ => false,
    }
}

/// Is every value this expression can evaluate to freshly allocated?
/// `bound` accumulates the block-local vars bound to fresh values.
fn expr_fresh(e: &IrExpr, cx: &Cx, bound: &mut HashSet<VarId>) -> bool {
    use IrExprKind as K;
    if rc_certainly_fresh(&e.kind) {
        return true;
    }
    match &e.kind {
        K::Var { id } => bound.contains(id) && !cx.assigned.contains(id),
        K::If { then, else_, .. } => {
            let mut b1 = bound.clone();
            let mut b2 = bound.clone();
            expr_fresh(then, cx, &mut b1) && expr_fresh(else_, cx, &mut b2)
        }
        K::Match { arms, .. } => arms.iter().all(|a| {
            let mut b = bound.clone();
            expr_fresh(&a.body, cx, &mut b)
        }),
        K::Block { stmts, expr } => {
            for s in stmts {
                if let IrStmtKind::Bind { var, value, .. } = &s.kind {
                    let mut b = bound.clone();
                    if expr_fresh(value, cx, &mut b) {
                        bound.insert(*var);
                    }
                }
            }
            expr.as_deref().is_some_and(|t| expr_fresh(t, cx, bound))
        }
        K::Call { target, .. } | K::TailCall { target, .. } => call_fresh(target, cx),
        _ => false,
    }
}
