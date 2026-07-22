use super::Ty;
use crate::intern::Sym;

/// Check if binding TypeVar `var` to `ty` would create an infinite type.
/// Uses Ty::any_child_recursive for uniform traversal across all type constructors.
fn occurs_in(var: Sym, ty: &Ty) -> bool {
    ty.any_child_recursive(&|t| matches!(t, Ty::TypeVar(name) if *name == var))
}

/// Unify a signature type against a concrete type, collecting TypeVar bindings.
/// Returns true if the types are compatible. Unknown still accepts anything (error recovery).
pub fn unify(sig_ty: &Ty, actual_ty: &Ty, bindings: &mut std::collections::HashMap<Sym, Ty>) -> bool {
    // Unknown: both Unknown → accept. One Unknown → accept but don't mask errors.
    // This is still lenient for error recovery, but avoids hiding real mismatches
    // when one side has a known type.
    if *sig_ty == Ty::Unknown && *actual_ty == Ty::Unknown {
        return true;
    }
    if *sig_ty == Ty::Unknown || *actual_ty == Ty::Unknown {
        return true;
    }
    // TypeVar: bind or check consistency
    if let Ty::TypeVar(name) = sig_ty {
        return unify_bind_typevar(*name, actual_ty, bindings);
    }
    // When actual is a TypeVar, it represents an unresolved polymorphic type.
    // Accept it (polymorphic types are compatible with anything) but don't bind —
    // the TypeVar will be resolved when the concrete call happens.
    if matches!(actual_ty, Ty::TypeVar(_)) {
        return true;
    }
    match (sig_ty, actual_ty) {
        (Ty::Applied(id1, args1), Ty::Applied(id2, args2)) if id1 == id2 && args1.len() == args2.len() => {
            args1.iter().zip(args2.iter()).all(|(a, b)| unify(a, b, bindings))
        }
        // Bare `Matrix` unifies with `Matrix[T]` by binding T → Float.
        // `Matrix` is the legacy non-parametric form; pre-P4 code treats
        // it as Matrix[Float] (the f64 default). Without this bridge,
        // a generic fn like `fn get[T](m: Matrix[T], ...)` rejects
        // every bare-Matrix call site because the TypeVar never binds.
        (Ty::Applied(crate::types::TypeConstructorId::Matrix, args), Ty::Matrix)
        | (Ty::Matrix, Ty::Applied(crate::types::TypeConstructorId::Matrix, args))
            if args.len() == 1 =>
        {
            unify(&args[0], &Ty::Float, bindings)
        }
        (Ty::Fn { params: p1, ret: r1 }, Ty::Fn { params: p2, ret: r2 }) => {
            if p1.len() != p2.len() { return false; }
            p1.iter().zip(p2.iter()).all(|(a, b)| unify(a, b, bindings)) && unify(r1, r2, bindings)
        }
        (Ty::Tuple(a), Ty::Tuple(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| unify(x, y, bindings))
        }
        // Named types with type args: unify each arg to bind TypeVars
        // Use bare name comparison to handle cross-module qualified names
        // (e.g. "patcher.PatchResult" vs "PatchResult")
        (Ty::Named(a, a_args), Ty::Named(b, b_args))
            if super::names_match(*a, *b) && a_args.len() == b_args.len() =>
        {
            a_args.iter().zip(b_args.iter()).all(|(x, y)| unify(x, y, bindings))
        }
        // Union: try each member with snapshotted bindings, commit first success
        (Ty::Union(members), _) => unify_union_sig(members, actual_ty, bindings),
        (_, Ty::Union(members)) => unify_union_actual(sig_ty, members, bindings),
        _ => sig_ty.compatible(actual_ty),
    }
}

/// TypeVar-on-the-signature-side case of `unify`: bind `name` to `actual_ty` if
/// unbound (subject to the occurs check), or check consistency against an
/// existing binding.
fn unify_bind_typevar(name: Sym, actual_ty: &Ty, bindings: &mut std::collections::HashMap<Sym, Ty>) -> bool {
    if let Some(bound) = bindings.get(&name) {
        return bound.compatible(actual_ty);
    }
    // Occurs check: prevent infinite types like T = List[T]
    if occurs_in(name, actual_ty) {
        return false;
    }
    bindings.insert(name, actual_ty.clone());
    true
}

/// `Ty::Union(members)` on the signature side: try each member with
/// snapshotted bindings, commit the bindings of the first member that unifies.
fn unify_union_sig(members: &[Ty], actual_ty: &Ty, bindings: &mut std::collections::HashMap<Sym, Ty>) -> bool {
    for m in members {
        let mut snapshot = bindings.clone();
        if unify(m, actual_ty, &mut snapshot) {
            *bindings = snapshot;
            return true;
        }
    }
    false
}

/// `Ty::Union(members)` on the actual side: same as `unify_union_sig` but with
/// the union on the other operand.
fn unify_union_actual(sig_ty: &Ty, members: &[Ty], bindings: &mut std::collections::HashMap<Sym, Ty>) -> bool {
    for m in members {
        let mut snapshot = bindings.clone();
        if unify(sig_ty, m, &mut snapshot) {
            *bindings = snapshot;
            return true;
        }
    }
    false
}

/// Substitute TypeVars in a type using the collected bindings.
/// Uses Ty::map_children for uniform recursive traversal.
pub fn substitute(ty: &Ty, bindings: &std::collections::HashMap<Sym, Ty>) -> Ty {
    if bindings.is_empty() {
        return ty.clone();
    }
    match ty {
        // TypeVar: look up binding or keep as-is
        Ty::TypeVar(name) => bindings.get(name).cloned().unwrap_or_else(|| Ty::TypeVar(*name)),
        // All other types: recursively substitute children
        _ => ty.map_children(&|child| substitute(child, bindings)),
    }
}

/// Check if a type contains any unbound TypeVars.
/// Uses Ty::any_child_recursive for uniform traversal.
pub fn contains_typevar(ty: &Ty) -> bool {
    ty.any_child_recursive(&|t| matches!(t, Ty::TypeVar(_)))
}
