// Declared type bindings for native runtime calls, included by native_hof.rs.
/// The call's type-param bindings, as owned native kinds.
type Env = BTreeMap<String, NTy>;

fn bind(env: &mut Env, v: &str, t: NTy) -> Option<()> {
    match env.get(v) {
        Some(b) if !crate::native_value_repr::same_owned(*b, t) => None,
        Some(_) => Some(()),
        None => {
            env.insert(v.to_string(), t);
            Some(())
        }
    }
}

/// An element or payload of declared type `d` carried as `t`.
fn bind_inner(env: &mut Env, d: &DTy, t: NTy) -> Option<()> {
    bind_param(env, d, t)
}

/// The element kind of a list the render carries.
fn list_elem(t: NTy) -> Option<NTy> {
    match t {
        NTy::Vec | NTy::VecRef => Some(NTy::I64),
        NTy::VecStr | NTy::VecStrRef => Some(NTy::Str),
        NTy::Rec(i) | NTy::RecRef(i) => rec::list_elem(i),
        _ => None,
    }
}

/// The payload kind of an Option the render carries.
fn opt_payload(t: NTy) -> Option<NTy> {
    match t {
        NTy::Opt => Some(NTy::I64),
        NTy::Rec(i) | NTy::RecRef(i) => rec::option_payload(i),
        _ => None,
    }
}

/// Both payloads come from the declared carrier, never its current tag.
fn result_payloads(t: NTy) -> Option<(NTy, NTy)> {
    match t {
        NTy::Res => Some((NTy::I64, NTy::Str)),
        NTy::ResStr => Some((NTy::Str, NTy::Str)),
        NTy::Rec(i) | NTy::RecRef(i) => rec::result_payloads(i),
        _ => None,
    }
}

/// Bind the type params a non-function param of declared type `d` fixes.
fn bind_param(env: &mut Env, d: &DTy, t: NTy) -> Option<()> {
    match d {
        DTy::List(e) => bind_inner(env, e, list_elem(t)?),
        DTy::Opt(e) => bind_inner(env, e, opt_payload(t)?),
        DTy::Result(o, e) => {
            let (ok, err) = result_payloads(t)?;
            bind_inner(env, o, ok)?;
            bind_inner(env, e, err)
        }
        DTy::Var(v) => bind(env, v, crate::render_native::owned_nty(t)),
        DTy::Matrix => rec::is_matrix(t).then_some(()),
        DTy::Value => rec::is_value(t).then_some(()),
        DTy::Bytes => rec::is_bytes(t).then_some(()),
        DTy::Set(e) => {
            let inner = rec::set_elem(t.rec()?)?;
            // Preserve the recursively declared element representation.
            rec::native_set_element_kind(inner).then_some(())?;
            bind_inner(env, e, inner)
        }
        DTy::Map(k, v) => {
            let (kk, vk) = match t {
                NTy::Rec(i) | NTy::RecRef(i) => rec::map_kinds(i)?,
                _ => return None,
            };
            bind_inner(env, k, kk)?;
            bind_inner(env, v, vk)
        }
        DTy::Unit | DTy::Int | DTy::Bool => (t == NTy::I64).then_some(()),
        DTy::Float => (t == NTy::F64).then_some(()),
        DTy::Str => matches!(t, NTy::Str | NTy::StrRef).then_some(()),
        DTy::Tuple(..) | DTy::Fn(..) => None,
    }
}

/// The native kind a declared type stands for under `env`.
fn kind_of(env: &Env, d: &DTy) -> Option<NTy> {
    Some(match d {
        DTy::Var(v) => *env.get(v)?,
        DTy::Tuple(es) => {
            // Tuple fields follow MIR's scalar lanes even when a runtime
            // container binds a type parameter to an actual Rust f64.
            let fields = es.iter().map(|e| kind_of(env, e).map(|k|
                if k == NTy::F64 { NTy::I64 } else { k }))
                .collect::<Option<Vec<_>>>()?;
            NTy::Rec(rec::tuple_type_of(&fields))
        }
        DTy::Unit | DTy::Int | DTy::Bool => NTy::I64,
        DTy::Float => NTy::F64,
        DTy::Str => NTy::Str,
        DTy::Matrix => NTy::Rec(rec::matrix_index()),
        DTy::Value => NTy::Rec(rec::value_index()),
        DTy::Bytes => NTy::Rec(rec::bytes_index()),
        DTy::Set(e) => {
            let inner = kind_of(env, e)?;
            rec::native_set_element_kind(inner).then_some(())?;
            NTy::Rec(rec::set_type_of(inner))
        }
        DTy::List(_) | DTy::Opt(_) | DTy::Result(..) => return wrapped_kind(env, d),
        DTy::Map(k, v) => NTy::Rec(rec::map_type_of(kind_of(env, k)?, kind_of(env, v)?)),
        DTy::Fn(..) => return None,
    })
}

/// [`kind_of`] for a list, an Option or a Result.
fn wrapped_kind(env: &Env, d: &DTy) -> Option<NTy> {
    Some(match d {
        // A list or Option of Float bits would leave the render's lanes
        // (`Vec<i64>`, `Option<i64>`) for the runtime's `f64`: refused.
        DTy::List(e) => match kind_of(env, e)? {
            NTy::I64 => NTy::Vec,
            NTy::Str => NTy::VecStr,
            NTy::F64 => return None,
            k => NTy::Rec(rec::list_type_of(k)),
        },
        DTy::Opt(e) => match kind_of(env, e)? {
            NTy::I64 => NTy::Opt,
            NTy::F64 => return None,
            k => NTy::Rec(rec::option_type_of(k)),
        },
        DTy::Result(o, e) => match (kind_of(env, o)?, kind_of(env, e)?) {
            (NTy::I64, NTy::Str) => NTy::Res,
            (NTy::Str, NTy::Str) => NTy::ResStr,
            (NTy::F64, _) | (_, NTy::F64) => return None,
            (ok, err) => NTy::Rec(rec::result_type_of(ok, err)),
        },
        _ => return None,
    })
}

