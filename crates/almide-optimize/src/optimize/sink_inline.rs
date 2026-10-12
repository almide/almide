//! Inline a list SINK into a call whose argument is a list chain (#3584,
//! #3590): `v |> list.map(f) |> total`, where `total(xs) = xs |> list.fold(…)`,
//! becomes `list.fold(list.map(v, f), …)` — the shape each leg's own fusion
//! already fuses (native `StreamFusion`, the structural leg's fused fold), and
//! decides under its own purity / totality rule. Where a leg declines, the
//! inlined form is evaluated exactly as the call was: the argument first, once.
//!
//! A SINK is a root fn whose whole body is ONE `list.*` call with its only
//! parameter as the first argument, read nowhere else, and no other free
//! variable — so substituting the argument for the parameter is the call's
//! whole meaning. The shape is read off the body; no function is named. A
//! call is rewritten only when its argument is itself a `list.*` call, the
//! one case where the inlined form can fuse.
//!
//! The copy's bound variables get fresh `VarId`s: the root program has one
//! var table and one binding site per id. Module fns keep their own tables
//! here (`UnifyVarTables` runs later, in codegen), so only root fns take part.

use std::collections::{HashMap, HashSet};

use almide_base::intern::Sym;
use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::visit_mut::{walk_expr_mut, walk_pattern_mut, walk_stmt_mut, IrMutVisitor};
use almide_ir::*;

pub fn inline_list_sinks(program: &mut IrProgram) {
    let sinks: HashMap<Sym, (VarId, IrExpr)> = program.functions.iter()
        .filter_map(|f| Some((f.name, sink(f)?)))
        .collect();
    if sinks.is_empty() {
        return;
    }
    let IrProgram { functions, top_lets, var_table, .. } = program;
    let mut v = Inliner { sinks: &sinks, vt: var_table };
    for f in functions.iter_mut() { v.visit_expr_mut(&mut f.body); }
    for tl in top_lets.iter_mut() { v.visit_expr_mut(&mut tl.value); }
}

fn sink(f: &IrFunction) -> Option<(VarId, IrExpr)> {
    let [p] = f.params.as_slice() else { return None };
    if f.is_effect || f.is_test || p.is_mut || f.name.as_str() == "main"
        || f.generics.as_ref().is_some_and(|g| !g.is_empty())
    {
        return None;
    }
    let body = tail(&f.body);
    let IrExprKind::Call { args, .. } = &body.kind else { return None };
    if !is_list_call(body) || !matches!(args.first().map(|a| &a.kind), Some(IrExprKind::Var { id }) if *id == p.var) {
        return None;
    }
    // `p` is the first argument and nothing else; nothing else is free.
    if free_vars::free_vars(body, &HashSet::new()) != [p.var] || occurrences(body, p.var) != 1 || !renamable(body) {
        return None;
    }
    Some((p.var, body.clone()))
}

fn is_list_call(e: &IrExpr) -> bool {
    matches!(&e.kind, IrExprKind::Call { target: CallTarget::Module { module, .. }, .. } if module.as_str() == "list")
}

/// A body is the tail of an empty block as much as it is itself.
fn tail(e: &IrExpr) -> &IrExpr {
    match &e.kind {
        IrExprKind::Block { stmts, expr: Some(t) } if stmts.is_empty() => tail(t),
        _ => e,
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

/// Every place a `VarId` can sit in the body is one `Rename` rewrites:
/// statements that name a variable by id other than `let` (assignments, the
/// list-mutation forms) and closure-converted forms are refused rather than
/// listed twice.
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

struct Inliner<'a> {
    sinks: &'a HashMap<Sym, (VarId, IrExpr)>,
    vt: &'a mut VarTable,
}

impl IrMutVisitor for Inliner<'_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        walk_expr_mut(self, expr);
        let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &expr.kind else { return };
        let (Some((param, body)), [arg]) = (self.sinks.get(name), args.as_slice()) else { return };
        if !is_list_call(arg) {
            return;
        }
        let mut copy = body.clone();
        Rename { vt: self.vt, map: HashMap::new() }.visit_expr_mut(&mut copy);
        if let IrExprKind::Call { args, .. } = &mut copy.kind {
            debug_assert!(matches!(args[0].kind, IrExprKind::Var { id } if id == *param));
            args[0] = arg.clone();
        }
        copy.ty = expr.ty.clone();
        copy.span = expr.span;
        *expr = copy;
    }
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
