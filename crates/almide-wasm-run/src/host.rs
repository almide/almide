//! The greenfield wasm HOST — the `almide.*` import surface
//! (println/eprintln/exit/fs_call/host_read) over wasmtime. ONE
//! implementation serves both the product runner (src/main.rs) and the
//! almide-wasm test harness (which delegates here), so the host the
//! gates verify IS the host that ships. Every module passes the
//! wasmparser wall before instantiation; `almide.exit` records the
//! process exit code and unwinds — ABORT parity (exit code +
//! stdout-before-abort, the C-153 family) is a first-class observable
//! of every run, never an opaque Err.

use std::sync::{Arc, Mutex};

/// One wasm run's cross-target observables: stdout, stderr, exit code.
/// A trap WITHOUT a recorded `almide.exit` code is a runtime abort
/// (unreachable / div-by-zero / OOB) — exit 1, the native abort contract.
/// Not every gate reads every field (the run manifest hashes stdout only).
#[allow(dead_code)]
pub struct RunResult {
    pub stdout: String,
    pub stderr: String,
    pub exit: i32,
}

struct Host {
    out: Arc<Mutex<String>>,
    err: Arc<Mutex<String>>,
    exit: Arc<Mutex<Option<i32>>>,
    /// The fs result parking buffer (host_read copies it to the guest).
    fs_buf: Arc<Mutex<Vec<u8>>>,
    /// The stdin stream (op 31 drains it — the guest caps counts on its
    /// side); tests run with a fixed buffer, the runner reads lazily.
    stdin: Arc<Mutex<StdinSource>>,
}

/// Where op 31 gets its bytes: a fixed buffer (tests, piped runs), or
/// the process's real stdin read at the FIRST guest read — so a program
/// that never touches stdin never blocks on an open terminal.
pub enum StdinSource {
    Buf(Vec<u8>),
    RealOnce,
    Drained,
}

impl StdinSource {
    fn drain(&mut self) -> Vec<u8> {
        match std::mem::replace(self, StdinSource::Drained) {
            StdinSource::Buf(b) => b,
            StdinSource::RealOnce => {
                use std::io::Read;
                let mut v = Vec::new();
                let _ = std::io::stdin().read_to_end(&mut v);
                v
            }
            StdinSource::Drained => Vec::new(),
        }
    }
}

/// io_err = Display — VERBATIM the native runtime's formatting, so error
/// strings ("No such file or directory (os error 2)") match by
/// construction.
fn io_err(e: impl std::fmt::Display) -> String {
    format!("{e}")
}

/// Length-prefixed string frames (u32 LE + bytes) — the list-of-strings
/// result encoding the guest decoder walks.
fn frames(names: &[String]) -> Vec<u8> {
    let mut b = Vec::new();
    for n in names {
        b.extend_from_slice(&(n.len() as u32).to_le_bytes());
        b.extend_from_slice(n.as_bytes());
    }
    b
}

/// status<<32 | len: 0 = ok, 1 = err (buffer holds the message), 2 =
/// ok-none (the *_if_exists shapes). `flag` rides len for bool ops.
fn pack(status: i64, len: usize) -> i64 {
    (status << 32) | (len as i64 & 0xFFFF_FFFF)
}

