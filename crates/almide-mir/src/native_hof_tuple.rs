// Explicit tuple positions in a runtime callback use Rust tuples. The MIR
// lambda uses its checked aggregate declaration. Adapt values at that boundary;
// a nominal record with coincidentally equal fields is never a tuple.

// The shared entry-split pass spreads explicit tuple parameters for the
// registered self-hosted callback ABI. Reassemble that boundary from the
// declaration and the lifted signature's arity, never from a function name.
fn callback_layout(dps: &[DTy], arity: usize) -> Option<Vec<(DTy, String)>> {
    if dps.len() == arity {
        return Some(dps.iter().enumerate().map(|(i, d)| (d.clone(), format!("__p{i}"))).collect());
    }
    for last_only in [false, true] {
        let mut out = Vec::new();
        for (i, d) in dps.iter().enumerate() {
            if let DTy::Tuple(es) = d {
                if es.len() >= 2 && (!last_only || i + 1 == dps.len()) {
                    out.extend(es.iter().enumerate().map(|(j, e)| (e.clone(), format!("__p{i}.{j}"))));
                    continue;
                }
            }
            out.push((d.clone(), format!("__p{i}")));
        }
        if out.len() == arity { return Some(out); }
    }
    None
}

fn bind_callback_shape(env: &mut Env, d: &DTy, got: NTy) -> Option<()> {
    if !matches!(d, DTy::Tuple(_) | DTy::List(_) | DTy::Opt(_) | DTy::Result(_, _)) {
        if has_runtime_tuple(d) { return None; }
        let runtime = kind_of(env, d);
        if got == NTy::I64 && runtime == Some(NTy::F64) { return Some(()); }
        // Generic containers retain MIR's Float bits, while a checked lambda
        // takes a Float as f64. Concrete Int declarations never use this arm.
        if matches!(d, DTy::Var(_)) && got == NTy::F64 && runtime == Some(NTy::I64) { return Some(()); }
        return bind_param(env, d, got);
    }
    match d {
        DTy::Tuple(es) => {
            let fields = rec::tuple_fields(got.rec()?)?;
            if es.len() != fields.len() { return None; }
            for (e, f) in es.iter().zip(fields) { bind_callback_shape(env, e, f)?; }
            Some(())
        }
        DTy::List(e) => bind_callback_shape(env, e, list_elem(got)?),
        DTy::Opt(e) => bind_callback_shape(env, e, opt_payload(got)?),
        DTy::Result(a, b) => {
            let (x, y) = result_payloads(got)?;
            bind_callback_shape(env, a, x)?;
            bind_callback_shape(env, b, y)
        }
        _ => None,
    }
}

fn runtime_type(env: &Env, d: &DTy) -> Option<String> {
    Some(match d {
        DTy::Unit => "()".into(),
        DTy::Bool => "bool".into(),
        DTy::Tuple(es) => {
            let ts = es.iter().map(|e| runtime_type(env, e)).collect::<Option<Vec<_>>>()?;
            format!("({}{})", ts.join(", "), if ts.is_empty() { "" } else { "," })
        }
        DTy::List(e) => format!("Vec<{}>", runtime_type(env, e)?),
        DTy::Opt(e) => format!("Option<{}>", runtime_type(env, e)?),
        DTy::Result(a, b) => format!("Result<{}, {}>", runtime_type(env, a)?, runtime_type(env, b)?),
        DTy::Fn(..) => return None,
        _ => rec::owned_ty(kind_of(env, d)?),
    })
}

/// `into_native` selects the direction. Every expression is evaluated once,
/// and owned fields move through the adapter instead of escaping as references.
fn convert_callback_value(env: &Env, d: &DTy, native: NTy, code: &str, into_native: bool) -> Option<String> {
    if !matches!(d, DTy::Tuple(_) | DTy::List(_) | DTy::Opt(_) | DTy::Result(_, _)) {
        if has_runtime_tuple(d) { return None; }
        return match d {
            DTy::Bool if native == NTy::I64 => Some(if into_native {
                format!("({code}) as i64")
            } else { format!("({code}) != 0") }),
            DTy::Unit if native == NTy::I64 => Some(if into_native {
                format!("{{ {code}; 0_i64 }}")
            } else { format!("{{ let _ = {code}; () }}") }),
            _ => {
                let runtime = kind_of(env, d)?;
                if crate::native_value_repr::same_owned(runtime, native) { return Some(code.into()); }
                let (from, to) = if into_native { (runtime, native) } else { (native, runtime) };
                crate::native_value_repr::scalar_recode(code, from, to)
                    .or_else(|| crate::native_value_repr::tuple_copy(code, from, to))
            }
        };
    }
    Some(match d {
        DTy::Tuple(es) => {
            let i = native.rec()?;
            let fields = rec::tuple_fields(i)?;
            if fields.len() != es.len() { return None; }
            let converted = es.iter().zip(fields).enumerate().map(|(k, (e, f))| {
                let access = if into_native { format!("__tuple.{k}") } else { format!("__tuple.f{k}") };
                let v = convert_callback_value(env, e, f, &access, into_native)?;
                Some(if into_native { format!("f{k}: {v}") } else { v })
            }).collect::<Option<Vec<_>>>()?;
            let value = if into_native {
                format!("{} {{ {} }}", rec::struct_name(i), converted.join(", "))
            } else {
                format!("({}{})", converted.join(", "), if converted.is_empty() { "" } else { "," })
            };
            format!("{{ let __tuple = {code}; {value} }}")
        }
        DTy::List(e) => {
            let value = convert_callback_value(env, e, list_elem(native)?, "__item", into_native)?;
            format!("({code}).into_iter().map(|__item| {value}).collect::<Vec<_>>()")
        }
        DTy::Opt(e) => format!("({code}).map(|__item| {})",
            convert_callback_value(env, e, opt_payload(native)?, "__item", into_native)?),
        DTy::Result(a, b) => {
            let (x, y) = result_payloads(native)?;
            format!("({code}).map(|__item| {}).map_err(|__item| {})",
                convert_callback_value(env, a, x, "__item", into_native)?,
                convert_callback_value(env, b, y, "__item", into_native)?)
        }
        // Hashing/ordering containers need their own declared semantics.
        _ => return None,
    })
}

