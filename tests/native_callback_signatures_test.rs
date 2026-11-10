//! A package's `native/*.rs` calling back into module fns whose signatures
//! are not all-scalar (#4640).
//!
//! Native code is written against the Rust signature the standard codegen
//! emits for a module fn: `&str` or `AlmideString`, `&[i64]` or `Vec<i64>`,
//! and `Result<T, AlmideString>` for an effect fn. `almide test` builds on the
//! native render alone, which adapted only all-scalar callbacks, so such a
//! package's tests stopped building where the previous release ran them.
//!
//! The matrix below is one row per kind. `SIGNATURES` and `OUTPUT` are what
//! the previous release emitted and printed for this package; the native
//! render must present the same signatures (also held against this
//! compiler's own standard-codegen emission) and print the same line.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

fn write(path: PathBuf, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, body).expect("write");
}

const MODULE: &str = r#"pub fn greet(name: String) -> String = "hello ${name}"
pub fn keep(name: String) -> String = name
pub fn total(xs: List[Int]) -> Int = list.sum(xs)
pub fn joined(xs: List[String]) -> String = list.join(xs, "+")
pub fn doubled(xs: List[Int]) -> List[Int] = xs |> list.map((x) => x * 2)
pub fn wrap(xs: List[String], extra: String) -> List[String] = xs + [extra]
pub fn halves(xs: List[Float]) -> List[Float] = xs |> list.map((x) => x / 2.0)
pub fn nested(xs: List[List[Int]]) -> Int = xs |> list.map((r) => list.sum(r)) |> list.sum
pub fn flags(on: Bool, w: Float) -> Bool = on and w > 0.5
pub fn pair(n: Int, s: String) -> (Int, String) = (n + 1, s + "!")
pub fn first_word(s: String) -> Option[String] = string.split(s, " ") |> list.first
pub fn opt_int(n: Int) -> Option[Int] = if n > 0 then some(n) else none
pub fn maybe_len(s: Option[String]) -> Int = match s { some(v) => string.len(v), none => -1 }
pub fn parse(s: String) -> Result[Int, String] = int.parse(s)
pub fn res_str(s: String) -> Result[String, String] = if s == "" then err("empty") else ok(s + "?")
pub fn counts(m: Map[String, Int]) -> Int = map.len(m)
pub fn index(xs: List[String]) -> Map[String, Int] = xs |> list.enumerate |> list.map(((i, s)) => (s, i)) |> map.from_list
pub fn uniq(xs: List[Int]) -> Set[Int] = set.from_list(xs)
pub fn size(b: Bytes) -> Int = bytes.len(b)
pub effect fn checked(n: Int) -> Result[Int, String] = if n < 0 then err("negative ${n}") else ok(n + 1)
pub effect fn shout(s: String) -> String = {
  println("shout ${s}")
  string.to_upper(s)
}
pub effect fn log(s: String) -> Unit = println("log ${s}")
pub effect fn fail_unit(n: Int) -> Unit = if n < 0 then err("neg")! else println("ok ${n}")
"#;

/// Each fn's parameter list and return type as the previous release emitted
/// them (`almide_rt_cb_<name>` there, `almide_rt_cb_0v0_<name>` linked).
const SIGNATURES: &[(&str, &str)] = &[
    ("greet", "(name: &str) -> AlmideString"),
    ("keep", "(name: AlmideString) -> AlmideString"),
    ("total", "(xs: &[i64]) -> i64"),
    ("joined", "(xs: &[AlmideString]) -> AlmideString"),
    ("doubled", "(xs: &[i64]) -> Vec<i64>"),
    ("wrap", "(xs: Vec<AlmideString>, extra: AlmideString) -> Vec<AlmideString>"),
    ("halves", "(xs: &[f64]) -> Vec<f64>"),
    ("nested", "(xs: &[Vec<i64>]) -> i64"),
    ("flags", "(on: bool, w: f64) -> bool"),
    ("pair", "(n: i64, s: AlmideString) -> (i64, AlmideString)"),
    ("first_word", "(s: &str) -> Option<AlmideString>"),
    ("opt_int", "(n: i64) -> Option<i64>"),
    ("maybe_len", "(s: Option<AlmideString>) -> i64"),
    ("parse", "(s: &str) -> Result<i64, AlmideString>"),
    ("res_str", "(s: AlmideString) -> Result<AlmideString, AlmideString>"),
    ("counts", "(m: &AlmideMap<AlmideString, i64>) -> i64"),
    ("index", "(xs: Vec<AlmideString>) -> AlmideMap<AlmideString, i64>"),
    ("uniq", "(xs: &[i64]) -> AlmideSet<i64>"),
    ("size", "(b: &AlmideRcCow<Vec<u8>>) -> i64"),
    ("checked", "(n: i64) -> Result<i64, AlmideString>"),
    ("shout", "(s: &str) -> Result<AlmideString, AlmideString>"),
    ("log", "(s: &str) -> Result<(), AlmideString>"),
    ("fail_unit", "(n: i64) -> Result<(), AlmideString>"),
];

