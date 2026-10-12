//! A self tail call that rebinds a map through `map.set` (`go(map.set(m, k,
//! v), i - 1)`) writes the dying map in place on wasm, as the loop and fold
//! shapes already did. Before the tail-call reuse of #3533 each step copied
//! the whole map: 20,000 entries and 1,000 / 4,000 steps requested 322 MB /
//! 1.28 GB on develop 1fec5931f; with it, both request the same 2.5 MB.
//! Native moved the map into `almide_rt_map_set` all along. Found by the
//! stdlib inspection (`map.set` tail shape: n per call on wasm only).
use std::process::Command;

fn program(steps: u32) -> String {
    format!(
        r#"fn go(m: Map[String, Int], i: Int) -> Map[String, Int] =
  if i == 0 then m else go(map.set(m, "k0", i), i - 1)

fn main() -> Unit = {{
  let m = list.range(0, 20000) |> list.fold(map.new(), (a, i) => map.set(a, "k${{100000000 + i}}", i))
  let r = go(m, {steps})
  println("${{map.len(r)}} ${{map.get(r, "k0") ?? -1}} ${{map.get(m, "k0") ?? -1}}")
}}
"#
    )
}

fn run(src: &str, wasm: bool) -> (String, String) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("tail.almd");
    std::fs::write(&file, src).expect("source");
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&file);
    if wasm {
        c.args(["--target", "wasm"]).env("ALMIDE_WASM_ALLOC_COUNT", "1");
    }
    let out = c.output().expect("almide run");
    assert!(out.status.success(), "wasm={wasm}: {}", String::from_utf8_lossy(&out.stderr));
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

fn requested_bytes(stderr: &str) -> u64 {
    let line = stderr.lines().find(|l| l.contains("__ALMD_WASM_ALLOC")).expect("alloc count line");
    line.split_whitespace().find_map(|w| w.strip_prefix("bytes=")).and_then(|v| v.parse().ok()).expect("bytes=")
}

#[test]
fn a_tail_call_map_set_rebind_does_not_copy_the_map_per_step() {
    // The caller's `m` is read after the call: the first step copies it once,
    // the rest write that copy in place.
    for steps in [1000, 4000] {
        let src = program(steps);
        let (native, _) = run(&src, false);
        let (wasm, err) = run(&src, true);
        assert_eq!(native, "20001 1 -1\n", "native, {steps} steps");
        assert_eq!(wasm, native, "wasm, {steps} steps");
        let bytes = requested_bytes(&err);
        assert!(bytes < 8_000_000, "{steps} steps requested {bytes} bytes: a copy per step is back");
    }
}
