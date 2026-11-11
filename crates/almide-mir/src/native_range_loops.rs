//! `for i in list.range(a, b)` as the counted loop `for i in a..<b` on the
//! native render. Spelled literally the call builds the list of every index
//! and the loop walks it — two million iterations allocated, filled and read
//! back sixteen megabytes only to count. The standard render iterates the
//! range directly, as the list combinators of this render already do
//! (`lower/defunc_hof_inner.rs`).
//!
//! Equivalence. `list.range(a, b)` holds exactly the Ints `a, a + 1, …, b - 1`
//! in order, none when `b <= a`, and the loop is its only reader. The call
//! reads its bounds once, before the first iteration, and so does the counted
//! loop: a bound the body can write is bound ahead of it
//! (`snapshot_range_bounds`, #4662). Bounds that evaluate nothing are
//! rewritten — Int literals, variables and wrapping `+ - *` over them; a
//! bound that calls a function keeps the list, since the guard below reads
//! the bounds more than once.
//!
//! A range too long to be a list at all ([`unbuildable`]) is an out-of-memory
//! abort when built, on every machine. The counted form keeps that abort:
//! ahead of the loop, such a range is still built ([`build_guard`]).
//!
//! `let ys = list.range(a, b) |> list.flat_map((i) => { …; [e1, e2] })` is
//! the same walk with a result (#4661): spelled literally the eight million
//! indices of a two-to-the-twenty-third range are a list of their own before
//! the first element is made. It becomes the counted loop that appends the
//! literal to the result as each round makes it ([`Counted::flat_mapped`]) —
//! the elements in the same order, the callback's statements run once per
//! index in order, and a round that aborts or prints does so where the
//! callback did. Scalar elements only, bound by a `let` in a statement list.
use almide_ir::{visit_mut::{walk_expr_mut, IrMutVisitor}, *};
use almide_lang::{intern::sym, types::{Ty, TypeConstructorId as C}};

/// The most elements a list holds: past it the bytes of its 8-byte slots
/// exceed what one allocation may ask for.
const MAX_LIST_LEN: i64 = i64::MAX / 8;

pub(crate) fn expand(ir: &mut IrProgram) {
    // A metered region charges what its plan names, and the plan names the
    // call of the source: such a program keeps it.
    if almide_ir::meter_plan::program_has_regions(ir) { return; }
    let mut pass = Counted { vars: &mut ir.var_table };
    for f in &mut ir.functions { pass.visit_expr_mut(&mut f.body); }
}

struct Counted<'a> { vars: &'a mut VarTable }

impl Counted<'_> {
    /// Reading `e` again gives the same value until a statement runs.
    fn fixed(&self, e: &IrExpr) -> bool {
        match &e.kind {
            IrExprKind::LitInt { .. } | IrExprKind::Var { .. } => true,
            IrExprKind::BinOp { op: BinOp::AddInt | BinOp::SubInt | BinOp::MulInt, left, right } => self.fixed(left) && self.fixed(right),
            _ => false,
        }
    }

    /// Rewrite the loop `e` when it walks a `list.range` of fixed bounds;
    /// the statement that goes ahead of it, when the range needs one.
    fn count(&mut self, e: &mut IrExpr) -> Option<IrStmt> {
        let IrExprKind::ForIn { iterable, var_tuple: None, .. } = &mut e.kind else { return None; };
        let (start, end) = range_call(iterable)?;
        if !self.fixed(start) || !self.fixed(end) { return None; }
        let (start, end) = (start.clone(), end.clone());
        let guard = match unbuildable(&start, &end) {
            Some(false) => None,
            Some(true) => return None,
            None => Some(build_guard(&start, &end, self.vars)),
        };
        iterable.kind = IrExprKind::Range { start: Box::new(start), end: Box::new(end), inclusive: false };
        guard
    }

    /// The statements that stand for `let var = list.range(a, b) |>
    /// list.flat_map((i) => { …; [e, …] })`: the guard, the result started
    /// empty, the counted loop appending each round's literal, and the bind.
    fn flat_mapped(&mut self, s: &IrStmt) -> Option<Vec<IrStmt>> {
        let IrStmtKind::Bind { var, mutability, ty, value } = &s.kind else { return None; };
        let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &value.kind else { return None; };
        let [source, IrExpr { kind: IrExprKind::Lambda { params, body, .. }, .. }] = args.as_slice() else { return None; };
        let [(index, Ty::Int)] = params.as_slice() else { return None; };
        let Ty::Applied(C::List, elem) = ty else { return None; };
        let scalar = matches!(elem.as_slice(), [Ty::Int | Ty::Float | Ty::Bool]);
        if module.as_str() != "list" || func.as_str() != "flat_map" || !scalar || value.ty != *ty { return None; }
        let (start, end) = range_call(source)?;
        if !self.fixed(start) || !self.fixed(end) { return None; }
        // The round's statements, then the literal it yields.
        let (mut round, literal) = match &body.kind {
            IrExprKind::List { .. } => (Vec::new(), (**body).clone()),
            IrExprKind::Block { stmts, expr: Some(tail) } if matches!(tail.kind, IrExprKind::List { .. }) => (stmts.clone(), (**tail).clone()),
            _ => return None,
        };
        if !matches!(&literal.kind, IrExprKind::List { elements } if !elements.is_empty()) || literal.ty != *ty { return None; }
        let mut out = Vec::new();
        match unbuildable(start, end) {
            Some(false) => {}
            Some(true) => return None,
            None => out.push(build_guard(start, end, self.vars)),
        }
        let acc = self.vars.alloc_fresh("flat_mapped", ty.clone(), Mutability::Var, s.span);
        let read = || node(IrExprKind::Var { id: acc }, ty.clone());
        out.push(IrStmt { kind: IrStmtKind::Bind { var: acc, mutability: Mutability::Var, ty: ty.clone(), value: node(IrExprKind::List { elements: Vec::new() }, ty.clone()) }, span: s.span });
        let grown = node(IrExprKind::BinOp { op: BinOp::ConcatList, left: Box::new(read()), right: Box::new(literal) }, ty.clone());
        round.push(IrStmt { kind: IrStmtKind::Assign { var: acc, value: grown }, span: s.span });
        let range = node(IrExprKind::Range { start: Box::new(start.clone()), end: Box::new(end.clone()), inclusive: false }, source.ty.clone());
        let walk = node(IrExprKind::ForIn { var: *index, var_tuple: None, iterable: Box::new(range), body: round }, Ty::Unit);
        out.push(IrStmt { kind: IrStmtKind::Expr { expr: walk }, span: s.span });
        out.push(IrStmt { kind: IrStmtKind::Bind { var: *var, mutability: *mutability, ty: ty.clone(), value: read() }, span: s.span });
        Some(out)
    }

    /// The loops among `stmts` (and `tail`), each behind its guard.
    fn count_all(&mut self, stmts: &mut Vec<IrStmt>, tail: Option<&mut IrExpr>) {
        let mut out = Vec::with_capacity(stmts.len());
        for mut s in std::mem::take(stmts) {
            if let Some(counted) = self.flat_mapped(&s) {
                out.extend(counted);
                continue;
            }
            if let IrStmtKind::Expr { expr } = &mut s.kind { out.extend(self.count(expr)); }
            out.push(s);
        }
        if let Some(e) = tail { out.extend(self.count(e)); }
        *stmts = out;
    }
}

