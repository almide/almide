
/// The element-drop class a `List[Option/Result]` LITERAL's elements take — the SINGLE
/// classifier the injection pre-scan ([`program_uses_lenlist_elem_lists`]) and the literal
/// builder (`try_lower_record_list_literal_as`) BOTH consult, so `$__drop_list_lenlist` is
/// emitted exactly when a list routes to it (the `field_displayable` agree-by-construction
/// precedent).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CtorElemClass {
    /// The element block owns NO heap (`Option[Int/Bool/Float]` — a scalar payload at
    /// data\[0\] under len-as-tag): the flat per-element `rc_dec` (`DropListStr` via
    /// `heap_elem_lists`) frees it EXACTLY.
    Flat,
    /// The element block's first `len` slots are OWNED handles (`Option[String]` Some =
    /// len 1 + payload; `Result[scalar, String]` Ok = len 0 / Err = len 1 + message;
    /// `Result[String, String]` = the cap-as-tag 1-slot form, len 1 either way): the
    /// len-loop `$__drop_list_lenlist` frees each element's owned slots then the element.
    LenLoop,
}

/// Classify a list-literal ELEMENT type as ctor-materializable, or `None` (the caller keeps
/// the record/tuple/wall paths). Only payload types whose OWN drop is one-level-exact are
/// admitted — an `Option[<heap-field record>]` element would leak its record's fields under
/// the len-loop (its wrapper needs `DropWrapperRec`), so it stays walled.
pub fn lenlist_elem_class(elem_ty: &Ty) -> Option<CtorElemClass> {
    use almide_lang::types::constructor::TypeConstructorId;
    // A one-level-exact HEAP payload: freeing it with ONE rc_dec is exact (no owned interior).
    let flat_heap = |t: &Ty| {
        matches!(t, Ty::String)
            || matches!(t, Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1 && !is_heap_ty(&a[0]))
    };
    match elem_ty {
        Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 => {
            if !is_heap_ty(&a[0]) {
                Some(CtorElemClass::Flat)
            } else if flat_heap(&a[0]) {
                Some(CtorElemClass::LenLoop)
            } else {
                None
            }
        }
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => {
            let ok_admits = !is_heap_ty(&a[0]) || flat_heap(&a[0]);
            let err_admits = flat_heap(&a[1]);
            if ok_admits && err_admits {
                Some(CtorElemClass::LenLoop)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Is `ty` a `List` whose ELEMENT type routes to the len-loop drop ([`lenlist_elem_class`]
/// = `LenLoop`) — the TYPE-driven registration the call-result / merged-bind sites consult
/// (a value of this type must free via `$__drop_list_lenlist`, never the flat
/// `heap_elem_lists` `DropListStr` that would leak each element's owned slots).
pub fn is_lenlist_list_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::List, a)
        if a.len() == 1 && lenlist_elem_class(&a[0]) == Some(CtorElemClass::LenLoop))
}

