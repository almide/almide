//! List-rest and as-patterns on the MIR rung (#3058 class 2, #1461).
//!
//! A top-level as-arm `x @ P => A` becomes `P => A[x := subject]` first (the
//! binder names the whole subject), so the as-arms of any match — a variant
//! match included — reach the lowering's own pattern machinery.
//!
//! `match xs { [h, ..t] => A, .. }` relaxes the length test to `>=` and binds
//! the tail past the prefix as a fresh list of the subject's type. The MIR
//! bind lowering has no tail-slice machinery, so [`desugar_list_rest_matches`]
//! rewrites a `match` over a list subject that has a rest arm into the chain
//! the lowering already runs — a length test, element reads under it, and the
//! tail as `list.drop(xs, k)`:
//!
//! ```text
//! { let s = xs; let n = list.len(s);
//!   if n == 0 then 0
//!   else if n >= 1 then { let h = s[0]; let t = list.drop(s, 1); A }
//!   else <unreachable: the checker proved the match exhaustive> }
//! ```
//!
//! Only arms whose pattern is a Bind / Wildcard, or a list pattern whose
//! elements are Bind / Wildcard / Literal with a Bind / Wildcard rest, are
//! admitted; any other shape leaves the match to the lowering's honest wall.
//! An arm with a guard binds, then tests the guard, and falls through to the
//! rest of the chain when it fails — the chain after a guarded arm is emitted
//! once per guard, so at most two guarded arms are admitted. The last arm
//! needs no test when unguarded: the match is exhaustive, so it is the one
//! that matches whenever it is reached.

use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use almide_ir::{BinOp, CallTarget, IrExpr, IrExprKind, IrMatchArm, IrPattern, IrStmt, IrStmtKind, Mutability, VarId, VarTable};
use almide_lang::intern::sym;
use almide_lang::types::constructor::TypeConstructorId;
use almide_lang::types::Ty;

fn node(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}

fn int(v: i64) -> IrExpr {
    node(IrExprKind::LitInt { value: v }, Ty::Int)
}

fn var(id: VarId, ty: &Ty) -> IrExpr {
    node(IrExprKind::Var { id }, ty.clone())
}

fn binop(op: BinOp, l: IrExpr, r: IrExpr) -> IrExpr {
    node(IrExprKind::BinOp { op, left: Box::new(l), right: Box::new(r) }, Ty::Bool)
}

fn list_call(func: &str, args: Vec<IrExpr>, ty: Ty) -> IrExpr {
    let target = CallTarget::Module { module: sym("list"), func: sym(func), def_id: None };
    node(IrExprKind::Call { target, args, type_args: Vec::new() }, ty)
}

fn bind(var: VarId, ty: &Ty, value: IrExpr) -> IrStmt {
    IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty: ty.clone(), value }, span: None }
}

fn block(stmts: Vec<IrStmt>, tail: IrExpr) -> IrExpr {
    let ty = tail.ty.clone();
    if stmts.is_empty() {
        return tail;
    }
    node(IrExprKind::Block { stmts, expr: Some(Box::new(tail)) }, ty)
}

/// The list pattern of an arm as (prefix elements, rest), or `None` for a
/// Bind / Wildcard arm. `Err(())` rejects the whole match.
#[allow(clippy::type_complexity)]
fn arm_shape(p: &IrPattern) -> Result<Option<(&[IrPattern], Option<&IrPattern>)>, ()> {
    match p {
        IrPattern::Wildcard | IrPattern::Bind { .. } => Ok(None),
        IrPattern::List { elements, rest } => {
            let elems_ok = elements
                .iter()
                .all(|e| matches!(e, IrPattern::Bind { .. } | IrPattern::Wildcard | IrPattern::Literal { .. }));
            let rest_ok = rest.as_deref().is_none_or(|r| matches!(r, IrPattern::Bind { .. } | IrPattern::Wildcard));
            if elems_ok && rest_ok {
                Ok(Some((elements.as_slice(), rest.as_deref())))
            } else {
                Err(())
            }
        }
        _ => Err(()),
    }
}

fn admits(arms: &[IrMatchArm]) -> bool {
    let has_rest = arms.iter().any(|a| matches!(&a.pattern, IrPattern::List { rest: Some(_), .. }));
    let guarded = arms.iter().filter(|a| a.guard.is_some()).count();
    let last_unguarded = arms.last().is_some_and(|a| a.guard.is_none());
    has_rest && last_unguarded && guarded <= 2 && arms.iter().all(|a| arm_shape(&a.pattern).is_ok())
}

struct Desugar<'a> {
    vt: &'a mut VarTable,
}

