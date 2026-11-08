// Runtime generic comparison bounds come from the generated declaration
// index. Check callback source types before their native lane erases them.
fn bounded_type_agrees(bounds: &[(String, almide_lang::capability_bounds::Cap)], d: &DTy, t: &almide_lang::types::Ty) -> bool {
    use almide_lang::types::Ty;
    match d {
        DTy::Var(v) => bounds.iter().filter(|(n, _)| n == v).all(|(_, cap)| rec::source_comparison_agrees(t, *cap, true)),
        DTy::List(e) | DTy::Opt(e) | DTy::Set(e) => match t {
            Ty::Applied(_, args) if args.len() == 1 => bounded_type_agrees(bounds, e, &args[0]),
            _ => false,
        },
        DTy::Result(a, b) | DTy::Map(a, b) => match t {
            Ty::Applied(_, args) if args.len() == 2 => bounded_type_agrees(bounds, a, &args[0]) && bounded_type_agrees(bounds, b, &args[1]),
            _ => false,
        },
        DTy::Tuple(es) => match t {
            Ty::Tuple(ts) if ts.len() == es.len() => es.iter().zip(ts).all(|(d, t)| bounded_type_agrees(bounds, d, t)),
            _ => false,
        },
        DTy::Fn(..) => false,
        _ => true,
    }
}

#[cfg(test)]
mod bound_tests {
    use super::*;
    use almide_lang::{capability_bounds::Cap, types::{constructor::TypeConstructorId as C, Ty}};
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
