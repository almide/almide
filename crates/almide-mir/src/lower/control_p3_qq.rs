// ── the `??` lowering's decline / attempt helpers, include!-spliced beside control_p3.rs ──
//
// Extracted verbatim from `LowerCtx::try_lower_option_unwrap_or` (codopsy A:
// cog 43 in one frame) and `seed_variant_value_shape` (nesting depth 7).
// Every block below is the text that sat inline, taking exactly the values it
// used and returning exactly what it produced; the speculation rollbacks and
// the ALMIDE_DBG_QQ traces travel with their blocks.

impl LowerCtx {
    /// Verbatim from `seed_variant_value_shape`: the HeapOk-family Result drop
    /// route (the payload-class chain after the cap-as-tag read shape).
    fn seed_result_heap_ok_drop_route(&mut self, v: ValueId, ty: &Ty) {
        if crate::lower::is_result_listval_ty(ty) {
            self.value_drops.entry(v).or_default().value_result_list = true;
        } else if crate::lower::is_value_result_ty(ty) {
            self.value_drops.entry(v).or_default().value_result = true;
        } else if crate::lower::is_list_str_result_ty(ty) {
            self.value_drops.entry(v).or_default().list_str_result = true;
            self.value_drops.entry(v).or_default().flat_elems = true;
        } else if crate::lower::is_res_map_si_ty(ty)
            || crate::lower::is_res_list_map_si_ty(ty)
        {
            let route = if crate::lower::is_res_map_si_ty(ty) {
                "res_msi"
            } else {
                "res_lmsi"
            };
            self.value_drops.entry(v).or_default().named_route = Some(route.to_string());
            self.value_drops.entry(v).or_default().flat_elems = true;
        } else {
            self.value_drops.entry(v).or_default().flat_elems = true;
        }
    }

