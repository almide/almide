/// Both arm-2 and arm-4 guards below are multi-clause booleans (each `&&`/
/// nested `matches!` is its own decision point to the complexity scorer);
/// naming them as standalone predicates collapses each guard to a single
/// call in the router without changing which arm fires for any input —
/// pure "extract the boolean, keep the shape" refactor.
fn is_scalar_result_err_target(expr_ty: &Ty, ty: &Ty) -> bool {
    !is_heap_ty(expr_ty)
        && matches!(ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, a)
                if a.len() == 2 && !is_heap_ty(&a[0]) && !is_heap_ty(&a[1]))
}

impl LowerCtx {
    fn is_heap_err_variant_ctor_target(&self, value: &IrExpr, ty: &Ty) -> bool {
        if let IrExprKind::ResultErr { expr } = &value.kind {
            is_heap_ty(&expr.ty)
                && !matches!(&expr.kind, IrExprKind::Var { .. })
                && self.is_heap_ok_variant_err_result(ty)
        } else {
            false
        }
    }

    /// Name router — arm bodies (2 of the 4 arms have real bodies; the other
    /// 2 already delegate to a single named call) moved to helpers, same
    /// "uniform match arm" split as binds_p4_b.rs above.
    fn try_lower_result_small_arms(&mut self, value: &IrExpr, ty: &Ty) -> Option<ValueId> {
        match &value.kind {
            IrExprKind::ResultOk { expr } if !is_heap_ty(&expr.ty) => {
                self.try_lower_result_ok_scalar(expr, ty)
            }
            IrExprKind::ResultErr { expr } if is_scalar_result_err_target(&expr.ty, ty) => {
                self.try_lower_result_err_scalar(expr, ty)
            }
            IrExprKind::ResultErr { .. } if self.is_scalar_ok_variant_err_result(ty) => {
                self.try_lower_result_err_variant_ctor(value, ty)
            }
            _ if self.is_heap_err_variant_ctor_target(value, ty) => {
                self.try_lower_result_err_variant_ctor_heap_ok(value, ty)
            }
            _ => None,
        }
    }

    fn try_lower_result_ok_scalar(&mut self, expr: &IrExpr, ty: &Ty) -> Option<ValueId> {
        let payload = self.lower_scalar_value(expr)?;
        let repr = repr_of(ty).ok()?;
        let dst = self.materialize_result_ok(payload, repr);
        self.materialized_results.insert(dst);
        Some(dst)
    }

    fn try_lower_result_err_scalar(&mut self, expr: &IrExpr, ty: &Ty) -> Option<ValueId> {
        let payload = self.lower_scalar_value(expr)?;
        let repr = repr_of(ty).ok()?;
        let dst = self.materialize_result_err_scalar(payload, repr);
        self.materialized_results.insert(dst);
        Some(dst)
    }

    /// Outer name router — unchanged. The guarded arm's WHOLE body (not just
    /// the inner match) moved to [`Self::result_err_heap_ok_result_body`]:
    /// one inner arm (`err(["a", …])`) has its own early `return Some(dst)`
    /// that bypasses the trailing `materialize_result_str` call and reads
    /// the outer `repr` — extracting only the inner match would have let
    /// that `return` escape to the wrong function and silently double-
    /// materialize. Moving the ENTIRE body keeps that early return's target
    /// (now the helper, exactly mirroring the old outer function) identical.
    fn try_lower_result_err_heap_ok_result(&mut self, value: &IrExpr, ty: &Ty) -> Option<ValueId> {
        match &value.kind {
            IrExprKind::ResultErr { expr }
                if is_heap_ty(&expr.ty) && Self::is_heap_ok_result(ty) =>
            {
                self.result_err_heap_ok_result_body(expr, ty)
            }
            _ => None,
        }
    }

