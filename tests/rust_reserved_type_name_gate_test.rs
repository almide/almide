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
//! Every cell must check and run: none is filtered out. A file's own
//! declaration answers its bare spelling, so `type Int` in `src/int.almd` or
//! `type String` in the entry program is that file's type in every position
//! (#2858, module-system §4.5). The cells therefore carry a `Bool` payload,
//! the one builtin no cell declares, instead of spelling the builtin their
//! own declaration takes over.

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

/// One cell's module: `type <ty>` built by `build`. The payload is a `Bool`,
/// never the declared name, so the signature and the literal both name the
/// declared type whatever builtin it shares a name with.
fn cell_module(ty: &str) -> String {
    format!("type {ty} = {{ ok: Bool }}\n\nfn build(b: Bool) -> {ty} = {ty} {{ ok: b }}\n")
}

/// The line a cell prints when its value came back through its own type.
fn cell_line(call: &str, tag: &str) -> String {
    format!("if {call}.ok then \"ok-{tag}\" else \"lost-{tag}\"")
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
    let parts: Vec<String> = modules.iter().map(|m| cell_line(&format!("{m}.build(true)"), m)).collect();
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

/// Every stdlib-name cell, each of which must check as a dependency on its
/// own — the reference shape, so a failure names its module.
fn applicable_stdlib_named_cells() -> Vec<String> {
    let cells: Vec<String> = stdlib_module_names().into_iter().collect();
    for m in &cells {
        let root = tempfile::tempdir().expect("tempdir");
        let app = stdlib_named_package(root.path(), std::slice::from_ref(m), true);
        let (ok, out) = almide_in(&app, &["check", "src/main.almd"]);
        assert!(ok, "cell `{m}` (`type {}` in src/{m}.almd) does not check as a dependency:\n{out}", pascal(m));
    }
    cells
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
        for (target, extra) in [("native", &[][..]), ("wasm", &["--target", "wasm"][..])] {
            let root = tempfile::tempdir().expect("tempdir");
            let dir = stdlib_named_package(root.path(), &modules, as_dependency);
            let args: Vec<&str> = ["run", "src/main.almd"].into_iter().chain(extra.iter().copied()).collect();
            assert_prints_every_cell(&format!("{shape} {target}"), almide_in(&dir, &args), &modules);
        }
    }
}

/// The first public top-level fn a bundled module's own source declares.
fn first_public_fn(module: &str) -> Option<String> {
    let src = almide_lang::stdlib_info::bundled_source(module)?;
    src.lines().find_map(|l| {
        let rest = l.strip_prefix("fn ").or_else(|| l.strip_prefix("effect fn "))?;
        let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        (!name.is_empty() && !name.starts_with('_') && rest[name.len()..].starts_with(['(', '['])).then_some(name)
    })
}

/// #2867: a package module keyed like a stdlib module may declare a fn the
/// stdlib module also declares. In-package the module shared the stdlib's key,
/// so the native leg bound the package's `regex.is_match(41)` to the stdlib's
/// signature and body (E0614 / E0061) while wasm printed `42`. Every bundled
/// module with a public fn is a cell, in one package, native and wasm.
#[test]
fn a_package_module_named_after_a_stdlib_module_may_reuse_its_fn_names() {
    let cells: Vec<(String, String)> =
        stdlib_module_names().into_iter().filter_map(|m| first_public_fn(&m).map(|f| (m, f))).collect();
    assert!(cells.len() >= 30, "only {} modules with a public fn: {cells:?}", cells.len());
    assert!(cells.iter().any(|(m, _)| m == "regex") && cells.iter().any(|(m, _)| m == "int8"), "the reported cells must run: {cells:?}");
    let root = tempfile::tempdir().expect("tempdir");
    let pkg = root.path();
    write(&pkg.join("almide.toml"), PKG_TOML);
    for (m, f) in &cells {
        write(&pkg.join(format!("src/{m}.almd")), &format!("fn {f}(n: Int) -> Int = n + 1\n"));
    }
    let imports: String = cells.iter().map(|(m, _)| format!("import self.{m}\n")).collect();
    let body: String = cells
        .iter()
        .map(|(m, f)| format!("  println(if {m}.{f}(41) == 42 then \"ok-{m}\" else \"lost-{m}\")\n"))
        .collect();
    write(&pkg.join("src/main.almd"), &format!("{imports}\neffect fn main() -> Unit = {{\n{body}}}\n"));
    let modules: Vec<String> = cells.iter().map(|(m, _)| m.clone()).collect();
    for (target, extra) in [("native", &[][..]), ("wasm", &["--target", "wasm"][..])] {
        let args: Vec<&str> = ["run", "src/main.almd"].into_iter().chain(extra.iter().copied()).collect();
        assert_prints_every_cell(&format!("in-package {target}"), almide_in(pkg, &args), &modules);
    }
}

