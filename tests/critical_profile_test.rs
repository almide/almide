//! #567: `almide check --profile critical` — the bounded profile (ALS §B,
//! E070–E078) applied to EVERY function of the entry program, capabilities
//! deny-all with explicit `--allow` grants.
//!
//! The load-bearing invariant is the SUBSET PROPERTY (the CG-3 "subset, not
//! a dialect" rule): critical only widens what is rejected, so every
//! critical-valid program is normal-valid, and every negative fixture here
//! is asserted to PASS the normal check — proving the profile adds
//! rejections without changing the language underneath.

use std::path::Path;
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// Run `almide check <args>` on a source string; returns (success, stderr).
fn check(source: &str, name: &str, args: &[&str]) -> (bool, String) {
    let dir = std::env::temp_dir().join("almide-critical-profile");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let src = dir.join(name);
    std::fs::write(&src, source).expect("write");
    let out = Command::new(almide())
        .arg("check")
        .arg(&src)
        .args(args)
        .current_dir(&dir)
        .output()
        .expect("spawn almide check");
    (out.status.success(), String::from_utf8_lossy(&out.stderr).to_string())
}

const CLEAN: &str = "fn sum_to(n: Int) -> Int = {\n  var acc = 0\n  for i in 0..<10 {\n    acc = acc + i\n  }\n  acc + n\n}\n\nfn main() -> Unit = {\n  println(int.to_string(sum_to(1)))\n}\n";

const WHILE_LOOP: &str = "fn count(n: Int) -> Int = {\n  var i = 0\n  while i < n {\n    i = i + 1\n  }\n  i\n}\n\nfn main() -> Unit = {\n  println(int.to_string(count(5)))\n}\n";

const RECURSION: &str = "fn fact(n: Int) -> Int =\n  if n <= 1 then 1\n  else n * fact(n - 1)\n\nfn main() -> Unit = {\n  println(int.to_string(fact(5)))\n}\n";

const RANDOM: &str = "import random\n\neffect fn main() -> Unit = {\n  let r = random.int(10, 20)\n  println(int.to_string(r))\n}\n";

#[test]
fn critical_clean_program_passes_both_modes() {
    let (crit, err) = check(CLEAN, "clean.almd", &["--profile", "critical"]);
    assert!(crit, "critical rejected the clean program:\n{err}");
    // the subset witness: the same file under the NORMAL check
    let (normal, err) = check(CLEAN, "clean.almd", &[]);
    assert!(normal, "normal check rejected the critical-clean program:\n{err}");
}

#[test]
fn while_loop_rejected_under_critical_only() {
    let (crit, err) = check(WHILE_LOOP, "wh.almd", &["--profile", "critical"]);
    assert!(!crit, "critical accepted a while loop");
    assert!(err.contains("E070"), "expected E070, got:\n{err}");
    // addressed to the profile, not to an attribute the author never wrote
    assert!(err.contains("--profile critical"), "message not profile-addressed:\n{err}");
    assert!(!err.contains("@bounded"), "critical diagnostic leaked @bounded addressing:\n{err}");
    let (normal, err) = check(WHILE_LOOP, "wh.almd", &[]);
    assert!(normal, "subset property broken — normal check rejected it too:\n{err}");
}

#[test]
fn recursion_rejected_under_critical_only() {
    let (crit, err) = check(RECURSION, "rec.almd", &["--profile", "critical"]);
    assert!(!crit, "critical accepted recursion");
    assert!(err.contains("E073"), "expected E073, got:\n{err}");
    let (normal, err) = check(RECURSION, "rec.almd", &[]);
    assert!(normal, "subset property broken — normal check rejected it too:\n{err}");
}

#[test]
fn capability_deny_all_with_explicit_grant() {
    // deny-all start: entropy is rejected and the hint names the grant flag
    let (crit, err) = check(RANDOM, "rand.almd", &["--profile", "critical"]);
    assert!(!crit, "critical accepted random.int without a grant");
    assert!(err.contains("E076"), "expected E076, got:\n{err}");
    assert!(err.contains("--allow"), "hint does not name the grant flag:\n{err}");
    // the explicit grant admits exactly that capability
    let (granted, err) =
        check(RANDOM, "rand.almd", &["--profile", "critical", "--allow", "Rand"]);
    assert!(granted, "--allow Rand did not admit random.int:\n{err}");
    // and the grant is per-capability: Time does not cover entropy
    let (wrong, _) = check(RANDOM, "rand.almd", &["--profile", "critical", "--allow", "Time"]);
    assert!(!wrong, "--allow Time wrongly admitted random.int");
    let (normal, err) = check(RANDOM, "rand.almd", &[]);
    assert!(normal, "subset property broken — normal check rejected it too:\n{err}");
}

#[test]
fn cli_vocabulary_is_closed() {
    let (ok, err) = check(CLEAN, "clean.almd", &["--profile", "bogus"]);
    assert!(!ok);
    assert!(err.contains("unknown profile"), "{err}");
    let (ok, err) = check(CLEAN, "clean.almd", &["--profile", "critical", "--allow", "Bogus"]);
    assert!(!ok);
    assert!(err.contains("unknown capability"), "{err}");
    let (ok, err) = check(CLEAN, "clean.almd", &["--allow", "Rand"]);
    assert!(!ok);
    assert!(err.contains("--allow requires --profile critical"), "{err}");
}

#[test]
fn attribute_mode_unchanged_by_the_profile_machinery() {
    // a plain check of a file with NO @bounded attribute must not run the
    // profile: the while loop passes, exactly as before #567
    let (normal, err) = check(WHILE_LOOP, "wh.almd", &[]);
    assert!(normal, "{err}");
    // and the e07x attribute fixtures still pin @bounded mode — this test
    // only guards the flag default staying off
    assert!(Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("crates/almide-frontend/src/check/bounded.rs")
        .exists());
}
