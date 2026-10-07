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

/// #3453: the sibling head reads of `match list.get` — `??` read through a
/// field or bound to a read-only `let`, `list.first`, a list-pattern head and
/// `option.map` — borrow the element and clone only what escapes.
const HEAD_SOURCE: &str = r#"type Big = { id: Int, data: List[Int], name: String }
fn dflt() -> Big = Big { id: -1, data: [], name: "d" }
fn show(b: Big) -> Int = b.id * 2
fn coalesce_field(xs: List[Big]) -> Int = (list.get(xs, 0) ?? dflt()).id
fn coalesce_let(xs: List[Big]) -> Int = {
  let h = list.get(xs, 0) ?? dflt()
  h.id + list.len(h.data)
}
fn first_field(xs: List[Big]) -> Int = (list.first(xs) ?? dflt()).id
fn first_match(xs: List[Big]) -> Int = match list.first(xs) { some(h) => h.id, none => 0 }
fn pattern_head(xs: List[Big]) -> Int = match xs { [h, ..] => h.id + list.len(h.data), _ => 0 }
fn map_head(xs: List[Big]) -> Int = list.get(xs, 0) |> option.map((h) => h.id) ?? 0
fn map_first(xs: List[Big]) -> Int = option.map(list.first(xs), (h) => list.len(h.data)) ?? 0
fn borrowed_len(xs: List[Big]) -> Int = {
  let d = dflt()
  list.len((list.get(xs, 0) ?? d).data) + d.id
}
fn borrowed_arg(xs: List[Big]) -> Int = {
  let d = dflt()
  show(list.first(xs) ?? d)
}
fn param_fallback(xs: List[Big], d: Big) -> Int = (list.get(xs, 0) ?? d).id
fn escape_coalesce(xs: List[Big]) -> Big = list.get(xs, 0) ?? dflt()
fn escape_let(xs: List[Big]) -> List[Big] = {
  let h = list.first(xs) ?? dflt()
  [h]
}
fn escape_pattern(xs: List[Big]) -> Big = match xs { [h, ..] => h, _ => dflt() }
fn escape_field(xs: List[Big]) -> Option[List[Int]] = list.first(xs) |> option.map((h) => h.data)
fn escape_moved(xs: List[Big]) -> List[Big] = {
  let h = list.get(xs, 0) ?? dflt()
  let n = h.id
  list.map(xs, (b) => Big { id: b.id + n, data: b.data, name: b.name })
}
effect fn main() -> Unit = {
  let xs = [Big { id: 7, data: [1, 2, 3], name: "a" }, Big { id: 8, data: [], name: "b" }]
  let e: List[Big] = []
  let d = dflt()
  println("${coalesce_field(xs)} ${coalesce_field(e)} ${coalesce_let(xs)} ${coalesce_let(e)} ${first_field(xs)} ${first_field(e)}")
  println("${first_match(xs)} ${first_match(e)} ${pattern_head(xs)} ${pattern_head(e)} ${map_head(xs)} ${map_head(e)} ${map_first(xs)} ${map_first(e)}")
  println("${borrowed_len(xs)} ${borrowed_len(e)} ${borrowed_arg(xs)} ${borrowed_arg(e)} ${param_fallback(xs, d)} ${param_fallback(e, d)}")
  println("${escape_coalesce(xs).name} ${escape_coalesce(e).name} ${list.len(escape_let(xs))} ${escape_pattern(xs).name} ${escape_pattern(e).name}")
  println("${escape_field(xs)} ${escape_field(e)} ${list.len(escape_moved(xs))} ${xs[0].name} ${list.len(xs[0].data)}")
}
"#;

const HEAD_OUTPUT: &str = "7 -1 10 -1 7 -1\n7 0 10 0 7 0 3 0\n2 -1 14 -2 7 -1\na d 1 a d\nsome([1, 2, 3]) none 2 a 3";

fn emit_fn_bodies(source: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.almd");
    std::fs::write(&path, source).unwrap();
    let out = Command::new(almide_bin()).arg("emit").arg(&path).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

fn fn_body<'a>(rust: &'a str, name: &str) -> &'a str {
    rust.split(&format!("pub fn {name}(")).nth(1).unwrap_or_else(|| panic!("no fn {name}")).split("\n}").next().unwrap()
}

