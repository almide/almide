// the generic-record type-var substitution of calls.rs, include!-spliced back at
// module level (a code move under the 800-line ceiling the codopsy gate holds this
// crate to).

/// Substitute `Ty::TypeVar(name)` with the supplied concrete type throughout `ty` —
/// the generic-record instantiation used by the VALUE MODEL (`Box[Int]`'s `value: T`
/// becomes `value: Int`). Total over `Ty`; an unmapped `TypeVar` is left as-is (the
/// caller's `scalar_field_width` then rejects it, walling the record).
pub(super) fn subst_type_var(
    ty: &Ty,
    subst: &std::collections::HashMap<almide_lang::intern::Sym, Ty>,
) -> Ty {
    match ty {
        Ty::TypeVar(name) => subst.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Ty::Applied(id, args) => {
            Ty::Applied(id.clone(), args.iter().map(|a| subst_type_var(a, subst)).collect())
        }
        Ty::Record { fields } => Ty::Record {
            fields: fields.iter().map(|(n, t)| (*n, subst_type_var(t, subst))).collect(),
        },
        Ty::OpenRecord { fields } => Ty::OpenRecord {
            fields: fields.iter().map(|(n, t)| (*n, subst_type_var(t, subst))).collect(),
        },
        Ty::Tuple(elems) => Ty::Tuple(elems.iter().map(|e| subst_type_var(e, subst)).collect()),
        // A generic PARAMETER of a record decl is stored as a bare `Named(T, [])` (the
        // frontend lowers an uninstantiated type variable to a nullary named type, NOT a
        // `Ty::TypeVar`). When `T` is one of this type's params (it is in `subst`), resolve
        // it to the instantiated arg — this is the #650 "generic field sized by its
        // INSTANTIATED type" fix, the substitution `aggregate_field_tys` relies on so a
        // `Box[Int]` field `value: T` resolves to `Int` (and its heap-ness is decided
        // correctly for the spread-copy / offset paths). A `Named` WITH args is a real
        // applied type — recurse into the args only.
        Ty::Named(name, args) if args.is_empty() && subst.contains_key(name) => {
            subst.get(name).cloned().unwrap_or_else(|| ty.clone())
        }
        Ty::Named(name, args) => {
            Ty::Named(*name, args.iter().map(|a| subst_type_var(a, subst)).collect())
        }
        Ty::Fn { is_effect: _, params, ret } => Ty::Fn { is_effect: false, 
            params: params.iter().map(|p| subst_type_var(p, subst)).collect(),
            ret: Box::new(subst_type_var(ret, subst)),
        },
        Ty::Union(members) => {
            Ty::Union(members.iter().map(|m| subst_type_var(m, subst)).collect())
        }
        // Scalars, Variant, Const*, Unknown, Never, etc. carry no nested TypeVar this
        // brick substitutes through — returned unchanged.
        other => other.clone(),
    }
}
