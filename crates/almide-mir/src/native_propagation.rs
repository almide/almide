//! Move a branch's immediate propagation to the branch result boundary.
//! `match x { p => r!, q => v }` becomes
//! `(match x { p => r, q => ok(v) })!`. Only the selected arm runs, and
//! the existing Result bind/return rules account for the selected carrier.
use almide_ir::{visit_mut::{walk_expr_mut, walk_stmt_mut, IrMutVisitor}, *};
use almide_lang::types::Ty;

pub(crate) fn expand(ir: &mut IrProgram) {
    crate::native_guard_continuation::expand(ir);
    let mut pass = Propagation { vars: &mut ir.var_table };
    for f in &mut ir.functions { pass.visit_expr_mut(&mut f.body); }
    for t in &mut ir.top_lets { pass.visit_expr_mut(&mut t.value); }
}
struct Propagation<'a> { vars: &'a mut VarTable }

impl Propagation<'_> {
    fn interpolation_operands(&mut self, e: &mut IrExpr) {
        let IrExprKind::StringInterp { parts } = &mut e.kind else { return; };
        let needs_bind = parts.iter().any(|p| {
            let almide_ir::IrStringPart::Expr { expr } = p else { return false; };
            let IrExprKind::BinOp { op: BinOp::Eq | BinOp::Neq, left, right } = &expr.kind else { return false; };
            [left, right].iter().any(|v| matches!(v.kind, IrExprKind::UnwrapOr { .. }) && crate::lower::is_heap_ty(&v.ty))
        });
        if !needs_bind { return; }
        let mut stmts = Vec::new();
        // Bind every fragment in order. The ordinary operand desugar then
        // handles each heap fallback inside its own binding, after all earlier
        // fragments, and preserves lazy evaluation of the fallback itself.
        for p in parts {
            if let almide_ir::IrStringPart::Expr { expr } = p { *expr = self.bind(expr.clone(), &mut stmts); }
        }
        let tail = e.clone();
        e.kind = IrExprKind::Block { stmts, expr: Some(Box::new(tail)) };
    }

    fn bind(&mut self, value: IrExpr, stmts: &mut Vec<IrStmt>) -> IrExpr {
        let var = self.vars.alloc_fresh("propagation_operand", value.ty.clone(), Mutability::Let, None);
        let read = IrExpr { kind: IrExprKind::Var { id: var }, ty: value.ty.clone(), span: value.span, def_id: None };
        stmts.push(IrStmt { span: value.span, kind: IrStmtKind::Bind {
            var, mutability: Mutability::Let, ty: value.ty.clone(), value,
        } });
        read
    }

    fn spread_operands(&mut self, e: &mut IrExpr) {
        let IrExprKind::SpreadRecord { base, fields } = &mut e.kind else { return; };
        let last = fields.iter().rposition(|(_, value)| propagates(value));
        let borrowed_base = matches!(base.kind, IrExprKind::Var { .. }
            | IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. });
        if last.is_none() && borrowed_base { return; }
        let mut stmts = Vec::new();
        // Snapshot the base and all earlier fields before any later operand
        // can fail or rebind a value they read. The container is built last.
        **base = self.bind((**base).clone(), &mut stmts);
        if let Some(last) = last {
            for (_, value) in &mut fields[..=last] { *value = self.bind(value.clone(), &mut stmts); }
        }
        let tail = e.clone();
        e.kind = IrExprKind::Block { stmts, expr: Some(Box::new(tail)) };
    }

    fn result_payload_block(&mut self, e: &mut IrExpr) {
        let IrExprKind::ResultOk { expr: payload } = &e.kind else { return; };
        let IrExprKind::Block { stmts, expr: Some(value) } = &payload.kind else { return; };
        if !propagates(payload) { return; }
        // A fallible callback's success wrapper must be built after the
        // payload's statements. Their `!` exits the callback on failure.
        let tail = IrExpr { kind: IrExprKind::ResultOk { expr: value.clone() },
            ty: e.ty.clone(), span: e.span, def_id: e.def_id };
        e.kind = IrExprKind::Block { stmts: stmts.clone(), expr: Some(Box::new(tail)) };
    }

    fn tuple_literal_reader(&mut self, e: &mut IrExpr) {
        let IrExprKind::TupleIndex { object, .. } = &mut e.kind else { return; };
        if !matches!(object.kind, IrExprKind::Tuple { .. }) { return; }
        // Materialize every element in order, including the unselected ones.
        // The normal tuple bind/read/drop path then owns this temporary.
        let mut stmts = Vec::new();
        **object = self.bind((**object).clone(), &mut stmts);
        let tail = e.clone();
        e.kind = IrExprKind::Block { stmts, expr: Some(Box::new(tail)) };
    }
}

