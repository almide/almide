//! #3423: a match in a top-level let binding a recursive variant's boxed
//! payload, READ through the global. Box-deref collected binders from fn
//! bodies only, so `l` kept its `Box<T>` and rustc refused the build (E0308);
//! wasm printed `node`. `spec/wasm_cross/recursive_payload_top_let.almd`
//! carries the shape into the corpus without reading the global from a fn
//! (the MIR brick walls a read of a computed global), so the read lives here.
//! The same program as a module's `pub let` (the module walk of the same pass)
//! is the second case.

use std::path::Path;
use std::process::Command;

const TYPES: &str = "type T =
  | Leaf(String)
  | Node(T, T)

let tree = Node(Node(Leaf(\"a\"), Leaf(\"b\")), Leaf(\"c\"))

pub let left = match tree {
  Node(l, _) => l,
  Leaf(_) => Leaf(\"none\"),
}

pub fn name(t: T) -> String =
  match t {
    Leaf(s) => s,
    Node(_, _) => \"node\",
  }
";

fn run(dir: &Path, target: &str) -> (bool, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(dir)
        .args(["run", "src/main.almd", "--target", target])
        .output()
        .expect("spawn almide run");
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).to_string(),
        String::from_utf8_lossy(&o.stderr).to_string(),
    )
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn project(name: &str, files: &[(&str, String)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join("almide.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n"),
    )
    .unwrap();
    for (path, src) in files {
        std::fs::write(dir.path().join(path), src).unwrap();
    }
    dir
}

fn assert_every_leg(dir: &Path) {
    let (ok, out, err) = run(dir, "rust");
    assert!(ok, "native run failed:\n{err}");
    assert_eq!(out.trim(), "node", "native leg");
    if !wasmtime_available() {
        eprintln!("wasmtime not on PATH — the wasm leg is skipped here (CI installs it)");
        return;
    }
    let (ok, out, err) = run(dir, "wasm");
    assert!(ok, "wasm run failed:\n{err}");
    assert_eq!(out.trim(), "node", "wasm leg");
}

#[cfg_attr(debug_assertions, ignore = "compiles the program on every leg (CI: release-shape job)")]
#[test]
fn an_entry_top_level_let_binds_a_boxed_payload_as_the_variant() {
    let main = format!("{TYPES}\neffect fn main() -> Unit = println(name(left))\n");
    let dir = project("top_let_entry", &[("src/main.almd", main)]);
    assert_every_leg(dir.path());
}

#[cfg_attr(debug_assertions, ignore = "compiles the program on every leg (CI: release-shape job)")]
#[test]
fn a_module_top_level_let_binds_a_boxed_payload_as_the_variant() {
    let main = "import self.tree\n\neffect fn main() -> Unit = println(tree.name(tree.left))\n".to_string();
    let dir = project("top_let_module", &[("src/tree.almd", TYPES.to_string()), ("src/main.almd", main)]);
    assert_every_leg(dir.path());
}
