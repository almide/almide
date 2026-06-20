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
                        } else if crate::lower::is_heap_elem_list_ty(&subject.ty) {
                            self.heap_elem_lists.insert(v);
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
                // A nested Unit `if` arm-tail EXECUTES (only the taken arm runs) — so a
                // chained `else if … else …` (fizzbuzz) runs ONE branch, not all of them;
                // else it falls back to linearization.
                IrExprKind::If { cond, then, else_ }
                    if self.try_lower_unit_if(cond, then, else_) => {}
                IrExprKind::If { .. } | IrExprKind::Match { .. } => self.lower_branch(tail)?,
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
        let mut some: Option<(&IrExpr, Option<(VarId, bool)>)> = None;
        let mut none: Option<&IrExpr> = None;
        for arm in arms {
            match &arm.pattern {
                IrPattern::Some { inner } => {
                    let bind = match inner.as_ref() {
                        IrPattern::Bind { var, ty } if !is_heap_ty(ty) => Some((*var, false)),
                        IrPattern::Bind { var, ty }
                            if is_heap_ty(ty) && self.heap_elem_lists.contains(&subj) =>
                        {
                            Some((*var, true))
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
        if let Some((bind_var, is_heap)) = some_bind {
            let payload = if is_heap {
                self.load_at_offset(h, 12, PrimKind::LoadHandle)
            } else {
                self.load_at_offset(h, 12, PrimKind::Load { width: 8 })
            };
            self.value_of.insert(bind_var, payload);
            if is_heap {
                self.param_values.insert(payload);
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

    /// EXECUTE a `match r { Ok(v) => …, Err(e) => … }` over a MATERIALIZED Result — only the taken
    /// arm runs. The Result analogue of [`Self::try_lower_variant_match`], reusing the same
    /// per-arm-balanced cert: the markers no-op in `verify_ownership`, each arm is a per-arm frame,
    /// the tag/payload reads are scalar prims. The len-as-tag is INVERSE of Option: `len == 0` = Ok
    /// (the value is a scalar slot-0 COPY, load64), `len != 0` = Err (the message is a borrowed
    /// `LoadHandle` of slot 0 — the Result owns it, freed by the scope-end DropListStr, so the bound
    /// var is not a second owner). SOUNDNESS — gated on `subject ∈ materialized_results`: only a
    /// known DynListStr-Result is read len-as-tag; any other (deferred `Opaque`, len 0) would
    /// MISREAD as Ok, so it is not in the set and keeps the sound LINEARIZED match.
    pub(crate) fn try_lower_result_match(
        &mut self,
        subject_value: Option<ValueId>,
        arms: &[IrMatchArm],
    ) -> bool {
        use crate::PrimKind;
        // A HEAP-Ok `Result[String, String]` (cap-as-tag, Ok binds a heap String) vs the scalar
        // `Result[Int, String]` (len-as-tag, Ok binds a scalar int).
        let (subj, str_result) = match subject_value {
            Some(v) if self.materialized_results_str.contains(&v) => (v, true),
            Some(v) if self.materialized_results.contains(&v) => (v, false),
            _ => return false,
        };
        if arms.len() != 2 || arms.iter().any(|a| a.guard.is_some()) {
            return false;
        }
        // Exactly [Ok(scalar-bind?), Err(heap-bind?)], no nested ctors, Unit bodies. An Ok binds a
        // SCALAR Int (value copy); an Err binds a heap String (borrowed slot-0 handle), gated to a
        // nested-ownership subject (so the Result keeps ownership through its DropListStr).
        let mut ok: Option<(&IrExpr, Option<VarId>)> = None;
        let mut err: Option<(&IrExpr, Option<(VarId, bool)>)> = None;
        for arm in arms {
            match &arm.pattern {
                IrPattern::Ok { inner } => {
                    let bind = match inner.as_ref() {
                        // Scalar Ok (Result[Int,String]) binds a scalar int; a heap-Ok
                        // (Result[String,String]) binds a heap String — gated to `str_result`.
                        IrPattern::Bind { var, ty } if is_heap_ty(ty) == str_result => Some(*var),
                        IrPattern::Wildcard => None,
                        _ => return false,
                    };
                    if ok.is_some() {
                        return false;
                    }
                    ok = Some((&arm.body, bind));
                }
                IrPattern::Err { inner } => {
                    let bind = match inner.as_ref() {
                        IrPattern::Bind { var, ty }
                            if is_heap_ty(ty) && self.heap_elem_lists.contains(&subj) =>
                        {
                            Some((*var, true))
                        }
                        IrPattern::Wildcard => None,
                        _ => return false,
                    };
                    if err.is_some() {
                        return false;
                    }
                    err = Some((&arm.body, bind));
                }
                _ => return false,
            }
        }
        let ((ok_body, ok_bind), (err_body, err_bind)) = match (ok, err) {
            (Some(o), Some(e)) => (o, e),
            _ => return false,
        };
        if !matches!(ok_body.ty, Ty::Unit) || !matches!(err_body.ty, Ty::Unit) {
            return false;
        }
        // tag = load32(handle(subj) + 4); if tag != 0 then Err-arm else Ok-arm (len 0 = Ok).
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        // The tag is the HIGH 32 bits of slot 0 (@16) for a heap-Ok Result (the low 32 bits @12 hold
        // the owned String handle), `len` (@4) for a scalar one.
        let tag_off = if str_result { 16 } else { 4 };
        let tag = self.load_at_offset(h, tag_off, PrimKind::Load { width: 4 });
        self.ops.push(Op::IfThen { cond: tag, dst: None });
        // THEN (tag != 0 = Err): the message is the BORROWED slot-0 handle.
        if let Some((bind_var, _)) = err_bind {
            let payload = self.load_at_offset(h, 12, PrimKind::LoadHandle);
            self.value_of.insert(bind_var, payload);
            self.param_values.insert(payload);
        }
        if self.lower_branch_arm(None, err_body).is_err() {
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return false;
        }
        self.ops.push(Op::Else { val: None });
        // ELSE (tag == 0 = Ok): a scalar Result yields the slot-0 int COPY; a heap-Ok Result yields
        // the BORROWED slot-0 String handle (the Result keeps ownership through its DropListStr).
        if let Some(bind_var) = ok_bind {
            if str_result {
                let payload = self.load_at_offset(h, 12, PrimKind::LoadHandle);
                self.value_of.insert(bind_var, payload);
                self.param_values.insert(payload);
            } else {
                let payload = self.load_at_offset(h, 12, PrimKind::Load { width: 8 });
                self.value_of.insert(bind_var, payload);
            }
        }
        if self.lower_branch_arm(None, ok_body).is_err() {
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return false;
        }
        self.ops.push(Op::EndIf { val: None });
        true
    }

    /// VALUE-position variant match: a `match opt { Some(x) => <scalar>, None => <scalar> }`
    /// (or `Ok/Err`) used as an OPERAND / let / call-argument EXECUTES to a SCALAR `dst` —
    /// read the tag, run ONLY the taken arm, bind the scalar payload. The value analogue of
    /// [`Self::try_lower_variant_match`] / [`Self::try_lower_result_match`] (which require
    /// UNIT arms): without it a ctor-pattern value match desugared to nothing (a `Some`/`Ok`
    /// pattern is not `subj == lit`) and the result local stayed UNSET = a silent 0.
    /// Returns `None` (rolled back) outside the subset — the caller then WALLs (a Const-0
    /// would silently pick a wrong arm).
    ///
    /// SOUNDNESS — the subject is materialized/borrowed by `lower_call_args` (an owned ctor
    /// temp drops at scope end via `live_heap_handles`; a tracked Var borrows), gated on
    /// `∈ materialized_options/results` so the len-as-tag read is only over a value KNOWN to
    /// carry the layout (`Some`=len1 / `None`=len0; scalar `Ok`=len0 / `Err`=len≠0). The
    /// tag/payload reads are scalar prims, the `IfThen`/`Else`/`EndIf` markers no-op in
    /// `verify_ownership`, and each arm is a scalar value with NO heap ownership event —
    /// exactly the per-arm-balanced linearization the cert already proves, wrapped so one
    /// arm runs. The enclosing `lower_scalar_value` self-rollback restores `ops` +
    /// `live_heap_handles` on a miss, so the subject materialize is rollback-safe. SCALAR
    /// payload + SCALAR result only (a heap-result variant match merges heap arms — later).
    pub(crate) fn try_lower_variant_value_match(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        // SCALAR result, OR a HEAP result over a SCALAR-PAYLOAD variant via the
        // SUBJECT-DROP-BEFORE-ARMS desugar (below): copy the scalar tag/payload, DROP the
        // owned subject BEFORE the arms, then run the proven heap-result-`if` (scalar cond) —
        // so the arm's per-arm heap move-out no longer overlaps the owned-subject borrow the
        // checker rejected. A HEAP-PAYLOAD variant (`Option[String]` — the arm borrows the
        // subject's slot, no scalar copy possible) stays the true Camp-4 frontier and is
        // gated out below.
        if !is_heap_ty(&subject.ty)
            || arms.len() != 2
            || arms.iter().any(|a| a.guard.is_some())
        {
            return None;
        }
        // DESUGAR a tuple Some/Ok payload — `some((idx, line)) => B` → `some($p) => { let (idx,line) = $p; B }`.
        // The single var `$p` is bound below via the HEAP-payload path (into `param_values`), so the
        // `let (idx,line) = $p` tuple destructure then lowers (`try_lower_tuple_destructure` borrows each
        // slot). A raw tuple VAR/param destructure alone walls (no `param_values` entry), so the rewrite to
        // the @12-handle bind is required, not a plain var destructure. `$p` ids start above subject+arms.
        let has_tuple_payload = arms.iter().any(|a| {
            matches!(&a.pattern, IrPattern::Some { inner } | IrPattern::Ok { inner }
                if matches!(&**inner, IrPattern::Tuple { .. }))
        });
        let desugared: Vec<IrMatchArm>;
        let arms: &[IrMatchArm] = if has_tuple_payload {
            let mut next = arms
                .iter()
                .map(|a| crate::lower::max_var_id(&a.body))
                .max()
                .unwrap_or(0)
                .max(crate::lower::max_var_id(subject))
                + 1;
            let mut out: Vec<IrMatchArm> = Vec::with_capacity(arms.len());
            for a in arms {
                let inner_tuple = match &a.pattern {
                    IrPattern::Some { inner } | IrPattern::Ok { inner } => match &**inner {
                        IrPattern::Tuple { elements } => Some(elements.clone()),
                        _ => None,
                    },
                    _ => None,
                };
                let Some(elements) = inner_tuple else {
                    out.push(a.clone());
                    continue;
                };
                let p = VarId(next);
                next += 1;
                let tuple_ty = Ty::Tuple(
                    elements
                        .iter()
                        .map(|e| match e {
                            IrPattern::Bind { ty, .. } => ty.clone(),
                            _ => Ty::Unknown,
                        })
                        .collect(),
                );
                let p_inner = Box::new(IrPattern::Bind { var: p, ty: tuple_ty.clone() });
                let new_pat = match &a.pattern {
                    IrPattern::Some { .. } => IrPattern::Some { inner: p_inner },
                    _ => IrPattern::Ok { inner: p_inner },
                };
                let destr = IrStmt {
                    kind: IrStmtKind::BindDestructure {
                        pattern: IrPattern::Tuple { elements },
                        value: IrExpr {
                            kind: IrExprKind::Var { id: p },
                            ty: tuple_ty,
                            span: None,
                            def_id: None,
                        },
                    },
                    span: None,
                };
                let body = IrExpr {
                    kind: IrExprKind::Block {
                        stmts: vec![destr],
                        expr: Some(Box::new(a.body.clone())),
                    },
                    ty: a.body.ty.clone(),
                    span: a.body.span.clone(),
                    def_id: a.body.def_id,
                };
                out.push(IrMatchArm { pattern: new_pat, guard: a.guard.clone(), body });
            }
            desugared = out;
            &desugared
        } else {
            arms
        };
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let rollback = |s: &mut Self| {
            s.ops.truncate(ops_mark);
            s.live_heap_handles.truncate(lhh_mark);
            None
        };
        // Materialize/borrow + track the subject exactly as the statement Match entry does:
        // an owned ctor temp (`Some(5)`) is dropped at scope end; a tracked Var (`let o =
        // Some(5)`) is borrowed; a self-host Option/Result-returning call is tracked here.
        let subj = match self
            .lower_call_args(std::slice::from_ref(subject))
            .ok()
            .and_then(|a| a.into_iter().next())
        {
            Some(CallArg::Handle(v)) => v,
            _ => return rollback(self),
        };
        // A USER-FN `Named` call returning Option/Result is tracked the SAME as a self-host call: every
        // Option/Result value uses the one DynListStr len-as-tag repr (brick #51), so `match find_colon(t)
        // { none => …, some(cp) => … }` over `fn find_colon(..) -> Option[Int]` reads the tag @4 + binds the
        // scalar payload @12 identically. `subj` is the OWNED call result (live, dropped-before for a scalar
        // payload), and the tracking is per-subject. A HEAP-Ok user Result still self-gates (heap_or_scalar_
        // bind requires a str-result), so only scalar payloads lower — never a silently-wrong heap move-out.
        let is_named_call =
            matches!(&subject.kind, IrExprKind::Call { target: CallTarget::Named { .. }, .. });
        if is_self_host_option_call(subject)
            || (is_named_call
                && crate::lower::is_variant_ty(&subject.ty)
                && !crate::lower::is_result_ty(&subject.ty))
        {
            self.materialized_options.insert(subj);
        }
        if is_self_host_result_call(subject)
            || (is_named_call && crate::lower::is_result_ty(&subject.ty))
        {
            self.materialized_results.insert(subj);
        }
        // A self-host HEAP-Ok Result (`value.as_string`/`value.as_array`/`result.zip` — cap-as-tag
        // DynListStr) is tracked as a str-result (the match reads tag @16 + binds the @12 payload
        // handle). The DROP differs by Ok-arm type: a `List[Value]` Ok (`value.as_array`) frees
        // RECURSIVELY (`value_result_lists` → `DropResultListValue`), else a String Ok frees flat
        // (`heap_elem_lists` → `DropListStr`). Type-driven so it is sound at every tracking site.
        if is_self_host_result_str_call(subject) {
            self.materialized_results_str.insert(subj);
            if crate::lower::is_result_listval_ty(&subject.ty) {
                self.value_result_lists.insert(subj);
            } else {
                self.heap_elem_lists.insert(subj);
            }
        }
        // Dispatch on the tracking set. An Option reads len-as-tag (Some=len≠0); a scalar
        // Result reads len-as-tag INVERSE (Err=len≠0, Ok=len0). The if-skeleton is uniform
        // (then = tag≠0, else = tag==0): Option → then=Some/else=None; Result → then=Err/else=Ok.
        let is_option = self.materialized_options.contains(&subj);
        // A scalar Result reads len-as-tag (@4); a HEAP-Ok `Result[String,String]` (value.as_string,
        // the cap-as-tag DynListStr) reads the tag at the slot-0 HIGH 32 bits (@16). Both arrange
        // Err=then(tag≠0)/Ok=else(tag0); only the tag OFFSET differs. A str-result match here is
        // ADMITTED for WILDCARD/scalar binds (`match value.as_string(v) { ok(_) => …, err(_) => … }`
        // — is_scalar_type); a heap-payload bind over a str-result (`ok(s: String)`) is the Camp-4
        // borrowed-slot case → gated out below (heap_or_scalar_bind already requires heap_elem_lists,
        // and the heap-RESULT branch defers it).
        let is_result_str = self.materialized_results_str.contains(&subj);
        let is_result = self.materialized_results.contains(&subj) || is_result_str;
        if !is_option && !is_result {
            return rollback(self);
        }
        // Parse the two arms into (then_body, then_bind, else_body, else_bind) where a bind is
        // an optional SCALAR payload var (`Some(x)` / `Ok(x)` / a scalar `Err(c)`). A heap bind
        // (`Err(msg: String)`) is allowed only when the arm body never needs it as an owner —
        // here it is bound as a BORROW of the Result's owned slot-0 handle, gated on the subject
        // being a nested-ownership list (it frees the payload at scope end). A wildcard binds nothing.
        let scalar_bind = |inner: &IrPattern| -> Result<Option<(VarId, bool)>, ()> {
            match inner {
                IrPattern::Bind { var, ty } if !is_heap_ty(ty) => Ok(Some((*var, false))),
                // A heap TUPLE payload (`some((idx,line))` desugared to `some($p)`): bind @12 as the
                // tuple-handle BORROW (the desugared `let (idx,line)=$p` then destructures each slot —
                // sound because `$p` lands in `param_values`). The subject drops AFTER the arms (the
                // tuple stays live through them); a move-out arm auto-`Dup`s in `lower_heap_result_arm`.
                IrPattern::Bind { var, ty } if is_heap_ty(ty) && matches!(ty, Ty::Tuple(_)) => {
                    Ok(Some((*var, true)))
                }
                IrPattern::Wildcard => Ok(None),
                _ => Err(()),
            }
        };
        let heap_or_scalar_bind = |s: &Self, inner: &IrPattern| -> Result<Option<(VarId, bool)>, ()> {
            match inner {
                IrPattern::Bind { var, ty } if !is_heap_ty(ty) => Ok(Some((*var, false))),
                // A heap payload bind is admitted over a nested-ownership subject — a str-result
                // (`heap_elem_lists`, the `value.as_string` String payload) OR a value-array result
                // (`value_result_lists`, the `value.as_array` `List[Value]` payload, e.g. `ok(items)
                // => emit_seq(items)`). Both bind the @12 handle as a BORROW (drop-subject-after).
                IrPattern::Bind { var, ty }
                    if is_heap_ty(ty)
                        && (s.heap_elem_lists.contains(&subj) || s.value_result_lists.contains(&subj)) =>
                {
                    Ok(Some((*var, true)))
                }
                IrPattern::Wildcard => Ok(None),
                _ => Err(()),
            }
        };
        let mut then_slot: Option<(&IrExpr, Option<(VarId, bool)>)> = None;
        let mut else_slot: Option<(&IrExpr, Option<(VarId, bool)>)> = None;
        for arm in arms {
            let parsed: Result<(bool, Option<(VarId, bool)>), ()> = match &arm.pattern {
                // Option Some (then) / None (else).
                IrPattern::Some { inner } if is_option => scalar_bind(inner).map(|b| (true, b)),
                IrPattern::None | IrPattern::Wildcard if is_option => Ok((false, None)),
                // Result Err (then) / Ok (else). BOTH use `heap_or_scalar_bind`: a scalar Result
                // binds a scalar payload, a str-result (`value.as_string`) binds its slot-0 String
                // as a BORROW (gated on `heap_elem_lists` — only a nested-ownership subject, so a
                // scalar Result still rejects a heap bind). The Ok side carries the str-result's
                // String payload (`ok(s) => emit_scalar(s)`), the very thing `emit` needs.
                IrPattern::Err { inner } if is_result => {
                    heap_or_scalar_bind(self, inner).map(|b| (true, b))
                }
                IrPattern::Ok { inner } if is_result => {
                    heap_or_scalar_bind(self, inner).map(|b| (false, b))
                }
                _ => Err(()),
            };
            match parsed {
                Ok((true, bind)) if then_slot.is_none() => then_slot = Some((&arm.body, bind)),
                Ok((false, bind)) if else_slot.is_none() => else_slot = Some((&arm.body, bind)),
                _ => return rollback(self),
            }
        }
        let ((then_body, then_bind), (else_body, else_bind)) = match (then_slot, else_slot) {
            (Some(t), Some(e)) => (t, e),
            _ => return rollback(self),
        };
        let heap_res = is_heap_ty(result_ty);
        let has_heap_bind =
            matches!(then_bind, Some((_, true))) || matches!(else_bind, Some((_, true)));
        // A HEAP result with a HEAP-PAYLOAD bind is admitted ONLY over a str-result
        // (`value.as_string` — slot-0 @12 owns the ONE String, the Ok/Err tag at @16). The
        // payload binds as a BORROW (`LoadHandle` @12, in `param_values`), the OWNED subject is
        // dropped AFTER the arms (not before) so the borrow is live through them, and a bare-Var
        // arm (`ok(s) => s`) auto-acquires (`Op::Dup`) — so the drop-after frees the subject's
        // slot-0 String exactly once whether an arm borrows it (a call arg) or returns it. The
        // `emit` shape (`match value.as_string(v) { ok(s) => emit_scalar(s), err(_) => … }`) is
        // exactly this. A NON-str heap payload (a heap-Result-of-list, an Array element) has no
        // single-slot borrow rep yet — the true Camp-4 frontier — so it still defers.
        let str_heap_bind = heap_res && has_heap_bind && is_result_str;
        // The Option-tuple payload (`some((idx,line))`): a heap bind over an OPTION subject is always
        // the desugared tuple-handle borrow (scalar_bind only returns heap for a `Ty::Tuple`). It is
        // handled exactly like `str_heap_bind` — borrow @12, subject drops AFTER the arms — but reads
        // the Option len-as-tag @4 (not the str-result cap-tag @16).
        let opt_tuple_bind = heap_res && has_heap_bind && is_option;
        if heap_res && has_heap_bind && !is_result_str && !opt_tuple_bind {
            return rollback(self);
        }
        // Emit: h = handle(subj); tag = load32(h + off); dst = if tag != 0 then <then> else <else>.
        // A scalar Option/Result reads len-as-tag (@4); a heap-Ok `Result[String,String]`
        // (value.as_string) reads the cap-as-tag at the slot-0 HIGH 32 bits (@16).
        let tag_off = if is_result_str { 16 } else { 4 };
        let dst = self.fresh_value();
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        let tag = self.load_at_offset(h, tag_off, PrimKind::Load { width: 4 });
        // Bind the scalar payload(s) as subj-independent COPIES (load64 @12) BEFORE the arms —
        // for the heap-result case this is what severs the arm's heap move-out from the subject.
        let bind_payload = |s: &mut Self, bind: Option<(VarId, bool)>| {
            if let Some((bind_var, is_heap)) = bind {
                let payload = if is_heap {
                    s.load_at_offset(h, 12, PrimKind::LoadHandle)
                } else {
                    s.load_at_offset(h, 12, PrimKind::Load { width: 8 })
                };
                s.value_of.insert(bind_var, payload);
                if is_heap {
                    s.param_values.insert(payload);
                }
            }
        };
        bind_payload(self, then_bind);
        bind_payload(self, else_bind);
        // SUBJECT-DROP-BEFORE-ARMS (the design that the checker accepts): for a HEAP result,
        // drop the OWNED subject NOW — before the arms — so its lifetime (`i..d`, balanced and
        // independent) does not overlap the per-arm heap move-out + branch-join (which is then
        // exactly the proven heap-result-`if` over a scalar cond). A BORROWED subject (param /
        // tracked var, not in `live_heap_handles`) is owned elsewhere → left untouched; the
        // scalar payload copy above already makes the arms subj-independent. Scalar-result
        // matches keep the subject live (unchanged — they were already proven). A str-result
        // HEAP-bind (`str_heap_bind`) is the exception: its payload BORROWS slot-0, so the subject
        // must stay live THROUGH the arms — its drop is deferred to AFTER the branch-join below.
        if heap_res && !str_heap_bind && !opt_tuple_bind {
            if let Some(pos) = self.live_heap_handles.iter().rposition(|&v| v == subj) {
                self.live_heap_handles.remove(pos);
                let op = self.drop_op_for(subj);
                self.ops.push(op);
            }
        }
        self.ops.push(Op::IfThen { cond: tag, dst: Some(dst) });
        let lower_arm = |s: &mut Self, body: &IrExpr| -> Option<ValueId> {
            if heap_res {
                s.lower_heap_result_arm(body, result_ty)
            } else {
                s.lower_scalar_arm(body)
            }
        };
        // BRANCH OWNERSHIP ISOLATION: the two arms are ALTERNATE (only one runs), so each must lower
        // from the SAME ownership state. A borrowed param consumed in the THEN arm (`pairs + [(k,v)]`)
        // must still be available to the ELSE arm (`value.object(pairs)`) and vice versa — without this
        // the THEN arm's `Consume`/move leaks into the ELSE arm's lowering-time view and the ELSE arm
        // walls. Snapshot the owned/borrowed sets before THEN, restore them before ELSE (the emitted ops
        // are per-branch; only the lowering-time tracking is reset). The shared payload binds (cp, $p)
        // were inserted before IfThen, so they survive in both.
        let pv_snapshot = self.param_values.clone();
        let lhh_snapshot = self.live_heap_handles.clone();
        let ma_snapshot = self.materialized_aggregates.clone();
        // THEN (tag != 0): the Some payload / the Err message.
        let then_val = match lower_arm(self, then_body) {
            Some(v) => v,
            None => return rollback(self),
        };
        self.ops.push(Op::Else { val: Some(then_val) });
        self.param_values = pv_snapshot;
        self.live_heap_handles = lhh_snapshot;
        self.materialized_aggregates = ma_snapshot;
        // ELSE (tag == 0): the None branch / the scalar Ok payload.
        let else_val = match lower_arm(self, else_body) {
            Some(v) => v,
            None => return rollback(self),
        };
        self.ops.push(Op::EndIf { val: Some(else_val) });
        // SUBJECT-DROP-AFTER-ARMS (the str-result heap-bind path): the payload borrowed slot-0, so
        // the subject stayed live through both arms — drop the OWNED subject ONCE here, after the
        // branch-join. The merged result `dst` is a fresh arm value (a concat, a Dup'd copy, a new
        // call result), independent of the freed subject, so freeing the subject's slot-0 String is
        // sound (a bare-Var arm already Dup'd it; a call arm only borrowed it). A BORROWED subject
        // (param / tracked var, not in `live_heap_handles`) is owned elsewhere → left untouched.
        if str_heap_bind || opt_tuple_bind {
            if let Some(pos) = self.live_heap_handles.iter().rposition(|&v| v == subj) {
                self.live_heap_handles.remove(pos);
                let op = self.drop_op_for(subj);
                self.ops.push(op);
            }
        }
        Some(dst)
    }

    /// If `ty` is a CUSTOM variant (a user ADT) with a registered [`VariantLayout`], return its
    /// type name. Handles the three surface forms a variant type takes
    /// (`Named` / inline `Variant` / `Applied(UserDefined)`). `None` for Option/Result (those
    /// use the dedicated len-as-tag path) and every non-variant type.
    pub(crate) fn custom_variant_type_name(&self, ty: &Ty) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        let name = match ty {
            Ty::Named(n, _) => n.as_str().to_string(),
            Ty::Variant { name, .. } => name.as_str().to_string(),
            Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.clone(),
            _ => return None,
        };
        self.variant_layouts.by_type.contains_key(&name).then_some(name)
    }

    /// EXECUTE a `match v { Ctor(binds…) => arm, … }` over a CUSTOM variant (ADT brick 3) —
    /// read the tag from slot 0 and dispatch to the matching arm; only the taken arm runs. The
    /// N-constructor generalization of [`Self::try_lower_variant_value_match`] (the 2-variant
    /// Option/Result case), over the v1 tag@slot0 + i64-slot value model (NOT the len-as-tag
    /// Option/Result repr). Returns the scalar result `dst`, or `None` (rolled back) outside
    /// the subset — the caller then walls (a Const-0 would silently pick a wrong arm — the ②
    /// cardinal rule).
    ///
    /// SUBSET: SCALAR result (ADT brick 3) OR a HEAP result over a BORROWED subject (ADT brick 4,
    /// e.g. recursive `to_string` — each arm reads the borrowed subject's scalar slots and moves
    /// out a fresh heap value). SCALAR ctor-field binds only (a heap/nested ctor field = ADT
    /// brick 5). No guards. An OWNED-temp subject with a heap result would need
    /// subject-drop-before-arms (the cert rejects the owned-borrow / arm-move-out overlap) — it
    /// WALLS here rather than emit cert-failing MIR (ADT brick 4b).
    ///
    /// SOUNDNESS: the subject is materialized/borrowed by `lower_call_args` (an owned ctor temp
    /// drops at scope end via `live_heap_handles`; a tracked Var/param borrows). The tag/field
    /// reads are scalar prims, the `IfThen`/`Else`/`EndIf` markers no-op in `verify_ownership`,
    /// and each arm is a per-arm-balanced frame (`lower_scalar_arm` / `lower_heap_result_arm`)
    /// with NO heap ownership event beyond the arm's own move-out — exactly the per-arm
    /// linearization the cert proves, wrapped so one arm runs. The LAST arm is the unconditional
    /// `else` (the frontend guarantees the match is exhaustive).
    pub(crate) fn try_lower_custom_variant_match(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        if arms.is_empty() || arms.iter().any(|a| a.guard.is_some()) {
            return None;
        }
        // The subject must be a registered custom variant; clone its layout out of the borrow.
        let type_name = self.custom_variant_type_name(&subject.ty)?;
        let layout = self.variant_layouts.by_type.get(&type_name)?.clone();
        let plans = self.parse_variant_arms(&layout, arms)?;
        // A SINGLE-arm HEAP-result match (a 1-ctor newtype `unbox`, `match b { B(x) => x }`)
        // returns the arm value DIRECTLY to `func.ret` — with no IfThen branch-merge `dst` to
        // route through, the arm's move-out `Consume` (`m`) double-moves with the ret's move
        // (`m`), the `amm`/`aamdm` net-−1 the proven checker REJECTS. A multi-arm match routes
        // through the IfThen `dst` (one ret move). Wall the degenerate single-arm heap case
        // (ADT brick 5c-newtype) rather than emit a checker-rejected double-move.
        if is_heap_ty(result_ty) && plans.len() == 1 {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        // Materialize/borrow the subject → a Handle (the variant block pointer).
        let subj = match self
            .lower_call_args(std::slice::from_ref(subject))
            .ok()
            .and_then(|a| a.into_iter().next())
        {
            Some(CallArg::Handle(v)) => v,
            _ => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        // A HEAP result over an OWNED subject temp would overlap the owned-subject borrow with the
        // arm's heap move-out (the cert rejects it). Subject-drop-before-arms is ADT brick 4b —
        // for now WALL it (a borrowed param/var subject, the recursive-to_string case, proceeds).
        if is_heap_ty(result_ty) && self.live_heap_handles.contains(&subj) {
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return None;
        }
        // Read the tag from slot 0, then emit the per-arm if-chain.
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        let tag = self.load_at_offset(h, layout::slot_offset(0) as i64, PrimKind::Load { width: 8 });
        match self.emit_variant_arm_chain(h, tag, &plans, result_ty) {
            Some(dst) => Some(dst),
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                None
            }
        }
    }

    /// Parse a custom-variant `match`'s arms into per-arm plans — shared by the value-result
    /// ([`Self::try_lower_custom_variant_match`]) and Unit-statement
    /// ([`Self::lower_custom_variant_unit_match`]) paths. `None` (the caller walls / declines)
    /// if any arm is outside the scalar-field subset: a guard, a heap-field bind, a nested ctor
    /// pattern, or a binder catch-all `x => …` (all later bricks). The bodies stay borrowed
    /// from `arms` (a param, not `self`) — no borrow conflict with the lowering that follows.
    fn parse_variant_arms<'a>(
        &self,
        layout: &VariantLayout,
        arms: &'a [IrMatchArm],
    ) -> Option<Vec<(VariantArmKind, &'a IrExpr)>> {
        let mut plans: Vec<(VariantArmKind, &IrExpr)> = Vec::with_capacity(arms.len());
        for arm in arms {
            if arm.guard.is_some() {
                return None;
            }
            let kind = match &arm.pattern {
                IrPattern::Constructor { name, args } => {
                    let case = layout.case_by_ctor(name)?;
                    if args.len() != case.fields.len() {
                        return None;
                    }
                    let mut binds = Vec::new();
                    for (i, fp) in args.iter().enumerate() {
                        match fp {
                            IrPattern::Wildcard => {}
                            // slot 1+i (slot 0 is the tag). A SCALAR field binds by value copy.
                            IrPattern::Bind { var, ty } if !is_heap_ty(ty) => {
                                binds.push((1 + i, *var, false))
                            }
                            // A leaf-heap (`String`) OR a nested-VARIANT field binds as a borrow of
                            // the slot handle (the subject owns it; a move-out auto-Dups, a
                            // borrow-pass like `tos(l)` just reads — ADT brick 5c, extended to
                            // variant fields for the recursive `Expr` to_string lever).
                            IrPattern::Bind { var, ty }
                                if matches!(ty, Ty::String)
                                    || self.variant_layouts.field_is_variant(ty) =>
                            {
                                binds.push((1 + i, *var, true))
                            }
                            // a List/other heap field / nested ctor pattern — a later brick.
                            _ => return None,
                        }
                    }
                    VariantArmKind::Ctor { tag: case.tag as i64, binds }
                }
                IrPattern::Wildcard => VariantArmKind::Wildcard,
                // a binder catch-all `x => …` over a variant — walled for now (later brick)
                _ => return None,
            };
            plans.push((kind, &arm.body));
        }
        Some(plans)
    }

    /// Bind a custom-variant arm's ctor fields from the block's slots (a `Wildcard` arm binds
    /// nothing). A SCALAR field is an i64 value COPY (`Load`); a leaf-heap (`String`) field is a
    /// `Dup`'d OWNED copy of the slot handle (`LoadHandle` then `Op::Dup`, rc+1) pushed to
    /// `live_heap_handles` so the ARM FRAME drops it at arm end (`emit_variant_arm_chain` marks
    /// before this call). The OWNED copy — not a borrow — is what the proven checker needs: a
    /// consuming re-use moves an owned ref, a read-only use drops it, a move-out hands it off,
    /// all rc-balanced; a BORROW would `Consume`/`m` at rc 0 on a re-use (the rejected double-free).
    fn bind_variant_arm(&mut self, kind: &VariantArmKind, h: ValueId) {
        if let VariantArmKind::Ctor { binds, .. } = kind {
            for (slot, var, is_heap) in binds {
                let off = layout::slot_offset(*slot) as i64;
                if *is_heap {
                    // BORROW the slot handle: the subject owns the String; a move-out auto-Dups
                    // in `lower_heap_result_arm`, a consuming re-use Dups in `lower_owned_heap_field`.
                    let p = self.load_at_offset(h, off, crate::PrimKind::LoadHandle);
                    self.param_values.insert(p);
                    self.value_of.insert(*var, p);
                } else {
                    let payload =
                        self.load_at_offset(h, off, crate::PrimKind::Load { width: 8 });
                    self.value_of.insert(*var, payload);
                }
            }
        }
    }

    /// Lower a UNIT-result custom-variant `match` in STATEMENT position (ADT brick 3, the unit
    /// sibling of [`Self::try_lower_custom_variant_match`]) — read the tag@slot0 and run only the
    /// taken arm's EFFECTS. The subject is ALREADY materialized/borrowed by the caller (the
    /// statement-`Match` entry), passed as `subject_value`.
    ///
    /// A custom variant must NEVER fall to the both-arms LINEARIZATION (that runs every arm's
    /// effects = a silent miscompile — e.g. all three `println`s instead of one), so this returns
    /// `Err` (WALL) on an out-of-subset arm rather than declining to the linearizer. Each arm
    /// runs in a per-arm frame (`lower_branch_arm`), wrapped in `IfThen`/`Else`/`EndIf` markers
    /// (no-ops in `verify_ownership`); the last arm / any wildcard is the unconditional else.
    pub(crate) fn lower_custom_variant_unit_match(
        &mut self,
        subject_ty: &Ty,
        subject_value: Option<ValueId>,
        arms: &[IrMatchArm],
    ) -> Result<(), LowerError> {
        use crate::PrimKind;
        let wall = |what: &str| {
            Err(LowerError::Unsupported(format!(
                "custom-variant statement match {what} cannot be faithfully lowered (a both-arms \
                 linearization would run every arm's effects) not in this brick"
            )))
        };
        let Some(subj) = subject_value else {
            return wall("over a non-materialized subject");
        };
        let type_name = match self.custom_variant_type_name(subject_ty) {
            Some(n) => n,
            None => return wall("over an unresolved variant type"),
        };
        let layout = match self.variant_layouts.by_type.get(&type_name) {
            Some(l) => l.clone(),
            None => return wall("over an unregistered variant"),
        };
        let plans = match self.parse_variant_arms(&layout, arms) {
            Some(p) if !p.is_empty() => p,
            _ => return wall("with an arm outside the scalar-field subset"),
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        let tag = self.load_at_offset(h, layout::slot_offset(0) as i64, PrimKind::Load { width: 8 });
        self.emit_variant_unit_chain(h, tag, &plans)
    }

    /// Emit the right-nested `if tag == t0 { arm0 } else if … else { last }` chain for a
    /// UNIT-result custom-variant statement match. Each arm is a per-arm effect frame
    /// (`lower_branch_arm` with no result), the markers carry `val: None`. The last plan / any
    /// wildcard is the unconditional else. `Err` (the whole match walls) if an arm body is out
    /// of subset — the unit sibling of [`Self::emit_variant_arm_chain`].
    fn emit_variant_unit_chain(
        &mut self,
        h: ValueId,
        tag: ValueId,
        plans: &[(VariantArmKind, &IrExpr)],
    ) -> Result<(), LowerError> {
        let Some(((kind, body), rest)) = plans.split_first() else {
            return Ok(());
        };
        if rest.is_empty() || matches!(kind, VariantArmKind::Wildcard) {
            return self.lower_variant_unit_arm(kind, body, h);
        }
        let arm_tag = match kind {
            VariantArmKind::Ctor { tag, .. } => *tag,
            VariantArmKind::Wildcard => unreachable!("handled above"),
        };
        let tc = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: tc, value: arm_tag });
        let cond = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond, op: IntOp::Eq, a: tag, b: tc });
        self.ops.push(Op::IfThen { cond, dst: None });
        self.lower_variant_unit_arm(kind, body, h)?;
        self.ops.push(Op::Else { val: None });
        self.emit_variant_unit_chain(h, tag, rest)?;
        self.ops.push(Op::EndIf { val: None });
        Ok(())
    }

    /// Lower one UNIT-statement custom-variant arm (its effects), with a PER-ARM FRAME that
    /// drops the arm's `Dup`'d heap-field binds at arm end (the unit sibling of
    /// [`Self::lower_variant_arm_value`]). The mark precedes `bind_variant_arm` so a heap field
    /// bound + read by the effect (`println(s)`) is released here. Scalar arms add nothing.
    fn lower_variant_unit_arm(
        &mut self,
        kind: &VariantArmKind,
        body: &IrExpr,
        h: ValueId,
    ) -> Result<(), LowerError> {
        let mark = self.live_heap_handles.len();
        self.bind_variant_arm(kind, h);
        self.lower_branch_arm(None, body)?;
        self.drop_arm_locals(mark);
        Ok(())
    }

    /// Emit the right-nested `if tag == t0 { arm0 } else if … else { last }` chain for a
    /// custom-variant value match, returning the ValueId holding the chain's result. The LAST
    /// plan is the unconditional `else` (no tag test — exhaustiveness guarantees it matches); a
    /// `Wildcard` anywhere is likewise an unconditional `else` (the rest is unreachable). Each
    /// arm body lowers in its own per-arm frame — `lower_scalar_arm` for a scalar result
    /// (ADT brick 3), `lower_heap_result_arm` for a heap result (ADT brick 4, the arm moves out
    /// a fresh heap value). `None` (caller rolls back) if an arm body is outside the subset.
    fn emit_variant_arm_chain(
        &mut self,
        h: ValueId,
        tag: ValueId,
        plans: &[(VariantArmKind, &IrExpr)],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let heap = is_heap_ty(result_ty);
        let ((kind, body), rest) = plans.split_first()?;
        // The last arm, or any Wildcard, is the unconditional else (no tag test).
        if rest.is_empty() || matches!(kind, VariantArmKind::Wildcard) {
            return self.lower_variant_arm_value(kind, body, h, result_ty, heap);
        }
        let arm_tag = match kind {
            VariantArmKind::Ctor { tag, .. } => *tag,
            VariantArmKind::Wildcard => unreachable!("handled above"),
        };
        let dst = self.fresh_value();
        let tc = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: tc, value: arm_tag });
        let cond = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond, op: IntOp::Eq, a: tag, b: tc });
        self.ops.push(Op::IfThen { cond, dst: Some(dst) });
        let then_v = self.lower_variant_arm_value(kind, body, h, result_ty, heap)?;
        self.ops.push(Op::Else { val: Some(then_v) });
        let else_v = self.emit_variant_arm_chain(h, tag, rest, result_ty)?;
        self.ops.push(Op::EndIf { val: Some(else_v) });
        Some(dst)
    }

    /// Lower one custom-variant arm to its value, with a PER-ARM FRAME that drops the arm's
    /// `Dup`'d heap-field binds at arm end. The mark is taken BEFORE `bind_variant_arm` (whose
    /// owned heap binds land in `live_heap_handles`), so `drop_arm_locals` releases exactly the
    /// fields not moved out: a borrow-passed field (`tos(l)`) drops here; a moved-out field
    /// (`Text(s) => s`) was `Dup`+`Consume`'d again by `lower_heap_result_arm`, so its original
    /// bind still drops here (rc-balanced — the transient extra ref is freed). A scalar arm adds
    /// nothing to the frame, so this is a no-op for the brick-2/3 paths.
    fn lower_variant_arm_value(
        &mut self,
        kind: &VariantArmKind,
        body: &IrExpr,
        h: ValueId,
        result_ty: &Ty,
        heap: bool,
    ) -> Option<ValueId> {
        let mark = self.live_heap_handles.len();
        self.bind_variant_arm(kind, h);
        let v = if heap {
            self.lower_heap_result_arm(body, result_ty)
        } else {
            self.lower_scalar_arm(body)
        }?;
        self.drop_arm_locals(mark);
        Some(v)
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
    /// `track_result` governs the HEAP-String result only: `true` (a let-bind / call-arg temp)
    /// pushes the fresh owned String to `live_heap_handles` so it is dropped at scope end; `false`
    /// (a RETURN/tail position) leaves it untracked because it is MOVED OUT to the caller (tracking
    /// it would double-free). The scalar path is unaffected (a scalar result owns nothing).
    pub(crate) fn try_lower_option_unwrap_or(
        &mut self,
        expr: &IrExpr,
        fallback: &IrExpr,
        track_result: bool,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        // The Option operand's handle: either a VAR already bound to a materialized Option
        // (`let o = list.get(xs, i); o ?? d` — the most common form, BORROWED, dropped by its
        // own let-bind at scope end), a function PARAM of Option type (`fn f(o: Option[String]) =
        // o ?? d` — passed by the caller as a real materialized Option block, BORROWED, not dropped
        // here), or a DIRECT self-host Option call (materialized here). The param case is sound by
        // the same evidence as `materialized_options`: an Option-typed param is a real 0-or-1-
        // element block (the calling convention), so its len-as-tag read is correct — NOT a deferred
        // Opaque (those are never params), which is why the bare-Var gate excludes non-Option Vars.
        //
        // A `??` operand is EITHER an Option (`o ?? d` → Some-payload / fallback) OR a scalar Result
        // (`int.parse(s) ?? -1` → Ok-payload / fallback). They share the len-as-tag layout but read
        // INVERSELY: Option Some = `tag != 0` (take payload), Result Ok = `tag == 0` (take payload).
        // `is_result` selects the arm arrangement below; a Result operand also skips the Option-only
        // `option.unwrap_or_str` String branch (a `Result[String,String] ?? "d"` is a later case).
        let is_named_variant_call = matches!(
            &expr.kind,
            IrExprKind::Call { target: CallTarget::Named { .. }, .. }
        ) && is_variant_ty(&expr.ty);
        let is_result = match &expr.kind {
            IrExprKind::Var { id } => self
                .value_for(*id)
                .ok()
                .map(|v| {
                    self.materialized_results.contains(&v)
                        && !self.materialized_options.contains(&v)
                })
                .unwrap_or(false),
            // A USER function returning Result — read its tag INVERSELY (Ok = tag 0).
            _ if is_named_variant_call => is_result_ty(&expr.ty),
            _ => is_self_host_result_call(expr),
        };
        let handle = if let IrExprKind::Var { id } = &expr.kind {
            match self.value_for(*id) {
                // A bare-Var operand must be a tracked materialized Option/Result OR a borrowed
                // variant PARAM (`param_values` — same calling-convention soundness as the match):
                // a deferred Opaque Var (len 0) would MISREAD as None/Err, so it is excluded.
                Ok(v)
                    if self.materialized_options.contains(&v)
                        || self.materialized_results.contains(&v)
                        || self.param_values.contains(&v) =>
                {
                    v
                }
                _ => return None,
            }
        } else if is_self_host_option_call(expr)
            || is_self_host_result_call(expr)
            || is_named_variant_call
        {
            // A self-host OR user-function call returning Option/Result — materialize it (the
            // Named-call arm seeds the READ-shape into `materialized_options/results`, so the
            // tag read below is over a KNOWN-layout block) and read its tag, exactly like a
            // tracked Var. The owned result is dropped at scope end by `materialized_call_arg`.
            match self.lower_call_args(std::slice::from_ref(expr)) {
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
            }
        } else {
            return None;
        };
        // HEAP-String result (`Option[String] ?? "default"` — the most common heap `??`): the scalar
        // unwrap below can't carry a heap payload (it would mis-read the slot-0 String HANDLE as an
        // i64 scalar). Route to the self-host `option.unwrap_or_str` CALL — a call returning a FRESH
        // owned String (cert `i`, bound + dropped like any heap value), which sidesteps the
        // bind-position heap-result-`if` cert problem entirely. The Option is BORROWED (the callee
        // reads + copies it); the fallback is materialized/borrowed by `lower_call_args`. Gated to
        // `Ty::String` (a `List`/other-heap payload would corrupt — its slot is not a String handle),
        // and `count_ir_calls` counts a String-fallback `UnwrapOr` node so this synthetic call keeps
        // `mir_calls <= ir_calls` (the same accounting as the `__str_concat` operator-desugar).
        if matches!(fallback.ty, Ty::String) && !is_result {
            let fb_args = match self.lower_call_args(std::slice::from_ref(fallback)) {
                Ok(a) => a,
                Err(_) => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            };
            let repr = match repr_of(&fallback.ty) {
                Ok(r) => r,
                Err(_) => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            };
            let mut call_args = vec![CallArg::Handle(handle)];
            call_args.extend(fb_args);
            let dst = self.fresh_value();
            self.ops.push(Op::CallFn {
                dst: Some(dst),
                name: "option.unwrap_or_str".to_string(),
                args: call_args,
                result: Some(repr),
            });
            if track_result {
                self.live_heap_handles.push(dst);
            }
            return Some(dst);
        }
        // A SCALAR `??`: read the tag (len @4) and pick the slot-0 payload vs the fallback. The
        // payload is an i64 value COPY (`Load width 8`) — fine for a scalar Ok/Some; a heap payload
        // is handled by the String branch above (Option) or stays out of subset (Result[String,…]).
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![handle] });
        let tag = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });
        let result = self.fresh_value();
        self.ops.push(Op::IfThen { cond: tag, dst: Some(result) });
        // `IfThen` runs the THEN arm when `tag != 0`. For an OPTION that is Some (take the slot-0
        // payload); for a RESULT that is Err (take the FALLBACK — Ok is `tag == 0`, the ELSE arm).
        // So the two arms are SWAPPED between the cases. The ops emitted between IfThen/Else land in
        // the THEN body, those between Else/EndIf in the ELSE body — so the payload Load and the
        // fallback computation must each sit in the arm that USES them.
        if is_result {
            // THEN = Err (tag != 0) → the fallback computed HERE; ELSE = Ok → the slot-0 payload.
            let fb = match self.lower_scalar_value(fallback) {
                Some(v) => v,
                None => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            };
            self.ops.push(Op::Else { val: Some(fb) });
            let payload = self.load_at_offset(h, 12, PrimKind::Load { width: 8 });
            self.ops.push(Op::EndIf { val: Some(payload) });
        } else {
            // THEN = Some (tag != 0) → the slot-0 payload loaded HERE; ELSE = None → the fallback.
            let payload = self.load_at_offset(h, 12, PrimKind::Load { width: 8 });
            self.ops.push(Op::Else { val: Some(payload) });
            let fb = match self.lower_scalar_value(fallback) {
                Some(v) => v,
                None => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            };
            self.ops.push(Op::EndIf { val: Some(fb) });
        }
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
    pub(crate) fn lower_scalar_arm(&mut self, arm: &IrExpr) -> Option<ValueId> {
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
            // A nested VARIANT (Option/Result) match (`err(_) => match float.parse(s) { … }` —
            // the is_numeric_or_bool / looks_numeric chain) EXECUTES via the same tag-read
            // value-match the tail uses; its own arms recurse through `lower_scalar_arm`, so an
            // N-deep nest lowers. A LITERAL-pattern match desugars to the if-chain as before.
            IrExprKind::Match { subject, arms } if is_variant_ty(&subject.ty) => {
                self.try_lower_variant_value_match(subject, arms, &t.ty)
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
            let op = self.drop_op_for(v);
            self.ops.push(op);
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
        // Then try to EXECUTE `for x in xs` over a List[Int] as a real element loop.
        if self.try_lower_scalar_for_list(var, var_tuple, iterable, body) {
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
        let lifted_mark = self.lifted.len();
        let value_of_snapshot = self.value_of.clone();

        self.ops.push(Op::LoopStart);
        let cond_v = match self.lower_scalar_value(cond) {
            Some(v) => v,
            None => {
                self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
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
            self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
            return false;
        }
        // Per-iteration heap (a string literal in a body `println`) is released before the
        // back-edge, INSIDE the loop, so each iteration is balanced.
        self.drop_arm_locals(body_mark);
        self.ops.push(Op::LoopEnd);
        true
    }

    /// Roll back a scalar-loop ATTEMPT (`try_lower_scalar_while` / `_for_range` / `_for_list`),
    /// restoring EVERY side-effect the partial body lowering may have produced — not only `ops`
    /// but the LAMBDA-LIFTED auxiliaries (`self.lifted`). A lambda call-arg in the body (`for x in
    /// xs { … list.map([y], (u) => …) … }`) lifts a `__lambda_*` MirFunction into `self.lifted`;
    /// if the attempt then rolls back (a heap reassignment aborts it → the model-one-iteration
    /// fallback re-lowers the SAME body, re-lifting the lambda), the abandoned first copy would
    /// survive and DOUBLE-COUNT its inner calls (a `mir > ir` wall breach). Truncating `lifted` to
    /// `lifted_mark` (captured at THIS attempt's start, threaded as a local so NESTED loop attempts
    /// each roll back to their own floor) makes the rollback total.
    fn rollback_scalar_loop(
        &mut self,
        ops_mark: usize,
        lhh_mark: usize,
        lifted_mark: usize,
        value_of_snapshot: std::collections::HashMap<almide_ir::VarId, ValueId>,
    ) {
        self.ops.truncate(ops_mark);
        self.live_heap_handles.truncate(lhh_mark);
        self.lifted.truncate(lifted_mark);
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

    /// Materialize the CONDITION of a heap-result `if` to a scalar (Bool = i64 0/1)
    /// BEFORE the `IfThen` marker. The common shape is a pure `lower_scalar_value` cond
    /// (a comparison, a Var, a literal) — tried first, no ownership. When that defers, a
    /// Bool/Int-returning PURE call WITH HEAP ARGS (`if string.contains(s, x) then …`,
    /// `if list.is_empty(xs) then …`) is materialized via `try_lower_scalar_call`: the
    /// call's heap arg temps are pushed to `live_heap_handles`, and a per-cond frame
    /// (`drop_arm_locals`) frees them IMMEDIATELY after the call — they are transient to
    /// the condition, not owned by either arm. The scalar result is not a heap handle, so
    /// it survives the frame teardown. SOUND: the cond eval is internally balanced (each
    /// arg temp alloc'd `i` + dropped `d` within the frame), exactly the per-arm
    /// discipline; outside the pure-scalar-call subset it walls (`None` → Opaque). The
    /// gate keeping a heap-arg call OUT of `lower_scalar_value` (its rollback-safe, no-
    /// ownership contract) does not bind here — this position freely emits ownership ops.
    fn lower_heap_result_cond(&mut self, cond: &IrExpr) -> Option<ValueId> {
        // A scalar cond can itself MATERIALIZE a transient heap temp — `if c == "#"` lowers to
        // `string.eq(c, "#")` whose `"#"` literal is a fresh owned String. That temp is dead the
        // instant the Bool is computed, so it MUST be freed HERE, within a cond-local frame, BEFORE
        // the `IfThen` marker — never deferred to the enclosing arm's `drop_arm_locals`. The cond
        // of a NESTED heap-result `if` (the else-of-an-else parser shape) sits inside one arm's
        // wasm branch; deferring its temp's `Drop` to the OUTER block scope emits an UNCONDITIONAL
        // `rc_dec` of a local that the sibling arm never initialized (garbage/0 → trap). The frame
        // keeps the cond internally `i…d`-balanced exactly where it executes.
        let frame = self.live_heap_handles.len();
        if let Some(v) = self.lower_scalar_value(cond) {
            self.drop_arm_locals(frame);
            return Some(v);
        }
        // A scalar-returning (Bool/Int) PURE call with heap args — materialize it, then
        // free the transient arg temps within a cond-local frame.
        if let IrExprKind::Call { .. } = &cond.kind {
            if !is_heap_ty(&cond.ty) {
                if let Some(v) = self.try_lower_scalar_call(cond, &cond.ty) {
                    self.drop_arm_locals(frame);
                    return Some(v);
                }
            }
        }
        None
    }

    fn lower_heap_result_if_inner(
        &mut self,
        cond: &IrExpr,
        then: &IrExpr,
        else_: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let cond_v = self.lower_heap_result_cond(cond)?;
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
            // An `e!` arm (`if c then parse_sequence(..)! else ..`) — effect-fn error
            // propagation: `e!` returns e's Result unchanged (Ok→Ok, Err→Err), so strip the
            // `!` and lower `e` as the arm (the same identity the tail-position `e!` uses).
            IrExprKind::Unwrap { expr } => self.lower_heap_result_arm(expr, result_ty),
            IrExprKind::LitStr { value } => {
                let repr = repr_of(result_ty).ok()?;
                let obj = self.fresh_value();
                self.ops.push(Op::Alloc { dst: obj, repr, init: Init::Str(value.clone()) });
                self.ops.push(Op::Consume { v: obj });
                Some(obj)
            }
            // A bare-Var arm (`if c then a else b` over heap params/locals — the `pick`
            // shape): the arm must MOVE OUT an owned reference, but `a`/`b` are still
            // owned elsewhere (a borrowed param the caller owns, or a let-local with its
            // own scope-end drop). ACQUIRE a fresh reference (`Op::Dup` = cert `i`-grade:
            // a new owned object, rc+1) and move it out (the arm's `Consume` = `m`) — the
            // SAME per-arm `"im"` balance as a literal arm, and the ORIGINAL handle is
            // untouched (no double-free: the Dup'd ref is independent; the original drops
            // exactly once at its own scope end). Sound for BOTH a param (the proven
            // auto-acquire from the tail-Var path) and a tracked local. `value_for` walls
            // an unbound/global var → the caller keeps the Opaque fallback.
            IrExprKind::Var { id } => {
                let src = self.value_for(*id).ok()?;
                let dst = self.fresh_value();
                self.ops.push(Op::Dup { dst, src });
                self.ops.push(Op::Consume { v: dst });
                Some(dst)
            }
            // A string-concat arm (`match x { _ => a + b }`, `if c then a + b else …`) — the
            // __str_concat call's fresh owned String (cert `i`) + the arm's `Consume` (`m`) = the
            // same per-arm `"im"` balance as the call arms; any materialized arg temp is freed
            // within the arm (`drop_arm_locals`). Closes an un-wired concat position (caps recovery).
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_concat_str(arm)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // A LIST-concat arm (`if string.is_empty(last) then acc else acc + [last]` — the flow_rec
            // base): `__list_concat`/`__list_concat_rc`'s fresh owned list (cert `i`) + the arm's
            // `Consume` (`m`) = the per-arm `"im"` move-out balance. The left operand (`acc`) is BORROWED
            // by the concat (copied), so it is untouched here and freed at its own scope end; any
            // materialized element temp is freed within the arm. Closes the heap-result-`if` return whose
            // arms are an append (the parser-accumulator base case).
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. } => {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_concat_list(arm)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // A STRING INTERPOLATION arm (`match e { Click{button,..} => "click:${button}" }`)
            // over the executable subset — the __str_concat chain's fresh owned String (`i`) +
            // the arm's `Consume` (`m`) = the same per-arm `"im"` balance as the concat arm; any
            // intermediate temp is freed within the arm (`drop_arm_locals`). A compound/call-
            // operand interp returns None → the caller keeps the sound Opaque arm fallback. This
            // is REQUIRED for gate exactness: `count_ir_calls` credits a lowerable interp wherever
            // it sits (the visitor walks match/if arms), so the lowering MUST fold it here too,
            // else `ir_calls > mir_calls` falsely taints the function (the −32 caps regression).
            IrExprKind::StringInterp { parts } => {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_string_interp(parts)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            IrExprKind::If { cond, then, else_ } => {
                self.lower_heap_result_if_inner(cond, then, else_, result_ty)
            }
            // A LIST-literal arm (`if string.is_empty(t) then [] else parse_rows_rec(...)` — the
            // parser entry's empty-or-recurse split): materialize the block + MOVE IT OUT
            // (`Consume` = `m`) — the same per-arm `"im"` as a literal arm. An EMPTY `[]` is a fresh
            // empty list block (no elements to free); a populated heap/scalar list reuses the bind
            // builders (which mark the right recursive-drop set, though the moved-out result is freed
            // by the CALLER per its type, not here).
            IrExprKind::List { elements } => {
                let arm_mark = self.live_heap_handles.len();
                let obj = if elements.is_empty() {
                    let repr = repr_of(result_ty).ok()?;
                    let dst = self.fresh_value();
                    self.ops.push(Op::Alloc { dst, repr, init: Init::IntList(vec![]) });
                    dst
                } else {
                    self.try_lower_str_list_literal(arm)
                        .or_else(|| self.try_lower_scalar_list_construct(arm))?
                };
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // A TUPLE literal arm (`if c then (a, b) else (0, 0)`, `... else (parse(s), pos)`):
            // materialize the flat (scalar) or nested-ownership (heap-element) tuple block
            // (cert `i`) and MOVE IT OUT (`Consume` = `m`) — the same per-arm `"im"` balance as
            // a literal arm. Any heap element it materializes is freed within the arm.
            IrExprKind::Tuple { elements } => {
                let arm_mark = self.live_heap_handles.len();
                let obj = self
                    .try_lower_scalar_tuple_construct(elements)
                    .or_else(|| self.try_lower_tuple_construct(elements))?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // A BLOCK arm (`else { let c = string.get(s, pos) ?? ""; <heap-tail> }` — the
            // dominant real-parser shape): lower its statements as effects in a per-arm frame,
            // then its tail as the arm's moved-out heap value (recursing into this same arm
            // lowering, which `Consume`s the tail). The block's own heap let-locals (tracked in
            // `live_heap_handles` since `arm_mark`) are freed WITHIN the arm via
            // `drop_arm_locals`; the moved-out value is `Consume`d (never in that set), so it is
            // not double-freed. Same per-arm balance the scalar block arm proves.
            IrExprKind::Block { stmts, expr } => {
                let tail = expr.as_deref()?;
                let arm_mark = self.live_heap_handles.len();
                self.in_frame += 1;
                let mut ok = true;
                for stmt in stmts {
                    if self.lower_stmt(stmt).is_err() {
                        ok = false;
                        break;
                    }
                }
                let obj = if ok {
                    self.lower_heap_result_arm(tail, result_ty)
                } else {
                    None
                };
                self.drop_arm_locals(arm_mark);
                self.in_frame -= 1;
                obj
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
                // A DIRECT SELF-RECURSIVE call arm (`name == fn_name`) is the unbounded-
                // stack tail-recursion shape (`fn spin = if … then acc else spin(…)`).
                // v1 has NO TCO, so EXECUTING it deeply overflows the wasm call stack
                // (a fail-stop trap). Executing the heap-result if here would convert a
                // shallow-correct / deep-trapping recursion — a NET LOSS over the sound
                // Opaque fallback for the canonical 2M-deep TCO acceptance fixture. WALL
                // it (→ `None`): the function keeps its memory-safe linearized form until
                // real TCO lands. (A non-self call recurses no deeper than the caller, so
                // it stays admitted.)
                if name.as_str() == self.fn_name {
                    return None;
                }
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
            // A PURE stdlib `Module`-call arm (`match n { 0 => "a", _ => int.to_string(n) }` —
            // the single most common real-program shape). Same per-arm `"im"` balance as the
            // Named-call arm: the pure call returns a FRESH owned heap value (`i`), the arm's
            // `Consume` moves it out (`m`); any heap arg it materializes is freed within the arm
            // (`drop_arm_locals`). The purity gate lives in `lower_pure_module_value_call` (an
            // impure/HO/unsupported call errors → `None` → the caller keeps the sound Opaque
            // fallback). Was the gap that dropped a real-program `match → stdlib-call` to Opaque.
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                let arm_mark = self.live_heap_handles.len();
                let obj = self
                    .lower_pure_module_value_call(module.as_str(), func.as_str(), args, result_ty)
                    .ok()?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // A direct Option ctor arm (`if c then Some(x*2) else None` — the filter_map / map
            // closure body): materialize the 0-or-1-element Option block + Consume (move-out)
            // — the SAME per-arm `"im"` balance as a literal arm (init-agnostic `Alloc` = `i`,
            // `Consume` = `m`). `Some`'s payload must be a lowerable scalar (a heap payload
            // aliases its element — a later brick; it falls out of the subset here).
            // A HEAP payload (`Some(string_var)` — an `Option[String]`) materializes a 0-or-1-
            // element `DynListStr` (Machinery 2): the owned String is MOVED into slot 0 (cert `m`)
            // and the whole Option is freed recursively (`DropListStr`) at scope end. Same `Alloc`
            // = `i` + `Consume` = `m` per-arm balance as the scalar case; reuses the proven
            // List[String] cert (init-agnostic). Only a Var payload (the owned slice, let-bound).
            IrExprKind::OptionSome { expr } if is_heap_ty(&expr.ty) => {
                let repr = repr_of(result_ty).ok()?;
                // The owned String payload: a let-bound Var (its handle), or a direct user-call
                // that RETURNS a fresh owned String (CallFn result, rc 1) — materialized into the
                // Option below (its `Consume` `m` balances the alloc/call `i`).
                let piece = match &expr.kind {
                    // `some(v)` over a Var STILL OWNED elsewhere (a borrowed param, or a local with
                    // its own scope-end drop): `Op::Dup` a fresh owned reference (cert `a`) to MOVE
                    // into the Option, leaving the original to drop once at its scope — never a bare
                    // move-out `m` the checker rejects (param → `am`, owned local → `iamd`).
                    IrExprKind::Var { id } => {
                        let src = self.value_for(*id).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::Dup { dst: p, src });
                        p
                    }
                    IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                        let lowered = self.lower_call_args(args).ok()?;
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::CallFn {
                            dst: Some(p),
                            name: name.as_str().to_string(),
                            args: lowered,
                            result: Some(pr),
                        });
                        p
                    }
                    _ => return None,
                };
                let obj = self.materialize_opt_str_some(piece, repr);
                self.ops.push(Op::Consume { v: obj });
                Some(obj)
            }
            IrExprKind::OptionSome { expr } => {
                let payload = self.lower_scalar_value(expr)?;
                let repr = repr_of(result_ty).ok()?;
                let obj = self.fresh_value();
                self.ops.push(Op::Alloc { dst: obj, repr, init: Init::OptSome { payload } });
                self.ops.push(Op::Consume { v: obj });
                Some(obj)
            }
            // A `None` for an `Option[heap]` is the 0-element `DynListStr` (so `DropListStr` frees
            // it uniformly); a scalar Option keeps `Init::OptNone`.
            IrExprKind::OptionNone if is_heap_elem_list_ty(result_ty) => {
                let repr = repr_of(result_ty).ok()?;
                let obj = self.materialize_opt_str_none(repr);
                self.ops.push(Op::Consume { v: obj });
                Some(obj)
            }
            IrExprKind::OptionNone => {
                let repr = repr_of(result_ty).ok()?;
                let obj = self.fresh_value();
                self.ops.push(Op::Alloc { dst: obj, repr, init: Init::OptNone });
                self.ops.push(Op::Consume { v: obj });
                Some(obj)
            }
            // `Ok(int)` / `Err(string)` arms of a `Result[Int, String]`-returning heap `if` (the
            // parse-family shape `if ok then Ok(v) else Err("msg")`). Result reuses the Option[String]
            // DynListStr layout with len-AS-TAG: `Ok` = a cap-1/len-0 block (the int sits in slot 0
            // but DropListStr frees no element — like `None`); `Err` = a cap-1/len-1 block owning the
            // message String (DropListStr frees it — exactly `Some(string)`). So BOTH arms reuse the
            // proven Option[String] cert (Alloc `i` + the per-arm `Consume` `m`; the Err's String is
            // moved in `m` and freed by the scope-end DropListStr `d`) — NO new Init, NO checker change.
            // HEAP-Ok `Result[String, String]`: BOTH `Ok(string)` and `Err(string)` own a String, so
            // len-as-tag can't distinguish — materialize a len-1 DynListStr + the Ok/Err tag in cap@8.
            IrExprKind::ResultOk { expr }
                if is_heap_ty(&expr.ty) && Self::is_heap_ok_result(result_ty) =>
            {
                let repr = repr_of(result_ty).ok()?;
                let piece = self.lower_result_str_piece(expr)?;
                let obj = self.materialize_result_str(piece, repr, false);
                self.ops.push(Op::Consume { v: obj });
                Some(obj)
            }
            IrExprKind::ResultErr { expr }
                if is_heap_ty(&expr.ty) && Self::is_heap_ok_result(result_ty) =>
            {
                let repr = repr_of(result_ty).ok()?;
                let piece = self.lower_result_str_piece(expr)?;
                let obj = self.materialize_result_str(piece, repr, true);
                self.ops.push(Op::Consume { v: obj });
                Some(obj)
            }
            IrExprKind::ResultOk { expr } if !is_heap_ty(&expr.ty) => {
                let payload = self.lower_scalar_value(expr)?;
                let repr = repr_of(result_ty).ok()?;
                let obj = self.materialize_result_ok(payload, repr);
                self.ops.push(Op::Consume { v: obj });
                Some(obj)
            }
            IrExprKind::ResultErr { expr } if is_heap_ty(&expr.ty) => {
                let repr = repr_of(result_ty).ok()?;
                let piece = match &expr.kind {
                    IrExprKind::Var { id } => self.value_for(*id).ok()?,
                    IrExprKind::LitStr { value } => {
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::Alloc { dst: p, repr: pr, init: Init::Str(value.clone()) });
                        p
                    }
                    IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                        let lowered = self.lower_call_args(args).ok()?;
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::CallFn {
                            dst: Some(p),
                            name: name.as_str().to_string(),
                            args: lowered,
                            result: Some(pr),
                        });
                        p
                    }
                    _ => return None,
                };
                // `Err` IS `Some(message)` physically (cap-1/len-1 DynListStr owning the String).
                let obj = self.materialize_opt_str_some(piece, repr);
                self.ops.push(Op::Consume { v: obj });
                Some(obj)
            }
            // A NESTED `match` arm (`match int.parse(c) { ok(n) => value.int(n), err(_) =>
            // match float.parse(c) { … } }` — try_decimal; `if … then match int.from_hex(..) {
            // ok(n) => value.int(n), err(_) => value.str(raw) } else …` — parse_number's then-arm).
            // Recurse through the SAME machinery the tail-position `match` uses: a variant subject
            // runs the proven `try_lower_variant_value_match` (subject-drop-before-arms over a
            // scalar payload, then a heap-result-`if` skeleton), an Int-literal subject desugars
            // to a nested heap-result `if`. The recursive call ALREADY `Consume`s each leaf arm
            // (the move-out balance) and returns the merged if-result `dst` — so this arm adds NO
            // extra `Consume` (exactly like the nested-`If` arm above), avoiding a double-move-out.
            // Cert-clean: it composes two already-proven, internally-balanced lowerings; on any
            // out-of-subset shape the inner attempt rolls itself back and returns `None`, so the
            // OUTER `try_lower_heap_result_if` restores the op stream and walls the function.
            IrExprKind::Match { subject, arms } => {
                if is_variant_ty(&subject.ty) {
                    if let Some(dst) =
                        self.try_lower_variant_value_match(subject, arms, result_ty)
                    {
                        return Some(dst);
                    }
                }
                if let Some(if_expr) = self.desugar_match_to_if(subject, arms, result_ty) {
                    if let IrExprKind::If { cond, then, else_ } = &if_expr.kind {
                        return self.lower_heap_result_if_inner(cond, then, else_, result_ty);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// `Some(piece)` for `Option[String]` = a 1-element `DynListStr`: store `piece`'s handle into
    /// slot 0 + CONSUME it (moves in), track as nested-ownership list + materialized Option.
    /// Reuses the proven Machinery-2 `store_str` op sequence — no new cert.
    pub(crate) fn materialize_opt_str_some(&mut self, piece: ValueId, repr: crate::Repr) -> ValueId {
        use crate::PrimKind;
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc { dst: obj, repr, init: Init::DynListStr { len: one } });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![obj] });
        let twelve = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: twelve, value: 12 });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: oh, b: twelve });
        let ph = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(ph), args: vec![piece] });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addr, ph] });
        self.ops.push(Op::Consume { v: piece });
        self.live_heap_handles.retain(|h| *h != piece);
        self.heap_elem_lists.insert(obj);
        self.materialized_options.insert(obj);
        obj
    }

    /// Materialize `None` for an `Option[String]` as a 0-element `DynListStr` (tracked like
    /// `materialize_opt_str_some`). `DropListStr` over len 0 frees only the block.
    pub(crate) fn materialize_opt_str_none(&mut self, repr: crate::Repr) -> ValueId {
        let zero = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero, value: 0 });
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc { dst: obj, repr, init: Init::DynListStr { len: zero } });
        self.heap_elem_lists.insert(obj);
        self.materialized_options.insert(obj);
        obj
    }

    /// `Ok(string)` / `Err(string)` for a HEAP-Ok `Result[String, String]` = a len-1 `DynListStr`
    /// owning the one String at slot 0 (Ok's value OR Err's message), with the Ok/Err TAG written to
    /// the `cap` field (@8): 0=Ok, 1=Err. `len` stays 1 so `DropListStr` frees the String regardless
    /// of which arm. Cert = `materialize_opt_str_some` (Alloc `i` + the String `m` + scope-end `d`);
    /// the cap-tag store is an opaque prim op. Tracked in `materialized_results_str` for the match.
    /// Is `ty` a `Result[heap, heap]` (e.g. `Result[String, String]`)? Both Ok and Err own a heap
    /// payload, so it uses the cap-as-tag heap-Ok materialization, NOT the scalar len-as-tag one.
    pub(crate) fn is_heap_ok_result(ty: &Ty) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
            if a.len() == 2 && is_heap_ty(&a[0]) && is_heap_ty(&a[1]))
    }

    /// Lower a heap-String piece (an `Ok`/`Err` payload) to its owned handle: a tracked Var, a
    /// String literal (fresh Alloc), or a Named-call result. Returns `None` for any other shape.
    pub(crate) fn lower_result_str_piece(&mut self, expr: &IrExpr) -> Option<ValueId> {
        match &expr.kind {
            // `ok(f)` / `err(msg)` over a Var that is STILL OWNED elsewhere — a borrowed param
            // (`fn validate(f) = .. ok(f)`) or a let-local with its own scope-end drop. The piece
            // is MOVED INTO the Result block (`materialize_result_str` `Consume`s it), so it must be
            // a FRESH owned reference: `Op::Dup` (acquire, cert `a`) the var, leaving the original
            // untouched (it drops exactly once at its own scope — no double-free, no bare move-out
            // `m` underflow the proven checker rejects). Same `a…m` balance as the bare-Var if-arm.
            IrExprKind::Var { id } => {
                let src = self.value_for(*id).ok()?;
                let dst = self.fresh_value();
                self.ops.push(Op::Dup { dst, src });
                Some(dst)
            }
            IrExprKind::LitStr { value } => {
                let pr = repr_of(&expr.ty).ok()?;
                let p = self.fresh_value();
                self.ops.push(Op::Alloc { dst: p, repr: pr, init: Init::Str(value.clone()) });
                Some(p)
            }
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
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
            _ => None,
        }
    }

    pub(crate) fn materialize_result_str(
        &mut self,
        piece: ValueId,
        repr: crate::Repr,
        is_err: bool,
    ) -> ValueId {
        use crate::PrimKind;
        // A 1-SLOT DynListStr (cap 1, len 1 — IDENTICAL block size to every other String/Value block,
        // so the single-head free-list reuses it; a wider block would be a distinct size that the
        // size-exact reuse leaks). Slot 0's LOW 32 bits (@12) own the String handle, its HIGH 32 bits
        // (@16) carry the Ok/Err tag — `DropListStr` does `i32.wrap` of the i64 slot, taking ONLY the
        // low-32 handle to free, so the high-32 tag is inert (never mistaken for a handle).
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc { dst: obj, repr, init: Init::DynListStr { len: one } });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![obj] });
        // slot 0 LOW (@12) := the String handle (zero-extended i64 → high 32 bits cleared), CONSUME
        // the piece (move-in). This 8-byte store MUST precede the tag store (it zeroes @16).
        let off12 = self.const_add(oh, 12);
        let ph = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(ph), args: vec![piece] });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![off12, ph] });
        self.ops.push(Op::Consume { v: piece });
        self.live_heap_handles.retain(|h| *h != piece);
        // slot 0 HIGH (@16) := the Ok/Err tag (0 = Ok, 1 = Err) — overwrites the cleared high half.
        let off16 = self.const_add(oh, 16);
        let tag = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: tag, value: if is_err { 1 } else { 0 } });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![off16, tag] });
        self.heap_elem_lists.insert(obj);
        self.materialized_results_str.insert(obj);
        obj
    }

    /// `handle + k` as a fresh i64 address value (ConstInt + IntBinOp::Add).
    fn const_add(&mut self, base: ValueId, k: i64) -> ValueId {
        let c = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: c, value: k });
        let dst = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst, op: IntOp::Add, a: base, b: c });
        dst
    }

    /// `Ok(int)` for `Result[Int, String]` = a cap-1/len-0 `DynListStr`: allocate ONE element slot
    /// (so the block is the same physical size as an `Err`'s, free-list-compatible via cap), store
    /// the int in slot 0, then OVERRIDE the len field to 0 so `DropListStr` frees no element (the
    /// int is scalar, owns nothing). Cert: a `None`-like DynListStr (Alloc `i`, no String move-in,
    /// scope-end DropListStr `d`) — the int store + len override are opaque prim ops the checker
    /// ignores. The tag read (len == 0) marks it `Ok`.
    pub(crate) fn materialize_result_ok(&mut self, payload: ValueId, repr: crate::Repr) -> ValueId {
        use crate::PrimKind;
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc { dst: obj, repr, init: Init::DynListStr { len: one } });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![obj] });
        // slot 0 (handle + 12) = the Ok int.
        let twelve = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: twelve, value: 12 });
        let daddr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: daddr, op: IntOp::Add, a: oh, b: twelve });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![daddr, payload] });
        // len field (handle + 4) := 0 so DropListStr treats it as element-free (the Ok tag).
        let four = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: four, value: 4 });
        let laddr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: laddr, op: IntOp::Add, a: oh, b: four });
        let zero = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero, value: 0 });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![laddr, zero] });
        self.heap_elem_lists.insert(obj);
        obj
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
            || matches!(find_var_ty(body, var), Some(t) if !matches!(t, Ty::Int))
            || !matches!(start.kind, IrExprKind::LitInt { .. })
        {
            return false;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let lifted_mark = self.lifted.len();
        let value_of_snapshot = self.value_of.clone();

        // Snapshot `end` once; init the index local `i = start` (a fresh ConstInt — a
        // distinct, mutable local, never aliasing a caller value). `one` for the step.
        let end_v = match self.lower_scalar_value(end) {
            Some(v) => v,
            None => {
                self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
                return false;
            }
        };
        if self.lower_bind(var, &Ty::Int, start).is_err() {
            self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
            return false;
        }
        let Some(&i_v) = self.value_of.get(&var) else {
            self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
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
            self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
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

    /// EXECUTE `for x in xs { … }` over a `List[T]` as a real loop (vs the model-one-iteration
    /// form): borrow the list handle once, walk an internal index `i` 0..len via the loop markers,
    /// bind element `i` to the loop var `x` each iteration, run the body.
    ///
    /// TWO element shapes, BOTH borrowing the list (read-only; the list keeps owning its elements):
    /// - a SCALAR element (`List[Int/Float/Bool]`, i64 slots) — `Load { width: 8 }` the slot and
    ///   `SetLocal` the loop var (a stable mutable i64 local, a COPY, no ownership);
    /// - a HEAP element (`List[String]` / nested-ownership DynListStr, i32-handle slots) — the loop
    ///   var is the BORROWED element handle, `LoadHandle`d fresh each iteration into `value_of[var]`
    ///   and recorded in `param_values` so it is NOT a second owner (the list's recursive drop frees
    ///   the element; the loop var must not free it — no double-free). The body reads the element via
    ///   string/list ops; a body that MOVES the element out (stores it elsewhere) is not in this
    ///   subset (the borrow stays read-only), so such a body rolls back.
    ///
    /// SOUND by reuse of the for-range / while machinery: the body is per-iteration-balanced
    /// (`drop_arm_locals`), the markers no-op in the cert (it verifies ONE balanced iteration), the
    /// `i < len` guard runs the body the REAL number of times (0 for an empty list — closing the
    /// model-one-iteration bug that ran a heap-element body ONCE on a garbage handle). GATED to a
    /// `List[scalar]` / heap-element list, a matching loop-var type, no tuple/break/continue.
    pub(crate) fn try_lower_scalar_for_list(
        &mut self,
        var: VarId,
        var_tuple: &Option<Vec<VarId>>,
        iterable: &IrExpr,
        body: &[IrStmt],
    ) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        use crate::PrimKind;
        // The element type: a scalar `List[Int/Float/Bool]` (i64 slot) OR a heap-element list
        // (`List[String]`, i32-handle slot). A Map / non-list iterable defers.
        let elem_ty = match &iterable.ty {
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => a[0].clone(),
            _ => return false,
        };
        let elem_heap = is_heap_ty(&elem_ty);
        // The element SHAPE (scalar vs heap) comes from the iterable's element type, so the loop var
        // is bound correctly even when it is UNUSED in the body (an `for _ in xs`, or a loop kept for
        // its effect count) — `find_var_ty` returns None then, which must NOT fall to the model-one-
        // iteration form (that ran the body ONCE; an empty list must run it ZERO times). When the var
        // IS used, its body-declared type must agree with the element shape (a defensive consistency
        // gate against a mis-typed body).
        let var_ty = find_var_ty(body, var);
        if let Some(vt) = &var_ty {
            if is_heap_ty(vt) != elem_heap {
                return false;
            }
        }
        if var_tuple.is_some() || body_breaks_or_continues(body) {
            return false;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let lifted_mark = self.lifted.len();
        let value_of_snapshot = self.value_of.clone();

        // Borrow the list (evaluated once); a Var is borrowed, a fresh literal is materialized
        // (owned, dropped at the outer scope — it stays in live_heap_handles). A heap-element
        // list LITERAL (`for s in ["x", "y"]`) needs its elements actually stored, so route it
        // through `try_lower_str_list_literal` (the filled owned list) rather than the generic
        // `lower_call_args` Alloc path (which would leave an empty/opaque block → zero iterations).
        let str_list_literal =
            elem_heap && matches!(&iterable.kind, IrExprKind::List { elements } if !elements.is_empty());
        let list_v = if str_list_literal {
            match self.try_lower_str_list_literal(iterable) {
                Some(v) => {
                    self.live_heap_handles.push(v);
                    v
                }
                None => {
                    self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
                    return false;
                }
            }
        } else {
            match self.lower_call_args(std::slice::from_ref(iterable)) {
                Ok(args) => match args.into_iter().next() {
                    Some(CallArg::Handle(v)) => v,
                    _ => {
                        self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
                        return false;
                    }
                },
                Err(_) => {
                    self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
                    return false;
                }
            }
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![list_v] });
        let len_v = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });
        let i_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: i_v, value: 0 });
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });
        // The SCALAR loop var is a stable mutable i64 local, `SetLocal` to element[i] each iteration.
        // (A HEAP loop var is bound fresh per iteration below — no stable local: a borrowed i32
        // handle re-`LoadHandle`d inside the loop.)
        let x_v = if elem_heap {
            None
        } else {
            let x = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: x, value: 0 });
            self.value_of.insert(var, x);
            Some(x)
        };

        self.ops.push(Op::LoopStart);
        let cond_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond_v, op: IntOp::Lt, a: i_v, b: len_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });
        // The element-slot address `h + 12 + i*8`.
        let i8_v = self.fresh_value();
        let eight = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: eight, value: 8 });
        self.ops.push(Op::IntBinOp { dst: i8_v, op: IntOp::Mul, a: i_v, b: eight });
        let base = self.load_addr(h, 12);
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: base, b: i8_v });
        if let Some(x_v) = x_v {
            // Scalar element: x = load64(slot) — a COPY into the stable mutable local.
            let elem = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(elem), args: vec![addr] });
            self.ops.push(Op::SetLocal { local: x_v, src: elem });
        } else {
            // Heap element: x = the BORROWED i32 handle at the slot (LoadHandle, Ptr repr), bound
            // fresh each iteration. Recorded in `param_values` — the list still OWNS the element
            // (its recursive DropListStr frees it), so the loop var is NOT a second owner and is
            // NOT added to the per-iteration drop set (no double-free).
            let elem = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(elem), args: vec![addr] });
            self.value_of.insert(var, elem);
            self.param_values.insert(elem);
        }

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
            self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
            return false;
        }
        self.drop_arm_locals(body_mark);
        let next_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: next_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::SetLocal { local: i_v, src: next_v });
        self.ops.push(Op::LoopEnd);
        true
    }

    /// C1 DEFUNCTIONALIZATION — inline a `list.map`/`filter`/`fold` with an INLINE-LAMBDA
    /// closure argument as a SPECIALIZED loop at the call site: NO runtime closure, NO
    /// `Op::CallIndirect`, NO lifted `__lambda_*` function. The lambda body is lowered
    /// INLINE per element with its PARAM bound to the element (`let x = elem`) and its
    /// CAPTURES resolved through the EXISTING `value_of` map (an inline / let-bound lambda's
    /// free vars are already in scope at the call site — no env block, no substitution). So
    /// a CAPTURING lambda (`let k = 10; list.map(xs, (x) => x * k)`) WORKS: `k` is just a
    /// `Var` the inlined `x * k` reads through `value_of`, exactly as if hand-written as a
    /// `for x in xs` loop.
    ///
    /// SOUNDNESS by REUSE — the same machinery the for-in/for-list loops already prove
    /// sound (task #67): a real `LoopStart`/`LoopBreakUnless`/`LoopEnd` over a stable i64
    /// index local; the result list is a `DynList`/`DynStr`-grade fresh OWNED block built
    /// exactly like a scalar list LITERAL (`try_lower_scalar_list_slots`); the per-element
    /// body lowers via `lower_scalar_value` (pure, no ownership event), so NO heap temp
    /// crosses the back-edge. The inlined body's calls are REAL IR call nodes that
    /// `count_ir_calls` already counts in-place (the lambda body sits in the IR call-arg the
    /// gate's visitor walks), and the caps fold sees them directly — there is NO
    /// `CallIndirect` conservatism and NO elided marker, so a function stays caps-verified
    /// iff its inlined bodies are pure. A body the scalar subset cannot lower (a `println`
    /// side effect, a heap result) → `None` (rolled back), and the caller keeps the existing
    /// self-host-combinator / WALL path. NARROW to a SCALAR-element source list and a SCALAR
    /// lambda result/element (the dual-oracle subset): a heap element/result needs the
    /// nested-ownership build this slice does not emit, so it WALLS (defers) cleanly.
    ///
    /// Returns the result value (`map`/`filter`: a fresh OWNED scalar `List`; `fold`: a
    /// scalar accumulator carrying no ownership), or `None` (fully rolled back) when out of
    /// subset. The caller (`lower_pure_module_value_call`) treats the `Some` result exactly
    /// like a self-host combinator's: a fresh owned heap list is bound + dropped, a scalar
    /// fold result is bound.
    pub(crate) fn try_lower_defunc_list_hof(
        &mut self,
        func: &str,
        args: &[IrExpr],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use almide_lang::types::constructor::TypeConstructorId;
        // The closure arg index per combinator: map/filter = arg 1, fold = arg 2 (after init).
        let (xs, lambda_idx, init_idx) = match func {
            "map" | "filter" if args.len() == 2 => (&args[0], 1usize, None),
            "fold" if args.len() == 3 => (&args[0], 2usize, Some(1usize)),
            _ => return None,
        };
        // The CLOSURE arg MUST be an INLINE lambda (`(x) => …`). A first-class Var/FnRef
        // closure is C2 (not inlinable here) — defer to the self-host path / WALL.
        let (params, body) = match &args[lambda_idx].kind {
            IrExprKind::Lambda { params, body, .. } => (params, body.as_ref()),
            _ => return None,
        };
        // SCALAR-element source list only (`List[Int/Float/Bool]`, i64 slots). A heap element
        // (an i32 handle) would need the nested-ownership build this slice does not emit.
        let src_scalar = matches!(&xs.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && !is_heap_ty(&a[0]));
        if !src_scalar {
            return None;
        }
        // map/filter: a SCALAR-element result list (`List[Int/Float/Bool]`, i64 slots — the
        // block this slice builds). fold: a SCALAR accumulator. A heap result element or a
        // heap accumulator needs the nested-ownership build this slice does not emit → defer.
        let result_ok = match func {
            "map" | "filter" => matches!(result_ty,
                Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && !is_heap_ty(&a[0])),
            "fold" => !is_heap_ty(result_ty),
            _ => false,
        };
        if !result_ok {
            return None;
        }
        // map/filter have exactly ONE param (the element); fold has TWO (acc, element).
        let expected_params = if func == "fold" { 2 } else { 1 };
        if params.len() != expected_params {
            return None;
        }

        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let lifted_mark = self.lifted.len();
        let value_of_snapshot = self.value_of.clone();

        let result = self.lower_defunc_list_hof_inner(func, xs, params, body, init_idx.map(|i| &args[i]));
        if result.is_none() {
            self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
        } else {
            // The closure was FAITHFULLY inlined (the body executes per element through real
            // ops) — there is NO unlifted/missing closure slot. Clear the flag so the bind
            // path treats the result as a genuinely-materialized list (`materialized_lists`),
            // NOT as an unfaithful HOF to WALL. (My result IS a real, populated block.)
            self.last_call_had_unlifted_closure = false;
        }
        result
    }

    fn lower_defunc_list_hof_inner(
        &mut self,
        func: &str,
        xs: &IrExpr,
        params: &[(VarId, Ty)],
        body: &IrExpr,
        init: Option<&IrExpr>,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        // Borrow the source list (evaluated once). A Var is borrowed; a fresh literal is
        // materialized into an owned temp dropped at the OUTER scope (it stays in
        // live_heap_handles). A non-handle iterable (a Range / scalar) is out of subset.
        let list_v = match self.lower_call_args(std::slice::from_ref(xs)).ok()?.into_iter().next()? {
            CallArg::Handle(v) => v,
            _ => return None,
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![list_v] });
        let len_v = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });

        // The FOLD accumulator: a stable mutable scalar local seeded from `init`. map/filter
        // build a result list block of `len` slots instead.
        let (acc_local, result_list, result_h, cursor) = match func {
            "fold" => {
                let init_v = self.lower_scalar_value(init?)?;
                // A STABLE mutable local: ConstInt-seed then SetLocal to the init value (so the
                // local is distinct and reassignable across iterations, the proven loop-state model).
                let acc = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: acc, value: 0 });
                self.ops.push(Op::SetLocal { local: acc, src: init_v });
                (Some(acc), None, None, None)
            }
            "map" | "filter" => {
                // A fresh OWNED `DynList` of `len` slots (map: len = len(xs); filter: len(xs) is
                // the MAX, the real length is patched to the write-cursor after the loop). Built
                // exactly like a scalar list literal — a flat block, scope-end `Drop`.
                let dst = self.fresh_value();
                self.ops.push(Op::Alloc {
                    dst,
                    repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                    init: crate::Init::DynList { len: len_v },
                });
                let rh = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(rh), args: vec![dst] });
                // filter needs a write-cursor (the count of kept elements) — a stable local.
                let cur = if func == "filter" {
                    let c = self.fresh_value();
                    self.ops.push(Op::ConstInt { dst: c, value: 0 });
                    Some(c)
                } else {
                    None
                };
                (None, Some(dst), Some(rh), cur)
            }
            _ => return None,
        };

        // The loop index (stable mutable i64 local) and the +1 step constant.
        let i_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: i_v, value: 0 });
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });

        self.ops.push(Op::LoopStart);
        let cond_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond_v, op: IntOp::Lt, a: i_v, b: len_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

        // Load element[i] from the SOURCE list: addr = src_h + 12 + i*8, then load64.
        let i8_v = self.fresh_value();
        let eight = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: eight, value: 8 });
        self.ops.push(Op::IntBinOp { dst: i8_v, op: IntOp::Mul, a: i_v, b: eight });
        let src_base = self.load_addr(h, 12);
        let src_addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: src_addr, op: IntOp::Add, a: src_base, b: i8_v });
        let elem = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(elem), args: vec![src_addr] });

        // Bind the lambda PARAM(s). map/filter: the single element param = elem. fold: acc
        // (the stable local) + element param = elem. The CAPTURES need no binding — their
        // VarIds already resolve through `value_of`.
        let elem_param = if func == "fold" { params[1].0 } else { params[0].0 };
        self.value_of.insert(elem_param, elem);
        if func == "fold" {
            self.value_of.insert(params[0].0, acc_local.unwrap());
        }

        // Lower the lambda BODY inline as a per-iteration scalar frame. A side-effecting /
        // heap-result body (the false-green `{ println("hit"); x }`) is NOT scalar-lowerable
        // → None → the whole HOF rolls back and the caller WALLS (caps stays honest).
        let body_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        self.scalar_loop_depth += 1;
        let body_v = self.lower_scalar_value(body);
        self.scalar_loop_depth -= 1;
        self.in_frame -= 1;
        let body_v = match body_v {
            Some(v) => v,
            None => return None,
        };
        self.drop_arm_locals(body_mark);

        match func {
            "map" => {
                // result[i] = body_v.
                let rh = result_h.unwrap();
                let rbase = self.load_addr(rh, 12);
                let raddr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: raddr, op: IntOp::Add, a: rbase, b: i8_v });
                self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![raddr, body_v] });
            }
            "filter" => {
                // if body_v (Bool) then { result[cursor] = elem; cursor += 1 }.
                let rh = result_h.unwrap();
                let cur = cursor.unwrap();
                self.ops.push(Op::IfThen { cond: body_v, dst: None });
                // then-arm: store elem at result[cursor*8], bump cursor.
                let c8 = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: c8, op: IntOp::Mul, a: cur, b: eight });
                let rbase = self.load_addr(rh, 12);
                let raddr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: raddr, op: IntOp::Add, a: rbase, b: c8 });
                self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![raddr, elem] });
                let cnext = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: cnext, op: IntOp::Add, a: cur, b: one_v });
                self.ops.push(Op::SetLocal { local: cur, src: cnext });
                self.ops.push(Op::Else { val: None });
                self.ops.push(Op::EndIf { val: None });
            }
            "fold" => {
                // acc = body_v.
                self.ops.push(Op::SetLocal { local: acc_local.unwrap(), src: body_v });
            }
            _ => return None,
        }

        // Advance the index and close the loop.
        let next_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: next_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::SetLocal { local: i_v, src: next_v });
        self.ops.push(Op::LoopEnd);

        match func {
            "fold" => Some(acc_local.unwrap()),
            "map" => Some(result_list.unwrap()),
            "filter" => {
                // Patch the result list's `len` field (offset 4) to the write-cursor: the
                // visible length is the count of kept elements (cap stays len(xs), unused
                // tail slots are harmless — a `${list}` Display reads `len`, an `xs[i]`
                // bounds-checks against `len`). `store32` at result_h + 4.
                let rh = result_h.unwrap();
                let cur = cursor.unwrap();
                let four = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: four, value: 4 });
                let lenaddr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: lenaddr, op: IntOp::Add, a: rh, b: four });
                self.ops.push(Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![lenaddr, cur] });
                Some(result_list.unwrap())
            }
            _ => None,
        }
    }

    /// `base + offset` as a fresh value (the address-arithmetic half of `load_at_offset`,
    /// without the load — used when the loaded address feeds further arithmetic).
    fn load_addr(&mut self, base: ValueId, offset: i64) -> ValueId {
        let off = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: off, value: offset });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: base, b: off });
        addr
    }

    pub(crate) fn lower_while(&mut self, cond: &IrExpr, body: &[IrStmt]) -> Result<(), LowerError> {
        // First try to EXECUTE it as a real scalar-state loop; on any out-of-subset
        // feature this rolls back cleanly and we reach the model-one-iteration form below.
        if self.try_lower_scalar_while(cond, body) {
            return Ok(());
        }
        // The fallback below runs the body straight-line ONCE (the model-one-iteration
        // form). A `break`/`continue` (no early-exit branch) and a HEAP ACCUMULATOR
        // reassignment (deferred → the accumulation is dropped) BOTH make that one
        // iteration produce the wrong answer — WALL them rather than silently miscompile.
        // (Walling BEFORE lowering the body avoids emitting partial ops; the executable
        // `try_lower_scalar_while` already declined both shapes and rolled back.)
        self.wall_break_over_heap_frame(body, "while", self.live_heap_handles.len())?;
        if body_reassigns_heap(body) {
            return Err(LowerError::Unsupported(
                "while body with a heap-accumulator reassignment cannot be faithfully lowered \
                 (the model-one-iteration fallback defers the reassignment, dropping the \
                 accumulation) not in this brick"
                    .into(),
            ));
        }
        self.record_elided_calls(cond);
        let mark = self.live_heap_handles.len();
        self.in_frame += 1;
        for stmt in body {
            self.lower_stmt(stmt)?;
        }
        self.in_frame -= 1;
        self.drop_arm_locals(mark);
        Ok(())
    }

    /// Post-lowering loop-body admission for `break`/`continue` reaching the
    /// MODEL-ONE-ITERATION fallback (the executable `try_lower_scalar_*` paths already
    /// decline a break/continue body and roll back, so this is only hit when the loop
    /// linearizes to one modeled iteration). That fallback runs the body straight-line
    /// ONCE with NO loop and NO early-exit branch, so it CANNOT honor an early exit: the
    /// break/continue is silently dropped and the loop produces the wrong answer (e.g.
    /// `while i<100 { if i==7 then break; i=i+1 }; print(i)` → v0 `7`, the one-iteration
    /// form `1`). WALL it — a break/continue is faithfully executed only by the real-loop
    /// markers (`try_lower_scalar_while`/`_for_*`), which do not yet cover early exits.
    /// (This SUBSUMES the prior heap-frame leak wall: a heap-frame early exit would also
    /// skip a per-iteration Drop, but the selection bug walls every break/continue first.)
    pub(crate) fn wall_break_over_heap_frame(
        &self,
        body: &[IrStmt],
        what: &str,
        _mark: usize,
    ) -> Result<(), LowerError> {
        if body_breaks_or_continues(body) {
            return Err(LowerError::Unsupported(format!(
                "{what} body with break/continue cannot be faithfully lowered (the model-one-iteration fallback runs the body once with no early-exit branch, losing the break/continue) not in this brick"
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
