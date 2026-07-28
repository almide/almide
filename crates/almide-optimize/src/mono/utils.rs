use std::collections::HashMap;
use almide_lang::types::Ty;

/// Key for a monomorphized instance: (function_name, concrete_type_suffix).
pub(super) type MonoKey = (String, String);

/// Info about a structurally-bounded type parameter in a function.
pub(super) struct BoundedParam {
    /// Index of the parameter in the function signature
    pub(super) param_idx: usize,
    /// Name of the type variable (e.g., "T")
    pub(super) type_var: String,
}

/// Generate a mangled suffix from type variable bindings.
pub(super) fn mangle_suffix(bindings: &HashMap<String, Ty>) -> String {
    let mut entries: Vec<(&String, &Ty)> = bindings.iter().collect();
    entries.sort_by_key(|(k, _)| (*k).clone());
    entries.iter().map(|(_, ty)| mangle_ty(ty)).collect::<Vec<_>>().join("_")
}

/// The type component of a MODULE-level generic's mono key.
///
/// Discovery and the call-site rewriter must agree on this string exactly — it
/// is the third component of the `(module, fn, suffix)` key that decides which
/// call sites share one specialization — so both sides call this one function
/// rather than each spelling the computation out.
///
/// Every bounded type variable contributes a component. An earlier form built
/// the suffix with `filter_map(ty_to_name)`, and `ty_to_name` answers `None`
/// for every compound type: a tuple, an `Applied` (so `Result[..]`, `List[..]`,
/// `Map[..]`), a function type. Those bindings vanished from the key, so two
/// call sites that differed ONLY in a compound binding collided on one
/// specialization, and the winner's signature — closure parameter included —
/// was emitted for both. `result.filter` at `Result[(Bool, String), String]`
/// and at `Result[Result[Float, String], String]` both keyed `String`, giving
/// one `result_filter__String` and a native build failure that `almide check`
/// had already passed (#905). `mangle_ty` is total and injective, so no binding
/// can drop out; a bounded variable with no binding at all gets an explicit
/// marker rather than silently contributing nothing.
pub(super) fn module_mono_suffix(bounds: &[BoundedParam], bindings: &HashMap<String, Ty>) -> String {
    let mut names: Vec<String> =
        bounds.iter().map(|b| b.type_var.clone()).collect::<std::collections::HashSet<_>>()
            .into_iter().collect();
    names.sort();
    names.iter()
        .map(|n| bindings.get(n).map_or_else(|| "NA".to_string(), mangle_ty))
        .collect::<Vec<_>>()
        .join("_")
}

pub(super) fn mangle_ty(ty: &Ty) -> String {
    if let Some(name) = mangle_scalar_ty_name(ty) {
        return name.to_string();
    }
    match ty {
        Ty::Named(name, args) => {
            if args.is_empty() { name.to_string() }
            else {
                let arg_strs: Vec<String> = args.iter().map(mangle_ty).collect();
                format!("{}_{}", name, arg_strs.join("_"))
            }
        }
        Ty::Record { fields } if !fields.is_empty() => {
            let mut names: Vec<String> = fields.iter().map(|(n, _)| n.to_string()).collect();
            names.sort();
            names.join("_")
        }
        Ty::Applied(almide_lang::types::TypeConstructorId::List, args) if args.len() == 1 => format!("List_{}", mangle_ty(&args[0])),
        Ty::Applied(id, args) => {
            let name = id.to_string();
            if args.is_empty() { name } else {
                let arg_strs: Vec<String> = args.iter().map(mangle_ty).collect();
                format!("{}_{}", name, arg_strs.join("_"))
            }
        }
        // A TUPLE and a FN type carry their components in the key. Falling to the
        // `Unknown` catch-all below made every compound-payload instantiation share
        // ONE key, so two calls at different types collapsed into a single
        // monomorphized function: `result.filter` at `Result[(Bool, String), String]`
        // and at `Result[Result[Float, String], String]` both keyed
        // `Result_Unknown_String`, and the survivor's whole signature — closure
        // parameter included — was emitted for both call sites (#905). Two SCALAR
        // instantiations never collided, because scalars have real names; it took a
        // compound payload to expose the hole.
        Ty::Tuple(elems) => {
            let parts: Vec<String> = elems.iter().map(mangle_ty).collect();
            format!("Tup{}_{}", elems.len(), parts.join("_"))
        }
        Ty::Fn { params, ret } => {
            let ps: Vec<String> = params.iter().map(mangle_ty).collect();
            format!("Fn{}_{}_to_{}", params.len(), ps.join("_"), mangle_ty(ret))
        }
        // A mono key's ONE invariant is that two DIFFERENT types never share it: the
        // key decides whether two call sites reuse one specialization, so a collision
        // emits a single function for both and the second call site's whole signature
        // — closure parameter included — is wrong. Every arm above yields a distinct
        // name for a distinct type; anything that reaches here (an empty structural
        // record, an unhandled constructor) used to collapse to a shared literal
        // (`"Unknown"`, or `""` for a field-less record — which is how
        // `result.filter` at `Result[(Bool, String), String]` and at
        // `Result[Result[Float, String], String]` both keyed `_String` and became one
        // function, #905). A structural digest keeps the invariant without pretending
        // to name the type: it is stable within a build, and distinct debug forms give
        // distinct keys.
        other => {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            format!("{other:?}").hash(&mut h);
            format!("Ty{:x}", h.finish())
        }
    }
}

