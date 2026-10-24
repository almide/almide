//! An effect fn reached through a function VALUE (#4151, #4152).
//!
//! A pure fn read and deleted files by passing an effect fn as a value
//! (`list.map(fs.read_text)`), through a user higher-order fn, through a
//! record field, and through a fn-typed parameter handed a stored closure —
//! on both targets. And `[permissions]` missed the same values: a manifest
//! allowing only `Time` built `list.map(fs.read_text)` and read the file.
//!
//! These tests pin, per shape and on both targets:
//! - the pure form is refused (E006 at a call that names the effect fn, E109
//!   where only the value flow sees it), and no route reads the file;
//! - the effect form under `allow = ["Time"]` is a capability violation
//!   naming `FS.read`, and no route reads the file;
//! - the control: the same effect form under `allow = ["Time", "FS", "IO"]` checks,
//!   and does read the file — so each refusal above is about the shape, not a
//!   program that could not have read anything.

use std::path::Path;
use std::process::{Command, Output};

const SECRET: &str = "secret-data-4151";

/// `(name, code the pure form is refused with, pure form, effect form)`.
const SHAPES: &[(&str, &str, &str, &str)] = &[
    (
        "list-map",
        "E109",
        "import fs\n\
         fn sneaky() -> List[Result[String, String]] = [\"s.txt\"] |> list.map(fs.read_text)\n\
         effect fn main() -> Unit = println(\"${sneaky()}\")\n",
        "import fs\n\
         effect fn reads() -> List[Result[String, String]] = [\"s.txt\"] |> list.map(fs.read_text)\n\
         effect fn main() -> Unit = println(\"${reads()!}\")\n",
    ),
    (
        "user-hof",
        "E006",
        "import fs\n\
         fn apply(f: (String) -> Result[String, String], p: String) -> Result[String, String] = f(p)\n\
         fn sneaky(p: String) -> Result[String, String] = apply(fs.read_text, p)\n\
         effect fn main() -> Unit = println(sneaky(\"s.txt\") ?? \"none\")\n",
        "import fs\n\
         fn apply(f: (String) -> Result[String, String], p: String) -> Result[String, String] = f(p)\n\
         effect fn reads(p: String) -> Result[String, String] = apply(fs.read_text, p)\n\
         effect fn main() -> Unit = println(reads(\"s.txt\") ?? \"none\")\n",
    ),
    (
        "record-field",
        "E109",
        "import fs\n\
         type Reader = { read: (String) -> Result[String, String] }\n\
         fn pure_looking(r: Reader) -> Result[String, String] = (r.read)(\"s.txt\")\n\
         effect fn main() -> Unit = {\n  let r = Reader { read: fs.read_text }\n  println(pure_looking(r) ?? \"none\")\n}\n",
        "import fs\n\
         type Reader = { read: (String) -> Result[String, String] }\n\
         effect fn reads(r: Reader) -> Result[String, String] = (r.read)(\"s.txt\")\n\
         effect fn main() -> Unit = {\n  let r = Reader { read: fs.read_text }\n  println(reads(r) ?? \"none\")\n}\n",
    ),
    (
        "fn-param",
        "E109",
        "import fs\n\
         type Reader = { read: (String) -> Result[String, String] }\n\
         fn call_it(f: (String) -> Result[String, String]) -> Result[String, String] = f(\"s.txt\")\n\
         fn pure_looking(r: Reader) -> Result[String, String] = call_it(r.read)\n\
         effect fn main() -> Unit = {\n  let r = Reader { read: (p) => fs.read_text(p) }\n  println(pure_looking(r) ?? \"none\")\n}\n",
        "import fs\n\
         type Reader = { read: (String) -> Result[String, String] }\n\
         fn call_it(f: (String) -> Result[String, String]) -> Result[String, String] = f(\"s.txt\")\n\
         effect fn reads(r: Reader) -> Result[String, String] = call_it(r.read)\n\
         effect fn main() -> Unit = {\n  let r = Reader { read: (p) => fs.read_text(p) }\n  println(reads(r) ?? \"none\")\n}\n",
    ),
];

