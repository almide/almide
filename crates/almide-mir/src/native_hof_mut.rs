// Declaration-derived mutable runtime loans, included by native_hof.rs.

/// A declared runtime receiver with no user callback boundary. This is used
/// only to schedule a native global value-cell publication after its mutation.
pub(crate) fn plain_mutator(name: &str, k: usize) -> bool {
    if crate::native_floor::declared(name).is_some_and(|f|
        matches!(f.params.get(k), Some(crate::native_floor::FloorParam::BytesMut))) {
        return true;
    }
    mutable_hof(name, k).is_some_and(|h| !h.params.iter().any(|(d, _)| matches!(d, DTy::Fn(..))))
}

/// Whether an argument is a place, including a callback-bearing mutator
/// that the plain receiver publication transform deliberately does not serve.
pub(crate) fn mutable_argument(name: &str, k: usize) -> bool {
    if let Some((module, func)) = name.split_once('.') {
        if let Some(program) = almide_lang::stdlib_info::bundled_source(module).and_then(almide_lang::parse_cached) {
            if program.decls.iter().any(|d| matches!(d,
                Decl::Fn { name, params, .. } if name.as_str() == func && params.get(k).is_some_and(|p| p.is_mut))) {
                return true;
            }
        }
    }
    crate::native_floor::declared(name).is_some_and(|f|
        matches!(f.params.get(k), Some(crate::native_floor::FloorParam::BytesMut)))
        || mutable_hof(name, k).is_some()
}

fn mutable_hof(name: &str, k: usize) -> Option<&'static Hof> {
    let impl_name = crate::mir_ops::self_host_runtime().iter().flat_map(|(_, pairs)| pairs.iter())
        .find_map(|(i, call)| (*call == name).then_some(*i)).unwrap_or(name);
    of(impl_name).filter(|h| h.mutable.get(k) == Some(&true))
}

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
    let (mut base, mut place) = crate::render_native_addr::field_of(*v)?;
    let mut seen = std::collections::BTreeSet::new();
    while matches!(tys.get(&base), Some(NTy::RecRef(_))) && seen.insert(base) {
        let (parent, parent_place) = crate::render_native_addr::field_of(base)?;
        let suffix = place.strip_prefix(&crate::render_native::var(base))?;
        place = format!("{parent_place}{suffix}");
        base = parent;
    }
    matches!(tys.get(&base), Some(NTy::Rec(_))).then(|| format!("&mut {place}"))
}

#[cfg(test)]
mod mutable_route_tests {
    use super::*;

    #[test]
    fn borrowed_option_declarations_keep_value_and_reference_arguments_distinct() {
        let d = DTy::Opt(Box::new(DTy::Var("A".into())));
        assert_eq!(rust_agrees(&d, "&Option<A>"), Some(true));
        assert_eq!(runtime_arg("o", NTy::Opt, true, false), "&o");
        assert_eq!(runtime_arg("o", NTy::Rec(0), true, false), "&o");
        assert_eq!(runtime_arg("o", NTy::RecRef(0), true, false), "o");
        for name in ["option_is_some", "option_is_none"] {
            assert_eq!(derive_explained(name).unwrap().params[0].1, true);
        }
    }

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
        let pop = derive_explained("list_pop").unwrap();
        assert_eq!(pop.mutable, [true]);
        assert_eq!(pop.symbol, "almide_rt_list_pop");
    }
}
