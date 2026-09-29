//! #1922 — a wasm wall is a named diagnostic (E082) and `almide check
//! --target wasm` reports it at CHECK time by running the build path's own
//! route (no declared table: measured, so it cannot drift from what `build`
//! does). Since #2752 there is one leg: E082 is that leg's refusal, with its
//! reason — there is no fallback renderer to hand the program to. E082 lives
//! in the build path, so it is pinned here rather than through the
//! check-harness fixtures (the E054 / E081 precedent).

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

/// A shape the wasm leg refuses ON PURPOSE, not one it merely has not
/// reached yet: a mut op through a DEEPER field path (`o.inner.xs`), which
/// the var-only arms refuse honestly (list_mut.rs: only `h.f` routes through
/// the copy-on-write field write). `the_ingredient_is_still_refused_in_the_
/// emitter` pins that refusal's spelling in the emitter source, so the day it
/// is lowered this test fails with a pointer instead of passing silently.
const WALLED: &str = r#"type Inner = { xs: List[Int] }
type Outer = { inner: Inner }

effect fn main() -> Unit = {
  var o = Outer { inner: Inner { xs: [1] } }
  list.push(o.inner.xs, 2)
  println("${list.len(o.inner.xs)}")
}
"#;

/// The wasm leg's decline reason for `WALLED`'s `list.push(o.inner.xs, ..)`,
/// as the emitter spells it.
const STRUCTURAL_DECLINE: &str = "list-push-nonvar";

#[test]
fn the_ingredient_is_still_refused_in_the_emitter() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/almide-wasm/src");
    let needle = format!("unsup(\"{STRUCTURAL_DECLINE}\")");
    let found = std::fs::read_dir(&src).expect("read crates/almide-wasm/src").any(|e| {
        let p = e.expect("dir entry").path();
        p.extension().is_some_and(|x| x == "rs") && std::fs::read_to_string(&p).is_ok_and(|t| t.contains(&needle))
    });
    assert!(
        found,
        "crates/almide-wasm/src no longer refuses with `{needle}`: the wasm leg now lowers \
         WALLED, so it is no longer a wall. Replace it (and STRUCTURAL_DECLINE) with a shape \
         the leg still refuses on purpose, and check it gives E082."
    );
}

/// A plain fs program builds (through the p1 fs service, since #2742).
const SERVED: &str = r#"import fs

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
fn check_target_wasm_reports_the_wall_as_e082() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let src = write("deepfield.almd", WALLED);
    // The native check stays clean: E082 is a wasm-route verdict.
    let o = Command::new(almide_bin()).args(["check", src.to_str().unwrap()]).output().expect("spawn");
    assert!(o.status.success(), "the native check must stay clean:\n{}", String::from_utf8_lossy(&o.stderr));

    let o = Command::new(almide_bin())
        .args(["check", src.to_str().unwrap(), "--target", "wasm"])
        .output()
        .expect("spawn");
    let log = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(!o.status.success(), "check --target wasm must refuse the wall:\n{log}");
    assert!(log.contains("error[E082]"), "must carry the E082 code:\n{log}");
    assert!(log.contains(STRUCTURAL_DECLINE), "must name the leg's reason ({STRUCTURAL_DECLINE}):\n{log}");
    assert!(
        log.lines().any(|l| l.trim() == format!("{}{STRUCTURAL_DECLINE}", almide::WASM_WALL_MARKER)),
        "must carry the machine-readable wall line:\n{log}"
    );
    assert!(log.contains("  --> in "), "must name the fn the wall came from (#2807):\n{log}");
    assert!(!log.contains("incumbent"), "there is no second leg to name:\n{log}");
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
fn check_target_wasm_stays_clean_when_the_leg_serves() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let src = write("plainfs.almd", SERVED);
    let o = Command::new(almide_bin())
        .args(["check", src.to_str().unwrap(), "--target", "wasm"])
        .output()
        .expect("spawn");
    let log = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(o.status.success(), "a served program is a clean wasm route:\n{log}");
    assert!(log.contains("No errors found"), "the verdict line stays:\n{log}");
    assert!(!log.contains("error[E08"), "no availability code on a served program:\n{log}");
}

#[test]
fn check_target_accepts_only_wasm() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let src = write("plain.almd", SERVED);
    let o = Command::new(almide_bin())
        .args(["check", src.to_str().unwrap(), "--target", "rust"])
        .output()
        .expect("spawn");
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("accepts only `wasm`"));
}
