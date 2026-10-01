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

/// Every var free in some lambda of `body`.
fn collect_captured(body: &IrExpr, out: &mut HashSet<VarId>) {
    struct Scan<'a>(&'a mut HashSet<VarId>);
    impl almide_ir::visit::IrVisitor for Scan<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Lambda { params, body, .. } = &e.kind {
                let bound: HashSet<VarId> = params.iter().map(|(v, _)| *v).collect();
                self.0.extend(almide_ir::free_vars::free_vars(body, &bound));
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    almide_ir::visit::IrVisitor::visit_expr(&mut Scan(out), body);
}
