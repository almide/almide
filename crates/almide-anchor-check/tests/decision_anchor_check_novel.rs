//! Row `anchor.check`: what the check judges is not chosen by the producer.
//!
//! A bundle whose init, bounds or fields record is absent is refused (an
//! absent witness is not an empty one), and every function the bundle
//! certifies (an `ownership` record) must name its RC frame on the anchor
//! file's `rcnames` line — at a position that carries an `rc` frame, or `-`
//! when its body did not ship. A certified function left out, a named
//! position with no frame, a frame line dropped whole, and a name the bundle
//! does not certify are each refused. The function names are fresh.
//!
//! The loop-converted body (tco.rs) ships its frame like any other: a
//! carried local's release recorded at the loop-back is held to the release
//! its next rebind performs, and one recorded with no release left for it is
//! refused.

#[path = "../../../tests/support/novel.rs"]
mod novel;
mod common;

use common::module;

/// A bundle certifying `names` (each an `ownership` record), with the init,
/// bounds and fields records unless one is named in `drop`.
fn bundle(names: &[&str], drop: &[&str]) -> Vec<u8> {
    let mut b = String::from("almide-certificate-bundle 1\nproducer novel\nsource novel\n");
    for n in names {
        b.push_str(&format!("witness ownership 2 {n}\nid\n"));
    }
    for p in ["init", "bounds", "fields"] {
        if !drop.contains(&p) {
            b.push_str(&format!("witness {p} 0 (program)\n\n"));
        }
    }
    b.into_bytes()
}

/// An anchor file whose `rc` frames are `frames` and whose `rcnames` line is
/// `names` (`None`: no such line); `rc: false` writes no rc block at all.
fn anchors(frames: &str, names: Option<&str>, rc: bool) -> String {
    let mut a = String::from("almide-anchors 1\nalloc -\ninit \nbounds \nfields \njudges \nholds \n");
    if rc {
        a.push_str(&format!("rcfns - 0\nrc {frames}\nrcarms \nrcroutines \nrchelpers \n"));
    }
    a.push_str("proved - -\n");
    if let Some(n) = names {
        a.push_str(&format!("rcnames {n}\n"));
    }
    a
}

const BODY: &str = "local.get 0 call $alloc drop";

#[test]
fn novel_every_certified_function_needs_its_rc_frame_and_every_record_its_witness() {
    let wasm = module("(local i32 i32)", BODY);
    let f = novel::ident("certified");
    let g = novel::ident("stranger");
    let ok = |b: &[u8], a: &str| almide_anchor_check::check(&wasm, b, a);

    // The positive case: the certified function names its frame, which holds.
    let r = ok(&bundle(&[&f], &[]), &anchors("1|d0", Some(&format!("1:{f}")), true)).expect("a covered frame is judged and holds");
    assert_eq!(r.rc.as_ref().map(|r| r.frames), Some(1));
    // A body that did not ship has no frame to hold: named `-`, counted.
    let r = ok(&bundle(&[&f], &[]), &anchors("", Some(&format!("-:{f}")), true)).expect("an unshipped body is named, not judged");
    assert_eq!(r.rc_unshipped, 1);

    // The frame left out at a position the function ships at: refused.
    let e = ok(&bundle(&[&f], &[]), &anchors("", Some(&format!("1:{f}")), true)).unwrap_err();
    assert!(e.contains("has no `rc` frame"), "{e}");
    // The certified function not named at all: refused.
    let e = ok(&bundle(&[&f, &g], &[]), &anchors("1|d0", Some(&format!("1:{f}")), true)).unwrap_err();
    assert!(e.contains(&g) && e.contains("RC frame is missing"), "{e}");
    // A name the bundle does not certify: refused.
    let e = ok(&bundle(&[&f], &[]), &anchors("1|d0", Some(&format!("1:{f} 1:{g}")), true)).unwrap_err();
    assert!(e.contains("does not certify"), "{e}");
    // No `rcnames` line, or no rc block at all: refused.
    assert!(ok(&bundle(&[&f], &[]), &anchors("1|d0", None, true)).unwrap_err().contains("no `rcnames` line"));
    assert!(ok(&bundle(&[&f], &[]), &anchors("", None, false)).unwrap_err().contains("no `rc` lines"));

    // An absent init / bounds / fields record: refused, each.
    for p in ["init", "bounds", "fields"] {
        let e = ok(&bundle(&[&f], &[p]), &anchors("1|d0", Some(&format!("1:{f}")), true)).unwrap_err();
        assert!(e.contains(&format!("no `{p}` record")), "{p}: {e}");
    }
}

/// The loop-converted shape: local 1 is a carried owner, its old block
/// released by the next iteration's rebind (`o1`, the dec-old every bind
/// logs) and its last one by the epilogue (`d1`). The recorder ends the
/// activation's credit at the loop-back (`D1`): it takes the rebind's
/// release, and the rebind's `o1` is then one with no instruction.
const LOOP: &str = "(loop
    local.get 1 call $alloc drop
    i32.const 64 local.set 1
    local.get 0 i32.eqz br_if 0)
  local.get 1 call $alloc drop";

#[test]
fn novel_a_loop_back_release_is_held_to_the_rebind_that_performs_it() {
    let wasm = module("(local i32 i32)", LOOP);
    let f = novel::ident("looped");
    let run = |events: &str| almide_anchor_check::check(&wasm, &bundle(&[&f], &[]), &anchors(&format!("1|{events}"), Some(&format!("1:{f}")), true));
    let r = run("o1 D1 d1").expect("the loop-back holds the rebind's release, the epilogue its own");
    let rc = r.rc.expect("rc judged");
    assert_eq!((rc.frames, rc.calls, rc.rebind_unmatched), (1, 2, 1));
    // Two loop-backs on paths no run passes both of (the two arms of one
    // site) share the one rebind release; two on one path do not.
    run("o1 ( D1 | D1 ) d1").expect("exclusive loop-backs share the rebind's release");
    assert!(run("o1 D1 D1 d1").unwrap_err().contains("recorded events with no call [D1]"));
    // A loop-back with no rebind to hold never takes a release no event
    // records: refused, and so is that release.
    let e = run("D1 d1").unwrap_err();
    assert!(e.contains("[D1]") && e.contains("calls with no recorded event [d@"), "{e}");
    // The epilogue's release unrecorded: the loop-back does not cover it.
    assert!(run("o1 D1").unwrap_err().contains("calls with no recorded event [d@"));
}
