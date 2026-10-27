//! #4107 canary gate (C-425): a canary planted in Secrets and in a field a
//! user `T.repr` hides must never reach a byte of stdout or stderr, on the
//! native leg, the wasm leg, the interpreter, and an `almide test` report.
//!
//! An empty run would pass a "no canary" check, so every run must ALSO show
//! the renderings that replace it (`Secret(***)` and the user repr's text).
//! The negative control is in the same test: a twin of the fixture that
//! reveals the secret once MUST show the canary on every leg — proof the
//! scanner reads the streams it claims to.
//!
//! Needs `wasmtime` on PATH for the wasm leg; without it the test skips the
//! wasm column and says so (CI sets `ALMIDE_EXPECT_TOOLS` to fail instead).

#![allow(dead_code)]

include!("wasm_runtime_test_parts/interp_leg.rs");

use std::path::{Path, PathBuf};
use std::process::Command;

const CANARY: &str = "CANARY4107x";
const FIXTURE: &str = "spec/wasm_cross/secret_never_rendered.almd";
const MASKS: &[&str] = &["Secret(***)", "Account(ana)"];

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn scratch(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("secret_render_canary").join(name);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn wasmtime_available() -> bool {
    let ok = Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success());
    if !ok && std::env::var_os("ALMIDE_EXPECT_TOOLS").is_some() {
        panic!("ALMIDE_EXPECT_TOOLS is set but wasmtime is not on PATH");
    }
    ok
}

/// `(leg, stdout + stderr)` for `file` on every leg this host can run.
fn legs(file: &Path) -> Vec<(&'static str, String)> {
    let run = |extra: &[&str]| {
        let o = Command::new(almide()).arg("run").arg(file).args(extra).output().expect("almide runs");
        format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
    };
    let mut out = vec![("native", run(&[]))];
    if wasmtime_available() {
        out.push(("wasm", run(&["--target", "wasm"])));
    } else {
        eprintln!("secret_render_canary: wasmtime not on PATH — wasm leg skipped");
    }
    let src = std::fs::read_to_string(file).unwrap();
    match run_interp_capture(&src) {
        InterpLeg::Ran(_, o, e) => out.push(("interp", format!("{o}{e}"))),
        InterpLeg::Skip(why) => panic!("the interpreter must vote on {}: {why}", file.display()),
    }
    out
}

/// A test file whose failing asserts compare values holding the canary, and
/// the `almide test` report of it on each test leg.
fn test_reports() -> Vec<(&'static str, String)> {
    let dir = scratch("report");
    let file = dir.join("canary_report_test.almd");
    std::fs::write(&file, format!(r#"import secret

type Account = {{ name: String, password: String }}
fn Account.repr(a: Account) -> String = "Account(${{a.name}})"
type Login = {{ user: String, pw: Secret[String] }}

test "userrepr" {{
  assert_eq(Account {{ name: "ana", password: "{CANARY}" }}, Account {{ name: "ana", password: "x" }})
}}
test "secretfield" {{
  assert_eq(Login {{ user: "ana", pw: secret.new("{CANARY}") }}, Login {{ user: "bob", pw: secret.new("y") }})
}}
test "secretne" {{
  assert_ne(secret.new("{CANARY}"), secret.new("{CANARY}"))
}}
"#)).unwrap();
    let run = |extra: &[&str]| {
        let o = Command::new(almide()).arg("test").arg(&file).args(extra).output().expect("almide test runs");
        assert!(!o.status.success(), "the canary tests are written to fail");
        format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
    };
    let mut out = vec![("test native", run(&[]))];
    if wasmtime_available() {
        // The wasm runner stops at a file's first failing test, so each test
        // runs alone and the three reports are read together.
        let all = ["userrepr", "secretfield", "secretne"]
            .iter()
            .map(|t| run(&["--target", "wasm", "--run", t]))
            .collect::<Vec<_>>()
            .join("\n");
        out.push(("test wasm", all));
    }
    out
}

#[test]
fn no_renderer_prints_a_planted_canary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut runs = legs(&root.join(FIXTURE));
    runs.extend(test_reports());
    for (leg, text) in &runs {
        assert!(!text.contains(CANARY), "{leg}: the canary reached the output:\n{text}");
        for mask in MASKS {
            assert!(text.contains(mask), "{leg}: `{mask}` missing — an empty or broken run cannot pass:\n{text}");
        }
    }
}

/// The control: the same fixture with ONE explicit `secret.reveal` printed
/// must show the canary on every leg.
#[test]
fn an_explicit_reveal_shows_the_canary_on_every_leg() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = std::fs::read_to_string(root.join(FIXTURE)).unwrap();
    let twin = src.replacen(
        "  println(\"revealed length: ",
        "  println(\"revealed: ${secret.reveal(s)}\")\n  println(\"revealed length: ",
        1,
    );
    assert_ne!(twin, src, "the control's insertion point moved");
    let file = scratch("control").join("secret_revealed.almd");
    std::fs::write(&file, twin).unwrap();
    for (leg, text) in legs(&file) {
        assert!(text.contains(&format!("revealed: {CANARY}")), "{leg}: the reveal control did not show the canary:\n{text}");
    }
}
