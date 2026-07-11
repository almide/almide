//! `LowerCtx` methods: control (extracted from lower/mod.rs).

use super::*;
use crate::{CallArg, IntOp, Op, ValueId};
use almide_ir::{
    CallTarget, IrExpr, IrExprKind, IrMatchArm, IrPattern, IrStmt, VarId,
};
use almide_lang::types::Ty;

/// One parsed arm of a custom-variant `match` (ADT bricks 3/5c). A `Ctor` arm tests `tag ==
/// tag` and binds its fields from slots — `(slot index 1+i, bound var, is_heap)`: a SCALAR
/// field is an i64 value copy; a leaf-heap (`String`) field is a BORROW of the slot handle
/// (the subject keeps ownership). A move-out arm auto-`Dup`s in `lower_heap_result_arm`; a
/// consuming re-use `Dup`s in `lower_owned_heap_field` — so the borrow is never released at
/// rc 0. A `Wildcard` arm is the unconditional catch-all.
enum VariantArmKind {
    Ctor { tag: i64, binds: Vec<(usize, VarId, bool)> },
    Wildcard,
    /// A BINDER catch-all (`e => err(e)` — the regrouped compute fall-through): matches any
    /// tag and binds the WHOLE subject value as a BORROW (`param_values` — a consuming
    /// re-use Dups, exactly the borrowed-param ctor discipline).
    BindAll { var: VarId },
}

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
                    // A `match` whose SUBJECT is a record/tuple FIELD that is an Option/Result
                    // (`match n.next { some(x) => … }` over `next: Option[Int]`): the field-borrow
                    // already loaded the field's owned handle into `v` (a real 0-or-1-element Option
                    // block the record owns). Track it so the match BRANCHES (reads tag @4) instead of
                    // LINEARIZING. The handle is a BORROW of the record's owned field — no new ownership
                    // (the record's masked drop frees it); a Some-payload bind auto-Dups if it escapes.
                    if matches!(&subject.kind,
                        IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. } | IrExprKind::IndexAccess { .. })
                    {
                        use almide_lang::types::constructor::TypeConstructorId;
                        if matches!(&subject.ty, Ty::Applied(TypeConstructorId::Option, _)) {
                            self.materialized_options.insert(v);
                            if crate::lower::is_heap_elem_list_ty(&subject.ty) {
                                self.heap_elem_lists.insert(v);
                            }
                        } else if crate::lower::is_result_ty(&subject.ty) {
                            self.materialized_results.insert(v);
                            if crate::lower::is_heap_elem_list_ty(&subject.ty) {
                                self.heap_elem_lists.insert(v);
                            }
                        }
                    }
                    if is_self_host_option_call(subject) {
                        self.materialized_options.insert(v);
                        // An `Option[heap]` (e.g. `Option[(Int,Int)]` from option.zip) OWNS its
                        // payload — track it as a nested-ownership list so the variant-match binds the
                        // Some payload by `LoadHandle` (the borrowed element handle, not the whole
                        // Option) AND the scope-end drop is the recursive `DropListStr` (frees the
                        // owned payload, no leak). Without this the heap-payload bind gate fails →
                        // the match linearizes and reads the Option's own slots as the payload.
                        if crate::lower::is_heap_elem_list_ty(&subject.ty) {
                            self.heap_elem_lists.insert(v);
                        }
                    }
                    if is_self_host_result_call(subject) {
                        self.materialized_results.insert(v);
                    }
                    // A self-host HEAP-Ok Result call (result.zip → Result[(Int,Int), String]) — track
                    // it in the cap-as-tag set (so the match reads tag @16 + binds the @12 payload
                    // handle) AND, since it owns a heap payload (the Err String / the Ok tuple), in
                    // heap_elem_lists (so the heap-payload bind gates open AND the scope-end drop is
                    // the recursive DropListStr). Without it the match linearizes → garbage.
                    if is_self_host_result_str_call(subject) {
                        self.materialized_results_str.insert(v);
                        if crate::lower::is_result_listval_ty(&subject.ty) {
                            self.value_result_lists.insert(v);
                        } else if crate::lower::is_list_str_result_ty(&subject.ty) {
                            // `Result[List[String], String]` (fs.list_dir) — the Ok payload is a
                            // List[String]; route the scope-end drop to the RECURSIVE DropResultListStr
                            // (frees each element String + the list block), NOT the flat DropListStr
                            // (heap_elem_lists) which would leak them.
                            self.list_str_result_results.insert(v);
                        } else if crate::lower::is_heap_elem_list_ty(&subject.ty) {
                            self.heap_elem_lists.insert(v);
                        }
                    }
                    // A USER Named-call returning Result (`match char_to_val(c) { ok(v)=>.., err(e)=>.. }`
                    // — the TCO loop body the unwrap-`!` desugar produces, base64 decode_chunks). Track
                    // it like the value-match subject: a SCALAR-Ok `Result[scalar,String]` reads len-tag
                    // @4 (materialized_results) + heap_elem_lists for the Err-String bind / DropListStr; a
                    // HEAP-Ok `Result[heap,String]` is constructed cap-tag @16 (materialize_result_str)
                    // so it reads cap-tag @16 (materialized_results_str) + the by-type drop. WITHOUT this
                    // a user-Result statement match LINEARIZES (runs BOTH arms) = a silent miscompile.
                    // A `match <never-err lifted-effect call> {…}` that `rewrite_never_err_effect_match`
                    // could NOT turn into a `let`-block (an `ok(_)`/structured/guarded Ok arm): its
                    // subject's `.ty` is the lifted `Result[T, String]` but the callee returns RAW `T`,
                    // so reading it as a Result handle TRAPs (the `$rc_dec` sentinel over raw bytes).
                    // WALL it cleanly — never a trap. (The common `ok(x)` shape is already rewritten away
                    // and never reaches here.)
                    if let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &subject.kind {
                        if crate::lower::NEVER_ERR_LIFTED_FNS.with(|s| s.borrow().contains(name.as_str())) {
                            return Err(LowerError::Unsupported(
                                "match over a never-err effect-fn call with a non-`ok(x)` Ok pattern \
                                 (ok(_)/structured/guarded) not in this brick — the effect-fn returns a \
                                 raw value, so there is no Result tag to dispatch on (the heap-effect-fn \
                                 error-model frontier)".into(),
                            ));
                        }
                    }
                    // A PURE heap-result MODULE call (`json.parse` — resolved by the
                    // self-host registry, so its Result is BUILT by the same
                    // materialize_result_str layout a user fn uses) is tracked exactly
                    // like a Named user call. Untracked, the match fell to the both-arms
                    // linearization and RAN BOTH println arms (silent miscompile,
                    // 2026-07-03; the json.parse read_message leg).
                    let result_call_subject = match &subject.kind {
                        IrExprKind::Call { target: CallTarget::Named { .. }, .. } => true,
                        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } =>
                            crate::purity::is_pure(module.as_str(), func.as_str()),
                        _ => false,
                    };
                    if result_call_subject
                        && crate::lower::is_result_ty(&subject.ty)
                    {
                        if Self::is_heap_ok_result(&subject.ty) {
                            // A USER heap-Ok Result is CONSTRUCTED by the heap-Ok ResultOk arm via
                            // materialize_result_str(value_ok=false) → cap-tag @16 + heap_elem_lists
                            // (DropListStr). The match MUST agree: track materialized_results_str (read
                            // tag @16) + heap_elem_lists (the err-arm String bind gate AND the flat
                            // DropListStr the construction uses for the List[Int]/String Ok payload).
                            self.materialized_results_str.insert(v);
                            // A `Result[(String, Int), String]` (toml parse_key_part) needs the
                            // RECURSIVE DropResultStrInt (frees the Ok tuple's String + block) — a
                            // flat DropListStr would rc_dec the @12 tuple HANDLE only, leaking its
                            // String. Other heap-Ok shapes keep the flat heap_elem_lists/DropListStr.
                            if crate::lower::is_str_int_result_ty(&subject.ty) {
                                self.str_int_result_results.insert(v);
                            } else if crate::lower::is_value_int_result_ty(&subject.ty) {
                                self.value_int_result_results.insert(v);
                            } else if crate::lower::is_list_str_int_result_ty(&subject.ty) {
                                self.list_str_int_result_results.insert(v);
                            } else if crate::lower::is_list_value_int_result_ty(&subject.ty) {
                                self.list_value_int_result_results.insert(v);
                            } else {
                                self.heap_elem_lists.insert(v);
                            }
                        } else {
                            self.materialized_results.insert(v);
                            if let Ty::Applied(
                                almide_lang::types::constructor::TypeConstructorId::Result,
                                a,
                            ) = &subject.ty
                            {
                                if a.len() == 2 && !is_heap_ty(&a[0]) && is_heap_ty(&a[1]) {
                                    self.heap_elem_lists.insert(v);
                                }
                            }
                        }
                    }
                }
                // A CUSTOM variant (user ADT) statement match — tag@slot0 dispatch (ADT brick 3,
                // unit sibling). A custom variant must NEVER reach the both-arms linearization
                // (that runs EVERY arm's effects = a silent miscompile), so once the subject is a
                // registered variant this either lowers or WALLs — it never falls through.
                if self.custom_variant_type_name(&subject.ty).is_some() {
                    return self.lower_custom_variant_unit_match(&subject.ty, subject_value, arms);
                }
                // A `match` over a MATERIALIZED Option (`Some(scalar)`/`None`) or Result
                // (`Ok(scalar)`/`Err(string)`) EXECUTES — only the taken arm runs — when the
                // subject is tracked; otherwise it LINEARIZES below (the sound both-arms fallback).
                if self.try_lower_variant_match(subject_value, arms) {
                    return Ok(());
                }
                if self.try_lower_result_match(subject_value, arms) {
                    return Ok(());
                }
                // A GUARDED arm reaching the linearization fallback cannot be faithfully
                // lowered: the both-arms linearization runs EVERY arm's effects regardless
                // of the guard's truth, so the guard's conditional SELECTION is lost — a
                // silent miscompile (it would run the wrong arm, or both). WALL it (the
                // executable desugar in `desugar_match_to_if` already declines guards, so
                // the only way a guard reaches here is the linearization path).
                if arms.iter().any(|a| a.guard.is_some()) {
                    return Err(LowerError::Unsupported(
                        "match arm guard cannot be faithfully lowered (the linearization runs \
                         every arm, losing the guard's conditional selection) not in this brick"
                            .into(),
                    ));
                }
                // The linearization is sound ONLY for effect-free arms (running both
                // bodies is then observationally a no-op). An arm containing a CALL can
                // print / write / recurse — running the untaken arm is a silent
                // miscompile (both println arms of an untracked Result match ran,
                // 2026-07-03). WALL it: an unlowered shape must be a clean Unsupported,
                // never wrong output.
                fn arm_has_call(e: &IrExpr) -> bool {
                    use almide_ir::visit::{walk_expr, IrVisitor};
                    struct C(bool);
                    impl IrVisitor for C {
                        fn visit_expr(&mut self, e: &IrExpr) {
                            if matches!(
                                e.kind,
                                IrExprKind::Call { .. }
                                    | IrExprKind::TailCall { .. }
                                    | IrExprKind::RuntimeCall { .. }
                            ) {
                                self.0 = true;
                            }
                            walk_expr(self, e);
                        }
                    }
                    let mut c = C(false);
                    c.visit_expr(e);
                    c.0
                }
                if arms.iter().any(|a| arm_has_call(&a.body)) {
                    return Err(LowerError::Unsupported(
                        "match over an UNTRACKED subject with a call-bearing arm cannot take \
                         the both-arms linearization (it would run the untaken arm's effects) \
                         not in this brick".into(),
                    ));
                }
                for arm in arms {
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
                // A Unit arm-tail effect call wrapped in `Try`/`Unwrap` (the auto-`?` of an
                // effect-fn call, e.g. the recursive `loop(rest)` tail or `eff_call(x)`):
                // its `Result[Unit, _]` is discarded, so `lower_effect_call` strips the
                // wrapper and runs the call for effect. WITHOUT this arm it would fall to
                // `record_elided_calls` below — which captures the inner calls as caps
                // markers but EMITS NO call, silently dropping the effect (and, for a
                // recursive tail, the recursion itself).
                IrExprKind::Try { .. } | IrExprKind::Unwrap { .. } if matches!(tail.ty, Ty::Unit) => {
                    self.lower_effect_call(tail)?
                }
                // A nested Unit `if` arm-tail EXECUTES (only the taken arm runs) — so a
                // chained `else if … else …` (fizzbuzz) runs ONE branch, not all of them;
                // else it falls back to linearization.
                IrExprKind::If { cond, then, else_ }
                    if self.try_lower_unit_if(cond, then, else_) => {}
                IrExprKind::If { .. } | IrExprKind::Match { .. } => self.lower_branch(tail)?,
                // A LOOP tail (`ArrV(rows) => { for row in rows { … } }` — the gguf ValArray
                // consumer arm; a `while` sibling): a loop is a Unit EFFECT, so it must RUN,
                // not fall to `record_elided_calls` (which captures the body's calls as caps
                // markers and SILENTLY DROPS the loop — the unlinked-`println` render leak).
                IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                    self.lower_for_in(*var, var_tuple, iterable, body)?
                }
                IrExprKind::While { cond, body } => self.lower_while(cond, body)?,
                // A nested BLOCK tail (`{ stmt; … }` as an arm's tail — e.g. a flattened
                // binder body, or a brace-wrapped arm) must NOT fall to `record_elided_calls`:
                // that captures only the calls inside and SILENTLY DROPS its statements (the
                // `match … { x => { r = 999 } }` assignment-loss). Recurse so its statements
                // run as effects and its own tail is dispatched the same way.
                IrExprKind::Block { .. } => self.lower_branch_arm(None, tail)?,
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

    /// Desugar a `match subj { lit => body, …, _ => body }` to a nested `if subj == lit
    /// then body else …` IrExpr — so it EXECUTES via the if machinery (only the matched
    /// arm runs). `subj` is cloned into each `==`; a Var resolves to the same ValueId
    /// (no re-eval), and a non-scalar-lowerable subject makes the cond fail → the caller
    /// falls back to linearization. Returns `None` for non-literal patterns / guards /
    /// a non-exhaustive literal list (the linearization handles those).
    ///
    /// Handled SCALAR-subject shapes:
    /// - INT LITERAL arms + a trailing wildcard/binder catch-all;
    /// - a BOOL subject `match b { true => A, false => B }` (exhaustive over `{true,false}`
    ///   with no wildcard) → `if b then A else B`, where the `true`/`false` arms may appear
    ///   in either order;
    /// - a BINDER catch-all `x => body`, which BINDS `x` to the subject (a `let x = subj`
    ///   wrapped around `body`) so the arm body's references to `x` resolve — without the
    ///   bind, `x` would lower to a deferred 0 and the whole match silently miscompile.
    pub(crate) fn desugar_match_to_if(
        &self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<IrExpr> {
        if arms.is_empty() {
            return None;
        }
        // A BOOL subject is exhaustive over `{true, false}` WITHOUT a wildcard: the literal
        // chain below would run off the end (`build_match_chain([])` → None). Desugar the
        // canonical 2-arm form `match b { true => A, false => B }` to `if b then A else B`
        // directly (arms in either order); other Bool shapes (a single wildcard/binder arm)
        // fall through to the generic chain.
        if matches!(subject.ty, Ty::Bool) {
            if let Some(if_expr) = self.desugar_bool_match(subject, arms, result_ty) {
                return Some(if_expr);
            }
        }
        self.build_match_chain(subject, arms, result_ty)
    }

    /// A 2-arm `match b { true => A, false => B }` (arms in either order, no guards) →
    /// `if b then A else B`. Returns `None` if the shape is not exactly the two Bool
    /// literals (e.g. a wildcard arm) — the caller then falls to `build_match_chain`.
    fn desugar_bool_match(
        &self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<IrExpr> {
        if arms.len() != 2 {
            return None;
        }
        let bool_lit = |arm: &IrMatchArm| -> Option<bool> {
            // A GUARDED bool arm (`true if g => ..`) is NOT an unconditional `true`; decline so
            // it falls to `build_match_chain`, which folds the guard into the condition.
            if arm.guard.is_some() {
                return None;
            }
            match &arm.pattern {
                IrPattern::Literal { expr } => match &expr.kind {
                    IrExprKind::LitBool { value } => Some(*value),
                    _ => None,
                },
                _ => None,
            }
        };
        let (b0, b1) = (bool_lit(&arms[0])?, bool_lit(&arms[1])?);
        // Must be exactly one `true` arm and one `false` arm.
        if b0 == b1 {
            return None;
        }
        let (then_arm, else_arm) = if b0 { (&arms[0], &arms[1]) } else { (&arms[1], &arms[0]) };
        Some(IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(subject.clone()),
                then: Box::new(then_arm.body.clone()),
                else_: Box::new(else_arm.body.clone()),
            },
            ty: result_ty.clone(),
            span: None,
            def_id: None,
        })
    }

    fn build_match_chain(
        &self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<IrExpr> {
        let (first, rest) = arms.split_first()?;
        // Bind an arm's pattern var to the subject. A SCALAR PURE subject (a Var / literal) is
        // freely substitutable: `var := subject` is inlined into the body, producing a DIRECT
        // expr (NOT a `{ let var = subj; .. }` Block) — so a HEAP-result binder/guard match
        // (`fn f(n) -> String = match n { x if g => "..", _ => ".." }`, a classifier) lowers
        // through the proven heap-result-`if` path too (which cannot lower a Block tail). A
        // NON-pure subject (a call — re-evaluation would duplicate effects) keeps `bind_subject`
        // (single eval; its heap case stays walled, its scalar case runs via `lower_scalar_arm`).
        let subject_pure = matches!(
            &subject.kind,
            IrExprKind::Var { .. }
                | IrExprKind::LitInt { .. }
                | IrExprKind::LitBool { .. }
                | IrExprKind::LitFloat { .. }
        );
        let bind_or_subst = |var: VarId, ty: &Ty, body: &IrExpr| -> IrExpr {
            if subject_pure {
                almide_ir::substitute_var_in_expr(body, var, subject)
            } else {
                Self::bind_subject(var, ty, subject, body)
            }
        };
        // A GUARDED arm `pat if g => body` runs `body` only when the pattern matches AND `g`
        // holds; otherwise control falls through to the rest. Desugar to `if <pat-test && g>
        // then body else <rest>` (a Bind pattern binds the subject around the test so both `g`
        // and `body` see it). This keeps a scalar guarded match in the cert-clean nested-`if`
        // subset — vs the linearization, which runs every arm and LOSES the guard (a
        // `match n { x if x > 3 => 10, _ => 0 }` → silent 0 miscompile).
        if let Some(guard) = &first.guard {
            let else_branch = self.build_match_chain(subject, rest, result_ty)?;
            let mk_if = |cond: IrExpr, then: &IrExpr, els: IrExpr| IrExpr {
                kind: IrExprKind::If {
                    cond: Box::new(cond),
                    then: Box::new(then.clone()),
                    else_: Box::new(els),
                },
                ty: result_ty.clone(),
                span: None,
                def_id: None,
            };
            return match &first.pattern {
                // `_ if g`: the guard alone gates the body.
                IrPattern::Wildcard => Some(mk_if(guard.clone(), &first.body, else_branch)),
                // `x if g`: bind x = subject in `if g then body else rest` so g/body see x.
                IrPattern::Bind { var, ty } => {
                    let inner = mk_if(guard.clone(), &first.body, else_branch);
                    Some(bind_or_subst(*var, ty, &inner))
                }
                // `lit if g`: cond = (subject == lit) && g.
                IrPattern::Literal { expr } => {
                    let eq = IrExpr {
                        kind: IrExprKind::BinOp {
                            op: almide_ir::BinOp::Eq,
                            left: Box::new(subject.clone()),
                            right: Box::new(expr.clone()),
                        },
                        ty: Ty::Bool,
                        span: None,
                        def_id: None,
                    };
                    let cond = IrExpr {
                        kind: IrExprKind::BinOp {
                            op: almide_ir::BinOp::And,
                            left: Box::new(eq),
                            right: Box::new(guard.clone()),
                        },
                        ty: Ty::Bool,
                        span: None,
                        def_id: None,
                    };
                    Some(mk_if(cond, &first.body, else_branch))
                }
                // A guarded ctor/tuple/Some·Ok·Err arm — defer (the variant path / linearization).
                _ => None,
            };
        }
        match &first.pattern {
            // A wildcard catch-all: its body is the value, no further test.
            IrPattern::Wildcard => Some(first.body.clone()),
            // A BINDER catch-all `x => body`: bind `x` to the subject so the body's
            // references to `x` resolve to the subject value. Without the bind, `x` would
            // lower to a deferred 0 (a silent miscompile of `match n { 0 => .., x => x+1 }`).
            IrPattern::Bind { var, ty } => Some(bind_or_subst(*var, ty, &first.body)),
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

    /// `{ let var = subject; body }` typed like `body` — the binder-arm binding so the
    /// arm body's references to `var` resolve to the subject value (a SCALAR subject; the
    /// `let` lowers as a Copy bind). The subject is re-cloned, but a scalar subject is a
    /// pure value (Var/literal) so re-evaluation is side-effect-free.
    ///
    /// When `body` is itself a Block (`x => { r = 999 }` in STATEMENT position), its
    /// statements are FLATTENED in after the `let` rather than nested as the outer Block's
    /// tail expr. A nested-Block tail would reach `lower_branch_arm`'s tail dispatch as an
    /// `IrExprKind::Block`, which only handled Call/If/Match — a bare-statement Block (an
    /// `Assign`) fell through to `record_elided_calls` and the assignment was SILENTLY
    /// DROPPED (the `match n { 0 => {r=100}, x => {r=999} }` miscompile). Flattening lifts
    /// the body's statements to be the outer Block's own statements, where the `stmts` loop
    /// lowers them as effects, and the body's own tail becomes the outer tail.
    fn bind_subject(var: VarId, var_ty: &Ty, subject: &IrExpr, body: &IrExpr) -> IrExpr {
        let bind = IrStmt {
            kind: IrStmtKind::Bind {
                var,
                mutability: almide_ir::Mutability::Let,
                ty: var_ty.clone(),
                value: subject.clone(),
            },
            span: None,
        };
        let (stmts, tail): (Vec<IrStmt>, Option<Box<IrExpr>>) = match &body.kind {
            IrExprKind::Block { stmts, expr } => {
                let mut s = Vec::with_capacity(stmts.len() + 1);
                s.push(bind);
                s.extend(stmts.iter().cloned());
                (s, expr.clone())
            }
            _ => (vec![bind], Some(Box::new(body.clone()))),
        };
        IrExpr {
            kind: IrExprKind::Block { stmts, expr: tail },
            ty: body.ty.clone(),
            span: None,
            def_id: None,
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
        // Exactly ONE arm runs at runtime, so a scalar reassignment of an outer mutable
        // var inside an arm must mutate that var's stable local IN PLACE (`SetLocal`), not
        // rebind a fresh frame-local — see `LowerCtx::unit_arm_depth`.
        self.unit_arm_depth += 1;
        let then_ok = self.lower_branch_arm(None, then).is_ok();
        let both_ok = then_ok && {
            self.ops.push(Op::Else { val: None });
            self.lower_branch_arm(None, else_).is_ok()
        };
        self.unit_arm_depth -= 1;
        if both_ok {
            self.ops.push(Op::EndIf { val: None });
            return true;
        }
        self.ops.truncate(ops_mark);
        self.live_heap_handles.truncate(lhh_mark);
        false
    }

    /// Recursively wrap each LEAF arm of `if_branch` so the arm `value` becomes `{ let s = value;
    /// <rest> }` typed `result_ty`. A nested `if` arm (an else-if chain from a desugared match)
    /// recurses; a leaf value-arm gets the continuation block.
    pub(crate) fn wrap_branch_arms(
        if_branch: &IrExpr,
        bind_var: VarId,
        bind_ty: &Ty,
        rest_stmts: &[IrStmt],
        rest_tail: &Option<Box<IrExpr>>,
        result_ty: &Ty,
    ) -> IrExpr {
        let IrExprKind::If { cond, then, else_ } = &if_branch.kind else {
            // A non-`if` leaf: wrap it as a continuation block.
            return Self::continuation_block(if_branch, bind_var, bind_ty, rest_stmts, rest_tail, result_ty);
        };
        let wrap = |arm: &IrExpr| -> IrExpr {
            if matches!(&arm.kind, IrExprKind::If { .. }) {
                Self::wrap_branch_arms(arm, bind_var, bind_ty, rest_stmts, rest_tail, result_ty)
            } else {
                Self::continuation_block(arm, bind_var, bind_ty, rest_stmts, rest_tail, result_ty)
            }
        };
        IrExpr {
            kind: IrExprKind::If {
                cond: cond.clone(),
                then: Box::new(wrap(then)),
                else_: Box::new(wrap(else_)),
            },
            ty: result_ty.clone(),
            span: None,
            def_id: None,
        }
    }

    /// The MATCH analogue of [`Self::wrap_branch_arms`] for a CUSTOM-VARIANT (or any
    /// non-literal-pattern) `let s = match subj { … }; rest` — `desugar_match_to_if` only
    /// reduces LITERAL-pattern matches, so a `match s.shape { Circle(_) => "circle", … }`
    /// declined and the whole let-bound-match walled. Here each arm's BODY is wrapped with
    /// the continuation `{ let s = <arm_body>; rest }`, keeping the `Match` so the proven
    /// `try_lower_custom_variant_match` (tail position) runs each per-arm-balanced arm. The
    /// pattern/guard are preserved; `rest` is duplicated once per arm (bounded by the outer
    /// `MAX_DESUGARED_NODES` guard, and call-count-invariant so `mir == ir` holds).
    pub(crate) fn wrap_match_arms(
        subject: &IrExpr,
        arms: &[IrMatchArm],
        bind_var: VarId,
        bind_ty: &Ty,
        rest_stmts: &[IrStmt],
        rest_tail: &Option<Box<IrExpr>>,
        result_ty: &Ty,
    ) -> IrExpr {
        let new_arms: Vec<IrMatchArm> = arms
            .iter()
            .map(|a| IrMatchArm {
                pattern: a.pattern.clone(),
                guard: a.guard.clone(),
                body: Self::continuation_block(
                    &a.body, bind_var, bind_ty, rest_stmts, rest_tail, result_ty,
                ),
            })
            .collect();
        IrExpr {
            kind: IrExprKind::Match { subject: Box::new(subject.clone()), arms: new_arms },
            ty: result_ty.clone(),
            span: None,
            def_id: None,
        }
    }

    /// `{ let s = arm_value; <rest_stmts>; <rest_tail> }` typed `result_ty` — the continuation pushed
    /// behind the per-arm bind of `s`.
    fn continuation_block(
        arm_value: &IrExpr,
        bind_var: VarId,
        bind_ty: &Ty,
        rest_stmts: &[IrStmt],
        rest_tail: &Option<Box<IrExpr>>,
        result_ty: &Ty,
    ) -> IrExpr {
        let mut stmts: Vec<IrStmt> = Vec::with_capacity(rest_stmts.len() + 1);
        stmts.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: bind_var,
                mutability: almide_ir::Mutability::Let,
                ty: bind_ty.clone(),
                value: arm_value.clone(),
            },
            span: None,
        });
        stmts.extend(rest_stmts.iter().cloned());
        IrExpr {
            kind: IrExprKind::Block { stmts, expr: rest_tail.clone() },
            ty: result_ty.clone(),
            span: None,
            def_id: None,
        }
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
        // The Some-bind carries an is_heap flag. A SCALAR payload is a value COPY (load64). A HEAP
        // payload (Option[String]) is bound as a BORROW of the Option's element (LoadHandle =
        // i32, recorded in param_values), gated to a subject that is a nested-ownership list (so
        // the Option keeps ownership through its scope-end DropListStr; a consuming arm auto-Dups).
        let mut some: Option<(&IrExpr, Option<(VarId, bool, Ty)>)> = None;
        let mut none: Option<&IrExpr> = None;
        for arm in arms {
            match &arm.pattern {
                IrPattern::Some { inner } => {
                    let bind = match inner.as_ref() {
                        IrPattern::Bind { var, ty } if !is_heap_ty(ty) => Some((*var, false, ty.clone())),
                        IrPattern::Bind { var, ty }
                            if is_heap_ty(ty) && self.heap_elem_lists.contains(&subj) =>
                        {
                            Some((*var, true, ty.clone()))
                        }
                        IrPattern::Wildcard => None,
                        _ => return false, // heap bind w/o nested-ownership subject / nested ctor
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
        // Some-arm (then): extract the payload `data[0]`, bind it, lower the arm in a per-arm
        // frame. A SCALAR is a value COPY (load64); a HEAP element is `LoadHandle` (an i32 Ptr)
        // recorded in `param_values` (BORROWED) — the Option owns it (DropListStr frees it at
        // scope end), so the bound var is not a second owner; a consuming use auto-Dups.
        if let Some((bind_var, is_heap, bind_ty)) = some_bind {
            let payload = if is_heap {
                self.load_at_offset(h, 12, PrimKind::LoadHandle)
            } else {
                self.load_at_offset(h, 12, PrimKind::Load { width: 8 })
            };
            self.value_of.insert(bind_var, payload);
            if is_heap {
                self.param_values.insert(payload);
                // The Some payload is itself an Option/Result (`some(inner)` where
                // `inner: Option[Int]` — a NESTED match): track it so an INNER `match inner {…}`
                // BRANCHES (reads its tag @4) instead of LINEARIZING (running every arm). The
                // payload is a BORROWED handle of the OUTER Option's owned inner block — the same
                // materialized-Option read-shape, no new ownership. Without this the nested match
                // fell to the both-arms linearization (printing every arm + a garbage 0).
                use almide_lang::types::constructor::TypeConstructorId;
                if matches!(&bind_ty, Ty::Applied(TypeConstructorId::Option, _)) {
                    self.materialized_options.insert(payload);
                    if crate::lower::is_lenlist_list_ty(&bind_ty) {
                        self.variant_drop_handles.insert(payload, "list_lenlist".to_string());
                    } else if crate::lower::is_heap_elem_list_ty(&bind_ty) {
                        self.heap_elem_lists.insert(payload);
                    }
                } else if crate::lower::is_result_ty(&bind_ty) {
                    self.materialized_results.insert(payload);
                    if crate::lower::is_lenlist_list_ty(&bind_ty) {
                        self.variant_drop_handles.insert(payload, "list_lenlist".to_string());
                    } else if crate::lower::is_heap_elem_list_ty(&bind_ty) {
                        self.heap_elem_lists.insert(payload);
                    }
                }
            }
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
}

include!("control_p2.rs");
include!("control_p3.rs");
include!("control_p4.rs");
include!("control_p5.rs");

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

/// The STRUCTURAL gate for [`LowerCtx::materialize_unwrap_or_operand`] — does a `??` whose OPERAND
/// is `expr` lower to a synthetic unwrap-helper `CallFn` via that NEW path (so the caps counter must
/// credit the `UnwrapOr` node +1 to keep `mir_calls <= ir_calls`)? Pure (no `&self`), so the
/// `classify_corpus` counter consults the SAME admission the lowering uses — no count drift.
///
/// Two disjuncts, mirroring `materialize_unwrap_or_operand`:
///   1. a PURE `Module` variant call (`json.parse` — routed through `lower_call_args`); OR
///   2. an IMPURE `Module` `Option[String]` call (`process.env` — routed through the effect path).
/// A self-host-RECOGNIZED operand is NOT this path (it is materialized by the existing gate, and the
/// counter already credits it via `is_self_host_option_module_fn` / `is_self_host_result_*`), so this
/// is only the previously-unrecognized remainder.
pub fn unwrap_or_operand_admitted(expr: &IrExpr) -> bool {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    if !is_variant_ty(&expr.ty) {
        return false;
    }
    match &expr.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } => {
            crate::purity::is_pure(module.as_str(), func.as_str())
                || matches!(
                    &expr.ty,
                    Ty::Applied(TC::Option, a) if a.len() == 1 && matches!(a[0], Ty::String)
                )
        }
        _ => false,
    }
}

/// Detect the enumerate+map FUSION shape: `list.map(list.enumerate(real), (entry) => { let (i,key) =
/// entry; <tail> })`. Returns `(real, i_var, key_var, key_ty, tail)` — the inner iterates `real`
/// binding i=loop-index + key=element, running `<tail>` (the block minus the leading destructure), so
/// the `(Int,String)` intermediate list is never built. `None` if the shape doesn't match (the caller
/// keeps the ordinary map path). The COMMON enumerate idiom (CLAUDE.md's `cases |> list.enumerate |>
/// list.map((entry) => { let (idx, case) = entry; … })`).
/// Detect the zip+map FUSION shape: `list.map(list.zip(a, b), (pair) => <body using
/// pair.0 / pair.1>)` — the nn concat_cols idiom. Returns `(a, b, p0, t0, p1, t1,
/// new_body)` where `new_body` is `body` with every `pair.0` / `pair.1` REPLACED by
/// fresh vars `p0` / `p1` (bound by the fused loop to a[i] / b[i]), so the
/// `(A, B)` tuple-element intermediate list is NEVER built (no tuple alloc, no
/// tuple-list drop). Declines when `pair` is used any way OTHER than `.0`/`.1`
/// (the whole-tuple escape would need the real tuple).
fn detect_zip_map_fusion<'a>(
    xs: &'a IrExpr,
    params: &[(VarId, Ty)],
    body: &IrExpr,
) -> Option<(&'a IrExpr, &'a IrExpr, VarId, Ty, VarId, Ty, IrExpr)> {
    let (a, b) = match &xs.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
            if module.as_str() == "list" && func.as_str() == "zip" && args.len() == 2 =>
        {
            (&args[0], &args[1])
        }
        _ => return None,
    };
    if params.len() != 1 {
        return None;
    }
    let pair_var = params[0].0;
    let (t0, t1) = match &params[0].1 {
        Ty::Tuple(ts) if ts.len() == 2 => (ts[0].clone(), ts[1].clone()),
        _ => return None,
    };
    let base = {
        use almide_ir::visit::{walk_expr, IrVisitor};
        struct M(u32);
        impl IrVisitor for M {
            fn visit_expr(&mut self, e: &IrExpr) {
                if let IrExprKind::Var { id } = &e.kind {
                    self.0 = self.0.max(id.0);
                }
                walk_expr(self, e);
            }
        }
        let mut m = M(pair_var.0);
        m.visit_expr(body);
        m.0 + 1
    };
    let p0 = VarId(base);
    let p1 = VarId(base + 1);
    // Replace `pair.0`/`pair.1` with the fresh vars; flag any OTHER use of `pair`.
    fn rewrite(e: IrExpr, pair: VarId, p0: VarId, p1: VarId, escaped: &mut bool) -> IrExpr {
        if let IrExprKind::TupleIndex { object, index } = &e.kind {
            if let IrExprKind::Var { id } = &object.kind {
                if *id == pair && (*index == 0 || *index == 1) {
                    return IrExpr {
                        kind: IrExprKind::Var { id: if *index == 0 { p0 } else { p1 } },
                        ty: e.ty.clone(),
                        span: e.span.clone(),
                        def_id: e.def_id,
                    };
                }
            }
        }
        if let IrExprKind::Var { id } = &e.kind {
            if *id == pair {
                *escaped = true;
            }
        }
        e.map_children(&mut |c| rewrite(c, pair, p0, p1, escaped))
    }
    let mut escaped = false;
    let new_body = rewrite(body.clone(), pair_var, p0, p1, &mut escaped);
    if escaped {
        return None;
    }
    Some((a, b, p0, t0, p1, t1, new_body))
}

fn detect_enum_map_fusion<'a>(
    xs: &'a IrExpr,
    params: &[(VarId, Ty)],
    body: &IrExpr,
) -> Option<(&'a IrExpr, VarId, VarId, Ty, IrExpr)> {
    use almide_ir::{IrPattern, IrStmtKind};
    // xs = list.enumerate(real)
    let real = match &xs.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
            if module.as_str() == "list" && func.as_str() == "enumerate" && args.len() == 1 =>
        {
            &args[0]
        }
        _ => return None,
    };
    if params.len() != 1 {
        return None;
    }
    let entry_var = params[0].0;
    // body = Block { stmts: [ let (i,key) = entry, ...rest ], expr }
    let IrExprKind::Block { stmts, expr } = &body.kind else {
        return None;
    };
    let first = stmts.first()?;
    let IrStmtKind::BindDestructure { pattern: IrPattern::Tuple { elements }, value } = &first.kind
    else {
        return None;
    };
    if elements.len() != 2 {
        return None;
    }
    match &value.kind {
        IrExprKind::Var { id } if *id == entry_var => {}
        _ => return None,
    }
    let i_var = match &elements[0] {
        IrPattern::Bind { var, .. } => *var,
        _ => return None,
    };
    let (key_var, key_ty) = match &elements[1] {
        IrPattern::Bind { var, ty } => (*var, ty.clone()),
        _ => return None,
    };
    // tail = the block with the leading destructure removed (the remaining stmts + the block tail).
    let tail = IrExpr {
        kind: IrExprKind::Block { stmts: stmts[1..].to_vec(), expr: expr.clone() },
        ty: body.ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    };
    Some((real, i_var, key_var, key_ty, tail))
}

