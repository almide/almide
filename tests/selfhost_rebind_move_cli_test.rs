//! `x = f(x, …)` hands the dying `x` to `f` on the PRODUCT route: `almide run
//! --target wasm`, the path the stdlib inspection measures on GCE.
//!
//! The in-process gate (crates/almide-wasm/tests/http_status_rebind.rs) ran
//! 200 steps and measured 3 B/step, while the CLI copied the whole list every
//! step. Both routes lower the same IR, so the route was not the cause: the loop
//! bound was. A counted `while i < LIT` with LIT >= 1024 goes through the
//! unrolled fast lane (unroll.rs), which lowered its body statement by statement
//! and skipped the block walk that finds a rebind's move-in. So `x = st(x, c)`
//! shared `x` into the call, the callee met refcount 2, and its `list.set`
//! copied. These probes run through the binary at both a rolled and an unrolled
//! bound, with the other argument a call (`a1()`), the shape the inspection
//! passes.
use std::process::Command;

fn almide() -> Command {
    Command::new(env!("CARGO_BIN_EXE_almide"))
}

/// stdout and the bytes the module requested from `$alloc`.
fn run_counted(src: &str) -> (String, u64) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("rebind.almd");
    std::fs::write(&file, src).expect("source");
    let out = almide()
        .arg("run")
        .arg(&file)
        .args(["--target", "wasm"])
        .env("ALMIDE_WASM_ALLOC_COUNT", "1")
        .output()
        .expect("wasm run");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(out.status.success(), "{stderr}");
    let field = |k: &str| -> u64 {
        stderr
            .split_whitespace()
            .find_map(|w| w.strip_prefix(k)?.parse().ok())
            .unwrap_or_else(|| panic!("no {k} in {stderr}"))
    };
    assert_eq!(field("live="), 0, "{stderr}");
    (String::from_utf8_lossy(&out.stdout).to_string(), field("bytes="))
}

