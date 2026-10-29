//! A `mut` parameter a closure captures, where the fn writes it (#3154).
//!
//! On the Rust target a `mut` param is a `&mut T` place, not a `var` local, so
//! the C-319 shared-cell lowering a captured-and-written `var` takes had no
//! declaration site to build its cell at: the capture read `r.get()` /
//! `r.borrow_proven(..)` off the bare `&mut T` and rustc refused (E0599). A
//! captured param the fn never writes is a plain read and needs nothing here.
//!
//! The param is given the storage a `var` has. Its body is rewritten onto a
//! fresh `var` local bound from the param at entry (copy-in), which the
//! shared-cell machinery then classifies like any captured var; the walker
//! renders that bind as the cell plus a write-back guard whose `Drop` stores
//! the cell's final value into the caller's place on every exit — the tail,
//! a `?` propagation, a guard — the copy-in/write-back a `mut` argument has on
//! every leg (#3103 ruling (A)).

use std::collections::{HashMap, HashSet};
use almide_ir::*;
use crate::use_kind::written_vars;

/// Rewrite every fn's captured-and-written `mut` params onto a `var` local;
/// returns `cell var -> param` for the walker's bind rendering.
pub(super) fn cell_mut_params(program: &mut IrProgram) -> HashMap<VarId, VarId> {
    let mut out = HashMap::new();
    let IrProgram { functions, modules, var_table, .. } = program;
    let module_fns = modules.iter_mut().flat_map(|m| m.functions.iter_mut());
    for func in functions.iter_mut().chain(module_fns) {
        for p in captured_written_mut_params(func) {
            let cell = almide_ir::param_rebind::rebind_param_as_var(&mut func.body, var_table, p);
            out.insert(cell, p);
        }
    }
    out
}

/// The `mut` params some lambda in the body captures and the fn writes
/// anywhere (inside a lambda or out).
fn captured_written_mut_params(func: &IrFunction) -> Vec<VarId> {
    let muts: Vec<VarId> = func.params.iter().filter(|p| p.is_mut).map(|p| p.var).collect();
    if muts.is_empty() {
        return Vec::new();
    }
    let written = written_vars(&func.body);
    let mut captured = HashSet::new();
    collect_captured(&func.body, &mut captured);
    muts.into_iter().filter(|v| written.contains(v) && captured.contains(v)).collect()
}

/// Every var free in some lambda of `body` that renders as a CLOSURE.
///
/// #3891: a fused chain's step / collector lambda is a scope, not a closure
/// (`transform_expr_iter_chain`): it renders as a non-`move` closure the
/// chain borrows for its own duration, so a `mut` param it writes is a plain
/// unique reborrow of the caller's place — no cell, no copy-in, no
/// write-back. That holds while the reborrow is the chain's ONLY use of the
/// param: one chain lambda mentions it, and nothing else in the chain does —
/// save a fold's `init`, which runs to completion before the fold's closure
/// is made. Any other shape (two lambdas, the source, a later step) would hold
/// two borrows at once, so the param keeps its cell.
fn collect_captured(body: &IrExpr, out: &mut HashSet<VarId>) {
    struct Scan<'a>(&'a mut HashSet<VarId>);
    impl Scan<'_> {
        /// A chain lambda's body is walked as a scope: closures nested in it
        /// still count, the lambda itself does not.
        fn visit_scope(&mut self, e: &IrExpr) {
            use almide_ir::visit::IrVisitor;
            match &e.kind {
                IrExprKind::Lambda { body, .. } => self.visit_expr(body),
                _ => self.visit_expr(e),
            }
        }
    }
    impl almide_ir::visit::IrVisitor for Scan<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Lambda { params, body, .. } => {
                    let bound: HashSet<VarId> = params.iter().map(|(v, _)| *v).collect();
                    self.0.extend(almide_ir::free_vars::free_vars(body, &bound));
                    almide_ir::visit::walk_expr(self, e);
                }
                IrExprKind::IterChain { source, steps, collector, .. } => {
                    self.0.extend(chain_shared_captures(source, steps, collector));
                    self.visit_expr(source);
                    for step in steps {
                        match step {
                            IterStep::Take { n } => self.visit_expr(n),
                            IterStep::Enumerate => {}
                            other => if let Some(l) = other.lambda() { self.visit_scope(l) },
                        }
                    }
                    if let IterCollector::Fold { init, .. } = collector {
                        self.visit_expr(init);
                    }
                    if let Some(l) = collector.lambda() {
                        self.visit_scope(l);
                    }
                }
                _ => almide_ir::visit::walk_expr(self, e),
            }
        }
    }
    almide_ir::visit::IrVisitor::visit_expr(&mut Scan(out), body);
}

/// The vars a chain lambda captures in a shape that would hold two borrows
/// of them at once (see [`collect_captured`]); those keep the cell.
fn chain_shared_captures(source: &IrExpr, steps: &[IterStep], collector: &IterCollector) -> HashSet<VarId> {
    let empty = HashSet::new();
    let lambda_fvs = |l: &IrExpr| -> HashSet<VarId> {
        match &l.kind {
            IrExprKind::Lambda { params, body, .. } => {
                let bound: HashSet<VarId> = params.iter().map(|(v, _)| *v).collect();
                almide_ir::free_vars::free_vars(body, &bound).into_iter().collect()
            }
            _ => almide_ir::free_vars::free_vars(l, &empty).into_iter().collect(),
        }
    };
    let plain_fvs = |e: &IrExpr| -> HashSet<VarId> { almide_ir::free_vars::free_vars(e, &empty).into_iter().collect() };
    // (free vars, is a lambda, is the fold's init / the fold's lambda)
    let mut parts: Vec<(HashSet<VarId>, bool, u8)> = vec![(plain_fvs(source), false, 0)];
    for step in steps {
        match step {
            IterStep::Take { n } => parts.push((plain_fvs(n), false, 0)),
            IterStep::Enumerate => {}
            other => if let Some(l) = other.lambda() { parts.push((lambda_fvs(l), true, 0)) },
        }
    }
    if let IterCollector::Fold { init, .. } = collector {
        parts.push((plain_fvs(init), false, 1));
    }
    if let Some(l) = collector.lambda() {
        let tag = if matches!(collector, IterCollector::Fold { .. }) { 2 } else { 0 };
        parts.push((lambda_fvs(l), true, tag));
    }
    let mut shared = HashSet::new();
    let in_lambdas: HashSet<VarId> = parts.iter().filter(|p| p.1).flat_map(|p| p.0.iter().copied()).collect();
    for v in in_lambdas {
        let holders: Vec<&(HashSet<VarId>, bool, u8)> = parts.iter().filter(|p| p.0.contains(&v)).collect();
        let lambdas = holders.iter().filter(|p| p.1).count();
        let sole_fold = lambdas == 1 && holders.iter().any(|p| p.2 == 2);
        let others_ok = holders.iter().all(|p| p.1 || (sole_fold && p.2 == 1));
        if lambdas > 1 || !others_ok {
            shared.insert(v);
        }
    }
    shared
}
