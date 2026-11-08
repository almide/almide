// Declaration-derived mutable runtime loans, included by native_hof.rs.

fn declared_param_mode(d: &DTy, mutable: bool, rust: &str) -> Option<bool> {
    match (mutable, rust.strip_prefix("&mut")) {
        (true, Some(inner)) => rust_agrees(d, &format!("&{inner}")),
        (false, None) => rust_agrees(d, rust),
        _ => None,
    }
}

/// A mutable loan must point at the caller's owned binding. A read-only
/// view cannot substitute a clone: the mutation would disappear on return.
fn mutable_runtime_arg(code: &str, got: NTy) -> Option<String> {
    matches!(got, NTy::Str | NTy::Vec | NTy::VecStr | NTy::Rec(_))
        .then(|| format!("&mut {code}"))
}

fn mutable_place_arg(a: &CallArg, code: &str, got: NTy, tys: &BTreeMap<ValueId, NTy>) -> Option<String> {
    if let Some(arg) = mutable_runtime_arg(code, got) { return Some(arg); }
    // A projected field is a read-only view, but the declared mutator lends
    // the actual field of its owned parent. Never mutate a cloned view.
    let CallArg::Handle(v) = a else { return None };
    let (base, place) = crate::render_native_addr::field_of(*v)?;
    matches!(tys.get(&base), Some(NTy::Rec(_))).then(|| format!("&mut {place}"))
}

#[cfg(test)]
mod mutable_route_tests {
    use super::*;

    #[test]
    fn mutable_runtime_loans_require_agreeing_declarations_and_owned_values() {
        let list = DTy::List(Box::new(DTy::Var("A".into())));
        assert_eq!(declared_param_mode(&list, true, "&mutVec<A>"), Some(true));
        assert_eq!(declared_param_mode(&list, false, "&mutVec<A>"), None);
        assert_eq!(declared_param_mode(&list, true, "&Vec<A>"), None);
        assert_eq!(declared_param_mode(&list, true, "Vec<A>"), None);
        assert_eq!(mutable_runtime_arg("xs", NTy::Vec), Some("&mut xs".into()));
        for view in [NTy::VecRef, NTy::VecStrRef, NTy::RecRef(0), NTy::StrRef] {
            assert_eq!(mutable_runtime_arg("xs", view), None);
        }
        let pop = derive_explained("list_pop").expect("list_pop has a derived runtime signature");
        assert_eq!(pop.mutable, [true]);
        assert_eq!(pop.symbol, "almide_rt_list_pop");
    }
}