/// Mangled name for a fixed-name scalar `Ty`. Returns `None` for the
/// structural/compound variants (`Named`, `Record`, `Applied`, ...) that
/// `mangle_ty` handles itself.
fn mangle_scalar_ty_name(ty: &Ty) -> Option<&'static str> {
    Some(match ty {
        Ty::Int => "Int",
        Ty::Float => "Float",
        Ty::Int8 => "Int8",
        Ty::Int16 => "Int16",
        Ty::Int32 => "Int32",
        Ty::UInt8 => "UInt8",
        Ty::UInt16 => "UInt16",
        Ty::UInt32 => "UInt32",
        Ty::UInt64 => "UInt64",
        Ty::Float32 => "Float32",
        Ty::String => "String",
        Ty::Bool => "Bool",
        Ty::Bytes => "Bytes",
        Ty::Matrix => "Matrix",
        Ty::Unit => "Unit",
        _ => return None,
    })
}

/// Extract the concrete type name from a Ty for protocol method rewriting.
pub(super) fn ty_to_name(ty: &Ty) -> Option<String> {
    match ty {
        Ty::Named(name, _) => Some(name.to_string()),
        Ty::Int => Some("Int".into()),
        Ty::Float => Some("Float".into()),
        Ty::Int8 => Some("Int8".into()),
        Ty::Int16 => Some("Int16".into()),
        Ty::Int32 => Some("Int32".into()),
        Ty::UInt8 => Some("UInt8".into()),
        Ty::UInt16 => Some("UInt16".into()),
        Ty::UInt32 => Some("UInt32".into()),
        Ty::UInt64 => Some("UInt64".into()),
        Ty::Float32 => Some("Float32".into()),
        Ty::String => Some("String".into()),
        Ty::Bool => Some("Bool".into()),
        Ty::Bytes => Some("Bytes".into()),
        Ty::Matrix => Some("Matrix".into()),
        Ty::Unit => Some("Unit".into()),
        _ => None,
    }
}

/// Check if a type contains a specific TypeVar anywhere in its structure.
/// Uses Ty::any_child_recursive for uniform traversal.
pub(super) fn ty_contains_typevar(ty: &Ty, name: &str) -> bool {
    ty.any_child_recursive(&|t| match t {
        Ty::TypeVar(n) => n == name,
        Ty::Named(n, args) => n == name && args.is_empty(),
        _ => false,
    })
}

pub(super) fn has_typevar(ty: &Ty) -> bool {
    ty.any_child_recursive(&|t| {
        matches!(t, Ty::TypeVar(_))
            || matches!(t, Ty::Named(n, args) if args.is_empty() && n.len() <= 2 && n.chars().next().map_or(false, |c| c.is_uppercase()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use almide_lang::types::TypeConstructorId;

    fn bounds(vars: &[&str]) -> Vec<BoundedParam> {
        vars.iter()
            .enumerate()
            .map(|(i, v)| BoundedParam { param_idx: i, type_var: (*v).to_string() })
            .collect()
    }

    fn bind(pairs: &[(&str, Ty)]) -> HashMap<String, Ty> {
        pairs.iter().map(|(k, t)| ((*k).to_string(), t.clone())).collect()
    }

    fn result_of(payload: Ty) -> Ty {
        Ty::Applied(TypeConstructorId::Result, vec![payload, Ty::String])
    }

    /// The two `result.filter` call sites from #905. They differ ONLY in the
    /// payload bound to `A`, and both payloads are compound — the exact pair
    /// the old `filter_map(ty_to_name)` suffix collapsed onto one key.
    #[test]
    fn compound_bindings_do_not_share_a_key() {
        let b = bounds(&["A", "E"]);
        let tuple_site = module_mono_suffix(
            &b,
            &bind(&[("A", Ty::Tuple(vec![Ty::Bool, Ty::String])), ("E", Ty::String)]),
        );
        let nested_site = module_mono_suffix(
            &b,
            &bind(&[("A", result_of(Ty::Float)), ("E", Ty::String)]),
        );
        assert_ne!(tuple_site, nested_site, "two payload types shared one specialization");
        for s in [&tuple_site, &nested_site] {
            assert!(!s.starts_with('_'), "a binding dropped out of the key: {s:?}");
        }
    }

    /// A function type is a binding like any other: `list.map`-shaped generics
    /// bind a `Ty::Fn`, and two different function types must not collide.
    #[test]
    fn function_bindings_do_not_share_a_key() {
        let b = bounds(&["F"]);
        let to_bool = module_mono_suffix(
            &b,
            &bind(&[("F", Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Bool) })]),
        );
        let to_string = module_mono_suffix(
            &b,
            &bind(&[("F", Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::String) })]),
        );
        assert_ne!(to_bool, to_string);
    }

    /// Scalar bindings keep the names they always had, so this fix does not
    /// churn the symbols of every existing specialization.
    #[test]
    fn scalar_bindings_keep_their_plain_names() {
        assert_eq!(
            module_mono_suffix(&bounds(&["A", "E"]), &bind(&[("A", Ty::Int), ("E", Ty::String)])),
            "Int_String"
        );
    }

    /// A bounded variable with no binding gets an explicit marker: dropping it
    /// silently is the same injectivity hole, one level up.
    #[test]
    fn an_unbound_variable_still_occupies_its_slot() {
        let s = module_mono_suffix(&bounds(&["A", "E"]), &bind(&[("E", Ty::String)]));
        assert_eq!(s, "NA_String");
    }
}
