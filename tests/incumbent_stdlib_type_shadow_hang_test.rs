//! A user type named after a stdlib-owned type must not re-layout the stdlib's
//! own type on the incumbent wasm leg (#2567, C-076).
//!
//! `spec/wasm_cross/stdlib_type_shadow.almd` declares `type Value = { n: Int }`
//! beside `json.parse`. Native and the structural leg print the same lines; the
//! incumbent leg (`ALMIDE_WASM_INCUMBENT=1`) never terminated — wasmtime at 99%
//! CPU for hours inside sweeps that had no per-run limit. The incumbent's layout
//! registry aliased the user's `self.Value` (#1828) to the bare name `Value`, and
//! its unique-suffix resolver did the same, so the linked json parser's
//! `List[Value]` accumulator lowered against the user's `{ n: Int }` record: its
//! drops walked an Int as a handle and the loop never came back.
//!
//! WHY THIS BINARY EXISTS ALONGSIDE THE FIXTURE: the corpus gates run the fixture
//! on the STRUCTURAL leg and only re-render it through the incumbent for host
//! determinism (bytes compared, never executed). The output-parity gate does run
//! the incumbent's module, and now counts a deadline as HANG and fails — but its
//! oracle comparison goes through `render_program`, a different driver. Here the
//! product's own route is run, under a process-group deadline, and compared with
//! native — which is what hung before the fix.
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const FIXTURE: &str = "spec/wasm_cross/stdlib_type_shadow.almd";
const DEADLINE: Duration = Duration::from_secs(90);

fn almide() -> String {
    std::env::var("ALMIDE_BIN")
        .unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

/// Run one `almide` invocation in its own process group under a deadline. On
/// the deadline the WHOLE group is killed — `almide run --target wasm` spawns
/// `wasmtime` as a child, and killing only the parent would leave the hung
/// module running at full CPU, exactly the residue the sweeps left behind.
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
                // kill(1) with a NEGATIVE pid signals the process group.
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

#[test]
fn the_incumbent_leg_terminates_on_the_stdlib_type_shadow_fixture_and_agrees_with_native() {
    let (nrc, native, nerr) = run_capped(&["run", FIXTURE], false).expect("native leg finishes");
    assert_eq!(nrc, 0, "native is the oracle here and must run clean:\n{nerr}");
    assert!(
        native.contains("14 big 21 true Value { n: 7 }") && native.contains("{\"a\":1} 3"),
        "the native oracle prints the fixture's stated lines:\n{native}"
    );
    let (wrc, incumbent, werr) = run_capped(&["run", FIXTURE, "--target", "wasm"], true)
        .unwrap_or_else(|hang| panic!("{hang}"));
    assert_eq!(
        wrc, 0,
        "the incumbent leg must run the fixture clean (a user `Value` record beside json.parse):\n{werr}"
    );
    assert_eq!(
        incumbent, native,
        "the incumbent wasm leg must print what native prints — the user's `self.Value` \
         layout must never stand in for the builtin `Value` the linked json parser carries"
    );
}