/// #2865 (b): a package module's record named like a builtin (`type Float`)
/// reprs by its declared name on the wasm leg.
#[test]
fn the_wasm_leg_reprs_a_module_record_named_like_a_builtin() {
    let root = tempfile::tempdir().expect("tempdir");
    let pkg = root.path();
    write(&pkg.join("almide.toml"), PKG_TOML);
    for m in ["float", "int", "bytes"] {
        write(&pkg.join(format!("src/{m}.almd")), &cell_module(&pascal(m)));
    }
    write(
        &pkg.join("src/main.almd"),
        "import self.float\nimport self.int\nimport self.bytes\n\neffect fn main() -> Unit = {\n  println(\"${float.build(true)} ${int.build(true)} ${bytes.build(false)}\")\n}\n",
    );
    let out = Command::new(almide())
        .current_dir(pkg)
        .args(["run", "src/main.almd", "--target", "wasm"])
        .output()
        .expect("run almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(
        out.status.success() && text.contains("Float { ok: true } Int { ok: true } Bytes { ok: false }"),
        "wasm leg:\n{text}"
    );
}

/// #2870: every declarable bare builtin head is a cell, enumerated from the
/// resolver's `BUILTIN_TYPE_HEADS`: the entry program declares a record under
/// that name and prints it, and the wasm leg must print what native prints.
#[test]
fn every_leg_reprs_an_entry_record_named_like_every_builtin() {
    use almide_frontend::canonicalize::resolve::{BUILTIN_TYPE_HEADS, TypeSpelling};
    let mut names: Vec<&str> = BUILTIN_TYPE_HEADS
        .iter()
        .filter(|h| h.arity.sample() == TypeSpelling::Bare)
        .map(|h| h.name)
        .filter(|n| n.chars().all(|c| c.is_ascii_alphanumeric()) && n.starts_with(|c: char| c.is_ascii_uppercase()))
        .collect();
    names.dedup();
    assert!(names.len() >= 18 && names.contains(&"String") && names.contains(&"Int"), "the builtin table looks truncated: {names:?}");
    let mut failures = Vec::new();
    for name in &names {
        // The payload is a builtin the cell does not declare: `type Bool`
        // carries an Int, every other cell a Bool.
        let (field, ty, value) = if *name == "Bool" { ("n", "Int", "7") } else { ("ok", "Bool", "true") };
        // `main`'s `-> Unit` names the cell's record when the cell declares
        // `Unit` (§4.5, #2880), so that main returns the record; an effect
        // main may declare any Ok type and the payload is discarded.
        let tail = if *name == "Unit" { "  f\n" } else { "" };
        let program = format!(
            "type {name} = {{ {field}: {ty} }}\n\neffect fn main() -> Unit = {{\n  let f = {name} {{ {field}: {value} }}\n  println(\"${{f}}\")\n{tail}}}\n"
        );
        let want = format!("{name} {{ {field}: {value} }}\n");
        let root = tempfile::tempdir().expect("tempdir");
        write(&root.path().join("t.almd"), &program);
        let legs: [(&str, &[&str]); 2] = [("structural", &["run", "t.almd", "--target", "wasm"]), ("native", &["run", "t.almd"])];
        for (leg, args) in legs {
            let mut cmd = Command::new(almide());
            cmd.current_dir(root.path()).args(args);
            let out = cmd.output().expect("run almide");
            let stdout = String::from_utf8_lossy(&out.stdout);
            if !out.status.success() || stdout != want {
                failures.push(format!(
                    "`type {name}` on the {leg} leg: {stdout:?}\n{}",
                    String::from_utf8_lossy(&out.stderr).lines().filter(|l| !l.starts_with("[almide]")).take(4).collect::<Vec<_>>().join("\n")
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{} cell(s) failed:\n{}", failures.len(), failures.join("\n\n"));
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
        "import self.option\n\neffect fn main() -> Unit = {\n  let o = option.build(true)\n  println(if o.ok then \"ok\" else \"lost\")\n  println(\"${o}\")\n}\n",
    );
    for (target, args) in [("native", &["run", "src/main.almd"][..]), ("wasm", &["run", "src/main.almd", "--target", "wasm"][..])] {
        let (ok, out) = almide_in(pkg, args);
        assert!(ok && out.contains("ok\nOption { ok: true }"), "{target}:\n{out}");
    }
}

/// Every reserved name, declared by the ENTRY program, in one program that
/// also uses the builtin `Option` / `Result` the runtime is written in.
#[test]
fn the_entry_program_may_declare_every_rust_reserved_type_name() {
    // `Some` / `None` / `Ok` / `Err` are keyword tokens, not type names: the
    // parser refuses them as a declaration, which is asserted, not skipped.
    let keywords = ["Some", "None", "Ok", "Err"];
    let names: Vec<String> = almide_codegen::pass_ir_link_flatten::RUST_RESERVED_TYPE_NAMES
        .iter()
        .filter(|n| !keywords.contains(n))
        .map(|n| n.to_string())
        .collect();
    for kw in keywords {
        let root = tempfile::tempdir().expect("tempdir");
        write(&root.path().join("t.almd"), &format!("{}\neffect fn main() -> Unit = ()\n", cell_module(kw)));
        let (ok, out) = almide_in(root.path(), &["check", "t.almd"]);
        assert!(!ok && out.contains("Expected type name"), "`type {kw}` should be a parse error:\n{out}");
    }
    for name in &names {
        let root = tempfile::tempdir().expect("tempdir");
        let main = format!("{}\neffect fn main() -> Unit = println({})\n", cell_module(name), cell_line("build(true)", name));
        write(&root.path().join("t.almd"), &main);
        let (ok, out) = almide_in(root.path(), &["check", "t.almd"]);
        assert!(ok, "the entry program declaring `type {name}` does not check:\n{out}");
    }

    let decls: String = names
        .iter()
        .map(|n| format!("type {n} = {{ ok_{}: Bool }}\n", n.to_ascii_lowercase()))
        .collect();
    let body: String = names
        .iter()
        .map(|n| format!("  println(\"${{{n} {{ ok_{}: true }}}}\")\n", n.to_ascii_lowercase()))
        .collect();
    let program = format!(
        "{decls}\neffect fn main() -> Unit = {{\n{body}  let first: Option[Int] = list.first([7])\n  println(int.to_string((first ?? 0) + (int.parse(\"8\") ?? 0)))\n}}\n"
    );
    let root = tempfile::tempdir().expect("tempdir");
    write(&root.path().join("t.almd"), &program);
    for (target, args) in [("native", &["run", "t.almd"][..]), ("wasm", &["run", "t.almd", "--target", "wasm"][..])] {
        let (ok, out) = almide_in(root.path(), args);
        assert!(ok, "{target}: the build failed:\n{out}");
        let missing: Vec<&String> = names
            .iter()
            .filter(|n| !out.contains(&format!("{n} {{ ok_{}: true }}", n.to_ascii_lowercase())))
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
