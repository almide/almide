//! C-319 shared-cell classification: a var that is BOTH referenced from
//! inside some lambda AND mutated anywhere in the fn lives in a one-slot
//! heap CELL — the local (in the fn and in every capturing closure)
//! holds the cell's address, reads load through it, writes store through
//! it, so mutation is visible in BOTH directions (the interp's
//! scope-by-reference capture, mirrored). The capture side is an
//! OVER-approximation (every Var id occurring inside a lambda subtree):
//! a cell for a var nobody actually captures is observably identical,
//! only slower — never unsound. For-in loop vars are excluded: the
//! interp BINDS them fresh per iteration (a new cell each time), so the
//! loop's own advancement is not a mutation of one shared cell.

use std::collections::{HashMap, HashSet};

use crate::SliceTy;

use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId};

#[derive(Default)]
struct Scan {
    in_lambda: u32,
    captured: HashSet<VarId>,
    mutated: HashSet<VarId>,
}

impl IrVisitor for Scan {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Lambda { .. } => {
                self.in_lambda += 1;
                walk_expr(self, e);
                self.in_lambda -= 1;
                return;
            }
            IrExprKind::Var { id } if self.in_lambda > 0 => {
                self.captured.insert(*id);
            }
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                // Which args a stdlib call writes comes from the callee's
                // DECLARATION (`mut` params), the source native's borrow
                // inference and the shared optimizer read too. A hand list
                // of mutators lived here and drifted twice: bytes' writers
                // were missing (the develop wasm_runtime catch), and then
                // every bytes writer but six still was — a captured Bytes
                // written through `bytes.set_u8` took the env value-copy
                // path and the write was lost (#2951).
                // The written place may be a field or tuple slot of the
                // var (`list.push(h.xs, 1)`, #2961): its ROOT is mutated.
                for k in almide_ir::mut_args::stdlib_mut_positions(module.as_str(), func.as_str()).unwrap_or_default() {
                    if let Some(id) = args.get(k).and_then(place_root) {
                        self.mutated.insert(id);
                    }
                }
            }
            _ => {}
        }
        walk_expr(self, e);
    }

    fn visit_stmt(&mut self, s: &IrStmt) {
        // A write target names the var without a `Var` read: inside a
        // lambda it is a capture too (`() => { xs[5] = 1 }` reads nothing).
        if let Some(v) = write_target(s) {
            self.mutated.insert(v);
            if self.in_lambda > 0 {
                self.captured.insert(v);
            }
        }
        walk_stmt(self, s);
    }
}

/// The var at the root of a place — `h`, `h.xs`, `h.a.b`, `t.0` — or
/// `None` when the expression is not a place over a var.
fn place_root(e: &IrExpr) -> Option<VarId> {
    match &e.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => place_root(object),
        _ => None,
    }
}

/// The var a write statement stores into, if it is one.
fn write_target(s: &IrStmt) -> Option<VarId> {
    match &s.kind {
        IrStmtKind::Assign { var, .. } => Some(*var),
        IrStmtKind::IndexAssign { target, .. }
        | IrStmtKind::MapInsert { target, .. }
        | IrStmtKind::FieldAssign { target, .. } => Some(*target),
        _ => None,
    }
}

/// Vars needing cell storage in this fn body.
pub(crate) fn cell_vars_of(body: &IrExpr) -> HashSet<VarId> {
    let mut s = Scan::default();
    s.visit_expr(body);
    s.captured.intersection(&s.mutated).copied().collect()
}

/// A PARAMETER in a fn's cell set (#3154: a `mut` param a closure captures
/// and the fn writes) has no bind to allocate its cell at, so its local held
/// the bare value while every read and write went through it as a cell
/// address — garbage reads, and an invalid module for a scalar. Each one is
/// rebound onto a `var` local at entry (`almide_ir::param_rebind`), which the
/// bind path allocates as a cell; the C-132 exits read that local back, so
/// the caller's place receives the final value. Runs after the move-mode
/// rewrite, whose write-backs are the writes a captured callee call makes.
pub fn rebind_cell_params(program: &mut almide_ir::IrProgram) {
    fn each(funcs: &mut [almide_ir::IrFunction], vt: &mut almide_ir::VarTable) {
        for func in funcs {
            let cells = cell_vars_of(&func.body);
            let params: Vec<VarId> = func.params.iter().map(|p| p.var).filter(|v| cells.contains(v)).collect();
            for p in params {
                almide_ir::param_rebind::rebind_param_as_var(&mut func.body, vt, p);
            }
        }
    }
    each(&mut program.functions, &mut program.var_table);
    // A module fn's VarIds index its MODULE's table on this leg.
    for m in &mut program.modules {
        each(&mut m.functions, &mut m.var_table);
    }
}

/// Which args a linked module call writes (and so must make unique first).
/// A bundled stdlib surface answers from its own DECLARATION, not from
/// whichever implementation the self-host registry linked for it: the
/// implementation's params carry no `mut` (`bytes_set_uint16(b: Bytes, ..)`
/// behind `set_uint16(mut b: Bytes, ..)`), so the write went through a shared
/// buffer and an alias saw it (#2949). The checker, native, the shared
/// optimizer and the cell scan above read the same declaration. A user or
/// package module fn keeps its own params (`linked`).
pub(crate) fn linked_param_mut(module: &str, func: &str, arity: usize, linked: &[bool]) -> Vec<bool> {
    if almide_types::stdlib_info::is_bundled_module(module) {
        let muts = almide_ir::mut_args::stdlib_mut_positions(module, func).unwrap_or_default();
        (0..arity).map(|k| muts.contains(&k)).collect()
    } else {
        linked.to_vec()
    }
}

