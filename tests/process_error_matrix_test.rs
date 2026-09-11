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
//! Two deliberate exclusions, both asserted rather than assumed:
//!
//! - **The ran-and-failed path is not in this family.** A child that starts and
//!   then exits non-zero already passes its stderr through, and `exec` reports
//!   `process '{cmd}' exited with status {s}`. The issue calls this out as
//!   already-correct; a gate that swept it in would break working behaviour.
//! - **`exec_status` / `exec_status_timeout` keep `exec failed:`** — C-214's
//!   statement quotes it and the judge holds that ledger normatively, so
//!   unifying the twins is an almide/als PR first. That is ONE exception and
//!   `the_exception_list_does_not_grow` is what keeps it one.
//!
//! `process` has no wasm host binding (E081: no wasm host binding for the
//! module), so this is the only leg that renders these strings — there is no
//! cross-target equality to maintain here, which is exactly why this half of
//! #2090 could land while the `fs` half waits on the structural leg's WAT.

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

/// Why this family has ONE leg, asserted rather than assumed.
///
/// Every string above is rendered by the native runtime and by nothing else,
/// because `process` has no wasm host binding at all — so unlike the `fs` half
/// of #2090, there is no second implementation to keep byte-identical and no
/// contract to update. That is the whole reason this half could land alone.
///
/// If someone ports `process` to wasi-p3 (#1628), this test goes red, and that
/// is the point: the port acquires a second renderer for these messages and
/// C-215's rule starts applying to them.
#[test]
fn the_process_surface_has_no_second_leg_to_keep_equal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("wasmleg.almd");
    std::fs::write(
        &file,
        "import process\n\
         effect fn main() -> Unit = {\n\
        \x20 let _ = process.exec(\"ghh-not-a-binary\", [])!\n\
         }\n",
    )
    .expect("write fixture");
    let out = Command::new(almide())
        .arg("check")
        .arg(&file)
        .args(["--target", "wasm"])
        .output()
        .expect("run almide check");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("error[E081]") && text.contains("no wasm host binding"),
        "process grew a wasm leg — these messages now have a second renderer, so \
         the #2090 form has to be reproduced there and C-215 applies:\n{text}"
    );
}

/// The exception list is ONE entry long — the `exec_status` twins, held by
/// C-214. This test is what makes "we will unify them later" a commitment
/// instead of a comment: a new `exec failed:` site, or any other bare-errno
/// spawn failure, fails here.
#[test]
fn the_exception_list_does_not_grow() {
    let src = include_str!("../runtime/rs/src/process.rs");
    let n = src.matches("\"exec failed: {}\"").count();
    assert_eq!(
        n, 3,
        "the C-214 exception is exec_status (1 site) + exec_status_timeout (2 sites). \
         Found {n}. Unifying them is an almide/als statement PR first; ADDING one is \
         a new spelling and is what this gate refuses."
    );
    // No spawn-failure site may go back to a bare stringified io::Error.
    assert!(
        !src.contains(".map_err(|e| e.to_string())"),
        "a process.* failure went back to the anonymous `e.to_string()` form (#2090)"
    );
}
