//! C-329: `env.set` / `env.get` behave identically on native, the embedded
//! host and a stock p1 artifact (last write wins), at any size that fits in
//! memory. The stock artifact's overlay log used to be one fixed 64 KiB page
//! that refused (`env.set log full`) once the cumulative bytes passed it —
//! a single large value, or enough re-sets of ONE name, aborted a program
//! native runs to completion. Each scenario here runs on all three legs and
//! the outputs must agree byte for byte.
use std::path::{Path, PathBuf};
use std::process::Command;

/// One 200 KB value set, then read back.
const BIG: &str = r#"import env

effect fn main() -> Unit = {
  let big = string.repeat("x", 200000)
  env.set("XF_BIG", big)!
  let got = env.get("XF_BIG") ?? ""
  println("big ${string.len(got)} ${got == big}")
}
"#;

/// 5000 overwrites of ONE name with 100-byte values: last write wins, and
/// the overwritten values must not accumulate against any cap.
const OVERWRITE: &str = r#"import env

effect fn main() -> Unit = {
  let pad = string.repeat("v", 96)
  for i in 0..<5000 {
    env.set("XF_SAME", "${i + 1000}${pad}")!
  }
  println("same ${env.get("XF_SAME") ?? "unset"}")
}
"#;

/// 2000 distinct names with 100-byte values (~210 KB live), a few read back.
const DISTINCT: &str = r#"import env

effect fn main() -> Unit = {
  let pad = string.repeat("v", 96)
  for i in 0..<2000 {
    env.set("XF_K${i}", "${i + 1000}${pad}")!
  }
  env.set("XF_K7", "re-set")!
  println("k0 ${env.get("XF_K0") ?? "unset"}")
  println("k7 ${env.get("XF_K7") ?? "unset"}")
  println("k999 ${env.get("XF_K999") ?? "unset"}")
  println("k1999 ${env.get("XF_K1999") ?? "unset"}")
  println("k2000 ${env.get("XF_K2000") ?? "unset"}")
}
"#;

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

fn probe_dir(tag: &str, src: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-env-growth-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("mkdir");
    std::fs::write(d.join("probe.almd"), src).expect("write");
    d
}

fn run_leg(dir: &Path, args: &[&str]) -> String {
    let o = Command::new(almide_bin())
        .arg("run")
        .arg("probe.almd")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn almide");
    assert!(o.status.success(), "leg {args:?} failed:\n{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).expect("utf8")
}

/// The shipped form: `almide build --target wasm` run by a stock wasmtime.
/// `None` without wasmtime.
fn run_stock_p1(dir: &Path) -> Option<String> {
    let wasm = dir.join("probe.wasm");
    let o = Command::new(almide_bin())
        .args(["build", "probe.almd", "--target", "wasm", "-o"])
        .arg(&wasm)
        .current_dir(dir)
        .output()
        .expect("spawn almide");
    assert!(o.status.success(), "stock build failed:\n{}", String::from_utf8_lossy(&o.stderr));
    let o = Command::new("wasmtime").args(["run", "--dir=."]).arg(&wasm).current_dir(dir).output().ok()?;
    assert!(o.status.success(), "stock run failed:\n{}", String::from_utf8_lossy(&o.stderr));
    Some(String::from_utf8(o.stdout).expect("utf8"))
}

fn every_leg_agrees(tag: &str, src: &str, expected: &str) {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let dir = probe_dir(tag, src);
    assert_eq!(run_leg(&dir, &[]), expected, "native");
    assert_eq!(run_leg(&dir, &["--target", "wasm"]), expected, "the embedded host");
    if let Some(stock) = run_stock_p1(&dir) {
        assert_eq!(stock, expected, "the stock-p1 artifact");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

fn padded(n: usize) -> String {
    format!("{n}{}", "v".repeat(96))
}

#[test]
fn one_large_value_round_trips() {
    every_leg_agrees("big", BIG, "big 200000 true\n");
}

#[test]
fn overwriting_one_name_does_not_accumulate() {
    every_leg_agrees("overwrite", OVERWRITE, &format!("same {}\n", padded(5999)));
}

#[test]
fn many_distinct_names_all_survive() {
    let expected =
        format!("k0 {}\nk7 re-set\nk999 {}\nk1999 {}\nk2000 unset\n", padded(1000), padded(1999), padded(2999));
    every_leg_agrees("distinct", DISTINCT, &expected);
}
