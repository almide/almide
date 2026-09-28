//! #2839 gate: every name the resolver answers as a builtin stays builtin in
//! a file that does not import a module redeclaring it — on the module path
//! (a dependency's module, checked by module inference under the key
//! `dep.<m>`) and on the entry path (the package's main file).
//!
//! The cells are enumerated from the resolver's OWN tables —
//! `BUILTIN_TYPE_HEADS`, which `resolve_type_expr_in` dispatches through, and
//! `BUILTIN_PROTOCOLS`, which `register_builtin_protocols` registers — never
//! from a hand list. The #2839 regression came from a second copy of the
//! builtin rule; a new builtin added to the table is covered here the moment
//! it exists, and a check that stops asking the table fails here.
//!
//! The controls (a non-builtin name only an unimported module declares) prove
//! each path's scope check actually runs in this harness, so a green builtin
//! cell is not a check that never looked.

use std::path::Path;
use std::process::Command;

use almide_frontend::canonicalize::protocols::BUILTIN_PROTOCOLS;
use almide_frontend::canonicalize::resolve::{BUILTIN_TYPE_HEADS, TypeSpelling};

/// `ALMIDE_BIN` runs the cells against another build (an A/B against a tree
/// without the fix); the default is this workspace's binary.
fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, content).expect("write");
}