/// Detect the enumerate+FOLD FUSION shape: `list.fold(list.enumerate(real), init, (acc, entry) => {
/// let (i, key) = entry; <tail> })`. Returns `(real, i_var, acc_param, key_var, key_ty, tail)` — the
/// inner iterates `real` binding i=loop-index + key=element (the acc param is preserved), running
/// `<tail>` (the block minus the leading destructure), so the `(Int,String)` intermediate list is never
/// built. The 2-param sibling of `detect_enum_map_fusion` (`acc` is `params[0]`, the enumerated `entry`
/// is `params[1]`). `None` if the shape doesn't match. The `find_flag` idiom (`args |> list.enumerate |>
/// list.fold("", (acc, entry) => { let (i, arg) = entry; if arg == flag then … else acc })`).
fn detect_enum_fold_fusion<'a>(
    xs: &'a IrExpr,
    params: &[(VarId, Ty)],
    body: &IrExpr,
) -> Option<(&'a IrExpr, VarId, (VarId, Ty), VarId, Ty, IrExpr)> {
    use almide_ir::{IrPattern, IrStmtKind};
    let real = match &xs.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
            if module.as_str() == "list" && func.as_str() == "enumerate" && args.len() == 1 =>
        {
            &args[0]
        }
        _ => return None,
    };
    if params.len() != 2 {
        return None;
    }
    let acc_param = params[0].clone();
    let entry_var = params[1].0;
    let IrExprKind::Block { stmts, expr } = &body.kind else {
        return None;
    };
    // The entry destructure may be the FIRST or SECOND stmt (an acc destructure
    // can precede it — the argmax `let (bi,bv)=acc; let (i,v)=entry; …` shape).
    let entry_at = stmts.iter().take(2).position(|st| {
        matches!(&st.kind,
            IrStmtKind::BindDestructure { pattern: IrPattern::Tuple { elements }, value }
                if elements.len() == 2
                    && matches!(&value.kind, IrExprKind::Var { id } if *id == entry_var))
    })?;
    let IrStmtKind::BindDestructure { pattern: IrPattern::Tuple { elements }, .. } =
        &stmts[entry_at].kind
    else {
        return None;
    };
    let i_var = match &elements[0] {
        IrPattern::Bind { var, .. } => *var,
        _ => return None,
    };
    let (key_var, key_ty) = match &elements[1] {
        IrPattern::Bind { var, ty } => (*var, ty.clone()),
        _ => return None,
    };
    let mut rest: Vec<almide_ir::IrStmt> = stmts.to_vec();
    rest.remove(entry_at);
    let tail = IrExpr {
        kind: IrExprKind::Block { stmts: rest, expr: expr.clone() },
        ty: body.ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    };
    Some((real, i_var, acc_param, key_var, key_ty, tail))
}

fn is_self_host_result_call(subject: &IrExpr) -> bool {
    match &subject.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } => {
            is_self_host_result_module_fn(module.as_str(), func.as_str())
        }
        _ => false,
    }
}

/// Is the match subject a self-host call returning a HEAP-Ok Result (`result.zip` /
/// `value.as_string` — the cap-as-tag 1-slot DynListStr)? Drives the `materialized_results_str` +
/// `heap_elem_lists` tracking so a direct `match` over it executes (binds the @12 payload handle).
fn is_self_host_result_str_call(subject: &IrExpr) -> bool {
    match &subject.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } => {
            crate::lower::is_self_host_result_str_module_fn(module.as_str(), func.as_str())
        }
        _ => false,
    }
}
