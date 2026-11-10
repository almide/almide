// [`plan`]'s generic environment and its declared-kind argument rendering.
// `include!`d by native_hof.rs; it shares that module's scope and imports.

/// Bind the declared shape `d` to the representation `t`.
fn bind_shape(env: &mut Env, d: &DTy, t: NTy) -> Option<()> {
    if has_value_conversion(d) {
        bind_callback_shape(env, d, t)?;
    } else {
        bind_param(env, d, t)?;
    }
    Some(())
}

/// [`plan`]'s generic environment: the value arguments' representations,
/// then the checked result's. `None` when a shape has no binding, or a bound
/// is one the generated representation does not preserve.
fn plan_env(hof: &Hof, got: &[(String, NTy)], declared_return: Option<NTy>) -> Option<Env> {
    let mut env = Env::new();
    // Containers establish their element representation before a standalone
    // generic value: a Float list stores bits while its scalar argument is f64.
    for vars in [false, true] {
        for ((d, _), (_, t)) in hof.params.iter().zip(got) {
            let this_pass = !matches!(d, DTy::Fn(..)) && matches!(d, DTy::Var(_)) == vars;
            if this_pass && generic_scalar_lane(&env, d, *t, &hof.bounds).is_none() {
                bind_shape(&mut env, d, *t)?;
            }
        }
    }
    // The checked result constrains callback-only generics before choosing
    // an adapter. Map Float values use an i64 bit carrier: without the
    // result constraint a callback's f64 lane selects the wrong conversion.
    if let Some(want) = declared_return {
        bind_shape(&mut env, &hof.ret, want)?;
    }
    plan_bounds_hold(hof, &env).then_some(env)
}

/// Are the call's comparison bounds ones the generated representation keeps?
fn plan_bounds_hold(hof: &Hof, env: &Env) -> bool {
    // Tuple slots erase scalar source types to lanes. A runtime comparison
    // bound must not silently compare Float bits as Int keys. Until source
    // key types accompany this adapter, only String-bound keys are admitted.
    let tuple_input = hof.params.iter().any(|(d, _)| !matches!(d, DTy::Fn(..)) && has_runtime_tuple(d));
    if tuple_input && hof.bounds.iter().any(|(v, _)| env.get(v) != Some(&NTy::Str)) {
        return false;
    }
    // Admit ordering only when both the source declaration and generated
    // representation preserve it, including recursively declared variants.
    !hof.bounds.iter().any(|(v, cap)| {
        *cap == almide_lang::capability_bounds::Cap::Ord
            && env.get(v).and_then(|k| k.rec()).is_some_and(|i| !rec::native_order_safe(i))
    })
}

/// [`plan`]'s rendering of argument `k` (`a`, rendered as `code`, of kind
/// `t`) by its declared shape `d`.
fn declared_arg(
    cx: &FnArgCx<'_>,
    env: &mut Env,
    side: &mut FnArgOut,
    (hof, k): (&Hof, usize),
    (d, by_ref): (&DTy, bool),
    (a, code, t): (&CallArg, &str, NTy),
) -> Option<String> {
    Some(match d {
        DTy::Fn(dps, dret) => fn_arg(cx, env, (dps, dret), (a, t), side)?,
        DTy::Unit => "()".to_string(),
        // The render carries a Bool in its i64 lane; the runtime takes `bool`.
        DTy::Bool => format!("({code} != 0)"),
        d if has_value_conversion(d) => {
            let owned = runtime_arg(code, t, false, false);
            let value = convert_callback_value(env, d, crate::render_native::owned_nty(t), &owned, false)?;
            if !by_ref {
                return Some(value);
            }
            let snapshot = format!("__tuple_arg_{}_{}", cx.at, k);
            side.prelude.push(format!("let {snapshot} = {value};"));
            format!("&{snapshot}")
        }
        _ => value_arg(hof, (cx.at, k), (a, code, t), side),
    })
}
