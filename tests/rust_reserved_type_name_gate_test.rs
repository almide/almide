//! #2842 gate: a user type whose name the generated Rust crate already uses
//! (`Option`, `Result`, `String`, `HashMap`, …) builds and runs like any other.
//!
//! A module's types normally reach Rust mangled (`almide_rt_<mod>_<Type>`),
//! but two kinds of declaration arrive under their bare name: the entry
//! program's own types, and the types of a package module whose key is a stdlib
//! module name (`src/option.almd`, key `option`). Either one declaring
//! `type Option` emitted `pub struct Option` at the crate root, and every
//! runtime `Option<A>` then named the user's struct (rustc E0107). The same
//! module reached as a dependency (`cellpkg.option`) was mangled and built.
//!
//! Cells are enumerated, never hand-listed:
//!   * every stdlib module name `m` (`STDLIB_MODULES` ∪ `BUNDLED_MODULES`) as a
//!     package module declaring `type <M>`, in-package and as a dependency,
//!     native and wasm;
//!   * every name in the codegen's `RUST_RESERVED_TYPE_NAMES`, declared by the
//!     entry program, native and wasm.
//!
//! A cell the checker rejects is a frontend matter, not this gate's: it is
//! filtered out by `almide check` in the reference shape and printed, and a
//! floor on the surviving count keeps a green run from coming from cells that
//! never ran.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

/// `ALMIDE_BIN` runs the cells against another build (A/B); the default is
/// this workspace's binary.
fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, content).expect("write");
}

