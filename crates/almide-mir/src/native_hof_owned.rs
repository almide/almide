// Included from native_hof.rs; a text move preserving function scope.
/// The owned twin's name when argument `k` (`a`, of kind `t`) is the
/// `@reuse` param, the call is its last reader, and the render holds it
/// owned: the twin takes it over instead of the borrowing body copying it.
fn handed_over<'h>(hof: &'h Hof, k: usize, a: &CallArg, t: NTy) -> Option<&'h str> {
    let (param, name) = hof.owned.as_ref().filter(|(p, _)| *p == k)?;
    let dying = matches!(a, CallArg::Handle(v) if crate::render_native_move::moving_arg(*v));
    let owned = matches!(t, NTy::Vec | NTy::VecStr | NTy::Rec(_)) && crate::render_native::owned_nty(t) == t;
    (dying && owned && !hof.mutable.iter().any(|m| *m) && hof.params[*param].1).then_some(name.as_str())
}

/// Each closure value's lambda name and captured values (`closure_origins`).
type ClosureOrigins = BTreeMap<ValueId, (String, Vec<ValueId>)>;

/// A `list.flat_map` whose callback is a lambda ending in a list literal of
/// at most eight elements runs as the runtime's array twin
/// (`almide_rt_list_flat_map_arr`, #1337), calling the lambda's array twin
/// (`array_twins`): the result is sized once and no element builds a Vec of
/// its own, where the Vec route allocated and freed one per element. The
/// standard codegen takes the same route (`lower_flat_map_arrays`).
fn flat_map_array_route(
    call: &crate::render_native::NativeCall<'_>,
    (origins, user_fns): (&ClosureOrigins, &BTreeMap<&str, &MirFunction>),
    rendered: &mut [String],
    side: &mut FnArgOut,
) -> Option<&'static str> {
    const ARR: &str = "almide_rt_list_flat_map_arr";
    let (lambda, n, lit) = call.args.iter().find_map(|a| fixed_literal_callback(a, origins, user_fns))?;
    let items = crate::native_floor::runtime_items(ARR)?;
    let head = format!("{}(", crate::render_native::mangle(&lambda));
    // A callback whose adapter copies the lambda's list into a fresh Vec (a
    // Float element's parameter recode brings that copy with it) still
    // returns a Vec whatever the lambda returns: it keeps the Vec route.
    let k = rendered.iter().position(|r| r.contains(&head) && r.contains("-> Vec<i64>") && !r.contains("let mut __copy"))?;
    rendered[k] = rendered[k]
        .replacen(&head, &format!("{}__arr(", crate::render_native::mangle(&lambda)), 1)
        .replacen("-> Vec<i64>", &format!("-> [i64; {n}]"), 1);
    side.shims.extend(items);
    STATE.with(|s| s.borrow_mut().array_twins.insert(lambda, (n, lit)));
    Some(ARR)
}

/// A callback argument naming a lambda whose result is one list literal of
/// at most eight elements that nothing else reads: the lambda, the arity and
/// the literal.
fn fixed_literal_callback(
    a: &CallArg,
    origins: &BTreeMap<ValueId, (String, Vec<ValueId>)>,
    user_fns: &BTreeMap<&str, &MirFunction>,
) -> Option<(String, usize, ValueId)> {
    let (CallArg::Handle(v) | CallArg::Scalar(v)) = a else { return None };
    let (name, _) = origins.get(v)?;
    let l = user_fns.get(name.as_str())?;
    let ret = l.ret?;
    let n = l.ops.iter().find_map(|op| match op {
        Op::ListLit { dst, elems } if *dst == ret && (1..=8).contains(&elems.len()) => Some(elems.len()),
        _ => None,
    })?;
    let mut reads = Vec::new();
    for op in &l.ops {
        crate::mir_ops::op_reads(op, &mut reads);
    }
    (!reads.contains(&ret)).then(|| (name.clone(), n, ret))
}

/// The array twins [`flat_map_array_route`] called, rendered from each
/// lambda's own text (`texts`, by fn name): the same body returning
/// `[i64; N]`, its literal an array. Taken once per render.
pub(crate) fn array_twins(texts: &BTreeMap<String, String>) -> Result<String, LowerError> {
    let twins = STATE.with(|s| std::mem::take(&mut s.borrow_mut().array_twins));
    let mut out = String::new();
    for (lambda, (n, lit)) in twins {
        let mangled = crate::render_native::mangle(&lambda);
        let text = texts.get(&lambda).ok_or_else(|| wall(format!("native: no rendered body for the array twin of `{lambda}`")))?;
        let lit_line = format!("let mut {}: Vec<i64> = vec![", var(lit));
        let (head, ret) = (format!("fn {mangled}("), ") -> Vec<i64> {");
        if !(text.contains(&head) && text.contains(ret) && text.contains(&lit_line)) {
            return Err(wall(format!("native: `{lambda}` does not render as its array twin expects")));
        }
        let twin = text
            .replacen(&head, &format!("fn {mangled}__arr("), 1)
            .replacen(ret, &format!(") -> [i64; {n}] {{"), 1)
            .replacen(&lit_line, &format!("let mut {}: [i64; {n}] = [", var(lit)), 1);
        out.push_str(&twin);
        out.push('\n');
    }
    Ok(out)
}


/// A borrowed string read of a list element or a record field handed to a
/// param that takes it by value: the place's own handle, shared by its count
/// (as a copy of the read is, `render_dup`), never its bytes copied into a
/// new block. `None` for any other argument.
fn shared_place_arg(a: &CallArg, t: NTy, by_ref: bool) -> Option<String> {
    let (false, NTy::StrRef, CallArg::Handle(v)) = (by_ref, t, a) else { return None };
    Some(format!("{}.clone()", crate::render_native_addr::str_field_of(*v)?))
}

/// Whether the runtime body that alone serves `call` takes its param `k` by
/// value, plainly — beside a callback too. [`takes_by_value`] refuses a body
/// with a callback because the linked body may serve the site instead, and
/// lends; a call no body is linked for has only this route.
pub(crate) fn served_by_value(call: &str, k: usize) -> bool {
    of(call).is_some_and(|h| {
        !h.mutable.iter().any(|m| *m)
            && matches!(h.params.get(k), Some((d, false)) if !matches!(d, DTy::Fn(..)) && !has_runtime_tuple(d))
    })
}
