//! A package's `native/*.rs` calling back into module fns that take a `mut`
//! parameter, or whose signatures name a generic record or variant or a
//! structural record (#4640).
//!
//! The standard codegen lends a `mut` parameter for writing (`xs: &mut
//! Vec<i64>`) and the native code reads the callee's writes through that
//! borrow, on the error arm of a failing call as well. It declares one
//! generic item for a generic type (`almide_rt_cb_Pair<A>`), and for a
//! structural record either the declared record type with those fields or
//! one item per label set (`AlmdRec_x_y<T0, T1>`, labels sorted). `almide
//! test` builds on the native render alone, which refused all of these by
//! name, so such a package stopped building its tests where the previous
//! release ran them.
//!
//! The matrix is one row per kind. `mut`: a scalar, a list, a string, a
//! record; one and several; a Unit return and a value; an effect fn that
//! cannot fail, one that fails by `err`, one that fails by `guard`, read
//! after an `Ok` and after an `Err`. Generic: a record at two instances,
//! taken and returned, in a list, inside a structural record, written
//! through `mut`; a variant with two parameters. Structural: all-scalar and
//! mixed, taken and returned, nested; one that is a declared record type
//! spelled in another label order; one that is an instance of a generic
//! declared type. `SIGNATURES` and `OUTPUT` are what the previous release
//! emitted and printed; the native render must present the same signatures
//! and the same type definitions (both also held against this compiler's own
//! standard-codegen emission) and print the same lines.

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

const MODULE: &str = r#"type Pair[A] = { a: A, b: A }
type Bag[A] = { tag: String, items: List[A] }
type Either[L, R] = | Left(L) | Right(R)
type Pt = { px: Int, py: Int }
type Boxed[T] = { item: T, n: Int }

pub fn greet(name: String) -> String = "hello ${name}"
pub fn push_one(mut xs: List[Int], n: Int) -> Int = {
  xs = xs + [n]
  list.len(xs)
}
pub fn bump(mut n: Int) -> Int = {
  n = n + 1
  n
}
pub fn wipe(mut xs: List[String]) -> Unit = {
  xs = []
}
pub fn two(mut a: List[Int], mut b: String, k: Int) -> List[Int] = {
  a = a + [k]
  b = b + "!"
  a
}
pub fn both(mut a: List[Int], mut b: String) -> Unit = {
  a = a + [7]
  b = b + "?"
}
pub effect fn eff(mut xs: List[Int], n: Int) -> Int = {
  xs = xs + [n]
  println("e")
  list.len(xs)
}
pub effect fn effr(mut xs: List[Int], n: Int) -> Result[Int, String] = {
  xs = xs + [n]
  if n > 5 then err("big") else ok(list.len(xs))
}
pub effect fn effu(mut s: String, n: Int) -> Unit = {
  s = s + "x"
  guard n < 3 else err("late")
  println(s)
}
pub fn flip(mut on: Bool, mut f: Float) -> Bool = {
  on = not on
  f = f * 2.0
  on
}
pub fn widen(mut p: Pair[Int], by: Int) -> Unit = {
  p = { a: p.a - by, b: p.b + by }
}
pub fn sw(p: Pair[Int]) -> Pair[Int] = { a: p.b, b: p.a }
pub fn names(p: Pair[String]) -> String = p.a + p.b
pub fn tagged(n: Int) -> Bag[Int] = { tag: "t${n}", items: [n, n + 1] }
pub fn count(t: Bag[String]) -> Int = string.len(t.tag) + list.len(t.items)
pub fn pick(n: Int) -> Either[Int, String] = if n > 0 then Left(n) else Right("neg")
pub fn side(e: Either[Int, String]) -> String = match e { Left(n) => "L${n}", Right(s) => "R${s}" }
pub fn pairs(n: Int) -> List[Pair[Int]] = [sw({ a: n, b: 0 }), { a: 1, b: n }]
pub fn sum(p: { x: Int, y: Int }) -> Int = p.x + p.y
pub fn tag(p: { name: String, n: Int }) -> String = p.name
pub fn mk(n: Int) -> { name: String, n: Int } = { name: "k", n: n }
pub fn nest(p: { at: Pair[Int], zs: List[String] }) -> Int = p.at.a + list.len(p.zs)
pub fn diff(p: { py: Int, px: Int }) -> Int = p.px - p.py
pub fn back(n: Int) -> { py: Int, px: Int } = { py: n, px: 0 }
pub fn open_box(b: { item: String, n: Int }) -> String = b.item
"#;

