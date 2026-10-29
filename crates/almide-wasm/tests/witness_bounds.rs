//! #4005 — the bounds witness (witness_bounds.rs) of #3345's elided checks.
//! fannkuchredux's flip reads `perm[lo]` and `perm[hi]` and stores to both:
//! the emitter checks the two reads and elides the two stores' checks. The
//! witness must say exactly that, the proven checker's portable mirror must
//! accept it, and a witness missing either check must be rejected.

const PROGRAM: &str = r#"fn main() -> Unit = {
  var perm = [3, 1, 2, 0]
  var lo = 0
  var hi = 3
  while lo < hi {
    let a = perm[lo]
    let b = perm[hi]
    perm[lo] = b
    perm[hi] = a
    lo = lo + 1
    hi = hi - 1
  }
  println("${perm[0]} ${perm[3]}")
}
"#;

fn bounds() -> Vec<(usize, String)> {
    let ir = almide_spine::s5::lower_to_ir("flip.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    let w = almide_wasm::witness::take_bounds();
    let _ = almide_wasm::witness::take();
    w
}

fn accepts(w: &str) -> bool {
    almide_verify::check(almide_verify::Property::Bounds, w.as_bytes())
}

#[test]
fn the_flip_elides_two_checks_and_the_checker_re_derives_them() {
    // ONE test: the witness sink is process-global.
    let all = bounds();
    let (_, w) = all.last().expect("a bounds witness for the shipped pass");
    // Two checked reads, two writes on the same slots with no check, then
    // the assignments of lo and hi kill both slots. The two reads in the
    // `println` are no fact candidates (outside any loop): each has a fresh
    // slot in the loose stretch, decided by its own check (#4005).
    assert_eq!(w, "c0 a0 c1 a1 w0 w1 s0 s1;c0 a0 c1 a1");
    assert!(accepts(w), "the portable checker must accept {w:?}");
    // The drills: drop either check and an elided write is unjustified.
    for (drop, name) in [("c0 ", "the lo check"), ("c1 ", "the hi check")] {
        let forged = w.replacen(drop, "", 1);
        assert!(!accepts(&forged), "without {name}, {forged:?} must be rejected");
    }
    // A kill moved before the writes: the facts no longer hold there.
    assert!(!accepts("c0 a0 c1 a1 s0 w0 w1 s1;c0 a0 c1 a1"));
    // A loose access whose own check is missing.
    assert!(!accepts(&w.replacen(";c0 ", ";", 1)));
}
