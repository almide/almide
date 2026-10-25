//! #4202: native JSON object keys are interned in ONE process-wide table of
//! bounded size. The table was per thread, with a leaked allocation per new
//! key, so every `fan` arm (a fresh thread per call) leaked its own table's
//! worth of keys of any length, and RSS grew with every call: 175 MB at 50
//! iterations and 566 MB at 200 in the issue's measurement, while the
//! sequential run stayed near 21 MB. Here six times the calls must not cost
//! anything like six times the memory.
//!
//! Peak RSS comes from `/usr/bin/time` (`-l` on macOS, `-v` with GNU time);
//! where neither is available the test says so and passes vacuously.

use std::process::Command;

const PROGRAM: &str = r#"import json
import env

effect fn parse_keys(s: String) -> Int = {
  let v = json.parse(s)!
  string.len(json.stringify(v)) % 7
}

effect fn main() -> Unit = {
  let iters = int.parse(env.args() |> list.get(0) ?? "10") ?? 10
  let pad = string.repeat("k", 2000)
  let s = "{" + (list.range(0, 1000) |> list.map((i) => "\"${i}${pad}\":${i}") |> list.join(",")) + "}"
  var acc = 0
  for i in list.range(0, iters) {
    let (a, b) = fan { parse_keys(s), parse_keys(s) }
    acc = acc + a + b
  }
  println("${acc}")
}
"#;

/// Peak RSS in bytes of `bin iters`, or None when no `time` can report it.
fn peak_rss(bin: &std::path::Path, iters: u32) -> Option<u64> {
    let (flag, key, scale) = if cfg!(target_os = "macos") {
        ("-l", "maximum resident set size", 1)
    } else {
        ("-v", "Maximum resident set size (kbytes):", 1024)
    };
    let out = Command::new("/usr/bin/time").arg(flag).arg(bin).arg(iters.to_string()).output().ok()?;
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let err = String::from_utf8_lossy(&out.stderr);
    let line = err.lines().find(|l| l.contains(key))?;
    let n: u64 = line.split_whitespace().find_map(|w| w.parse().ok())?;
    Some(n * scale)
}

#[test]
fn fan_arms_parsing_json_do_not_grow_rss_with_every_call() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("keys.almd");
    std::fs::write(&src, PROGRAM).unwrap();
    let bin = dir.path().join("keys");
    let built = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["build"])
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run almide build");
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    let Some(small) = peak_rss(&bin, 40) else {
        eprintln!("no /usr/bin/time that reports peak RSS here; nothing measured");
        return;
    };
    let large = peak_rss(&bin, 240).expect("the second measurement");
    assert!(
        large < small * 2,
        "6x the fan calls took {} MB of peak RSS against {} MB",
        large >> 20,
        small >> 20
    );
}