impl Desugar<'_> {
    fn fresh(&mut self, name: &str, ty: &Ty) -> VarId {
        self.vt.alloc(sym(name), ty.clone(), Mutability::Let, None)
    }

    /// The test, binds, and guard of one arm against subject `s` of length `n`.
    fn arm_parts(&mut self, arm: &IrMatchArm, s: &IrExpr, n: &IrExpr, elem_ty: &Ty) -> (Option<IrExpr>, Vec<IrStmt>) {
        let mut binds = Vec::new();
        let Some((elements, rest)) = arm_shape(&arm.pattern).ok().flatten() else {
            if let IrPattern::Bind { var, ty } = &arm.pattern {
                binds.push(bind(*var, ty, s.clone()));
            }
            return (None, binds);
        };
        let k = elements.len() as i64;
        let op = if rest.is_some() { BinOp::Gte } else { BinOp::Eq };
        let mut test = binop(op, n.clone(), int(k));
        for (i, e) in elements.iter().enumerate() {
            let read = node(IrExprKind::IndexAccess { object: Box::new(s.clone()), index: Box::new(int(i as i64)) }, elem_ty.clone());
            match e {
                IrPattern::Bind { var, ty } => binds.push(bind(*var, ty, read)),
                IrPattern::Literal { expr } => {
                    // The element read sits under the length test (`and` short-circuits).
                    test = binop(BinOp::And, test, binop(BinOp::Eq, read, expr.clone()));
                }
                _ => {}
            }
        }
        if let Some(IrPattern::Bind { var, ty }) = rest {
            binds.push(bind(*var, ty, list_call("drop", vec![s.clone(), int(k)], s.ty.clone())));
        }
        (Some(test), binds)
    }

    /// The if-chain for `arms[i..]`.
    fn chain(&mut self, arms: &[IrMatchArm], s: &IrExpr, n: &IrExpr, elem_ty: &Ty, out_ty: &Ty) -> IrExpr {
        // `admits` requires an unguarded last arm, which ends every chain.
        let (arm, rest) = arms.split_first().expect("a chain ends at its unguarded last arm");
        let (test, binds) = self.arm_parts(arm, s, n, elem_ty);
        let last = rest.is_empty();
        let taken = match &arm.guard {
            None => block(binds.clone(), arm.body.clone()),
            Some(g) => {
                let fall = self.chain(rest, s, n, elem_ty, out_ty);
                let inner = node(
                    IrExprKind::If { cond: Box::new(g.clone()), then: Box::new(arm.body.clone()), else_: Box::new(fall) },
                    out_ty.clone(),
                );
                block(binds.clone(), inner)
            }
        };
        match test {
            Some(t) if !(last && arm.guard.is_none()) => {
                let fall = self.chain(rest, s, n, elem_ty, out_ty);
                node(IrExprKind::If { cond: Box::new(t), then: Box::new(taken), else_: Box::new(fall) }, out_ty.clone())
            }
            _ => taken,
        }
    }

    fn rewrite(&mut self, expr: &mut IrExpr) -> bool {
        let IrExprKind::Match { subject, arms } = &expr.kind else { return false };
        let Ty::Applied(TypeConstructorId::List, a) = &subject.ty else { return false };
        let [elem_ty] = a.as_slice() else { return false };
        if !admits(arms) {
            return false;
        }
        let (elem_ty, list_ty, out_ty) = (elem_ty.clone(), subject.ty.clone(), expr.ty.clone());
        let mut stmts = Vec::new();
        let s = match &subject.kind {
            IrExprKind::Var { .. } => (**subject).clone(),
            _ => {
                let v = self.fresh("__rest_subject", &list_ty);
                stmts.push(bind(v, &list_ty, (**subject).clone()));
                var(v, &list_ty)
            }
        };
        let n_var = self.fresh("__rest_len", &Ty::Int);
        stmts.push(bind(n_var, &Ty::Int, list_call("len", vec![s.clone()], Ty::Int)));
        let n = var(n_var, &Ty::Int);
        let arms = arms.clone();
        let chain = self.chain(&arms, &s, &n, &elem_ty, &out_ty);
        *expr = block(stmts, chain);
        true
    }
}

impl Desugar<'_> {
    /// `x @ P => A` at an arm's top level → `P => A[x := s]`: the as-binder
    /// names the whole subject, so every use of it reads the subject itself (a
    /// non-Var subject is bound to a fresh var first, so it evaluates once).
    /// Returns the match node to go on with (inside the new block, if any).
    fn strip_as_arms<'e>(&mut self, expr: &'e mut IrExpr) -> &'e mut IrExpr {
        let IrExprKind::Match { subject, arms } = &mut expr.kind else { return expr };
        if !arms.iter().any(|a| matches!(a.pattern, IrPattern::As { .. })) {
            return expr;
        }
        let hoisted = if matches!(subject.kind, IrExprKind::Var { .. }) {
            None
        } else {
            let ty = subject.ty.clone();
            let v = self.fresh("__as_subject", &ty);
            let value = std::mem::replace(&mut **subject, var(v, &ty));
            Some(bind(v, &ty, value))
        };
        let s = (**subject).clone();
        for arm in arms.iter_mut() {
            let IrPattern::As { var: x, inner, .. } = &arm.pattern else { continue };
            let (x, inner) = (*x, (**inner).clone());
            arm.pattern = inner;
            arm.guard = arm.guard.as_ref().map(|g| almide_ir::substitute_var_in_expr(g, x, &s));
            arm.body = almide_ir::substitute_var_in_expr(&arm.body, x, &s);
        }
        let Some(stmt) = hoisted else { return expr };
        let matched = std::mem::take(expr);
        *expr = block(vec![stmt], matched);
        let IrExprKind::Block { expr: Some(tail), .. } = &mut expr.kind else { unreachable!() };
        tail
    }
}

impl IrMutVisitor for Desugar<'_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        walk_expr_mut(self, expr);
        let m = self.strip_as_arms(expr);
        self.rewrite(m);
    }
}

/// Rewrite every list-rest `match` of the program (see the module doc).
pub fn desugar_list_rest_matches(program: &mut almide_ir::IrProgram) {
    let almide_ir::IrProgram { functions, modules, var_table, .. } = program;
    let mut d = Desugar { vt: var_table };
    for f in functions.iter_mut() {
        d.visit_expr_mut(&mut f.body);
    }
    for m in modules.iter_mut() {
        let mut d = Desugar { vt: &mut m.var_table };
        for f in m.functions.iter_mut() {
            d.visit_expr_mut(&mut f.body);
        }
    }
}
