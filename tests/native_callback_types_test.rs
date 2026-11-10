//! A package's `native/*.rs` calling back into module fns whose signatures
//! name a record or a variant (#4640).
//!
//! Native code is written against the struct or enum the standard codegen
//! declares for a module type: it builds one, reads its fields, matches its
//! cases, prints it with `{:?}`. `almide test` builds on the native render
//! alone, which declared no such type, so a package like this one stopped
//! building its tests where the previous release ran them.
//!
//! The matrix is one row per kind: a record taken and returned, borrowed and
//! owned; a record nested in a record, a list, an option, a result, a tuple
//! and a map; an all-scalar record with a float and a bool; a variant with
//! unit, tuple and record-shaped cases; an all-scalar variant; a recursive
//! variant, by tuple case and by record case; an effect fn returning a
//! record. `SIGNATURES` and `OUTPUT` are what the previous release emitted
//! and printed; the native render must present the same signatures and the
//! same type definitions (both also held against this compiler's own
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

const MODULE: &str = r#"type Point = { x: Int, label: String }
type Shape = | Circle(Int) | Square(Int, Int) | Dot | Named(String)
type Line = { from: Point, to: Point, tags: List[String] }
type Pt = { y: Int, x: Int, w: Float, on: Bool }
type Sz = | Small | Big(Int) | Wide(Float, Bool)
type Ev = | Click { x: Int, name: String } | Key(String) | Moved(Point) | Idle
type Tree = | Leaf | Node(Tree, Int, Tree)
type Expr = | Num(Int) | Add { l: Expr, r: Expr } | Neg(Expr) | Var(String)
type Holder = { shape: Shape, at: Option[Point], evs: List[Ev] }

pub fn greet(name: String) -> String = "hello ${name}"
pub fn mk(x: Int, label: String) -> Point = { x: x, label: label }
pub fn px(p: Point) -> Int = p.x
pub fn relabel(p: Point, label: String) -> Point = { ...p, label: label }
pub fn area(s: Shape) -> Int = match s { Circle(r) => 3 * r * r, Square(w, h) => w * h, Dot => 0, Named(n) => string.len(n) }
pub fn unit_sq(n: Int) -> Shape = if n == 0 then Dot else Square(n, n)
pub fn named(s: String) -> Shape = Named(s)
pub fn span(l: Line) -> Int = l.to.x - l.from.x + list.len(l.tags)
pub fn line(a: Point, b: Point) -> Line = { from: a, to: b, tags: [a.label, b.label] }
pub fn points(n: Int) -> List[Point] = list.range(0, n) |> list.map((i) => mk(i, "p${i}"))
pub fn find(ps: List[Point], x: Int) -> Option[Point] = ps |> list.find((p) => p.x == x)
pub fn opt_x(p: Option[Point]) -> Int = match p { some(v) => v.x, none => -1 }
pub fn checked(x: Int) -> Result[Point, String] = if x < 0 then err("negative ${x}") else ok(mk(x, "ok"))
pub fn res_x(r: Result[Point, String]) -> Int = match r { ok(p) => p.x, err(e) => string.len(e) }
pub fn pt(y: Int, x: Int) -> Pt = { y: y, x: x, w: 1.5, on: y > x }
pub fn pt_sum(p: Pt) -> Float = if p.on then p.w + float.from(p.y - p.x) else 0.0 - p.w
pub fn flip(p: Pt) -> Pt = { y: p.x, x: p.y, w: p.w * 2.0, on: not p.on }
pub fn sz(n: Int) -> Sz = if n == 0 then Small else if n < 10 then Big(n) else Wide(float.from(n), true)
pub fn sz_n(s: Sz) -> Int = match s { Small => 0, Big(n) => n, Wide(w, on) => if on then int.from_saturating(w) else -1 }
pub fn ev_name(e: Ev) -> String = match e { Click { x, name } => "${name}@${x}", Key(k) => "key ${k}", Moved(p) => "moved ${p.label}", Idle => "idle" }
pub fn click(x: Int, name: String) -> Ev = Click { x: x, name: name }
pub fn moved(p: Point) -> Ev = Moved(p)
pub fn grow(n: Int) -> Tree = if n == 0 then Leaf else Node(grow(n - 1), n, Leaf)
pub fn depth(t: Tree) -> Int = match t { Leaf => 0, Node(l, _, r) => 1 + int.max(depth(l), depth(r)) }
pub fn eval(e: Expr) -> Int = match e { Num(n) => n, Add { l, r } => eval(l) + eval(r), Neg(x) => 0 - eval(x), Var(_) => 100 }
pub fn double(e: Expr) -> Expr = Add { l: e, r: Neg(Num(1)) }
pub fn hold(s: Shape) -> Holder = { shape: s, at: some(mk(7, "h")), evs: [Idle, Key("z")] }
pub fn held(h: Holder) -> Int = area(h.shape) + opt_x(h.at) + list.len(h.evs)
pub fn both(p: Point) -> (Int, Point) = (p.x, p)
pub fn by_label(ps: List[Point]) -> Map[String, Point] = ps |> list.map((p) => (p.label, p)) |> map.from_list
pub effect fn load(x: Int) -> Point = {
  println("load ${x}")
  mk(x, "loaded")
}
pub effect fn load_checked(x: Int) -> Result[Point, String] = if x < 0 then err("neg") else ok(mk(x, "lc"))
"#;

