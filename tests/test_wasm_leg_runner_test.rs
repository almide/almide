//! `almide test`'s wasm leg runs on the embedded host (#3046).
//!
//! It used to spawn the `wasmtime` CLI. Where that binary was missing (a fresh
//! CI runner, a container) every file fell to the native fallback and the
//! summary count was the only trace. For a package with heavy `[native-deps]`
//! that looked like a hang. The leg now runs on the host every `almide` binary
//! carries, the one `almide run --target wasm` uses. The CLI route stays
//! reachable (`ALMIDE_TEST_WASM_RUNNER=wasmtime`), and when it cannot start, a
//! one-line note names the cause once per run.

use std::path::Path;
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn write(dir: &Path, name: &str, src: &str) {
    std::fs::write(dir.join(name), src).unwrap();
}

/// `almide test --target wasm <dir>` with `PATH` pointing at an empty
/// directory: no `wasmtime` (or anything else) to spawn.
fn test_wasm_without_path(dir: &Path, runner: Option<&str>) -> (bool, String) {
    let empty = dir.join("empty-path");
    std::fs::create_dir_all(&empty).unwrap();
    let mut cmd = Command::new(almide());
    cmd.env("PATH", &empty).args(["test", "--target", "wasm"]).arg(dir.join("t"));
    match runner {
        Some(r) => cmd.env("ALMIDE_TEST_WASM_RUNNER", r),
        None => cmd.env_remove("ALMIDE_TEST_WASM_RUNNER"),
    };
    let out = cmd.output().expect("spawn almide test");
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

fn two_files() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let t = dir.path().join("t");
    std::fs::create_dir_all(&t).unwrap();
    write(&t, "a_test.almd", "test \"one\" { assert_eq(1 + 1, 2) }\n");
    write(&t, "b_test.almd", "test \"two\" { assert_eq(\"ab\" + \"c\", \"abc\") }\n");
    dir
}

#[test]
fn the_wasm_leg_needs_no_wasmtime_on_path() {
    let dir = two_files();
    let (ok, out) = test_wasm_without_path(dir.path(), None);
    assert!(ok, "the wasm leg did not run without `wasmtime` on PATH:\n{out}");
    assert!(out.contains("2 passed, 0 failed"), "both files must pass ON wasm:\n{out}");
    assert!(!out.contains("note: the wasm leg could not start"), "the embedded host always starts:\n{out}");
}

#[test]
fn a_leg_that_cannot_start_is_named_once_per_run() {
    let dir = two_files();
    let (_, out) = test_wasm_without_path(dir.path(), Some("wasmtime"));
    let note = "note: the wasm leg could not start for 2 file(s) (ALMIDE_TEST_WASM_RUNNER=wasmtime, and the `wasmtime` CLI did not start";
    assert_eq!(out.matches("note: the wasm leg could not start").count(), 1, "one note per run and cause:\n{out}");
    assert!(out.contains(note), "the note names the cause and the file count:\n{out}");
}

/// The embedded host serves `env.set` from an overlay. It used to be
/// process-wide, so in one `almide test` process a file's `env.set` leaked
/// into every file that ran after it. Natively each test file is its own
/// process. Two runs on ONE thread, in order: the second must not see the
/// first's variable.
#[test]
fn env_set_does_not_leak_from_one_run_into_the_next() {
    let run = |src: &str| {
        let ir = almide::wasm_leg::lower_to_ir_tests_with_deps("t.almd", src, &[], None).expect("lowers on the wasm leg");
        let (bytes, _) = almide_wasm::emit_program_with_ops(&ir).expect("emits on the wasm leg");
        almide_wasm_run::run_wasm_unbounded(&bytes).expect("the embedded host runs the module")
    };
    let setter = "import env\ntest \"set\" {\n  env.set(\"ALMIDE_T3046_LEAK\", \"leaked\")\n  assert_eq(env.get(\"ALMIDE_T3046_LEAK\") ?? \"none\", \"leaked\")\n}\n";
    let reader = "import env\ntest \"get\" { assert_eq(env.get(\"ALMIDE_T3046_LEAK\") ?? \"none\", \"none\") }\n";
    let first = run(setter);
    assert_eq!(first.exit, 0, "the setter sees its own value:\n{}{}", first.stdout, first.stderr);
    let second = run(reader);
    assert_eq!(second.exit, 0, "the next run saw the previous run's env.set:\n{}{}", second.stdout, second.stderr);
}
