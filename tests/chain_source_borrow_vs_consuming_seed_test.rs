//! A chain source stays cloned when the fold seed consumes the same list
//! (#2377).
//!
//! `ChainSourceBorrowPass` strips the clone in front of a fused chain's source
//! and iterates from a borrow — the right optimisation, and one of 0.63's
//! headline changes. Its guard asked whether anything inside the chain WRITES
//! the same variable. There are two ways to reach it, and the other one is a
//! MOVE: the fold seed is evaluated while the receiver's borrow is live, so
//!
//!     list.fold(xs, list.is_empty(list.sort_by(xs, f)), g)
//!
//! emitted `(xs).iter().cloned().fold(.. sort_by(xs, ..) ..)` and rustc
//! answered `error[E0505]: cannot move out of 'xs' because it is borrowed` —
//! under the "codegen produced invalid Rust — this is an Almide bug" banner,
//! on a program 0.62.0 compiled and ran.
//!
//! Found by the 2026-09-20 fuzz-nightly campaign (seed 567971353073, index 78)
//! rather than by a fixture, which is why the first test here is the fuzzer's
//! own program reduced only as far as it stays legible.
//!
//! The guard now asks both questions through `moves_var`, the predicate the
//! clone pass already owned. The last test is the one that keeps the
//! optimisation honest: a seed that does NOT touch the list must still borrow.

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

fn tools_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

fn write(name: &str, src: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-issue2377-{}", name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let f = dir.join("m.almd");
    std::fs::write(&f, src).expect("write");
    f
}

/// Build and run; returns (built, stdout+stderr of the build, program stdout).
fn build_and_run(name: &str, src: &str) -> (bool, String, String) {
    let f = write(name, src);
    let out_bin = f.with_file_name("m_bin");
    let b = Command::new(almide_bin())
        .args(["build", "m.almd", "-o", "m_bin"])
        .current_dir(f.parent().unwrap())
        .output()
        .expect("spawn almide");
    let mut log = String::from_utf8_lossy(&b.stdout).to_string();
    log.push_str(&String::from_utf8_lossy(&b.stderr));
    if !b.status.success() {
        return (false, log, String::new());
    }
    let r = Command::new(&out_bin).output().expect("run built program");
    (true, log, String::from_utf8_lossy(&r.stdout).trim().to_string())
}

#[test]
fn a_fold_seed_that_consumes_the_source_still_compiles() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let (ok, log, out) = build_and_run(
        "seed-consumes",
        concat!(
            "fn main() -> Unit = {\n",
            "  let xs: List[Bool] = [true]\n",
            "  let r: Bool = list.fold(xs, list.is_empty(list.sort_by(xs, ((y) => 1))), ((a, b) => a))\n",
            "  println(\"${r}\")\n",
            "}\n",
        ),
    );
    assert!(ok, "the chain borrowed a source its own seed moves:\n{log}");
    assert!(!log.contains("E0505"), "still E0505:\n{log}");
    assert_eq!(out, "false", "the program's answer changed");
}

/// `list.scan` is the shape the fuzzer actually produced; `sort_by` above is
/// the reduction. Both are by-value consumers that also take a closure, and
/// `list.sort` — by value, no closure — never reached the bug, so both stay.
#[test]
fn the_same_holds_for_a_scan_in_the_seed() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let (ok, log, out) = build_and_run(
        "seed-scan",
        concat!(
            "fn main() -> Unit = {\n",
            "  let xs: List[Bool] = []\n",
            "  let v: String? = none\n",
            "  let r: String? = list.fold(\n",
            "    xs,\n",
            "    (if list.is_empty(list.scan(xs, 1, ((y, s) => 1))) then v else some(\"\")),\n",
            "    ((x, a) => v),\n",
            "  )\n",
            "  println(\"r = ${r}\")\n",
            "}\n",
        ),
    );
    assert!(ok, "the scan-in-seed shape did not build:\n{log}");
    assert_eq!(out, "r = none", "the program's answer changed");
}

/// The guard must not cost the optimisation it guards. A seed that does not
/// mention the list still iterates from a borrow — no `.clone()` in front of
/// the source. Asserted on the emitted Rust, because a build that merely
/// succeeds cannot tell a borrow from a clone.
#[test]
fn a_seed_that_does_not_touch_the_list_still_borrows() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let f = write(
        "seed-clean",
        concat!(
            "fn main() -> Unit = {\n",
            "  let xs: List[Bool] = [true, false]\n",
            "  let r: Int = list.fold(xs, 0, ((a, b) => a + 1))\n",
            "  println(\"${r}\")\n",
            "}\n",
        ),
    );
    let out = Command::new(almide_bin())
        .args(["m.almd", "--target", "rust"])
        .current_dir(f.parent().unwrap())
        .output()
        .expect("spawn almide");
    let rust = String::from_utf8_lossy(&out.stdout).to_string();
    // The runtime prelude is thousands of lines and has `.fold(` over its own
    // `xs` parameters, so neither the method nor the variable name pins the
    // line. The emitter marks where the runtime ends; read only past it.
    let user = rust
        .split_once("//__ALMIDE_RT_BOUNDARY__")
        .unwrap_or_else(|| panic!("no runtime boundary marker in the emitted Rust"))
        .1;
    let fold = user
        .lines()
        .find(|l| l.contains(".fold("))
        .unwrap_or_else(|| panic!("no fold in the emitted user code:\n{user}"));
    assert!(
        fold.contains("(xs).iter()"),
        "the source is no longer borrowed — the guard cost the optimisation:\n{fold}"
    );
    assert!(
        !fold.contains("xs.clone()"),
        "a clone came back for a seed that does not touch the list:\n{fold}"
    );
}