/// Each fn's parameter list and return type as the previous release emitted
/// them (`almide_rt_cb_<name>` there, `almide_rt_cb_0v0_<name>` linked).
const SIGNATURES: &[(&str, &str)] = &[
    ("mk", "(x: i64, label: AlmideString) -> almide_rt_cb_Point"),
    ("px", "(p: &almide_rt_cb_Point) -> i64"),
    ("relabel", "(p: almide_rt_cb_Point, label: AlmideString) -> almide_rt_cb_Point"),
    ("area", "(s: &almide_rt_cb_Shape) -> i64"),
    ("unit_sq", "(n: i64) -> almide_rt_cb_Shape"),
    ("named", "(s: AlmideString) -> almide_rt_cb_Shape"),
    ("span", "(l: &almide_rt_cb_Line) -> i64"),
    ("line", "(a: &almide_rt_cb_Point, b: &almide_rt_cb_Point) -> almide_rt_cb_Line"),
    ("points", "(n: i64) -> Vec<almide_rt_cb_Point>"),
    ("find", "(ps: Vec<almide_rt_cb_Point>, x: i64) -> Option<almide_rt_cb_Point>"),
    ("opt_x", "(p: Option<almide_rt_cb_Point>) -> i64"),
    ("checked", "(x: i64) -> Result<almide_rt_cb_Point, AlmideString>"),
    ("res_x", "(r: Result<almide_rt_cb_Point, AlmideString>) -> i64"),
    ("pt", "(y: i64, x: i64) -> almide_rt_cb_Pt"),
    ("pt_sum", "(p: &almide_rt_cb_Pt) -> f64"),
    ("flip", "(p: &almide_rt_cb_Pt) -> almide_rt_cb_Pt"),
    ("sz", "(n: i64) -> almide_rt_cb_Sz"),
    ("sz_n", "(s: &almide_rt_cb_Sz) -> i64"),
    ("ev_name", "(e: almide_rt_cb_Ev) -> AlmideString"),
    ("click", "(x: i64, name: AlmideString) -> almide_rt_cb_Ev"),
    ("moved", "(p: almide_rt_cb_Point) -> almide_rt_cb_Ev"),
    ("grow", "(n: i64) -> almide_rt_cb_Tree"),
    ("depth", "(t: &almide_rt_cb_Tree) -> i64"),
    ("eval", "(e: &almide_rt_cb_Expr) -> i64"),
    ("double", "(e: almide_rt_cb_Expr) -> almide_rt_cb_Expr"),
    ("hold", "(s: almide_rt_cb_Shape) -> almide_rt_cb_Holder"),
    ("held", "(h: &almide_rt_cb_Holder) -> i64"),
    ("both", "(p: almide_rt_cb_Point) -> (i64, almide_rt_cb_Point)"),
    ("by_label", "(ps: Vec<almide_rt_cb_Point>) -> AlmideMap<AlmideString, almide_rt_cb_Point>"),
    ("load", "(x: i64) -> Result<almide_rt_cb_Point, AlmideString>"),
    ("load_checked", "(x: i64) -> Result<almide_rt_cb_Point, AlmideString>"),
];

/// The module's types, each declared by the standard codegen under this name.
const TYPES: &[&str] = &["Point", "Shape", "Line", "Pt", "Sz", "Ev", "Tree", "Expr", "Holder"];

