//! `LowerCtx` methods: control (extracted from lower/mod.rs).

use super::*;
use crate::{CallArg, IntOp, Op, ValueId};
use almide_ir::{
    CallTarget, IrExpr, IrExprKind, IrMatchArm, IrPattern, IrStmt, VarId,
};
use almide_lang::types::Ty;

impl LowerCtx {

    /// Lower an `if`/`match` in STATEMENT or scalar-/Unit-TAIL position by
    /// LINEARIZING its arms into the flat op stream — NO `Branch` op. A branch op
    /// would force the certificate fold (and `exec`/`verify`) to RECURSE a control-
    /// flow graph; the v1 checker must stay a flat fold (the certificate-format-v1
    /// tripwire: the instant the checker walks a CFG, the shape is broken). So the
    /// branch discipline lives ENTIRELY here in the untrusted lowering, and the cert
    /// the checker sees is a flat sequence.
    ///
    /// SOUNDNESS over a runtime where only ONE arm executes: each arm is lowered with
    /// a PER-ARM SCOPE FRAME ([`Self::lower_branch_arm`]) so every heap object it
    /// allocates is balanced WITHIN the arm (`i…d`). Such an object is therefore safe
    /// on EVERY path — the arm that allocates it runs its balanced `i…d`; on the
    /// other path it is simply never allocated (its `i…d` is vacuous). A handle that
    /// READS a pre-branch object (`var w = z`) is a balanced `a…d` PAIR inside the
    /// arm, removable on the other path, so the shared object stays balanced too. No
    /// arm value ESCAPES the branch: the RESULT is emitted by the CALLER as ONE
    /// merged slot — DISCARDED (statement / Unit position), a `Const` (scalar), or a
    /// fresh `Alloc{Opaque}` (heap). So no per-arm `i`/`a` crosses the branch and the
    /// flat cert is sound on both paths. The fresh-`Opaque` heap result is the same
    /// value-CONTENT deferral as every other heap value (which arm's value it equals
    /// is functional, not a safety property — `守るのは安全性であって機能の正しさで
    /// はない`); it is memory-safe BY CONSTRUCTION (a clean fresh alloc), so it needs
    /// no result-phi merge and bypasses no soundness check (a borrowed-param arm
    /// result is simply not moved out — the function returns the fresh `Opaque`).
    ///
    /// CAPS: both arms are lowered, so the witness captures the UNION of their
    /// capabilities — a conservative over-approximation (the path actually taken
    /// reaches a SUBSET), hence `actual ⊆ union ⊆ declared` stays sound. Const-ing a
    /// scalar branch instead (dropping the arms) would MISS an arm's `println` =
    /// caps-unsound, so the arms MUST be lowered even for a scalar result.
    ///
    /// A heap `match` SUBJECT is materialized (a fresh value into an owned temp dropped
    /// at the outer scope, a tracked var borrowed) so its `Alloc` is never elided.
    /// WALLED (each an explicit `Unsupported`, never a silent miscompile): a
    /// payload-BINDING `match` pattern (extracting a field needs the layout brick), a
    /// `match` arm GUARD, and an arm that REASSIGNS a variable (a path-dependent
    /// `value_of` rebind the flat fold cannot see → UAF).
    pub(crate) fn lower_branch(&mut self, expr: &IrExpr) -> Result<(), LowerError> {
        match &expr.kind {
            IrExprKind::If { cond, then, else_ } => {
                // The condition is evaluated ONCE before the branch — it is scalar
                // (Bool), so no ownership, but capture the caps of any call in it.
                self.record_elided_calls(cond);
                self.lower_branch_arm(None, then)?;
                self.lower_branch_arm(None, else_)?;
                Ok(())
            }
            IrExprKind::Match { subject, arms } => {
                // The subject is inspected once. A heap subject goes through
                // `lower_call_args` — an already-tracked `Var` is BORROWED, a FRESH
                // heap value (a call/literal result) is MATERIALIZED into an owned temp
                // dropped at the OUTER scope (never leaked — eliding its `Alloc` would
                // be accept-but-unsafe). A scalar subject carries no ownership; capture
                // any call in it for caps. Its ValueId (when heap) is the container a
                // payload-binding pattern aliases per arm.
                let subject_value: Option<ValueId> = if is_heap_ty(&subject.ty) {
                    match self.lower_call_args(std::slice::from_ref(subject))?.into_iter().next() {
                        Some(CallArg::Handle(v)) => Some(v),
                        _ => None,
                    }
                } else {
                    self.record_elided_calls(subject);
                    None
                };
                // A `match` whose SUBJECT is a self-host Option-returning call
                // (list.get/first/last) — which returns a real materialized 0-or-1-element-
                // list Option — gets that result TRACKED so the variant-match executes over
                // it. (A direct `Some`/`None` bound var is already tracked at construction.)
                if let Some(v) = subject_value {
                    if is_self_host_option_call(subject) {
                        self.materialized_options.insert(v);
                    }
                }
                // A `match` over a MATERIALIZED Option (`Some(scalar)`/`None`) EXECUTES
                // — only the taken arm runs — when the subject is tracked; otherwise it
                // LINEARIZES below (the sound both-arms fallback).
                if self.try_lower_variant_match(subject_value, arms) {
                    return Ok(());
                }
                for arm in arms {
                    // An arm GUARD is a scalar Bool sub-condition. The arms are
                    // LINEARIZED regardless of the guard, so it adds no ownership — just
                    // capture the caps of any call inside it; the guard's conditional
                    // truth (and any heap touch within it) is deferred like every Opaque.
                    if let Some(guard) = &arm.guard {
                        self.record_elided_calls(guard);
                    }
                    self.lower_branch_arm(Some((&arm.pattern, subject_value)), &arm.body)?;
                }
                Ok(())
            }
            other => Err(LowerError::Unsupported(format!(
                "lower_branch on a non-branch {}",
                kind_name(other)
            ))),
        }
    }

