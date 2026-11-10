//! Native nested sequence patterns expressed as ordinary checked IR.
//! A row computes Option[bindings]; failed shape tests return None before
//! any payload/index read. Its success body and its next row each occur once.
use almide_ir::{visit_mut::{walk_expr_mut, IrMutVisitor}, *};
use almide_lang::{intern::sym, types::{constructor::TypeConstructorId as TC, Ty}};
use crate::lower::VariantLayouts;

pub(crate) fn expand(ir: &mut IrProgram, layouts: &VariantLayouts) {
    let mut pass = Patterns { vars: &mut ir.var_table, layouts };
    for f in &mut ir.functions { pass.visit_expr_mut(&mut f.body); }
    for t in &mut ir.top_lets { pass.visit_expr_mut(&mut t.value); }
}
struct Patterns<'a> { vars: &'a mut VarTable, layouts: &'a VariantLayouts }

fn node(kind: IrExprKind, ty: Ty) -> IrExpr { IrExpr { kind, ty, span: None, def_id: None } }
fn var(v: VarId, ty: &Ty) -> IrExpr { node(IrExprKind::Var { id: v }, ty.clone()) }
fn int(n: i64) -> IrExpr { node(IrExprKind::LitInt { value: n }, Ty::Int) }
fn branch(cond: IrExpr, yes: IrExpr, no: IrExpr) -> IrExpr {
    let ty = yes.ty.clone();
    node(IrExprKind::If { cond: Box::new(cond), then: Box::new(yes), else_: Box::new(no) }, ty)
}
fn bind(v: VarId, value: IrExpr, body: IrExpr) -> IrExpr {
    let ty = body.ty.clone();
    let stmt = IrStmt { kind: IrStmtKind::Bind { var: v, mutability: Mutability::Let,
        ty: value.ty.clone(), value }, span: None };
    node(IrExprKind::Block { stmts: vec![stmt], expr: Some(Box::new(body)) }, ty)
}
fn eq(a: IrExpr, b: IrExpr) -> IrExpr {
    node(IrExprKind::BinOp { op: BinOp::Eq, left: Box::new(a), right: Box::new(b) }, Ty::Bool)
}
fn literal_or_bind(p: &IrPattern) -> bool {
    matches!(p, IrPattern::Bind { .. } | IrPattern::Wildcard | IrPattern::Literal { .. })
}
fn sequence_inside(p: &IrPattern) -> bool {
    match p {
        IrPattern::Tuple { .. } | IrPattern::List { .. } => true,
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => sequence_inside(inner),
        IrPattern::Constructor { args, .. } => args.iter().any(sequence_inside),
        _ => false,
    }
}
fn needs(p: &IrPattern) -> bool {
    match p {
        IrPattern::Tuple { .. } => true,
        IrPattern::List { elements, rest: None } => elements.iter().any(|p| !literal_or_bind(p)),
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => sequence_inside(inner),
        _ => false,
    }
}
fn irrefutable(p: &IrPattern) -> bool {
    match p {
        IrPattern::Bind { .. } | IrPattern::Wildcard => true,
        IrPattern::Tuple { elements } => elements.iter().all(irrefutable),
        _ => false,
    }
}

