//! #1690: when the structural leg declines and the incumbent fallback also
//! refuses, only the incumbent's wall used to be printed — the DEFAULT leg's
//! own reason (often a completely different shape, e.g. the two-mut-param
//! C-132 write-back) was invisible, and the reader bisected a function the
//! structural leg lowers fine for a reason that belongs to the other engine.
//! The router now prints both, each labelled with its leg.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// An effect move-mode call (C-132) bound as a Result VALUE instead of
/// propagated with `!`: outside both legs' subsets today (the structural leg
/// sees the carrier where the Result was declared; the incumbent refuses the
/// heap bind from Unit), so the build must fail — with BOTH reasons on
/// stderr. (The two-mut-param shape this test used first lowers on the
/// structural leg since #2907.)
const DOUBLE_WALL: &str = r#"effect fn fill(mut a: List[Int], x: Int) -> Unit = {
  a[0] = a[0] + x
  list.push(a, x)
}

effect fn main() -> Unit = {
  var a = [0]
  let _r: Result[Unit, String] = fill(a, 2)
  println("${a}")
}
"#;

#[test]
fn both_leg_walls_are_reported_with_their_leg() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("double_wall.almd");
    let wasm = dir.path().join("double_wall.wasm");
    std::fs::write(&src, DOUBLE_WALL).expect("write repro");

    let out = Command::new(almide())
        .args(["build", src.to_str().unwrap(), "--target", "wasm", "-o", wasm.to_str().unwrap()])
        .output()
        .expect("run almide build");
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "a program neither leg lowers must fail the build; stderr:\n{stderr}"
    );
    // The incumbent's wall keeps its rich diagnostics…
    assert!(
        stderr.contains("not in this brick"),
        "the incumbent's wall text is missing; stderr:\n{stderr}"
    );
    // …and the structural leg's own reason is no longer swallowed.
    assert!(
        stderr.contains("wall (structural leg, the default):"),
        "the structural leg's wall line is missing (#1690); stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("both wasm legs refused"),
        "the both-legs note is missing; stderr:\n{stderr}"
    );
    // #2807: the structural wall says where it came from — the reason string
    // alone (`ty-mismatch:…`) names no location, and
    // in a 77-file package that could not be acted on.
    assert!(
        stderr.contains("\n  --> in fn `main` (entry file, line 8)"),
        "the structural wall's site line is missing (#2807); stderr:\n{stderr}"
    );
}

/// The added lines must NOT appear when the reroute succeeds: a shape the
/// structural leg declines but the incumbent lowers stays a silent, working
/// handover (the pre-#1690 contract for the success path).
const HANDOVER_OK: &str = r#"import env

fn main() -> Unit = {
  let os = env.os()
  println(os)
}
"#;

#[test]
fn successful_handover_stays_silent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("handover.almd");
    let wasm = dir.path().join("handover.wasm");
    std::fs::write(&src, HANDOVER_OK).expect("write repro");

    let out = Command::new(almide())
        .args(["build", src.to_str().unwrap(), "--target", "wasm", "-o", wasm.to_str().unwrap()])
        .output()
        .expect("run almide build");
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(out.status.success(), "host-variant program must build via the incumbent; stderr:\n{stderr}");
    assert!(
        !stderr.contains("wall (structural leg"),
        "the both-walls report leaked into a successful handover; stderr:\n{stderr}"
    );
}
