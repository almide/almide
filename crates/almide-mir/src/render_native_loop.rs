// ── the render_fn op loop of render_native.rs, include!-spliced back at module level ──
//
// A pure code move: this file continues its parent verbatim. The split exists
// only so the parent stays under the 800-line ceiling the codopsy gate holds
// this crate to; there is no boundary of meaning here, and `include!` at module
// level is the one splice Rust allows (an impl-item position rejects it).

/// The running state of one [`render_fn`] op loop: the value-type map, the
/// source being built, the open if-as-value joins, and the per-function facts
/// the loop consults. One value, so each phase of the loop is a named helper
/// that reads and extends the same state.
struct FnLoop<'a> {
    func: &'a MirFunction,
    user_fns: &'a BTreeMap<&'a str, &'a MirFunction>,
    sigs: &'a NativeSigs,
    used_shims: &'a mut Vec<&'static str>,
    tys: BTreeMap<ValueId, NTy>,
    out: String,
    indent: usize,
    /// Each open if-as-value join: (marker, dst) — the decl is patched in once
    /// the first arm yield reveals the join type.
    if_stack: Vec<Option<IfJoin>>,
    /// The read-set the dead pure-Handle elision consults.
    used: std::collections::BTreeSet<ValueId>,
    /// #3532: the if-as-value joins realized as a borrow of the selected value.
    borrows: crate::borrow_join::BorrowJoins,
    addrs: crate::render_native_addr::AddrState,
    /// Interpolation chains built in one buffer (crate::render_native_interp).
    chains: crate::render_native_interp::Chains,
    printed: std::collections::BTreeSet<usize>,
    scalar_defs: std::collections::BTreeSet<usize>,
    scalar_prints: BTreeMap<usize, (ValueId, bool)>,
    /// The owned values released right after each op (`early_releases`).
    early: BTreeMap<usize, Vec<ValueId>>,
}