    fn result_err_heap_ok_result_body(&mut self, expr: &IrExpr, ty: &Ty) -> Option<ValueId> {
        let repr = repr_of(ty).ok()?;
        // `err(["a", "b"])` — a `List[String]` LITERAL payload (the result.collect
        // Err side, `Result[List[Int], List[String]]`): the inner list builds
        // fresh-owned; the Result block's flat DropListStr would free slot-0 as a
        // STRING (leaking the inner list's elements), so RECLASSIFY the drop below
        // to the recursive list-of-list-str free. The ONLY arm not shared with
        // [`Self::result_err_heap_fallback_piece`] (checked first, same order as
        // the original single match), so it stays a standalone early-return arm
        // rather than folding into [`Self::lower_heap_err_common_piece`].
        if matches!(&expr.kind, IrExprKind::List { .. })
            && matches!(&expr.ty,
                        Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, i)
                            if i.len() == 1 && matches!(i[0], Ty::String))
        {
            let obj = self.try_lower_str_list_literal(expr)?;
            let dst = self.materialize_result_str(obj, repr, true, false);
            self.heap_elem_lists.remove(&dst);
            self.list_list_str_lists.insert(dst);
            return Some(dst);
        }
        let piece = self.lower_heap_err_common_piece(expr)?;
        Some(self.materialize_result_str(piece, repr, true, false))
    }

    /// Outer name router — unchanged; the inner "which construction strategy
    /// does this heap Err payload's `expr.kind` need" match moved to
    /// [`Self::result_err_heap_fallback_piece`] (no early-return-with-
    /// different-shape arm here, so the plain inner-match extraction — same
    /// as [`Self::try_lower_result_ok_heap`] above — applies).
    fn try_lower_result_err_heap_fallback(&mut self, value: &IrExpr, ty: &Ty) -> Option<ValueId> {
        match &value.kind {
            IrExprKind::ResultErr { expr } if is_heap_ty(&expr.ty) => {
                let repr = repr_of(ty).ok()?;
                let piece = self.result_err_heap_fallback_piece(expr)?;
                let dst = self.materialize_opt_str_some(piece, repr);
                // materialize_opt_str_some registers the OPTION read-shape; this value is
                // a RESULT (len-as-tag, Err = len 1) — a reader that keeps both entries
                // resolves it as an Option (`is_result = results ∧ ¬options`) and takes
                // the Err payload as a Some payload (`err("x") ?? 0` returned the String
                // HANDLE — result_option_matrix's "if with ??"). Result-only tracking.
                self.materialized_options.remove(&dst);
                self.materialized_results.insert(dst);
                Some(dst)
            }
            _ => None,
        }
    }

    /// A FRESH owned message only — a LitStr alloc, a Named-call result, or an OWNED
    /// `Var` (one in `live_heap_handles` — a freshly-built/closure-returned String, NOT
    /// a BORROWED param). Consuming a borrow into the Err would move out a value the
    /// caller still owns (a double-free the checker rejects), so a borrowed `Var` falls
    /// through to the sound deferred `Opaque`. Delegates entirely to
    /// [`Self::lower_heap_err_common_piece`] — this Result's scalar-Ok Err payload
    /// shapes are IDENTICAL to [`Self::result_err_heap_ok_result_body`]'s (both build a
    /// fresh-owned or Dup'd heap piece from the same `expr.kind`); only the two callers'
    /// surrounding materialize call differs, so the piece-construction match itself is
    /// factored once rather than duplicated per caller.
    fn result_err_heap_fallback_piece(&mut self, expr: &IrExpr) -> Option<ValueId> {
        self.lower_heap_err_common_piece(expr)
    }

    /// Shared arm-for-arm with what used to be two separate matches (see
    /// [`Self::result_err_heap_ok_result_body`] and
    /// [`Self::result_err_heap_fallback_piece`]): a fresh-owned or Dup'd heap piece
    /// from a heap-typed `ResultErr` payload expr, covering every shape EXCEPT the
    /// `List[String]` literal (that one only occurs on the heap-Ok-Result side and
    /// stays inline there, since it early-returns through its own `materialize_result_str`
    /// + read-shape reclassification instead of flowing through this common piece).
    fn lower_heap_err_common_piece(&mut self, expr: &IrExpr) -> Option<ValueId> {
        match &expr.kind {
            IrExprKind::Var { id }
                if self
                    .value_for(*id)
                    .map(|v| self.live_heap_handles.contains(&v))
                    .unwrap_or(false) =>
            {
                // Dup, do NOT move: the ctor gets its OWN co-owned reference and
                // the var keeps its handle + its scope-end drop. Moving consumed
                // the var — a SECOND `ok(r0)` then found nothing and deferred to
                // the zeroed Opaque, printing `ok("")` (fuzz seed-20260718 index
                // 248); native value-semantics copies each time. The same
                // borrow-then-Dup discipline as the param arm below.
                let src = self.value_for(*id).ok()?;
                let dup = self.fresh_value();
                self.ops.push(Op::Dup { dst: dup, src });
                Some(dup)
            }
            // A BORROWED param payload (`effect fn fail(msg: String) = err(msg)` — the
            // fan-family tail ctors): Dup the param's handle into a fresh CO-OWNED ref
            // (cert `a`) and move THAT in — the caller keeps its own reference (freed by
            // its owner once), the wrapper owns the Dup. The borrow-then-Dup discipline
            // the spread-record copy already proves.
            IrExprKind::Var { id }
                if self
                    .value_for(*id)
                    .map(|v| self.param_values.contains(&v))
                    .unwrap_or(false) =>
            {
                let src = self.value_for(*id).ok()?;
                let dup = self.fresh_value();
                self.ops.push(Op::Dup { dst: dup, src });
                Some(dup)
            }
            IrExprKind::LitStr { value } => {
                let pr = repr_of(&expr.ty).ok()?;
                let p = self.fresh_value();
                self.ops.push(Op::Alloc {
                    dst: p,
                    repr: pr,
                    init: Init::Str(value.clone()),
                });
                Some(p)
            }
            IrExprKind::Call {
                target: CallTarget::Named { name },
                args,
                ..
            } => {
                let lowered = self.lower_call_args(args).ok()?;
                let pr = repr_of(&expr.ty).ok()?;
                let p = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(p),
                    name: name.as_str().to_string(),
                    args: lowered,
                    result: Some(pr),
                });
                Some(p)
            }
            // A COMPUTED String Err payload (`ConcatStr`) — fresh-owned concat piece.
            IrExprKind::BinOp {
                op: almide_ir::BinOp::ConcatStr,
                ..
            } => {
                let mark = self.live_heap_handles.len();
                let obj = self.try_lower_concat_str(expr)?;
                self.drop_arm_locals(mark);
                Some(obj)
            }
            // An INTERPOLATED String payload (`err("bad ${id}")`, `ok("v=${x}")`) — the
            // same fresh-owned `__str_concat` chain as the ConcatStr arm (the interp IS a
            // concat fold); operand temps drop here so only the result survives the move.
            IrExprKind::StringInterp { parts } if matches!(expr.ty, Ty::String) => {
                let mark = self.live_heap_handles.len();
                let obj = self.try_lower_string_interp(parts)?;
                self.drop_arm_locals(mark);
                Some(obj)
            }
            // `err(float.to_fixed(x, 4))` — a PURE Module call yielding a fresh owned
            // STRING Err payload (fuzz C-class: fell to the deferred Opaque whose zeroed
            // block even flipped the TAG — printed `ok("")` for an err). Same piece as the
            // ok-side Module-call arm; the cap-as-tag Err slot owns the one String.
            IrExprKind::Call {
                target: CallTarget::Module { .. },
                ..
            } if matches!(expr.ty, Ty::String) => {
                let p = self.lower_owned_heap_field(expr)?;
                self.live_heap_handles.retain(|h| *h != p);
                Some(p)
            }
            // `err((if c then a else b))` — a heap-result IF/MATCH String Err payload
            // (the F-858 family): the one owned result moves into the Err slot.
            IrExprKind::If { .. } | IrExprKind::Match { .. } if matches!(expr.ty, Ty::String) => {
                let p = self.lower_owned_heap_field(expr)?;
                self.live_heap_handles.retain(|h| *h != p);
                Some(p)
            }
            _ => None,
        }
    }
}
