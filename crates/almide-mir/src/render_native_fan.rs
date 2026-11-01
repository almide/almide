
// `fan { … }` arms on scoped threads (#4006 family H; `include!`d by
// render_native.rs, sharing its scope and imports).
//
// crate::fan_native lifts each arm of a fan block into a function of its own;
// the MIR calls them in arm order, back to back, and each is certified like
// any other function. Here the K calls of one group render as the standard
// codegen's threaded form (crates/almide-codegen/templates/rust.toml
// `fan_expr`): a fan group opened after a stdout flush, every arm spawned
// through `almide_fan_spawn` into its own output timeline, the results taken
// in arm order. The runtime is the shipped prelude_fan text, verbatim
// (`almide_codegen::fan_timeline_prelude_text`), with the glue this render's
// header needs ([`fan_glue`]). The TRUSTED step — that running the arms
// concurrently is observation-equal to the sequential MIR the certificate
// judges — is proofs/TRUSTED_BASE.md's "fan arms on threads" row.

thread_local! {
    static FAN_RENDERED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Did the program render a threaded fan group (so it needs [`fan_glue`])?
pub(crate) fn fan_rendered() -> bool {
    FAN_RENDERED.with(|c| c.get())
}

pub(crate) fn fan_rendered_reset() {
    FAN_RENDERED.with(|c| c.set(false));
}

/// One rendered arm call `let mut vN: T = EXPR;`: `(vN, T, EXPR)`.
fn split_arm_line(line: &str) -> Option<(String, String, String)> {
    let rest = line.trim().strip_prefix("let mut ")?.strip_suffix(';')?;
    let (lhs, expr) = rest.split_once(" = ")?;
    let (v, t) = lhs.split_once(": ")?;
    Some((v.to_string(), t.to_string(), expr.to_string()))
}

/// At op `i`, the first call of a lifted fan group: render the whole group
/// (its K calls) as the threaded form and consume its ops. `false` when op
/// `i` is no group's first arm.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_fan_group<'f>(
    func: &'f MirFunction,
    i: usize,
    ops: &mut impl Iterator<Item = (usize, &'f Op)>,
    user_fns: &BTreeMap<&str, &MirFunction>,
    sigs: &NativeSigs,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
    used_shims: &mut Vec<&'static str>,
) -> Result<bool, LowerError> {
    let Op::CallFn { name, .. } = &func.ops[i] else { return Ok(false) };
    let Some(arm) = crate::fan_native::arm(name) else { return Ok(false) };
    if arm.index != 0 {
        return Err(wall(format!("native: fan arm `{name}` reached before its group's first arm")));
    }
    let k = arm.count;
    let mut handles = Vec::new();
    let mut spawns = String::new();
    let mut marks = String::new();
    for j in 0..k {
        let at = i + j;
        let want = crate::fan_native::arm_name(arm.group, j);
        let Some(Op::CallFn { dst, name, args, result }) = func.ops.get(at) else {
            return Err(wall(format!("native: fan group {} is not {k} consecutive arm calls", arm.group)));
        };
        if *name != want {
            return Err(wall(format!("native: fan group {} is not {k} consecutive arm calls", arm.group)));
        }
        for a in args {
            if let CallArg::Handle(v) = a {
                // ADR-0013 design point 3: a value an arm reads on another
                // thread is marked thread-shared before the spawn.
                writeln!(marks, "almide_str_block::almide_str_mark_reachable(&{});", var(*v)).unwrap();
            }
        }
        let mut line = String::new();
        render_call_fn(
            NativeCall { dst, name, args, result },
            NativeSink { user_fns, sigs, tys, out: &mut line, indent: 0, used_shims },
        )?;
        let Some((v, t, expr)) = split_arm_line(&line) else {
            return Err(wall(format!("native: fan arm `{name}` renders as more than one bound call")));
        };
        writeln!(
            spawns,
            "let __almide_fan_h{j} = almide_fan_spawn(__almide_s, || {{ let __almide_fan_e = almide_fan_enter(__almide_fan_g, {j}); {expr} }});"
        )
        .unwrap();
        handles.push((v, t));
    }
    // Consume the group's ops after the first.
    for _ in 1..k {
        ops.next();
    }
    let pad = "    ".repeat(indent);
    let vars: Vec<String> = handles.iter().map(|(v, _)| format!("mut {v}")).collect();
    let types: Vec<&str> = handles.iter().map(|(_, t)| t.as_str()).collect();
    let takes: Vec<String> = (0..k).map(|j| format!("almide_fan_take(__almide_fan_h{j})")).collect();
    for l in marks.lines() {
        writeln!(out, "{pad}{l}").unwrap();
    }
    writeln!(out, "{pad}almide_stdout_flush();").unwrap();
    writeln!(out, "{pad}let __almide_fan_go = almide_fan_group({k}); let __almide_fan_g = &__almide_fan_go;").unwrap();
    writeln!(out, "{pad}let ({}): ({}) = std::thread::scope(|__almide_s| {{", vars.join(", "), types.join(", ")).unwrap();
    for l in spawns.lines() {
        writeln!(out, "{pad}    {l}").unwrap();
    }
    writeln!(out, "{pad}    ({})", takes.join(", ")).unwrap();
    writeln!(out, "{pad}}});").unwrap();
    FAN_RENDERED.with(|c| c.set(true));
    Ok(true)
}

/// The fan runtime as this render's header needs it: the shipped prelude_fan
/// text, the stdout handle and stack-guard hooks it calls, the output macros
/// routed through the arm timelines, and the trap-waiting exit every abort
/// path takes ([`route_exits`]).
pub(crate) fn fan_glue() -> String {
    let mut s = String::from(
        "// #4006 family H: the fan runtime (crates/almide-codegen/src/prelude_fan.rs, verbatim)\n\
         // and the glue this render's header needs.\n\
         use std::process::exit as almide_process_exit;\n\
         thread_local! { static ALMIDE_STDOUT_BUF: std::cell::RefCell<std::io::Stdout> = std::cell::RefCell::new(std::io::stdout()); }\n\
         fn almide_stdout_flush() { let _ = std::io::Write::flush(&mut std::io::stdout()); }\n\
         fn almide_stdout_is_terminal() -> bool { std::io::IsTerminal::is_terminal(&std::io::stdout()) }\n\
         fn almide_stack_guard_register() {\n    #[cfg(all(any(target_os = \"linux\", target_os = \"macos\"), not(miri)))]\n    almide_stack_guard::register();\n}\n\
         fn almide_v1_out(err: bool, args: std::fmt::Arguments<'_>, newline: bool) {\n\
         \x20   if almide_fan_active() { let mut v: Vec<u8> = Vec::new(); let _ = std::io::Write::write_fmt(&mut v, args); if newline { v.push(b'\\n'); } return almide_out_write(err, &v); }\n\
         \x20   if err { let mut e = std::io::stderr().lock(); let _ = std::io::Write::write_fmt(&mut e, args); if newline { let _ = std::io::Write::write_all(&mut e, b\"\\n\"); } }\n\
         \x20   else { let mut o = std::io::stdout().lock(); let _ = std::io::Write::write_fmt(&mut o, args); if newline { let _ = std::io::Write::write_all(&mut o, b\"\\n\"); } }\n\
         }\n\
         macro_rules! println { () => { almide_v1_out(false, format_args!(\"\"), true) }; ($($a:tt)*) => { almide_v1_out(false, format_args!($($a)*), true) }; }\n\
         macro_rules! print { ($($a:tt)*) => { almide_v1_out(false, format_args!($($a)*), false) }; }\n\
         macro_rules! eprintln { () => { almide_v1_out(true, format_args!(\"\"), true) }; ($($a:tt)*) => { almide_v1_out(true, format_args!($($a)*), true) }; }\n\
         macro_rules! eprint { ($($a:tt)*) => { almide_v1_out(true, format_args!($($a)*), false) }; }\n\
         fn almide_v1_exit(code: i32) -> ! { almide_fan_trap_wait(); almide_stdout_flush(); almide_process_exit(code) }\n",
    );
    s.push_str(&almide_codegen::fan_timeline_prelude_text());
    s.push('\n');
    s
}

/// Every abort path of the rendered program exits through `almide_v1_exit`,
/// which first waits for the arms below the trapping one (ADR-0024 D5).
pub(crate) fn route_exits(program: &str) -> String {
    program.replace("std::process::exit(", "almide_v1_exit(")
}

/// At op `i`, a call of a self-host body the lift admitted a parallel twin
/// for (crate::fan_map_twin): render it as the standard codegen's
/// `almide_rt_fan_map_par` over the list, each element calling the certified
/// lambda through its typed dispatch table. `false` when op `i` is no such
/// call.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_fan_map_twin(
    func: &MirFunction,
    i: usize,
    user_fns: &BTreeMap<&str, &MirFunction>,
    sigs: &NativeSigs,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
    used_shims: &mut Vec<&'static str>,
) -> Result<bool, LowerError> {
    let Op::CallFn { dst, name, args, result } = &func.ops[i] else { return Ok(false) };
    if !crate::fan_map_twin::is_twin(name) {
        return Ok(false);
    }
    let twin_wall = |why: &str| wall(format!("native: the parallel twin of `{name}`: {why}"));
    let [CallArg::Handle(xs), CallArg::Handle(blk)] = args.as_slice() else {
        return Err(twin_wall("not a call of a list and a closure block"));
    };
    let (elem, elem_nty, xs_slice) = match tys.get(xs) {
        Some(NTy::Vec) => ("i64", NTy::I64, format!("&{}[..]", var(*xs))),
        Some(NTy::VecRef) => ("i64", NTy::I64, var(*xs)),
        Some(NTy::VecStr) => ("AlmideString", NTy::Str, format!("&{}[..]", var(*xs))),
        other => return Err(twin_wall(&format!("a list held as {other:?}"))),
    };
    if !matches!(tys.get(blk), Some(NTy::Vec | NTy::VecRef)) {
        return Err(twin_wall("the closure is not a block"));
    }
    // The bound result, as the sequential call would bind it.
    let mut line = String::new();
    render_call_fn(
        NativeCall { dst, name, args, result },
        NativeSink { user_fns, sigs, tys, out: &mut line, indent: 0, used_shims },
    )?;
    let Some((v, t, _)) = split_arm_line(&line) else {
        return Err(twin_wall("the call renders as more than one bound value"));
    };
    // One element's call: the dispatch the self-host body's `CallIndirect`
    // makes, rendered over stand-in values and then spelled over the element.
    let (x, idx, d) = (ValueId(u32::MAX - 2), ValueId(u32::MAX - 1), ValueId(u32::MAX));
    tys.insert(x, elem_nty);
    tys.insert(idx, NTy::I64);
    let x_arg = if elem_nty == NTy::I64 { CallArg::Scalar(x) } else { CallArg::Handle(x) };
    let mut call = String::new();
    let typed = crate::render_native_closure::render_typed_call_indirect(
        &Some(d),
        &idx,
        &[CallArg::Handle(*blk), x_arg],
        &Some(Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }),
        sigs,
        tys,
        &mut call,
        0,
    );
    tys.remove(&x);
    tys.remove(&idx);
    tys.remove(&d);
    if !typed? {
        return Err(twin_wall("no typed dispatch table serves its lambda"));
    }
    let Some((_, _, each)) = split_arm_line(&call) else {
        return Err(twin_wall("the element call renders as more than one bound value"));
    };
    let each = each.replace(&var(x), "__almide_x").replace(&var(idx), &format!("{}[0]", var(*blk)));
    let pad = "    ".repeat(indent);
    writeln!(out, "{pad}let mut {v}: {t} = almide_v1_fan_map({xs_slice}, |__almide_x: {elem}| {each});").unwrap();
    FAN_RENDERED.with(|c| c.set(true));
    FAN_MAP_RENDERED.with(|c| c.set(true));
    Ok(true)
}

