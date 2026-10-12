//! Fusing through a user-fn sink (#3584): `xs |> list.map(f) |> total`, where
//! `total`'s whole body is one chain over its only parameter, fuses exactly
//! as `xs |> list.map(f) |> list.fold(…)` written inline does.
//!
//! A SINK is a function whose body is ONE fused chain that ends in a
//! non-`Collect` collector and whose source is its only parameter, with no
//! other free variable — so the call `total(<chain>.collect())` means "run
//! `total`'s chain over these elements", and substituting the argument for
//! the source is the whole of the call's meaning. The shape is read off the
//! body after `StreamFusion` has fused it; no function is named.
//!
//! At a call whose argument is a `Collect`-ending chain, the sink's body is
//! copied with every variable it binds renamed to a fresh `VarId` (the
//! program has one var table, and one binding site per id), the argument is
//! put in the source slot, and the result is handed to the same `merge` an
//! inline chain gets: the purity / totality rule that decides whether the
//! interleaved order is unobservable is not restated here. When `merge`
//! refuses, the call is left as it was — the copy is never emitted nested.

use std::collections::{HashMap, HashSet};

use almide_base::intern::Sym;
use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::visit_mut::{walk_expr_mut, walk_pattern_mut, walk_stmt_mut, IrMutVisitor};
use almide_ir::*;

/// A sink's parameter and its (fused) body chain.
pub(super) struct Sink {
    param: VarId,
    body: IrExpr,
}

/// The sinks among `functions`, keyed by the name a `CallTarget::Named`
/// call carries.
pub(super) fn sinks<'f>(functions: impl Iterator<Item = &'f IrFunction>) -> HashMap<Sym, Sink> {
    functions.filter_map(|f| Some((f.name, sink(f)?))).collect()
}

fn sink(f: &IrFunction) -> Option<Sink> {
    let [p] = f.params.as_slice() else { return None };
    if f.is_effect || f.is_test || p.is_mut || f.generics.as_ref().is_some_and(|g| !g.is_empty()) {
        return None;
    }
    let body = tail(&f.body);
    let IrExprKind::IterChain { source, collector, .. } = &body.kind else { return None };
    if matches!(collector, IterCollector::Collect) || source_var(source) != Some(p.var) {
        return None;
    }
    // `p` is read by the source slot and nowhere else, and nothing else is
    // free: the argument is evaluated once, where the source is.
    if free_vars::free_vars(body, &HashSet::new()) != [p.var] || occurrences(body, p.var) != 1 {
        return None;
    }
    if !renamable(body) {
        return None;
    }
    Some(Sink { param: p.var, body: body.clone() })
}

/// A body is the tail of an empty block as much as it is itself.
fn tail(e: &IrExpr) -> &IrExpr {
    match &e.kind {
        IrExprKind::Block { stmts, expr: Some(t) } if stmts.is_empty() => tail(t),
        _ => e,
    }
}

fn source_var(source: &IrExpr) -> Option<VarId> {
    match &source.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Borrow { expr, .. } => source_var(expr),
        _ => None,
    }
}

fn occurrences(e: &IrExpr, var: VarId) -> usize {
    struct Count { var: VarId, n: usize }
    impl IrVisitor for Count {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e.kind, IrExprKind::Var { id } if id == self.var) {
                self.n += 1;
            }
            walk_expr(self, e);
        }
    }
    let mut c = Count { var, n: 0 };
    c.visit_expr(e);
    c.n
}

/// Every place a `VarId` can sit in the body is one `Rename` rewrites: the
/// statements that name a variable by id other than `let` (assignments, the
/// list-mutation forms) and the closure-converted forms are refused rather
/// than listed twice. A pure chain body never holds them anyway.
fn renamable(e: &IrExpr) -> bool {
    struct Scan { ok: bool }
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e.kind, IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }) {
                self.ok = false;
            }
            if self.ok { walk_expr(self, e); }
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            if !matches!(s.kind, IrStmtKind::Bind { .. } | IrStmtKind::BindDestructure { .. }
                | IrStmtKind::Expr { .. } | IrStmtKind::Guard { .. } | IrStmtKind::Comment { .. })
            {
                self.ok = false;
            }
            if self.ok { walk_stmt(self, s); }
        }
    }
    let mut s = Scan { ok: true };
    s.visit_expr(e);
    s.ok
}

/// Gives every variable the copy binds a fresh id. Binding sites are met
/// before their uses in walk order (a lambda's params before its body, a
/// `let` before the statements after it, an arm's pattern before its guard
/// and body), so one pass both allocates and rewrites.
struct Rename<'v> {
    vt: &'v mut VarTable,
    map: HashMap<VarId, VarId>,
}

impl Rename<'_> {
    fn bind(&mut self, id: &mut VarId) {
        let info = self.vt.get(*id);
        let (ty, m) = (info.ty.clone(), info.mutability);
        let fresh = self.vt.alloc_like(*id, ty, m);
        self.map.insert(*id, fresh);
        *id = fresh;
    }
    fn read(&self, id: &mut VarId) {
        if let Some(n) = self.map.get(id) {
            *id = *n;
        }
    }
}

impl IrMutVisitor for Rename<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        match &mut e.kind {
            IrExprKind::Var { id } => self.read(id),
            IrExprKind::Lambda { params, .. } => {
                for (id, _) in params.iter_mut() { self.bind(id); }
            }
            IrExprKind::ForIn { var, var_tuple, .. } => {
                self.bind(var);
                for id in var_tuple.iter_mut().flatten() { self.bind(id); }
            }
            _ => {}
        }
        walk_expr_mut(self, e);
    }
    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        // The value is walked first: it cannot read the variable it binds.
        walk_stmt_mut(self, s);
        if let IrStmtKind::Bind { var, .. } = &mut s.kind {
            self.bind(var);
        }
    }
    fn visit_pattern_mut(&mut self, p: &mut IrPattern) {
        if let IrPattern::Bind { var, .. } | IrPattern::As { var, .. } = p {
            self.bind(var);
        }
        walk_pattern_mut(self, p);
    }
}

/// The sink's chain with `arg` in the source slot and every bound variable
/// renamed — not yet merged.
pub(super) fn instantiate(sink: &Sink, arg: IrExpr, vt: &mut VarTable) -> IrExpr {
    let mut body = sink.body.clone();
    Rename { vt, map: HashMap::new() }.visit_expr_mut(&mut body);
    if let IrExprKind::IterChain { source, .. } = &mut body.kind {
        debug_assert_eq!(source_var(source), Some(sink.param));
        **source = arg;
    }
    body
}