    /// Lower ONE branch arm into the flat op stream with a PER-ARM SCOPE FRAME:
    /// snapshot the live-handle count, lower the arm, then DROP every handle the arm
    /// added (so the arm is internally balanced, and vacuous when the other arm runs).
    /// The arm's result is DISCARDED (Unit/statement) or a SCALAR the caller merges
    /// into one `Const`; a heap result is walled. See [`Self::lower_branch`].
    ///
    /// For a `match` arm, `pattern` is `Some((pat, subject))` — the pattern's bound
    /// variables are introduced at the START of the frame (so they drop with the arm):
    /// a HEAP payload aliases the whole SUBJECT (`Op::Dup` — container-grain, like a
    /// field extraction; element/payload-PRECISE identity needs the layout brick),
    /// a SCALAR payload is a `Const`. See [`Self::bind_pattern`].
    pub(crate) fn lower_branch_arm(
        &mut self,
        pattern: Option<(&IrPattern, Option<ValueId>)>,
        body: &IrExpr,
    ) -> Result<(), LowerError> {
        let (stmts, tail): (&[IrStmt], Option<&IrExpr>) = match &body.kind {
            IrExprKind::Block { stmts, expr } => (stmts, expr.as_deref()),
            _ => (&[], Some(body)),
        };
        let mark = self.live_heap_handles.len();
        if let Some((pat, subject)) = pattern {
            self.bind_pattern(pat, subject)?;
        }
        // Inside the arm, a HEAP reassignment is DEFERRED, not rebound: a post-branch
        // read must not dereference a handle this arm dropped (the `in_frame` discipline
        // in `lower_stmt`). The accumulator keeps its still-live handle — memory-safe.
        self.in_frame += 1;
        for stmt in stmts {
            self.lower_stmt(stmt)?;
        }
        if let Some(tail) = tail {
            // The arm's tail VALUE never escapes the arm — the branch RESULT is one
            // fresh `Alloc{Opaque}` the CALLER emits (a heap result) or a `Const` (a
            // scalar). So a Unit-call tail is lowered as an EFFECT (`println`, so its
            // Stdout reaches the witness); a nested branch recurses (its own arms get
            // per-arm frames); ANY OTHER tail — scalar or HEAP — is a deferred value
            // whose calls we capture as effect markers (its content, like every
            // `Opaque`, is carried by the merged result, not modelled per-arm).
            match &tail.kind {
                IrExprKind::Call { .. } if matches!(tail.ty, Ty::Unit) => {
                    self.lower_effect_call(tail)?
                }
                // A nested Unit `if` arm-tail EXECUTES (only the taken arm runs) — so a
                // chained `else if … else …` (fizzbuzz) runs ONE branch, not all of them;
                // else it falls back to linearization.
                IrExprKind::If { cond, then, else_ }
                    if self.try_lower_unit_if(cond, then, else_) => {}
                IrExprKind::If { .. } | IrExprKind::Match { .. } => self.lower_branch(tail)?,
                _ => self.record_elided_calls(tail),
            }
        }
        self.in_frame -= 1;
        self.drop_arm_locals(mark);
        Ok(())
    }

