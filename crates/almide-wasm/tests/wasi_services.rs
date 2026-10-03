//! Pin for the p1 shim's reachability-gated services (#1841): the
//! environ/args imports and their shims ship ONLY when the module's
//! emitted op set reaches them — the #1712 fixed-slot discipline applied
//! to the `to_wasi` transform. `env.get` earns the environ pair,
//! `env.args` the args pair, `env.set` its overlay shim and no import at
//! all. The other three base imports (fd_read / random_get /
//! clock_time_get) ship only with the op whose shim arm calls them (#3114).
//! The final prune (#3136) then drops every function nothing exported
//! reaches: hello, world ships `fd_write` alone, no `proc_exit` (nothing in
//! it can trap or exit), and none of the emitter's unreached helper slots.
//!
//! The behavioural half (an args/env program answering byte-identically
//! on native and stock wasmtime) is the cross-target gate's — this pin
//! holds the SHAPE, so a change that quietly ships the service to every
//! artifact again reads red here, not as a README size row a day later.

use almide_wasm_run::wasi::{to_wasi, P1Services};
use wasmparser::{Parser, Payload};

const HELLO: &str = r#"fn main() -> Unit = { println("Hello, world!") }
"#;

const ARGS: &str = r#"import env

effect fn main() -> Unit = {
  let args = env.args()
  println(int.to_string(list.len(args)))
}
"#;

const ENV_GET: &str = r#"import env

effect fn main() -> Unit = {
  let home = env.get("HOME") ?? ""
  println("home nonempty ${string.len(home) > 0}")
}
"#;

const ENV_SET: &str = r#"import env

effect fn main() -> Unit = {
  env.set("WASI_SERVICES_PIN", "1")
  println("set")
}
"#;

const ENV_ROUND_TRIP: &str = r#"import env

effect fn main() -> Unit = {
  env.set("WASI_SERVICES_PIN", "1")
  println(env.get("WASI_SERVICES_PIN") ?? "(none)")
  println(int.to_string(list.len(env.args())))
}
"#;

/// The stock-runtime artifact for `src`, with the op set the emitter
/// recorded for it.
fn artifact(name: &str, src: &str) -> (Vec<u8>, Vec<i32>) {
    let ir = almide_spine::s5::lower_to_ir(name, src).expect("lowers");
    let (bytes, host_ops) = almide_wasm::emit_program_with_ops(&ir).expect("emits");
    let host_ops: Vec<i32> = host_ops.into_iter().collect();
    let wasi = to_wasi(&bytes, &host_ops).expect("to_wasi");
    (wasi, host_ops)
}

/// The `wasi_snapshot_preview1` import names, in module order, and the
/// number of defined (non-import) functions — what `wasm-tools print`
/// shows in its import block and function section.
fn shape(wasm: &[u8]) -> (Vec<String>, usize) {
    let mut imports = Vec::new();
    let mut defined = 0usize;
    for payload in Parser::new(0).parse_all(wasm) {
        match payload.expect("valid module") {
            Payload::ImportSection(r) => {
                for group in r {
                    for item in group.expect("imports") {
                        let (_, i) = item.expect("import row");
                        assert_eq!(i.module, "wasi_snapshot_preview1", "a p1 artifact imports only from wasi_snapshot_preview1");
                        imports.push(i.name.to_string());
                    }
                }
            }
            Payload::FunctionSection(r) => defined = r.count() as usize,
            _ => {}
        }
    }
    (imports, defined)
}

/// What every artifact that calls a host op imports: the console, and the
/// exit its refusal path takes.
const BASE: [&str; 2] = ["fd_write", "proc_exit"];
const ENVIRON_PAIR: [&str; 2] = ["environ_sizes_get", "environ_get"];
const ARGS_PAIR: [&str; 2] = ["args_sizes_get", "args_get"];

fn expect_imports(got: &[String], want: &[&str]) {
    let got: Vec<&str> = got.iter().map(String::as_str).collect();
    assert_eq!(got, want, "p1 import surface");
}

/// The emitted module for `src`, before the transform.
fn raw(name: &str, src: &str) -> Vec<u8> {
    let ir = almide_spine::s5::lower_to_ir(name, src).expect("lowers");
    almide_wasm::emit_program(&ir).expect("emits")
}

/// The defined functions `raw` ships with no host op served: the service
/// shims are the difference between this and the artifact's count.
fn defined_without_services(raw: &[u8]) -> usize {
    shape(&to_wasi(raw, &[]).expect("to_wasi")).1
}

