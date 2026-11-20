//! #4787: a Set element or Map key of a generated type (a record, a tuple, a
//! variant) stayed on the native runtime's linear scan — the runtime hashes
//! only key types it can name — so building a set of n distinct records was
//! O(n^2) (40,000 BigInts took 2 s). The native render now gives each such
//! type canonical key bytes and registers it at the start of `main`, so the
//! runtime indexes it like an Int or String key. A type whose `==` is not the
//! derived field-by-field one over exact values (a Float field) keeps the
//! linear scan.
//!
//! Two checks: the render registers exactly the qualifying key types (the old
//! render registered none), and a program that drives every operation past
//! the index threshold prints the same answers natively and on wasm.
use std::process::Command;

const SOURCE: &str = r#"type Point = { x: Int, name: String }
type Shape = | Dot | Box(Int, Int) | Named(String, List[Int])

effect fn main() -> Unit = {
  let pts = list.range(0, 3000) |> list.map((i) => { x: i % 1000, name: "p${i % 1000}" })
  let s = set.from_list(pts)
  println("${set.len(s)} ${set.contains(s, { x: 7, name: "p7" })} ${set.contains(s, { x: 7, name: "p8" })}")
  let s2 = set.remove(set.insert(s, { x: 5000, name: "new" }), { x: 3, name: "p3" })
  println("${set.len(s2)} ${set.contains(s2, { x: 5000, name: "new" })} ${set.contains(s2, { x: 3, name: "p3" })}")
  var counts: Map[Shape, Int] = map.new()
  var i = 0
  while i < 600 {
    let sh = match i % 3 {
      0 => Dot,
      1 => Box(i % 50, 1),
      _ => Named("n${i % 40}", [i % 7]),
    }
    counts = map.set(counts, sh, (map.get(counts, sh) ?? 0) + 1)
    i = i + 1
  }
  println("${map.len(counts)} ${map.get(counts, Dot) ?? 0} ${map.get(counts, Box(1, 1)) ?? 0} ${map.get(counts, Named("n0", [0])) ?? -1}")
}
"#;

/// A Float field: its `==` is IEEE (0.0 == -0.0), so it has no canonical
/// bytes and its set stays on the linear scan, on every route.
const FLOAT_SOURCE: &str = r#"type Reading = { at: Int, value: Float }

effect fn main() -> Unit = {
  let rs = list.range(0, 40) |> list.map((i) => { at: i % 20, value: if i < 20 then 0.0 else -0.0 })
  println("${set.len(set.from_list(rs))}")
}
"#;

/// A program that imports `fs`: the bundled `FileStat` decl reaches its type
/// decls, but the runtime defines that type under its reserved spelling and
/// the standard codegen never emits the decl, so it must not get a key impl.
const TWIN_SOURCE: &str = r#"import fs

type Point = { x: Int, name: String }

effect fn main() -> Unit = {
  let s = set.from_list(list.range(0, 300) |> list.map((i) => { x: i % 100, name: "p" }))
  println("${set.len(s)} ${fs.exists("/no/such/almide/path")}")
}
"#;

const EXPECTED: &str = "1000 true false\n1000 true false\n251 200 4 1\n";

#[test]
fn record_variant_and_tuple_keys_are_registered_and_float_ones_are_not() {
    let rust = almide_mir::pipeline::try_render_rust_source(SOURCE).unwrap_or_else(|e| panic!("the v1 native render walled: {e:?}"));
    let registration = rust
        .split("fn almide_register_key_types() {")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .unwrap_or_else(|| panic!("no key-type registration in the render:\n{rust}"));
    let registered = registration.matches("almide_map_register_key::<").count();
    // Point (the Set element) and Shape (the Map key).
    assert_eq!(registered, 2, "{registration}");
    assert!(rust.contains("    almide_register_key_types();\n"), "main does not register the key types");
    // One impl per generated key type, beside the runtime's own impls.
    let user = rust.split("pub use almide_v1_rt::*;").nth(1).unwrap_or(&rust);
    assert_eq!(user.matches("impl AlmideKeyEncode for ").count(), 2, "{user}");
}

#[test]
fn record_keys_answer_the_same_natively_and_on_wasm() {
    let dir = tempfile::tempdir().expect("scratch");
    for (name, src, expected) in [("record_keys", SOURCE, EXPECTED), ("float_keys", FLOAT_SOURCE, "20\n"), ("twin_keys", TWIN_SOURCE, "100 false\n")] {
        let file = dir.path().join(format!("{name}.almd"));
        std::fs::write(&file, src).expect("source");
        for wasm in [false, true] {
            let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
            c.arg("run").arg(&file);
            if wasm {
                c.args(["--target", "wasm"]);
            }
            let out = c.output().expect("almide run");
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
            assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "{name} wasm={wasm}");
        }
        // The standard codegen (the v0 parity oracle) carries the same keys.
        let bin = dir.path().join(format!("{name}_v0"));
        let out = Command::new(env!("CARGO_BIN_EXE_almide"))
            .env("ALMIDE_NO_VERIFIED_OK", "1")
            .args(["build"]).arg(&file).args(["--no-verified", "-o"]).arg(&bin)
            .output()
            .expect("almide build --no-verified");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let run = Command::new(&bin).output().expect("run the v0 build");
        assert_eq!(String::from_utf8_lossy(&run.stdout), expected, "{name} v0");
    }
}

#[test]
fn the_standard_codegen_registers_the_same_key_types() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("record_keys.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg(&file).args(["--target", "rust"]).output().expect("almide --target rust");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8_lossy(&out.stdout);
    let registration = rust
        .split("fn almide_register_key_types() {")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .unwrap_or_else(|| panic!("no key-type registration in the v0 source"));
    assert_eq!(registration.matches("almide_map_register_key::<").count(), 2, "{registration}");
    assert!(rust.contains("    almide_register_key_types();\n"), "main does not register the key types");
    assert!(!rust.contains("__ALMIDE_KEY_TYPES__"), "the marker survived");
}
