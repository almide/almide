//! The embedded host's `almide:process/spawn` (#2589, ADR-0025): the private
//! subprocess capability, served with the SAME core native runs
//! (`almide_rt_core::process_core`, which runtime/rs/src/process.rs includes
//! verbatim), so the captured text, the exit code and every err string agree
//! with native by construction.
//!
//! One operation = `(op, a, b) -> ok(text) | err(text)`, op 80..=90 and the
//! `start` child's 140..=145 in the WIT `enum op` order
//! (wit/process/spawn.wit). It reaches the host two ways:
//!
//! - the raw module's `almide.fs_call` (the embedded lane — `almide run
//!   --target wasm`, `almide test`), like every other embedded service;
//! - the `almide:process/spawn.call` import itself (the canonical-ABI
//!   lowering of the WIT function), which a stock artifact carries and a
//!   stock runtime refuses at load ([`link_spawn_import`]).
//!
//! Operand frames are stdlib/process_wasm.almd's decimal CHAR-length cells.
//! The `[permissions] proc` allowlist ([`set_allowlist`]) bounds every
//! operation that starts a child: a command outside it is answered with an
//! err naming the command, before anything runs.

use std::sync::Mutex;

/// The first and last process op (`almide:process/spawn`'s `enum op`).
pub(crate) const OP_FIRST: i32 = 80;
pub(crate) const OP_LAST: i32 = 90;
/// The `process.start` child's ops (#4308): the WIT cases after `run-with`.
pub(crate) const CHILD_FIRST: i32 = 140;
pub(crate) const CHILD_LAST: i32 = 145;
const _: () = assert!(
    *crate::wasi::PROC_OPS.start() == OP_FIRST
        && *crate::wasi::PROC_OPS.end() == OP_LAST
        && *crate::wasi::PROC_CHILD_OPS.start() == CHILD_FIRST
        && *crate::wasi::PROC_CHILD_OPS.end() == CHILD_LAST
);

/// Is `op` one of `almide:process/spawn`'s operations?
pub(crate) fn is_op(op: i32) -> bool {
    crate::wasi::is_proc_op(op)
}

/// The children `process.start` made on one run, by handle (their slot) —
/// native's `almide_proc_handles` table, held per run so a run's children
/// are its own (and stopped at its end, C-437's rule).
pub(crate) type ProcTable = Mutex<Vec<Option<almide_rt_core::process_core::AlmideProcChild>>>;

/// The table of the canonical import's children ([`link_spawn_import`]): a
/// stand-alone artifact's host keeps one table for the process.
fn import_table() -> &'static ProcTable {
    static T: ProcTable = Mutex::new(Vec::new());
    &T
}

/// Run `f` on the child behind handle `h` (native's `almide_proc_with`).
fn with_child<R>(procs: &ProcTable, h: &str, f: impl FnOnce(&mut almide_rt_core::process_core::AlmideProcChild) -> R) -> Result<R, String> {
    let mut t = procs.lock().unwrap_or_else(|p| p.into_inner());
    let c = h
        .parse::<usize>()
        .ok()
        .and_then(|i| t.get_mut(i))
        .and_then(|c| c.as_mut())
        .ok_or_else(|| format!("invalid ProcessChild handle: {h}"))?;
    Ok(f(c))
}

/// At the run's end: stop every child neither waited on nor stopped, the way
/// native's exit does (`almide_proc_stop_all_unwaited`, C-437). A child whose
/// outcome is in is left alone.
pub(crate) fn stop_unwaited(procs: &ProcTable) {
    use almide_rt_core::process_core as core;
    let mut t = procs.lock().unwrap_or_else(|p| p.into_inner());
    for c in t.iter_mut().flatten() {
        if c.step(core::almide_proc_try_wait_poll).is_none() {
            let _ = c.stop(core::ALMIDE_PROC_EXIT_GRACE_MS);
        }
    }
}

/// `None` = every command is allowed (no `proc` key in `[permissions]`).
static ALLOW: Mutex<Option<Vec<String>>> = Mutex::new(None);

/// Bound the commands this host may start (`[permissions] proc`, the
/// project's manifest); `None` lifts the bound. Process-wide: one project per
/// `almide` invocation.
pub fn set_allowlist(list: Option<Vec<String>>) {
    *ALLOW.lock().unwrap_or_else(|p| p.into_inner()) = list;
}

fn allowed(call: &'static str, cmd: &str) -> Result<(), String> {
    match &*ALLOW.lock().unwrap_or_else(|p| p.into_inner()) {
        Some(list) if !list.iter().any(|c| c == cmd) => {
            let shown = almide_base::err_text::almide_rt_errtext_escape(cmd);
            Err(format!("{call}(\"{shown}\"): `{shown}` is not in [permissions] proc"))
        }
        _ => Ok(()),
    }
}