impl IrMutVisitor for Counted<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        // A loop is rewritten where it stands in a statement list, the one
        // place its guard can go ahead of it.
        match &mut e.kind {
            IrExprKind::Block { stmts, expr } => self.count_all(stmts, expr.as_deref_mut()),
            IrExprKind::ForIn { body, .. } | IrExprKind::While { body, .. } => self.count_all(body, None),
            _ => {}
        }
    }
}

/// The bounds of `e` when it is `list.range(a, b)` over Ints.
pub(crate) fn range_call(e: &IrExpr) -> Option<(&IrExpr, &IrExpr)> {
    let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &e.kind else { return None; };
    let [start, end] = args.as_slice() else { return None; };
    (module.as_str() == "list" && func.as_str() == "range" && start.ty == Ty::Int && end.ty == Ty::Int).then_some((start, end))
}

/// Whether `list.range(start, end)` is too long to be built, when both
/// bounds are literals; `None` when only the run can tell.
pub(crate) fn unbuildable(start: &IrExpr, end: &IrExpr) -> Option<bool> {
    let (IrExprKind::LitInt { value: a }, IrExprKind::LitInt { value: b }) = (&start.kind, &end.kind) else { return None; };
    Some(a < b && b.checked_sub(*a).is_none_or(|n| n > MAX_LIST_LEN))
}

/// `if start < end and (end - start < 0 or end - start > MAX) then { let _ =
/// list.range(start, end) } else ()`: the range that cannot be a list is
/// built as before, which aborts; any other falls through. `start` and `end`
/// are read more than once: the caller hands expressions that allow it.
pub(crate) fn build_guard(start: &IrExpr, end: &IrExpr, vars: &mut VarTable) -> IrStmt {
    let int = |kind| node(kind, Ty::Int);
    let test = |op, left: IrExpr, right: IrExpr| node(IrExprKind::BinOp { op, left: Box::new(left), right: Box::new(right) }, Ty::Bool);
    let count = || int(IrExprKind::BinOp { op: BinOp::SubInt, left: Box::new(end.clone()), right: Box::new(start.clone()) });
    let wrapped = test(BinOp::Lt, count(), int(IrExprKind::LitInt { value: 0 }));
    let long = test(BinOp::Gt, count(), int(IrExprKind::LitInt { value: MAX_LIST_LEN }));
    let cond = test(BinOp::And, test(BinOp::Lt, start.clone(), end.clone()), test(BinOp::Or, wrapped, long));
    let list = Ty::Applied(C::List, vec![Ty::Int].into());
    let target = CallTarget::Module { module: sym("list"), func: sym("range"), def_id: None };
    let built = node(IrExprKind::Call { target, args: vec![start.clone(), end.clone()], type_args: Vec::new() }, list.clone());
    let unread = vars.alloc_fresh("unbuildable_range", list.clone(), Mutability::Let, None);
    let bind = IrStmt { kind: IrStmtKind::Bind { var: unread, mutability: Mutability::Let, ty: list, value: built }, span: None };
    let then = node(IrExprKind::Block { stmts: vec![bind], expr: None }, Ty::Unit);
    let guard = node(IrExprKind::If { cond: Box::new(cond), then: Box::new(then), else_: Box::new(node(IrExprKind::Unit, Ty::Unit)) }, Ty::Unit);
    IrStmt { kind: IrStmtKind::Expr { expr: guard }, span: None }
}

fn node(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}