/// Each fn's parameter list and return type as the previous release emitted
/// them (`almide_rt_cb_<name>` there, `almide_rt_cb_0v0_<name>` linked).
const SIGNATURES: &[(&str, &str)] = &[
    ("push_one", "(xs: &mut Vec<i64>, n: i64) -> i64"),
    ("bump", "(n: &mut i64) -> i64"),
    ("wipe", "(xs: &mut Vec<AlmideString>) -> ()"),
    ("two", "(a: &mut Vec<i64>, b: &mut AlmideString, k: i64) -> Vec<i64>"),
    ("both", "(a: &mut Vec<i64>, b: &mut AlmideString) -> ()"),
    ("eff", "(xs: &mut Vec<i64>, n: i64) -> Result<i64, AlmideString>"),
    ("effr", "(xs: &mut Vec<i64>, n: i64) -> Result<i64, AlmideString>"),
    ("effu", "(s: &mut AlmideString, n: i64) -> Result<(), AlmideString>"),
    ("flip", "(on: &mut bool, f: &mut f64) -> bool"),
    ("widen", "(p: &mut almide_rt_cb_Pair<i64>, by: i64) -> ()"),
    ("sw", "(p: &almide_rt_cb_Pair<i64>) -> almide_rt_cb_Pair<i64>"),
    ("names", "(p: &almide_rt_cb_Pair<AlmideString>) -> AlmideString"),
    ("tagged", "(n: i64) -> almide_rt_cb_Bag<i64>"),
    ("count", "(t: &almide_rt_cb_Bag<AlmideString>) -> i64"),
    ("pick", "(n: i64) -> almide_rt_cb_Either<i64, AlmideString>"),
    ("side", "(e: almide_rt_cb_Either<i64, AlmideString>) -> AlmideString"),
    ("pairs", "(n: i64) -> Vec<almide_rt_cb_Pair<i64>>"),
    ("sum", "(p: &AlmdRec_x_y<i64, i64>) -> i64"),
    ("tag", "(p: &AlmdRec_n_name<i64, AlmideString>) -> AlmideString"),
    ("mk", "(n: i64) -> AlmdRec_n_name<i64, AlmideString>"),
    ("nest", "(p: &AlmdRec_at_zs<almide_rt_cb_Pair<i64>, Vec<AlmideString>>) -> i64"),
    ("diff", "(p: &almide_rt_cb_Pt) -> i64"),
    ("back", "(n: i64) -> almide_rt_cb_Pt"),
    ("open_box", "(b: &almide_rt_cb_Boxed<AlmideString>) -> AlmideString"),
];

/// The head of every item the previous release declared for the module's
/// types and the structural records its signatures name.
const TYPES: &[&str] = &["AlmdRec_at_zs<T0, T1>", "AlmdRec_n_name<T0, T1>", "AlmdRec_x_y<T0, T1>", "almide_rt_cb_Pair<A>", "almide_rt_cb_Bag<A>", "almide_rt_cb_Either<L, R>", "almide_rt_cb_Pt", "almide_rt_cb_Boxed<T>"];