/// The decimal CHAR-length cells `<n>\n<payload>…` (process_wasm.almd's
/// `__proc_argv`).
fn cells(frame: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut rest = frame;
    while !rest.is_empty() {
        let nl = rest.find('\n').ok_or("malformed process frame (missing length)")?;
        let n: usize = rest[..nl].parse().map_err(|_| "malformed process frame (bad length)")?;
        let tail = &rest[nl + 1..];
        let end = tail.char_indices().nth(n).map_or(tail.len(), |(i, _)| i);
        if end == tail.len() && tail.chars().count() < n {
            return Err("malformed process frame (short cell)".to_string());
        }
        out.push(tail[..end].to_string());
        rest = &tail[end..];
    }
    Ok(out)
}

/// `exec_status`'s answer: `<code>\n<stdout chars>\n<stdout><stderr>`.
fn status_text((code, stdout, stderr): (i64, String, String)) -> String {
    format!("{code}\n{}\n{stdout}{stderr}", stdout.chars().count())
}

/// Split a frame whose FIRST cell is an extra operand (exec_in's / run_in's command,
/// exec_with_stdin's input, exec_status_timeout's bound) from the arguments.
fn head_and_args(b: &str) -> Result<(String, Vec<String>), String> {
    let mut all = cells(b)?;
    if all.is_empty() {
        return Err("malformed process frame (missing operand)".to_string());
    }
    let head = all.remove(0);
    Ok((head, all))
}

/// The core's signal syscall (#4125): kill(2) through rustix (safe), never a
/// `kill` looked up on PATH. The core has already refused pid <= 0 and a
/// signal outside 0..=31.
#[cfg(unix)]
fn signal(pid: i32, sig: i32) -> Result<(), i32> {
    let Some(pid) = rustix::process::Pid::from_raw(pid) else { return Err(3) };
    let r = if sig == 0 {
        rustix::process::test_kill_process(pid)
    } else {
        // from_named_raw answers None for a number that is not a named signal.
        match rustix::process::Signal::from_named_raw(sig) {
            Some(s) => rustix::process::kill_process(pid, s),
            None => return Err(22),
        }
    };
    r.map_err(|e| e.raw_os_error())
}
#[cfg(windows)]
fn signal(pid: i32, sig: i32) -> Result<(), i32> {
    almide_rt_core::process_core::almide_proc_signal_windows(pid, sig)
}

/// Serve one operation. `flush` runs before a child that shares this
/// program's stdout starts (run / run_in), as native flushes its own.
pub(crate) fn call(op: i32, a: &str, b: &str, flush: &dyn Fn(), procs: &ProcTable) -> Result<String, String> {
    use almide_rt_core::process_core as core;
    match op {
        80 => {
            allowed("process.exec", a)?;
            core::almide_proc_exec(a, &cells(b)?)
        }
        81 => {
            let (cmd, args) = head_and_args(b)?;
            allowed("process.exec_in", &cmd)?;
            core::almide_proc_exec_in(a, &cmd, &args)
        }
        82 => {
            let (input, args) = head_and_args(b)?;
            allowed("process.exec_with_stdin", a)?;
            core::almide_proc_exec_with_stdin(a, &args, &input)
        }
        83 => {
            allowed("process.exec_status", a)?;
            core::almide_proc_exec_status(a, &cells(b)?).map(status_text)
        }
        84 => {
            let mut all = cells(b)?;
            if all.len() < 3 {
                return Err("malformed process frame (missing operand)".to_string());
            }
            let rest = all.split_off(3);
            let ms: i64 = all[0].parse().map_err(|_| "malformed process frame (bad timeout)".to_string())?;
            allowed("process.output", a)?;
            core::almide_proc_output(a, &rest, &all[1], &all[2], ms, core::almide_proc_try_wait_poll).map(status_text)
        }
        85 => {
            allowed("process.run", a)?;
            flush();
            core::almide_proc_run(a, &cells(b)?).map(|c| c.to_string())
        }
        87 => {
            // #4125: signalling is bounded by `[permissions] proc` as the
            // `kill` command it used to run (src/cli/mod.rs PROC_SIGNALLING).
            allowed("process.kill", "kill")?;
            let num = |s: &str| s.parse::<i64>().map_err(|_| format!("malformed process operand `{}`", almide_base::err_text::almide_rt_errtext_escape(s)));
            core::almide_proc_kill(num(a)?, num(b)?, signal).map(|()| String::new())
        }
        // 86 (spawn) and 88 (is_alive) served the surface process.start
        // replaced (R-process-03); their numbers stay unused.
        89 => Ok(std::process::id().to_string()),
        90 => {
            let mut all = cells(b)?;
            if all.len() < 3 {
                return Err("malformed process frame (missing operand)".to_string());
            }
            let rest = all.split_off(3);
            let ms: i64 = all[2].parse().map_err(|_| "malformed process frame (bad timeout)".to_string())?;
            allowed("process.run", &all[0])?;
            flush();
            core::almide_proc_run_cmd(&all[0], &rest, a, &all[1], ms).map(|c| c.to_string())
        }
        // The `start` child (#4308): native's child core on this run's table.
        140 => {
            let mut all = cells(b)?;
            if all.len() < 3 {
                return Err("malformed process frame (missing operand)".to_string());
            }
            let rest = all.split_off(3);
            let ms: i64 = all[0].parse().map_err(|_| "malformed process frame (bad timeout)".to_string())?;
            allowed("process.start", a)?;
            flush();
            let c = core::AlmideProcChild::start("process.start", a, &rest, &all[1], &all[2], ms)?;
            let mut t = procs.lock().unwrap_or_else(|p| p.into_inner());
            t.push(Some(c));
            Ok((t.len() - 1).to_string())
        }
        141 => with_child(procs, a, |c| if c.step(core::almide_proc_try_wait_poll).is_some() { "1" } else { "0" }.to_string()),
        142 => with_child(procs, a, |c| c.wait(core::almide_proc_try_wait_poll))?.map(status_text),
        143 => {
            let grace: i64 = b.parse().map_err(|_| format!("malformed process operand `{}`", almide_base::err_text::almide_rt_errtext_escape(b)))?;
            with_child(procs, a, |c| c.stop(grace))?.map(status_text)
        }
        144 => Ok(with_child(procs, a, |c| c.read_new(b == "2")).unwrap_or_default()),
        145 => Ok(with_child(procs, a, |c| c.pid()).unwrap_or(-1).to_string()),
        _ => Err(format!("unknown process op {op}")),
    }
}