/// The WRITE-side ops — split from fs_dispatch for the complexity
/// budget. Bodies verbatim from the native runtime (io_err = Display).
fn fs_dispatch_w(op: i32, a: &str, b: &[u8]) -> (i64, Vec<u8>) {
    use std::path::Path;
    let err_s = |m: String| (pack(1, m.len()), m.into_bytes());
    let unit = |r: Result<(), String>| match r {
        Ok(()) => (pack(0, 0), Vec::new()),
        Err(m) => err_s(m),
    };
    match op {
        2 | 15 => unit(std::fs::write(a, b).map_err(io_err)),
        // write_bytes: b is the guest List[Int] payload — i64 LE slots,
        // low byte each (native `x as u8`).
        3 => {
            let data: Vec<u8> = b
                .chunks_exact(8)
                .map(|c| i64::from_le_bytes(c.try_into().expect("chunk")) as u8)
                .collect();
            unit(std::fs::write(a, &data).map_err(io_err))
        }
        7 => unit(std::fs::create_dir_all(a).map_err(io_err)),
        8 => {
            let p = Path::new(a);
            unit(if p.is_dir() {
                std::fs::remove_dir(a).map_err(io_err)
            } else {
                std::fs::remove_file(a).map_err(io_err)
            })
        }
        9 => {
            let p = Path::new(a);
            unit(if p.is_dir() {
                std::fs::remove_dir_all(a).map_err(io_err)
            } else {
                std::fs::remove_file(a).map_err(io_err)
            })
        }
        _ => unit(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(a)
                .and_then(|mut f| std::io::Write::write_all(&mut f, b))
                .map_err(io_err),
        ),
    }
}

/// The structured READ ops (temp dir / list_dir / read_lines /
/// if-exists / read_bytes) — split for the complexity budget.
fn fs_dispatch_r2(op: i32, a: &str) -> (i64, Vec<u8>) {
    use std::path::Path;
    let ok_text = |t: String| (pack(0, t.len()), t.into_bytes());
    let err_s = |m: String| (pack(1, m.len()), m.into_bytes());
    match op {
        10 => {
            let dir = std::env::temp_dir();
            let name = format!(
                "{}{}",
                a,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            );
            let path = dir.join(&name);
            match std::fs::create_dir_all(&path).map_err(io_err) {
                Ok(()) => ok_text(path.to_string_lossy().replace('\\', "/")),
                Err(m) => err_s(m),
            }
        }
        11 => match std::fs::read_dir(a) {
            Ok(entries) => {
                let mut names = Vec::new();
                for entry in entries {
                    match entry {
                        Ok(e) => names.push(e.file_name().to_string_lossy().to_string()),
                        Err(e) => return err_s(io_err(e)),
                    }
                }
                names.sort();
                let buf = frames(&names);
                (pack(0, buf.len()), buf)
            }
            Err(e) => err_s(io_err(e)),
        },
        12 => match std::fs::read_to_string(a) {
            Ok(t) => {
                let lines: Vec<String> = t.lines().map(str::to_string).collect();
                let buf = frames(&lines);
                (pack(0, buf.len()), buf)
            }
            Err(e) => err_s(io_err(e)),
        },
        13 => {
            if Path::new(a).exists() {
                match std::fs::read_to_string(a) {
                    Ok(t) => ok_text(t),
                    Err(e) => err_s(io_err(e)),
                }
            } else {
                (pack(2, 0), Vec::new())
            }
        }
        _ => match std::fs::read(a) {
            Ok(bytes) => (pack(0, bytes.len()), bytes),
            Err(e) => err_s(io_err(e)),
        },
    }
}