impl crate::emitter::Emitter<'_> {
    /// The lambda body's captured OUTER locals (VarIds are unique within
    /// a function context, so any Var resolving through the enclosing
    /// locals map that is not a lambda param is a capture).
    pub(crate) fn captured_vars(
        &self,
        params: &std::collections::HashSet<VarId>,
        body: &IrExpr,
    ) -> Vec<(VarId, SliceTy)> {
        struct Scan<'x> {
            locals: &'x HashMap<VarId, (u32, SliceTy)>,
            params: &'x std::collections::HashSet<VarId>,
            out: Vec<(VarId, SliceTy)>,
        }
        impl almide_ir::visit::IrVisitor for Scan<'_> {
            fn visit_expr(&mut self, e: &IrExpr) {
                if let IrExprKind::Var { id } = &e.kind
                    && !self.params.contains(id)
                    && let Some(&(_, ty)) = self.locals.get(id)
                    && !self.out.iter().any(|(v, _)| v == id)
                {
                    self.out.push((*id, ty));
                }
                almide_ir::visit::walk_expr(self, e);
            }
            fn visit_stmt(&mut self, s: &IrStmt) {
                if let Some(id) = write_target(s)
                    && !self.params.contains(&id)
                    && let Some(&(_, ty)) = self.locals.get(&id)
                    && !self.out.iter().any(|(v, _)| *v == id)
                {
                    self.out.push((id, ty));
                }
                almide_ir::visit::walk_stmt(self, s);
            }
        }
        let mut sc = Scan { locals: self.locals, params, out: Vec::new() };
        almide_ir::visit::IrVisitor::visit_expr(&mut sc, body);
        // A var BOUND inside the body is the lambda's own local, not a
        // capture (#2758): the enclosing frame never assigns it, so the env
        // slot only ever carried a NULL the drop glue released again.
        let inner = bound_within(body);
        sc.out.retain(|(v, _)| !inner.contains(v));
        sc.out
    }
}

/// Every var a binding form inside `body` introduces: a `let`, a
/// destructure or match pattern, a `for` variable, a nested lambda's params.
fn bound_within(body: &IrExpr) -> HashSet<VarId> {
    #[derive(Default)]
    struct Bound(HashSet<VarId>);
    impl IrVisitor for Bound {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::ForIn { var, var_tuple, .. } => {
                    self.0.insert(*var);
                    self.0.extend(var_tuple.iter().flatten().copied());
                }
                IrExprKind::Lambda { params, .. } => self.0.extend(params.iter().map(|(v, _)| *v)),
                _ => {}
            }
            walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            if let IrStmtKind::Bind { var, .. } = &s.kind {
                self.0.insert(*var);
            }
            walk_stmt(self, s);
        }
        fn visit_pattern(&mut self, p: &almide_ir::IrPattern) {
            if let almide_ir::IrPattern::Bind { var, .. } | almide_ir::IrPattern::As { var, .. } = p {
                self.0.insert(*var);
            }
            almide_ir::visit::walk_pattern(self, p);
        }
    }
    let mut b = Bound::default();
    b.visit_expr(body);
    b.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use almide_types::types::Ty;

    fn e(kind: IrExprKind) -> IrExpr {
        IrExpr { kind, ty: Ty::Unknown, span: None, def_id: None }
    }

    /// The #2951 GATE, enumerated from the stdlib declarations rather than a
    /// hand list: a var a closure captures and that ANY stdlib call writes
    /// through a declared `mut` param gets a shared cell. The list this
    /// replaced covered six of the fifty bytes writers.
    #[test]
    fn every_stdlib_mut_write_to_a_captured_var_makes_a_cell() {
        let mut missed = Vec::new();
        for (module, func, idxs) in almide_ir::mut_args::stdlib_mut_fns() {
            let arity = idxs.iter().max().copied().unwrap_or(0) + 1;
            let args: Vec<IrExpr> = (0..arity)
                .map(|i| if i == idxs[0] { e(IrExprKind::Var { id: VarId(0) }) } else { e(IrExprKind::LitInt { value: 0 }) })
                .collect();
            let call = e(IrExprKind::Call {
                target: CallTarget::Module { module: almide_base::intern::sym(module), func: almide_base::intern::sym(&func), def_id: None },
                args,
                type_args: vec![],
            });
            let lambda = e(IrExprKind::Lambda { params: vec![], body: Box::new(e(IrExprKind::Var { id: VarId(0) })), lambda_id: None });
            let body = e(IrExprKind::Block {
                stmts: vec![
                    IrStmt { kind: IrStmtKind::Expr { expr: lambda }, span: None },
                    IrStmt { kind: IrStmtKind::Expr { expr: call }, span: None },
                ],
                expr: None,
            });
            if !cell_vars_of(&body).contains(&VarId(0)) {
                missed.push(format!("{module}.{func}"));
            }
        }
        assert!(missed.is_empty(), "a captured var written through these got no cell: {missed:?}");
    }
}