fn render_fn(
    func: &MirFunction,
    user_fns: &BTreeMap<&str, &MirFunction>,
    sigs: &NativeSigs,
    used_shims: &mut Vec<&'static str>,
) -> Result<(String, Option<NTy>), LowerError> {
    let tys = seed_param_ntys(func, sigs)?;
    crate::render_native_bounds::begin(func);
    let _rem_divisors = rem_divisor_scope(func);
    let is_main = func.name == "main";
    let mut out = String::new();
    let indent = 1usize;
    // Each open if-as-value join: (marker, dst) — the decl is patched in once the
    // first arm yield reveals the join type.
    let if_stack: Vec<Option<IfJoin>> = Vec::new();

    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    // Dead pure-Handle elision (rung-5 variants slab): the variant-match lower
    // still threads a `Prim{Handle}` for the heap-payload arms, but a
    // scalar-only match reads every slot through ListGetScalar and leaves the
    // handle dead. Handle is PURE (address materialization, no ownership, no
    // side effect), so skipping an unused one is sound — and it keeps the
    // subset honest: a USED Handle still walls below.
    let used = native_used_values(func);
    // #3532: the if-as-value joins realized as a borrow of the selected value.
    let borrows = crate::borrow_join::borrow_joins(func);
    let addrs = crate::render_native_addr::AddrState::new(func, sigs);
    if is_main {
        used_shims.push(almide_base::native_stack_guard::NATIVE_STACK_GUARD);
        used_shims.push(STACK_GUARD_GLUE_SHIM);
        // #1950: SIGPIPE's default disposition first, as the standard
        // codegen's `main` (the shared text, already 4-space indented).
        out.push_str(almide_codegen::main_sigpipe_prelude_text());
        line!("almide_stack_guard_install();");
    }
    if is_main && crate::charge_probe::probe_enabled() {
        used_shims.push(COUNTER_SHIM);
        used_shims.push(CHARGE_SHIM);
        line!("let __almd_probe_guard = __AlmdProbeGuard;");
        line!("let _ = &__almd_probe_guard;");
    }
    // Tail-recursion loops whose exit flag is raised as an arm's last act:
    // the render breaks there (crate::render_native_addr::flag_exits).
    let flag_exits = crate::render_native_addr::flag_exits(func);
    crate::render_native_move::set_static_strs(func);
    crate::render_native_addr::clear_str_fields();
    crate::render_native_move::set_rebound(func);
    // Interpolation chains built in one buffer (crate::render_native_interp).
    let chains = crate::render_native_interp::chains(func);
    crate::render_native_move::set_stack_lits(func, &user_fns.keys().copied().collect());
    let printed: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    let (scalar_defs, scalar_prints) = crate::render_native_interp::printed_scalars(func);
    let early = early_releases(func, &chains);
    let mut st = FnLoop {
        func,
        user_fns,
        sigs,
        used_shims,
        tys,
        out,
        indent,
        if_stack,
        used,
        borrows,
        addrs,
        chains,
        printed,
        scalar_defs,
        scalar_prints,
        early,
    };
    // The op iterator runs over the loop state's own view of the function, so
    // a phase that consumes ops ahead (`ops.nth(..)`) shares its lifetime.
    let loop_func: &MirFunction = st.func;
    let mut ops = loop_func.ops.iter().enumerate();
    while let Some((i, op)) = ops.next() {
        // An owned heap value whose last reader was the op before frees now,
        // as v0's temporaries do at their statement's end, not at the end of
        // the function its release was placed at: a long `main` would
        // otherwise hold every dead buffer to its end. A plain move, so any
        // later use the render made of it would be a compile error, never a
        // silent read of an emptied value.
        for v in st.early.get(&i).cloned().into_iter().flatten() {
            let v = &v;
            let owned = matches!(st.tys.get(v), Some(NTy::Str | NTy::VecStr | NTy::Vec | NTy::Rec(_)));
            // (A value its last call moved in is gone already.)
            let named = matches!(&func.ops[i - 1], Op::CallFn { args, .. } if args.iter().any(|a| matches!(a, crate::CallArg::Handle(x) if x == v)));
            let moved = named && st.tys.get(v).is_some_and(|t| crate::render_native_move::moves_at_call(func, i - 1, *v, *t, &st.borrows.arms));
            let gone = EARLY_DROPPED.with(|e| e.borrow().contains(v));
            if owned && !moved && !gone && st.out.contains(&format!("let mut {}:", var(*v))) {
                for _ in 0..st.indent {
                    st.out.push_str("    ");
                }
                let _ = writeln!(st.out, "drop({});", var(*v));
                EARLY_DROPPED.with(|e| e.borrow_mut().insert(*v));
            }
        }
        if render_fn_step(&mut st, i, op, &mut ops)? && flag_exits.contains(&i) {
            for _ in 0..st.indent {
                st.out.push_str("    ");
            }
            st.out.push_str("break;\n");
        }
    }

    if !st.if_stack.is_empty() {
        return Err(wall("native: unbalanced IfThen/EndIf markers"));
    }
    crate::render_native_bounds::finish(func);
    finish_native_fn(func, sigs, is_main, &mut st.tys, st.out)
}

/// One op of the [`render_fn`] loop. The phases run in order and the first
/// that renders the op ends it; `Ok(true)` = the op rendered through the
/// general op match, the only path the tail-recursion flag-exit `break`
/// follows.
fn render_fn_step<'a>(
    st: &mut FnLoop<'a>,
    i: usize,
    op: &'a Op,
    ops: &mut impl Iterator<Item = (usize, &'a Op)>,
) -> Result<bool, LowerError> {
    crate::render_native_bounds::at(i);
    crate::render_native_move::set_moving_args(st.func, i, &st.tys, &st.borrows.arms);
    if st.chains.skip.contains(&i) || st.printed.contains(&i) || st.scalar_defs.contains(&i) {
        return Ok(false);
    }
    if render_printed_scalar(st, i)? || render_chain_final(st, i)? {
        return Ok(false);
    }
    let sink = NativeSink {
        user_fns: st.user_fns,
        sigs: st.sigs,
        tys: &mut st.tys,
        out: &mut st.out,
        indent: st.indent,
        used_shims: &mut *st.used_shims,
    };
    if render_fan_group(st.func, i, &mut *ops, sink)? {
        return Ok(false);
    }
    if render_fn_appends(st, i, &mut *ops) {
        return Ok(false);
    }
    // #4006 List[String] slab: the slot-address algebra (used handles,
    // address arithmetic, header/slot loads and stores, DynList allocs).
    if crate::render_native_addr::render_addr_op(op, &st.used, &mut st.addrs, &mut st.tys, &mut st.out, st.indent)? {
        return Ok(false);
    }
    // #4006 family G: a closure call with a heap arg or result.
    if crate::render_native_closure::render_typed_call_indirect(op, st.sigs, &mut st.tys, &mut st.out, st.indent)? {
        return Ok(false);
    }
    render_fn_op(st, i, op)?;
    Ok(true)
}

