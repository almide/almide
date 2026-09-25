//! The silent half of the two-way repair ratchet (#2149).
//!
//! `tests/diagnostics/<case>/` pins what the compiler must SAY about a wrong
//! program. `tests/diagnostics/silent/*.almd` pins the opposite: correct
//! programs written in the shapes that have drawn a wrong or misdirecting
//! diagnostic before — a local binding named like an imported module
//! (#2345), the same-stem sibling used correctly (#2097), a Result binding in
//! a lambda body that says what it does with failure (#2254), a real call
//! under `!` (#2096), `?!` in an Option fn (#2607), calls inside `${}`
//! (#2093/#2095), imports used only through a bare type (#1853), and the
//! near-miss of each mechanical fix-it (E031/E043/E049/E060/E062 …).
//!
//! Every file must check with NO diagnostic at all — not an error, not a
//! warning. A new diagnostic on any of them is a FAIL: that is how the family
//! "plausible rejection / misdirected hint on a valid program" is caught as a
//! family instead of one issue at a time. The header comment of each file
//! names the shape it guards.
//!
//! Adding a case: write the program, confirm `almide check --json <file>`
//! prints nothing and exits 0, and raise `SILENT_FLOOR` to the new count.

use std::path::PathBuf;
use std::process::Command;

/// Shrink-only in spirit: lowering it needs a reason in the PR body.
const SILENT_FLOOR: usize = 34;

fn silent_cases() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/diagnostics/silent");
    let mut cases: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("read tests/diagnostics/silent")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "almd"))
        .collect();
    cases.sort();
    cases
}

#[test]
fn silent_programs_check_with_no_diagnostic() {
    let cases = silent_cases();
    assert!(
        cases.len() >= SILENT_FLOOR,
        "silent cases dropped to {} (floor {})",
        cases.len(),
        SILENT_FLOOR
    );
    let mut loud: Vec<String> = Vec::new();
    for case in &cases {
        let out = Command::new(env!("CARGO_BIN_EXE_almide"))
            .args(["check", "--json"])
            .arg(case)
            .output()
            .expect("almide check --json");
        let stdout = String::from_utf8_lossy(&out.stdout);
        // One JSON object per diagnostic; anything else on stdout (a summary
        // line) is not a verdict. The exit status is judged separately so a
        // failure that prints no JSON still counts.
        let diagnostics: Vec<&str> = stdout.lines().filter(|l| l.trim_start().starts_with('{')).collect();
        if !out.status.success() || !diagnostics.is_empty() {
            loud.push(format!(
                "{} (exit {:?}):\n  {}{}",
                case.display(),
                out.status.code(),
                diagnostics.join("\n  "),
                String::from_utf8_lossy(&out.stderr),
            ));
        }
    }
    assert!(
        loud.is_empty(),
        "{} silent case(s) drew a diagnostic — a valid program is being rejected or \
         warned about (the #2097 family):\n{}",
        loud.len(),
        loud.join("\n")
    );
}