#[test]
fn head_reads_borrow_the_element_and_clone_only_what_escapes() {
    let rust = emit_fn_bodies(HEAD_SOURCE);
    let body = |name| fn_body(&rust, name);
    // Every read-only shape borrows in place and copies nothing.
    for name in ["coalesce_field", "coalesce_let", "first_field", "first_match", "map_head", "map_first", "borrowed_len", "borrowed_arg"] {
        assert!(body(name).contains("almide_list_get_ref!"), "{name} must borrow the head: {}", body(name));
        assert!(!body(name).contains(".clone()"), "{name} must not copy the element: {}", body(name));
        assert!(!body(name).contains("almide_rt_list_get(") && !body(name).contains("almide_rt_list_first("), "{name}: {}", body(name));
        assert!(!body(name).contains("almide_rt_option_map"), "{name}: {}", body(name));
    }
    assert!(body("pattern_head").contains("almide_index_ref!") && !body("pattern_head").contains(".clone()"), "{}", body("pattern_head"));
    // An escaping head is copied exactly once: the runtime read owns it, or
    // the one `.clone()` the pattern binder takes; a projected field that
    // escapes clones that field, never the element.
    // An owned param as the fallback keeps its copy: the signature is
    // settled before the rewrite, and a never-consumed owned param would
    // make every caller pay a clone (the certifier's C4).
    for name in ["escape_coalesce", "escape_let", "escape_moved", "param_fallback"] {
        assert!(!body(name).contains("almide_list_get_ref!"), "{name} escapes, it must own: {}", body(name));
        assert_eq!(body(name).matches(".clone()").count(), usize::from(name == "escape_moved"), "{name}: {}", body(name));
    }
    assert!(body("escape_moved").contains("b.data.clone()"), "{}", body("escape_moved"));
    assert_eq!(body("escape_pattern").matches(".clone()").count(), 1, "{}", body("escape_pattern"));
    assert!(body("escape_pattern").contains("almide_index!(xs, 0i64).clone()"), "{}", body("escape_pattern"));
    assert_eq!(body("escape_field").matches(".clone()").count(), 1, "{}", body("escape_field"));
    assert!(body("escape_field").contains(".data.clone()") && body("escape_field").contains("almide_list_get_ref!"), "{}", body("escape_field"));

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("main.almd");
    std::fs::write(&source, HEAD_SOURCE).unwrap();
    for target in ["rust", "wasm"] {
        let out = Command::new(almide_bin()).arg("run").arg(&source).args(["--target", target]).output().unwrap();
        assert!(out.status.success(), "{target}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), HEAD_OUTPUT, "{target}");
    }
}

/// #3453 perf cell: 20k head reads per shape. With the element borrowed the
/// time does not grow with the element; before, every read copied it, so a
/// 16x larger element took several times longer. One release binary, two
/// element sizes from argv, best of three; the bound is generous.
#[test]
fn head_read_time_does_not_scale_with_element_size() {
    const PERF: &str = r#"import env
type Big = { id: Int, data: List[Int] }
fn reads(xs: List[Big], i: Int) -> Int = {
  let d = Big { id: -1, data: [] }
  let k = i % 100
  let a = list.get((list.get(xs, 0) ?? d).data, k) ?? 0
  let h = list.first(xs) ?? d
  let b = list.get(h.data, k) ?? 0
  let c = match list.first(xs) { some(p) => list.get(p.data, k) ?? 0, none => 0 }
  let e = match xs { [p, ..] => list.get(p.data, k) ?? 0, _ => 0 }
  let f = list.get(xs, 0) |> option.map((p) => list.get(p.data, k) ?? 0) ?? 0
  a + b + c + e + f + (list.first(xs) ?? d).id
}
effect fn main() -> Unit = {
  let n = int.parse(list.last(env.args()) ?? "10")!
  let xs = [Big { id: 3, data: list.repeat(1, n) }, Big { id: 4, data: [] }]
  var acc = 0
  for i in 0..<20000 {
    acc = acc + reads(xs, i)
  }
  println(int.to_string(acc))
}
"#;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("perf.almd");
    std::fs::write(&source, PERF).unwrap();
    let exe = dir.path().join("perf");
    let out = Command::new(almide_bin()).arg("build").arg("--release").arg(&source).arg("-o").arg(&exe).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let time = |n: &str| (0..3).map(|_| {
        let t = std::time::Instant::now();
        let out = Command::new(&exe).arg(n).output().unwrap();
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "160000");
        t.elapsed()
    }).min().unwrap();
    let (small, big) = (time("10000"), time("160000"));
    assert!(big < small * 3 + std::time::Duration::from_millis(100),
        "head reads scale with the element: 10k ints {small:?}, 160k ints {big:?}");
}