/// A declared closure param or result of declared type `d` and lambda kind
/// `k`: binds the type params it leaves unbound, else the kinds must agree.
/// `Some(true)` = a declared `Float` the lambda takes in its i64 lane (the
/// MIR carries a Float as its bits), so the adapter converts it.
fn bind_lambda(env: &mut Env, d: &DTy, k: NativeSigKind) -> Option<bool> {
    if *d == DTy::Float && k == NativeSigKind::I64 {
        return Some(true);
    }
    if let Some(t) = kind_of(env, d) {
        return crate::native_value_repr::same_owned(t, clo::owned_nty(k)).then_some(false);
    }
    let t = clo::owned_nty(k);
    bind_param(env, d, t)?;
    kind_of(env, d)
        .filter(|want| crate::native_value_repr::same_owned(*want, t))
        .map(|_| false)
}

#[cfg(test)]
mod type_tests {
    use super::*;
    #[test]
    fn runtime_values_and_sets_bind_only_their_declared_representations() {
        let _guard = rec::install(vec![]);
        let mut env = Env::new();
        let value = NTy::Rec(rec::value_index());
        let bytes = NTy::Rec(rec::bytes_index());
        assert_eq!(bind_param(&mut env, &DTy::Bytes, bytes), Some(()));
        assert_eq!(bind_param(&mut env, &DTy::Bytes, NTy::Vec), None);
        assert_eq!(kind_of(&env, &DTy::Bytes), Some(bytes));
        assert_eq!(rust_agrees(&DTy::Bytes, "&[u8]"), Some(true));
        assert_eq!(rust_agrees(&DTy::Bytes, "&Vec<u8>"), Some(true));
        assert_eq!(rust_agrees(&DTy::Bytes, "Vec<u8>"), Some(false));
        assert_eq!(rust_agrees(&DTy::Bytes, "&mutVec<u8>"), None);
        assert_eq!(rust_agrees(&DTy::Str, "AlmideString"), Some(false));
        assert_eq!(rust_agrees(&DTy::Str, "&str"), Some(true));
        assert_eq!(rust_agrees(&DTy::Str, "String"), None);
        assert_eq!(rust_agrees(&DTy::Str, "str"), None);
        assert_eq!(bind_param(&mut env, &DTy::Value, value), Some(()));
        assert_eq!(bind_param(&mut env, &DTy::Value, NTy::Str), None);
        for inner in [NTy::I64, NTy::Str] {
            let set = NTy::Rec(rec::set_type_of(inner));
            let declared = DTy::Set(Box::new(DTy::Var("A".into())));
            let mut binding = Env::new();
            assert_eq!(bind_param(&mut binding, &declared, set), Some(()));
            assert_eq!(kind_of(&binding, &declared), Some(set));
            assert_eq!(bind_param(&mut binding, &declared, value), None);
        }
        assert_eq!(kind_of(&env, &DTy::Set(Box::new(DTy::Float))), None);
        assert_eq!(kind_of(&env, &DTy::Set(Box::new(DTy::Value))), None);
    }

    #[test]
    fn recursive_payload_bindings_keep_both_result_arms_and_nominal_types() {
        let _guard = rec::install(vec![
            rec::RecDecl {
                name: "numbers".into(),
                list_of: Some(NTy::I64),
                ..Default::default()
            },
            rec::RecDecl {
                name: "response".into(),
                result_of: Some((NTy::Rec(0), NTy::Str)),
                ..Default::default()
            },
            rec::RecDecl {
                name: "responses".into(),
                list_of: Some(NTy::Rec(1)),
                ..Default::default()
            },
            rec::RecDecl {
                name: "record-a".into(),
                fields: vec![NTy::Str],
                ..Default::default()
            },
            rec::RecDecl {
                name: "record-b".into(),
                fields: vec![NTy::Str],
                ..Default::default()
            },
        ]);
        let declared = DTy::List(Box::new(DTy::Result(
            Box::new(DTy::List(Box::new(DTy::Var("A".into())))),
            Box::new(DTy::Var("E".into())),
        )));
        let mut env = Env::new();
        assert_eq!(bind_param(&mut env, &declared, NTy::RecRef(2)), Some(()));
        assert_eq!(env.get("A"), Some(&NTy::I64));
        assert_eq!(env.get("E"), Some(&NTy::Str));
        assert!(crate::native_value_repr::same_owned(
            kind_of(&env, &declared).unwrap(),
            NTy::Rec(2)
        ));
        assert_eq!(bind(&mut env, "A", NTy::Str), None);
        assert_eq!(bind(&mut env, "R", NTy::Rec(3)), Some(()));
        assert_eq!(bind(&mut env, "R", NTy::Rec(4)), None);
        assert_eq!(bind(&mut env, "L", NTy::Vec), Some(()));
        assert_eq!(bind(&mut env, "L", NTy::Rec(0)), Some(()));
        let result_float = DTy::Result(Box::new(DTy::Float), Box::new(DTy::Str));
        assert_eq!(kind_of(&env, &result_float), None);
    }
}
