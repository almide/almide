//! #1922 — the both-legs wasm wall is a named diagnostic (E082) and
//! `almide check --target wasm` reports it at CHECK time by running the
//! build path's own routing decision (no declared table: measured, so it
//! cannot drift from what `build` does). E082 lives in the build path, so
//! it is pinned here rather than through the check-harness fixtures (the
//! E054 / E081 precedent).

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

/// The issue's program shape — the CHEATSHEET's fallible-map idiom over
/// `fan.*` with an fs op inside, which the incumbent walls — plus shapes the
/// structural leg still declines. Its original structural reason (the fs op
/// had no stock-WASI service) is gone since the p1 fs service (#2742), so the
/// structural decline now comes from the retirement bill's open rows
/// (`list.partition` and matrix `*`; `random.shuffle` #2749 and `continue`
/// #2745 were the previous pair until both landed together): two, so one
/// landing does not silently turn this test into a pass of a different
/// program.
const BOTH_LEGS_WALL: &str = r#"import fs

effect fn read_one(p: String) -> String = { let t = fs.read_text(p)!; string.trim(t) }

effect fn main() -> Unit = {
  let (evens, odds) = list.partition([1, 2, 3, 4], (x) => x % 2 == 0)
  let m = matrix.from_lists([[1.0, 2.0], [3.0, 4.0]])
  let sq = m * m
  let texts = fan.map(["a.txt"], (p) => read_one(p)!)!
  println("${list.len(evens)} ${list.len(odds)} ${matrix.rows(sq)} " + (texts |> list.join(",")))
}
"#;

/// One ingredient alone: a plain fs program builds (on the structural leg,
/// through the p1 fs service, since #2742).
const ONE_LEG_SERVES: &str = r#"import fs

effect fn main() -> Unit = {
  let t = fs.read_text("a.txt")!
  println(t)
}
"#;

fn write(name: &str, src: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join("almide-e082");
    std::fs::create_dir_all(&d).expect("mkdir");
    let p = d.join(name);
    std::fs::write(&p, src).expect("write");
    p
}

#[test]
fn check_target_wasm_reports_the_both_legs_wall_as_e082() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let src = write("fanfs.almd", BOTH_LEGS_WALL);
    // The native check stays clean: E082 is a wasm-route verdict.
    let o = Command::new(almide_bin()).args(["check", src.to_str().unwrap()]).output().expect("spawn");
    assert!(o.status.success(), "the native check must stay clean:\n{}", String::from_utf8_lossy(&o.stderr));

    let o = Command::new(almide_bin())
        .args(["check", src.to_str().unwrap(), "--target", "wasm"])
        .output()
        .expect("spawn");
    let log = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(!o.status.success(), "check --target wasm must refuse the both-legs wall:\n{log}");
    assert!(log.contains("error[E082]"), "must carry the E082 code:\n{log}");
    assert!(log.contains("wall (structural leg, the default): "), "must name the structural leg's reason:\n{log}");
    assert!(log.contains("fan.map consumed by"), "must name the incumbent's shape reason:\n{log}");
    assert!(!log.contains("No errors found"), "a refused route is not a clean check:\n{log}");

    // The build path carries the same code (the backstop).
    let o = Command::new(almide_bin())
        .args(["build", src.to_str().unwrap(), "--target", "wasm", "-o", "/dev/null"])
        .output()
        .expect("spawn");
    let log = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(!o.status.success() && log.contains("error[E082]"), "build must refuse with E082:\n{log}");
}

#[test]
fn check_target_wasm_stays_clean_when_one_leg_serves() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let src = write("plainfs.almd", ONE_LEG_SERVES);
    let o = Command::new(almide_bin())
        .args(["check", src.to_str().unwrap(), "--target", "wasm"])
        .output()
        .expect("spawn");
    let log = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(o.status.success(), "one serving leg is a clean wasm route:\n{log}");
    assert!(log.contains("No errors found"), "the verdict line stays:\n{log}");
    assert!(!log.contains("error[E08"), "no availability code on a served program:\n{log}");
}

#[test]
fn check_target_accepts_only_wasm() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let src = write("plain.almd", ONE_LEG_SERVES);
    let o = Command::new(almide_bin())
        .args(["check", src.to_str().unwrap(), "--target", "rust"])
        .output()
        .expect("spawn");
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("accepts only `wasm`"));
}