fn tuple_callback_adapter(env: &Env, lambda: Option<&str>, captured: &str, dps: &[DTy], dret: &DTy,
    ps: &[NativeSigKind], ret: Option<NativeSigKind>) -> Option<String> {
    if ps.iter().chain(ret.iter()).any(|k| matches!(k, NativeSigKind::Narrow { .. })) { return None; }
    let mut params = Vec::new();
    let mut setup = Vec::new();
    let mut args = Vec::new();
    for (i, d) in dps.iter().enumerate() {
        params.push(format!("__p{i}: {}", runtime_type(env, d)?));
    }
    for (i, ((d, code), k)) in callback_layout(dps, ps.len())?.iter().zip(ps).enumerate() {
        let ty = clo::owned_nty(*k);
        let value = convert_callback_value(env, d, ty, code, true)?;
        setup.push(format!("let __a{i} = {value};"));
        let borrow = matches!(ty, NTy::Str | NTy::Vec | NTy::VecStr | NTy::Rec(_));
        args.push(format!("{}__a{i}", if borrow { "&" } else { "" }));
    }
    let call = format!("{}{})", callback_head(lambda, captured, ps, ret),
        args.iter().map(|a| format!(", {a}")).collect::<String>());
    let call = if ret == Some(NativeSigKind::F64) { format!("AlmdLaneF64::lane_f64({call})") } else { call };
    let result = match ret {
        Some(ret) => convert_callback_value(env, dret, clo::owned_nty(ret), &call, false)?,
        None if *dret == DTy::Unit => call,
        None => return None,
    };
    Some(format!("|{}| -> {} {{ {} {result} }}", params.join(", "), runtime_type(env, dret)?, setup.join(" ")))
}

#[cfg(test)]
mod tuple_callback_tests {
    use super::*;

    #[test]
    fn spread_callback_arguments_follow_the_declared_tuple_positions() {
        let pair = DTy::Tuple(vec![DTy::Str, DTy::Int]);
        let acc = DTy::Var("A".into());
        let layout = callback_layout(&[acc.clone(), pair.clone()], 3).unwrap();
        assert_eq!(layout, vec![(acc, "__p0".into()), (DTy::Str, "__p1.0".into()), (DTy::Int, "__p1.1".into())]);
        assert_eq!(callback_layout(&[pair.clone()], 1), Some(vec![(pair.clone(), "__p0".into())]));
        assert!(callback_layout(&[pair], 3).is_none());
        for call in ["map.filter", "map.filter_skv", "map.find", "map.find_skv", "map.fold",
            "map.fold_skv", "map.fold_str_msi", "map.fold_skv_msi", "map.fold_str_sacc", "map.fold_skv_hacc"] {
            assert!(declared_call(call), "{call}");
        }
    }

    #[test]
    fn tuple_callbacks_use_checked_fields_and_refuse_nominal_shapes() {
        let _guard = rec::install(vec![
            rec::RecDecl { name: "Tuple([String, Float])".into(), fields: vec![NTy::Str, NTy::I64], ..Default::default() },
            rec::RecDecl { name: "NamedRecord".into(), fields: vec![NTy::Str, NTy::I64], ..Default::default() },
        ]);
        let pair = DTy::Tuple(vec![DTy::Str, DTy::Float]);
        let mut env = Env::new();
        assert_eq!(bind_callback_shape(&mut env, &pair, NTy::Rec(0)), Some(()));
        assert_eq!(bind_callback_shape(&mut env, &pair, NTy::Rec(1)), None);
        let input = convert_callback_value(&env, &pair, NTy::Rec(0), "read_once()", true).unwrap();
        assert_eq!(input.matches("read_once()").count(), 1);
        assert!(input.contains("to_bits()"));
        let output = convert_callback_value(&env, &pair, NTy::Rec(0), "result_once()", false).unwrap();
        assert_eq!(output.matches("result_once()").count(), 1);
        assert!(output.contains("f64::from_bits"));
        assert!(convert_callback_value(&env, &pair, NTy::Rec(1), "wrong", true).is_none());
    }
}
