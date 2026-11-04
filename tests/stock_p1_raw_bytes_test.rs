//! #4391 (C-449, C-450): the stock WASI preview-1 artifact never turns host
//! bytes that are not UTF-8 into a String, whatever runtime hosts it.
//! wasmtime refuses such an argument or variable before the module starts,
//! and refuses a directory holding such a name (`ilseq`), so this test is the
//! host that does not: its own `wasi_snapshot_preview1` imports hand the
//! artifact the raw bytes, and the program must read what native reads.
//!
//! - `env.args`: an argument's lossy decode;
//! - `env.get`: `none` for a variable whose value is not UTF-8 (C-133);
//! - `env.vars`: that variable left out (C-439);
//! - `fs.list_dir` / `fs.walk` / `fs.glob`: each name's lossy decode, sorted
//!   by the decoded bytes (C-272's order), the walk still descending by the
//!   raw name;
//! - `env.cwd` / `fs.temp_dir`: the lossy decode of `PWD` / `TMPDIR`.
//!
//! Negative control: before #4391 the guest and the fs service copied the
//! bytes raw — the argument printed `[97, 128, 98]`, `env.get` answered
//! `some`, and the names listed raw in raw order.
#![cfg(all(unix, feature = "embedded-host"))]

use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::process::Command;
use std::sync::{Arc, Mutex};

use almide_wasm_run::wasmtime::{Caller, Engine, Extern, Func, Linker, Memory, Module, Store, Val};

const RAW: &[u8] = b"a\x80b";

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// The host's whole world: argv, environ, and directories by their path
/// under the root preopen (fd 3, "/"): `Ok(names)` lists, `Err(errno)` is
/// what `fd_readdir` answers for it.
#[derive(Clone, Default)]
struct World {
    argv: Vec<Vec<u8>>,
    env: Vec<Vec<u8>>,
    dirs: HashMap<Vec<u8>, Result<Vec<Vec<u8>>, i32>>,
}

/// Write `strs` NUL-terminated at `buf` and their addresses at `ptrs`.
fn put_strings(mem: &mut [u8], ptrs: usize, buf: usize, strs: &[Vec<u8>]) {
    let mut at = buf;
    for (k, s) in strs.iter().enumerate() {
        mem[ptrs + 4 * k..ptrs + 4 * k + 4].copy_from_slice(&(at as u32).to_le_bytes());
        mem[at..at + s.len()].copy_from_slice(s);
        mem[at + s.len()] = 0;
        at += s.len() + 1;
    }
}

