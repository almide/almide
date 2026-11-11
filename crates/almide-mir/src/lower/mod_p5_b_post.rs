// the TCO rewrite's post-loop dispatch and loop-state inits, include!-spliced back
// from mod_p5_b.rs at module level (a code move under the 800-line ceiling the
// codopsy gate holds this crate to).

/// The post-loop dispatch `if rk == 1 then base_1 else if … else base_N` —
/// recomputes the hit base from the carried params.
fn tco_post_dispatch(base_exprs: &[IrExpr], rk: VarId, ret_ty: &Ty) -> Option<IrExpr> {
    let mut post = base_exprs.last()?.clone();
    for (idx, base) in base_exprs.iter().enumerate().rev().skip(1) {
        post = tco_ir(
            IrExprKind::If {
                cond: Box::new(tco_eq_rk(rk, (idx + 1) as i64)),
                then: Box::new(base.clone()),
                else_: Box::new(post),
            },
            ret_ty.clone(),
        );
    }
    Some(post)
}

/// The loop-state init binds, in order: `[var idx = 0;] [var result = <empty>;]
/// var rk = 0;`. The result accumulator (when used) starts at an empty value of
/// the result type — a placeholder the first base case overwrites IN the loop;
/// declared mutable so the in-loop base assigns it.
fn tco_state_inits(
    idx_var: Option<VarId>,
    result_slot: Option<(VarId, IrExpr)>,
    rk: VarId,
    ret_ty: &Ty,
) -> Vec<IrStmt> {
    let mut inits: Vec<IrStmt> = Vec::new();
    if let Some(iv) = idx_var {
        inits.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: iv,
                mutability: almide_ir::Mutability::Var,
                ty: Ty::Int,
                value: tco_ir(IrExprKind::LitInt { value: 0 }, Ty::Int),
            },
            span: None,
        });
    }
    if let Some((rv, empty)) = result_slot {
        inits.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: rv,
                mutability: almide_ir::Mutability::Var,
                ty: ret_ty.clone(),
                value: empty,
            },
            span: None,
        });
    }
    inits.push(IrStmt {
        kind: IrStmtKind::Bind {
            var: rk,
            mutability: almide_ir::Mutability::Var,
            ty: Ty::Int,
            value: tco_ir(IrExprKind::LitInt { value: 0 }, Ty::Int),
        },
        span: None,
    });
    inits
}

/// A declared type's fields, resolved by the caller's layout registry: the
/// names (empty for a tuple) and the instantiated field types.
type TcoFieldsOf<'a> = &'a dyn Fn(&Ty) -> Option<(Vec<almide_lang::intern::Sym>, Vec<Ty>)>;

/// How deep [`tco_slot_empty`] follows records nested in records.
const TCO_SLOT_EMPTY_DEPTH: u8 = 8;

/// The in-loop result slot's initial value: [`tco_empty_for`]'s, or — on the
/// native render's owned slots only — a RECORD built fieldwise from its
/// declaration, each field at its own empty value. A placeholder like the
/// others: a base overwrites the slot before the loop ends, and the overwrite
/// drops it. `None` when a field has no empty value (the fn keeps its wall).
fn tco_slot_empty(ty: &Ty, fields_of: TcoFieldsOf<'_>, depth: u8) -> Option<IrExpr> {
    if let Some(e) = tco_empty_for(ty) {
        return Some(e);
    }
    if !crate::lower::owned_tco_slots() || depth == 0 {
        return None;
    }
    let name = match ty {
        Ty::Named(n, _) => Some(*n),
        Ty::Record { .. } => None,
        _ => return None,
    };
    let (names, tys) = fields_of(ty)?;
    let fields = names
        .into_iter()
        .zip(tys.iter())
        .map(|(n, t)| Some((n, tco_slot_empty(t, fields_of, depth - 1)?)))
        .collect::<Option<Vec<_>>>()?;
    Some(tco_ir(IrExprKind::Record { name, fields }, ty.clone()))
}
