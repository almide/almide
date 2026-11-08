// the effect-`!` desugar's return/tail-position recursion, include!-spliced back from
// desugar_unwrap.rs at module level (a code move under the 800-line ceiling the
// codopsy gate holds this crate to).

/// Recurse the effect-`!` desugar into RETURN/TAIL positions — an `if`/`match` arm body or a nested
/// block tail — so a stmt-`!` inside a branch (porta_init's `else { … fs.write(p, c)!; … }`,
/// signal_instance's nested if/else arm blocks) desugars to the same nested-match continuation. The
/// err-arm `err(e) => err(e)` propagates to the ENCLOSING fn's `Result[_, String]` return; the
/// continuation nests only in the ok-arm. Each arm/tail recurses INDEPENDENTLY (no duplication), so
/// `count_ir_calls` stays exact (`f()` once, continuation in one arm, `err(e)` is a ctor). HOLE-1
/// (admitted Ok-payload reprs only) is enforced inside the block path (`desugar_effect_unwrap_inner`).
/// Returns `None` if no `!` is reachable in a return position of `tail` (the body keeps its form).
fn desugar_tail_effect_unwrap(
    tail: &IrExpr,
    next_var: &mut u32,
    unit_main: bool,
    ret_is_result: bool,
) -> Option<IrExpr> {
    use almide_ir::{IrMatchArm, IrPattern};
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    match &tail.kind {
        // A nested block — its own stmts/tail may carry a stmt-`!`.
        IrExprKind::Block { .. } => {
            desugar_effect_unwrap_inner(tail, next_var, unit_main, ret_is_result)
        }
        // A BARE tail-position OPTION-`!` (`{ let o = r!; o! }`'s continuation, or a plainly
        // declared-Result fn's `{ let o: Option[Int] = some(42); o! }` / `{ …; list.get(xs, i)! }`)
        // — WITHOUT this desugar it falls through to `tail.rs`'s raw pass-through, which is a
        // correct no-op ONLY for a RESULT operand (same repr the fn already returns); an OPTION
        // operand has a DIFFERENT repr, so the pass-through silently returned the RAW Option
        // handle AS the Result — a CONFIRMED live wrong-value bug (v0 prints the payload, v1
        // printed `Error: `), reachable with NO auto-wrap involved at all. Desugar it into the
        // real none/some match (`match o { none => err("none"), some(v) => ok(v) }` — v0's
        // unwrap-of-none message), typed at the SYNTHESIZED `Result[T, String]` (from the Option's
        // own payload type — NOT `tail.ty`/`body.ty`, whose values proved unreliable across the
        // desugar fixpoint's interleaved probe/lowering streams in two prior reverted attempts;
        // `ret_is_result` is threaded EXPLICITLY from `lower_body_into` exactly like `unit_main`,
        // so the gate is a per-fn FACT, not a fragile tree-local type). A RESULT operand keeps
        // the (correct) pass-through; a non-Result-ABI fn (`unwrap_option_some`'s declared `->
        // Int`, where the raw payload IS the return) keeps its pre-existing path via the gate.
        IrExprKind::Unwrap { expr }
            if ret_is_result
                && matches!(&expr.ty,
                    Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1) =>
        {
            let Ty::Applied(TypeConstructorId::Option, a) = &expr.ty else { unreachable!() };
            let payload_ty = a[0].clone();
            let result_ty = Ty::result(payload_ty.clone(), Ty::String);
            let v = VarId(*next_var);
            *next_var += 1;
            let none_arm = IrMatchArm {
                pattern: IrPattern::None,
                guard: None,
                body: IrExpr {
                    kind: IrExprKind::ResultErr {
                        expr: Box::new(IrExpr {
                            kind: IrExprKind::LitStr { value: "none".into() },
                            ty: Ty::String,
                            span: tail.span.clone(),
                            def_id: None,
                        }),
                    },
                    ty: result_ty.clone(),
                    span: tail.span.clone(),
                    def_id: None,
                },
            };
            let some_arm = IrMatchArm {
                pattern: IrPattern::Some {
                    inner: Box::new(IrPattern::Bind { var: v, ty: payload_ty.clone() }),
                },
                guard: None,
                body: IrExpr {
                    kind: IrExprKind::ResultOk {
                        expr: Box::new(IrExpr {
                            kind: IrExprKind::Var { id: v },
                            ty: payload_ty,
                            span: tail.span.clone(),
                            def_id: None,
                        }),
                    },
                    ty: result_ty.clone(),
                    span: tail.span.clone(),
                    def_id: None,
                },
            };
            Some(IrExpr {
                kind: IrExprKind::Match {
                    subject: Box::new((**expr).clone()),
                    arms: vec![none_arm, some_arm],
                },
                ty: result_ty,
                span: tail.span.clone(),
                def_id: tail.def_id,
            })
        }
        // The void `main`'s TAIL loop (`{ let v = f()!; for … { let x = g()!; … } }`).
        IrExprKind::ForIn { .. } | IrExprKind::While { .. } if unit_main => {
            desugar_unit_main_loop(tail, next_var, ret_is_result)
        }
        IrExprKind::If { cond, then, else_ } => {
            let nt = desugar_tail_effect_unwrap(then, next_var, unit_main, ret_is_result);
            let ne = desugar_tail_effect_unwrap(else_, next_var, unit_main, ret_is_result);
            if nt.is_none() && ne.is_none() {
                return None;
            }
            Some(IrExpr {
                kind: IrExprKind::If {
                    cond: cond.clone(),
                    then: Box::new(nt.unwrap_or_else(|| (**then).clone())),
                    else_: Box::new(ne.unwrap_or_else(|| (**else_).clone())),
                },
                ty: tail.ty.clone(),
                span: tail.span.clone(),
                def_id: tail.def_id,
            })
        }
        IrExprKind::Match { subject, arms } => {
            let mut changed = false;
            let new_arms: Vec<almide_ir::IrMatchArm> = arms
                .iter()
                .map(|a| match desugar_tail_effect_unwrap(&a.body, next_var, unit_main, ret_is_result) {
                    Some(nb) => {
                        changed = true;
                        almide_ir::IrMatchArm {
                            pattern: a.pattern.clone(),
                            guard: a.guard.clone(),
                            body: nb,
                        }
                    }
                    None => a.clone(),
                })
                .collect();
            if !changed {
                return None;
            }
            Some(IrExpr {
                kind: IrExprKind::Match { subject: subject.clone(), arms: new_arms },
                ty: tail.ty.clone(),
                span: tail.span.clone(),
                def_id: tail.def_id,
            })
        }
        _ => None,
    }
}