fn unwrapped(e: &IrExpr) -> Option<&IrExpr> {
    match &e.kind {
        IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => Some(expr),
        _ => None,
    }
}
fn result_type(arms: &[&IrExpr], payload: &Ty) -> Option<Ty> {
    let mut found = None;
    for a in arms {
        if let Some(inner) = unwrapped(a) {
            if inner.ty.result_ok_ty().as_ref() != Some(payload) { return None; }
            if found.as_ref().is_some_and(|t| *t != inner.ty) { return None; }
            found = Some(inner.ty.clone());
        }
    }
    found
}
fn wrap(a: &mut IrExpr, ty: &Ty) {
    if let Some(inner) = unwrapped(a) { *a = inner.clone(); }
    else {
        let old = a.clone();
        a.kind = IrExprKind::ResultOk { expr: Box::new(old) };
        a.ty = ty.clone();
    }
}
pub(crate) fn propagates(e: &IrExpr) -> bool {
    struct Scan(bool);
    impl almide_ir::IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e.kind, IrExprKind::Lambda { .. }) { return; }
            if unwrapped(e).is_some() { self.0 = true; }
            almide_ir::walk_expr(self, e);
        }
    }
    let mut scan = Scan(false);
    almide_ir::IrVisitor::visit_expr(&mut scan, e);
    scan.0
}

/// Native heap Result rewraps use the balanced owned-carrier join. Keep
/// scalar Result joins and arms that compute a new carrier on the older
/// continuation path until their frame/drop accounting supports a join.
pub(crate) fn joins_owned_carrier(e: &IrExpr) -> bool {
    e.ty.result_ok_ty().is_some_and(|t| crate::lower::is_heap_ty(&t))
        && matches!(&e.kind, IrExprKind::Match { subject, arms }
            if crate::lower::is_result_ty(&subject.ty) && arms.iter().all(|a|
                matches!(a.body.kind, IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. })))
        && !propagates(e)
}
impl IrMutVisitor for Propagation<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        self.spread_operands(e);
        self.result_payload_block(e);
        self.tuple_literal_reader(e);
        self.interpolation_operands(e);
        let result = match &e.kind {
            IrExprKind::Match { arms, .. } => result_type(&arms.iter().map(|a| &a.body).collect::<Vec<_>>(), &e.ty),
            _ => None,
        };
        let Some(ty) = result else { return; };
        let mut branch = e.clone();
        match &mut branch.kind {
            IrExprKind::Match { arms, .. } => for a in arms { wrap(&mut a.body, &ty); },
            _ => unreachable!(),
        }
        branch.ty = ty;
        e.kind = IrExprKind::Unwrap { expr: Box::new(branch) };
    }

    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        walk_stmt_mut(self, s);
        let IrStmtKind::FieldAssign { target, field, value } = &s.kind else { return; };
        if !propagates(value) { return; }
        // Expose the RHS to the normal bind-position propagation pass before
        // a store reads the current record. A failing RHS never stores.
        let var = self.vars.alloc_fresh("field_rhs", value.ty.clone(), Mutability::Let, None);
        let read = IrExpr { kind: IrExprKind::Var { id: var }, ty: value.ty.clone(), span: value.span, def_id: None };
        let stmts = vec![
            IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let,
                ty: value.ty.clone(), value: value.clone() }, span: value.span },
            IrStmt { kind: IrStmtKind::FieldAssign { target: *target, field: *field, value: read }, span: s.span },
        ];
        s.kind = IrStmtKind::Expr { expr: IrExpr {
            kind: IrExprKind::Block { stmts, expr: None }, ty: Ty::Unit, span: s.span, def_id: None,
        } };
    }
}
