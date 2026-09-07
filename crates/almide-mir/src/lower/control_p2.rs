/// Which borrow-through discipline (if any) keeps a variant-match subject
/// alive through the arms — decided once by
/// [`LowerCtx::variant_match_route`], read at the two subject-drop sites.
struct VariantMatchRoute {
    /// The match produces a heap value (the arms lower via `lower_heap_result_arm`).
    heap_res: bool,
    /// str-result heap bind: the payload borrows slot-0, subject drops AFTER the arms.
    str_heap_bind: bool,
    /// Option-tuple payload borrow — same drop-after discipline, len-as-tag @4.
    opt_tuple_bind: bool,
    /// Scalar-Ok / heap-Err `Result[Int, String]` borrow — same drop-after discipline.
    result_heap_err_bind: bool,
}

impl VariantMatchRoute {
    /// The subject's drop is deferred to after the branch-join (a payload
    /// borrows its slot-0 through the arms).
    fn subject_drops_after(&self) -> bool {
        self.str_heap_bind || self.opt_tuple_bind || self.result_heap_err_bind
    }
}

/// Is this parsed payload bind a HEAP (borrowed @12 handle) bind?
fn is_heap_payload_bind(bind: &Option<(VarId, bool, Ty)>) -> bool {
    matches!(bind, Some((_, true, _)))
}

/// The value-position variant match's entry admission: a HEAP subject, exactly
/// two arms, no guards.
fn variant_match_entry_ok(subject: &IrExpr, arms: &[IrMatchArm]) -> bool {
    is_heap_ty(&subject.ty) && two_unguarded_arms(arms)
}

/// Exactly two arms, none guarded — the two-sided match admission every
/// executed variant match requires.
fn two_unguarded_arms(arms: &[IrMatchArm]) -> bool {
    arms.len() == 2 && arms.iter().all(|a| a.guard.is_none())
}

/// Both arm bodies are `Unit` — the statement-position (unit-if) requirement.
fn both_unit(a: &IrExpr, b: &IrExpr) -> bool {
    matches!(a.ty, Ty::Unit) && matches!(b.ty, Ty::Unit)
}

/// Fill a one-shot slot; `None` (decline) if it was already filled — the
/// duplicate-arm rejection.
fn fill_once<T>(slot: &mut Option<T>, v: T) -> Option<()> {
    if slot.is_some() {
        return None;
    }
    *slot = Some(v);
    Some(())
}

/// The Ok-arm bind of a statement-position Result match: scalar Ok
/// (Result[Int,String]) binds a scalar int; a heap-Ok (Result[String,String])
/// binds a heap String — gated to `str_result`.
fn result_ok_arm_bind(inner: &IrPattern, str_result: bool) -> Result<Option<(VarId, Ty)>, ()> {
    match inner {
        IrPattern::Bind { var, ty } if is_heap_ty(ty) == str_result => Ok(Some((*var, ty.clone()))),
        IrPattern::Wildcard => Ok(None),
        _ => Err(()),
    }
}

/// Classify ONE variant-match arm as `(is_then_side, bind)` via the shared
/// payload-bind rule, or `Err(())` for an unsupported shape.
///
/// Option Some (then) / None (else). The bind rule lets a HEAP Some-payload
/// (`some(key)` where key: String/Value/Tuple — toml set_nested's `match list.first(path)`)
/// bind the @12 handle as a BORROW, gated on the Option[heap] subject being tracked
/// nested-ownership (heap_elem_lists, set at tracking time); a scalar payload binds
/// a copy. A Wildcard takes the Option else-side ONLY when the subject is not ALSO
/// result-tracked: a Result Err CTOR bind reuses the Some(string) machinery
/// (materialize_opt_str_some inserts materialized_options), so both flags are
/// true — Result semantics must win (the flexible-side arm). Result Err (then) /
/// Ok (else) BOTH use the bind rule: a scalar Result binds a scalar payload, a
/// str-result (`value.as_string`) binds its slot-0 String as a BORROW (gated on
/// `heap_elem_lists` — only a nested-ownership subject, so a scalar Result still
/// rejects a heap bind); the Ok side carries the str-result's String payload
/// (`ok(s) => emit_scalar(s)`), the very thing `emit` needs. A WILDCARD arm over
/// a RESULT subject (`if let v = x { A } else { B }` — the frontend's if-let
/// desugar emits `Ok(v) => A, _ => B`) takes whichever side the ctor arm did NOT
/// (then=Err when Ok is filled, else=Ok when Err is filled); a wildcard BEFORE
/// any ctor arm is ambiguous → reject.
fn classify_variant_arm(
    arm: &IrMatchArm,
    flags: (bool, bool),
    filled: (bool, bool),
    bind: &dyn Fn(&IrPattern) -> Result<Option<(VarId, bool, Ty)>, ()>,
) -> Result<(bool, Option<(VarId, bool, Ty)>), ()> {
    let (is_option, is_result) = flags;
    let (then_filled, else_filled) = filled;
    match &arm.pattern {
        IrPattern::Some { inner } if is_option => bind(inner).map(|b| (true, b)),
        IrPattern::None if is_option => Ok((false, None)),
        IrPattern::Wildcard if is_option && !is_result => Ok((false, None)),
        IrPattern::Err { inner } if is_result => bind(inner).map(|b| (true, b)),
        IrPattern::Ok { inner } if is_result => bind(inner).map(|b| (false, b)),
        IrPattern::Wildcard if is_result && (then_filled != else_filled) => {
            Ok((!then_filled, None))
        }
        _ => Err(()),
    }
}

/// DESUGAR a tuple Some/Ok payload — `some((idx, line)) => B` → `some($p) => { let (idx,line) = $p; B }`.
/// The single var `$p` is bound via the HEAP-payload path (into `param_values`), so the
/// `let (idx,line) = $p` tuple destructure then lowers (`try_lower_tuple_destructure` borrows each
/// slot). A raw tuple VAR/param destructure alone walls (no `param_values` entry), so the rewrite to
/// the @12-handle bind is required, not a plain var destructure. `$p` ids start above subject+arms.
/// `None` = no tuple payload anywhere (the caller keeps the original arms).
fn desugar_tuple_payload_arms(_subject: &IrExpr, arms: &[IrMatchArm]) -> Option<Vec<IrMatchArm>> {
    let has_tuple_payload = arms.iter().any(|a| {
        matches!(&a.pattern, IrPattern::Some { inner } | IrPattern::Ok { inner }
            if matches!(&**inner, IrPattern::Tuple { .. }))
    });
    if !has_tuple_payload {
        return None;
    }
    // Band-allocated (the multi-line max-scan shape the 32-site sweep missed —
    // the same collision class: a pass introducing binds above these arms would
    // make this scan mint an id another pass already owns).
    let mut next = crate::lower::desugar_var_seed();
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
    Some(out)
}