/// The metadata / host-environment ops (17+) — split for the
/// complexity budget. Bodies verbatim from the native runtime.
fn fs_dispatch_meta(op: i32, a: &str, b: &[u8]) -> (i64, Vec<u8>) {
    use std::path::Path;
    let ok_text = |t: String| (pack(0, t.len()), t.into_bytes());
    let err_s = |m: String| (pack(1, m.len()), m.into_bytes());
    let ok_i64 = |v: i64| (pack(0, 8), v.to_le_bytes().to_vec());
    let unit = |r: Result<(), String>| match r {
        Ok(()) => (pack(0, 0), Vec::new()),
        Err(m) => err_s(m),
    };
    match op {
        17 => match std::fs::metadata(a) {
            Ok(m) => ok_i64(m.len() as i64),
            Err(e) => err_s(io_err(e)),
        },
        18 => match std::fs::metadata(a).map_err(io_err).and_then(|m| m.modified().map_err(io_err))
        {
            Ok(t) => ok_i64(
                t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64,
            ),
            Err(m) => err_s(m),
        },
        19 => unit(
            std::fs::copy(a, String::from_utf8_lossy(b).as_ref()).map(|_| ()).map_err(io_err),
        ),
        20 => unit(std::fs::rename(a, String::from_utf8_lossy(b).as_ref()).map_err(io_err)),
        21 => {
            let dir = std::env::temp_dir();
            let name = format!(
                "{}{}",
                a,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            );
            let path = dir.join(&name);
            match std::fs::write(&path, "").map_err(io_err) {
                Ok(()) => ok_text(path.to_string_lossy().replace('\\', "/")),
                Err(m) => err_s(m),
            }
        }
        22 => (pack(0, usize::from(Path::new(a).is_symlink())), Vec::new()),
        23 => {
            fn walk(dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
                for entry in std::fs::read_dir(dir).map_err(io_err)? {
                    let entry = entry.map_err(io_err)?;
                    let path = entry.path();
                    out.push(path.to_string_lossy().replace('\\', "/"));
                    if path.is_dir() {
                        walk(&path, out)?;
                    }
                }
                Ok(())
            }
            let mut results = Vec::new();
            match walk(Path::new(a), &mut results) {
                Ok(()) => {
                    results.sort();
                    let buf = frames(&results);
                    (pack(0, buf.len()), buf)
                }
                Err(m) => err_s(m),
            }
        }
        24 => match std::fs::read_to_string(a) {
            Ok(t) => {
                let lines: Vec<String> = t.lines().map(str::to_string).collect();
                let buf = frames(&lines);
                (pack(0, buf.len()), buf)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (pack(2, 0), Vec::new()),
            Err(e) => err_s(io_err(e)),
        },
        25 => match std::fs::read(a) {
            Ok(bytes) => (pack(0, bytes.len()), bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (pack(2, 0), Vec::new()),
            Err(e) => err_s(io_err(e)),
        },
        _ => fs_dispatch_host(op, a, b),
    }
}

/// The host-environment ops (26+): env/args/entropy — split from
/// fs_dispatch_meta for the complexity budget.
fn fs_dispatch_host(op: i32, a: &str, b: &[u8]) -> (i64, Vec<u8>) {
    let ok_text = |t: String| (pack(0, t.len()), t.into_bytes());
    let err_s = |m: String| (pack(1, m.len()), m.into_bytes());
    match op {
        26 => match std::env::var(a) {
            Ok(v) => ok_text(v),
            Err(_) => (pack(2, 0), Vec::new()),
        },
        27 => ok_text(std::env::consts::OS.to_string()),
        28 => ok_text(std::env::temp_dir().to_string_lossy().replace('\\', "/")),
        // args: [argv0] — a non-empty program path on both legs; the
        // fixtures only observe len + non-emptiness.
        29 => {
            let buf = frames(&["wasm-harness".to_string()]);
            (pack(0, buf.len()), buf)
        }
        // stdin read (up to n = a bytes) — the harness has no stdin.
        31 => (pack(0, 0), Vec::new()),
        // cwd — the same std::env the native runtime reads.
        33 => match std::env::current_dir() {
            Ok(p) => ok_text(p.to_string_lossy().replace('\\', "/")),
            Err(e) => err_s(io_err(e)),
        },
        // host entropy: n = b_len bytes from a seeded-by-time xorshift
        // (the range property is the only observable, C-112).
        32 => {
            let n = b.len();
            let mut seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64
                | 1;
            let mut out = Vec::with_capacity(n);
            for _ in 0..n {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                out.push(seed as u8);
            }
            (pack(0, out.len()), out)
        }
        _ => err_s(format!("unknown fs op {op}")),
    }
}

fn fs_dispatch(op: i32, a: &str, b: &[u8]) -> (i64, Vec<u8>) {
    use std::path::Path;
    if matches!(op, 2 | 3 | 7..=9 | 15 | 16) {
        return fs_dispatch_w(op, a, b);
    }
    if matches!(op, 10..=14) {
        return fs_dispatch_r2(op, a);
    }
    if op >= 17 {
        return fs_dispatch_meta(op, a, b);
    }
    let ok_text = |t: String| (pack(0, t.len()), t.into_bytes());
    let err_s = |m: String| (pack(1, m.len()), m.into_bytes());
    match op {
        1 => match std::fs::read_to_string(a) {
            Ok(t) => ok_text(t),
            Err(e) => err_s(io_err(e)),
        },
        4 => (pack(0, usize::from(Path::new(a).exists())), Vec::new()),
        5 => (pack(0, usize::from(Path::new(a).is_dir())), Vec::new()),
        6 => (pack(0, usize::from(Path::new(a).is_file())), Vec::new()),
        _ => err_s(format!("unknown fs op {op}")),
    }
}

fn append_line(
    caller: &mut wasmtime::Caller<'_, Host>,
    sink: fn(&Host) -> &Arc<Mutex<String>>,
    ptr: i32,
    len: i32,
) {
    let mem = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .expect("exported memory");
    let mut buf = vec![0u8; len as u32 as usize];
    if let Err(e) = mem.read(&caller, ptr as u32 as usize, &mut buf) {
        panic!("in-bounds read: {e:?} ptr={ptr} len={len} memsize={}", mem.data_size(&caller));
    }
    let mut o = sink(caller.data()).lock().expect("test harness invariant");
    o.push_str(&String::from_utf8_lossy(&buf));
    o.push('\n');
}

pub fn run_wasm(bytes: &[u8]) -> anyhow::Result<RunResult> {
    run_wasm_with(bytes, &[])
}

/// Run with a fixed stdin buffer (tests; piped byte streams).
pub fn run_wasm_with(bytes: &[u8], stdin: &[u8]) -> anyhow::Result<RunResult> {
    run_wasm_src(bytes, StdinSource::Buf(stdin.to_vec()))
}

/// Run with the process's real stdin, read lazily on first guest read
/// (the product runner — never blocks for programs that skip stdin).
pub fn run_wasm_real_stdin(bytes: &[u8]) -> anyhow::Result<RunResult> {
    run_wasm_src(bytes, StdinSource::RealOnce)
}

fn run_wasm_src(bytes: &[u8], stdin: StdinSource) -> anyhow::Result<RunResult> {
    wasmparser::validate(bytes)?; // the wall: never instantiate an invalid module
    // Epoch deadline: a fixture (or a MUTANT under the gate) that
    // diverges must FAIL the run, never hang the suite. 30s of real time
    // is orders beyond any fixture; the deadline maps to a plain trap.
    let mut cfg = wasmtime::Config::new();
    cfg.epoch_interruption(true);
    let engine = wasmtime::Engine::new(&cfg)?;
    let module = wasmtime::Module::new(&engine, bytes)?;
    let out = Arc::new(Mutex::new(String::new()));
    let err = Arc::new(Mutex::new(String::new()));
    let exit = Arc::new(Mutex::new(None));
    let fs_buf = Arc::new(Mutex::new(Vec::new()));
    let stdin_buf = Arc::new(Mutex::new(stdin));
    let mut store = wasmtime::Store::new(
        &engine,
        Host {
            out: out.clone(),
            err: err.clone(),
            exit: exit.clone(),
            fs_buf: fs_buf.clone(),
            stdin: stdin_buf.clone(),
        },
    );
    let mut linker = wasmtime::Linker::new(&engine);
    linker.func_wrap(
        "almide",
        "println",
        |mut caller: wasmtime::Caller<'_, Host>, ptr: i32, len: i32| {
            append_line(&mut caller, |h| &h.out, ptr, len);
        },
    )?;
    linker.func_wrap(
        "almide",
        "eprintln",
        |mut caller: wasmtime::Caller<'_, Host>, ptr: i32, len: i32| {
            append_line(&mut caller, |h| &h.err, ptr, len);
        },
    )?;
    // A fn item (not a closure): a return-type-annotated closure is not
    // higher-ranked over the Caller lifetime and fails IntoFunc.
    fn exit_host(caller: wasmtime::Caller<'_, Host>, code: i32) -> wasmtime::Result<()> {
        *caller.data().exit.lock().expect("test harness invariant") = Some(code);
        // Unwind: the emitter guarantees an `unreachable` follows the
        // call, so returning an error here is the ONLY way out — no
        // instruction after `process.exit` ever executes.
        Err(wasmtime::Error::msg("almide.exit"))
    }
    linker.func_wrap("almide", "exit", exit_host)?;
    linker.func_wrap(
        "almide",
        "fs_call",
        |mut caller: wasmtime::Caller<'_, Host>,
         op: i32,
         a_ptr: i32,
         a_len: i32,
         b_ptr: i32,
         b_len: i32|
         -> wasmtime::Result<i64> {
            let mem = caller
                .get_export("memory")
                .and_then(|e| e.into_memory())
                .expect("exported memory");
            let mut a = vec![0u8; a_len as u32 as usize];
            mem.read(&caller, a_ptr as u32 as usize, &mut a)?;
            let mut b = vec![0u8; b_len as u32 as usize];
            mem.read(&caller, b_ptr as u32 as usize, &mut b)?;
            let a = String::from_utf8_lossy(&a).to_string();
            // op 30 = raw stdout append (io.write / io.write_bytes):
            // PROGRAM order with println is the C-contract, so it goes
            // straight into the same sink, no trailing newline.
            // op 31 = stdin: drain the remaining stream into the
            // parking buffer (native read-to-end semantics).
            if op == 31 {
                let drained = caller.data().stdin.lock().expect("stdin").drain();
                let len = drained.len();
                *caller.data().fs_buf.lock().expect("fs buf") = drained;
                return Ok((len as i64) & 0xFFFF_FFFF);
            }
            // op 34 = wall clock (nanos, RAW i64 — no status packing).
            if op == 34 {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as i64;
                return Ok(now);
            }
            if op == 30 {
                let mut o = caller.data().out.lock().expect("test harness invariant");
                o.push_str(&String::from_utf8_lossy(&b));
                return Ok(0);
            }
            let (ret, buf) = fs_dispatch(op, &a, &b);
            *caller.data().fs_buf.lock().expect("fs buf") = buf;
            Ok(ret)
        },
    )?;
    linker.func_wrap(
        "almide",
        "host_read",
        |mut caller: wasmtime::Caller<'_, Host>, dst: i32| -> wasmtime::Result<()> {
            let mem = caller
                .get_export("memory")
                .and_then(|e| e.into_memory())
                .expect("exported memory");
            let buf = caller.data().fs_buf.lock().expect("fs buf").clone();
            mem.write(&mut caller, dst as u32 as usize, &buf)?;
            Ok(())
        },
    )?;
    store.set_epoch_deadline(1);
    let eng = engine.clone();
    let ticker = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(30));
        eng.increment_epoch();
    });
    let instance = linker.instantiate(&mut store, &module)?;
    let main = instance.get_typed_func::<(), ()>(&mut store, "main")?;
    let call = main.call(&mut store, ());
    let recorded = exit.lock().expect("test harness invariant").take();
    let exit_code = match (&call, recorded) {
        (Ok(()), None) => 0,
        (Err(_), Some(code)) => code,
        (Err(e), None) => {
            if std::env::var("ALMIDE_DBG_TRAP").is_ok() {
                eprintln!("TRAP: {e:?}");
            }
            1 // genuine trap = runtime abort
        }
        (Ok(()), Some(_)) => {
            anyhow::bail!("almide.exit recorded a code but the run returned normally")
        }
    };
    drop(ticker);
    Ok(RunResult {
        stdout: out.lock().expect("test harness invariant").clone(),
        stderr: err.lock().expect("test harness invariant").clone(),
        exit: exit_code,
    })
}
