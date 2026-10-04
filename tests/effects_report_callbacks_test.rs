//! `almide check --effects` says when a function runs closures it is handed (#3268).
//!
//! A plain fn that calls a fn-typed parameter or a record-field closure runs
//! whatever that closure does, and the closure's categories are charged to the
//! function that creates it, not to the one that calls it. The report printed
//! such a function as `→ {}` and counted it pure. It now names the value it
//! calls (`f (arg 1)`, `b.run`) and counts it as callback-dependent; it never
//! invents a callee (ADR-0026 D4). The capability check, which charges the
//! creator, is unchanged and still refuses these programs under a narrow grant.

use std::path::{Path, PathBuf};
use std::process::Command;

/// An effect fn returns a file-reading closure; a plain fn calls it.
const RETURNED_CLOSURE: &str = r#"import fs

effect fn make() -> (String) -> String = (p) => fs.read_text(p) ?? ""

fn use_it(f: (String) -> String) -> String = f("/etc/hosts")

fn shout(s: String) -> String = string.to_upper(s)

effect fn main() -> Unit = {
  let f = make()!
  println(shout(use_it(f)))
}
"#;

/// A plain fn calls whatever closure a record field holds.
const RECORD_FIELD: &str = r#"import fs

type Box = { run: (String) -> String }

fn call_box(b: Box) -> String = (b.run)("/etc/hosts")

effect fn main() -> Unit = {
  let b = Box { run: (p) => fs.read_text(p) ?? "" }
  println(call_box(b))
}
"#;

/// A plain fn hands its callback to a stdlib HOF, and another one calls a
/// lambda it writes itself: the first runs `f`, the second runs only its own
/// code.
const HANDED_ON_AND_OWN: &str = r#"fn each(xs: List[String], f: (String) -> String) -> List[String] = xs |> list.map(f)

fn own(xs: List[String]) -> List[String] = {
  let g = (s: String) => s + "!"
  xs |> list.map((s) => g(s))
}

effect fn main() -> Unit = {
  println(list.join(each(["a"], (s) => s), ","))
  println(list.join(own(["b"]), ","))
}
"#;

/// A named effect fn taken as a value is a closure its taker creates: the
/// taker is charged, as it is for a lambda it writes. Before #3268 a `FnRef`
/// was no edge of the call graph, so `main` here read `{}` and a grant
/// without IO admitted it.
const NAMED_FN_VALUE: &str = r#"import fs

effect fn rd(p: String) -> String = fs.read_text(p)!

fn apply(f: (String) -> String!, x: String) -> String = f(x) ?? ""

effect fn main() -> Unit = {
  println(apply(rd, "/etc/hosts"))
}
"#;

fn project(tag: &str, allow: Option<&str>, main: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-issue3268-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let permissions = allow.map(|a| format!("\n[permissions]\nallow = [{a}]\n")).unwrap_or_default();
    std::fs::write(
        dir.join("almide.toml"),
        format!("[package]\nname = \"effcb\"\nversion = \"0.1.0\"\n{permissions}"),
    )
    .expect("write manifest");
    std::fs::write(dir.join("main.almd"), main).expect("write program");
    dir
}

fn almide(dir: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn almide");
    (out.status.success(), String::from_utf8_lossy(&out.stderr).to_string())
}

fn effects_report(tag: &str, main: &str) -> String {
    let dir = project(tag, None, main);
    let (ok, stderr) = almide(&dir, &["check", "--effects", "main.almd"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ok, "`almide check --effects` failed:\n{stderr}");
    stderr
}

#[test]
fn a_called_fn_parameter_is_named_with_its_position() {
    let report = effects_report("param", RETURNED_CLOSURE);
    assert!(report.contains("  use_it  → {} + whatever f (arg 1) does\n"), "{report}");
    // The creator is charged, as before.
    assert!(report.contains("  make  → {IO} (effect fn)\n"), "{report}");
    // A fn with no indirect call stays `{}` and is the one counted pure.
    assert!(report.contains("  shout  → {}\n"), "{report}");
    assert!(report.contains("4 functions: 1 pure, 1 callback-dependent, 2 with effects"), "{report}");
}

#[test]
fn a_called_record_field_is_named_by_its_path() {
    let report = effects_report("record", RECORD_FIELD);
    assert!(report.contains("  call_box  → {} + whatever b.run does\n"), "{report}");
    assert!(report.contains("2 functions: 0 pure, 1 callback-dependent, 1 with effects"), "{report}");
}

#[test]
fn a_parameter_handed_to_a_hof_counts_and_an_own_lambda_does_not() {
    let report = effects_report("hof", HANDED_ON_AND_OWN);
    assert!(report.contains("  each  → {} + whatever f (arg 2) does\n"), "{report}");
    assert!(report.contains("  own  → {}\n"), "{report}");
}

#[test]
fn a_named_fn_taken_as_a_value_charges_its_taker() {
    let report = effects_report("fnref", NAMED_FN_VALUE);
    assert!(report.contains("  apply  → {} + whatever f (arg 1) does\n"), "{report}");
    assert!(report.contains("  main  → {IO} (effect fn)\n"), "{report}");
    let dir = project("fnref-perm", Some("\"Env\""), NAMED_FN_VALUE);
    let (ok, stderr) = almide(&dir, &["check", "--effects", "main.almd"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!ok && stderr.contains("capability violation in `main`"), "{stderr}");
}

/// The report changed; the gate did not. A closure's category is charged to
/// the function that creates it, so a grant without IO still refuses both
/// programs, and names the creator.
#[test]
fn permissions_still_refuse_the_creator() {
    for (tag, main, creator) in [("perm-param", RETURNED_CLOSURE, "make"), ("perm-record", RECORD_FIELD, "main")] {
        let dir = project(tag, Some("\"Env\""), main);
        for args in [&["check", "main.almd"][..], &["check", "--effects", "main.almd"][..]] {
            let (ok, stderr) = almide(&dir, args);
            assert!(!ok, "`almide {}` admitted a file read under allow = [\"Env\"]:\n{stderr}", args.join(" "));
            assert!(stderr.contains("IO is not in [permissions].allow"), "{stderr}");
            if args.contains(&"--effects") {
                assert!(stderr.contains(&format!("capability violation in `{creator}`")), "{stderr}");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
