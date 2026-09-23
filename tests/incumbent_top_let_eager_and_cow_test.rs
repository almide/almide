//! The incumbent wasm leg agrees with native on the two fixtures #2571 found it
//! silently disagreeing on — through the PRODUCT route, under a deadline.
//!
//! `spec/wasm_cross/mut_param_alias_cow.almd` (C-033, C-132): an alias bound
//! before a `mut`-parameter call keeps its pre-write bytes. The incumbent's
//! callee wrote straight through the caller's block (`held=[1, 9, 3]` where
//! native prints `held=[1, 2, 3]`); its C-132 write-back bind now takes the
//! same rc-gated copy-on-write the structural leg takes at the call site.
//!
//! `spec/wasm_cross/top_let_closure_eager.almd` (C-007): an abortable top-level
//! `let` evaluates at startup. The incumbent inlined the pure call-bearing init
//! at its use and printed `before use` before trapping; its `__global_init`
//! runner now re-evaluates such an init's reachable `/` and `%` before `main`.
//!
//! WHY THIS BINARY EXISTS ALONGSIDE THE BASELINE ROWS: `proofs/output-parity.sh`
//! holds both fixtures on the gate's `render_program` driver; this test runs the
//! product's own incumbent switch (`ALMIDE_WASM_INCUMBENT=1 almide run --target
//! wasm`) in its own process group under a deadline and compares the full
//! observable — stdout, exit code, and the program's stderr — with native.
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(90);

fn almide() -> String {
    std::env::var("ALMIDE_BIN")
        .unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

/// Run one `almide` invocation in its own process group under a deadline. On
/// the deadline the WHOLE group is killed — `almide run --target wasm` spawns
/// `wasmtime` as a child, and killing only the parent would leave a hung
/// module running at full CPU.
fn run_capped(args: &[&str], incumbent: bool) -> Result<(i32, String, String), String> {
    let mut cmd = Command::new(almide());
    cmd.args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    if incumbent {
        cmd.env("ALMIDE_WASM_INCUMBENT", "1");
    }
    let mut child = cmd.spawn().expect("spawn almide");
    let pid = child.id();
    let started = Instant::now();
    loop {
        match child.try_wait().expect("wait on almide") {
            Some(_) => break,
            None if started.elapsed() > DEADLINE => {
                let _ = Command::new("kill").args(["-9", &format!("-{pid}")]).status();
                let _ = child.wait();
                return Err(format!(
                    "`almide {}`{} did not finish within {:?} — the leg HUNG (its process group was killed)",
                    args.join(" "),
                    if incumbent { " (ALMIDE_WASM_INCUMBENT=1)" } else { "" },
                    DEADLINE
                ));
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    let out = child.wait_with_output().expect("collect almide output");
    Ok((
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    ))
}

/// The program's own stderr lines: the incumbent-switch notice and wasmtime's
/// trap preamble/backtrace are host infrastructure, not a program observable
/// (the same normalization `proofs/output-parity.sh` applies).
fn program_stderr(raw: &str) -> Vec<String> {
    raw.lines()
        .filter(|l| l.starts_with("Error: ") && !l.starts_with("Error: failed to run main module"))
        .map(str::to_string)
        .collect()
}

fn incumbent_agrees_with_native(fixture: &str, expect_exit: i32, stdout_holds: &str) {
    let (nrc, native, nerr) = run_capped(&["run", fixture], false).expect("native leg finishes");
    assert_eq!(nrc, expect_exit, "native is the oracle and exits {expect_exit} on {fixture}:\n{nerr}");
    assert!(
        native.contains(stdout_holds),
        "the native oracle's stdout on {fixture} holds {stdout_holds:?}:\n{native}"
    );
    let (wrc, incumbent, werr) = run_capped(&["run", fixture, "--target", "wasm"], true)
        .unwrap_or_else(|hang| panic!("{hang}"));
    assert_eq!(
        (wrc, incumbent.as_str()),
        (nrc, native.as_str()),
        "the incumbent wasm leg must exit as native exits and print what native prints on {fixture}\n\
         incumbent stderr:\n{werr}"
    );
    assert_eq!(
        program_stderr(&werr),
        program_stderr(&nerr),
        "the incumbent wasm leg must raise the error native raises on {fixture}\n\
         incumbent stderr:\n{werr}"
    );
}

#[test]
fn the_incumbent_leg_keeps_an_alias_bound_before_a_mut_param_call_intact() {
    incumbent_agrees_with_native(
        "spec/wasm_cross/mut_param_alias_cow.almd",
        0,
        "A.b=[1, 9, 3] held=[1, 2, 3]\n",
    );
}

#[test]
fn the_incumbent_leg_aborts_on_an_abortable_closure_holding_top_let_before_main_prints() {
    // Native prints NOTHING: the abort is at startup, before `before use`.
    let (nrc, native, _) = run_capped(&["run", "spec/wasm_cross/top_let_closure_eager.almd"], false)
        .expect("native leg finishes");
    assert_eq!((nrc, native.as_str()), (1, ""), "the native oracle aborts at startup with empty stdout");
    incumbent_agrees_with_native("spec/wasm_cross/top_let_closure_eager.almd", 1, "");
}
