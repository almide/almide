//! #4391: the stock WASI preview-1 artifact never turns host bytes that are
//! not UTF-8 into a String, whatever runtime hosts it. wasmtime refuses such
//! an argument or variable before the module starts, so this test is the
//! host that does not: its own `wasi_snapshot_preview1` imports hand the
//! artifact `a\x80b` as argv[1] and as one variable's value, and the program
//! must read what the native binary reads from the same bytes.
//!
//! - `env.args`: the argument's lossy decode (rule 1).
//! - `env.get`: `none` for the variable, as `std::env::var(..).ok()` (C-133).
//! - `env.vars`: the variable left out.
//!
//! Negative control: before #4391 the shims copied the bytes raw, so the
//! artifact printed `[97, 128, 98]` and answered `some` for the variable.
#![cfg(all(unix, feature = "embedded-host"))]

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::process::Command;
use std::sync::{Arc, Mutex};

use almide_wasm_run::wasmtime::{Caller, Engine, Extern, Func, Linker, Module, Store, Val};

const PROGRAM: &str = r#"import env

fn hx(s: String) -> String = "${bytes.to_list(bytes.from_string(s))}"

effect fn main() -> Unit = {
  println("args=${env.args() |> list.map((a) => hx(a))}")
  println(match env.get("A4391_BAD") {
    some(v) => "bad some ${hx(v)}",
    none => "bad none",
  })
  println(match env.get("A4391_OK") {
    some(v) => "ok some ${v}",
    none => "ok none",
  })
  let names = map.keys(env.vars()!)
  println("listed bad=${list.contains(names, "A4391_BAD")} ok=${list.contains(names, "A4391_OK")}")
}
"#;

const RAW: &[u8] = b"a\x80b";

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
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

/// Run `wasm` on a minimal preview-1 host whose argv and environ are `argv`
/// and `env`: (stdout, exit code). Every other import answers ENOSYS (52).
fn run_raw(wasm: &[u8], argv: Vec<Vec<u8>>, env: Vec<Vec<u8>>) -> (String, i32) {
    let engine = Engine::default();
    let module = Module::new(&engine, wasm).expect("module");
    let out = Arc::new(Mutex::new(Vec::<u8>::new()));
    let mut store: Store<()> = Store::new(&engine, ());
    let mut linker: Linker<()> = Linker::new(&engine);
    fn memory(c: &mut Caller<'_, ()>) -> almide_wasm_run::wasmtime::Memory {
        match c.get_export("memory") {
            Some(Extern::Memory(m)) => m,
            _ => panic!("no memory export"),
        }
    }
    for imp in module.imports() {
        if imp.module() != "wasi_snapshot_preview1" {
            continue;
        }
        let name = imp.name().to_string();
        let ty = imp.ty().func().expect("func import").clone();
        let (argv, env, out) = (argv.clone(), env.clone(), out.clone());
        let f = Func::new(&mut store, ty, move |mut c, params, results| {
            let p = |k: usize| params[k].unwrap_i32() as usize;
            let sizes = |strs: &[Vec<u8>]| (strs.len() as u32, strs.iter().map(|s| s.len() as u32 + 1).sum::<u32>());
            let mem = memory(&mut c);
            let errno = match name.as_str() {
                "args_sizes_get" | "environ_sizes_get" => {
                    let (n, b) = sizes(if name == "args_sizes_get" { &argv } else { &env });
                    let m = mem.data_mut(&mut c);
                    m[p(0)..p(0) + 4].copy_from_slice(&n.to_le_bytes());
                    m[p(1)..p(1) + 4].copy_from_slice(&b.to_le_bytes());
                    0
                }
                "args_get" | "environ_get" => {
                    let strs = if name == "args_get" { &argv } else { &env };
                    put_strings(mem.data_mut(&mut c), p(0), p(1), strs);
                    0
                }
                "fd_write" => {
                    let m = mem.data_mut(&mut c);
                    let mut n = 0u32;
                    for k in 0..p(2) {
                        let iov = p(1) + 8 * k;
                        let base = u32::from_le_bytes(m[iov..iov + 4].try_into().unwrap()) as usize;
                        let len = u32::from_le_bytes(m[iov + 4..iov + 8].try_into().unwrap()) as usize;
                        if p(0) == 1 {
                            out.lock().unwrap().extend_from_slice(&m[base..base + len]);
                        }
                        n += len as u32;
                    }
                    m[p(3)..p(3) + 4].copy_from_slice(&n.to_le_bytes());
                    0
                }
                "proc_exit" => return Err(almide_wasm_run::wasmtime::Error::msg(format!("exit {}", p(0)))),
                _ => 52,
            };
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

#[test]
fn the_stock_artifact_reads_non_utf8_host_bytes_as_native_does() {
    let t = tempfile::tempdir().expect("scratch");
    let src = t.path().join("app.almd");
    std::fs::write(&src, PROGRAM).unwrap();
    let bin = t.path().join("app");
    let wasm = t.path().join("app.wasm");
    for (target, out) in [(None, &bin), (Some("wasm"), &wasm)] {
        let mut c = Command::new(almide());
        c.arg("build").arg(&src).arg("-o").arg(out);
        if let Some(t) = target {
            c.args(["--target", t]);
        }
        let o = c.output().expect("spawn");
        assert!(o.status.success(), "almide build {target:?}: {}", String::from_utf8_lossy(&o.stderr));
    }
    let native = Command::new(&bin)
        .arg(OsStr::from_bytes(RAW))
        .env("A4391_BAD", OsStr::from_bytes(RAW))
        .env("A4391_OK", "fine")
        .output()
        .expect("native");
    let native = (String::from_utf8_lossy(&native.stdout).into_owned(), native.status.code().unwrap_or(-1));
    let want = "args=[\"[97, 239, 191, 189, 98]\"]\nbad none\nok some fine\nlisted bad=false ok=true\n";
    assert_eq!(native, (want.to_string(), 0), "native");
    let mut bad = b"A4391_BAD=".to_vec();
    bad.extend_from_slice(RAW);
    let stock = run_raw(
        &std::fs::read(&wasm).unwrap(),
        vec![b"app.wasm".to_vec(), RAW.to_vec()],
        vec![bad, b"A4391_OK=fine".to_vec()],
    );
    assert_eq!(stock, native, "the stock artifact on a raw-bytes host differs from native");
}