/// The number of defined functions whose whole body is `unreachable` — the
/// index-keeping stubs the emitter ships for helper slots a program never
/// reaches, which no shipped artifact carries (#3136).
fn unreachable_stubs(wasm: &[u8]) -> usize {
    Parser::new(0)
        .parse_all(wasm)
        .filter(|p| match p.as_ref().expect("valid module") {
            Payload::CodeSectionEntry(b) => {
                let ops: Vec<_> = b.get_operators_reader().expect("body").into_iter().map(|o| o.expect("op")).collect();
                matches!(ops.as_slice(), [wasmparser::Operator::Unreachable, wasmparser::Operator::End])
            }
            _ => false,
        })
        .count()
}

#[test]
fn hello_ships_no_environ_or_args_service() {
    let (wasm, host_ops) = artifact("hello.almd", HELLO);
    assert_eq!(P1Services::from_ops(&host_ops), P1Services { env_get: false, env_set: false, args: false, fs: false, proc: false });
    let (imports, defined) = shape(&wasm);
    expect_imports(&imports, &["fd_write"]);
    // The emitter's own module keeps its fixed helper slots as stubs; the
    // shipped one keeps only what `_start` reaches (#3136).
    let raw = raw("hello.almd", HELLO);
    assert!(unreachable_stubs(&raw) > 0, "the emitted module keeps its helper slots");
    assert_eq!(unreachable_stubs(&wasm), 0, "hello ships no unreachable stub");
    assert!(defined <= 4, "hello ships its reached functions and the println shim alone: {defined}");
}

#[test]
fn args_program_ships_the_args_pair_only() {
    let (wasm, host_ops) = artifact("args.almd", ARGS);
    assert_eq!(P1Services::from_ops(&host_ops), P1Services { env_get: false, env_set: false, args: true, fs: false, proc: false });
    let (imports, defined) = shape(&wasm);
    let want: Vec<&str> = BASE.iter().chain(ARGS_PAIR.iter()).copied().collect();
    expect_imports(&imports, &want);
    assert_eq!(defined, defined_without_services(&raw("args.almd", ARGS)) + 1, "the args frames shim, alone");
}

#[test]
fn env_get_program_ships_the_environ_pair_only() {
    let (wasm, host_ops) = artifact("env_get.almd", ENV_GET);
    assert_eq!(P1Services::from_ops(&host_ops), P1Services { env_get: true, env_set: false, args: false, fs: false, proc: false });
    let (imports, defined) = shape(&wasm);
    let want: Vec<&str> = BASE.iter().chain(ENVIRON_PAIR.iter()).copied().collect();
    expect_imports(&imports, &want);
    assert_eq!(defined, defined_without_services(&raw("env_get.almd", ENV_GET)) + 1, "the environ scan shim, alone");
}

#[test]
fn env_set_program_ships_the_overlay_shim_and_no_import() {
    let (wasm, host_ops) = artifact("env_set.almd", ENV_SET);
    assert_eq!(P1Services::from_ops(&host_ops), P1Services { env_get: false, env_set: true, args: false, fs: false, proc: false });
    let (imports, defined) = shape(&wasm);
    expect_imports(&imports, &BASE);
    assert_eq!(defined, defined_without_services(&raw("env_set.almd", ENV_SET)) + 1, "the overlay-append shim, alone");
}

#[test]
fn full_env_surface_ships_the_whole_quartet() {
    let (wasm, host_ops) = artifact("env_round_trip.almd", ENV_ROUND_TRIP);
    assert_eq!(P1Services::from_ops(&host_ops), P1Services { env_get: true, env_set: true, args: true, fs: false, proc: false });
    let (imports, defined) = shape(&wasm);
    let want: Vec<&str> = BASE.iter().chain(ENVIRON_PAIR.iter()).chain(ARGS_PAIR.iter()).copied().collect();
    expect_imports(&imports, &want);
    let without = defined_without_services(&raw("env_round_trip.almd", ENV_ROUND_TRIP));
    assert_eq!(defined, without + 3, "env_get + env_set + args shims");
}

/// The gate is a SELECTION over the op table: the same bytes with a
/// different op set produce a different import surface, and the two
/// selections differ by exactly the args pair and the two shims the
/// smaller one leaves out. The module must call `fs_call` — over one that
/// never does (hello), the prune drops every service whatever the op set.
#[test]
fn the_gate_selects_from_the_op_table() {
    let raw = raw("env_round_trip.almd", ENV_ROUND_TRIP);
    let part = to_wasi(&raw, &[26]).expect("to_wasi");
    let full = to_wasi(&raw, &[26, 29, 37]).expect("to_wasi");
    let (part_imports, part_defined) = shape(&part);
    let (full_imports, full_defined) = shape(&full);
    let want: Vec<&str> = BASE.iter().chain(ENVIRON_PAIR.iter()).copied().collect();
    expect_imports(&part_imports, &want);
    let want: Vec<&str> = BASE.iter().chain(ENVIRON_PAIR.iter()).chain(ARGS_PAIR.iter()).copied().collect();
    expect_imports(&full_imports, &want);
    assert_eq!(full_defined, part_defined + 2);
    assert!(full.len() > part.len() + 300, "args + env.set are ~0.5 KB: part {} B, full {} B", part.len(), full.len());
    // Both artifacts run: the shim selection never breaks validation.
    wasmparser::validate(&part).expect("part validates");
    wasmparser::validate(&full).expect("full validates");
    // Over hello, which never calls `fs_call`, the selected functions and
    // imports prune away (the service's data lines, which the op set gates
    // and the prune does not judge, stay — a real op set never names an op
    // the module does not call).
    let hello = self::raw("hello.almd", HELLO);
    assert_eq!(shape(&to_wasi(&hello, &[]).expect("to_wasi")), shape(&to_wasi(&hello, &[26, 29, 37]).expect("to_wasi")));
}

