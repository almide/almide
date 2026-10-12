//! #3643: `map.merge` is declared `@reuse(a)`, so a left operand at its last
//! use — the rhs of `m = map.merge(m, …)`, a fold's accumulator, a self tail
//! call's argument — is extended in place natively through the owned twin
//! `almide_rt_map_merge_owned` instead of cloned whole per step (the stdlib
//! inspection measured the native rebind shapes at about 480 instructions per
//! entry of the left map, per call).
//!
//! VALUES: the shapes that must still copy — a left map shared with an alias,
//! a holder or a closure, a right operand that is the left map or an alias of
//! it — are `spec/wasm_cross/map_merge_in_place.almd`; its outputs below are
//! the pre-change compiler's, both legs.
use std::process::Command;

const SOURCE: &str = r#"fn small(i: Int) -> Map[Int, Int] = map.from_list([(i % 7, 0 - i), (1000000 + i, i)])

fn go(m: Map[Int, Int], i: Int) -> Map[Int, Int] = if i >= 300 then m else go(map.merge(m, small(i)), i + 1)

effect fn main() -> Unit = {
  var g: Map[Int, Int] = list.range(0, 2000) |> list.fold(map.new(), (acc, k) => map.set(acc, k, k))
  var i = 0
  while i < 300 {
    g = map.merge(g, small(i))
    i = i + 1
  }
  let t = go(map.from_list([(5, 5)]), 0)
  let f = list.range(0, 300) |> list.fold(map.from_list([(1, 1)]), (acc, k) => map.merge(acc, small(k)))
  println("${map.len(g)} ${map.get(g, 3)} ${map.len(t)} ${map.get(t, 5)} ${map.len(f)} ${map.get(f, 1000299)}")
}
"#;

const EXPECTED: &str = "2300 some(-297) 307 some(-299) 307 some(299)\n";

fn write(name: &str, src: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join(name);
    std::fs::write(&file, src).expect("source");
    (dir, file)
}

fn run(file: &std::path::Path, wasm: bool) -> String {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let out = c.output().expect("almide run");
    assert!(out.status.success(), "wasm={wasm}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn map_merge_rebinds_keep_value_semantics_on_both_legs() {
    let (_dir, file) = write("map_merge.almd", SOURCE);
    assert_eq!(run(&file, false), EXPECTED, "native");
    assert_eq!(run(&file, true), EXPECTED, "wasm");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/wasm_cross/map_merge_in_place.almd");
    assert_eq!(run(&fixture, false), run(&fixture, true), "the shared-operand fixture diverges");
}

#[test]
fn native_map_merge_rebinds_call_the_owned_twin() {
    let (_dir, file) = write("map_merge.almd", SOURCE);
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg(&file).args(["--target", "rust"]).output().expect("emit");
    let rust = String::from_utf8_lossy(&out.stdout);
    let twins = rust.matches("almide_rt_map_merge_owned(").count();
    assert!(
        twins >= 3,
        "the loop rebind, the tail call and the fold should each move their left map into \
         `almide_rt_map_merge_owned` (found {twins}):\n{}",
        rust.lines().filter(|l| l.contains("almide_rt_map_merge")).collect::<Vec<_>>().join("\n")
    );
}
