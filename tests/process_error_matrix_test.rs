//! The `process.*` failure surface is a FAMILY with one rendered form (#2090).
//!
//! The defect the issue reports is one symptom of a surface that had grown
//! **seven** spellings of "this host call failed":
//!
//! | site | before |
//! |---|---|
//! | `exec` (spawn) | `{errno}` |
//! | `exec_in` | `{errno}` |
//! | `exec_with_stdin` | `{errno}` |
//! | `stdin_lines` | `{errno}` |
//! | `exec_status` | `exec failed: {errno}` |
//! | `exec_status_timeout` | `exec failed: {errno}` |
//! | `spawn` | `spawn '{cmd}' failed: {errno}` |
//!
//! Since R-process-02 the family is `exec`, `output`, `run` and `start`
//! (one `ProcessCommand` record each but `exec`).
//!
//! Four said nothing at all, and the three that said something each said it
//! differently. Fixing only the one in the repro would leave six. So the rule is
//! stated here and gated, per the family discipline in CLAUDE.md:
//!
//! > Every `process.*` intrinsic that can fail to START its command renders
//! > `process.<fn>(<operands>): <platform text>` — the call as the writer spelled
//! > it, its identifying operands, then the platform's own text VERBATIM as a
//! > suffix.
//!
//! A child that starts and exits non-zero retains its own stderr/status.
//! C-214 now includes both status twins after almide/als#65 merged.
//!
//! Since #2589 (ADR-0025) the embedded wasm host renders these strings too —
//! with the SAME code: `runtime/rs/src/process.rs` includes
//! `crates/almide-rt-core/src/process_core.rs`, and the host's
//! `almide:process/spawn` calls it. The last test below holds the two legs
//! byte-equal over every row.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn run(dir: &std::path::Path, name: &str, source: &str) -> String {
    let file = dir.join(name);
    std::fs::write(&file, source).expect("write fixture");
    let out = Command::new(almide())
        .args(["run", file.to_str().unwrap()])
        .output()
        .expect("run almide");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A row is: the call as written in Almide, and the prefix its failure must
/// carry. `MISSING` is a command that cannot exist, so every row fails at the
/// spawn — the one path this family covers.
const MISSING: &str = "almide-no-such-binary-2090";

fn rows() -> Vec<(&'static str, String)> {
    vec![
        (
            "let _ = process.output(process.ProcessCommand { cmd: \"almide-no-such-binary-2090\", args: [] })!",
            format!("process.output({MISSING:?}):"),
        ),
        (
            "let _ = process.output(process.ProcessCommand { cmd: \"almide-no-such-binary-2090\", args: [], timeout_ms: 1000 })!",
            format!("process.output({MISSING:?}):"),
        ),
        (
            "let _ = process.output(process.ProcessCommand { cmd: \"almide-no-such-binary-2090\", args: [], dir: \".\" })!",
            format!("process.output(\".\", {MISSING:?}):"),
        ),
        (
            "let _ = process.output(process.ProcessCommand { cmd: \"almide-no-such-binary-2090\", args: [], stdin: \"x\" })!",
            format!("process.output({MISSING:?}):"),
        ),
        (
            "let _ = process.run(process.ProcessCommand { cmd: \"almide-no-such-binary-2090\", args: [] })!",
            format!("process.run({MISSING:?}):"),
        ),
        (
            "let _ = process.run(process.ProcessCommand { cmd: \"almide-no-such-binary-2090\", args: [], dir: \".\" })!",
            format!("process.run(\".\", {MISSING:?}):"),
        ),
        (
            "let _ = process.exec(\"almide-no-such-binary-2090\", [])!",
            format!("process.exec({MISSING:?}):"),
        ),
        (
            "let _ = process.start(process.ProcessCommand { cmd: \"almide-no-such-binary-2090\", args: [] })!",
            format!("process.start({MISSING:?}):"),
        ),
    ]
}

/// The rows the embedded wasm host serves: `start`'s handle is native-only
/// (E081 on the wasm legs, proofs/target-availability.toml).
fn wasm_rows() -> Vec<(&'static str, String)> {
    rows().into_iter().filter(|(c, _)| !c.contains("process.start(")).collect()
}

/// Every row in the family names itself, and keeps the platform text verbatim.
#[test]
fn every_spawn_failure_names_its_call_and_operand() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut broken = Vec::new();
    for (i, (call, want)) in rows().into_iter().enumerate() {
        let out = run(
            dir.path(),
            &format!("row{i}.almd"),
            &format!("import process\neffect fn main() -> Unit = {{\n  {call}\n}}\n"),
        );
        if !out.contains(&want) {
            broken.push(format!("  {call}\n    want prefix: {want}\n    got: {}", out.trim()));
        }
        // The errno tail is the half that must NOT change: C-215's
        // classification and #1368's errno-carrying fix both read it.
        if !out.contains("(os error 2)") {
            broken.push(format!("  {call}\n    lost the verbatim errno tail: {}", out.trim()));
        }
    }
    assert!(
        broken.is_empty(),
        "the process.* failure family is not rendering one form:\n{}",
        broken.join("\n")
    );
}

