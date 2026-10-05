//! The embedded host's `almide:process/spawn` (#2589, ADR-0025): the private
//! subprocess capability, served with the SAME core native runs
//! (`almide_rt_core::process_core`, which runtime/rs/src/process.rs includes
//! verbatim), so the captured text, the exit code and every err string agree
//! with native by construction.
//!
//! One operation = `(op, a, b) -> ok(text) | err(text)`, op 80..=90 in the
//! WIT `enum op` order (wit/process/spawn.wit). It reaches the host two ways:
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

/// `None` = every command is allowed (no `proc` key in `[permissions]`).
static ALLOW: Mutex<Option<Vec<String>>> = Mutex::new(None);

/// Bound the commands this host may start (`[permissions] proc`, the
/// project's manifest); `None` lifts the bound. Process-wide: one project per
/// `almide` invocation.
pub fn set_allowlist(list: Option<Vec<String>>) {
    *ALLOW.lock().unwrap_or_else(|p| p.into_inner()) = list;
}

fn allowed(call: &str, cmd: &str) -> Result<(), String> {
    match &*ALLOW.lock().unwrap_or_else(|p| p.into_inner()) {
        Some(list) if !list.iter().any(|c| c == cmd) => Err(format!(
            "{call}(\"{cmd}\"): `{cmd}` is not in [permissions] proc"
        )),
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

/// Serve one operation. `flush` runs before a child that shares this
/// program's stdout starts (run / run_in), as native flushes its own.
pub(crate) fn call(op: i32, a: &str, b: &str, flush: &dyn Fn()) -> Result<String, String> {
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
            let (ms, args) = head_and_args(b)?;
            allowed("process.exec_status_timeout", a)?;
            let ms: i64 = ms.parse().map_err(|_| "malformed process frame (bad timeout)".to_string())?;
            core::almide_proc_exec_status_timeout(a, &args, ms, core::almide_proc_try_wait_poll).map(status_text)
        }
        85 => {
            allowed("process.run", a)?;
            flush();
            core::almide_proc_run(a, &cells(b)?).map(|c| c.to_string())
        }
        86 => {
            allowed("process.spawn", a)?;
            core::almide_proc_spawn(a, &cells(b)?).map(|p| p.to_string())
        }
        87 => {
            let num = |s: &str| s.parse::<i64>().map_err(|_| format!("malformed process operand `{s}`"));
            core::almide_proc_kill(num(a)?, num(b)?).map(|()| String::new())
        }
        88 => {
            let pid = a.parse::<i64>().map_err(|_| format!("malformed process operand `{a}`"))?;
            Ok(if core::almide_proc_is_alive(pid) { "1" } else { "0" }.to_string())
        }
        89 => Ok(std::process::id().to_string()),
        90 => {
            let (cmd, args) = head_and_args(b)?;
            allowed("process.run_in", &cmd)?;
            flush();
            core::almide_proc_run_in(a, &cmd, &args).map(|c| c.to_string())
        }
        _ => Err(format!("unknown process op {op}")),
    }
}

/// The `fs_call` form: status 0 = ok, 1 = err, the text parked for
/// `host_read`.
pub(crate) fn dispatch(op: i32, a: &str, b: &[u8], flush: &dyn Fn()) -> (i64, Vec<u8>) {
    let (status, text) = match call(op, a, &String::from_utf8_lossy(b), flush) {
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
        let mut buf = vec![0u8; len as u32 as usize];
        mem.read(caller, ptr as u32 as usize, &mut buf)?;
        String::from_utf8(buf).map_err(|_| wasmtime::Error::msg("almide:process/spawn: operand is not UTF-8"))
    };
    let a = read(caller, a_ptr, a_len)?;
    let b = read(caller, b_ptr, b_len)?;
    let (disc, text) = match call(op + OP_FIRST, &a, &b, &|| {}) {
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