thread_local! {
    static FAN_MAP_RENDERED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Did the program render a parallel `fan.map` twin (so it needs
/// [`fan_map_glue`])?
pub(crate) fn fan_map_rendered() -> bool {
    FAN_MAP_RENDERED.with(|c| c.get())
}

pub(crate) fn fan_map_rendered_reset() {
    FAN_MAP_RENDERED.with(|c| c.set(false));
}

/// The parallel `fan.map` runtime (verbatim: `almide_rt_fan_map_par` and the
/// list module's dealer, `almide_codegen::fan_map_parallel_runtime_text`) and
/// the one call this render makes of it.
pub(crate) fn fan_map_glue() -> Result<String, LowerError> {
    let runtime = almide_codegen::fan_map_parallel_runtime_text()
        .ok_or_else(|| wall("native: the parallel fan.map runtime text has moved from its markers"))?;
    Ok(format!(
        "// #4006 family H: the parallel fan.map runtime (runtime/rs/src/fan.rs + list.rs, verbatim).\n\
         use almide_str_block::almide_str_mark_reachable;\n\
         {runtime}\n\
         fn almide_v1_fan_map<A: Send + Sync + Clone, B: Send>(xs: &[A], f: impl Fn(A) -> Result<B, AlmideString> + Send + Sync) -> Result<Vec<B>, AlmideString> {{\n\
         \x20   almide_rt_fan_map_par(xs.to_vec(), f)\n\
         }}\n"
    ))
}
