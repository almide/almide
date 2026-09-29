//! A qualified (cross-module) variant constructor hands its payload types to
//! its arguments exactly as the bare constructor and a fn parameter do (#2925).
//!
//! `tree.Node("n", [])` was E018 "cannot infer the element type of empty list"
//! while `Node("n", [])` in the owning module checked: the qualified call kept
//! its own copy of the payload unification, gated on the type being generic.
//! Both calls now share `check_positional_ctor_call`, and both constructor-as-
//! value forms share `ctor_fn_value_ty`.
//!
//! The family, in a package module (`import self.tree as tree`) and in a path
//! DEPENDENCY (`import shapes`): an empty list / empty map / `none`-shaped
//! payload of a tuple case, nested inside a tuple payload, inside
//! `Option[List[T]]` and `Map[K, List[V]]`, a record case, a generic variant,
//! and the constructor used as a function value (which also used to lower to
//! a call of a function named `tree.Node` that does not exist). Each program
//! must check, and native and wasm must print the same, expected, output.

use std::path::Path;
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

const TYPES: &str = r#"type Arg = | Leaf(Int) | Node(String, List[Arg])
type Rec = | Rec { xs: List[Int], m: Map[String, Int] } | Empty
type Tup = | Pair((List[Int], String))
type Opt = | Maybe(Option[List[String]]) | Dict(Map[String, List[Int]])
type Box[T] = | Box(List[T]) | Nothing

fn count(a: Arg) -> Int = match a { Leaf(_) => 1, Node(_, xs) => list.len(xs) }
fn rec_len(r: Rec) -> Int = match r { Rec { xs, m } => list.len(xs) + map.len(m), Empty => 0 }
fn tup_len(t: Tup) -> Int = match t { Pair((xs, s)) => list.len(xs) + string.len(s) }
fn opt_len(o: Opt) -> Int = match o { Maybe(x) => list.len(x ?? []), Dict(m) => map.len(m) }
fn box_len[T](b: Box[T]) -> Int = match b { Box(xs) => list.len(xs), Nothing => 0 }
"#;

/// The body of `main`, over a module spelled `M`.
const BODY: &str = r#"  println(int.to_string(M.count(M.Node("n", []))))
  println(int.to_string(M.count(M.Node("n", [M.Leaf(1), M.Node("m", [])]))))
  println(int.to_string(M.rec_len(M.Rec { xs: [], m: [:] })))
  println(int.to_string(M.tup_len(M.Pair(([], "ab")))))
  println(int.to_string(M.opt_len(M.Maybe(some([])))))
  println(int.to_string(M.opt_len(M.Maybe(none))))
  println(int.to_string(M.opt_len(M.Dict([:]))))
  let b: M.Box[Int] = M.Box([])
  println(int.to_string(M.box_len(b)))
  println(int.to_string(M.box_len(M.Box(["x", "y"]))))
  let mk = M.Node
  println(int.to_string(M.count(mk("v", []))))
  let boxes = [[1], [2, 3]] |> list.map(M.Box)
  println(int.to_string(boxes |> list.map((x) => M.box_len(x)) |> list.sum))
"#;

const EXPECTED: &str = "0\n2\n0\n2\n0\n0\n0\n0\n2\n0\n3\n";

fn main_src(import: &str, module: &str) -> String {
    format!("{}\n\neffect fn main() -> Unit = {{\n{}}}\n", import, BODY.replace("M.", &format!("{}.", module)))
}

fn run(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(almide_bin()).args(args).current_dir(dir).output().unwrap()
}

fn assert_checks_and_agrees(app: &Path) {
    let check = run(app, &["check", "src/main.almd"]);
    assert!(
        check.status.success(),
        "a qualified constructor's payload gave no type to its argument:\n{}",
        String::from_utf8_lossy(&check.stderr)
    );
    let native = run(app, &["run", "src/main.almd"]);
    assert!(native.status.success(), "native run failed:\n{}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout), EXPECTED);
    let wasm = run(app, &["run", "src/main.almd", "--target", "wasm"]);
    assert!(wasm.status.success(), "wasm run failed:\n{}", String::from_utf8_lossy(&wasm.stderr));
    assert_eq!(wasm.stdout, native.stdout, "wasm/native divergence");
}

fn write(root: &Path, files: &[(&str, String)]) {
    for (rel, text) in files {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

#[test]
fn qualified_ctor_payload_types_its_arguments_in_a_package_module() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), &[
        ("app/almide.toml", "[package]\nname = \"app\"\nversion = \"0.1.0\"\n".to_string()),
        ("app/src/tree.almd", TYPES.to_string()),
        ("app/src/main.almd", main_src("import self.tree as tree", "tree")),
    ]);
    assert_checks_and_agrees(&root.path().join("app"));
}

#[test]
fn qualified_ctor_payload_types_its_arguments_in_a_dependency() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), &[
        ("shapes/almide.toml", "[package]\nname = \"shapes\"\nversion = \"0.1.0\"\n".to_string()),
        ("shapes/src/mod.almd", TYPES.to_string()),
        ("app/almide.toml", "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nshapes = { path = \"../shapes\" }\n".to_string()),
        ("app/src/main.almd", main_src("import shapes", "shapes")),
    ]);
    assert_checks_and_agrees(&root.path().join("app"));
}

/// The bare constructor inside its own module, the shape that always checked:
/// the shared code must keep it exactly as it was.
#[test]
fn bare_ctor_payload_still_types_its_arguments() {
    let root = tempfile::tempdir().unwrap();
    let body = BODY.replace("M.", "");
    write(root.path(), &[
        ("app/almide.toml", "[package]\nname = \"app\"\nversion = \"0.1.0\"\n".to_string()),
        ("app/src/main.almd", format!("{}\neffect fn main() -> Unit = {{\n{}}}\n", TYPES, body)),
    ]);
    assert_checks_and_agrees(&root.path().join("app"));
}

/// Sharing the unification does not add a second diagnostic to a wrong
/// payload: a qualified constructor still reports exactly the E005s the bare
/// one does, and the empty list beside the bad argument is not an E018.
#[test]
fn qualified_ctor_wrong_payload_is_one_e005_per_argument() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), &[
        ("app/almide.toml", "[package]\nname = \"app\"\nversion = \"0.1.0\"\n".to_string()),
        ("app/src/tree.almd", TYPES.to_string()),
        ("app/src/main.almd", "import self.tree as tree\n\neffect fn main() -> Unit = {\n  println(int.to_string(tree.count(tree.Node(1, []))))\n}\n".to_string()),
    ]);
    let check = run(&root.path().join("app"), &["check", "src/main.almd"]);
    assert!(!check.status.success());
    let stderr = String::from_utf8_lossy(&check.stderr);
    let errors: Vec<&str> = stderr.lines().filter(|l| l.starts_with("error[")).collect();
    assert_eq!(errors, vec!["error[E005]: Node() argument 1 expects String but got Int"], "{}", stderr);
}