const SCALAR_ROW: (&str, &str) = ("flags", "(a0: bool, a1: f64) -> bool");

/// Written against `SIGNATURES`: every row is called, and every result is
/// read back into one line.
const HOST_RS: &str = r#"use crate::*;
fn s(x: &str) -> AlmideString { AlmideString::from(x) }
fn shown<T: std::fmt::Debug>(r: Result<T, AlmideString>) -> String { format!("{:?}", r.map_err(|e| e.to_string())) }
pub fn run_all() -> String {
    let mut out: Vec<String> = Vec::new();
    out.push(almide_rt_cb_0v0_greet("bob").to_string());
    out.push(almide_rt_cb_0v0_keep(s("kept")).to_string());
    out.push(almide_rt_cb_0v0_total(&[1, 2, 3]).to_string());
    out.push(almide_rt_cb_0v0_joined(&[s("a"), s("b")]).to_string());
    out.push(format!("{:?}", almide_rt_cb_0v0_doubled(&[4, 5])));
    out.push(almide_rt_cb_0v0_wrap(vec![s("a")], s("b")).iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","));
    out.push(format!("{:?}", almide_rt_cb_0v0_halves(&[1.0, 3.0])));
    out.push(almide_rt_cb_0v0_nested(&[vec![1, 2], vec![3]]).to_string());
    out.push(almide_rt_cb_0v0_flags(true, 0.75).to_string());
    let (n, t) = almide_rt_cb_0v0_pair(1, s("p"));
    out.push(format!("{n}{t}"));
    out.push(format!("{:?}", almide_rt_cb_0v0_first_word("one two").map(|w| w.to_string())));
    out.push(format!("{:?}{:?}", almide_rt_cb_0v0_opt_int(3), almide_rt_cb_0v0_opt_int(0)));
    out.push(format!("{}{}", almide_rt_cb_0v0_maybe_len(Some(s("four"))), almide_rt_cb_0v0_maybe_len(None)));
    out.push(format!("{}{}", shown(almide_rt_cb_0v0_parse("41")), almide_rt_cb_0v0_parse("x").is_err()));
    out.push(format!("{}{}", shown(almide_rt_cb_0v0_res_str(s("q")).map(|v| v.to_string())), shown(almide_rt_cb_0v0_res_str(s("")).map(|v| v.to_string()))));
    let mut m: AlmideMap<AlmideString, i64> = Default::default();
    m.insert(s("k"), 1);
    out.push(almide_rt_cb_0v0_counts(&m).to_string());
    out.push(format!("{:?}", almide_rt_cb_0v0_index(vec![s("x"), s("y")]).get(&s("y")).copied()));
    out.push(almide_rt_cb_0v0_uniq(&[1, 1, 2]).len().to_string());
    out.push(almide_rt_cb_0v0_size(&AlmideRcCow::from(vec![1u8, 2, 3])).to_string());
    out.push(format!("{}{}", shown(almide_rt_cb_0v0_checked(1)), shown(almide_rt_cb_0v0_checked(-1))));
    out.push(shown(almide_rt_cb_0v0_shout("hey").map(|v| v.to_string())));
    out.push(shown(almide_rt_cb_0v0_log("line")));
    out.push(format!("{}{}", shown(almide_rt_cb_0v0_fail_unit(2)), shown(almide_rt_cb_0v0_fail_unit(-2))));
    out.join("|")
}
"#;

/// What the previous release printed for this package's `main`.
const OUTPUT: &str = "shout hey\nlog line\nok 2\n\
hello bob|kept|6|a+b|[8, 10]|a,b|[0.5, 1.5]|6|true|2p!|Some(\"one\")|Some(3)None|4-1|Ok(41)true|Ok(\"q?\")Err(\"empty\")|1|Some(1)|2|3|Ok(2)Err(\"negative -1\")|Ok(\"HEY\")|Ok(())|Ok(())Err(\"neg\")\nhello main\n";

/// The entry and the test file import the package's root module, as a
/// package's own files do: the standard codegen's signatures are those of the
/// program the module is built in.
const EXTERN: &str = "import self as cb\n@extern(rust, \"host\", \"run_all\")\nfn run_all() -> String\n";

fn package(tag: &str, module: &str, host_rs: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue4640-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(root.join("almide.toml"), "[package]\nname = \"cb\"\nversion = \"0.1.0\"\n");
    write(root.join("src/mod.almd"), module);
    write(root.join("native/host.rs"), host_rs);
    write(root.join("src/main.almd"), &format!("{EXTERN}\neffect fn main() -> Unit = {{\n  println(run_all())\n  println(cb.greet(\"main\"))\n}}\n"));
    let line = OUTPUT.lines().nth(3).unwrap().replace('\\', "\\\\").replace('"', "\\\"");
    write(root.join("src/cb_test.almd"), &format!("{EXTERN}\ntest \"native code calls back\" {{\n  assert_eq(run_all(), \"{line}\")\n  assert_eq(cb.greet(\"t\"), \"hello t\")\n}}\n"));
    root
}

