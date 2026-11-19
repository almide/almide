//! bounded.shape-rules (#4855): the @bounded allocation (E071), sized
//! construction (E075) and Float computation (E077) rules judge a stdlib
//! member from its types, so a member no table names is judged too. Each case
//! is a fresh @bounded fn calling a member the old name tables never listed;
//! the control is the same fn shape on Int-only members, which passes.
use std::process::Command;

#[path = "support/novel.rs"]
mod novel;

fn check(prog: &str) -> (bool, String) {
    let td = tempfile::TempDir::new().unwrap();
    let f = td.path().join("main.almd");
    std::fs::write(&f, prog).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_almide")).arg("check").arg(&f).current_dir(td.path()).output().expect("almide runs");
    (o.status.success(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
}

#[test]
fn novel_unlisted_members_are_judged_by_their_types() {
    let g = novel::ident("shape");
    let cases = [
        // a String built per iteration of a counted loop
        ("E071", format!("@bounded\nfn {g}() -> Int = {{\n  var n = 0\n  for i in 0..<4 {{ n = n + string.len(int.to_string(i)) }}\n  n\n}}\n\nfn main() -> Unit = println(\"${{{g}()}}\")\n")),
        // a List whose length is a run-time Int
        ("E075", format!("@bounded\nfn {g}(n: Int) -> Int = list.len(list.range_step(0, n, 1))\n\nfn main() -> Unit = println(\"${{{g}(3)}}\")\n")),
        // a Float computed by a member call
        ("E077", format!("@bounded\nfn {g}(x: Float) -> Float = float.abs(x)\n\nfn main() -> Unit = println(float.to_string({g}(1.5)))\n")),
    ];
    for (code, prog) in &cases {
        let (ok, out) = check(prog);
        assert!(!ok && out.contains(code), "an unlisted member was admitted ({code} expected):\n{prog}\n{out}");
    }
    let control = format!("@bounded\nfn {g}(n: Int) -> Int = {{\n  var t = 0\n  for i in 0..<4 {{ t = t + int.abs(i - n) }}\n  t\n}}\n\nfn main() -> Unit = println(\"${{{g}(2)}}\")\n");
    let (ok, out) = check(&control);
    assert!(ok, "control: Int-only members in a counted loop pass:\n{control}\n{out}");
    // #4871: the same rules reach through a callee
    novel_a_callee_allocation_is_followed_by_reach();
}

/// #4871: E071 follows a callee by reach. A fresh @bounded helper that
/// allocates (a list literal, an interpolated String, or through a second
/// fresh helper) and is called inside a counted loop is refused, under
/// `@bounded` and under `--profile critical`. The controls: an Int-only
/// helper in the loop, and the allocating helper called outside the loop.
#[test]
fn novel_a_callee_allocation_is_followed_by_reach() {
    let g = novel::ident("alloc");
    let h = novel::ident("relay");
    let caller = |callee: &str| format!("@bounded\nfn drive() -> Int = {{\n  var n = 0\n  for i in 0..<4 {{ n = n + {callee}(i) }}\n  n\n}}\n\neffect fn main() -> Unit = println(\"${{drive()}}\")\n");
    let cases = [
        // the helper builds a List
        format!("@bounded\nfn {g}(i: Int) -> Int = list.len([i, i])\n\n{}", caller(&g)),
        // the helper builds a String
        format!("@bounded\nfn {g}(i: Int) -> Int = string.len(\"${{i}}!\")\n\n{}", caller(&g)),
        // two hops: the loop calls h, h calls the allocating g
        format!("@bounded\nfn {g}(i: Int) -> Int = list.len([i])\n\n@bounded\nfn {h}(i: Int) -> Int = {g}(i) + 1\n\n{}", caller(&h)),
    ];
    for prog in &cases {
        let (ok, out) = check(prog);
        assert!(!ok && out.contains("E071"), "an allocating callee in a counted loop was admitted:\n{prog}\n{out}");
    }
    // --profile critical: the same shape with no attribute
    let crit = format!("fn {g}(i: Int) -> Int = list.len([i, i])\n\nfn drive() -> Int = {{\n  var n = 0\n  for i in 0..<4 {{ n = n + {g}(i) }}\n  n\n}}\n\neffect fn main() -> Unit = println(\"${{drive()}}\")\n");
    let (ok, out) = check_critical(&crit);
    assert!(!ok && out.contains("E071"), "critical admitted an allocating callee in a counted loop:\n{crit}\n{out}");
    // controls
    let int_only = format!("@bounded\nfn {g}(i: Int) -> Int = int.abs(i - 2) * 3\n\n{}", caller(&g));
    let (ok, out) = check(&int_only);
    assert!(ok, "control: an Int-only callee in a counted loop passes:\n{int_only}\n{out}");
    let outside = format!("@bounded\nfn {g}(i: Int) -> Int = list.len([i, i])\n\n@bounded\nfn drive() -> Int = {{\n  let base = {g}(1)\n  var n = base\n  for i in 0..<4 {{ n = n + i }}\n  n\n}}\n\neffect fn main() -> Unit = println(\"${{drive()}}\")\n");
    let (ok, out) = check(&outside);
    assert!(ok, "control: an allocating callee outside the loop passes:\n{outside}\n{out}");
}

fn check_critical(prog: &str) -> (bool, String) {
    let td = tempfile::TempDir::new().unwrap();
    let f = td.path().join("main.almd");
    std::fs::write(&f, prog).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_almide")).arg("check").arg(&f).args(["--profile", "critical"]).current_dir(td.path()).output().expect("almide runs");
    (o.status.success(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
}
