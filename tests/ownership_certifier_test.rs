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
    certify_with(tag, src, &[])
}

fn certify_with(tag: &str, src: &str, env: &[(&str, &str)]) -> (bool, String) {
    let dir = std::env::temp_dir().join(format!("almide-certify-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("prog.almd");
    std::fs::write(&file, src).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.arg(&file).arg("--target").arg("rust").env("ALMIDE_CERTIFY_OWNERSHIP", "fail");
    for (k, v) in env { cmd.env(k, v); }
    if let Some(unset) = env.iter().find(|(k, _)| k.is_empty()).map(|(_, v)| *v) { cmd.env_remove(unset); }
    let out = cmd.output().expect("almide");
    let _ = std::fs::remove_dir_all(Path::new(&dir));
    (out.status.success(), String::from_utf8_lossy(&out.stderr).into_owned())
}

const GREETER: &str = "fn greeter(name: String) -> (String) -> String = (x) => name + \", \" + x\nfn main() -> Unit = println(greeter(\"hi\")(\"you\"))\n";

#[test]
fn a_capture_cloned_at_its_last_use_is_a_c3_violation() {
    // With last-use moves ablated, `name` is cloned into the closure and
    // never used again: ownership was available, the clone copies for
    // nothing. Builds and prints correctly on both legs — only the certifier
    // and the allocation count see it.
    let (ok, err) = certify_with("c3", GREETER, &[("ALMIDE_CAPTURE_MOVE_OFF", "1")]);
    assert!(!ok, "the build must fail under ALMIDE_CERTIFY_OWNERSHIP=fail:\n{err}");
    assert!(err.contains("[C3 clone-at-last-use] greeter: `name: String`"), "{err}");
}

#[test]
fn a_capture_whose_closure_is_its_sole_user_moves_and_certifies() {
    // The same program with CaptureClone's last-use move (#2231): the bind
    // is `let __cap = name`, and the body certifies clean.
    let (ok, err) = certify("c3-fixed", GREETER);
    assert!(ok, "the greeter must certify once the capture moves:\n{err}");
}

const FIRST: &str = "fn first[T](xs: List[T]) -> T? = list.get(xs, 0)\nfn main() -> Unit = {\n  let xs = [1, 2]\n  println(int.to_string(first(xs) ?? 0))\n  println(int.to_string(list.len(xs)))\n}\n";

#[test]
fn an_owned_param_the_body_only_borrows_is_a_c4_violation() {
    // With borrow inference ablated every eligible param is owned: the
    // monomorphised `first__Int` takes its list owned though the body only
    // borrows it — every caller moves (or clones) a value the body never
    // needs. The certifier names it.
    let (ok, err) = certify_with("c4", FIRST, &[("ALMIDE_BORROW_OWN_ALL", "1")]);
    assert!(!ok, "the build must fail under ALMIDE_CERTIFY_OWNERSHIP=fail:\n{err}");
    assert!(err.contains("[C4 owned-never-consumed] first__Int: param `xs"), "{err}");
}

#[test]
fn a_monomorphised_instance_borrows_and_certifies() {
    // The same program with inference on (#2231 wave 2): `first__Int` takes
    // `&[i64]` and the body certifies clean.
    let (ok, err) = certify("c4-fixed", FIRST);
    assert!(ok, "first__Int must certify once instances are inferred:\n{err}");
}

#[test]
fn a_body_whose_verdicts_are_right_certifies() {
    // Consumes its param once (concat), borrows the other (len), clones
    // nothing: every verdict is the one the certifier would derive.
    let (ok, err) = certify("clean", "fn shout(s: String, tail: String) -> String = s + \"!\" + int.to_string(string.len(tail))\nfn main() -> Unit = {\n  let t = \"abc\"\n  println(shout(\"hey\", t))\n  println(t)\n}\n");
    assert!(ok, "a clean body must certify:\n{err}");
    assert!(!err.contains("[CERTIFY OWNERSHIP]"), "{err}");
}

#[test]
fn a_capture_cloned_inside_an_interpolation_that_also_prints_it_certifies() {
    // `k` is a bare part of the interpolation (`format_args!` borrows it for
    // the whole `format!`) and captured by the closure built INSIDE the same
    // interpolation: the capture must clone (moving `k` there is E0505), and
    // the clone pass's `holds_last_occurrence` says so. The certifier used
    // to call it a C3 violation because the capture-clone binding sits in a
    // block of its own, one statement ordinal away from the part that holds
    // the borrow — the rule now reads the OUTERMOST statement.
    let src = "fn main() -> Unit = {\n  let k = string.reverse(\"abc\")\n  println(\"${k} ${list.zip_with([1, 2], [3, 4], ((x, y) => k))}\")\n}\n";
    let (ok, err) = certify("c3-interp-held", src);
    assert!(ok, "a capture clone the interpolation forces must certify:\n{err}");
    assert!(!err.contains("[CERTIFY OWNERSHIP]"), "{err}");
}

/// Unset, the switch means `fail` in a debug build and `off` in a release
/// build — the certifier is the debug build's own gate now that the corpus
/// ledger is empty. `off` opts out of either.
#[test]
fn a_debug_build_certifies_by_default_and_off_opts_out() {
    let (ok, err) = certify_with("default", GREETER, &[("ALMIDE_CAPTURE_MOVE_OFF", "1"), ("", "ALMIDE_CERTIFY_OWNERSHIP")]);
    if cfg!(debug_assertions) {
        assert!(!ok, "a debug build must fail with the switch unset:\n{err}");
        assert!(err.contains("[C3 clone-at-last-use]"), "{err}");
    } else {
        assert!(ok, "a release build must skip the certifier with the switch unset:\n{err}");
    }
    let (ok, err) = certify_with("off", GREETER, &[("ALMIDE_CAPTURE_MOVE_OFF", "1"), ("ALMIDE_CERTIFY_OWNERSHIP", "off")]);
    assert!(ok, "`off` must build the violating program:\n{err}");
    assert!(!err.contains("[CERTIFY OWNERSHIP]"), "{err}");
}

const KEEPER: &str = "fn keep(f: (Int) -> Int) -> (Int) -> Int = f\nfn apply(f: (Int) -> Int, x: Int) -> Int = f(x)\nfn main() -> Unit = println(int.to_string(keep((x) => x + 1)(1) + apply((x) => x * 2, 3)))\n";

#[test]
fn a_borrowed_callable_that_escapes_is_a_c5_violation() {
    // With escape inference ablated every fn-typed param is `&dyn Fn`: `keep`
    // returns its callable, which a borrow cannot do. The certifier names
    // the body before rustc names the lifetime (#2288).
    let (ok, err) = certify_with("c5", KEEPER, &[("ALMIDE_FN_ESCAPE_OFF", "1")]);
    assert!(!ok, "the build must fail under ALMIDE_CERTIFY_OWNERSHIP=fail:\n{err}");
    assert!(err.contains("[C5 closure-escape] keep: param `f`"), "{err}");
}

#[test]
fn a_callable_the_callee_only_calls_certifies_borrowed() {
    // The same program with the inference on: `keep` owns its callable
    // (it escapes), `apply` borrows it, and the call site passes `&|x| …`.
    let (ok, err) = certify("c5-fixed", KEEPER);
    assert!(ok, "the program must certify once escape is inferred:\n{err}");
}

// ── #2410: the in-loop FRESH binder, and a guard on the shared `fresh` notion ──

/// A capture whose variable is bound INSIDE the loop body — rebound every
/// iteration, scope ending with it — so the per-iteration closure moves a
/// DIFFERENT value each time and there is no second move of one value. Both
/// the capture-move rule and the certifier's C3 previously tested a bare
/// `in_loop` and so cloned (and could not report) exactly this shape: #2316.
const LOOP_FRESH_CAPTURE: &str = r#"fn main() -> Unit = {
  var sink = 0
  var i = 0
  while i < 3 {
    let s = "cap" + int.to_string(i)
    let f = (x) => string.len(s) + x
    sink = sink + f(1)
    i = i + 1
  }
  println(int.to_string(sink))
}
"#;

fn emit_rust(tag: &str, src: &str, env: &[(&str, &str)]) -> String {
    let dir = std::env::temp_dir().join(format!("almide-emit-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("prog.almd");
    std::fs::write(&file, src).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.arg(&file).arg("--target").arg("rust");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("almide");
    assert!(out.status.success(), "emit failed: {}", String::from_utf8_lossy(&out.stderr));
    let _ = std::fs::remove_dir_all(Path::new(&dir));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// THE INDEPENDENCE GUARD (#2410). `loop_fresh_union` is deliberately ONE
/// definition consulted by both the capture-move rule and the certifier, so a
/// wrong `loop_fresh_vars` would be agreed upon by both and reported by
/// neither. This test does not ask the certifier anything: it reads the
/// EMITTED RUST and requires the capture bind to be a move. If the fresh set
/// ever comes back empty, the bind reverts to `s.clone()` and this fails —
/// which is the one check the shared notion cannot talk its way out of.
#[test]
fn a_loop_fresh_capture_binds_by_move_in_the_emitted_rust() {
    let on = emit_rust("lf-on", LOOP_FRESH_CAPTURE, &[]);
    let moved = on.contains("let __cap_2: String = s;");
    let cloned = on.contains("let __cap_2: String = s.clone();");
    assert!(moved && !cloned, "the loop-fresh capture must bind by MOVE (moved={moved} cloned={cloned})");
}

/// The sensitivity half: with the capture-move rule ablated the pass emits the
/// clone, and the widened C3 must SEE it. Before #2410 the certifier abstained
/// from every in-loop clone, so this violation was unreportable — the ledger
/// could not have caught #2316 before its fix or a recurrence after it.
#[test]
fn an_ablated_loop_fresh_capture_is_a_c3_violation() {
    let (ok, err) = certify_with("lf-c3", LOOP_FRESH_CAPTURE, &[("ALMIDE_CAPTURE_MOVE_OFF", "1")]);
    assert!(!ok, "the build must fail under ALMIDE_CERTIFY_OWNERSHIP=fail:\n{err}");
    assert!(err.contains("[C3 clone-at-last-use] main: `s: String`"), "{err}");
}

/// …and with the rule live, the same program is clean: the pass moves and the
/// certifier agrees. Paired with the ablation above, the two directions show
/// the check is conditional rather than always-on.
#[test]
fn a_live_loop_fresh_capture_certifies_clean() {
    let (ok, err) = certify("lf-clean", LOOP_FRESH_CAPTURE);
    assert!(ok, "a moved loop-fresh capture must certify clean:\n{err}");
}

/// The nesting case that a naive union gets wrong (#2410, caught by the gate
/// tools in CI after a spec-only sweep reported clean). `base` is bound in the
/// OUTER loop body — fresh for that loop — but its last occurrence sits inside
/// an INNER loop, which repeats, so that occurrence is NOT the dynamically last
/// one and the clone is load-bearing. A binder therefore qualifies only if no
/// loop contains an occurrence of it without also binding it.
///
/// `outer_loop_variable_used_in_an_inner_loop_still_clones` states the same
/// rule for the clone pass and passed throughout, because it reads the emitted
/// code rather than asking the certifier — which is exactly why it did not
/// catch the certifier disagreeing with it.
const OUTER_BOUND_USED_IN_INNER_LOOP: &str = r#"fn main() -> Unit = {
  var out: List[String] = []
  let names: List[String] = ["a", "b"]
  let tags: List[String] = ["x", "y"]
  for n in names {
    let base = n + "!"
    for t in tags {
      out = out + [base + t]
    }
  }
  println(int.to_string(list.len(out)))
}
"#;

#[test]
fn a_binder_used_inside_a_nested_loop_is_not_judged_fresh() {
    let (ok, err) = certify("nested-fresh", OUTER_BOUND_USED_IN_INNER_LOOP);
    assert!(ok, "an outer-loop binder used in an inner loop must NOT be reported: its clone is load-bearing\n{err}");
}