/// A scalar the next op prints, printed straight from its number.
/// `Ok(false)` = `i` defines no printed scalar.
fn render_printed_scalar(st: &mut FnLoop<'_>, i: usize) -> Result<bool, LowerError> {
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..st.indent { st.out.push_str("    "); }
            writeln!(st.out, $($arg)*).unwrap();
        }};
    }
    let Some(&(n, is_bool)) = st.scalar_prints.get(&i) else { return Ok(false) };
    if st.tys.get(&n) != Some(&NTy::I64) {
        return Err(wall("native: a printed number that is not an Int"));
    }
    st.used_shims.push(crate::render_native_shims::PRINT_SHIMS);
    if is_bool {
        line!("rt_println_fmt(format_args!(\"{{}}\", if {} != 0 {{ \"true\" }} else {{ \"false\" }}));", var(n));
    } else {
        // By value: a reference would pin the local to memory.
        line!("rt_println_fmt(format_args!(\"{{}}\", {{ {} }}));", var(n));
    }
    Ok(true)
}

/// The last step of an interpolation chain: one buffer built in place, or one
/// `println!` when the next op prints it. `Ok(false)` = `i` ends no chain.
fn render_chain_final(st: &mut FnLoop<'_>, i: usize) -> Result<bool, LowerError> {
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..st.indent { st.out.push_str("    "); }
            writeln!(st.out, $($arg)*).unwrap();
        }};
    }
    let func = st.func;
    let Some((dst, pieces)) = st.chains.finals.get(&i) else { return Ok(false) };
    let dst = *dst;
    // Printed by the next op and read by nothing else: one println!,
    // the macro `rt_print_str` prints through (a fan program's glue
    // routes that same macro).
    if let (Some(j), Some(code)) = (
        crate::render_native_interp::printed_by(func, i, dst),
        crate::render_native_interp::render_print(pieces, &st.tys),
    ) {
        st.printed.insert(j);
        st.tys.insert(dst, NTy::Str);
        st.used_shims.push(crate::render_native_shims::PRINT_SHIMS);
        line!("{code}");
        return Ok(true);
    }
    // A chain whose head string dies here grows that string's own
    // buffer, as v0's `AlmideConcat` consumes its owned left side.
    if let Some(crate::render_native_interp::Piece::Str(head)) = pieces.first() {
        let dies = chain_end(func, &st.chains, i).and_then(|e| st.early.get(&(e + 1))).is_some_and(|vs| vs.contains(head))
            && st.tys.get(head) == Some(&NTy::Str)
            && st.out.contains(&format!("let mut {}:", var(*head)))
            && !pieces[1..].iter().any(|p| matches!(p, crate::render_native_interp::Piece::Str(x) if x == head));
        if let Some(rest) = dies.then(|| crate::render_native_interp::push_pieces(&pieces[1..], &st.tys)).flatten() {
            let head = *head;
            st.tys.insert(dst, NTy::Str);
            line!("let mut {}: AlmideString = {{ let mut __head = {}; {rest} __head }};", var(dst), var(head));
            EARLY_DROPPED.with(|e| e.borrow_mut().insert(head));
            return Ok(true);
        }
    }
    let code = crate::render_native_interp::render_final(dst, pieces, &st.tys)?;
    st.tys.insert(dst, NTy::Str);
    line!("{code}");
    Ok(true)
}

