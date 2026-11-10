// Preserve explicit Float instantiations of generic runtime declarations.
// MIR stores list Float elements as i64 bits. Binding the generic from that
// erased representation would compare bit patterns rather than Float values.
// Only a consistent, declaration-level Float binding selects this conversion.
fn specialize_float_params(impl_fn: &str, params: &mut [(DTy, bool)], ret: &mut DTy) {
    let declared = crate::mir_ops::self_host_runtime().iter().find_map(|(source, pairs)| {
        pairs.iter().any(|(name, _)| *name == impl_fn).then_some(())?;
        let source = almide_lang::parse_cached(source)?;
        source.decls.iter().find_map(|d| match d {
            Decl::Fn { name, params, .. } if name.as_str() == impl_fn =>
                params.iter().map(|p| dty_of(&p.ty, &BTreeSet::new())).collect::<Option<Vec<_>>>(),
            _ => None,
        })
    });
    let Some(declared) = declared.filter(|ds| ds.len() == params.len()) else { return };
    let mut bindings = BTreeMap::<String, DTy>::new();
    for ((generic, _), concrete) in params.iter().zip(&declared) {
        if !match_specialized_type(generic, concrete, &mut bindings) { return; }
    }
    let floats: BTreeSet<_> = bindings.into_iter().filter_map(|(v, d)|
        (d == DTy::Float).then_some(v)).collect();
    for (d, _) in params { substitute_float(d, &floats); }
    substitute_float(ret, &floats);
}

fn match_specialized_type(generic: &DTy, concrete: &DTy, bindings: &mut BTreeMap<String, DTy>) -> bool {
    match (generic, concrete) {
        (DTy::Var(v), c) => match bindings.get(v) {
            Some(old) => old == c,
            None => { bindings.insert(v.clone(), c.clone()); true }
        },
        (DTy::List(a), DTy::List(b)) | (DTy::Opt(a), DTy::Opt(b)) | (DTy::Set(a), DTy::Set(b)) =>
            match_specialized_type(a, b, bindings),
        (DTy::Map(a, b), DTy::Map(c, d)) | (DTy::Result(a, b), DTy::Result(c, d)) =>
            match_specialized_type(a, c, bindings) && match_specialized_type(b, d, bindings),
        (DTy::Tuple(a), DTy::Tuple(b)) => a.len() == b.len() && a.iter().zip(b)
            .all(|(a, b)| match_specialized_type(a, b, bindings)),
        _ => generic == concrete,
    }
}

fn substitute_float(d: &mut DTy, floats: &BTreeSet<String>) {
    match d {
        DTy::Var(v) if floats.contains(v) => *d = DTy::Float,
        DTy::List(e) | DTy::Opt(e) | DTy::Set(e) => substitute_float(e, floats),
        DTy::Map(a, b) | DTy::Result(a, b) => { substitute_float(a, floats); substitute_float(b, floats); }
        DTy::Tuple(es) => es.iter_mut().for_each(|e| substitute_float(e, floats)),
        DTy::Fn(ps, r) => { ps.iter_mut().for_each(|p| substitute_float(p, floats)); substitute_float(r, floats); }
        _ => (),
    }
}

#[cfg(test)]
mod specialized_float_tests {
    use super::*;
    #[test]
    fn float_sort_keys_keep_the_declared_callback_type() {
        for name in ["list_sort_by_float", "list_sort_by_float_rc"] {
            let h = derive_explained(name).expect("the named HOF derives");
            assert_eq!(h.symbol, "almide_rt_list_sort_by_float");
            assert!(matches!(&h.params[1].0, DTy::Fn(_, r) if **r == DTy::Float));
            assert!(matches!(&h.params[0].0, DTy::List(e) if matches!(**e, DTy::Var(_))));
        }
    }
    #[test]
    fn float_specializations_use_value_equality_and_reject_conflicting_bindings() {
        for name in ["list_contains_float", "list_index_of_float", "list_unique_float", "list_dedup_float"] {
            let h = derive_explained(name).expect("the named HOF derives");
            assert_eq!(h.params[0].0, DTy::List(Box::new(DTy::Float)), "{name}");
        }
        let var = DTy::Var("A".into());
        let mut env = BTreeMap::new();
        assert!(match_specialized_type(&var, &DTy::Float, &mut env));
        assert!(!match_specialized_type(&var, &DTy::Int, &mut env));
        assert!(!match_specialized_type(&DTy::List(Box::new(var)), &DTy::Float, &mut env));
        assert!(matches!(derive_explained("list_contains").unwrap().params[0].0, DTy::List(ref e) if matches!(**e, DTy::Var(_))));
    }
}