    /// Try to lower a SCALAR `if cond then … else …` to EXECUTABLE control flow
    /// (`IfThen`/`Else`/`EndIf` markers — only the taken arm runs), returning the
    /// result `dst`. Scalar result ONLY (a heap-result `if` needs the arms' heap
    /// values merged per-arm, the linearization path). Each arm is PER-ARM-BALANCED
    /// (its heap temps dropped WITHIN the arm via `drop_arm_locals`, emitted inside the
    /// wasm `then`/`else`), so executing exactly one arm is memory-safe. The cert sees
    /// the arm ops FLAT between the markers — the same sound linearization it proves.
    /// Returns `None` (rolled back) when not in this subset — the caller then defers.
    pub(crate) fn try_lower_scalar_if(
        &mut self,
        cond: &IrExpr,
        then: &IrExpr,
        else_: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        if is_heap_ty(result_ty) {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let dst = self.fresh_value();
        if let Some(cond_v) = self.lower_scalar_value(cond) {
            self.ops.push(Op::IfThen { cond: cond_v, dst: Some(dst) });
            if let Some(then_val) = self.lower_scalar_arm(then) {
                self.ops.push(Op::Else { val: Some(then_val) });
                if let Some(else_val) = self.lower_scalar_arm(else_) {
                    self.ops.push(Op::EndIf { val: Some(else_val) });
                    return Some(dst);
                }
            }
        }
        // Not in the scalar-if subset — roll back every op/handle the attempt pushed.
        self.ops.truncate(ops_mark);
        self.live_heap_handles.truncate(lhh_mark);
        None
    }

    /// Desugar a `match subj { lit => body, …, _ => body }` over INT LITERAL patterns
    /// (+ a trailing wildcard/bind catch-all, no guards) to a nested `if subj == lit
    /// then body else …` IrExpr — so it EXECUTES via the if machinery (only the matched
    /// arm runs). `subj` is cloned into each `==`; a Var resolves to the same ValueId
    /// (no re-eval), and a non-scalar-lowerable subject makes the cond fail → the caller
    /// falls back to linearization. Returns `None` for non-literal patterns / guards /
    /// a non-exhaustive literal list (the linearization handles those).
    pub(crate) fn desugar_match_to_if(
        &self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<IrExpr> {
        if arms.is_empty() || arms.iter().any(|a| a.guard.is_some()) {
            return None;
        }
        self.build_match_chain(subject, arms, result_ty)
    }

    fn build_match_chain(
        &self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<IrExpr> {
        let (first, rest) = arms.split_first()?;
        match &first.pattern {
            // A catch-all: its body is the value, no further test.
            IrPattern::Wildcard | IrPattern::Bind { .. } => Some(first.body.clone()),
            IrPattern::Literal { expr } => {
                // A literal-only tail with no catch-all is not exhaustive over Int — defer.
                let else_branch = self.build_match_chain(subject, rest, result_ty)?;
                let cond = IrExpr {
                    kind: IrExprKind::BinOp {
                        op: almide_ir::BinOp::Eq,
                        left: Box::new(subject.clone()),
                        right: Box::new(expr.clone()),
                    },
                    ty: Ty::Bool,
                    span: None,
                    def_id: None,
                };
                Some(IrExpr {
                    kind: IrExprKind::If {
                        cond: Box::new(cond),
                        then: Box::new(first.body.clone()),
                        else_: Box::new(else_branch),
                    },
                    ty: result_ty.clone(),
                    span: None,
                    def_id: None,
                })
            }
            // Constructor / Tuple / Some·Ok·Err / record / list patterns: defer.
            _ => None,
        }
    }

    /// Try to lower a UNIT (effect) `if cond then … else …` to EXECUTABLE control flow
    /// — only the taken arm's EFFECTS run (the old linearization ran BOTH, mismatching
    /// v0). Each arm goes through `lower_branch_arm` (its Unit-call tail is an effect,
    /// its heap temps dropped per-arm), wrapped in `IfThen`/`Else`/`EndIf` with no
    /// result. Returns `false` (rolled back) if the cond is not a lowerable scalar.
    pub(crate) fn try_lower_unit_if(&mut self, cond: &IrExpr, then: &IrExpr, else_: &IrExpr) -> bool {
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let cond_v = match self.lower_scalar_value(cond) {
            Some(v) => v,
            None => {
                self.ops.truncate(ops_mark);
                return false;
            }
        };
        self.ops.push(Op::IfThen { cond: cond_v, dst: None });
        let then_ok = self.lower_branch_arm(None, then).is_ok();
        if then_ok {
            self.ops.push(Op::Else { val: None });
            if self.lower_branch_arm(None, else_).is_ok() {
                self.ops.push(Op::EndIf { val: None });
                return true;
            }
        }
        self.ops.truncate(ops_mark);
        self.live_heap_handles.truncate(lhh_mark);
        false
    }

    /// Try to EXECUTE a `match opt { Some(x) => …, None => … }` over a MATERIALIZED
    /// Option (the 0-or-1-element-list layout): read `len` as the tag, and on the Some
    /// branch extract `data[0]` as the payload. Only the taken arm runs (v0 semantics),
    /// vs the linearized fallback that runs both. Returns `false` (rolled back) when not
    /// in the subset — the caller then LINEARIZES.
    ///
    /// SOUNDNESS — the gate is `subject ∈ materialized_options`: the len-as-tag read is
    /// correct ONLY for a value KNOWN to carry the layout (`Some`=len1 / `None`=len0).
    /// Every other Option is a deferred `Opaque` (len0) that would MISREAD as `None`, so
    /// it is NOT in the set and keeps the sound linearized match. The execution adds NO
    /// ownership event: the tag/payload reads are scalar prims, the markers are no-ops in
    /// `verify_ownership`, and each arm is a PER-ARM-BALANCED frame (`lower_branch_arm`)
    /// — exactly the linearization the cert already proves, only wrapped in `IfThen`/
    /// `Else`/`EndIf` so one arm runs. SCALAR payload only (a heap bind would alias the
    /// element — a later refinement); UNIT arm bodies only (a value result is a later
    /// refinement). The subject was already materialized/borrowed by the caller.
    pub(crate) fn try_lower_variant_match(
        &mut self,
        subject_value: Option<ValueId>,
        arms: &[IrMatchArm],
    ) -> bool {
        use crate::PrimKind;
        // Gate 1: the subject is a TRACKED materialized Option.
        let subj = match subject_value {
            Some(v) if self.materialized_options.contains(&v) => v,
            _ => return false,
        };
        // Gate 2: exactly a `[Some(scalar-bind?), None]` shape, no guards, Unit bodies.
        if arms.len() != 2 || arms.iter().any(|a| a.guard.is_some()) {
            return false;
        }
        let mut some: Option<(&IrExpr, Option<VarId>)> = None;
        let mut none: Option<&IrExpr> = None;
        for arm in arms {
            match &arm.pattern {
                IrPattern::Some { inner } => {
                    let bind = match inner.as_ref() {
                        IrPattern::Bind { var, ty } if !is_heap_ty(ty) => Some(*var),
                        IrPattern::Wildcard => None,
                        _ => return false, // heap bind / nested ctor — not in this subset
                    };
                    if some.is_some() {
                        return false;
                    }
                    some = Some((&arm.body, bind));
                }
                IrPattern::None | IrPattern::Wildcard => {
                    if none.is_some() {
                        return false;
                    }
                    none = Some(&arm.body);
                }
                _ => return false,
            }
        }
        let ((some_body, some_bind), none_body) = match (some, none) {
            (Some(s), Some(n)) => (s, n),
            _ => return false,
        };
        if !matches!(some_body.ty, Ty::Unit) || !matches!(none_body.ty, Ty::Unit) {
            return false;
        }
        // Emit: tag = load32(handle(subj) + 4); if tag != 0 then Some-arm else None-arm.
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        let tag = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });
        self.ops.push(Op::IfThen { cond: tag, dst: None });
        // Some-arm (then): extract the scalar payload `data[0] = load64(handle + 12)`,
        // bind it, lower the arm body in a per-arm frame.
        if let Some(bind_var) = some_bind {
            let payload = self.load_at_offset(h, 12, PrimKind::Load { width: 8 });
            self.value_of.insert(bind_var, payload);
        }
        let some_ok = self.lower_branch_arm(None, some_body).is_ok();
        if !some_ok {
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return false;
        }
        self.ops.push(Op::Else { val: None });
        if self.lower_branch_arm(None, none_body).is_err() {
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return false;
        }
        self.ops.push(Op::EndIf { val: None });
        true
    }

    /// Try to EXECUTE `<materialized Option> ?? <scalar fallback>` to a SCALAR value: read
    /// the tag (len) and yield the payload (`data[0]`) when Some, else the fallback. Gated
    /// to a DIRECT self-host Option call — every such fn returns `Option[Int]`, so the
    /// payload is a scalar (no element alias), and its result is a real materialized Option
    /// dropped at scope end. Returns the scalar `dst`, or `None` (rolled back) when not in
    /// this subset (a non-call expr, or a heap fallback) — the caller defers to `Opaque`.
    ///
    /// SOUND: the Option's `Alloc` (the now-MATERIALIZED call, no longer elided) is `i`,
    /// dropped at scope end `d` = balanced; the tag/payload reads are scalar prims, the
    /// markers no-op, the payload is an i64 value COPY (not an alias), so dropping the
    /// Option after is safe. The call becoming real only improves caps (analyzed, not
    /// elided) and stays 1:1 with its IR call-node (no mir>ir issue).
    pub(crate) fn try_lower_option_unwrap_or(
        &mut self,
        expr: &IrExpr,
        fallback: &IrExpr,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        if !is_self_host_option_call(expr) {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let handle = match self.lower_call_args(std::slice::from_ref(expr)) {
            Ok(args) => match args.into_iter().next() {
                Some(CallArg::Handle(v)) => v,
                _ => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            },
            Err(_) => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![handle] });
        let tag = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });
        let result = self.fresh_value();
        self.ops.push(Op::IfThen { cond: tag, dst: Some(result) });
        let payload = self.load_at_offset(h, 12, PrimKind::Load { width: 8 });
        self.ops.push(Op::Else { val: Some(payload) });
        // The fallback is evaluated in the None (else) arm; a heap fallback rolls back.
        let fb = match self.lower_scalar_value(fallback) {
            Some(v) => v,
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        self.ops.push(Op::EndIf { val: Some(fb) });
        Some(result)
    }