/// The in-place growth shapes: a string or list appended to itself, a push,
/// and a one-element string list held as its element. `true` = the op (and
/// the ops it consumed ahead) is rendered.
fn render_fn_appends(st: &mut FnLoop<'_>, i: usize, ops: &mut impl Iterator) -> bool {
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..st.indent { st.out.push_str("    "); }
            writeln!(st.out, $($arg)*).unwrap();
        }};
    }
    let func = st.func;
    // #3639: `s = s + x` / `string.push(s, x)` grows `s` in place.
    if let Some(append) = crate::render_native_append::str_self_append(func, i, &st.tys) {
        line!("{append}");
        ops.nth(1);
        return true;
    }
    if let Some(push) = crate::render_native_append::list_push(func, i, &st.tys) {
        line!("{push}");
        ops.nth(2);
        return true;
    }
    // A one-element string list held as its element: pushed whole, or
    // built here for the append below.
    if let Some((code, pushed)) = crate::render_native_addr::single_push(func, i, &st.tys, &mut st.addrs) {
        line!("{code}");
        if pushed {
            ops.nth(1);
            return true;
        }
    }
    if let Some((code, done)) = crate::render_native_addr::single_concat(func, i, &mut st.tys, &mut st.addrs) {
        line!("{code}");
        if done {
            return true;
        }
        st.tys.entry(func.ops.get(i).and_then(|op| match op {
            Op::CallFn { args, .. } => match args.get(1) {
                Some(crate::CallArg::Handle(x)) => Some(*x),
                _ => None,
            },
            _ => None,
        }).expect("a list `+`")).or_insert(NTy::VecStr);
    }
    if let Some(append) = crate::render_native_append::list_self_append(func, i, &st.tys, &st.borrows.arms) {
        line!("{append}");
        ops.nth(1);
        return true;
    }
    false
}

/// The general op match of the [`render_fn`] loop: the moves first, then the
/// fn references and calls, then every op tier in turn.
fn render_fn_op(st: &mut FnLoop<'_>, i: usize, op: &Op) -> Result<(), LowerError> {
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..st.indent { st.out.push_str("    "); }
            writeln!(st.out, $($arg)*).unwrap();
        }};
    }
    if render_fn_move_op(st, i, op)? {
        return Ok(());
    }
    match op {
        // Rung-5 closures slab: a FuncRef is the lambda's DISPATCH-TABLE index
        // (the name-sorted position shared with the `__almd_ci_*` tables).
        Op::FuncRef { dst, name } => {
            let idx = lambda_index(st.user_fns, name)
                .ok_or_else(|| wall(format!("native: FuncRef to unknown lambda `{name}`")))?;
            st.tys.insert(*dst, NTy::I64);
            line!("let mut {}: i64 = {idx}; // fn table: {name}", var(*dst));
        }
        Op::CallFn {
            dst,
            name,
            args,
            result,
        } => render_call_fn(
            crate::render_native::NativeCall {
                dst,
                name,
                args,
                result,
            },
            crate::render_native::NativeSink {
                user_fns: st.user_fns,
                sigs: st.sigs,
                tys: &mut st.tys,
                out: &mut st.out,
                indent: st.indent,
                used_shims: &mut *st.used_shims,
            },
        )?,
        other => render_fn_tier_op(st, other)?,
    }
    Ok(())
}

