// ── emitter.rs, value-position control flow and the sum-shape test ──
//
// include!-spliced into `emitter.rs` at module level (the 800-line file
// discipline). The imports are emitter.rs's own.

impl Emitter<'_> {
    /// Value-position control flow (`if`, `match`) — the block type comes
    /// from the hint or the checker's annotation. Split from `lower` for
    /// complexity budget; the `want` check happens in `lower`'s tail.
    pub(crate) fn lower_control(
        &mut self,
        e: &IrExpr,
        want: Option<SliceTy>,
        tail: bool,
    ) -> Result<SliceTy, EmitError> {
        let got = match &e.kind {
            // Value-position `if`: the arm type comes from the hint or is
            // inferred WITHOUT emitting (wasm wants the block type up
            // front), then both arms are lowered against it.
            // Both arms are lowered against it by `lower_if_arms`, which
            // also gives each arm the same credit count (#2317).
            IrExprKind::If { cond, .. } => {
                self.lower(cond, Some(BOOL))?;
                let ty = match want {
                    Some(w) => w,
                    None => self.infer(e)?,
                };
                self.lower_if_arms(e, ty, tail)?;
                ty
            }
            IrExprKind::Match { subject, arms } => {
                let ty = match want {
                    Some(w) => w,
                    None => self.infer(e)?,
                };
                self.lower_match_at(subject, arms, Some(ty), tail)?;
                ty
            }
            other => return unsup(&format!("expr:{}", expr_kind_name(other))),
        };
        Ok(got)
    }
}

/// Kinds `lower_sum` owns (constructors and unwraps).
fn is_sum_shape(k: &IrExprKind) -> bool {
    matches!(
        k,
        IrExprKind::OptionNone
            | IrExprKind::OptionSome { .. }
            | IrExprKind::ResultOk { .. }
            | IrExprKind::ResultErr { .. }
            // `?` and `!` are ONE oracle marker (eval_try_unwrap) and
            // lower_sum handles Try | Unwrap in one arm — the routing
            // predicate omitting Try was the entire ×15 "expr:Try" wall.
            | IrExprKind::Try { .. }
            | IrExprKind::Unwrap { .. }
            | IrExprKind::UnwrapOr { .. }
            | IrExprKind::ToOption { .. }
    )
}
