//! ADR-0026 D1: an effect category set rides the fn value (#3243, #3268).
//!
//! A closure's categories are performed where it is CALLED, and the set moves
//! with the value: through a bare fn-type parameter (transparent — the call
//! site that hands the closure over is charged with it), a `let` local, a
//! record field, and a return value. Creating a closure performs nothing.
//! `[permissions].allow` is checked on these sets, and a violation names a
//! path that goes through the callback by its parameter (D4).

use std::path::{Path, PathBuf};
use std::process::Command;

fn project(tag: &str, allow: Option<&str>, main: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-adr26-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let permissions = allow.map(|a| format!("\n[permissions]\nallow = [{a}]\n")).unwrap_or_default();
    std::fs::write(dir.join("almide.toml"), format!("[package]\nname = \"effflow\"\nversion = \"0.1.0\"\n{permissions}"))
        .expect("write manifest");
    std::fs::write(dir.join("main.almd"), main).expect("write program");
    dir
}

fn almide(dir: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).args(args).current_dir(dir).output().expect("spawn almide");
    (out.status.success(), String::from_utf8_lossy(&out.stderr).to_string())
}

fn report(tag: &str, main: &str) -> String {
    let dir = project(tag, None, main);
    let (ok, stderr) = almide(&dir, &["check", "--effects", "main.almd"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ok, "`almide check --effects` failed:\n{stderr}");
    stderr
}

/// The report line of `name`, without its indentation.
fn line_of<'a>(report: &'a str, name: &str) -> &'a str {
    let prefix = format!("  {name}  → ");
    report.lines().find(|l| l.starts_with(&prefix)).map(str::trim).unwrap_or_else(|| panic!("no line for {name}:\n{report}"))
}

/// (a) A bare fn-type parameter is transparent: each call site is charged
/// with the closure it passes, the pure one with nothing.
const PARAM: &str = r#"import fs

fn apply(f: (String) -> String, x: String) -> String = f(x)

fn shout(s: String) -> String = apply((t) => string.to_upper(t), s)

effect fn read(p: String) -> String = apply((q) => fs.read_text(q) ?? "", p)

effect fn main() -> Unit = {
  println(shout("a"))
  println(read("/etc/hosts")!)
}
"#;

#[test]
fn a_parameter_carries_the_set_of_the_closure_each_call_site_passes() {
    let r = report("param", PARAM);
    assert_eq!(line_of(&r, "apply"), "apply  → {} + whatever f (arg 1) does", "{r}");
    assert_eq!(line_of(&r, "shout"), "shout  → {}", "{r}");
    assert_eq!(line_of(&r, "read"), "read  → {IO} (effect fn)", "{r}");
    assert_eq!(line_of(&r, "main"), "main  → {IO} (effect fn)", "{r}");
}

/// (b) A local keeps the set of the closure bound to it; binding a closure
/// that is never called performs nothing.
const LOCAL: &str = r#"import fs

effect fn called() -> String = {
  let g = (p: String) => fs.read_text(p) ?? ""
  g("/etc/hosts")
}

effect fn never_called() -> Int = {
  let g = (p: String) => fs.read_text(p) ?? ""
  1
}

effect fn main() -> Unit = {
  println(called()!)
  println(int.to_string(never_called()!))
}
"#;

#[test]
fn a_local_keeps_the_set_of_the_closure_bound_to_it() {
    let r = report("local", LOCAL);
    assert_eq!(line_of(&r, "called"), "called  → {IO} (effect fn)", "{r}");
    assert_eq!(line_of(&r, "never_called"), "never_called  → {} (effect fn)", "{r}");
}

/// (c) A record field keeps the set of every closure stored in it.
const FIELD: &str = r#"import fs

type Box = { run: (String) -> String }

fn call_box(b: Box) -> String = (b.run)("/etc/hosts")

effect fn main() -> Unit = {
  let b = Box { run: (p) => fs.read_text(p) ?? "" }
  println(call_box(b))
}
"#;

#[test]
fn a_record_field_keeps_the_set_of_the_closure_stored_in_it() {
    let r = report("field", FIELD);
    assert_eq!(line_of(&r, "call_box"), "call_box  → {IO}", "{r}");
    assert_eq!(line_of(&r, "main"), "main  → {IO} (effect fn)", "{r}");
}

/// (d) A returned closure keeps its set; the function that builds it
/// performs nothing, the one that calls it performs the set.
const RETURN: &str = r#"import fs

effect fn make() -> (String) -> String = (p) => fs.read_text(p) ?? ""

fn use_it(f: (String) -> String) -> String = f("/etc/hosts")

effect fn main() -> Unit = {
  let f = make()!
  println(use_it(f))
}
"#;

#[test]
fn a_returned_closure_keeps_its_set_and_is_charged_where_it_runs() {
    let r = report("return", RETURN);
    assert_eq!(line_of(&r, "make"), "make  → {} (effect fn); returns a closure doing {IO}", "{r}");
    assert_eq!(line_of(&r, "use_it"), "use_it  → {} + whatever f (arg 1) does", "{r}");
    assert_eq!(line_of(&r, "main"), "main  → {IO} (effect fn)", "{r}");
}