/// The move-shaped arms of the general op match: a dead handle, a borrow-join
/// arm, and the copies, error carriers and reassignments that move their
/// source instead of cloning it. `Ok(false)` = none of them.
fn render_fn_move_op(st: &mut FnLoop<'_>, i: usize, op: &Op) -> Result<bool, LowerError> {
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..st.indent { st.out.push_str("    "); }
            writeln!(st.out, $($arg)*).unwrap();
        }};
    }
    let func = st.func;
    match op {
        Op::Prim {
            kind: crate::PrimKind::Handle,
            dst: Some(d),
            ..
        } if !st.used.contains(d) => {
            line!("// dead handle elided");
        }
        Op::Dup { dst, src } if st.borrows.arms.contains(dst) => {
            crate::borrow_join::render_arm(dst, src, &mut st.tys, &mut st.out, st.indent)?
        }
        // A copy of a borrowed field read off an owned record that dies
        // here moves the field out (#4006, no-perf-loss): nothing reads
        // the record again, so its field is the copy.
        Op::Dup { dst, src }
            if matches!(st.tys.get(src), Some(NTy::VecRef | NTy::StrRef | NTy::VecStrRef))
                && crate::render_native_addr::field_of(*src).is_some_and(|(base, place)| {
                    matches!(st.tys.get(&base), Some(NTy::Rec(_)))
                        && crate::render_native_move::dying_after(func, i, base, &st.borrows.arms)
                        && crate::render_native_addr::reads_of_place(&place)
                            .iter()
                            .all(|r| crate::render_native_move::dying_after(func, i, *r, &st.borrows.arms))
                }) =>
        {
            let (_, place) = crate::render_native_addr::field_of(*src).expect("a field read");
            let owned = match st.tys[src] {
                NTy::StrRef => NTy::Str,
                NTy::VecRef => NTy::Vec,
                _ => NTy::VecStr,
            };
            st.tys.insert(*dst, owned);
            line!("let mut {} = std::mem::take(&mut {place});", var(*dst));
        }
        // A copy of a value nothing reads again moves it (no clone).
        Op::Dup { dst, src }
            if st
                .tys
                .get(src)
                .is_some_and(|t| crate::render_native_move::last_use(func, i, *src, *t, &st.borrows.arms)) =>
        {
            let t = st.tys[src];
            st.tys.insert(*dst, t);
            line!("let mut {} = {};", var(*dst), crate::render_native_move::take_expr(&var(*src)));
        }
        // An error carrier over an owned string nothing reads again takes it.
        Op::Prim { kind: crate::PrimKind::ResMakeErrStr, dst: Some(d), args }
            if args.len() == 1
                && st.tys.get(&args[0]) == Some(&NTy::Str)
                && crate::render_native_move::last_use(func, i, args[0], NTy::Str, &st.borrows.arms) =>
        {
            st.tys.insert(*d, NTy::Res);
            line!("let {}: Result<i64, AlmideString> = Err({});", var(*d), crate::render_native_move::take_expr(&var(args[0])));
        }
        Op::SetLocal { local, src }
            if st.tys.get(src).is_some_and(|t| {
                st.tys.get(local).is_none_or(|l| l == t)
                    && crate::render_native_move::last_use(func, i, *src, *t, &st.borrows.arms)
            }) =>
        {
            let t = st.tys[src];
            if st.tys.contains_key(local) {
                line!("{} = {};", var(*local), crate::render_native_move::take_expr(&var(*src)));
            } else {
                st.tys.insert(*local, t);
                line!("let mut {} = {};", var(*local), crate::render_native_move::take_expr(&var(*src)));
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// Every op tier in turn — call-shaped, meter, Result carrier, termination,
/// scalar, control flow; an op none of them takes is outside the rung subset.
fn render_fn_tier_op(st: &mut FnLoop<'_>, other: &Op) -> Result<(), LowerError> {
    let handled = render_native_call_op(
        other,
        OpSink {
            tys: &mut st.tys,
            out: &mut st.out,
            indent: st.indent,
            used_shims: &mut *st.used_shims,
        },
    )? || render_native_meter_op(
        other,
        OpSink {
            tys: &mut st.tys,
            out: &mut st.out,
            indent: st.indent,
            used_shims: &mut *st.used_shims,
        },
    )? || render_native_result_op(
        other,
        OpSink {
            tys: &mut st.tys,
            out: &mut st.out,
            indent: st.indent,
            used_shims: &mut *st.used_shims,
        },
    )? || render_native_termination_op(
        other,
        OpSink {
            tys: &mut st.tys,
            out: &mut st.out,
            indent: st.indent,
            used_shims: &mut *st.used_shims,
        },
    )? || render_native_scalar_op(
        other,
        OpSink {
            tys: &mut st.tys,
            out: &mut st.out,
            indent: st.indent,
            used_shims: &mut *st.used_shims,
        },
    )? || render_native_flow_op(
        other,
        &st.borrows.joins,
        &mut st.tys,
        &mut st.out,
        &mut st.indent,
        &mut st.if_stack,
    )?;
    if !handled {
        let detail = if let Op::Prim { kind, .. } = other {
            format!("Prim {kind:?}")
        } else {
            op_name(other)
        };
        return Err(wall(format!(
            "native: op {detail:?} in `{}` — outside the rung subset",
            st.func.name
        )));
    }
    Ok(())
}