/// Distinctness is the point of the issue: these must not collapse onto each
/// other the way `process.exec` and `fs.read_text` did.
#[test]
fn the_family_members_are_distinguishable_from_each_other() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut seen: Vec<(String, String)> = Vec::new();
    for (i, (call, _)) in rows().into_iter().enumerate() {
        let out = run(
            dir.path(),
            &format!("d{i}.almd"),
            &format!("import process\neffect fn main() -> Unit = {{\n  {call}\n}}\n"),
        );
        let out = out.trim().to_string();
        if let Some((prev, _)) = seen.iter().find(|(_, o)| *o == out) {
            panic!("`{call}` and `{prev}` render identically:\n{out}");
        }
        seen.push((call.to_string(), out));
    }
}

/// A child that STARTS and then fails is a different thing and keeps its own
/// shape — it already names the command and passes stderr through. The gate
/// above must not have swept it in.
#[test]
fn a_child_that_ran_and_failed_is_untouched() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = run(
        dir.path(),
        "ran.almd",
        "import process\n\
         effect fn main() -> Unit = {\n\
        \x20 let _ = process.exec(\"false\", [])!\n\
         }\n",
    );
    assert!(
        out.contains("exited with status"),
        "the ran-and-failed path lost its own shape:\n{out}"
    );
    assert!(
        !out.contains("process.exec(\"false\"):"),
        "the spawn-failure form leaked onto a process that actually ran:\n{out}"
    );
}

/// The second leg (#2589): the embedded wasm host renders every row of the
/// family byte for byte as native does — stdout, stderr and exit code — since
/// both run one core. This test used to assert the opposite (that `process`
/// had no wasm leg), as the tripwire for exactly this moment.
#[test]
fn the_embedded_wasm_leg_renders_every_row_as_native_does() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut diverged = Vec::new();
    for (i, (call, _)) in wasm_rows().into_iter().enumerate() {
        let file = dir.path().join(format!("leg{i}.almd"));
        std::fs::write(&file, format!("import process\neffect fn main() -> Unit = {{\n  {call}\n}}\n")).expect("write fixture");
        let leg = |extra: &[&str]| {
            let out = Command::new(almide()).arg("run").arg(&file).args(extra).output().expect("run almide");
            (out.status.code(), String::from_utf8_lossy(&out.stdout).to_string(), String::from_utf8_lossy(&out.stderr).to_string())
        };
        let (native, wasm) = (leg(&[]), leg(&["--target", "wasm"]));
        if native != wasm {
            diverged.push(format!("  {call}\n    native: {native:?}\n    wasm:   {wasm:?}"));
        }
    }
    assert!(diverged.is_empty(), "the process.* failure family diverges between native and the embedded host:\n{}", diverged.join("\n"));
}

/// The status twins joined the family after the normative amendment.
#[test]
fn no_anonymous_process_failure_remains() {
    // The family's bodies live in the shared core since #2589.
    for src in [include_str!("../runtime/rs/src/process.rs"), include_str!("../crates/almide-rt-core/src/process_core.rs")] {
        assert!(!src.contains("exec failed:"));
        assert!(!src.contains(".map_err(|e| e.to_string())"));
    }
}

#[test]
fn status_errors_escape_commands_and_omit_arguments() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("quoted.almd");
    let app = dir.path().join("quoted");
    std::fs::write(&file, r#"import process
import env
effect fn main() -> Unit = {
  let cmd = env.args()[0]
  match process.output(process.ProcessCommand { cmd: cmd, args: ["private-argument-2103"] }) {
    ok(_) => println("unexpected success"),
    err(e) => println(e),
  }
  match process.output(process.ProcessCommand { cmd: cmd, args: ["private-argument-2103"], timeout_ms: 1000 }) {
    ok(_) => println("unexpected success"),
    err(e) => println(e),
  }
}
"#).expect("fixture");
    let built = Command::new(almide()).arg("build").arg(&file).arg("-o").arg(&app).output().expect("build");
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    for (cmd, quoted) in [
        ("missing-2103-\"quote", "\"missing-2103-\\\"quote\""),
        ("missing-2103-\\slash", "\"missing-2103-\\\\slash\""),
        ("missing-2103-\n\r\tcontrol", "\"missing-2103-\\n\\r\\tcontrol\""),
        ("missing-2103-日本語", "\"missing-2103-日本語\""),
    ] {
        let host = Command::new(cmd).arg("private-argument-2103").output().expect_err("missing command").to_string();
        let output = Command::new(&app).arg(cmd).output().expect("run");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), format!(
            "process.output({quoted}): {host}\nprocess.output({quoted}): {host}\n"));
    }
}