/// A HOF that hands its parameter to a stdlib HOF is transparent too.
const HOF: &str = r#"import fs

fn each(xs: List[String], f: (String) -> String) -> List[String] = xs |> list.map(f)

fn upper_all(xs: List[String]) -> List[String] = each(xs, (s) => string.to_upper(s))

effect fn main() -> Unit = {
  println(list.join(upper_all(["a"]), ","))
  println(list.join(each(["/etc/hosts"], (p) => fs.read_text(p) ?? ""), ","))
}
"#;

#[test]
fn a_parameter_handed_to_list_map_is_transparent() {
    let r = report("hof", HOF);
    assert_eq!(line_of(&r, "each"), "each  → {} + whatever f (arg 2) does", "{r}");
    assert_eq!(line_of(&r, "upper_all"), "upper_all  → {}", "{r}");
    assert_eq!(line_of(&r, "main"), "main  → {IO} (effect fn)", "{r}");
}

/// A grant without IO refuses the function that RUNS the closure, and the
/// path goes through the callback by its parameter to the closure and the
/// operation (D4). `almide check` and `check --effects` agree.
#[test]
fn permissions_refuse_the_runner_with_a_path_through_the_parameter() {
    let dir = project("perm", Some("\"Env\""), RETURN);
    for args in [&["check", "main.almd"][..], &["check", "--effects", "main.almd"][..]] {
        let (ok, stderr) = almide(&dir, args);
        assert!(!ok, "`almide {}` admitted a file read under allow = [\"Env\"]:\n{stderr}", args.join(" "));
        assert!(stderr.contains("capability violation in `main`"), "{stderr}");
        assert!(!stderr.contains("capability violation in `make`"), "the builder runs nothing:\n{stderr}");
        let path = stderr.lines().find(|l| l.trim_start().starts_with("path: main")).unwrap_or_else(|| panic!("no path:\n{stderr}"));
        assert!(path.contains("main → use_it → f (arg 1 of use_it) → closure (line 3:"), "{path}");
        assert!(path.contains("in make) → fs.read_text (line 3:"), "{path}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// D4: when the value is not known at the step, the path names the
/// parameter it came in by and marks the next step as an example.
const STORED_PARAM: &str = r#"import fs

type Box = { run: (String) -> String }

fn wrap(f: (String) -> String) -> Box = Box { run: f }

fn call_box(b: Box) -> String = (b.run)("/etc/hosts")

effect fn main() -> Unit = {
  println(call_box(wrap((p) => fs.read_text(p) ?? "")))
}
"#;

#[test]
fn a_path_names_the_parameter_when_the_value_is_not_known() {
    let dir = project("stored", Some("\"Env\""), STORED_PARAM);
    let (ok, stderr) = almide(&dir, &["check", "--effects", "main.almd"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!ok, "{stderr}");
    assert!(stderr.contains("capability violation in `call_box`"), "{stderr}");
    let path = stderr.lines().find(|l| l.trim_start().starts_with("path: call_box")).unwrap_or_else(|| panic!("no path:\n{stderr}"));
    assert!(path.contains("call_box → field `run` → f (arg 1 of wrap) → e.g. closure ("), "{path}");
}

/// Untracked positions: a closure in a list, in a destructured tuple, or the
/// operand of `??` is still charged where it runs — through the pool of its
/// arity (list, tuple) or through the parameter it came in by (`??`).
const UNTRACKED: &str = r#"import fs

fn run_all(hs: List[(String) -> String], p: String) -> List[String] = hs |> list.map((h) => h(p))

fn call_pair(t: ((String) -> String, Int)) -> String = {
  let (f, n) = t
  f("/etc/hosts")
}

fn pick(o: Option[(String) -> String]) -> String = (o ?? ((s) => s))("/etc/hosts")

effect fn main() -> Unit = {
  println(list.join(run_all([(p: String) => fs.read_text(p) ?? ""], "/etc/hosts"), ","))
  println(call_pair(((p: String) => fs.read_text(p) ?? "", 1)))
  println(pick(some((p: String) => fs.read_text(p) ?? "")))
}
"#;

#[test]
fn a_closure_in_a_list_a_tuple_or_an_option_is_charged_where_it_runs() {
    let r = report("untracked", UNTRACKED);
    assert_eq!(line_of(&r, "run_all"), "run_all  → {IO}", "{r}");
    assert_eq!(line_of(&r, "call_pair"), "call_pair  → {IO}", "{r}");
    assert_eq!(line_of(&r, "pick"), "pick  → {} + whatever o (arg 1) does", "{r}");
    assert_eq!(line_of(&r, "main"), "main  → {IO} (effect fn)", "{r}");
    let dir = project("untracked-perm", Some("\"Env\""), UNTRACKED);
    let (ok, stderr) = almide(&dir, &["check", "--effects", "main.almd"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!ok, "{stderr}");
    let path = stderr.lines().find(|l| l.trim_start().starts_with("path: run_all")).unwrap_or_else(|| panic!("no path:\n{stderr}"));
    assert!(path.contains("an untracked closure value → e.g. closure ("), "{path}");
}