fn project(dir: &Path, allow: Option<&str>, program: &str) {
    let permissions = allow.map(|a| format!("\n[permissions]\nallow = [{a}]\n")).unwrap_or_default();
    std::fs::write(
        dir.join("almide.toml"),
        format!("[package]\nname = \"launder\"\nversion = \"0.1.0\"\n{permissions}"),
    )
    .unwrap();
    std::fs::write(dir.join("main.almd"), program).unwrap();
    std::fs::write(dir.join("s.txt"), SECRET).unwrap();
}

/// The binary under test. `ALMIDE_CONTROL_BIN` points the same assertions
/// at another build — the negative control runs them against a compiler
/// without the check and sees them fail.
fn almide(dir: &Path, args: &[&str]) -> Output {
    let bin = std::env::var("ALMIDE_CONTROL_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    Command::new(bin).args(args).current_dir(dir).output().expect("almide runs")
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

/// Every route refuses, prints `needle`, and no route reads the file.
fn refused_everywhere(dir: &Path, shape: &str, needle: &str) {
    let routes: [&[&str]; 4] = [
        &["check", "main.almd"],
        &["build", "main.almd", "-o", "app"],
        &["build", "main.almd", "--target", "wasm", "-o", "app.wasm"],
        &["run", "main.almd", "--target", "wasm"],
    ];
    for args in routes {
        let o = almide(dir, args);
        let t = text(&o);
        assert!(!o.status.success(), "{shape}: `almide {}` accepted it:\n{t}", args.join(" "));
        assert!(t.contains(needle), "{shape}: `almide {}` did not say {needle:?}:\n{t}", args.join(" "));
        assert!(!t.contains(SECRET), "{shape}: `almide {}` read the file:\n{t}", args.join(" "));
    }
    assert!(!dir.join("app.wasm").exists(), "{shape}: the refused wasm build wrote a module");
}

#[test]
fn a_pure_fn_cannot_reach_a_file_through_a_fn_value() {
    for &(shape, code, pure, _) in SHAPES {
        let td = tempfile::TempDir::new().unwrap();
        project(td.path(), None, pure);
        refused_everywhere(td.path(), shape, &format!("error[{code}]"));
    }
}

#[test]
fn permissions_see_a_category_carried_by_a_fn_value() {
    for &(shape, _, _, effect) in SHAPES {
        let td = tempfile::TempDir::new().unwrap();
        project(td.path(), Some("\"Time\""), effect);
        refused_everywhere(td.path(), shape, "FS.read is not in [permissions].allow");
    }
}

/// The control: the effect forms read the file when the manifest grants it,
/// on the wasm leg for every shape and on the native leg for one.
#[test]
fn the_effect_forms_read_the_file_when_io_is_granted() {
    for &(shape, _, _, effect) in SHAPES {
        let td = tempfile::TempDir::new().unwrap();
        project(td.path(), Some("\"Time\", \"FS\", \"IO\""), effect);
        let check = almide(td.path(), &["check", "main.almd"]);
        assert!(check.status.success(), "{shape}: check refused the granted form:\n{}", text(&check));
        let run = almide(td.path(), &["run", "main.almd", "--target", "wasm"]);
        assert!(run.status.success() && text(&run).contains(SECRET), "{shape}: wasm run did not read the file:\n{}", text(&run));
    }
    let td = tempfile::TempDir::new().unwrap();
    project(td.path(), Some("\"Time\", \"FS\", \"IO\""), SHAPES[2].3);
    let run = almide(td.path(), &["run", "main.almd"]);
    assert!(run.status.success() && text(&run).contains(SECRET), "native run did not read the file:\n{}", text(&run));
}