fn put32(m: &mut [u8], at: usize, v: u32) {
    m[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn get32(m: &[u8], at: usize) -> usize {
    u32::from_le_bytes(m[at..at + 4].try_into().unwrap()) as usize
}

/// `fd_readdir`'s dirents for `names` from entry `cookie` on, as many as fit
/// in `cap` bytes (the last one may be cut, as the spec allows).
fn dirents(names: &[Vec<u8>], cookie: usize, cap: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for (k, n) in names.iter().enumerate().skip(cookie) {
        out.extend_from_slice(&((k + 1) as u64).to_le_bytes());
        out.extend_from_slice(&((k + 100) as u64).to_le_bytes());
        out.extend_from_slice(&(n.len() as u32).to_le_bytes());
        out.extend_from_slice(&[4, 0, 0, 0]);
        out.extend_from_slice(n);
    }
    out.truncate(cap);
    out
}

fn memory(c: &mut Caller<'_, ()>) -> Memory {
    match c.get_export("memory") {
        Some(Extern::Memory(m)) => m,
        _ => panic!("no memory export"),
    }
}

/// One preview-1 import over `w`; every one the test does not serve answers
/// ENOSYS (52).
fn serve(name: &str, w: &World, fds: &Mutex<HashMap<usize, Vec<u8>>>, out: &Mutex<Vec<u8>>, m: &mut [u8], params: &[Val]) -> Result<i32, String> {
    let p = |k: usize| params[k].unwrap_i32() as u32 as usize;
    let sizes = |strs: &[Vec<u8>]| (strs.len() as u32, strs.iter().map(|s| s.len() as u32 + 1).sum::<u32>());
    Ok(match name {
        "args_sizes_get" | "environ_sizes_get" => {
            let (n, b) = sizes(if name == "args_sizes_get" { &w.argv } else { &w.env });
            put32(m, p(0), n);
            put32(m, p(1), b);
            0
        }
        "args_get" => {
            put_strings(m, p(0), p(1), &w.argv);
            0
        }
        "environ_get" => {
            put_strings(m, p(0), p(1), &w.env);
            0
        }
        "fd_write" => {
            let mut n = 0u32;
            for k in 0..p(2) {
                let iov = p(1) + 8 * k;
                let (base, len) = (get32(m, iov), get32(m, iov + 4));
                if p(0) == 1 {
                    out.lock().unwrap().extend_from_slice(&m[base..base + len]);
                }
                n += len as u32;
            }
            put32(m, p(3), n);
            0
        }
        "fd_read" => {
            put32(m, p(3), 0);
            0
        }
        "clock_time_get" => {
            m[p(2)..p(2) + 8].fill(0);
            0
        }
        "random_get" => {
            m[p(0)..p(0) + p(1)].fill(0);
            0
        }
        // The root preopen, fd 3 named "/"; none past it.
        "fd_prestat_get" if p(0) == 3 => {
            m[p(1)..p(1) + 4].fill(0);
            put32(m, p(1) + 4, 1);
            0
        }
        "fd_prestat_get" => 8,
        "fd_prestat_dir_name" => {
            m[p(1)] = b'/';
            0
        }
        // path_open(fd, dirflags, path, len, oflags, rights, inherit, fdflags, *fd)
        "path_open" => {
            let path = m[p(2)..p(2) + p(3)].to_vec();
            if !w.dirs.contains_key(&path) {
                return Ok(44);
            }
            let mut fds = fds.lock().unwrap();
            let fd = 10 + fds.len();
            fds.insert(fd, path);
            put32(m, p(8), fd as u32);
            0
        }
        "fd_close" => 0,
        // fd_readdir(fd, buf, len, cookie, *used)
        "fd_readdir" => {
            let path = fds.lock().unwrap().get(&p(0)).cloned().ok_or("readdir of an unopened fd")?;
            let cookie = params[3].unwrap_i64() as usize;
            match &w.dirs[&path] {
                Ok(names) => {
                    let ents = dirents(names, cookie, p(2));
                    m[p(1)..p(1) + ents.len()].copy_from_slice(&ents);
                    put32(m, p(4), ents.len() as u32);
                    0
                }
                Err(e) => *e,
            }
        }
        // path_filestat_get(fd, flags, path, len, *filestat): a listed
        // directory is a directory (3), anything else a regular file (4).
        "path_filestat_get" => {
            let path = m[p(2)..p(2) + p(3)].to_vec();
            m[p(4)..p(4) + 64].fill(0);
            m[p(4) + 16] = if w.dirs.contains_key(&path) { 3 } else { 4 };
            0
        }
        "proc_exit" => return Err(format!("exit {}", p(0))),
        _ => 52,
    })
}

/// Run `wasm` on the raw-bytes host `w`: (stdout, exit code).
fn run_raw(wasm: &[u8], w: World) -> (String, i32) {
    let engine = Engine::default();
    let module = Module::new(&engine, wasm).expect("module");
    let out = Arc::new(Mutex::new(Vec::<u8>::new()));
    let fds = Arc::new(Mutex::new(HashMap::<usize, Vec<u8>>::new()));
    let mut store: Store<()> = Store::new(&engine, ());
    let mut linker: Linker<()> = Linker::new(&engine);
    for imp in module.imports() {
        if imp.module() != "wasi_snapshot_preview1" {
            continue;
        }
        let name = imp.name().to_string();
        let ty = imp.ty().func().expect("func import").clone();
        let (w, out, fds) = (w.clone(), out.clone(), fds.clone());
        let f = Func::new(&mut store, ty, move |mut c, params, results| {
            let mem = memory(&mut c);
            let errno = serve(&name, &w, &fds, &out, mem.data_mut(&mut c), params).map_err(almide_wasm_run::wasmtime::Error::msg)?;
            if let Some(r) = results.first_mut() {
                *r = Val::I32(errno);
            }
            Ok(())
        });
        linker.define(&store, "wasi_snapshot_preview1", imp.name(), f).expect("define");
    }
    let inst = linker.instantiate(&mut store, &module).expect("instantiate");
    let start = inst.get_typed_func::<(), ()>(&mut store, "_start").expect("_start");
    let code = match start.call(&mut store, ()) {
        Ok(()) => 0,
        Err(e) => {
            let msg = format!("{e:#}");
            msg.split("exit ").nth(1).and_then(|t| t.trim().parse().ok()).unwrap_or_else(|| panic!("trap: {msg}"))
        }
    };
    let stdout = String::from_utf8_lossy(&out.lock().unwrap()).into_owned();
    (stdout, code)
}

/// Build `src` as the stock artifact (and, with `native`, the binary too).
fn build(dir: &std::path::Path, src: &str, native: bool) -> (std::path::PathBuf, Vec<u8>) {
    let file = dir.join("app.almd");
    std::fs::write(&file, src).unwrap();
    let (bin, wasm) = (dir.join("app"), dir.join("app.wasm"));
    let mut targets = vec![(Some("wasm"), &wasm)];
    if native {
        targets.push((None, &bin));
    }
    for (target, out) in targets {
        let mut c = Command::new(almide());
        c.arg("build").arg(&file).arg("-o").arg(out);
        if let Some(t) = target {
            c.args(["--target", t]);
        }
        let o = c.output().expect("spawn");
        assert!(o.status.success(), "almide build {target:?}: {}", String::from_utf8_lossy(&o.stderr));
    }
    (bin, std::fs::read(&wasm).unwrap())
}

#[test]
fn the_stock_artifact_reads_non_utf8_host_bytes_as_native_does() {
    let t = tempfile::tempdir().expect("scratch");
    let (bin, wasm) = build(t.path(), include_str!("../spec/embedded_cross/env_non_utf8_host_text.almd"), true);
    let native = Command::new(&bin)
        .arg(OsStr::from_bytes(RAW))
        .env("T4391_BAD", OsStr::from_bytes(RAW))
        .env("T4391_OK", "fine")
        .output()
        .expect("native");
    let native = (String::from_utf8_lossy(&native.stdout).into_owned(), native.status.code().unwrap_or(-1));
    let want = "args=[\"[97, 239, 191, 189, 98]\"]\nbad none\nok some fine\nlisted bad=false ok=true\n";
    assert_eq!(native, (want.to_string(), 0), "native");
    let mut bad = b"T4391_BAD=".to_vec();
    bad.extend_from_slice(RAW);
    let world = World { argv: vec![b"app.wasm".to_vec(), RAW.to_vec()], env: vec![bad, b"T4391_OK=fine".to_vec()], ..World::default() };
    assert_eq!(run_raw(&wasm, world), native, "the stock artifact on a raw-bytes host differs from native");
}

const NAMES: &str = r#"import env
import fs

fn hex(s: String) -> String = "${bytes.to_list(bytes.from_string(s))}"

fn shown(call: String, r: Result[List[String], String]) -> String = match r {
  ok(ns) => "${call} ${ns |> list.map((n) => hex(n))}",
  err(m) => "${call} err ${m}",
}

effect fn main() -> Unit = {
  println(shown("list_dir", fs.list_dir("/d")))
  println(shown("walk", fs.walk("/d")))
  println(shown("glob", fs.glob("/d/*")))
  println(shown("refused", fs.list_dir("/bad")))
  println("cwd ${hex(env.cwd()!)}")
  println("temp_dir ${hex(fs.temp_dir())}")
}
"#;

#[test]
fn the_stock_artifact_lists_non_utf8_names_and_paths_as_their_lossy_decode() {
    let t = tempfile::tempdir().expect("scratch");
    let (_, wasm) = build(t.path(), NAMES, false);
    let names = vec![b"f\x80g".to_vec(), b"ok".to_vec(), "é".as_bytes().to_vec(), b"\x80z".to_vec(), b"sub\x80".to_vec()];
    let mut dirs = HashMap::new();
    dirs.insert(b"d".to_vec(), Ok(names));
    // A directory whose name is not UTF-8: the walk descends by the RAW path.
    dirs.insert(b"d/sub\x80".to_vec(), Ok(vec![b"in".to_vec()]));
    dirs.insert(b"bad".to_vec(), Err(i32::from(almide_base::fs_errno::FS_HOST_ILSEQ_WASI)));
    let world = World {
        argv: vec![b"app.wasm".to_vec()],
        env: vec![b"PWD=/w\x80".to_vec(), b"TMPDIR=/t\xC3".to_vec()],
        dirs,
    };
    // Each name decoded, then sorted by the decoded bytes, as native does.
    let (f, ok, e, z, sub) = ("[102, 239, 191, 189, 103]", "[111, 107]", "[195, 169]", "[239, 191, 189, 122]", "[115, 117, 98, 239, 191, 189]");
    let path = |n: &str| format!("\"[47, 100, 47, {}\"", &n[1..]);
    let sub_in = "\"[47, 100, 47, 115, 117, 98, 239, 191, 189, 47, 105, 110]\"";
    let listed = format!("[\"{f}\", \"{ok}\", \"{sub}\", \"{e}\", \"{z}\"]");
    let walked = format!("[{}, {}, {}, {sub_in}, {}, {}]", path(f), path(ok), path(sub), path(e), path(z));
    let globbed = format!("[{}, {}, {}, {}, {}]", path(f), path(ok), path(sub), path(e), path(z));
    let text = almide_base::fs_errno::FS_HOST_ILSEQ_TEXT;
    let want = format!(
        "list_dir {listed}\nwalk {walked}\nglob {globbed}\nrefused err fs.list_dir(\"/bad\"): {text}\ncwd [47, 119, 239, 191, 189]\ntemp_dir [47, 116, 239, 191, 189]\n"
    );
    assert_eq!(run_raw(&wasm, world), (want, 0), "the stock artifact on a raw-bytes host");
}
