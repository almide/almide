impl LowerCtx {

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
                // A HEAP `==`/`!=` condition (`if e == err("a") then println(…) …` —
                // the rc4 shape, previously the call-bearing linearization wall): the
                // typed materialized eq yields a REAL scalar Bool (rollback-safe on
                // decline), so the if executes ONE arm like any scalar cond.
                let heap_eq = if let IrExprKind::BinOp { op, left, right } = &cond.kind {
                    match op {
                        almide_ir::BinOp::Eq if is_heap_ty(&left.ty) => {
                            self.lower_heap_eq_cond(left, right, false)
                        }
                        almide_ir::BinOp::Neq if is_heap_ty(&left.ty) => {
                            self.lower_heap_eq_cond(left, right, true)
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                match heap_eq {
                    Some(v) => v,
                    None => {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        return false;
                    }
                }
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
    /// Extracted from `Self::try_lower_variant_match` (codopsy7 max-depth sweep): the
    /// nested-Option/Result/aggregate read-shape seeding for a HEAP `some(payload)` bind,
    /// verbatim (pure text move — was nested 2 levels deeper inside the caller's
    /// `if let Some(..) = some_bind { if is_heap { .. } }`, which alone pushed every arm of
    /// this classification past the depth threshold). The Some payload is itself an
    /// Option/Result (`some(inner)` where `inner: Option[Int]` — a NESTED match): track it
    /// so an INNER `match inner {…}` BRANCHES (reads its tag @4) instead of LINEARIZING
    /// (running every arm). The payload is a BORROWED handle of the OUTER Option's owned
    /// inner block — the same materialized-Option read-shape, no new ownership. Without
    /// this the nested match fell to the both-arms linearization (printing every arm + a
    /// garbage 0).
    fn seed_option_some_payload_read_shape(&mut self, payload: ValueId, bind_ty: &Ty) {
        use almide_lang::types::constructor::TypeConstructorId;
        if matches!(bind_ty, Ty::Applied(TypeConstructorId::Option, _)) {
            self.materialized_options.insert(payload);
            if crate::lower::is_lenlist_list_ty(bind_ty) {
                self.variant_drop_handles.insert(payload, "list_lenlist".to_string());
            } else if crate::lower::is_heap_elem_list_ty(bind_ty) {
                self.heap_elem_lists.insert(payload);
            }
            return;
        }
        if crate::lower::is_result_ty(bind_ty) {
            self.materialized_results.insert(payload);
            if crate::lower::is_lenlist_list_ty(bind_ty) {
                self.variant_drop_handles.insert(payload, "list_lenlist".to_string());
            } else if crate::lower::is_heap_elem_list_ty(bind_ty) {
                self.heap_elem_lists.insert(payload);
            }
            return;
        }
        if self.aggregate_field_tys(bind_ty).is_some() {
            // A RECORD/TUPLE payload (`some(p) => … p.name …` — the optional-chain
            // heap-field projection): seed its aggregate READ-shape so a field
            // access inside the arm loads the real slot (a borrowed handle of the
            // Option's owned payload block — no ownership event, exactly the
            // seed_variant_param aggregate discipline).
            self.materialized_aggregates.insert(payload);
        }
    }

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
                            if is_heap_ty(ty)
                                && (self.heap_elem_lists.contains(&subj)
                                    // `Option[List[String]]` (the heap-acc fold value) — routed
                                    // to the nested DropListListStr set; the payload-borrow
                                    // discipline is identical.
                                    || self.list_list_str_lists.contains(&subj)
                                    // An `Option[record]` subject (the materialized option
                                    // toplet — its drop routes "optrec:<R>" via
                                    // DropWrapperRec): the record payload binds as the SAME
                                    // borrow; the option's recursive drop keeps ownership.
                                    || self
                                        .variant_drop_handles
                                        .get(&subj)
                                        .is_some_and(|d| d.starts_with("optrec:"))) =>
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
                self.seed_option_some_payload_read_shape(payload, &bind_ty);
            }
        }
        // Exactly ONE arm runs at runtime (the unit-if discipline): an outer var's
        // reassignment inside an arm mutates the stable local IN PLACE — scalar via
        // SetLocal, heap via the drop-old + SetLocal rebind — see `unit_arm_depth`.
        self.unit_arm_depth += 1;
        let some_ok = self.lower_branch_arm(None, some_body).is_ok();
        if !some_ok {
            self.unit_arm_depth -= 1;
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return false;
        }
        self.ops.push(Op::Else { val: None });
        let none_ok = self.lower_branch_arm(None, none_body).is_ok();
        self.unit_arm_depth -= 1;
        if !none_ok {
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return false;
        }
        self.ops.push(Op::EndIf { val: None });
        true
    }
}

include!("control_p2.rs");
include!("control_p2_b.rs");
include!("control_p2_c.rs");
include!("control_p2_d.rs");
include!("control_p3.rs");
include!("control_p3_b.rs");
include!("control_p3_c.rs");
include!("heap_result_arm.rs");
include!("heap_result_arm_b.rs");
include!("result_materialize.rs");
include!("result_ctors.rs");
include!("scalar_for.rs");
// The defunc HOF family (formerly one 3.5k-line control_p5.rs), split by concern:
include!("defunc_hof.rs");
include!("defunc_fold.rs");
include!("defunc_fold_b.rs");
include!("defunc_str_acc.rs");
include!("defunc_str_acc_b.rs");
include!("defunc_find.rs");
include!("defunc_tuple_fold.rs");
include!("defunc_tuple_fold_b.rs");
include!("control_while.rs");

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
