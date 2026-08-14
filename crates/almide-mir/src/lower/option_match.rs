// The tracked-Option `match` EXECUTION unit: `[Some(bind?), None]` over a
// materialized-Option subject branches on the real tag instead of taking
// the both-arms linearization. include!-spliced from control_b.rs.

impl LowerCtx {
    /// The arm-classification gate of [`Self::try_lower_variant_match`]:
    /// exactly a `[Some(bind?), None]` pair, the Some-bind either a scalar
    /// COPY or (for a nested-ownership subject) a heap BORROW. `None` = the
    /// shape declines and the caller falls through to the linearization.
    fn classify_option_match_arms<'a>(
        &self,
        subj: ValueId,
        arms: &'a [IrMatchArm],
    ) -> Option<((&'a IrExpr, Option<(VarId, bool, Ty)>), &'a IrExpr)> {
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
                    _ => return None, // heap bind w/o nested-ownership subject / nested ctor
                };
                if some.is_some() {
                    return None;
                }
                some = Some((&arm.body, bind));
            }
            IrPattern::None | IrPattern::Wildcard => {
                if none.is_some() {
                    return None;
                }
                none = Some(&arm.body);
            }
            _ => return None,
        }
    }
    match (some, none) {
        (Some(s), Some(n)) => Some((s, n)),
        _ => None,
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
            Some(v) if self.value_shapes.get(&v) == Some(&crate::lower::VariantShape::Option) => v,
            _ => return false,
        };
        // Gate 2: exactly a `[Some(scalar-bind?), None]` shape, no guards, Unit bodies.
        if arms.len() != 2 || arms.iter().any(|a| a.guard.is_some()) {
            return false;
        }
        let Some(((some_body, some_bind), none_body)) = self.classify_option_match_arms(subj, arms) else {
            return false;
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
