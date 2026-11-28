// ── part of binds_p3_b.rs, include!-spliced (split for the 800-line budget) ──

impl LowerCtx {
    /// Rung 3b: the Option/Map element families and the flat-aggregate tail
    /// — the ordered continuation of rung 3 (same ladder, same order).
    fn classify_elem_drop_map_options(&self, elem_ty: &Ty) -> Option<ListElemDrop> {
        // An `Option[Map[String, <scalar>]]` element (`[some(["k0": true]), some(n1),
        // none]` — Wave 4 L6): the payload map breaks the lenlist "one-level-exact"
        // rule (its interior owns key Strings), so it takes its OWN class with the
        // static 3-level `$__drop_list_omb` (list → option block len-slot → the msb
        // key sweep) instead of widening the shared `$__drop_list_lenlist` in place.
        // Decided BEFORE the lenlist arm on purpose — lenlist_elem_class would
        // return None for a map payload and fall to the nested-ownership wall.
        if matches!(elem_ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, o)
            if o.len() == 1
                && matches!(&o[0], Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, b)
                    if b.len() == 2 && matches!(b[0], Ty::String)
                        && matches!(b[1], Ty::Bool | Ty::Int | Ty::Float)))
        {
            return Some(ListElemDrop::OptMapSkv);
        }
        // A BARE `Map[String, <scalar>]` element (`[["k0": true], ["k1": false]]` —
        // Wave 4 P2, reduced to `let xs: List[Map[String, Bool]] = [["k0": true]]`).
        // Same reasoning as the `Option[Map[…]]` arm directly above, one indirection
        // shorter: the element map's interior owns its key Strings, which breaks the
        // lenlist "one-level-exact" rule, so it takes its own class with the static
        // 2-level `$__drop_list_mb` (list -> the msb key sweep, which also rc_decs the
        // element block). Decided BEFORE the lenlist arm for the same reason the
        // Option sibling is — lenlist_elem_class returns None for a map element and it
        // would fall through to the nested-ownership wall.
        if matches!(elem_ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, b)
            if b.len() == 2 && matches!(b[0], Ty::String)
                && matches!(b[1], Ty::Bool | Ty::Int | Ty::Float))
        {
            return Some(ListElemDrop::MapSkv);
        }
        // An `Option[<record>]` element (`[some(Inner { n: 1 }), none]` — the codec
        // `lc: List[Inner?]` cell, #1134): each element is a 0-or-1 Option block whose
        // Some payload is the record — freed via the generated tag-aware
        // `$__drop_list_opt_<R>` (Some → the record's own free: one rc_dec for a flat
        // record, `$__drop_<R>` recursion for a rich one). Decided BEFORE the lenlist
        // arm (lenlist's flat_heap test would decline the record payload and fall to
        // the nested-ownership wall). Gated to a NON-generic Named record with a
        // resolvable layout; variants keep their own classes.
        if let almide_lang::types::Ty::Applied(
            almide_lang::types::constructor::TypeConstructorId::Option, oa) = elem_ty
        {
            if let [Ty::Named(n, args)] = &oa[..] {
                if args.is_empty()
                    && self.custom_variant_type_name(&oa[0]).is_none()
                    && self.aggregate_field_tys(&oa[0]).is_some()
                {
                    return Some(ListElemDrop::OptRecord(n.as_str().to_string()));
                }
            }
        }
        if let Some(class) = crate::lower::lenlist_elem_class(elem_ty) {
            return Some(match class {
                crate::lower::CtorElemClass::Flat => ListElemDrop::CtorFlat,
                crate::lower::CtorElemClass::LenLoop => ListElemDrop::CtorLenLoop,
            });
        }
        if matches!(elem_ty, Ty::Fn { .. }) {
            // A `List[<Fn>]` LITERAL element (`[(x: Int) => x + 1, (x: Int) => x * 2]` —
            // #623's closure-parameter shape): each element is a fresh closure BLOCK (lifted
            // via `lift_lambda`, the SAME mechanism a call-argument lambda already uses),
            // freed per-element via the generated `$__drop_list_closure` (recurses into the
            // uniform `$__drop_closure` — required even for a non-capturing lambda, since the
            // LIST's TYPE alone (`List[(Int)->Int]`) does not preclude a capturing element).
            return Some(ListElemDrop::Closure);
        }
        // A SCALAR-ONLY aggregate element (a record/tuple whose every field is
        // inline scalar — snaidhm's `GlyphPoint`, ceangal's rect records). Its
        // block is FLAT: it owns no further heap, so the per-element `rc_dec` of
        // the Flat class IS its full free — exactly the physics `calls_p2`'s
        // `scalar_aggregate_elem` and C-183's nested sweep already rely on, and
        // the same reason a flat VARIANT element takes `CtorFlat` above.
        //
        // Classifying it here is what lets a `[xs[i]]` operand of a
        // `List[<flat record>]` concat go through the LITERAL builder (which has
        // the per-element arms) instead of the generic call-argument path, which
        // declined it and walled the enclosing accumulator loop (#888/#904).
        if is_heap_ty(elem_ty)
            && self
                .aggregate_field_tys(elem_ty)
                .and_then(|(_, tys)| crate::lower::layout::scalar_slots(&tys))
                .is_some()
        {
            return Some(ListElemDrop::CtorFlat);
        }
        None
    }
}