fn check(dir: &Path) -> String {
    let out = Command::new(almide()).current_dir(dir).args(["check", "src/main.almd"]).output().expect("run almide");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

/// Only names a module can declare: `!` / `?` are postfix markers, not
/// identifiers, so no module redeclares them.
fn declarable(name: &str) -> bool {
    name.chars().all(|c| c.is_ascii_alphanumeric()) && name.starts_with(|c: char| c.is_ascii_uppercase())
}

/// The source spelling of `name` in `spelling`: `Map[Int, Int]`, `Int`.
fn spell(name: &str, spelling: TypeSpelling) -> String {
    match spelling {
        TypeSpelling::Applied(n) => format!("{name}[{}]", vec!["Int"; n].join(", ")),
        _ => name.to_string(),
    }
}

/// Every declarable builtin type head, spelled at an arity the resolver
/// answers as builtin.
fn builtin_type_spellings() -> Vec<(&'static str, String)> {
    let cells: Vec<_> = BUILTIN_TYPE_HEADS.iter()
        .filter(|h| declarable(h.name))
        .map(|h| (h.name, spell(h.name, h.arity.sample())))
        .collect();
    assert!(cells.len() >= 20, "the builtin type table looks truncated: {cells:?}");
    cells
}

/// A dependency whose module `shadow` declares `type <name>`, and whose
/// module `user` — which does not import `shadow` — spells `use_ty`.
fn check_dependency_type_cell(name: &str, use_ty: &str) -> String {
    let root = tempfile::tempdir().expect("tempdir");
    let dep = root.path().join("dep");
    let app = root.path().join("app");
    write(&dep.join("almide.toml"), "[package]\nname = \"dep\"\nversion = \"0.1.0\"\n");
    write(&dep.join("src/shadow.almd"), &format!("type {name} = {{ text: String }}\n\nfn one() -> Int = 1\n"));
    write(&dep.join("src/user.almd"), &format!("fn use_it(x: {use_ty}) -> Int = 0\n\nfn two() -> Int = 2\n"));
    write(&dep.join("src/mod.almd"), "import self.shadow\nimport self.user\n\nfn run() -> Int = shadow.one() + user.two()\n");
    write(&app.join("almide.toml"), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ndep = { path = \"../dep\" }\n");
    write(&app.join("src/main.almd"), "import dep\n\neffect fn main() -> Unit = {\n  println(int.to_string(dep.run()))\n}\n");
    check(&app)
}

/// A package whose module `shadow` declares `type <name>`, reached only
/// through `other`; the entry file imports `other` and spells `use_ty`.
fn check_entry_type_cell(name: &str, use_ty: &str) -> String {
    let root = tempfile::tempdir().expect("tempdir");
    let pkg = root.path();
    write(&pkg.join("almide.toml"), "[package]\nname = \"one\"\nversion = \"0.1.0\"\n");
    write(&pkg.join("src/shadow.almd"), &format!("type {name} = {{ text: String }}\n\nfn one() -> Int = 1\n"));
    write(&pkg.join("src/other.almd"), "import self.shadow\n\nfn two() -> Int = shadow.one() + 1\n");
    write(
        &pkg.join("src/main.almd"),
        &format!("import self.other\n\nfn use_it(x: {use_ty}) -> Int = 0\n\neffect fn main() -> Unit = {{\n  println(int.to_string(other.two()))\n}}\n"),
    );
    check(pkg)
}

fn failures(cells: impl Iterator<Item = (String, String)>) -> Vec<String> {
    cells.filter(|(_, out)| out.contains("E029")).map(|(cell, out)| format!("{cell}:\n{out}")).collect()
}

#[test]
fn every_builtin_type_head_stays_builtin_beside_an_unimported_dependency_module() {
    let bad = failures(builtin_type_spellings().into_iter()
        .map(|(name, spelled)| (format!("dependency `{spelled}`"), check_dependency_type_cell(name, &spelled))));
    assert!(bad.is_empty(), "a builtin spelling was judged out of scope:\n{}", bad.join("\n"));
}

#[test]
fn every_builtin_type_head_stays_builtin_beside_an_unimported_package_module() {
    let bad = failures(builtin_type_spellings().into_iter()
        .map(|(name, spelled)| (format!("entry `{spelled}`"), check_entry_type_cell(name, &spelled))));
    assert!(bad.is_empty(), "a builtin spelling was judged out of scope:\n{}", bad.join("\n"));
}

/// The control on both paths: the same shape with a name no builtin owns is
/// E029, so the cells above run the check they claim to.
#[test]
fn a_non_builtin_type_only_an_unimported_module_declares_is_e029_on_both_paths() {
    for (path, out) in [("dependency", check_dependency_type_cell("Grid", "Grid")), ("entry", check_entry_type_cell("Grid", "Grid"))] {
        assert!(out.contains("E029") && out.contains("type 'Grid' is not in scope here"), "{path} path: expected E029 for Grid:\n{out}");
    }
}

fn check_dependency_protocol_cell(proto: &str) -> String {
    let root = tempfile::tempdir().expect("tempdir");
    let dep = root.path().join("dep");
    let app = root.path().join("app");
    write(&dep.join("almide.toml"), "[package]\nname = \"dep\"\nversion = \"0.1.0\"\n");
    write(
        &dep.join("src/proto.almd"),
        &format!("protocol {proto} {{\n  fn ping(self) -> Int\n}}\n\ntype Q: {proto} = {{ v: Int }}\n\nfn Q.ping(self) -> Int = self.v\n\nfn get() -> Int = Q {{ v: 1 }}.ping()\n"),
    );
    write(&dep.join("src/user.almd"), &format!("fn pick[T: {proto}](a: T) -> T = a\n\nfn two() -> Int = 2\n"));
    write(&dep.join("src/mod.almd"), "import self.proto\nimport self.user\n\nfn run() -> Int = proto.get() + user.two()\n");
    write(&app.join("almide.toml"), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ndep = { path = \"../dep\" }\n");
    write(&app.join("src/main.almd"), "import dep\n\neffect fn main() -> Unit = {\n  println(int.to_string(dep.run()))\n}\n");
    check(&app)
}

#[test]
fn every_builtin_protocol_stays_builtin_beside_an_unimported_dependency_module() {
    assert!(BUILTIN_PROTOCOLS.len() >= 8, "the builtin protocol table looks truncated");
    let bad = failures(BUILTIN_PROTOCOLS.iter()
        .map(|p| (format!("protocol `{}`", p.name), check_dependency_protocol_cell(p.name))));
    assert!(bad.is_empty(), "a builtin protocol was judged out of scope:\n{}", bad.join("\n"));
}

#[test]
fn a_non_builtin_protocol_only_an_unimported_module_declares_is_e029() {
    let out = check_dependency_protocol_cell("Pingable");
    assert!(out.contains("E029") && out.contains("protocol 'Pingable' is not in scope here"), "expected E029 for Pingable:\n{out}");
}
