use super::Ty;

/// Check if binding TypeVar `var` to `ty` would create an infinite type.
/// Recursively checks all type constructors: List, Option, Result, Map, Tuple, Record, Fn.
fn occurs_in(var: &str, ty: &Ty) -> bool {
    match ty {
        Ty::TypeVar(name) => name == var,
        Ty::List(inner) | Ty::Option(inner) => occurs_in(var, inner),
        Ty::Result(a, b) | Ty::Map(a, b) => occurs_in(var, a) || occurs_in(var, b),
        Ty::Tuple(elems) => elems.iter().any(|e| occurs_in(var, e)),
        Ty::Record { fields } | Ty::OpenRecord { fields, .. } => fields.iter().any(|(_, t)| occurs_in(var, t)),
        Ty::Fn { params, ret } => params.iter().any(|p| occurs_in(var, p)) || occurs_in(var, ret),
        Ty::Named(_, args) => args.iter().any(|a| occurs_in(var, a)),
        Ty::Union(members) => members.iter().any(|m| occurs_in(var, m)),
        _ => false,
    }
}

/// Unify a signature type against a concrete type, collecting TypeVar bindings.
/// Returns true if the types are compatible. Unknown still accepts anything (error recovery).
pub fn unify(sig_ty: &Ty, actual_ty: &Ty, bindings: &mut std::collections::HashMap<std::string::String, Ty>) -> bool {
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
        if let Some(bound) = bindings.get(name) {
            return bound.compatible(actual_ty);
        } else {
            // Occurs check: prevent infinite types like T = List[T]
            if occurs_in(name, actual_ty) {
                return false;
            }
            bindings.insert(name.clone(), actual_ty.clone());
            return true;
        }
    }
    // When actual is a TypeVar, it represents an unresolved polymorphic type.
    // Accept it (polymorphic types are compatible with anything) but don't bind —
    // the TypeVar will be resolved when the concrete call happens.
    if matches!(actual_ty, Ty::TypeVar(_)) {
        return true;
    }
    match (sig_ty, actual_ty) {
        (Ty::List(a), Ty::List(b)) => unify(a, b, bindings),
        (Ty::Option(a), Ty::Option(b)) => unify(a, b, bindings),
        (Ty::Result(a1, a2), Ty::Result(b1, b2)) => unify(a1, b1, bindings) && unify(a2, b2, bindings),
        (Ty::Map(k1, v1), Ty::Map(k2, v2)) => unify(k1, k2, bindings) && unify(v1, v2, bindings),
        (Ty::Fn { params: p1, ret: r1 }, Ty::Fn { params: p2, ret: r2 }) => {
            if p1.len() != p2.len() { return false; }
            p1.iter().zip(p2.iter()).all(|(a, b)| unify(a, b, bindings)) && unify(r1, r2, bindings)
        }
        (Ty::Tuple(a), Ty::Tuple(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| unify(x, y, bindings))
        }
        // Named types with type args: unify each arg to bind TypeVars
        (Ty::Named(a, a_args), Ty::Named(b, b_args)) if a == b && a_args.len() == b_args.len() => {
            a_args.iter().zip(b_args.iter()).all(|(x, y)| unify(x, y, bindings))
        }
        // Union: try each member with snapshotted bindings, commit first success
        (Ty::Union(members), _) => {
            for m in members {
                let mut snapshot = bindings.clone();
                if unify(m, actual_ty, &mut snapshot) { *bindings = snapshot; return true; }
            }
            false
        }
        (_, Ty::Union(members)) => {
            for m in members {
                let mut snapshot = bindings.clone();
                if unify(sig_ty, m, &mut snapshot) { *bindings = snapshot; return true; }
            }
            false
        }
        _ => sig_ty.compatible(actual_ty),
    }
}

/// Substitute TypeVars in a type using the collected bindings.
pub fn substitute(ty: &Ty, bindings: &std::collections::HashMap<std::string::String, Ty>) -> Ty {
    if bindings.is_empty() {
        return ty.clone();
    }
    match ty {
        Ty::TypeVar(name) => bindings.get(name).cloned().unwrap_or_else(|| Ty::TypeVar(name.clone())),
        Ty::Unknown => Ty::Unknown,
        Ty::List(inner) => Ty::List(Box::new(substitute(inner, bindings))),
        Ty::Option(inner) => Ty::Option(Box::new(substitute(inner, bindings))),
        Ty::Result(ok, err) => Ty::Result(Box::new(substitute(ok, bindings)), Box::new(substitute(err, bindings))),
        Ty::Map(k, v) => Ty::Map(Box::new(substitute(k, bindings)), Box::new(substitute(v, bindings))),
        Ty::Fn { params, ret } => Ty::Fn {
            params: params.iter().map(|p| substitute(p, bindings)).collect(),
            ret: Box::new(substitute(ret, bindings)),
        },
        Ty::Tuple(tys) => Ty::Tuple(tys.iter().map(|t| substitute(t, bindings)).collect()),
        Ty::Record { fields } => Ty::Record {
            fields: fields.iter().map(|(n, t)| (n.clone(), substitute(t, bindings))).collect(),
        },
        Ty::OpenRecord { fields } => Ty::OpenRecord {
            fields: fields.iter().map(|(n, t)| (n.clone(), substitute(t, bindings))).collect(),
        },
        Ty::Union(members) => Ty::union(members.iter().map(|m| substitute(m, bindings)).collect()),
        Ty::Named(name, args) if !args.is_empty() => {
            Ty::Named(name.clone(), args.iter().map(|a| substitute(a, bindings)).collect())
        }
        _ => ty.clone(),
    }
}

/// Check if a type contains any unbound TypeVars.
pub fn contains_typevar(ty: &Ty) -> bool {
    match ty {
        Ty::TypeVar(_) => true,
        Ty::List(inner) | Ty::Option(inner) => contains_typevar(inner),
        Ty::Result(a, b) | Ty::Map(a, b) => contains_typevar(a) || contains_typevar(b),
        Ty::Tuple(elems) => elems.iter().any(contains_typevar),
        Ty::Fn { params, ret } => params.iter().any(contains_typevar) || contains_typevar(ret),
        Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().any(|(_, t)| contains_typevar(t)),
        Ty::Union(members) => members.iter().any(contains_typevar),
        Ty::Named(_, args) => args.iter().any(contains_typevar),
        _ => false,
    }
}
