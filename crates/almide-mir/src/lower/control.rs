//! `LowerCtx` methods: control (extracted from lower/mod.rs).

use super::*;
use crate::{CallArg, IntOp, Op, ValueId};
use almide_ir::{
    CallTarget, IrExpr, IrExprKind, IrMatchArm, IrPattern, IrStmt, VarId,
};
use almide_lang::types::Ty;

/// One parsed arm of a custom-variant `match` (ADT bricks 3/5c). A `Ctor` arm tests `tag ==
/// tag` and binds its fields from slots — `(slot index 1+i, bound var, is_heap, field ty)`: a
/// SCALAR field is an i64 value copy; a leaf-heap (`String`) field is a BORROW of the slot
/// handle (the subject keeps ownership). The field TY lets an Option/Result payload bind seed
/// its READ-shape (`seed_variant_param`) so an inner `match` over it executes. A move-out arm
/// auto-`Dup`s in `lower_heap_result_arm`; a consuming re-use `Dup`s in
/// `lower_owned_heap_field` — so the borrow is never released at rc 0. A `Wildcard` arm is the
/// unconditional catch-all.
enum VariantArmKind {
    Ctor { tag: i64, binds: Vec<(usize, VarId, bool, Ty)> },
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
            IrExprKind::If { .. } => {
                self.lower_branch_if(expr)
            }
            IrExprKind::Match { .. } => {
                self.lower_branch_match(expr)
            }
            other => Err(LowerError::Unsupported(format!(
                "lower_branch on a non-branch {}",
                kind_name(other)
            ))),
        }
    }

    /// Extracted from `Self::lower_branch` (pattern-2 uniform-arm split, cog reduction):
    /// the arm body verbatim, re-narrowed via `let-else`. Pure text move.
    fn lower_branch_if(&mut self, expr: &IrExpr) -> Result<(), LowerError> {
        let IrExprKind::If { cond, then, else_ } = &expr.kind else { unreachable!() };
        // The condition is evaluated ONCE before the branch — it is scalar
        // (Bool), so no ownership, but capture the caps of any call in it.
        //
        // A CALL-BEARING arm must NOT linearize: reaching here means every
        // real-branch path (try_lower_unit_if / the scalar-if machinery)
        // declined the condition, and the linearized render RUNS BOTH arms —
        // the rc4 double-print (`println(if e == err("a") then "eq" else
        // "ne")` printed eq AND ne, 2026-07-12). WALL it, mirroring the
        // untracked-subject match rule below: an unlowered shape must be a
        // clean Unsupported, never wrong output. (Call-free arms stay
        // linearizable — their double-evaluation has no observable effect;
        // the merged Opaque result carries the content.)
        if crate::lower::expr_contains_call(then) || crate::lower::expr_contains_call(else_)
        {
            // Name WHICH cond shape declined — the burn-down histogram (and any
            // user staring at the wall) needs the operator + operand type, not
            // just "unresolvable".
            fn operand_desc(e: &IrExpr) -> String {
                match &e.kind {
                    IrExprKind::Call { target, .. } => {
                        let callee = match target {
                            almide_ir::CallTarget::Named { name, .. } => {
                                name.as_str().to_string()
                            }
                            almide_ir::CallTarget::Module { module, func, .. } => {
                                format!("{}.{}", module.as_str(), func.as_str())
                            }
                            almide_ir::CallTarget::Method { method, .. } => {
                                format!(".{}", method.as_str())
                            }
                            almide_ir::CallTarget::Computed { .. } => "<computed>".into(),
                        };
                        format!("Call[{callee}]")
                    }
                    other => kind_name(other).to_string(),
                }
            }
            let cond_desc = match &cond.kind {
                IrExprKind::BinOp { op, left, right } => {
                    format!(
                        "{op:?} over {:?}; {} vs {}",
                        left.ty,
                        operand_desc(left),
                        operand_desc(right)
                    )
                }
                other => format!("{} of {:?}", kind_name(other), cond.ty),
            };
            return Err(LowerError::Unsupported(format!(
                "if over an unresolvable condition ({cond_desc}) with a call-bearing \
                 arm cannot take the both-arms linearization (it would run the \
                 untaken arm's effects) not in this brick"
            )));
        }
        self.record_elided_calls(cond);
        self.lower_branch_arm(None, then)?;
        self.lower_branch_arm(None, else_)?;
        Ok(())
    }

    /// Extracted from `Self::lower_branch` (pattern-2 uniform-arm split, cog reduction):
    /// the arm body verbatim, re-narrowed via `let-else`. Pure text move.
    fn lower_branch_match(&mut self, expr: &IrExpr) -> Result<(), LowerError> {
        let IrExprKind::Match { subject, arms } = &expr.kind else { unreachable!() };
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
            self.seed_match_subject_read_shape(subject, v)?;
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

    /// Extracted from `Self::lower_branch_match` (second-round split, cog reduction):
    /// the subject read-shape seeding block (formerly `if let Some(v) = subject_value { .. }`),
    /// verbatim. Returns `Err` for the one WALL case (a never-err lifted-effect-fn subject
    /// outside the `ok(x)` shape); the caller propagates it with `?`.
    fn seed_match_subject_read_shape(&mut self, subject: &IrExpr, v: ValueId) -> Result<(), LowerError> {
        self.seed_match_subject_field_shape(subject, v);
        self.seed_match_subject_option_call_shape(subject, v);
        self.seed_match_subject_result_call_shape(subject, v);
        self.wall_match_subject_never_err_lifted_call(subject)?;
        self.seed_match_subject_user_call_shape(subject, v);
        Ok(())
    }

    /// Extracted from `Self::seed_match_subject_read_shape` (second-round split, cog
    /// reduction): the record/tuple FIELD subject block, verbatim.
    fn seed_match_subject_field_shape(&mut self, subject: &IrExpr, v: ValueId) {
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
    }

    /// Extracted from `Self::seed_match_subject_read_shape` (second-round split, cog
    /// reduction): the self-host Option-call subject block, verbatim.
    fn seed_match_subject_option_call_shape(&mut self, subject: &IrExpr, v: ValueId) {
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
            // `map.find`'s `Option[(String, <scalar>)]` payload OWNS a HEAP slot (the
            // String) inside the tuple — the flat `heap_elem_lists`/`DropListStr` route
            // above would only `rc_dec` the TUPLE's own handle, leaking its String (the
            // exact class of bug this session's `_str`-dispatch fix already caught
            // elsewhere). Override with the type-specific recursive
            // `$__drop_opt_str_int` (generated, gated on `program_calls_map_find`) —
            // `variant_drop_handles` is checked BEFORE `heap_elem_lists` in the drop-op
            // cascade (`drop_op_for`), so this takes priority for the SAME value; the
            // `heap_elem_lists` entry above still stands and is harmlessly unused for
            // drop purposes (kept only so the Some-arm heap-bind admission gate, which
            // checks `heap_elem_lists.contains`, still fires).
            use almide_lang::types::constructor::TypeConstructorId;
            if let Ty::Applied(TypeConstructorId::Option, oa) = &subject.ty {
                if oa.len() == 1 {
                    if let Ty::Tuple(tys) = &oa[0] {
                        if tys.len() == 2
                            && matches!(tys[0], Ty::String)
                            && !is_heap_ty(&tys[1])
                        {
                            self.variant_drop_handles.insert(v, "opt_str_int".to_string());
                        }
                    }
                }
            }
        }
    }

    /// Extracted from `Self::seed_match_subject_read_shape` (second-round split, cog
    /// reduction): the self-host Result-call subject blocks, verbatim.
    fn seed_match_subject_result_call_shape(&mut self, subject: &IrExpr, v: ValueId) {
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
    }

    /// Extracted from `Self::seed_match_subject_read_shape` (second-round split, cog
    /// reduction): the never-err lifted-effect-fn WALL check, verbatim.
    fn wall_match_subject_never_err_lifted_call(&mut self, subject: &IrExpr) -> Result<(), LowerError> {
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
            if crate::lower::NEVER_ERR_LIFTED_FNS.with(|s| s.borrow().contains(name.as_str()))
                && !crate::lower::AUTO_WRAP_ABI_FNS.with(|s| s.borrow().contains(name.as_str()))
            {
                return Err(LowerError::Unsupported(
                    "match over a never-err effect-fn call with a non-`ok(x)` Ok pattern \
                     (ok(_)/structured/guarded) not in this brick — the effect-fn returns a \
                     raw value, so there is no Result tag to dispatch on (the heap-effect-fn \
                     error-model frontier)".into(),
                ));
            }
        }
        Ok(())
    }

    /// Extracted from `Self::seed_match_subject_read_shape` (second-round split, cog
    /// reduction): the user Named-call / pure Module-call subject's Option/Result read-shape
    /// tracking — computes `result_call_subject` once, verbatim, then delegates to the
    /// Option and Result read-shape sub-blocks (second split of the same extraction).
    fn seed_match_subject_user_call_shape(&mut self, subject: &IrExpr, v: ValueId) {
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
        if !result_call_subject {
            return;
        }
        self.seed_match_subject_user_call_option_shape(subject, v);
        self.seed_match_subject_user_call_result_shape(subject, v);
    }

    /// Extracted from `Self::seed_match_subject_user_call_shape` (third-round split, cog
    /// reduction): the OPTION-returning branch, verbatim (only called when
    /// `result_call_subject` is true).
    fn seed_match_subject_user_call_option_shape(&mut self, subject: &IrExpr, v: ValueId) {
        // A user Named-call (or pure Module-call) subject returning OPTION
        // (`match get_profile(1) { some(p) => …, none => … }` — the
        // optional-chain desugar's shape after the let-bind continuation
        // transform): the callee builds a REAL same-layout Option block (the
        // v1 calling convention — `seed_variant_param`'s contract, the same
        // trust the let-bound Named-call path in binds_p2 already places).
        // Track its READ-shape so the match EXECUTES (len-as-tag) instead of
        // walling at the linearization gate. The DROP routes by payload: a
        // RICH record payload recurses through the option wrapper
        // (`optrec:<R>` → `$__drop_<R>` — checked BEFORE heap_elem_lists in
        // drop_op_for, the map.find precedent); the heap_elem_lists entry
        // additionally opens the Some-arm heap-payload borrow gate.
        if let Ty::Applied(
            almide_lang::types::constructor::TypeConstructorId::Option,
            a,
        ) = &subject.ty
        {
            if a.len() == 1 {
                self.materialized_options.insert(v);
                if let Some(rn) = self.record_or_anon_drop_type_name(&a[0]) {
                    self.variant_drop_handles.insert(v, format!("optrec:{rn}"));
                    self.heap_elem_lists.insert(v);
                } else if is_heap_ty(&a[0]) {
                    self.heap_elem_lists.insert(v);
                }
            }
        }
    }

    /// Extracted from `Self::seed_match_subject_user_call_shape` (third-round split, cog
    /// reduction): the RESULT-returning branch, verbatim (only called when
    /// `result_call_subject` is true).
    fn seed_match_subject_user_call_result_shape(&mut self, subject: &IrExpr, v: ValueId) {
        if !crate::lower::is_result_ty(&subject.ty) {
            return;
        }
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
                // Route through the STATEMENT dispatcher, not lower_effect_call
                // directly: an in-place mutator tail (`if c then { list.push(out,
                // x) } else { … }` — the arm-tail shape) must take the SAME
                // functional-rebind interceptions a statement-position push gets
                // (mod_p3's push/clear/insert arms), or it falls through to a raw
                // unlinked `list.push` CallFn (#782: the retired v0 emitter used
                // to absorb that). Double-run safety: a call-bearing arm never
                // reaches the linearization (lower_branch walls it), so this arm
                // only executes under a REAL branch (try_lower_unit_if).
                IrExprKind::Call { .. } if matches!(tail.ty, Ty::Unit) => {
                    self.lower_stmt_expr(tail)?
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
}
include!("control_b.rs");
