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
}