fn almide_in(dir: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(almide()).current_dir(dir).args(args).output().expect("run almide");
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

fn pascal(module: &str) -> String {
    module
        .split('_')
        .map(|p| {
            let mut c = p.chars();
            c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
        })
        .collect()
}

fn stdlib_module_names() -> BTreeSet<String> {
    let s = &almide_lang::stdlib_info::STDLIB_MODULES;
    let b = &almide_lang::stdlib_info::BUNDLED_MODULES;
    let names: BTreeSet<String> = s.iter().chain(b.iter()).map(|m| m.to_string()).collect();
    assert!(names.contains("option") && names.contains("result"), "the stdlib registry looks truncated: {names:?}");
    names
}

fn cell_module(ty: &str) -> String {
    format!("type {ty} = {{ text: String }}\n\nfn build(t: String) -> {ty} = {ty} {{ text: t }}\n")
}

const PKG_TOML: &str = "[package]\nname = \"cellpkg\"\nversion = \"0.1.0\"\n";
const APP_TOML: &str =
    "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ncellpkg = { path = \"../cellpkg\" }\n";

/// A package with one module per stdlib name in `modules`. In-package, the
/// entry imports them all; as a dependency, `mod.almd` does and the app calls
/// `cellpkg.run()`. Either way the program prints `ok-<m>` once per module.
fn stdlib_named_package(root: &Path, modules: &[String], as_dependency: bool) -> std::path::PathBuf {
    let pkg = root.join("cellpkg");
    write(&pkg.join("almide.toml"), PKG_TOML);
    for m in modules {
        write(&pkg.join(format!("src/{m}.almd")), &cell_module(&pascal(m)));
    }
    let imports: String = modules.iter().map(|m| format!("import self.{m}\n")).collect();
    let parts: Vec<String> = modules.iter().map(|m| format!("{m}.build(\"ok-{m}\").text")).collect();
    if as_dependency {
        write(&pkg.join("src/mod.almd"), &format!("{imports}\nfn run() -> List[String] = [{}]\n", parts.join(", ")));
        let app = root.join("app");
        write(&app.join("almide.toml"), APP_TOML);
        write(
            &app.join("src/main.almd"),
            "import cellpkg\n\neffect fn main() -> Unit = {\n  for line in cellpkg.run() {\n    println(line)\n  }\n}\n",
        );
        app
    } else {
        let body: String = parts.iter().map(|p| format!("  println({p})\n")).collect();
        write(&pkg.join("src/main.almd"), &format!("{imports}\neffect fn main() -> Unit = {{\n{body}}}\n"));
        pkg
    }
}

/// The stdlib-name cells the checker accepts as a DEPENDENCY — the shape that
/// always mangled, so the reference for what the codegen must also build
/// in-package.
fn applicable_stdlib_named_cells() -> Vec<String> {
    let mut ok = Vec::new();
    let mut rejected = Vec::new();
    for m in stdlib_module_names() {
        let root = tempfile::tempdir().expect("tempdir");
        let app = stdlib_named_package(root.path(), std::slice::from_ref(&m), true);
        match almide_in(&app, &["check", "src/main.almd"]) {
            (true, _) => ok.push(m),
            (false, _) => rejected.push(m),
        }
    }
    eprintln!("stdlib-name cells rejected by the checker as a dependency (not this gate's): {rejected:?}");
    assert!(ok.len() >= 20, "only {} stdlib-name cells check as a dependency: {ok:?}", ok.len());
    assert!(ok.iter().any(|m| m == "option") && ok.iter().any(|m| m == "result"), "the reported cells must run: {ok:?}");
    ok
}

fn assert_prints_every_cell(shape: &str, (ok, out): (bool, String), modules: &[String]) {
    assert!(ok, "{shape}: the build failed:\n{out}");
    let missing: Vec<&String> = modules.iter().filter(|m| !out.lines().any(|l| l == format!("ok-{m}"))).collect();
    assert!(missing.is_empty(), "{shape}: no output for {missing:?}:\n{out}");
}

#[test]
fn a_package_module_named_after_a_stdlib_module_builds_its_types_in_both_shapes() {
    let modules = applicable_stdlib_named_cells();
    for as_dependency in [false, true] {
        let shape = if as_dependency { "dependency" } else { "in-package" };
        let root = tempfile::tempdir().expect("tempdir");
        let dir = stdlib_named_package(root.path(), &modules, as_dependency);
        assert_prints_every_cell(&format!("{shape} native"), almide_in(&dir, &["run", "src/main.almd"]), &modules);
        assert_prints_every_cell(
            &format!("{shape} wasm"),
            almide_in(&dir, &["run", "src/main.almd", "--target", "wasm"]),
            &modules,
        );
    }
}

/// The issue's reported cell, alone and end to end, repr included: the
/// printed type name is the declared one, not the mangle.
#[test]
fn a_package_module_option_declaring_option_runs_in_package() {
    let root = tempfile::tempdir().expect("tempdir");
    let pkg = root.path();
    write(&pkg.join("almide.toml"), PKG_TOML);
    write(&pkg.join("src/option.almd"), &cell_module("Option"));
    write(
        &pkg.join("src/main.almd"),
        "import self.option\n\neffect fn main() -> Unit = {\n  let o = option.build(\"ok\")\n  println(o.text)\n  println(\"${o}\")\n}\n",
    );
    for (target, args) in [("native", &["run", "src/main.almd"][..]), ("wasm", &["run", "src/main.almd", "--target", "wasm"][..])] {
        let (ok, out) = almide_in(pkg, args);
        assert!(ok && out.contains("ok\nOption { text: \"ok\" }"), "{target}:\n{out}");
    }
}

/// Every reserved name, declared by the ENTRY program, in one program that
/// also uses the builtin `Option` / `Result` the runtime is written in.
#[test]
fn the_entry_program_may_declare_every_rust_reserved_type_name() {
    let reserved = almide_codegen::pass_ir_link_flatten::RUST_RESERVED_TYPE_NAMES;
    let mut names = Vec::new();
    let mut rejected = Vec::new();
    for name in reserved {
        let root = tempfile::tempdir().expect("tempdir");
        write(&root.path().join("t.almd"), &format!("{}\neffect fn main() -> Unit = println(build(\"x\").text)\n", cell_module(name)));
        match almide_in(root.path(), &["check", "t.almd"]) {
            (true, _) => names.push(name.to_string()),
            (false, _) => rejected.push(name.to_string()),
        }
    }
    eprintln!("reserved names the checker rejects as a declaration (not this gate's): {rejected:?}");
    assert!(names.len() >= 30, "only {} reserved names check: {names:?}", names.len());
    assert!(names.iter().any(|n| n == "Option") && names.iter().any(|n| n == "Result"), "{names:?}");

    let decls: String = names
        .iter()
        .map(|n| format!("type {n} = {{ text_{}: String }}\n", n.to_ascii_lowercase()))
        .collect();
    let body: String = names
        .iter()
        .map(|n| format!("  println(\"${{{n} {{ text_{}: \"ok-{n}\" }}}}\")\n", n.to_ascii_lowercase()))
        .collect();
    let program = format!(
        "{decls}\neffect fn main() -> Unit = {{\n{body}  let first: Option[Int] = list.first([7])\n  let parsed: Result[Int, String] = int.parse(\"8\")\n  println(int.to_string((first ?? 0) + (parsed ?? 0)))\n}}\n"
    );
    let root = tempfile::tempdir().expect("tempdir");
    write(&root.path().join("t.almd"), &program);
    for (target, args) in [("native", &["run", "t.almd"][..]), ("wasm", &["run", "t.almd", "--target", "wasm"][..])] {
        let (ok, out) = almide_in(root.path(), args);
        assert!(ok, "{target}: the build failed:\n{out}");
        let missing: Vec<&String> = names
            .iter()
            .filter(|n| !out.contains(&format!("{n} {{ text_{}: \"ok-{n}\" }}", n.to_ascii_lowercase())))
            .collect();
        assert!(missing.is_empty() && out.contains("15"), "{target}: no output for {missing:?}:\n{out}");
    }
}

/// Each declaration shape under a reserved name, beside the shapes that spell
/// the same name as a CONSTRUCTOR: a variant whose case shares the type's name
/// (`type Box[T] = | Box(T)`, the case's ctor passed as a fn value and matched
/// — the case must keep its name), an opaque newtype (its ctor IS the type), a
/// record (literal and pattern), a variant with a record case.
#[test]
fn every_declaration_shape_under_a_reserved_name_builds_on_both_targets() {
    let program = "\
type Box[T] = | Box(T) | NoBox

type Option = | Full { n: Int } | Empty

mod type Result = Int

type Vec = { n: Int }

fn unbox(b: Box[Int]) -> Int = match b {
  Box(n) => n,
  NoBox => 0,
}

fn opt(o: Option) -> Int = match o {
  Full { n } => n,
  Empty => 0,
}

effect fn main() -> Unit = {
  let xs = [1, 2, 3] |> list.map(Box)
  println(int.to_string(xs |> list.map(unbox) |> list.sum))
  println(int.to_string(opt(Full { n: 4 }) + opt(Empty)))
  let r = Result(5)
  match r { Result(k) => println(int.to_string(k)) }
  let v = Vec { n: 6 }
  match v { Vec { n } => println(int.to_string(n)) }
  println(\"${xs} ${Full { n: 1 }} ${v}\")
}
";
    let root = tempfile::tempdir().expect("tempdir");
    write(&root.path().join("t.almd"), program);
    let want = "6\n4\n5\n6\n[Box(1), Box(2), Box(3)] Full { n: 1 } Vec { n: 6 }\n";
    for (target, args) in [("native", &["run", "t.almd"][..]), ("wasm", &["run", "t.almd", "--target", "wasm"][..])] {
        let (ok, out) = almide_in(root.path(), args);
        assert!(ok && out.ends_with(want), "{target}: expected\n{want}got:\n{out}");
    }
}