// Exhaustive Boolean refinements need no wildcard row. Enumerate only the
// finite Boolean part of the declared type; other fields stay abstract and
// can be covered only by irrefutable patterns. Bound the expansion cost.
fn boolean_cases(ty: &Ty) -> Option<Vec<IrPattern>> {
    match ty {
        Ty::Bool => Some([false, true].into_iter().map(|value| IrPattern::Literal {
            expr: node(IrExprKind::LitBool { value }, Ty::Bool),
        }).collect()),
        Ty::Tuple(ts) => {
            let mut rows = vec![Vec::new()];
            for ty in ts.iter() {
                let cases = boolean_cases(ty)?;
                if rows.len().checked_mul(cases.len())? > 256 { return None; }
                rows = rows.into_iter().flat_map(|row| cases.iter().map(move |case| {
                    let mut next = row.clone(); next.push(case.clone()); next
                })).collect();
            }
            Some(rows.into_iter().map(|elements| IrPattern::Tuple { elements }).collect())
        }
        _ => Some(vec![IrPattern::Wildcard]),
    }
}
fn covers_boolean_case(p: &IrPattern, case: &IrPattern) -> bool {
    match (p, case) {
        (IrPattern::Wildcard | IrPattern::Bind { .. }, _) => true,
        (IrPattern::Literal { expr: p }, IrPattern::Literal { expr: c }) => {
            matches!((&p.kind, &c.kind), (IrExprKind::LitBool { value: a },
                IrExprKind::LitBool { value: b }) if a == b)
        }
        (IrPattern::Tuple { elements: ps }, IrPattern::Tuple { elements: cs }) =>
            ps.len() == cs.len() && ps.iter().zip(cs).all(|(p, c)| covers_boolean_case(p, c)),
        _ => false,
    }
}
fn boolean_exhaustive(ty: &Ty, arms: &[IrMatchArm]) -> bool {
    boolean_cases(ty).is_some_and(|cases| cases.iter().all(|case|
        arms.iter().any(|a| a.guard.is_none() && covers_boolean_case(&a.pattern, case))))
}
fn boolean_final_bindings(p: &IrPattern) -> IrPattern {
    match p {
        IrPattern::Literal { .. } => IrPattern::Wildcard,
        IrPattern::Tuple { elements } => IrPattern::Tuple {
            elements: elements.iter().map(boolean_final_bindings).collect(),
        },
        _ => p.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(first: bool, second: IrPattern, guard: Option<IrExpr>) -> IrMatchArm {
        IrMatchArm { pattern: IrPattern::Tuple { elements: vec![
            IrPattern::Literal { expr: node(IrExprKind::LitBool { value: first }, Ty::Bool) },
            second,
        ] }, guard, body: int(1) }
    }

    #[test]
    fn boolean_coverage_keeps_guards_and_non_boolean_refinements() {
        let ty = Ty::Tuple(vec![Ty::Bool, Ty::String].into());
        let mut arms = vec![row(true, IrPattern::Wildcard, None),
            row(false, IrPattern::Bind { var: VarId(0), ty: Ty::String }, None)];
        assert!(boolean_exhaustive(&ty, &arms));
        assert!(!boolean_exhaustive(&ty, &arms[..1]));
        arms[1].guard = Some(node(IrExprKind::LitBool { value: true }, Ty::Bool));
        assert!(!boolean_exhaustive(&ty, &arms));
        arms[1].guard = None;
        arms[1].pattern = row(false, IrPattern::Literal {
            expr: node(IrExprKind::LitStr { value: "only".into() }, Ty::String),
        }, None).pattern;
        assert!(!boolean_exhaustive(&ty, &arms));
        assert!(boolean_cases(&Ty::Tuple(vec![Ty::Bool; 9].into())).is_none());
        let pair = Ty::Tuple(vec![Ty::Bool, Ty::Bool].into());
        let cases = boolean_cases(&pair).unwrap();
        let all: Vec<_> = cases.into_iter().map(|pattern| IrMatchArm {
            pattern, guard: None, body: int(0),
        }).collect();
        assert!(boolean_exhaustive(&pair, &all));
        assert!(!boolean_exhaustive(&pair, &[all[0].clone(), all[3].clone()]));
        assert!(!boolean_exhaustive(&pair, &[all[0].clone(), all[0].clone()]));
    }
}
fn bindings(p: &IrPattern, out: &mut Vec<(VarId, Ty)>) -> Option<()> {
    match p {
        IrPattern::Bind { var, ty } => out.push((*var, ty.clone())),
        IrPattern::Wildcard | IrPattern::Literal { .. } | IrPattern::None => {},
        IrPattern::Tuple { elements } | IrPattern::List { elements, rest: None }
        | IrPattern::Constructor { args: elements, .. } => {
            for p in elements { bindings(p, out)?; }
        }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => bindings(inner, out)?,
        _ => return None,
    }
    Some(())
}
impl IrMutVisitor for Patterns<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        let IrExprKind::Match { subject, arms } = &e.kind else { return };
        if !arms.iter().any(|a| needs(&a.pattern)) { return; }
        let mark = self.vars.entries.len();
        if let Some(mut result) = self.option_rows(subject, arms)
            .or_else(|| self.rows(subject, arms)) {
            result.span = e.span; *e = result;
        } else { self.vars.entries.truncate(mark); }
    }
}
impl Patterns<'_> {
    fn fresh(&mut self, ty: &Ty) -> VarId { self.vars.alloc_fresh("pattern_value", ty.clone(), Mutability::Let, None) }
    // Disjoint Option tags can be selected before testing their payloads.
    // Preserve the order and guards of all Some rows, and evaluate the
    // subject once. This exposes exhaustive tuple refinements inside Some
    // without reading a missing payload or inventing a wildcard fallback.
    fn option_rows(&mut self, subject: &IrExpr, arms: &[IrMatchArm]) -> Option<IrExpr> {
        let Ty::Applied(TC::Option, ts) = &subject.ty else { return None };
        let [ty] = &ts[..] else { return None };
        let mut absent = None;
        let mut payload_rows = Vec::new();
        for arm in arms {
            match &arm.pattern {
                IrPattern::None if arm.guard.is_none() && absent.is_none() =>
                    absent = Some(arm.body.clone()),
                IrPattern::Some { inner } => payload_rows.push(IrMatchArm {
                    pattern: (**inner).clone(), guard: arm.guard.clone(), body: arm.body.clone(),
                }),
                _ => return None,
            }
        }
        if payload_rows.len() < 2 { return None; }
        let absent = absent?;
        let id = self.fresh(ty);
        let result_ty = absent.ty.clone();
        let mut present = node(IrExprKind::Match { subject: Box::new(var(id, ty)),
            arms: payload_rows }, result_ty.clone());
        self.visit_expr_mut(&mut present);
        Some(node(IrExprKind::Match { subject: Box::new(subject.clone()), arms: vec![
            IrMatchArm { pattern: IrPattern::Some { inner: Box::new(IrPattern::Bind {
                var: id, ty: ty.clone(),
            }) }, guard: None, body: present },
            IrMatchArm { pattern: IrPattern::None, guard: None, body: absent },
        ] }, result_ty))
    }
    fn rows(&mut self, subject: &IrExpr, arms: &[IrMatchArm]) -> Option<IrExpr> {
        let (last, init) = arms.split_last()?;
        if last.guard.is_some()
            || (!irrefutable(&last.pattern) && !boolean_exhaustive(&subject.ty, arms)) {
            return None;
        }
        let root = self.fresh(&subject.ty);
        let value = var(root, &subject.ty);
        // Exhaustiveness makes the final row unconditional. Keep its binders,
        // but do not manufacture a failed-test branch with unbound variables.
        let final_pattern = boolean_final_bindings(&last.pattern);
        let mut rest = self.pattern(&final_pattern, value.clone(), last.body.clone(), last.body.clone())?;
        for arm in init.iter().rev() {
            let mut bs = Vec::new(); bindings(&arm.pattern, &mut bs)?;
            // A dummy scalar keeps even a binder-free row's carrier concrete.
            let mut fields = vec![int(0)]; fields.extend(bs.iter().map(|(v, t)| var(*v, t)));
            let tuple_ty = Ty::Tuple(fields.iter().map(|e| e.ty.clone()).collect::<Vec<_>>().into());
            let option_ty = Ty::Applied(TC::Option, vec![tuple_ty.clone()].into());
            let tuple = node(IrExprKind::Tuple { elements: fields }, tuple_ty.clone());
            let some = node(IrExprKind::OptionSome { expr: Box::new(tuple) }, option_ty.clone());
            let none = node(IrExprKind::OptionNone, option_ty.clone());
            let success = if let Some(g) = &arm.guard { branch(g.clone(), some, none.clone()) } else { some };
            let candidate = self.pattern(&arm.pattern, value.clone(), success, none)?;
            let matched = self.fresh(&option_ty);
            let payload = self.fresh(&tuple_ty);
            let mut body = arm.body.clone();
            for (i, (v, ty)) in bs.iter().enumerate() {
                let field = node(IrExprKind::TupleIndex { object: Box::new(var(payload, &tuple_ty)), index: i + 1 }, ty.clone());
                body = almide_ir::substitute_var_in_expr(&body, *v, &field);
            }
            let ty = body.ty.clone();
            let choose = node(IrExprKind::Match { subject: Box::new(var(matched, &option_ty)), arms: vec![
                IrMatchArm { pattern: IrPattern::Some { inner: Box::new(IrPattern::Bind { var: payload, ty: tuple_ty }) }, guard: None, body },
                IrMatchArm { pattern: IrPattern::None, guard: None, body: rest },
            ] }, ty);
            rest = bind(matched, candidate, choose);
        }
        Some(bind(root, subject.clone(), rest))
    }
    fn pattern(&mut self, p: &IrPattern, value: IrExpr, yes: IrExpr, no: IrExpr) -> Option<IrExpr> {
        if !literal_or_bind(p) && !matches!(value.kind, IrExprKind::Var { .. }) {
            let id = self.fresh(&value.ty);
            let body = self.pattern(p, var(id, &value.ty), yes, no)?;
            return Some(bind(id, value, body));
        }
        match p {
            IrPattern::Wildcard => Some(yes),
            IrPattern::Bind { var, .. } => Some(bind(*var, value, yes)),
            IrPattern::Literal { expr } => Some(branch(eq(value, expr.clone()), yes, no)),
            IrPattern::Tuple { elements } => {
                let Ty::Tuple(ts) = &value.ty else { return None };
                if ts.len() != elements.len() { return None; }
                let fields: Vec<_> = ts.iter().enumerate().map(|(i, ty)|
                    node(IrExprKind::TupleIndex { object: Box::new(value.clone()), index: i }, ty.clone())).collect();
                self.fields(elements, fields, yes, no)
            }
            IrPattern::List { elements, rest: None } => {
                let Ty::Applied(TC::List, ts) = &value.ty else { return None };
                let [elem] = &ts[..] else { return None };
                let fields = elements.iter().enumerate().map(|(i, _)| node(IrExprKind::IndexAccess {
                    object: Box::new(value.clone()), index: Box::new(int(i as i64)),
                }, elem.clone())).collect();
                let yes = self.fields(elements, fields, yes, no.clone())?;
                let len = node(IrExprKind::Call { target: CallTarget::Module { module: sym("list"), func: sym("len"), def_id: None },
                    args: vec![value], type_args: vec![] }, Ty::Int);
                Some(branch(eq(len, int(elements.len() as i64)), yes, no))
            }
            IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
                let Ty::Applied(tc, ts) = &value.ty else { return None };
                let index = usize::from(matches!(p, IrPattern::Err { .. }));
                if !matches!((p, tc, ts.len()), (IrPattern::Some { .. }, TC::Option, 1)
                    | (IrPattern::Ok { .. } | IrPattern::Err { .. }, TC::Result, 2)) { return None; }
                let ty = ts[index].clone(); let id = self.fresh(&ty);
                let yes = self.pattern(inner, var(id, &ty), yes, no.clone())?;
                let payload = Box::new(IrPattern::Bind { var: id, ty });
                let head = match p { IrPattern::Some { .. } => IrPattern::Some { inner: payload },
                    IrPattern::Ok { .. } => IrPattern::Ok { inner: payload }, _ => IrPattern::Err { inner: payload } };
                Some(self.tag(value, head, yes, no))
            }
            IrPattern::None => Some(self.tag(value, IrPattern::None, yes, no)),
            IrPattern::Constructor { name, args } => {
                let (_, layout, case) = self.layouts.lookup_ctor_for(name, &value.ty)?;
                if !layout.generics.is_empty() || case.fields.len() != args.len() { return None; }
                let ts: Vec<_> = case.fields.iter().map(|(_, t)| t.clone()).collect();
                let ids: Vec<_> = ts.iter().map(|t| self.fresh(t)).collect();
                let fields = ids.iter().zip(&ts).map(|(v, t)| var(*v, t)).collect();
                let yes = self.fields(args, fields, yes, no.clone())?;
                let head = IrPattern::Constructor { name: name.clone(), args: ids.into_iter().zip(ts)
                    .map(|(var, ty)| IrPattern::Bind { var, ty }).collect() };
                Some(self.tag(value, head, yes, no))
            }
            _ => None,
        }
    }
    fn fields(&mut self, ps: &[IrPattern], values: Vec<IrExpr>, mut yes: IrExpr, no: IrExpr) -> Option<IrExpr> {
        for (p, value) in ps.iter().zip(values).rev() { yes = self.pattern(p, value, yes, no.clone())?; }
        Some(yes)
    }
    fn tag(&self, value: IrExpr, head: IrPattern, yes: IrExpr, no: IrExpr) -> IrExpr {
        let ty = yes.ty.clone();
        let other = if matches!(head, IrPattern::None) {
            IrPattern::Some { inner: Box::new(IrPattern::Wildcard) }
        } else { IrPattern::Wildcard };
        node(IrExprKind::Match { subject: Box::new(value), arms: vec![
            IrMatchArm { pattern: head, guard: None, body: yes },
            IrMatchArm { pattern: other, guard: None, body: no },
        ] }, ty)
    }
}
