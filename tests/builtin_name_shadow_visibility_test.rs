//! #2839: a module that declares its own type (or protocol) named like a
//! builtin — `type Map` in a repo-map module — must not take the builtin
//! spelling away from the files that do not import it.
//!
//! The #2715 visibility check listed every user module declaring a bare name
//! and reported E029 when none of them was imported, without first asking
//! whether the spelling resolves to a builtin (`Map[K, V]`, `List[T]`, ...)
//! before any module is consulted. It hid inside a single package only by
//! accident: a module there named `map` has the key `map`, which the check
//! skipped as a stdlib module name. Loaded as a dependency the same module is
//! `dep.map`, and every file of the dependency spelling the builtin `Map`
//! failed (gramide-cli). So the cells run the module both as a dependency and
//! under a non-stdlib name inside one package.

use std::path::Path;
use std::process::Command;

/// `ALMIDE_BIN` lets the cells run against another build (an A/B against the
/// release that regressed); the default is this workspace's binary.
fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, content).expect("write");
}

fn run(dir: &Path) -> (bool, String) {
    let out = Command::new(almide()).current_dir(dir).args(["run", "src/main.almd"]).output().expect("run almide");
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

/// `(builtin type, module declaring a same-named type, a use of the builtin)`.
const CELLS: &[(&str, &str, &str)] = &[
    ("Map", "map", "fn use_it(x: Map[String, Int]) -> Int = map.len(x)\nfn go() -> Int = use_it([\"a\": 1])\n"),
    ("List", "list", "fn use_it(x: List[Int]) -> Int = list.len(x)\nfn go() -> Int = use_it([1])\n"),
    ("Option", "option", "fn use_it(x: Option[Int]) -> Int = x ?? 0\nfn go() -> Int = use_it(some(1))\n"),
    ("Set", "set", "fn use_it(x: Set[Int]) -> Int = set.len(x)\nfn go() -> Int = use_it(set.from_list([1]))\n"),
    ("Result", "result", "fn use_it(x: Result[Int, String]) -> Int = x ?? 0\nfn go() -> Int = use_it(ok(1))\n"),
];

fn shadow_module(ty: &str) -> String {
    format!("type {ty} = {{ text: String }}\n\nfn build(t: String) -> {ty} = {ty} {{ text: t }}\n")
}

/// The module declaring `type <T>` is a DEPENDENCY's module named after the
/// builtin — the gramide-cli shape.
#[test]
fn a_dependency_module_named_after_a_builtin_type_does_not_hide_the_builtin() {
    for (ty, module, user) in CELLS {
        let root = tempfile::tempdir().expect("tempdir");
        let dep = root.path().join("dep");
        let app = root.path().join("app");
        write(&dep.join("almide.toml"), "[package]\nname = \"dep\"\nversion = \"0.1.0\"\n");
        write(&dep.join("src").join(format!("{module}.almd")), &shadow_module(ty));
        write(&dep.join("src").join("user.almd"), user);
        write(
            &dep.join("src").join("mod.almd"),
            &format!("import self.{module}\nimport self.user\n\nfn run() -> String = \"${{{module}.build(\"ok\").text}} ${{user.go()}}\"\n"),
        );
        write(&app.join("almide.toml"), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ndep = { path = \"../dep\" }\n");
        write(&app.join("src").join("main.almd"), "import dep\n\neffect fn main() -> Unit = {\n  println(dep.run())\n}\n");
        let (ok, out) = run(&app);
        assert!(ok && out.contains("ok 1"), "builtin `{ty}` in a dependency with module `{module}`:\n{out}");
    }
}

/// The same shape inside one package, under a module name the stdlib does not
/// own (a stdlib-named module used to be skipped by accident).
#[test]
fn a_package_module_declaring_a_builtin_named_type_does_not_hide_the_builtin() {
    for (ty, _, user) in CELLS {
        let root = tempfile::tempdir().expect("tempdir");
        let pkg = root.path();
        write(&pkg.join("almide.toml"), "[package]\nname = \"one\"\nversion = \"0.1.0\"\n");
        write(&pkg.join("src").join("repomap.almd"), &shadow_module(ty));
        write(&pkg.join("src").join("user.almd"), user);
        write(
            &pkg.join("src").join("main.almd"),
            "import self.repomap\nimport self.user\n\neffect fn main() -> Unit = {\n  println(\"${repomap.build(\"ok\").text} ${user.go()}\")\n}\n",
        );
        let (ok, out) = run(pkg);
        assert!(ok && out.contains("ok 1"), "builtin `{ty}` beside a module declaring `type {ty}`:\n{out}");
    }
}

/// A dependency `dep` whose module `proto` declares `protocol <name>` with a
/// `ping` method, and whose module `user` conforms a type to the built-in
/// `<builtin>` and compares two values.
fn protocol_project(root: &Path, name: &str, builtin: &str) -> std::path::PathBuf {
    let dep = root.join("dep");
    let app = root.join("app");
    write(&dep.join("almide.toml"), "[package]\nname = \"dep\"\nversion = \"0.1.0\"\n");
    write(
        &dep.join("src").join("proto.almd"),
        &format!("protocol {name} {{\n  fn ping(self) -> Int\n}}\n\ntype Q: {name} = {{ v: Int }}\n\nfn Q.ping(self) -> Int = self.v\n\nfn get() -> Int = Q {{ v: 1 }}.ping()\n"),
    );
    write(&dep.join("src").join("user.almd"), &format!("type C: {builtin} = Red | Green\n\nfn same(a: C, b: C) -> Bool = a == b\n"));
    write(&dep.join("src").join("mod.almd"), "import self.proto\nimport self.user\n\nfn run() -> String = int.to_string(proto.get())\n");
    write(&app.join("almide.toml"), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ndep = { path = \"../dep\" }\n");
    write(&app.join("src").join("main.almd"), "import dep\n\neffect fn main() -> Unit = {\n  println(dep.run())\n}\n");
    app
}

/// Built-in protocol names are reserved in every module (#4190). Protocols
/// are keyed by bare name, so a dependency module's `protocol Eq` (or
/// `Integer`) replaced the built-in's requirements for every module: another
/// module's `type C: Eq` was then checked against the redeclaration's
/// `ping`. The declaration is refused by name, in a dependency module as in
/// the entry file (tests/diagnostics/builtin-protocol-redeclared).
#[test]
fn a_dependency_module_redeclaring_a_builtin_protocol_is_refused() {
    for proto in ["Eq", "Repr", "Ord", "Hash", "Codec", "Numeric", "Integer", "Text"] {
        let root = tempfile::tempdir().expect("tempdir");
        let app = protocol_project(root.path(), proto, "Eq");
        let (ok, out) = run(&app);
        assert!(
            !ok && out.contains(&format!("protocol '{proto}' is built in and cannot be redeclared")),
            "built-in protocol `{proto}` redeclared in a dependency module must be refused:\n{out}"
        );
    }
}

/// The control: the same dependency with its protocol under its own name
/// runs, and the sibling module's built-in conformance is untouched.
#[test]
fn a_dependency_module_protocol_under_its_own_name_leaves_the_builtin_conformance() {
    for builtin in ["Eq", "Repr", "Ord", "Hash", "Codec"] {
        let root = tempfile::tempdir().expect("tempdir");
        let app = protocol_project(root.path(), "Ping", builtin);
        let (ok, out) = run(&app);
        assert!(ok && out.trim_end().ends_with('1'), "built-in protocol `{builtin}` beside a dependency's `protocol Ping`:\n{out}");
    }
}

/// The control: a name only an unimported module declares, and no builtin
/// owns, is still E029 — the exemption is the builtin spelling, nothing wider.
#[test]
fn a_non_builtin_name_only_an_unimported_module_declares_is_still_e029() {
    let root = tempfile::tempdir().expect("tempdir");
    let pkg = root.path();
    write(&pkg.join("almide.toml"), "[package]\nname = \"one\"\nversion = \"0.1.0\"\n");
    write(&pkg.join("src").join("repomap.almd"), &shadow_module("Grid"));
    write(&pkg.join("src").join("user.almd"), "fn use_it(x: Grid) -> Int = 0\n");
    write(
        &pkg.join("src").join("main.almd"),
        "import self.repomap\nimport self.user\n\neffect fn main() -> Unit = {\n  println(repomap.build(\"ok\").text)\n}\n",
    );
    let (ok, out) = run(pkg);
    assert!(!ok && out.contains("E029") && out.contains("'Grid'"), "expected E029 for Grid:\n{out}");
}