    /// Emit `base + offset` then a `prim` load of `kind` at that address, returning the
    /// loaded value (an i64 in the prim floor's uniform model). The address arithmetic
    /// mirrors what `prim.handle(x) + offset` lowers to (`Op::ConstInt` + `Op::IntBinOp`).
    fn load_at_offset(&mut self, base: ValueId, offset: i64, kind: crate::PrimKind) -> ValueId {
        let off = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: off, value: offset });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: base, b: off });
        let dst = self.fresh_value();
        self.ops.push(Op::Prim { kind, dst: Some(dst), args: vec![addr] });
        dst
    }

    /// Lower ONE scalar `if` arm (a block's statements + a scalar tail value) with a
    /// per-arm scope frame: the heap temps the arm allocates are dropped WITHIN the arm
    /// (so taken-arm-only execution stays balanced). Returns the tail's scalar value.
    fn lower_scalar_arm(&mut self, arm: &IrExpr) -> Option<ValueId> {
        let (stmts, tail): (&[IrStmt], Option<&IrExpr>) = match &arm.kind {
            IrExprKind::Block { stmts, expr } => (stmts, expr.as_deref()),
            _ => (&[], Some(arm)),
        };
        let lhh_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        for stmt in stmts {
            if self.lower_stmt(stmt).is_err() {
                self.in_frame -= 1;
                return None;
            }
        }
        // A nested `if`/`match` tail (an else-if chain — what a desugared `match`
        // produces) EXECUTES recursively via the scalar-if machinery; otherwise the
        // tail is a scalar value or a scalar call.
        let val = tail.and_then(|t| match &t.kind {
            IrExprKind::If { cond, then, else_ } => {
                self.try_lower_scalar_if(cond, then, else_, &t.ty)
            }
            IrExprKind::Match { subject, arms } => self
                .desugar_match_to_if(subject, arms, &t.ty)
                .and_then(|if_expr| match &if_expr.kind {
                    IrExprKind::If { cond, then, else_ } => {
                        self.try_lower_scalar_if(cond, then, else_, &t.ty)
                    }
                    _ => None,
                }),
            _ => self.lower_scalar_value(t).or_else(|| self.try_lower_scalar_call(t, &t.ty)),
        });
        self.in_frame -= 1;
        self.drop_arm_locals(lhh_mark);
        val
    }

    /// Drop every heap handle the current scope frame added beyond `mark` (LIFO),
    /// restoring `live_heap_handles` to its pre-frame length — the per-arm teardown.
    pub(crate) fn drop_arm_locals(&mut self, mark: usize) {
        for v in self.live_heap_handles.split_off(mark).into_iter().rev() {
            self.ops.push(Op::Drop { v });
        }
    }

    /// Lower a `for v in iterable { body }` by modeling ONE iteration with a
    /// PER-ITERATION SCOPE FRAME. Each iteration is internally balanced (its loop
    /// variable + body locals are all dropped at iteration end), so N runtime
    /// iterations are N balanced episodes — no cross-iteration leak or double-free,
    /// and the flat cert (one iteration) is sound for any N (including 0: every op is
    /// in a balanced frame). NO loop op — the iteration discipline lives entirely in
    /// the lowering, the checker stays a flat fold.
    ///
    /// The ITERABLE is evaluated once: a heap iterable is lowered by `lower_call_args`
    /// — an already-tracked `Var` is BORROWED, a FRESH heap value (a call/literal
    /// result) is MATERIALIZED into an owned temp released at the OUTER scope; a scalar
    /// iterable (a `Range`) carries no ownership. The LOOP VARIABLE binds one element per
    /// iteration: a HEAP element ALIASES the whole container (`Op::Dup`, container-
    /// grain like field extraction — it keeps the container alive for the iteration,
    /// dropped at its end; element-precise identity needs the layout brick), a SCALAR
    /// element is a `Const`. A `break`/`continue` is a no-op admitted ONLY over a
    /// SCALAR-only frame (`wall_break_over_heap_frame`); over a heap frame it is WALLED
    /// (a real early exit would skip a per-iteration heap Drop = a wasm leak). A HEAP
    /// reassignment (the accumulator, `acc = acc + [x]`) is DEFERRED, not walled: the
    /// `in_frame` discipline keeps `acc` pinned to its still-live handle across
    /// iterations (memory-safe; the accumulation is deferred like every `Opaque`) and it
    /// is not a frame handle. A scalar reassignment (`i = i + 1`) is a Copy `Const`,
    /// harmless, admitted.
    pub(crate) fn lower_for_in(
        &mut self,
        var: VarId,
        var_tuple: &Option<Vec<VarId>>,
        iterable: &IrExpr,
        body: &[IrStmt],
    ) -> Result<(), LowerError> {
        // First try to EXECUTE a scalar `for i in start..end` as a real loop; out of that
        // subset it rolls back and we keep the model-one-iteration form below.
        if self.try_lower_scalar_for_range(var, var_tuple, iterable, body) {
            return Ok(());
        }
        // The iterable is evaluated ONCE before the loop. A heap iterable goes through
        // `lower_call_args` — an already-tracked `Var` is borrowed (no new ownership),
        // a fresh heap value is materialized into an owned temp dropped at the OUTER
        // scope (its caps captured by the lowering). A scalar iterable (a `Range`)
        // carries no ownership; capture any call in it for caps.
        let container: Option<ValueId> = if is_heap_ty(&iterable.ty) {
            match self.lower_call_args(std::slice::from_ref(iterable))?.into_iter().next() {
                Some(CallArg::Handle(v)) => Some(v),
                _ => None,
            }
        } else {
            self.record_elided_calls(iterable);
            None
        };
        let mark = self.live_heap_handles.len();
        let vars: Vec<VarId> = match var_tuple {
            Some(vs) => vs.clone(),
            None => vec![var],
        };
        for v in vars {
            // A heap element aliases the whole container; a scalar element is a Const.
            let elem_heap = find_var_ty(body, v).map(|t| is_heap_ty(&t)).unwrap_or(false);
            if elem_heap {
                let src = container.ok_or_else(|| {
                    LowerError::Unsupported(
                        "for-in heap loop variable over a non-container iterable not in this brick".into(),
                    )
                })?;
                let dst = self.fresh_value();
                self.ops.push(Op::Dup { dst, src });
                self.value_of.insert(v, dst);
                self.live_heap_handles.push(dst);
            } else {
                let dst = self.fresh_value();
                self.ops.push(Op::Const { dst });
                self.value_of.insert(v, dst);
            }
        }
        // A heap reassignment in the body is the loop ACCUMULATOR (`acc = acc + [x]`):
        // it is DEFERRED, not rebound (the `in_frame` discipline) — `acc` keeps its
        // still-live handle across iterations, so the next iteration never dereferences
        // a freed handle. Memory-safe; the accumulation itself is deferred like `Opaque`.
        self.in_frame += 1;
        for stmt in body {
            self.lower_stmt(stmt)?;
        }
        self.in_frame -= 1;
        self.wall_break_over_heap_frame(body, "for-in", mark)?;
        self.drop_arm_locals(mark);
        Ok(())
    }

    /// Lower a `while cond { body }` like a `for-in` body — a PER-ITERATION SCOPE
    /// FRAME makes one modeled iteration balanced, sound for any N. The condition is
    /// evaluated each iteration (its caps captured); the body's locals are dropped at
    /// iteration end. Same as `for-in`: a `break`/`continue` over a HEAP frame is walled
    /// (post-lowering), a heap reassignment (accumulator) deferred by `in_frame`.
    /// Try to lower `while cond { body }` as a REAL scalar-state loop that EXECUTES N
    /// times (the `LoopStart`/`LoopBreakUnless`/`LoopEnd` markers), reassigning scalar
    /// loop-carried state via [`Op::SetLocal`]. Restricted to the sound + runnable subset:
    /// an Int/Bool cond, NO `break`/`continue` (a no-op early-exit would be wrong inside a
    /// real loop), and a body with NO heap reassignment (the `scalar_loop_depth` Assign
    /// rule errors on one) and NO net heap handle escaping the per-iteration frame. The
    /// cond ops sit INSIDE the loop (re-evaluated each iteration); per-iteration heap (a
    /// string literal in `println`) is dropped before the back-edge. SOUNDNESS by REUSE:
    /// the markers are no-ops in verify_ownership and the body is a per-iteration-balanced
    /// frame — the cert verifies ONE balanced iteration, sound for any N (the existing
    /// model-one-iteration argument), the markers only make wasm actually run it N times.
    /// Returns false (and rolls back) when out of subset; `lower_while` then falls back.
    pub(crate) fn try_lower_scalar_while(&mut self, cond: &IrExpr, body: &[IrStmt]) -> bool {
        if !matches!(cond.ty, Ty::Int | Ty::Bool) || body_breaks_or_continues(body) {
            return false;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let value_of_snapshot = self.value_of.clone();

        self.ops.push(Op::LoopStart);
        let cond_v = match self.lower_scalar_value(cond) {
            Some(v) => v,
            None => {
                self.rollback_scalar_loop(ops_mark, lhh_mark, value_of_snapshot);
                return false;
            }
        };
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

        let body_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        self.scalar_loop_depth += 1;
        let mut ok = true;
        for stmt in body {
            if self.lower_stmt(stmt).is_err() {
                ok = false;
                break;
            }
        }
        self.scalar_loop_depth -= 1;
        self.in_frame -= 1;

        if !ok {
            self.rollback_scalar_loop(ops_mark, lhh_mark, value_of_snapshot);
            return false;
        }
        // Per-iteration heap (a string literal in a body `println`) is released before the
        // back-edge, INSIDE the loop, so each iteration is balanced.
        self.drop_arm_locals(body_mark);
        self.ops.push(Op::LoopEnd);
        true
    }

    fn rollback_scalar_loop(
        &mut self,
        ops_mark: usize,
        lhh_mark: usize,
        value_of_snapshot: std::collections::HashMap<almide_ir::VarId, ValueId>,
    ) {
        self.ops.truncate(ops_mark);
        self.live_heap_handles.truncate(lhh_mark);
        self.value_of = value_of_snapshot;
    }

    /// Try to lower a HEAP-RESULT `if cond then A else B` (a String/data-returning branch)
    /// to EXECUTABLE control flow — only the taken arm allocates, and its value is the
    /// function result. SOUNDNESS by PER-ARM BALANCE (no Coq change — see
    /// docs/roadmap/active/v1-heap-result-control-flow.md): each arm `Alloc`s its value
    /// (cert `i`) AND `Consume`s it (cert `m`) so the arm is internally `"im"` balanced
    /// exactly like a scalar arm carries none; the `IfThen` result `dst` is NEVER an
    /// `Alloc`, so it is not in the ownership cert's object set and `func.ret = dst` emits
    /// no second move-out (no double-free). The render selects one arm at runtime
    /// (`(if (result i32) …)`), so exactly one `Alloc` happens and is returned rc=1 to the
    /// caller — the untaken arm never allocates (no leak). FIRST version: both arms are
    /// direct string LITERALS (the common `if c then "a" else "b"`); other arm kinds fall
    /// back to today's sound Opaque form. Returns the result `dst`, or `None` (rolled
    /// back) when out of subset. Arms may be string LITERALS or a NESTED heap-result `if`
    /// (the else-if chain a desugared `match` produces — `match n { 0 => "a", _ => "b" }`),
    /// recursively. Other arm kinds fall back to today's sound Opaque form.
    pub(crate) fn try_lower_heap_result_if(
        &mut self,
        cond: &IrExpr,
        then: &IrExpr,
        else_: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        if !is_heap_ty(result_ty) {
            return None;
        }
        // The whole attempt rolls back as a unit: the recursion below truncates nothing,
        // so the OUTERMOST call restores the op stream AND the live-handle set on any
        // out-of-subset arm (a call arm may have materialized + tracked a temp).
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let result = self.lower_heap_result_if_inner(cond, then, else_, result_ty);
        if result.is_none() {
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
        }
        result
    }

    fn lower_heap_result_if_inner(
        &mut self,
        cond: &IrExpr,
        then: &IrExpr,
        else_: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let cond_v = self.lower_scalar_value(cond)?;
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond: cond_v, dst: Some(dst) });
        let then_obj = self.lower_heap_result_arm(then, result_ty)?;
        self.ops.push(Op::Else { val: Some(then_obj) });
        let else_obj = self.lower_heap_result_arm(else_, result_ty)?;
        self.ops.push(Op::EndIf { val: Some(else_obj) });
        Some(dst)
    }

    /// Lower ONE arm of a heap-result `if` to the value the arm leaves on the wasm stack.
    /// A string LITERAL is `Alloc{Str}` + `Consume` (the per-arm `"im"` move-out balance —
    /// NOT added to `live_heap_handles`, it is moved out as the result). A NESTED `if` (a
    /// desugared `match`'s else-if) recurses, its result dst being this arm's value.
    fn lower_heap_result_arm(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        match &arm.kind {
            IrExprKind::LitStr { value } => {
                let repr = repr_of(result_ty).ok()?;
                let obj = self.fresh_value();
                self.ops.push(Op::Alloc { dst: obj, repr, init: Init::Str(value.clone()) });
                self.ops.push(Op::Consume { v: obj });
                Some(obj)
            }
            IrExprKind::If { cond, then, else_ } => {
                self.lower_heap_result_if_inner(cond, then, else_, result_ty)
            }
            // A direct user-call arm (`if c then f(x) else "d"`): the callee returns a
            // FRESH owned heap value (CallFn-with-heap-result = cert `i`), moved out by the
            // arm's `Consume` (cert `m`) — the same `"im"` balance as a literal arm. Any
            // heap arg the call MATERIALIZES (a heap-literal/fresh-value arg) is dropped
            // WITHIN the arm (`drop_arm_locals`), NOT at function scope: a per-arm temp
            // freed at function scope would `Drop` an uninitialized local when the OTHER arm
            // ran (garbage rc_dec → trap). Per-arm, the temp is freed only if this arm
            // executes — the same per-iteration-balance discipline the loops use.
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                let repr = repr_of(result_ty).ok()?;
                let arm_mark = self.live_heap_handles.len();
                let lowered = self.lower_call_args(args).ok()?;
                let obj = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(obj),
                    name: name.as_str().to_string(),
                    args: lowered,
                    result: Some(repr),
                });
                self.ops.push(Op::Consume { v: obj });
                // Free materialized arg temps inside the arm (obj is moved out, never in
                // `live_heap_handles`, so it is not among them).
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            _ => None,
        }
    }

    /// Try to lower `for i in start..end { body }` over a SCALAR Int index as a REAL loop
    /// that EXECUTES every step — desugaring the range to the same while machinery
    /// (`LoopStart`/`LoopBreakUnless`/`LoopEnd` + `SetLocal`). The index is its own stable
    /// local initialized to `start` and incremented by 1 each iteration; `end` is snapshot
    /// ONCE before the loop (v0 builds the range once). Restricted to the runnable subset:
    /// a LITERAL `start` (so the index local is a fresh, distinct `ConstInt` — safe to
    /// mutate, never aliasing a caller value), a scalar-lowerable `end`, an Int loop var
    /// (no tuple), no `break`/`continue`, and a heap-reassign-free body (the
    /// `scalar_loop_depth` rule errors otherwise). Returns false (rolled back) when out of
    /// subset; `lower_for_in` then falls back to its sound model-one-iteration form.
    pub(crate) fn try_lower_scalar_for_range(
        &mut self,
        var: VarId,
        var_tuple: &Option<Vec<VarId>>,
        iterable: &IrExpr,
        body: &[IrStmt],
    ) -> bool {
        let IrExprKind::Range { start, end, inclusive } = &iterable.kind else {
            return false;
        };
        if var_tuple.is_some()
            || body_breaks_or_continues(body)
            || !matches!(find_var_ty(body, var), Some(Ty::Int))
            || !matches!(start.kind, IrExprKind::LitInt { .. })
        {
            return false;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let value_of_snapshot = self.value_of.clone();

        // Snapshot `end` once; init the index local `i = start` (a fresh ConstInt — a
        // distinct, mutable local, never aliasing a caller value). `one` for the step.
        let end_v = match self.lower_scalar_value(end) {
            Some(v) => v,
            None => {
                self.rollback_scalar_loop(ops_mark, lhh_mark, value_of_snapshot);
                return false;
            }
        };
        if self.lower_bind(var, &Ty::Int, start).is_err() {
            self.rollback_scalar_loop(ops_mark, lhh_mark, value_of_snapshot);
            return false;
        }
        let Some(&i_v) = self.value_of.get(&var) else {
            self.rollback_scalar_loop(ops_mark, lhh_mark, value_of_snapshot);
            return false;
        };
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });

        self.ops.push(Op::LoopStart);
        // The bound test, re-read each iteration: `i < end` (exclusive) / `i <= end` (incl).
        let cond_v = self.fresh_value();
        let cmp = if *inclusive { IntOp::Le } else { IntOp::Lt };
        self.ops.push(Op::IntBinOp { dst: cond_v, op: cmp, a: i_v, b: end_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

        let body_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        self.scalar_loop_depth += 1;
        let mut ok = true;
        for stmt in body {
            if self.lower_stmt(stmt).is_err() {
                ok = false;
                break;
            }
        }
        self.scalar_loop_depth -= 1;
        self.in_frame -= 1;
        if !ok {
            self.rollback_scalar_loop(ops_mark, lhh_mark, value_of_snapshot);
            return false;
        }
        self.drop_arm_locals(body_mark);
        // The implicit step `i = i + 1`, then the back-edge.
        let next_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: next_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::SetLocal { local: i_v, src: next_v });
        self.ops.push(Op::LoopEnd);
        true
    }

    pub(crate) fn lower_while(&mut self, cond: &IrExpr, body: &[IrStmt]) -> Result<(), LowerError> {
        // First try to EXECUTE it as a real scalar-state loop; on any out-of-subset
        // feature this rolls back cleanly and we keep the model-one-iteration form below.
        if self.try_lower_scalar_while(cond, body) {
            return Ok(());
        }
        self.record_elided_calls(cond);
        let mark = self.live_heap_handles.len();
        // A heap reassignment in the body is DEFERRED (the `in_frame` discipline) — the
        // accumulator keeps its still-live handle across iterations. Memory-safe.
        self.in_frame += 1;
        for stmt in body {
            self.lower_stmt(stmt)?;
        }
        self.in_frame -= 1;
        self.wall_break_over_heap_frame(body, "while", mark)?;
        self.drop_arm_locals(mark);
        Ok(())
    }

    /// Post-lowering loop-body admission for `break`/`continue`. The early exit is
    /// lowered as a no-op (the cert models the loop completing, frame Drops intact),
    /// which is leak-safe ONLY when the per-iteration frame holds NO heap handle a real
    /// early exit could skip — `live_heap_handles` holds only heap handles, so a frame
    /// that grew past `mark` (a heap loop variable's `Op::Dup`, a heap body local, or a
    /// materialized temp) holds one. At runtime the v0 wasm backend frees AFTER the break
    /// branch target, so such a frame would LEAK; KEEP WALLING it. A scalar-only frame
    /// (scalar loop variable, no heap local; the heap accumulator is deferred, not a
    /// frame handle) has no Drop to skip and is admitted with the break/continue no-op.
    pub(crate) fn wall_break_over_heap_frame(
        &self,
        body: &[IrStmt],
        what: &str,
        mark: usize,
    ) -> Result<(), LowerError> {
        if self.live_heap_handles.len() > mark && body_breaks_or_continues(body) {
            return Err(LowerError::Unsupported(format!(
                "{what} body with break/continue over a heap frame (early exit would skip a per-iteration heap drop = a wasm leak) not in this brick"
            )));
        }
        Ok(())
    }
}

/// Is `subject` a call to a SELF-HOST Option-returning stdlib fn? Such a call returns a
/// real MATERIALIZED 0-or-1-element-list Option (its impl returns through `Some(scalar)`/
/// `None` helpers, tail-materialized), so a `match` over its result may EXECUTE — the call
/// dst is tracked in `materialized_options`. NARROW to the fns ACTUALLY self-hosted today
/// (`list.get`): a fn merely declared Option-returning but NOT self-hosted would return a
/// deferred `Opaque` (len0) that must NOT be tracked, else the match would misread it as
/// `None`. Add a name here only when its self-host impl + registry entry land together.
fn is_self_host_option_call(subject: &IrExpr) -> bool {
    match &subject.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } => {
            is_self_host_option_module_fn(module.as_str(), func.as_str())
        }
        _ => false,
    }
}