const FS_EXISTS: &str = r#"import fs

effect fn main() -> Unit = {
  println("${fs.exists("/")}")
}
"#;

/// The p1 fs service (#2742) is gated the same way: an fs op ships it, and
/// only the WASI calls the ops present reach — `fs.exists` is a stat after
/// the cwd join and the preopen lookup, so it earns neither `path_open` nor
/// any write or directory call.
#[test]
fn an_fs_op_ships_the_fs_service_with_only_the_imports_it_reaches() {
    let (wasm, host_ops) = artifact("fs_exists.almd", FS_EXISTS);
    assert!(P1Services::from_ops(&host_ops).fs, "fs.exists reaches the fs service: {host_ops:?}");
    let (imports, _) = shape(&wasm);
    let mut want = BASE.to_vec();
    want.extend(ENVIRON_PAIR);
    want.extend(["fd_prestat_get", "fd_prestat_dir_name", "path_filestat_get"]);
    expect_imports(&imports, &want);

    let (hello, host_ops) = artifact("hello.almd", HELLO);
    assert!(!P1Services::from_ops(&host_ops).fs);
    expect_imports(&shape(&hello).0, &["fd_write"]);
}

/// #3114: a base import ships only with the op whose shim arm calls it —
/// stdin (35) `fd_read`, entropy (32) `random_get`, the wall clock (34),
/// the monotonic clock (60) and sleep (36) `clock_time_get` — and an op set
/// that names none of them keeps the two-import surface. The module calls
/// `fs_call` (env.set), so the dispatcher and its arms are reached.
#[test]
fn each_served_arm_brings_only_its_own_import() {
    let raw = raw("env_set.almd", ENV_SET);
    for (ops, extra) in [
        (&[30][..], &[][..]),
        (&[35], &["fd_read"]),
        (&[32], &["random_get"]),
        (&[34], &["clock_time_get"]),
        (&[60], &["clock_time_get"]),
        (&[36], &["clock_time_get"]),
        (&[32, 34, 35], &["random_get", "clock_time_get", "fd_read"]),
    ] {
        let wasm = to_wasi(&raw, ops).expect("to_wasi");
        wasmparser::validate(&wasm).expect("validates");
        let want: Vec<&str> = BASE.iter().chain(extra.iter()).copied().collect();
        assert_eq!(shape(&wasm).0, want, "op set {ops:?}");
    }
}

/// The subprocess family (#2589, ADR-0025): the p1 artifact carries the
/// PRIVATE `almide:process/spawn.call` import — the one non-preview-1 import a
/// p1 artifact may carry — and exports `cabi_realloc`, only because the op
/// set names a process op; a stock runtime refuses such a module at load.
#[test]
fn process_program_ships_the_private_spawn_import_and_cabi_realloc() {
    const PROC: &str = r#"import process

effect fn main() -> Unit = {
  let s = process.exec_status("sh", ["-c", "printf hi"])!
  println(s.stdout)
}
"#;
    let (wasm, host_ops) = artifact("proc.almd", PROC);
    assert!(host_ops.contains(&83), "exec_status is op 83: {host_ops:?}");
    assert!(P1Services::from_ops(&host_ops).proc);
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    for payload in Parser::new(0).parse_all(&wasm) {
        match payload.expect("valid module") {
            Payload::ImportSection(r) => {
                for group in r {
                    for item in group.expect("imports") {
                        let (_, i) = item.expect("import row");
                        imports.push(format!("{}::{}", i.module, i.name));
                    }
                }
            }
            Payload::ExportSection(r) => exports.extend(r.into_iter().map(|e| e.expect("export").name.to_string())),
            _ => {}
        }
    }
    let private: Vec<&String> = imports.iter().filter(|i| !i.starts_with("wasi_snapshot_preview1::")).collect();
    assert_eq!(private, ["almide:process/spawn::call"], "{imports:?}");
    assert!(exports.iter().any(|e| e == "cabi_realloc"), "{exports:?}");
}
