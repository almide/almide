//! #2067/#2069/#2070: read projections borrow; owned rebuilds transfer fields.
use std::process::Command;

const SOURCE: &str = r#"type Hit = { kids: List[Int], pos: Int }
type Out = { hit: Option[Hit], st: Int }
type Node = { kind: String, field: String }
type Tree = { kind: String, kids: List[Int], field: String }
fn take_pos(o: Out) -> Int = match o.hit { some(h) => h.pos, none => 0 }
fn bound_pos(o: Out) -> Int = {
  let hit = o.hit
  match hit { some(h) => h.pos, none => 0 }
}
fn get_pos(xs: List[Hit], i: Int) -> Int = match list.get(xs, i) { some(h) => h.pos, none => 0 }
fn index_pos(xs: List[Hit], i: Int) -> Int = xs[i].pos
fn relabel(ns: List[Node], name: String) -> List[Node] = list.map(ns, (n) => Node { kind: n.kind, field: name })
fn with_field(n: Node, name: String) -> Node = Node { kind: n.kind, field: name }
fn kind_level(n: Node) -> Int = match n.kind { "k" => 1, _ => 0 }
fn tree_field(n: Tree, name: String) -> Tree = Tree { kind: n.kind, kids: n.kids, field: name }
fn reused_tree(n: Tree) -> (Tree, Tree) = {
  let changed = Tree { kind: n.kind, kids: n.kids, field: "changed" }
  (changed, n)
}
fn repeated_field(n: Node) -> Node = Node { kind: n.kind, field: n.kind }
fn captured_field(n: Node) -> List[Node] = list.map([1, 2], (_) => Node { kind: n.kind, field: "captured" })
fn mutate_match(mut o: Out) -> Int = match o.hit {
  some(h) => { o.hit = none; h.pos },
  none => 0,
}
fn take_owned() -> List[Int] = {
  let o = Out { hit: some(Hit { kids: [1, 2], pos: 3 }), st: 1 }
  match o.hit { some(h) => h.kids, none => [] }
}
effect fn main() -> Unit = {
  let h = Hit { kids: [1, 2], pos: 3 }
  let o = Out { hit: some(h), st: 1 }
  assert_eq(take_pos(o), 3)
  assert_eq(bound_pos(o), 3)
  assert_eq(o.st, 1)
  let xs = [h]
  assert_eq(get_pos(xs, 0), 3)
  assert_eq(get_pos(xs, -1), 0)
  assert_eq(get_pos(xs, 3), 0)
  assert_eq(index_pos(xs, 0), 3)
  assert_eq(xs[0].kids, [1, 2])
  let n = Node { kind: "k", field: "f" }
  assert_eq(with_field(n, "renamed").kind, "k")
  assert_eq(n.field, "f")
  assert_eq(kind_level(n), 1)
  let ns = relabel([n, n], "name")
  assert_eq(ns[0].field, "name")
  assert_eq(ns[1].kind, "k")
  let tree = Tree { kind: "tree", kids: [1, 2, 3], field: "old" }
  assert_eq(tree_field(tree, "new").kids, [1, 2, 3])
  let (changed_tree, kept_tree) = reused_tree(tree)
  assert_eq(changed_tree.field, "changed")
  assert_eq(kept_tree.field, "old")
  assert_eq(repeated_field(n).field, "k")
  assert_eq(captured_field(n)[1].kind, "k")
  var changed = o
  assert_eq(mutate_match(changed), 3)
  assert_eq(changed.hit, none)
  assert_eq(take_owned(), [1, 2])
  println("projections ok")
}
"#;

#[test]
fn native_projections_preserve_values_without_container_copies() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("main.almd");
    std::fs::write(&source, SOURCE).unwrap();
    let bin = std::env::var("ALMIDE_BIN").unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")));
    let out = Command::new(&bin).arg("emit").arg(&source).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8(out.stdout).unwrap();
    let body = |name| rust.split(&format!("pub fn {name}(")).nth(1).unwrap().split("\n}").next().unwrap();
    assert!(!body("take_pos").contains(".clone()"), "{}", body("take_pos"));
    assert!(!body("bound_pos").contains(".clone()"), "{}", body("bound_pos"));
    assert!(body("get_pos").contains("almide_list_get_ref!"), "{}", body("get_pos"));
    assert!(body("index_pos").contains("almide_index_ref!"), "{}", body("index_pos"));
    for name in ["relabel", "with_field", "tree_field", "take_owned"] {
        assert!(!body(name).contains(".kind.clone()") && !body(name).contains(".kids.clone()"), "{name}: {}", body(name));
    }
    for target in ["rust", "wasm"] {
        let out = Command::new(&bin).arg("run").arg(&source).args(["--target", target]).output().unwrap();
        assert!(out.status.success(), "{target}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "projections ok");
    }
}

#[test]
fn borrowed_index_keeps_bounds_errors() {
    let bin = std::env::var("ALMIDE_BIN").unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")));
    for index in [-1, 1] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("main.almd");
        std::fs::write(&source, format!(
            "type Hit = {{ pos: Int }}\nfn read(xs: List[Hit], i: Int) -> Int = xs[i].pos\neffect fn main() -> Unit = println(int.to_string(read([Hit {{ pos: 3 }}], {index})))\n"
        )).unwrap();
        for target in ["rust", "wasm"] {
            let out = Command::new(&bin).arg("run").arg(&source).args(["--target", target]).output().unwrap();
            assert!(!out.status.success(), "{target}: index {index} must fail");
            assert!(String::from_utf8_lossy(&out.stderr).contains("index out of bounds"), "{target}: {}", String::from_utf8_lossy(&out.stderr));
        }
    }
}

#[test]
fn global_index_snapshot_survives_argument_and_field_reads() {
    let bin = std::env::var("ALMIDE_BIN").unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")));
    let source = format!("{}/spec/wasm_cross/mg_rebuild_two_phase.almd", env!("CARGO_MANIFEST_DIR"));
    for target in ["rust", "wasm"] {
        let out = Command::new(&bin).arg("run").arg(&source).args(["--target", target]).output().unwrap();
        assert!(out.status.success(), "{target}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "110.0 4.0");
    }
}
