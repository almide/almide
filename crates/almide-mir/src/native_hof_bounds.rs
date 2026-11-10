/// Only comparison-bound variables carried by this callback prohibit the
/// scalar value adapter. A bound on an unrelated map key says nothing about
/// the representation of the mapped value/result.
fn mentions_comparison_bound(d: &DTy, bounds: &[(String, almide_lang::capability_bounds::Cap)]) -> bool {
    match d {
        DTy::Var(v) => bounds.iter().any(|(name, _)| name == v),
        DTy::List(e) | DTy::Opt(e) | DTy::Set(e) => mentions_comparison_bound(e, bounds),
        DTy::Map(a, b) | DTy::Result(a, b) => mentions_comparison_bound(a, bounds) || mentions_comparison_bound(b, bounds),
        DTy::Tuple(es) => es.iter().any(|e| mentions_comparison_bound(e, bounds)),
        DTy::Fn(ps, r) => ps.iter().any(|e| mentions_comparison_bound(e, bounds)) || mentions_comparison_bound(r, bounds),
        _ => false,
    }
}

// Runtime generic comparison bounds come from the generated declaration
// index. Check callback source types before their native lane erases them.
fn bounded_type_agrees(bounds: &[(String, almide_lang::capability_bounds::Cap)], d: &DTy, t: &almide_lang::types::Ty) -> bool {
    bounded_type_agrees_mode(bounds, d, t, true)
}

fn bounded_type_agrees_mode(bounds: &[(String, almide_lang::capability_bounds::Cap)], d: &DTy, t: &almide_lang::types::Ty, scalar: bool) -> bool {
    use almide_lang::types::Ty;
    match d {
        DTy::Var(v) => bounds.iter().filter(|(n, _)| n == v).all(|(_, cap)| rec::source_comparison_agrees(t, *cap, scalar)),
        DTy::List(e) | DTy::Opt(e) | DTy::Set(e) => match t {
            Ty::Applied(_, args) if args.len() == 1 => bounded_type_agrees_mode(bounds, e, &args[0], scalar),
            _ => false,
        },
        DTy::Result(a, b) | DTy::Map(a, b) => match t {
            Ty::Applied(_, args) if args.len() == 2 => bounded_type_agrees_mode(bounds, a, &args[0], scalar) && bounded_type_agrees_mode(bounds, b, &args[1], scalar),
            _ => false,
        },
        DTy::Tuple(es) => match t {
            Ty::Tuple(ts) if ts.len() == es.len() => es.iter().zip(ts).all(|(d, t)| bounded_type_agrees_mode(bounds, d, t, scalar)),
            _ => false,
        },
        DTy::Fn(ps, ret) if !scalar => match t {
            Ty::Fn { params, ret: actual_ret, .. } if ps.len() == params.len() =>
                ps.iter().zip(params).all(|(d, t)| bounded_type_agrees_mode(bounds, d, t, scalar))
                    && bounded_type_agrees_mode(bounds, ret, actual_ret, scalar),
            _ => false,
        },
        DTy::Fn(..) => false,
        _ => true,
    }
}


/// Comparison routing follows the declaration's generic capability bounds,
/// including a callback's key type, rather than a second list of API names.
/// Before scalar-lane adaptation the original collection values must agree.
pub(crate) fn declared_comparisons_agree(call: &str, args: &[almide_lang::types::Ty], result: &almide_lang::types::Ty) -> bool {
    crate::mir_ops::self_host_runtime().iter().any(|(_, pairs)| pairs.iter().any(|(implementation, source)| {
        if *source != call { return false; }
        let Some(hof) = of(implementation) else { return false };
        !hof.bounds.is_empty() && hof.params.len() == args.len()
            && hof.params.iter().zip(args).all(|((d, _), t)| bounded_type_agrees_mode(&hof.bounds, d, t, false))
            && bounded_type_agrees_mode(&hof.bounds, &hof.ret, result, false)
    }))
}

#[cfg(test)]
mod bound_tests {
    use super::*;
    use almide_lang::{capability_bounds::Cap, types::{constructor::TypeConstructorId as C, Ty}};
    #[test]
    fn declared_collection_comparisons_preserve_source_types() {
        let list = |t| Ty::Applied(C::List, vec![t].into());
        assert!(declared_comparisons_agree("list.sort", &[list(Ty::Int)], &list(Ty::Int)));
        assert!(!declared_comparisons_agree("list.sort", &[list(Ty::Float)], &list(Ty::Float)));
        let key = |ret| Ty::Fn { params: vec![Ty::String], ret: Box::new(ret), is_effect: false };
        assert!(declared_comparisons_agree("list.unique_by", &[list(Ty::String), key(Ty::Int)], &list(Ty::String)));
        assert!(!declared_comparisons_agree("list.unique_by", &[list(Ty::String), key(Ty::Float)], &list(Ty::String)));
        assert!(!declared_comparisons_agree("list.reverse", &[list(Ty::Int)], &list(Ty::Int)), "no comparison bound");
    }

    #[test]
    fn callback_value_adaptation_preserves_its_own_comparison_bounds() {
        let bounds = vec![("K".into(), Cap::Eq)];
        let key = DTy::Var("K".into());
        let value = DTy::Var("V".into());
        assert!(!mentions_comparison_bound(&value, &bounds));
        for d in [key.clone(), DTy::Tuple(vec![value.clone(), key.clone()]),
            DTy::List(Box::new(key.clone())), DTy::Opt(Box::new(key.clone())),
            DTy::Result(Box::new(value.clone()), Box::new(key.clone())),
            DTy::Map(Box::new(key.clone()), Box::new(value.clone())),
            DTy::Set(Box::new(key.clone())), DTy::Fn(vec![value.clone()], Box::new(key))] {
            assert!(mentions_comparison_bound(&d, &bounds));
        }
    }

    #[test]
    fn generic_keys_keep_numeric_equality_and_do_not_compare_float_bits() {
        let bounds = vec![("K".into(), Cap::Eq)];
        let key = DTy::Var("K".into());
        assert!(bounded_type_agrees(&bounds, &key, &Ty::Float));
        assert!(!bounded_type_agrees(&bounds, &key, &Ty::Float32));
        assert!(bounded_type_agrees(&bounds, &key, &Ty::Applied(C::List, vec![Ty::Int].into())));
        assert!(!bounded_type_agrees(&bounds, &key, &Ty::Applied(C::List, vec![Ty::Float].into())));
        assert!(!bounded_type_agrees(&bounds, &key, &Ty::Applied(C::Option, vec![Ty::Float].into())));
        assert!(bounded_type_agrees(&[], &key, &Ty::Applied(C::List, vec![Ty::Float].into())));
    }
}
