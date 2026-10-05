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
            "let _ = process.exec_status(\"almide-no-such-binary-2090\", [])!",
            format!("process.exec_status({MISSING:?}):"),
        ),
        (
            "let _ = process.exec_status_timeout(\"almide-no-such-binary-2090\", [], 1000)!",
            format!("process.exec_status_timeout({MISSING:?}, 1000):"),
        ),
        (
            "let _ = process.run(\"almide-no-such-binary-2090\", [])!",
            format!("process.run({MISSING:?}):"),
        ),
        (
            "let _ = process.run_in(\".\", \"almide-no-such-binary-2090\", [])!",
            format!("process.run_in(\".\", {MISSING:?}):"),
        ),
        (
            "let _ = process.exec(\"almide-no-such-binary-2090\", [])!",
            format!("process.exec({MISSING:?}):"),
        ),
        (
            "let _ = process.exec_in(\".\", \"almide-no-such-binary-2090\", [])!",
            format!("process.exec_in(\".\", {MISSING:?}):"),
        ),
        (
            "let _ = process.exec_with_stdin(\"almide-no-such-binary-2090\", [], \"x\")!",
            format!("process.exec_with_stdin({MISSING:?}):"),
        ),
        (
            "let _ = process.spawn(\"almide-no-such-binary-2090\", [])!",
            format!("process.spawn({MISSING:?}):"),
        ),
    ]
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
    for (i, (call, _)) in rows().into_iter().enumerate() {
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
  match process.exec_status(cmd, ["private-argument-2103"]) {
    ok(_) => println("unexpected success"),
    err(e) => println(e),
  }
  match process.exec_status_timeout(cmd, ["private-argument-2103"], 1000) {
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
            "process.exec_status({quoted}): {host}\nprocess.exec_status_timeout({quoted}, 1000): {host}\n"));
    }
}

/// The subprocess family is extended by MATRIX, never point-wise (#3379:
/// `run_in` was asked for because `exec_in` had no terminal-attached twin,
/// and `exec_attached` was the one member C-374's list did not name).
///
/// The family is every fn stdlib/process.almd declares, minus the named
/// non-members below. Each member must have every cell:
/// - its `almide:process/spawn` WIT case (`exec_in` -> `exec-in`);
/// - its `__proc_<fn>` leaf in the emitter's op table (fs_meta.rs);
/// - its self-host registry row (the wasm legs);
/// - its stock-p1 `host-capability` row in proofs/target-availability.toml;
/// - when it starts a child: a `[permissions] proc` row (src/cli/mod.rs) and
///   a spawn-failure row in [`rows`] above, which the two-leg test runs.
/// The pairs `exec`/`run` and `exec_in`/`run_in` are the capture-versus-
/// terminal matrix; a cell missing on either axis fails here.
#[test]
fn the_subprocess_family_has_every_cell() {
    // Not in the family: process-wide state and the deprecated alias.
    const NON_MEMBERS: &[&str] = &["exit", "args", "stdin_lines", "env", "sleep", "exec_attached"];
    // Members that do not start a child.
    const NO_CHILD: &[&str] = &["kill", "is_alive", "pid"];
    let root = env!("CARGO_MANIFEST_DIR");
    let read = |p: &str| std::fs::read_to_string(format!("{root}/{p}")).unwrap_or_else(|e| panic!("{p}: {e}"));
    let stdlib = read("stdlib/process.almd");
    let wit = read("crates/almide-wasm-run/wit/process/spawn.wit");
    let leaves = read("crates/almide-wasm/src/fs_meta.rs");
    let registry = read("crates/almide-types/src/self_host_registry.rs");
    let availability = read("proofs/target-availability.toml");
    let permissions = read("src/cli/mod.rs");
    let failure_rows: Vec<String> = rows().into_iter().map(|(call, _)| call.to_string()).collect();

    let family: Vec<String> = stdlib
        .lines()
        .filter_map(|l| l.strip_prefix("effect fn ").or_else(|| l.strip_prefix("fn ")))
        .filter_map(|l| l.split('(').next())
        .map(str::to_string)
        .filter(|f| !NON_MEMBERS.contains(&f.as_str()))
        .collect();
    for pair in [["exec", "run"], ["exec_in", "run_in"]] {
        for f in pair {
            assert!(family.iter().any(|m| m == f), "the capture/terminal matrix lost `process.{f}`");
        }
    }

    let mut missing = Vec::new();
    for f in &family {
        let mut need = |ok: bool, cell: &str| {
            if !ok {
                missing.push(format!("  process.{f}: {cell}"));
            }
        };
        let case = f.replace('_', "-");
        need(wit.lines().any(|l| l.trim() == format!("{case},")), "WIT case in spawn.wit");
        need(leaves.contains(&format!("(\"__proc_{f}\", ")), "__proc_ leaf in fs_meta.rs PROC_LEAVES");
        need(registry.contains(&format!("\"process.{f}\")")), "self-host registry row");
        need(availability.contains(&format!("fn = \"process.{f}\"")), "target-availability row");
        if !NO_CHILD.contains(&f.as_str()) {
            need(permissions.contains(&format!("(\"{f}\", ")), "[permissions] proc row in src/cli/mod.rs");
            need(failure_rows.iter().any(|c| c.contains(&format!("process.{f}("))), "spawn-failure row in rows()");
        }
    }
    assert!(missing.is_empty(), "the subprocess family is missing cells:\n{}", missing.join("\n"));
}
