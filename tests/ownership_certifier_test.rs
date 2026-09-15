//! The native ownership certifier (#2231) fires on the two defect shapes
//! rustc cannot see, and stays silent on a body whose verdicts are right —
//! pinned on the REAL compiler's output, so the certifier's sensitivity is
//! measured rather than assumed (roc's `arc_certify` tests are the model).
//!
//! Corpus-wide the certifier is held by scripts/check-ownership-certifier.sh
//! against a shrink-only ledger; this file is the per-shape proof that the
//! ledger's lines mean what they say.

use std::path::Path;
use std::process::Command;

fn certify(tag: &str, src: &str) -> (bool, String) {
    let dir = std::env::temp_dir().join(format!("almide-certify-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("prog.almd");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg(&file).arg("--target").arg("rust")
        .env("ALMIDE_CERTIFY_OWNERSHIP", "fail")
        .output().expect("almide");
    let _ = std::fs::remove_dir_all(Path::new(&dir));
    (out.status.success(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn a_capture_cloned_at_its_last_use_is_a_c3_violation() {
    // `name` is cloned into the closure and never used again: ownership was
    // available, the clone copies for nothing. Builds and prints correctly
    // on both legs — only the certifier and the allocation count see it.
    let (ok, err) = certify("c3", "fn greeter(name: String) -> (String) -> String = (x) => name + \", \" + x\nfn main() -> Unit = println(greeter(\"hi\")(\"you\"))\n");
    assert!(!ok, "the build must fail under ALMIDE_CERTIFY_OWNERSHIP=fail:\n{err}");
    assert!(err.contains("[C3 clone-at-last-use] greeter: `name: String`"), "{err}");
}

#[test]
fn an_owned_param_the_body_only_borrows_is_a_c4_violation() {
    // A monomorphised generic whose body only borrows its list still takes
    // it owned: every caller moves (or clones) a value the body never needs.
    let (ok, err) = certify("c4", "fn first[T](xs: List[T]) -> T? = list.get(xs, 0)\nfn main() -> Unit = {\n  let xs = [1, 2]\n  println(int.to_string(first(xs) ?? 0))\n  println(int.to_string(list.len(xs)))\n}\n");
    assert!(!ok, "the build must fail under ALMIDE_CERTIFY_OWNERSHIP=fail:\n{err}");
    assert!(err.contains("[C4 owned-never-consumed] first__Int: param `xs"), "{err}");
}

#[test]
fn a_body_whose_verdicts_are_right_certifies() {
    // Consumes its param once (concat), borrows the other (len), clones
    // nothing: every verdict is the one the certifier would derive.
    let (ok, err) = certify("clean", "fn shout(s: String, tail: String) -> String = s + \"!\" + int.to_string(string.len(tail))\nfn main() -> Unit = {\n  let t = \"abc\"\n  println(shout(\"hey\", t))\n  println(t)\n}\n");
    assert!(ok, "a clean body must certify:\n{err}");
    assert!(!err.contains("[CERTIFY OWNERSHIP]"), "{err}");
}