/// Written against `SIGNATURES` and the standard codegen's definitions:
/// every row is called, and every result is read back into one line.
const HOST_RS: &str = r#"use crate::*;
fn s(x: &str) -> AlmideString { AlmideString::from(x) }
type P = almide_rt_cb_Point;
pub fn run_all() -> String {
    let mut out: Vec<String> = Vec::new();
    let p = almide_rt_cb_0v0_mk(3, s("a"));
    out.push(format!("{}{}", p.x, p.label));
    out.push(almide_rt_cb_0v0_px(&p).to_string());
    let q = almide_rt_cb_0v0_relabel(p.clone(), s("b"));
    out.push(format!("{:?}", q == P { x: 3, label: s("b") }));
    out.push(almide_rt_cb_0v0_area(&almide_rt_cb_Shape::Square(2, 5)).to_string());
    out.push(almide_rt_cb_0v0_area(&almide_rt_cb_0v0_named(s("four"))).to_string());
    out.push(format!("{:?}", matches!(almide_rt_cb_0v0_unit_sq(0), almide_rt_cb_Shape::Dot)));
    out.push(format!("{:?}", matches!(almide_rt_cb_0v0_unit_sq(2), almide_rt_cb_Shape::Square(2, 2))));
    let l = almide_rt_cb_0v0_line(&p, &q);
    out.push(format!("{}{}", l.tags.len(), almide_rt_cb_0v0_span(&l)));
    let ps = almide_rt_cb_0v0_points(3);
    out.push(format!("{}{:?}", ps.len(), almide_rt_cb_0v0_find(ps.clone(), 2).map(|p| p.label.to_string())));
    out.push(format!("{}{}", almide_rt_cb_0v0_opt_x(Some(p.clone())), almide_rt_cb_0v0_opt_x(None)));
    out.push(format!("{:?}{:?}", almide_rt_cb_0v0_checked(4).map(|p| p.x), almide_rt_cb_0v0_checked(-4).map(|p| p.x).map_err(|e| e.to_string())));
    out.push(format!("{}{}", almide_rt_cb_0v0_res_x(Ok(q.clone())), almide_rt_cb_0v0_res_x(Err(s("four")))));
    let t = almide_rt_cb_0v0_pt(9, 2);
    out.push(format!("{}{}{}{}", t.y, t.x, t.w, t.on));
    out.push(almide_rt_cb_0v0_pt_sum(&t).to_string());
    let f = almide_rt_cb_0v0_flip(&almide_rt_cb_Pt { y: 1, x: 5, w: 0.25, on: false });
    out.push(format!("{:?}", f));
    out.push(format!("{:?}{:?}{:?}", almide_rt_cb_0v0_sz(0), almide_rt_cb_0v0_sz(5), almide_rt_cb_0v0_sz(12)));
    out.push(format!("{}{}", almide_rt_cb_0v0_sz_n(&almide_rt_cb_Sz::Big(8)), almide_rt_cb_0v0_sz_n(&almide_rt_cb_Sz::Wide(3.0, false))));
    out.push(almide_rt_cb_0v0_ev_name(almide_rt_cb_Ev::Click { x: 4, name: s("btn") }).to_string());
    out.push(almide_rt_cb_0v0_ev_name(almide_rt_cb_0v0_moved(q.clone())).to_string());
    out.push(format!("{:?}", almide_rt_cb_0v0_click(1, s("c"))));
    let tree = almide_rt_cb_0v0_grow(3);
    out.push(almide_rt_cb_0v0_depth(&tree).to_string());
    if let almide_rt_cb_Tree::Node(l, n, _) = &tree { out.push(format!("{}{}", n, almide_rt_cb_0v0_depth(l))); }
    let e = almide_rt_cb_0v0_double(almide_rt_cb_Expr::Var(s("v")));
    out.push(almide_rt_cb_0v0_eval(&e).to_string());
    out.push(format!("{:?}", e));
    let h = almide_rt_cb_0v0_hold(almide_rt_cb_Shape::Circle(1));
    out.push(format!("{}{}", h.evs.len(), almide_rt_cb_0v0_held(&h)));
    let (n, bp) = almide_rt_cb_0v0_both(p.clone());
    out.push(format!("{}{}", n, bp.label));
    out.push(format!("{:?}", almide_rt_cb_0v0_by_label(ps).get(&s("p1")).map(|p| p.x)));
    out.push(format!("{:?}", almide_rt_cb_0v0_load(6).map(|p| p.label.to_string()).map_err(|e| e.to_string())));
    out.push(format!("{:?}", almide_rt_cb_0v0_load_checked(-1).map(|p| p.x).map_err(|e| e.to_string())));
    out.join("|")
}
"#;