/// `almide <args>` in `root`, building in the package's own scratch dir:
/// (success, stdout, stderr).
fn almide(root: &Path, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(almide_bin())
        .current_dir(root)
        .args(args)
        .env("ALMIDE_VERBOSE", "1")
        .env("ALMIDE_CERTIFY_OWNERSHIP", "fail")
        .env("ALMIDE_RUN_PROJECT_DIR", root.join("scratch"))
        .output()
        .expect("spawn almide");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn the_native_render_presents_the_standard_codegens_signature_for_every_kind() {
    if !available() {
        return;
    }
    let root = package("matrix", MODULE, HOST_RS);
    let (ok, emitted, err) = almide(&root, &["src/main.almd", "--target", "rust"]);
    assert!(ok, "the standard codegen's emission failed:\n{err}");
    let (ok, out, err) = almide(&root, &["run", "src/main.almd"]);
    assert!(ok, "the package did not build and run:\n{err}");
    assert!(!err.contains("walled"), "the native render refused the package:\n{err}");
    assert_eq!(out, OUTPUT, "the native render's output differs from the previous release's");
    let rendered = std::fs::read_to_string(root.join("scratch/src/main.rs")).expect("the rendered source");
    assert!(rendered.contains("Almide v1 trust spine"), "the build did not ship the native render");
    for (name, sig) in SIGNATURES {
        assert!(emitted.contains(&format!("pub fn almide_rt_cb_{name}{sig} {{")),
            "`{name}`: the standard codegen no longer emits `{sig}`");
        // An all-scalar row has one host spelling and is adapted without the
        // standard codegen's text: same types, positional parameter names.
        let sig = if *name == SCALAR_ROW.0 { SCALAR_ROW.1 } else { sig };
        assert!(rendered.contains(&format!("pub fn almide_rt_cb_0v0_{name}{sig} {{")),
            "`{name}`: the native render's adapter is not `{sig}`");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_packages_tests_build_and_pass_on_the_native_lane() {
    if !available() {
        return;
    }
    let root = package("test", MODULE, HOST_RS);
    let (ok, out, err) = almide(&root, &["test", "src/cb_test.almd", "--target", "rust"]);
    assert!(ok, "the package's tests failed on the native lane:\n{out}{err}");
    assert!(out.contains("1 test in 1 file") || err.contains("1 test in 1 file"), "no test ran:\n{out}{err}");
    let _ = std::fs::remove_dir_all(&root);
}

const RECORD_MODULE: &str = r#"type Point = { x: Int, label: String }
type Shape = | Circle(Int) | Square(Int, Int) | Dot
pub fn greet(name: String) -> String = "hello ${name}"
pub fn mk(x: Int, label: String) -> Point = { x: x, label: label }
pub fn px(p: Point) -> Int = p.x
pub fn area(s: Shape) -> Int = match s { Circle(r) => 3 * r * r, Square(w, h) => w * h, Dot => 0 }
pub fn unit_sq(n: Int) -> Shape = if n == 0 then Dot else Square(n, n)
"#;

const RECORD_HOST_RS: &str = r#"use crate::*;
pub fn run_all() -> String {
    let p = almide_rt_cb_0v0_mk(3, AlmideString::from("a"));
    let dot = matches!(almide_rt_cb_0v0_unit_sq(0), almide_rt_cb_Shape::Dot);
    format!("{}{}|{}|{}|{}", p.x, p.label, almide_rt_cb_0v0_px(&p), almide_rt_cb_0v0_area(&almide_rt_cb_Shape::Square(2, 5)), dot)
}
"#;

/// The kinds that still have no adapter: a record or a variant in a
/// callback's signature. The standard codegen declares the host struct or
/// enum the native code names; the native render does not, and refuses the
/// callback by name. `run` keeps the standard codegen's build (the previous
/// release's output); the package's tests, which have no such route, do not
/// build. This row moves when those kinds are adapted.
#[test]
fn a_record_or_variant_callback_is_refused_by_name_and_still_runs_on_the_standard_codegen() {
    if !available() {
        return;
    }
    let root = package("record", RECORD_MODULE, RECORD_HOST_RS);
    let (ok, out, err) = almide(&root, &["run", "src/main.almd"]);
    assert!(ok, "the package did not build and run:\n{err}");
    assert_eq!(out, "3a|3|10|true\nhello main\n");
    assert!(err.contains("walled") && err.contains("(cb.Point) has no host adapter"), "the refusal is not named:\n{err}");
    let (ok, out, err) = almide(&root, &["test", "src/cb_test.almd", "--target", "rust"]);
    assert!(!ok, "a package the native render refuses built its tests:\n{out}{err}");
    assert!(format!("{out}{err}").contains("(cb.Point) has no host adapter"), "the refusal is not named:\n{out}{err}");
    let _ = std::fs::remove_dir_all(&root);
}
