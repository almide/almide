//! Module-level vars of an instantiated GENERIC record on the structural wasm
//! leg (#2877): `var n: bx.Box[Int]` in a module other than the entry, spelled
//! through that module's own import alias, and the same shape inside a path
//! DEPENDENCY (`var counter: cl.Cell[Int]`, the reactive-cell state model).
//!
//! The alias spelling used to resolve to the bare `Box` when the module was
//! registered — its alias was only in scope while the module was checked — so
//! the global's type named no declaration and the structural leg declined
//! with `bind-ty:Named`. Forced-structural success proves the structural leg
//! lowered it (a wall is a hard error), and the output matches native.

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

const FILES: &[(&str, &str)] = &[
    ("cells/almide.toml", "[package]\nname = \"cells\"\nversion = \"0.1.0\"\n"),
    ("cells/src/cell.almd", "type Cell[T] = { _val: T, _id: Int }\n\nfn cell[T](v: T) -> Cell[T] = { _val: v, _id: 0 }\n\nfn get[T](c: Cell[T]) -> T = c._val\n"),
    ("cells/src/mod.almd", "import self.cell as cl\n\nvar counter: cl.Cell[Int] = cl.cell(3)\n\nfn counter_value() -> Int = cl.get(counter)\n"),
    ("app/almide.toml", "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ncells = { path = \"../cells\" }\n"),
    ("app/src/boxes.almd", "type Box[T] = { v: T, id: Int }\n\nfn box[T](x: T) -> Box[T] = { v: x, id: 0 }\n"),
    ("app/src/store.almd", "import self.boxes as bx\nimport cells.cell as cl\n\nvar n: bx.Box[Int] = bx.box(7)\n\nvar names: bx.Box[List[String]] = bx.box([\"a\", \"b\"])\n\nvar todos: cl.Cell[List[String]] = cl.cell([\"write\", \"test\"])\n\nfn count() -> Int = n.v\n\nfn names_joined() -> String = list.join(names.v, \",\")\n\nfn todo_count() -> Int = list.len(cl.get(todos))\n"),
    ("app/src/main.almd", "import self.store as st\nimport cells\n\neffect fn main() -> Unit = {\n  println(int.to_string(st.count()))\n  println(st.names_joined())\n  println(int.to_string(st.todo_count()))\n  println(int.to_string(cells.counter_value()))\n}\n"),
];

#[test]
fn generic_record_module_vars_lower_on_the_structural_leg() {
    let root = tempfile::tempdir().unwrap();
    for (rel, text) in FILES {
        let path = root.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let app = root.path().join("app");
    let native = Command::new(almide_bin()).args(["run", "src/main.almd"]).current_dir(&app).output().unwrap();
    assert!(native.status.success(), "native run failed:\n{}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout), "7\na,b\n2\n3\n");
    let wasm = Command::new(almide_bin())
        .args(["run", "src/main.almd", "--target", "wasm"])
        .env("ALMIDE_WASM_STRUCTURAL", "1")
        .current_dir(&app)
        .output()
        .unwrap();
    assert!(
        wasm.status.success(),
        "the structural leg walled on a generic-record module var:\n{}",
        String::from_utf8_lossy(&wasm.stderr)
    );
    assert_eq!(wasm.stdout, native.stdout, "wasm/native divergence on the forced-structural leg");
}
