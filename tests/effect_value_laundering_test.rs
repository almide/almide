//! An effect fn reached through a function VALUE (#4151, #4152).
//!
//! A pure fn read and deleted files by passing an effect fn as a value
//! (`list.map(fs.read_text)`), through a user higher-order fn, through a
//! record field, and through a fn-typed parameter handed a stored closure —
//! on both targets. The same holds for `option.map` and `result.map` handed
//! the effect fn and for an effect fn piped into a lambda that calls it
//! (`fs.read_text |> ((f) => f(p))`): v0.68.0-rc2 checked each pure form and
//! printed the file on both targets. And `[permissions]` missed the same values: a manifest
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
    (
        "option-map",
        "E006",
        "import fs\n\
         fn sneaky() -> Option[Result[String, String]] = option.map(some(\"s.txt\"), fs.read_text)\n\
         effect fn main() -> Unit = println(\"${sneaky()}\")\n",
        "import fs\n\
         effect fn reads() -> Option[Result[String, String]] = option.map(some(\"s.txt\"), fs.read_text)\n\
         effect fn main() -> Unit = println(\"${reads()!}\")\n",
    ),
    (
        "result-map",
        "E006",
        "import fs\n\
         fn sneaky() -> Result[Result[String, String], String] = result.map(ok(\"s.txt\"), fs.read_text)\n\
         effect fn main() -> Unit = println(match sneaky() {\n  ok(r) => r ?? \"none\",\n  err(e) => e,\n})\n",
        "import fs\n\
         effect fn reads() -> Result[Result[String, String], String] = result.map(ok(\"s.txt\"), fs.read_text)\n\
         effect fn main() -> Unit = println(match reads() {\n  ok(r) => r ?? \"none\",\n  err(e) => e,\n})\n",
    ),
    (
        "pipe-into-lambda",
        "E109",
        "import fs\n\
         fn sneaky() -> Result[String, String] = fs.read_text |> ((f) => f(\"s.txt\"))\n\
         effect fn main() -> Unit = println(sneaky() ?? \"none\")\n",
        "import fs\n\
         effect fn reads() -> Result[String, String] = fs.read_text |> ((f) => f(\"s.txt\"))\n\
         effect fn main() -> Unit = println(reads() ?? \"none\")\n",
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

// ── #4522: the pools follow the value's type, not its arity ──
//
// A fn value read at a type with a type variable used to read every pool of
// its arity, so the heap's `(h.cmp)(a, b)` at `(A, A) -> Int` was charged
// with an `effect (Float, Float) -> Unit` handler stored in a record
// elsewhere: E109 against `heap.__before`. Each program below uses `heap`, so
// the repro checks clean only because the read no longer sees that pool, and
// the true positives stay E109 with the heap in the program.

const HEAP_USE: &str = "fn top(xs: List[Int]) -> Int? = {\n  var h = heap.from_list(xs)\n  heap.pop(h)\n}\n";

/// The issue's program.
const REPRO_4522: &str = "type V = { on_context: effect (Float, Float) -> Unit }\n\
fn on_context(v: V, f: effect (Float, Float) -> Unit) -> V = { ...v, on_context: f }\n\
var n = 0\n\
effect fn save() -> Unit = { n = n + 1 }\n\
fn view() -> V = { on_context: (x, y) => () } |> on_context((x, y) => { save()! })\n\
effect fn main() -> Unit = {\n  let v = view()\n  v.on_context(1.0, 2.0)!\n  println(\"${n}\")\n}\n";

/// `(shape, the pure fn E109 names, program without the heap use)`.
const PRECISION_POSITIVES: &[(&str, &str, &str)] = &[
    (
        // A pure fn hands a closure it read from a list to a fn that calls
        // its closure parameter.
        "closure-param",
        "pure_looking",
        "import fs\n\
         fn call_it(f: (String) -> Result[String, String]) -> Result[String, String] = f(\"s.txt\")\n\
         fn pure_looking(rs: List[(String) -> Result[String, String]]) -> Result[String, String] = match list.first(rs) {\n  some(f) => call_it(f),\n  none => err(\"none\"),\n}\n\
         effect fn main() -> Unit = {\n  let rs = [(p) => fs.read_text(p)]\n  println(pure_looking(rs) ?? \"none\")\n  println(\"${top([3, 1]) ?? 0}\")\n}\n",
    ),
    (
        // A pure fn calls a closure stored in a record field it reads.
        "record-field",
        "pure_looking",
        "import fs\n\
         type Reader = { read: (String) -> Result[String, String] }\n\
         fn pure_looking(r: Reader) -> Result[String, String] = (r.read)(\"s.txt\")\n\
         effect fn main() -> Unit = {\n  let r = Reader { read: (p) => fs.read_text(p) }\n  println(pure_looking(r) ?? \"none\")\n  println(\"${top([3, 1]) ?? 0}\")\n}\n",
    ),
    (
        // A pure fn gives a stdlib higher-order fn a closure through a value.
        "stdlib-hof",
        "pure_looking",
        "import fs\n\
         type Reader = { read: (String) -> Result[String, String] }\n\
         fn pure_looking(r: Reader) -> List[Result[String, String]] = [\"s.txt\"] |> list.map(r.read)\n\
         effect fn main() -> Unit = {\n  let r = Reader { read: (p) => fs.read_text(p) }\n  println(\"${list.len(pure_looking(r))}\")\n  println(\"${top([3, 1]) ?? 0}\")\n}\n",
    ),
    (
        // A generic body calls an element at a type with a type variable,
        // `(A) -> Result[A, String]`: it still sees the closure pooled at
        // `(String) -> Result[String, String]`, an instance of that type.
        "generic-read",
        "run_all",
        "import fs\n\
         fn run_all[A](rs: List[(A) -> Result[A, String]], x: A) -> List[Result[A, String]] = rs |> list.map((f) => f(x))\n\
         fn pure_looking(rs: List[(String) -> Result[String, String]]) -> List[Result[String, String]] = run_all(rs, \"s.txt\")\n\
         effect fn main() -> Unit = {\n  let rs = [(p) => fs.read_text(p)]\n  println(\"${list.len(pure_looking(rs))}\")\n  println(\"${top([3, 1]) ?? 0}\")\n}\n",
    ),
];

fn check_with_heap(program: &str) -> (Output, String) {
    let td = tempfile::TempDir::new().unwrap();
    project(td.path(), None, &format!("{program}{HEAP_USE}"));
    let o = almide(td.path(), &["check", "main.almd"]);
    let t = text(&o);
    (o, t)
}

#[test]
fn heap_is_not_blamed_for_a_closure_it_never_receives_4522() {
    let (o, t) = check_with_heap(REPRO_4522);
    assert!(o.status.success(), "#4522 repro: `almide check` refused it:\n{t}");
    assert!(!t.contains("E109"), "#4522 repro: E109 reported:\n{t}");
    // The same read in user code, without the stdlib heap: a generic fn
    // calls a field at `(A, A) -> Int`.
    let ord = "type Ord[A] = { cmp: (A, A) -> Int }\n\
               fn before[A](o: Ord[A], a: A, b: A) -> Bool = (o.cmp)(a, b) < 0\n\
               fn first_is_less() -> Bool = before({ cmp: (a, b) => a - b }, 1, 2)\n";
    let td = tempfile::TempDir::new().unwrap();
    project(td.path(), None, &format!("{REPRO_4522}{ord}"));
    let o = almide(td.path(), &["check", "main.almd"]);
    let t = text(&o);
    assert!(o.status.success() && !t.contains("E109"), "#4522 repro without heap: `almide check` refused it:\n{t}");
}

#[test]
fn a_pure_fn_running_an_effectful_closure_stays_e109_with_heap_in_the_program() {
    // Every shape is judged before failing, so a control names each one.
    let mut missed = Vec::new();
    for &(shape, blamed, program) in PRECISION_POSITIVES {
        let (o, t) = check_with_heap(program);
        let needle = format!("pure function `{blamed}` runs an effect function it does not call by name");
        if o.status.success() {
            missed.push(format!("{shape}: `almide check` accepted it:\n{t}"));
        } else if !(t.contains("error[E109]") && t.contains(&needle)) {
            missed.push(format!("{shape}: no E109 naming `{blamed}`:\n{t}"));
        } else if t.contains("heap.") {
            missed.push(format!("{shape}: a heap fn was blamed:\n{t}"));
        }
    }
    assert!(missed.is_empty(), "{} of {} shapes:\n{}", missed.len(), PRECISION_POSITIVES.len(), missed.join("\n"));
}