/// The `fs_call` form: status 0 = ok, 1 = err, the text parked for
/// `host_read`.
pub(crate) fn dispatch(op: i32, a: &str, b: &[u8], flush: &dyn Fn(), procs: &ProcTable) -> (i64, Vec<u8>) {
    let (status, text) = match call(op, a, &String::from_utf8_lossy(b), flush, procs) {
        Ok(t) => (0i64, t),
        Err(m) => (1i64, m),
    };
    ((status << 32) | (text.len() as i64 & 0xFFFF_FFFF), text.into_bytes())
}

/// Define `almide:process/spawn.call` on a core-module linker: the
/// canonical-ABI lowering of `call: func(op: op, a: string, b: string) ->
/// result<string, string>` — `(op, a_ptr, a_len, b_ptr, b_len, retptr)`,
/// the answer's bytes placed with the guest's `cabi_realloc` export, the
/// discriminant at `retptr` and `(ptr, len)` at `retptr + 4`.
pub fn link_spawn_import<T: 'static>(linker: &mut wasmtime::Linker<T>) -> anyhow::Result<()> {
    let ty = wasmtime::FuncType::new(linker.engine(), std::iter::repeat_n(wasmtime::ValType::I32, 6), []);
    linker.func_new("almide:process/spawn", "call", ty, |mut caller, params, _results| {
        let mut args = [0i32; 6];
        for (slot, v) in args.iter_mut().zip(params) {
            *slot = v.unwrap_i32();
        }
        spawn_call(&mut caller, args)
    })?;
    Ok(())
}

/// One canonical-ABI `call`: `[op, a_ptr, a_len, b_ptr, b_len, retptr]`.
fn spawn_call<T>(caller: &mut wasmtime::Caller<'_, T>, [op, a_ptr, a_len, b_ptr, b_len, ret]: [i32; 6]) -> wasmtime::Result<()> {
    let mem = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| wasmtime::Error::msg("almide:process/spawn: the guest exports no memory"))?;
    let read = |caller: &wasmtime::Caller<'_, T>, ptr: i32, len: i32| -> wasmtime::Result<String> {
        // Bounds-checked before anything is allocated (#4135): a forged
        // length traps instead of reserving up to 4 GiB.
        let buf = crate::host::read_guest(caller, mem, ptr, len)?;
        String::from_utf8(buf).map_err(|_| wasmtime::Error::msg("almide:process/spawn: operand is not UTF-8"))
    };
    let a = read(caller, a_ptr, a_len)?;
    let b = read(caller, b_ptr, b_len)?;
    let (disc, text) = match call(crate::wasi::proc_op_of_case(op), &a, &b, &|| {}, import_table()) {
        Ok(t) => (0u8, t),
        Err(m) => (1u8, m),
    };
    let realloc = caller
        .get_export("cabi_realloc")
        .and_then(|e| e.into_func())
        .ok_or_else(|| wasmtime::Error::msg("almide:process/spawn: the guest exports no cabi_realloc"))?
        .typed::<(i32, i32, i32, i32), i32>(&*caller)?;
    let len = i32::try_from(text.len()).map_err(|_| wasmtime::Error::msg("almide:process/spawn: answer too large"))?;
    let ptr = realloc.call(&mut *caller, (0, 0, 1, len))?;
    mem.write(&mut *caller, ptr as u32 as usize, text.as_bytes())?;
    let mut cell = [0u8; 12];
    cell[0] = disc;
    cell[4..8].copy_from_slice(&ptr.to_le_bytes());
    cell[8..12].copy_from_slice(&len.to_le_bytes());
    mem.write(&mut *caller, ret as u32 as usize, &cell)?;
    Ok(())
}

#[cfg(test)]
#[path = "tests/host_process_test.rs"]
mod tests;
