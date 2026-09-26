//! #2655: an `if` whose arms are both `some(cut(string.drop(t, 1), ","))` — a
//! user call whose arguments materialize heap temps, wrapped in `some` — trapped
//! on the INCUMBENT wasm leg.
//!
//! `arm_some_heap_piece`'s user-call case lowered the call's arguments without
//! an arm frame, so the `string.drop` result and the literal stayed on the
//! function's live-handle list and the EPILOGUE released them — after the
//! `if`, for both arms, whichever ran. The untaken arm's locals were never
//! assigned, so the epilogue `rc_dec`'d address 0 and hit the refcount trap.
//! The module-call and list-concat cases one screen below always had the frame.
//!
//! The report came through `almide test --target wasm`, which before #2179
//! rendered every test file on the incumbent; `main` on `almide run --target
//! wasm` took the structural leg and passed. Since #2179 the test lane routes
//! structural-first too, so the default route no longer reaches the defect —
//! but the incumbent still renders whatever the structural leg declines (#2658
//! is such a program), so this pins the incumbent leg directly, on both the
//! run and the test lane, against native.

use std::path::PathBuf;
use std::process::Command;

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("spec/wasm_cross/heap_result_if_some_user_call_temps.almd")
}

fn run(args: &[&str], incumbent: bool) -> std::process::Output {
    let mut c = Command::new(almide_bin());
    c.args(args).env_remove("ALMIDE_COMPONENT_P3");
    if incumbent {
        c.env("ALMIDE_WASM_INCUMBENT", "1");
    } else {
        c.env_remove("ALMIDE_WASM_INCUMBENT");
    }
    c.output().expect("spawn almide")
}

fn text(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string() + &String::from_utf8_lossy(&o.stderr)
}

#[test]
fn the_incumbent_run_agrees_with_native() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    let path = fixture();
    let path = path.to_str().expect("path");
    let native = run(&["run", path], false);
    assert!(native.status.success(), "native run failed:\n{}", text(&native));
    let wasm = run(&["run", path, "--target", "wasm"], true);
    assert!(
        wasm.status.success(),
        "the incumbent wasm leg must not trap on the untaken arm's temps:\n{}",
        text(&wasm)
    );
    assert_eq!(
        String::from_utf8_lossy(&wasm.stdout),
        String::from_utf8_lossy(&native.stdout),
        "the incumbent wasm leg answered differently from native"
    );
}

#[test]
fn the_incumbent_test_lane_runs_the_test_block() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    let path = fixture();
    let out = run(&["test", path.to_str().expect("path"), "--target", "wasm"], true);
    let report = text(&out);
    assert_eq!(out.status.code(), Some(0), "the test lane must pass on the incumbent:\n{report}");
    assert!(report.contains("1 tests passed"), "and report the test as run there:\n{report}");
}