/// What the previous release printed for this package's `main`.
const OUTPUT: &str = r#"load 6
3a|3|true|10|4|true|true|22|3Some("p2")|3-1|Ok(4)Err("negative -4")|34|921.5true|8.5|almide_rt_cb_Pt { y: 5, x: 1, w: 0.5, on: true }|SmallBig(5)Wide(12.0, true)|8-1|btn@4|moved b|Click { x: 1, name: "c" }|3|32|99|Add { l: Var("v"), r: Neg(Num(1)) }|212|3a|Some(1)|Ok("loaded")|Err("neg")
hello main
"#;

const EXTERN: &str = "import self as cb\n@extern(rust, \"host\", \"run_all\")\nfn run_all() -> String\n";

fn package(tag: &str, module: &str, host_rs: &str, line: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue4640-types-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(root.join("almide.toml"), "[package]\nname = \"cb\"\nversion = \"0.1.0\"\n");
    write(root.join("src/mod.almd"), module);
    write(root.join("native/host.rs"), host_rs);
    write(root.join("src/main.almd"), &format!("{EXTERN}\neffect fn main() -> Unit = {{\n  println(run_all())\n  println(cb.greet(\"main\"))\n}}\n"));
    let line = line.replace('\\', "\\\\").replace('"', "\\\"");
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

/// The item `text` declares the module type `name` with, derives included.
fn definition<'a>(text: &'a str, name: &str) -> &'a str {
    let at = ["struct", "enum"].iter()
        .find_map(|kind| text.find(&format!("#[derive(Clone, Debug, PartialEq)]\npub {kind} almide_rt_cb_{name} {{")))
        .unwrap_or_else(|| panic!("`{name}`: the standard codegen declares no such type"));
    let end = text[at..].find("\n}\n").expect("the item closes") + at;
    &text[at..end + 3]
}

#[test]
fn the_native_render_carries_the_standard_codegens_types_and_signatures_for_every_kind() {
    if !available() {
        return;
    }
    let root = package("matrix", MODULE, HOST_RS, OUTPUT.lines().nth(1).unwrap());
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
    for name in TYPES {
        let item = definition(&emitted, name);
        assert!(rendered.contains(item), "`{name}`: the native render does not carry\n{item}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_packages_tests_build_and_pass_on_the_native_lane() {
    if !available() {
        return;
    }
    let root = package("test", MODULE, HOST_RS, OUTPUT.lines().nth(1).unwrap());
    let (ok, out, err) = almide(&root, &["test", "src/cb_test.almd", "--target", "rust"]);
    assert!(ok, "the package's tests failed on the native lane:\n{out}{err}");
    assert!(out.contains("1 test in 1 file") || err.contains("1 test in 1 file"), "no test ran:\n{out}{err}");
    let _ = std::fs::remove_dir_all(&root);
}

const GENERIC_MODULE: &str = r#"pub fn greet(name: String) -> String = "hello ${name}"
pub fn ident[T](x: T) -> T = x
pub fn use_it() -> Int = ident(3)
"#;

const GENERIC_HOST_RS: &str = r#"use crate::*;
pub fn run_all() -> String {
    format!("{}", almide_rt_cb_0v0_ident(7i64))
}
"#;

/// Not a kind the previous release ran: a generic module fn has no item under
/// its linked name on either route (each instance is emitted under a name of
/// its own), so native code that calls it back never linked. The release
/// failed this package with the same compiler error; nothing is adapted, and
/// nothing regressed.
#[test]
fn a_generic_module_fn_has_no_linked_name_to_call_back() {
    if !available() {
        return;
    }
    let root = package("generic", GENERIC_MODULE, GENERIC_HOST_RS, "7");
    let (ok, out, err) = almide(&root, &["run", "src/main.almd"]);
    assert!(!ok, "native code linked a generic module fn by name:\n{out}{err}");
    assert!(err.contains("cannot find function `almide_rt_cb_0v0_ident`"), "the failure is not the unlinked name:\n{err}");
    let _ = std::fs::remove_dir_all(&root);
}