/// The subprocess family is extended by MATRIX, never point-wise (#3379,
/// R-process-02/03). Two groups, read off stdlib/process.almd:
/// - the one-shot runs and the process-wide calls the wasm legs serve —
///   `exec`, `output`, `run`, `kill`, `pid` — each with its
///   `almide:process/spawn` WIT case, its `__proc_*` leaf (fs_meta.rs) and
///   the self-host registry row of the surface it implements;
/// - the `start` handle — `start`, `poll`, `wait`, `stop`, `read_new_stdout`,
///   `read_new_stderr`, `child_pid` — native-only, each declared unavailable
///   on the wasm legs in proofs/target-availability.toml (E081).
/// Every fn that starts a child has a `[permissions] proc` row (src/cli/mod.rs)
/// and a spawn-failure row in [`rows`].
#[test]
fn the_subprocess_family_has_every_cell() {
    const NON_MEMBERS: &[&str] = &["exit", "args", "stdin_lines", "env", "sleep"];
    // (fn, WIT case, `__proc_` leaf, the registry surface its impl serves)
    const SERVED: &[(&str, &str, &str, &str)] = &[
        ("exec", "exec", "exec", "process.exec"),
        ("output", "output", "output", "process.__os_output"),
        ("run", "run-with", "run_cmd", "process.__os_run"),
        ("kill", "kill", "kill", "process.kill"),
        ("pid", "pid", "pid", "process.pid"),
    ];
    const HANDLE: &[&str] = &["start", "poll", "wait", "stop", "read_new_stdout", "read_new_stderr", "child_pid"];
    const STARTS_A_CHILD: &[&str] = &["exec", "output", "run", "start"];
    let root = env!("CARGO_MANIFEST_DIR");
    let read = |p: &str| std::fs::read_to_string(format!("{root}/{p}")).unwrap_or_else(|e| panic!("{p}: {e}"));
    let stdlib = read("stdlib/process.almd");
    let wit = read("crates/almide-wasm-run/wit/process/spawn.wit");
    let leaves = read("crates/almide-wasm/src/fs_meta.rs");
    let registry = read("crates/almide-types/src/self_host_registry.rs");
    let availability = read("proofs/target-availability.toml");
    let permissions = read("src/cli/mod.rs");
    let failure_rows: Vec<String> = rows().into_iter().map(|(call, _)| call.to_string()).collect();

    let public: Vec<String> = stdlib
        .lines()
        .filter_map(|l| l.strip_prefix("effect fn ").or_else(|| l.strip_prefix("fn ")))
        .filter_map(|l| l.split('(').next())
        .map(str::to_string)
        .filter(|f| !f.starts_with("__") && !NON_MEMBERS.contains(&f.as_str()))
        .collect();
    let mut missing = Vec::new();
    for f in &public {
        if !SERVED.iter().any(|(n, ..)| n == f) && !HANDLE.contains(&f.as_str()) {
            missing.push(format!("  process.{f}: in neither the served group nor the handle"));
        }
    }
    for (f, case, leaf, surface) in SERVED {
        if !wit.lines().any(|l| l.trim() == format!("{case},")) {
            missing.push(format!("  process.{f}: WIT case `{case}` in spawn.wit"));
        }
        if !leaves.contains(&format!("(\"__proc_{leaf}\", ")) {
            missing.push(format!("  process.{f}: __proc_{leaf} in fs_meta.rs PROC_LEAVES"));
        }
        if !registry.contains(&format!("\"{surface}\")")) {
            missing.push(format!("  process.{f}: self-host registry row for {surface}"));
        }
    }
    for f in HANDLE {
        if !availability.contains(&format!("fn = \"process.{f}\"")) {
            missing.push(format!("  process.{f}: target-availability row (native-only, E081)"));
        }
    }
    for f in STARTS_A_CHILD {
        if !permissions.contains(&format!("(\"{f}\", ")) {
            missing.push(format!("  process.{f}: [permissions] proc row in src/cli/mod.rs"));
        }
        if !failure_rows.iter().any(|c| c.contains(&format!("process.{f}("))) {
            missing.push(format!("  process.{f}: spawn-failure row in rows()"));
        }
    }
    assert!(missing.is_empty(), "the subprocess family is missing cells:\n{}", missing.join("\n"));
}