/// Written against `SIGNATURES` and the standard codegen's definitions:
/// every row is called, and every result and every written place is read
/// back into one line.
const HOST_RS: &str = r#"use crate::*;
fn s(x: &str) -> AlmideString { AlmideString::from(x) }
type Pi = almide_rt_cb_Pair<i64>;
pub fn run_all() -> String {
    let mut out: Vec<String> = Vec::new();
    let mut n = 1i64;
    let r = almide_rt_cb_0v0_bump(&mut n);
    out.push(format!("{r}{n}"));
    let mut xs = vec![1i64, 2];
    out.push(almide_rt_cb_0v0_push_one(&mut xs, 9).to_string());
    let mut ss = vec![s("a")];
    almide_rt_cb_0v0_wipe(&mut ss);
    out.push(ss.len().to_string());
    let mut b = s("b");
    out.push(format!("{:?}", almide_rt_cb_0v0_two(&mut xs, &mut b, 4)));
    almide_rt_cb_0v0_both(&mut xs, &mut b);
    out.push(format!("{:?}", almide_rt_cb_0v0_eff(&mut xs, 5)));
    out.push(format!("{:?}", almide_rt_cb_0v0_effr(&mut xs, 3)));
    out.push(format!("{:?}", almide_rt_cb_0v0_effr(&mut xs, 8).map_err(|e| e.to_string())));
    out.push(format!("{:?}", almide_rt_cb_0v0_effu(&mut b, 1)));
    out.push(format!("{:?}", almide_rt_cb_0v0_effu(&mut b, 9).map_err(|e| e.to_string())));
    out.push(format!("{}{:?}", b.as_str(), xs));
    let (mut on, mut f) = (false, 1.5f64);
    let fl = almide_rt_cb_0v0_flip(&mut on, &mut f);
    out.push(format!("{fl}{on}{f}"));
    let mut p = Pi { a: 10, b: 20 };
    almide_rt_cb_0v0_widen(&mut p, 3);
    out.push(format!("{:?}", p));
    let q = almide_rt_cb_0v0_sw(&p);
    out.push(format!("{}{}{}", q.a, q.b, q == Pi { a: 23, b: 7 }));
    out.push(almide_rt_cb_0v0_names(&almide_rt_cb_Pair { a: s("x"), b: s("y") }).to_string());
    let t = almide_rt_cb_0v0_tagged(4);
    out.push(format!("{}{:?}", t.tag.as_str(), t.items));
    out.push(almide_rt_cb_0v0_count(&almide_rt_cb_Bag { tag: s("five!"), items: vec![s("i")] }).to_string());
    out.push(format!("{:?}{:?}", almide_rt_cb_0v0_pick(2), almide_rt_cb_0v0_pick(0)));
    out.push(almide_rt_cb_0v0_side(almide_rt_cb_Either::Right(s("r"))).to_string());
    out.push(format!("{:?}", almide_rt_cb_0v0_pairs(2)));
    out.push(almide_rt_cb_0v0_sum(&AlmdRec_x_y { x: 1i64, y: 2i64 }).to_string());
    out.push(almide_rt_cb_0v0_tag(&AlmdRec_n_name { n: 3i64, name: s("nm") }).to_string());
    let m = almide_rt_cb_0v0_mk(5);
    out.push(format!("{}{}", m.name.as_str(), m.n));
    out.push(almide_rt_cb_0v0_nest(&AlmdRec_at_zs { at: Pi { a: 6, b: 0 }, zs: vec![s("z")] }).to_string());
    out.push(format!("{}{:?}", almide_rt_cb_0v0_diff(&almide_rt_cb_Pt { px: 10, py: 3 }), almide_rt_cb_0v0_back(7)));
    out.push(almide_rt_cb_0v0_open_box(&almide_rt_cb_Boxed { item: s("it"), n: 1 }).to_string());
    out.join("|")
}
"#;

/// What the previous release printed for this package's `main`.
const OUTPUT: &str = r#"e
b!?x
22|3|0|[1, 2, 9, 4]|Ok(6)|Ok(7)|Err("big")|Ok(())|Err("late")|b!?xx[1, 2, 9, 4, 7, 5, 3, 8]|truetrue3|almide_rt_cb_Pair { a: 7, b: 23 }|237true|xy|t4[4, 5]|6|Left(2)Right("neg")|Rr|[almide_rt_cb_Pair { a: 0, b: 2 }, almide_rt_cb_Pair { a: 1, b: 2 }]|3|nm|k5|7|7almide_rt_cb_Pt { px: 0, py: 7 }|it
hello main
"#;

