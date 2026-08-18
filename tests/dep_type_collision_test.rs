//! A bare type name declared in BOTH a package and its dependency (#1501).
//!
//! Each package compiles clean alone: inside the dependency, the checker's
//! inferred expression types carry the BARE base name (`Node`) for the
//! package's own `deplib.syntax.Node`. Standalone that is unambiguous and
//! the codegen-entry repair completes it. The moment a DEPENDENT that
//! declares its own `type Node` links the two, the global view has two
//! qualified owners, the repair declined, and the #433 gate refused the
//! build — reporting the dependency's own internal references as compiler
//! bugs. The repair is now scope-aware: a bare reference inside module
//! `deplib.engine` can only mean a declaration its own package could name
//! bare (`deplib.*`), never the dependent's `parse.Node` — the checker's
//! visibility rule replayed at repair time.

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

fn tools_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, content).expect("write");
}

fn scratch() -> std::path::PathBuf {
    let root = std::env::temp_dir().join("almide-issue1501");
    let _ = std::fs::remove_dir_all(&root);
    write(
        &root.join("dep").join("almide.toml"),
        "[package]\nname = \"deplib\"\nversion = \"0.1.0\"\n",
    );
    write(
        &root.join("dep").join("src").join("syntax.almd"),
        "type Node =\n  | Leaf(Int)\n  | Pair(Int, Int)\n\nfn make(n: Int) -> Node = Leaf(n)\n\nfn weight(n: Node) -> Int = match n {\n  Leaf(k) => k,\n  Pair(a, b) => a + b,\n}\n",
    );
    // The cross-module type annotations + match arms are what leave BARE
    // `Node` in the dep's inferred expression types.
    write(
        &root.join("dep").join("src").join("engine.almd"),
        "import self.syntax\n\nfn total(asts: List[syntax.Node]) -> Int =\n  list.fold(asts, 0, (acc, ast) => acc + syntax.weight(ast))\n",
    );
    write(
        &root.join("dep").join("src").join("mod.almd"),
        "import self.syntax\nimport self.engine\n\nfn compile(ns: List[Int]) -> Int = {\n  let asts = list.map(ns, (n) => syntax.make(n))\n  engine.total(asts)\n}\n",
    );
    write(
        &root.join("app").join("almide.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ndeplib = { path = \"../dep\" }\n",
    );
    write(
        &root.join("app").join("src").join("parse.almd"),
        "type Node =\n  | Word(String)\n  | Gap\n\nfn label(n: Node) -> String = match n {\n  Word(s) => s,\n  Gap => \"_\",\n}\n",
    );
    write(
        &root.join("app").join("src").join("main.almd"),
        "import deplib\nimport self.parse\n\neffect fn main() -> Unit = {\n  println(int.to_string(deplib.compile([1, 2, 3])))\n  println(parse.label(parse.Word(\"hi\")))\n}\n",
    );
    root.join("app")
}

#[test]
fn colliding_bare_type_names_link_across_the_dependency_boundary() {
    if !tools_available() {
        return;
    }
    let app = scratch();
    let out = Command::new(almide_bin())
        .args(["run", "src/main.almd"])
        .current_dir(&app)
        .output()
        .expect("spawn almide");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("COMPILER BUG"),
        "the #433 gate fired on the dependency's own types:\n{stderr}"
    );
    assert!(out.status.success(), "run failed:\nstdout: {stdout}\nstderr: {stderr}");
    assert_eq!(stdout, "6\nhi\n", "wrong output: {stdout}");
}
