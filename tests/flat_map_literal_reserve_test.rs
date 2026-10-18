//! `list.flat_map(xs, (x) => [a, b])` with scalar elements knows its result
//! length before it starts: `len(xs)` times the literal's length. wasm
//! still grew the accumulator by doubling from empty, so it allocated and
//! copied about twice the result. The stdlib inspection's complexity
//! criterion read the step pattern as a series no class fits (GCE,
//! 2026-10-14: wasm bytes moved rms 22.5%, instructions rms 8.3%). The
//! accumulator is now reserved at the final size.
use std::process::Command;

const SOURCE: &str = r#"effect fn main() -> Unit = {
  let xs = list.range(0, 100000)
  let ys = list.flat_map(xs, (x) => [x, x + 1])
  let zs = list.flat_map(xs, (x) => { let y = x * 2
    [y, y, y] })
  let ws: List[Float] = list.flat_map([1.5, 2.5], (f) => [f, f * 2.0])
  let es: List[Int] = list.flat_map(xs, (x) => [])
  println("${list.len(ys)} ${list.get(ys, 199999)} ${list.len(zs)} ${list.get(zs, 299999)} ${ws} ${list.len(es)}")
}
"#;

const EXPECTED: &str = "200000 some(100000) 300000 some(199998) [1.5, 3, 2.5, 5] 0\n";

fn almide() -> Command {
    Command::new(env!("CARGO_BIN_EXE_almide"))
}

#[test]
fn flat_map_with_a_literal_tail_reserves_its_result() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("flat.almd");
    std::fs::write(&file, SOURCE).expect("source");

    let native = almide().arg("run").arg(&file).output().expect("native run");
    assert!(native.status.success(), "{}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout), EXPECTED, "native");

    let wasm = almide()
        .arg("run")
        .arg(&file)
        .args(["--target", "wasm"])
        .env("ALMIDE_WASM_ALLOC_COUNT", "1")
        .output()
        .expect("wasm run");
    assert!(wasm.status.success(), "{}", String::from_utf8_lossy(&wasm.stderr));
    assert_eq!(String::from_utf8_lossy(&wasm.stdout), EXPECTED, "wasm");
    let counts = String::from_utf8_lossy(&wasm.stderr);
    let field = |k: &str| -> u64 {
        counts
            .split_whitespace()
            .find_map(|w| w.strip_prefix(k)?.parse().ok())
            .unwrap_or_else(|| panic!("no {k} in {counts}"))
    };
    assert_eq!(field("live="), 0, "{counts}");
    // xs, ys and zs hold 0.8 + 1.6 + 2.4 MB; growth by doubling allocated
    // 13.4 MB in all.
    let bytes = field("bytes=");
    assert!(bytes < 6_000_000, "flat_map allocated {bytes} bytes — growing by doubling again? {counts}");
}
