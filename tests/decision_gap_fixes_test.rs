//! Novel-item tests for the checker gaps #4859-#4862 (proofs/decision-sites.toml
//! rows effect.e011-closure-mutation, ownership.e087-scoped-callee,
//! input.e110-input-sink, ownership.e119-lent-region-escape): each builds a
//! fresh program from tests/support/novel.rs names and expects its refusal.
use std::process::Command;

#[path = "support/novel.rs"]
mod novel;

fn check(prog: &str) -> String {
    let td = tempfile::TempDir::new().unwrap();
    let f = td.path().join("main.almd");
    std::fs::write(&f, prog).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_almide")).arg("check").arg(&f).current_dir(td.path()).output().expect("almide runs");
    format!("exit {:?}\n{}{}", o.status.code(), String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn refused(code: &str, prog: &str) {
    let out = check(prog);
    assert!(out.contains(&format!("[{code}]")), "a fresh program was admitted ({code} expected):\n{prog}\n{out}");
}

/// #4859: a closure in a pure fn that changes a top-level `var`.
#[test]
fn novel_e011_a_top_level_var_changed_in_a_closure() {
    let (g, apply, tally) = (novel::ident("total"), novel::ident("apply"), novel::ident("tally"));
    refused("E011", &format!(
        "var {g} = 0\n\nfn {apply}(ws: List[Int], f: (Int) -> Int) -> Int = list.fold(ws, 0, (a, w) => a + f(w))\n\nfn {tally}(ws: List[Int]) -> Int = {apply}(ws, (w) => {{ {g} = {g} + w; w }})\n\neffect fn main() -> Unit = println(int.to_string({tally}([1, 2, 3])))\n"
    ));
}

/// #4860: a builtin the scoped walk does not name (`assert_ne`).
#[test]
fn novel_e087_an_unnamed_builtin_in_a_scoped_fn() {
    let f = novel::ident("chk");
    refused("E087", &format!("scoped fn {f}(n: Int) -> Int = {{\n  assert_ne(n, 0)\n  n\n}}\n\neffect fn main() -> Unit = println(int.to_string({f}(3)))\n"));
}

/// #4861: a sent-out type decoded from a request inside a container.
#[test]
fn novel_e110_a_sent_out_type_inside_a_list() {
    let (t, h) = (novel::ident("Acct").replace('_', ""), novel::ident("handle"));
    let t = format!("Z{t}");
    refused("E110", &format!(
        "import http\n\ntype {t}: Codec = {{ name: String, admin: Bool }}\n\neffect fn {h}(req: HttpRequest) -> Int = match http.decode_json[List[{t}]](req) {{\n  ok(xs) => list.len(xs),\n  err(_) => 0,\n}}\n\neffect fn main() -> Unit = println(\"x\")\n"
    ));
}

/// #4862: a lent Region bound through a destructuring `let`, then stored.
#[test]
fn novel_e119_a_region_through_a_destructuring_let() {
    let (keep, stash, a) = (novel::ident("keep"), novel::ident("stash"), novel::ident("a"));
    refused("E119", &format!(
        "import ffi\n\nvar {stash}: Option[Region] = none\n\n@export(c, \"{keep}\")\nfn {keep}(r: Region) -> Int = {{\n  let ({a}, _) = (r, 1)\n  {stash} = some({a})\n  0\n}}\n\neffect fn main() -> Unit = println(\"x\")\n"
    ));
}