const EXTERN: &str = "import self as cb\n@extern(rust, \"host\", \"run_all\")\nfn run_all() -> String\n";

fn package(tag: &str, module: &str, host_rs: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue4640-shapes-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(root.join("almide.toml"), "[package]\nname = \"cb\"\nversion = \"0.1.0\"\n");
    write(root.join("src/mod.almd"), module);
    write(root.join("native/host.rs"), host_rs);
    write(root.join("src/main.almd"), &format!("{EXTERN}\neffect fn main() -> Unit = {{\n  println(run_all())\n  println(cb.greet(\"main\"))\n}}\n"));
    write(root.join("src/cb_test.almd"), &format!("{EXTERN}\ntest \"native code calls back\" {{\n  assert_eq(string.len(run_all()) > 0, true)\n  assert_eq(cb.greet(\"t\"), \"hello t\")\n}}\n"));
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

/// The item `text` declares under `head`, derives included.
fn definition<'a>(text: &'a str, head: &str) -> &'a str {
    let at = ["struct", "enum"].iter()
        .find_map(|kind| text.find(&format!("#[derive(Clone, Debug, PartialEq)]\npub {kind} {head} {{")))
        .unwrap_or_else(|| panic!("`{head}`: the standard codegen declares no such type"));
    let end = text[at..].find("\n}\n").expect("the item closes") + at;
    &text[at..end + 3]
}

#[test]
fn the_native_render_presents_the_standard_codegens_signatures_and_types_for_every_kind() {
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
        assert!(rendered.contains(&format!("pub fn almide_rt_cb_0v0_{name}{sig} {{")),
            "`{name}`: the native render's adapter is not `{sig}`");
    }
    for head in TYPES {
        let item = definition(&emitted, head);
        assert!(rendered.contains(item), "`{head}`: the native render does not carry\n{item}");
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

const UNCARRIED_MODULE: &str = r#"pub fn greet(name: String) -> String = "hello ${name}"
pub effect fn first_or_fail(mut xs: List[Int], n: Int) -> Int = {
  xs = xs + [n]
  let v = list.get(xs, 5)!
  v
}
"#;

/// Written against the previous release's signature:
/// `first_or_fail(xs: &mut Vec<i64>, n: i64) -> Result<i64, AlmideString>`.
const UNCARRIED_HOST_RS: &str = r#"use crate::*;
pub fn run_all() -> String {
    let mut xs = vec![1i64, 2];
    let r = almide_rt_cb_0v0_first_or_fail(&mut xs, 9);
    format!("{:?}{:?}", r.map_err(|e| e.to_string()), xs)
}
"#;

/// The `mut` kind that still has no adapter: a callback that can fail
/// through an Option-typed `!`. The native render returns a `mut` parameter
/// by value, and for this failure its error arm does not carry the
/// parameter, so the adapter would lose the write the previous release shows
/// after the `Err` (`[1, 2, 9]`). It is refused by name instead: `run` keeps
/// the standard codegen's build and that output; the package's tests, which
/// have no such route, do not build. This row moves when that error arm
/// carries its parameters.
#[test]
fn a_mut_callback_whose_error_arm_carries_nothing_is_refused_by_name_and_still_runs_on_the_standard_codegen() {
    if !available() {
        return;
    }
    let refusal = "has `mut` parameters, and it can fail without carrying them";
    let root = package("uncarried", UNCARRIED_MODULE, UNCARRIED_HOST_RS);
    let (ok, out, err) = almide(&root, &["run", "src/main.almd"]);
    assert!(ok, "the package did not build and run:\n{err}");
    assert_eq!(out, "Err(\"none\")[1, 2, 9]\nhello main\n");
    assert!(err.contains("walled") && err.contains(refusal), "the refusal is not named:\n{err}");
    let (ok, out, err) = almide(&root, &["test", "src/cb_test.almd", "--target", "rust"]);
    assert!(!ok, "a package the native render refuses built its tests:\n{out}{err}");
    assert!(format!("{out}{err}").contains(refusal), "the refusal is not named:\n{out}{err}");
    let _ = std::fs::remove_dir_all(&root);
}