    /// Verbatim from `try_lower_option_unwrap_or`: the two record-payload
    /// declines (`Option[record]` / `Result[record, _]`). `true` = decline.
    fn unwrap_or_declines_record_payload(&self, expr: &IrExpr) -> bool {
        // An `Option[record]` operand (`list.get(tools, i) ?? { name: "", … }`) has NO faithful
        // `??` lowering yet: the Value-shaped `option.value_unwrap_or` corrupts a record field
        // block (both arms printed garbage / empty fields vs v0), and no other path here handles
        // it. DECLINE outright so the whole `??` walls cleanly (never a wrong byte) — a correct
        // record-payload unwrap-or is a follow-up. Gated to a record/anon-record Option payload.
        if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a) = &expr.ty
        {
            if a.len() == 1 && self.record_or_anon_drop_type_name(&a[0]).is_some() {
                return true;
            }
        }
        // The RESULT twin (#1582): a `Result[record, _] ?? <record fallback>`
        // taken here rode the value-position variant match, which ACCEPTS the
        // shape and misreads it — the ok arm bound an empty field, the err
        // arm garbage bytes, with no wall (the silent-wrong class). DECLINE
        // outright: the bind-position caller then rewrites to the explicit
        // `match e { ok(p) => p, err(_) => d }` through the PROVEN
        // bind-position heap-match machinery (measured byte-identical to the
        // user-written match on both targets), and a value-position `??`
        // walls honestly instead of fabricating a record. Gated on a
        // record-typed Ok payload (any registered record — an all-scalar
        // record's field read misreads the same way).
        if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, a) = &expr.ty
        {
            if a.len() == 2 {
                if let Ty::Named(n, _) = &a[0] {
                    if crate::lower::canonical_record_key(&self.record_layouts, n.as_str())
                        .is_some()
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Verbatim from `try_lower_option_unwrap_or`: the #1418 scalar match-first
    /// attempt (rewrite to the canonical match, then the subject-ANF retry),
    /// entered only for a non-heap, non-propagating fallback.
    fn try_lower_scalar_unwrap_or_match_first(
        &mut self,
        expr: &IrExpr,
        fallback: &IrExpr,
    ) -> Option<ValueId> {
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let synth = IrExpr {
            kind: IrExprKind::Unit,
            ty: fallback.ty.clone(),
            span: expr.span.clone(),
            def_id: None,
        };
        let rewritten = if expr.ty.is_result() {
            Some(Self::unwrap_or_as_result_match(&synth, expr, fallback))
        } else if expr.ty.is_option() {
            Some(Self::unwrap_or_as_option_match(&synth, expr, fallback))
        } else {
            None
        };
        if let Some(m) = rewritten {
            if let Some(v) = self.lower_scalar_match_operand(&m) {
                crate::trace::trace("ALMIDE_DBG_QQ", || {
                    format!("[qq] match-first took {:?} ?? (scalar)", expr.ty)
                });
                return Some(v);
            }
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
            // Subject-ANF retry (the scalar twin of the heap section's):
            // a CALL subject the scalar match cannot classify — a
            // FALLIBLE-CLOSURE carrier (`g("21") ?? -1`, ADR-0009), a
            // helper outside the name sets — binds fine through the
            // bind-position machinery (which owns the Computed-call
            // carrier arm), after which the match dispatches on a
            // tracked, type-seeded VAR. `speculate` rolls back a decline.
            let m2 = m.clone();
            let out = self.speculate(|ctx| {
                let IrExprKind::Match { subject, .. } = &m2.kind else { unreachable!() };
                if !matches!(subject.kind, IrExprKind::Call { .. }) {
                    return None;
                }
                let t = almide_ir::VarId(crate::lower::desugar_var_seed());
                ctx.lower_bind(t, &subject.ty, subject).ok()?;
                if let Ok(tval) = ctx.value_for(t) {
                    ctx.seed_variant_value_shape(tval, &subject.ty);
                }
                let tv = IrExpr {
                    kind: IrExprKind::Var { id: t },
                    ty: subject.ty.clone(),
                    span: subject.span.clone(),
                    def_id: None,
                };
                let IrExprKind::Match { arms, .. } = &m2.kind else { unreachable!() };
                let m3 = IrExpr {
                    kind: IrExprKind::Match { subject: Box::new(tv), arms: arms.clone() },
                    ty: m2.ty.clone(),
                    span: m2.span.clone(),
                    def_id: None,
                };
                ctx.lower_scalar_match_operand(&m3)
            });
            crate::trace::trace("ALMIDE_DBG_QQ", || match out {
                Some(_) => format!("[qq] match took {:?} ?? (scalar, anf)", expr.ty),
                None => format!("[qq] declined {:?} ?? (scalar)", expr.ty),
            });
            if out.is_some() {
                return out;
            }
        }
        None
    }

    /// Verbatim from `try_lower_option_unwrap_or`: the CLOSURE payload class
    /// (`Option[<Fn>] ?? <lambda>`), entered only for an `Option[Fn]` operand.
    fn try_lower_opt_fn_unwrap_or(
        &mut self,
        expr: &IrExpr,
        fallback: &IrExpr,
        track_result: bool,
    ) -> Option<ValueId> {
        if let IrExprKind::Lambda { params, body, .. } = &fallback.kind {
            let (params, body) = (params.clone(), body.clone());
            let out = self.speculate(|ctx| {
                use crate::PrimKind;
                let handle = match &expr.kind {
                    IrExprKind::Var { id } => ctx.value_for(*id).ok()?,
                    IrExprKind::Call { .. } => {
                        let t = almide_ir::VarId(crate::lower::desugar_var_seed());
                        ctx.lower_bind(t, &expr.ty, expr).ok()?;
                        ctx.value_for(t).ok()?
                    }
                    _ => return None,
                };
                let h = ctx.fresh_value();
                ctx.ops.push(Op::Prim {
                    kind: PrimKind::Handle,
                    dst: Some(h),
                    args: vec![handle],
                });
                let tag = ctx.load_at_offset(h, 4, PrimKind::Load { width: 4 });
                let result = ctx.fresh_value();
                ctx.ops.push(Op::IfThen { cond: tag, dst: Some(result) });
                let borrowed = ctx.load_at_offset(h, 12, PrimKind::LoadHandle);
                let owned = ctx.fresh_value();
                ctx.ops.push(Op::Dup { dst: owned, src: borrowed });
                ctx.ops.push(Op::Else { val: Some(owned) });
                let fb = ctx.lift_lambda(&params, &body)?;
                // The fallback block is ELSE-ARM-LOCAL and MOVES into
                // the merge — remove it from the scope-end drop set
                // (`lift_lambda` pushed it): an unconditional drop
                // would free a never-allocated local on the Some path.
                ctx.live_heap_handles.retain(|x| *x != fb);
                ctx.ops.push(Op::EndIf { val: Some(fb) });
                if track_result {
                    ctx.live_heap_handles.push(result);
                }
                ctx.closure_values.insert(result);
                Some(result)
            });
            if out.is_some() {
                return out;
            }
        }
        None
    }
}