fn user_fn(steps: u32) -> String {
    format!(
        "fn a1() -> Int = 201

fn st(r: List[String], c: Int) -> List[String] = list.set(r, 0, int.to_string(c))

effect fn main() -> Unit = {{
  var x = list.range(0, 2000) |> list.map((i) => \"k${{i}}\")
  var i = 0
  while i < {steps} {{
    x = st(x, a1())
    i = i + 1
  }}
  println(\"${{list.get(x, 0) ?? \"\"}} ${{list.len(x)}}\")
}}
"
    )
}

fn http_status(steps: u32) -> String {
    format!(
        "import http

fn a1() -> Int = 201

effect fn main() -> Unit = {{
  var x = http.with_headers(200, \"b\", list.range(0, 2000) |> list.map((i) => (\"k${{i}}\", \"v\")))
  var i = 0
  while i < {steps} {{
    x = http.set_status(x, a1())
    i = i + 1
  }}
  println(\"${{http.status(x)}} ${{list.len(http.headers(x))}}\")
}}
"
    )
}

/// Bytes per step over the zero-step program; a copy of the 2000-slot list
/// is 8000 bytes at least, a fresh status string a handful.
fn per_step(shape: fn(u32) -> String, steps: u32, want: &str, base_want: &str) -> u64 {
    let (out0, base) = run_counted(&shape(0));
    assert_eq!(out0, base_want);
    let (out, looped) = run_counted(&shape(steps));
    assert_eq!(out, want);
    looped.saturating_sub(base) / u64::from(steps)
}

#[test]
fn a_user_fn_rebind_moves_the_dying_list_at_rolled_and_unrolled_bounds() {
    for steps in [1000, 4096] {
        let b = per_step(user_fn, steps, "201 2000\n", "k0 2000\n");
        assert!(b < 256, "{b} bytes per `x = st(x, a1())` step at bound {steps} — the step copies the list");
    }
}

#[test]
fn an_http_status_rebind_moves_the_dying_response_at_rolled_and_unrolled_bounds() {
    for steps in [1000, 4096] {
        let b = per_step(http_status, steps, "201 2000\n", "200 2000\n");
        assert!(b < 256, "{b} bytes per `x = http.set_status(x, a1())` step at bound {steps} — the step copies the headers");
    }
}

/// Value semantics inside the unrolled lane: an alias taken before a rebind
/// keeps the old status, on both legs.
const ALIAS: &str = r#"import http

fn a1() -> Int = 201

effect fn main() -> Unit = {
  var x = http.with_headers(200, "b", [("k", "v")])
  let old = x
  var bad = 0
  var i = 0
  while i < 2048 {
    let before = x
    x = http.set_status(x, a1() + i)
    if http.status(before) != (if i == 0 then 200 else 200 + i) then bad = bad + 1 else ()
    i = i + 1
  }
  println("${bad} ${http.status(old)} ${http.status(x)} ${http.header(x, "k") ?? ""}")
}
"#;

#[test]
fn an_alias_kept_across_unrolled_rebinds_keeps_its_status_on_both_legs() {
    let want = "0 200 2248 v\n";
    let (wasm, _) = run_counted(ALIAS);
    assert_eq!(wasm, want, "wasm");
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("alias.almd");
    std::fs::write(&file, ALIAS).expect("source");
    let native = almide().arg("run").arg(&file).output().expect("native run");
    assert!(native.status.success(), "{}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout), want, "native");
}

/// The TAIL-CALL shape: the dying `x` is a PARAM whose last read is the
/// argument of a consuming call inside the self tail call. The inner call
/// runs before the tail transfer releases the frame, so a shared `x` meets
/// its callee at refcount 2 and `list.set` copies.
fn user_tail(depth: u32) -> String {
    format!(
        "fn a1() -> Int = 201

fn st(r: List[String], c: Int) -> List[String] = list.set(r, 0, int.to_string(c))

fn go(x: List[String], i: Int) -> List[String] = if i <= 0 then x else go(st(x, a1()), i - 1)

effect fn main() -> Unit = {{
  let x0 = list.range(0, 2000) |> list.map((i) => \"k${{i}}\")
  let x = go(x0, {depth})
  println(\"${{list.get(x, 0) ?? \"\"}} ${{list.len(x)}}\")
}}
"
    )
}

fn http_tail(depth: u32) -> String {
    format!(
        "import http

fn a1() -> Int = 201

fn go(x: HttpResponse, i: Int) -> HttpResponse = if i <= 0 then x else go(http.set_status(x, a1()), i - 1)

effect fn main() -> Unit = {{
  let x0: HttpResponse = http.with_headers(200, \"b\", list.range(0, 2000) |> list.map((i) => (\"k${{i}}\", \"v\")))
  let x: HttpResponse = go(x0, {depth})
  println(\"${{http.status(x)}} ${{list.len(http.headers(x))}}\")
}}
"
    )
}

#[test]
fn a_user_fn_tail_call_moves_its_dying_param_into_the_inner_call() {
    for depth in [100, 4096] {
        let b = per_step(user_tail, depth, "201 2000\n", "k0 2000\n");
        assert!(b < 256, "{b} bytes per `go(st(x, a1()), i - 1)` step at depth {depth} — the step copies the list");
    }
}

#[test]
fn an_http_status_tail_call_moves_its_dying_param_into_the_inner_call() {
    for depth in [100, 4096] {
        let b = per_step(http_tail, depth, "201 2000\n", "200 2000\n");
        assert!(b < 256, "{b} bytes per `go(http.set_status(x, a1()), i - 1)` step at depth {depth} — the step copies the headers");
    }
}

/// Value semantics in the tail shape: the caller's `x0`, an alias the frame
/// keeps in a later argument, and an element read before the call all keep
/// the old values, on both legs.
const TAIL_ALIAS: &str = r#"import http

fn a1() -> Int = 201

fn st(r: List[String], c: Int) -> List[String] = list.set(r, 0, int.to_string(c))

fn go(x: HttpResponse, i: Int) -> HttpResponse = if i <= 0 then x else go(http.set_status(x, a1() + i), i - 1)

fn keep(x: List[String], olds: List[String], i: Int) -> List[String] =
  if i <= 0 then x + olds
  else {
    let h = list.get(x, 0) ?? ""
    keep(st(x, i), olds + [h], i - 1)
  }

fn pair(x: List[String], i: Int) -> (List[String], List[String]) = if i <= 0 then (x, x) else (st(x, i), x)

effect fn main() -> Unit = {
  let x0: HttpResponse = http.with_headers(200, "b", [("k", "v")])
  let x = go(x0, 3)
  let l0 = ["a", "b"]
  let l = keep(l0, [], 3)
  let (p, q) = pair(l0, 7)
  println("${http.status(x0)} ${http.status(x)} ${http.header(x, "k") ?? ""} ${l0} ${l} ${p} ${q}")
}
"#;

#[test]
fn a_tail_call_move_keeps_every_alias_on_both_legs() {
    let want = "200 202 v [\"a\", \"b\"] [\"1\", \"b\", \"a\", \"3\", \"2\"] [\"7\", \"b\"] [\"a\", \"b\"]\n";
    let (wasm, _) = run_counted(TAIL_ALIAS);
    assert_eq!(wasm, want, "wasm");
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("tail_alias.almd");
    std::fs::write(&file, TAIL_ALIAS).expect("source");
    let native = almide().arg("run").arg(&file).output().expect("native run");
    assert!(native.status.success(), "{}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout), want, "native");
}
