//! Pin for the p1 shim's reachability-gated services (#1841): the
//! environ/args imports and their shims ship ONLY when the module's
//! emitted op set reaches them — the #1712 fixed-slot discipline applied
//! to the `to_wasi` transform. Hello, world keeps the five base imports
//! (fd_write / proc_exit / random_get / clock_time_get / fd_read) and
//! the five base shims; `env.get` earns the environ pair, `env.args` the
//! args pair, `env.set` its overlay shim and no import at all.
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
  env.set("ALMIDE_PIN", "1")
  println("set")
}
"#;

const ENV_ROUND_TRIP: &str = r#"import env

effect fn main() -> Unit = {
  env.set("ALMIDE_PIN", "1")
  println(env.get("ALMIDE_PIN") ?? "(none)")
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

const BASE: [&str; 5] = ["fd_write", "proc_exit", "random_get", "clock_time_get", "fd_read"];
const ENVIRON_PAIR: [&str; 2] = ["environ_sizes_get", "environ_get"];
const ARGS_PAIR: [&str; 2] = ["args_sizes_get", "args_get"];

fn expect_imports(got: &[String], want: &[&str]) {
    let got: Vec<&str> = got.iter().map(String::as_str).collect();
    assert_eq!(got, want, "p1 import surface");
}

/// The base shim count: the five service-independent shims the
/// transform appends to every artifact.
fn base_shims(wasm_bytes_before: &[u8]) -> usize {
    let ir_defined = Parser::new(0)
        .parse_all(wasm_bytes_before)
        .filter_map(|p| match p.expect("valid module") {
            Payload::FunctionSection(r) => Some(r.count() as usize),
            _ => None,
        })
        .next()
        .unwrap_or(0);
    ir_defined + 5
}

#[test]
fn hello_ships_no_environ_or_args_service() {
    let (wasm, host_ops) = artifact("hello.almd", HELLO);
    assert_eq!(P1Services::from_ops(&host_ops), P1Services { env_get: false, env_set: false, args: false });
    let (imports, defined) = shape(&wasm);
    expect_imports(&imports, &BASE);
    assert!(
        !imports.iter().any(|n| n.starts_with("environ_") || n.starts_with("args_")),
        "hello must not import the environ/args quartet: {imports:?}"
    );
    // Five base shims and nothing behind them: the emitted module's own
    // functions plus println/eprintln/exit/fs_call/host_read.
    let ir = almide_spine::s5::lower_to_ir("hello.almd", HELLO).expect("lowers");
    let raw = almide_wasm::emit_program(&ir).expect("emits");
    assert_eq!(defined, base_shims(&raw), "hello carries exactly the five base shims");
}

#[test]
fn args_program_ships_the_args_pair_only() {
    let (wasm, host_ops) = artifact("args.almd", ARGS);
    assert_eq!(P1Services::from_ops(&host_ops), P1Services { env_get: false, env_set: false, args: true });
    let (imports, defined) = shape(&wasm);
    let want: Vec<&str> = BASE.iter().chain(ARGS_PAIR.iter()).copied().collect();
    expect_imports(&imports, &want);
    let ir = almide_spine::s5::lower_to_ir("args.almd", ARGS).expect("lowers");
    let raw = almide_wasm::emit_program(&ir).expect("emits");
    assert_eq!(defined, base_shims(&raw) + 1, "the args frames shim, alone");
}

#[test]
fn env_get_program_ships_the_environ_pair_only() {
    let (wasm, host_ops) = artifact("env_get.almd", ENV_GET);
    assert_eq!(P1Services::from_ops(&host_ops), P1Services { env_get: true, env_set: false, args: false });
    let (imports, defined) = shape(&wasm);
    let want: Vec<&str> = BASE.iter().chain(ENVIRON_PAIR.iter()).copied().collect();
    expect_imports(&imports, &want);
    let ir = almide_spine::s5::lower_to_ir("env_get.almd", ENV_GET).expect("lowers");
    let raw = almide_wasm::emit_program(&ir).expect("emits");
    assert_eq!(defined, base_shims(&raw) + 1, "the environ scan shim, alone");
}

#[test]
fn env_set_program_ships_the_overlay_shim_and_no_import() {
    let (wasm, host_ops) = artifact("env_set.almd", ENV_SET);
    assert_eq!(P1Services::from_ops(&host_ops), P1Services { env_get: false, env_set: true, args: false });
    let (imports, defined) = shape(&wasm);
    expect_imports(&imports, &BASE);
    let ir = almide_spine::s5::lower_to_ir("env_set.almd", ENV_SET).expect("lowers");
    let raw = almide_wasm::emit_program(&ir).expect("emits");
    assert_eq!(defined, base_shims(&raw) + 1, "the overlay-append shim, alone");
}

#[test]
fn full_env_surface_ships_the_whole_quartet() {
    let (wasm, host_ops) = artifact("env_round_trip.almd", ENV_ROUND_TRIP);
    assert_eq!(P1Services::from_ops(&host_ops), P1Services { env_get: true, env_set: true, args: true });
    let (imports, defined) = shape(&wasm);
    let want: Vec<&str> = BASE.iter().chain(ENVIRON_PAIR.iter()).chain(ARGS_PAIR.iter()).copied().collect();
    expect_imports(&imports, &want);
    let ir = almide_spine::s5::lower_to_ir("env_round_trip.almd", ENV_ROUND_TRIP).expect("lowers");
    let raw = almide_wasm::emit_program(&ir).expect("emits");
    assert_eq!(defined, base_shims(&raw) + 3, "env_get + env_set + args shims");
}

/// The gate is a SELECTION over the op table: the same bytes with a
/// different op set produce a different import surface, and the two
/// disjoint selections differ by exactly the quartet.
#[test]
fn the_gate_selects_from_the_op_table() {
    let ir = almide_spine::s5::lower_to_ir("hello.almd", HELLO).expect("lowers");
    let raw = almide_wasm::emit_program(&ir).expect("emits");
    let bare = to_wasi(&raw, &[]).expect("to_wasi");
    let full = to_wasi(&raw, &[26, 29, 37]).expect("to_wasi");
    let (bare_imports, bare_defined) = shape(&bare);
    let (full_imports, full_defined) = shape(&full);
    expect_imports(&bare_imports, &BASE);
    let want: Vec<&str> = BASE.iter().chain(ENVIRON_PAIR.iter()).chain(ARGS_PAIR.iter()).copied().collect();
    expect_imports(&full_imports, &want);
    assert_eq!(full_defined, bare_defined + 3);
    assert!(full.len() > bare.len() + 900, "the quartet is ~1 KB: bare {} B, full {} B", bare.len(), full.len());
    // Both artifacts run: the shim selection never breaks validation.
    wasmparser::validate(&bare).expect("bare validates");
    wasmparser::validate(&full).expect("full validates");
}
