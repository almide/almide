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

/// The profile walk reaches every expression kind (#2296). Its container
/// recursion once ended in a wildcard, so a violation nested in any kind it
/// did not list was never seen: a `while` loop inside a pipe stage, an
/// `ok(..)` / `some(..)` / `err(..)` payload, a tuple index, an `if let`, a
/// spread record, an optional chain or a type-ascribed call argument, a Float
/// operation inside an interpolation hole, and a function composition all
/// passed `--profile critical` clean. Each program here is valid under the
/// normal check (the subset witness) and must be rejected with its code.
#[test]
fn a_violation_is_seen_through_every_expression_kind() {
    const LOOP: &str = "{\n    var i = 0\n    while i < n { i = i + 1 }\n    i\n  }";
    let cases: Vec<(&str, String, &str)> = vec![
        ("pipe stage", format!("fn f(n: Int) -> Int = n |> ((m) => m + {LOOP})\n\nfn main() -> Unit = println(int.to_string(f(3)))\n"), "E070"),
        ("function composition", "fn double(x: Int) -> Int = x * 2\n\nfn f(n: Int) -> Int = {\n  let g = double >> double\n  n\n}\n\nfn main() -> Unit = println(int.to_string(f(3)))\n".to_string(), "E074"),
        ("spread record", format!("type P = {{ x: Int, y: Int }}\n\nfn f(p: P, n: Int) -> P = {{ ...p, x: {LOOP} }}\n\nfn main() -> Unit = println(int.to_string(f({{ x: 1, y: 2 }}, 3).x))\n"), "E070"),
        ("tuple index", "fn f(n: Int) -> Int = ({\n    var i = 0\n    while i < n { i = i + 1 }\n    (i, 0)\n  }).0\n\nfn main() -> Unit = println(int.to_string(f(3)))\n".to_string(), "E070"),
        ("if let", format!("fn f(o: Int?, n: Int) -> Int = if let v = o {{ v + {LOOP} }} else {{ 0 }}\n\nfn main() -> Unit = println(int.to_string(f(some(1), 3)))\n"), "E070"),
        ("optional chain", format!("type P = {{ x: Int, y: Int }}\n\nfn f(n: Int) -> Int? = (some({{ x: {LOOP}, y: 0 }}))?.x\n\nfn main() -> Unit = println(int.to_string(f(3) ?? 0))\n"), "E070"),
        ("some payload", format!("fn f(n: Int) -> Int? = some({LOOP})\n\nfn main() -> Unit = println(int.to_string(f(3) ?? 0))\n"), "E070"),
        ("ok payload", format!("fn f(n: Int) -> Result[Int, String] = ok({LOOP})\n\nfn main() -> Unit = println(int.to_string(f(3) ?? 0))\n"), "E070"),
        ("err payload", format!("fn f(n: Int) -> Result[Int, Int] = err({LOOP})\n\nfn main() -> Unit = println(int.to_string(f(3) ?? 0))\n"), "E070"),
        ("type-ascribed argument", format!("fn f(n: Int) -> String = int.to_string({LOOP}: Int)\n\nfn main() -> Unit = println(f(3))\n"), "E070"),
        ("interpolation hole", "fn f(x: Float) -> String = \"${x * 2.0}\"\n\nfn main() -> Unit = println(f(1.5))\n".to_string(), "E077"),
        ("variant payload", format!("type B = | Holds(Int) | Empty\n\nfn f(n: Int) -> B = Holds({LOOP})\n\nfn main() -> Unit = println(match f(3) {{ Holds(v) => int.to_string(v), Empty => \"-\" }})\n"), "E070"),
    ];
    for (i, (kind, src, code)) in cases.iter().enumerate() {
        let name = format!("kind{i}.almd");
        let (normal, err) = check(src, &name, &[]);
        assert!(normal, "{kind}: the normal check must accept the program (subset witness):\n{err}");
        let (crit, err) = check(src, &name, &["--profile", "critical"]);
        assert!(!crit, "{kind}: --profile critical accepted a violation nested in it");
        assert!(err.contains(code), "{kind}: expected {code}, got:\n{err}");
    }
}

/// A pipe is judged as the call it lowers to, including through a trailing
/// `??` on the stage (ADR-0005): a first-order pure stdlib stage stays
/// admissible, and so does the rest of the chain.
#[test]
fn a_pipe_into_a_first_order_stdlib_member_stays_admissible() {
    let src = "fn head(xs: List[String]) -> String = xs |> list.first ?? \"none\"\n\nfn main() -> Unit = println(head([\"a\"]))\n";
    let (crit, err) = check(src, "pipe_first.almd", &["--profile", "critical"]);
    assert!(crit, "critical rejected a first-order pipe stage:\n{err}");
}

/// A variant constructor application builds a value, as a record literal or a
/// tuple does (ALS-B7, #2297). The profile once classified its `TypeName`
/// callee as an indirect call, so every Critical program that built a variant
/// value was rejected with E074. It is admissible in a function body and
/// inside a counted loop, and its payload stays judged (the "variant payload"
/// row of `a_violation_is_seen_through_every_expression_kind`).
#[test]
fn a_variant_constructor_is_a_value_not_a_call() {
    let src = "type Shape = | Circle(Int) | Dot\n\nfn mk(n: Int) -> Shape = if n > 0 then Circle(n) else Dot\n\nfn count() -> Int = {\n  var c = 0\n  for i in 0..<6 {\n    let s = if i % 2 == 0 then Circle(i) else Dot\n    c = c + (match s { Circle(_) => 1, Dot => 0 })\n  }\n  c\n}\n\nfn main() -> Unit = println(match mk(2) { Circle(r) => int.to_string(r + count()), Dot => \"dot\" })\n";
    let (crit, err) = check(src, "variant_ctor.almd", &["--profile", "critical"]);
    assert!(crit, "critical rejected a variant constructor:\n{err}");
    let (normal, err) = check(src, "variant_ctor.almd", &[]);
    assert!(normal, "normal check rejected the program:\n{err}");
    let payload = "type Box = | Holds(Int) | Empty\n\nfn f(xs: List[Int]) -> Box = Holds(list.fold(xs, 0, (a, x) => a + x))\n\nfn main() -> Unit = println(match f([1]) { Holds(v) => int.to_string(v), Empty => \"-\" })\n";
    let (crit, err) = check(payload, "variant_payload_hof.almd", &["--profile", "critical"]);
    assert!(!crit && err.contains("E074"), "a higher-order call inside a constructor payload must stay E074:\n{err}");
}
