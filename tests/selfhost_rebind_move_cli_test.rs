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
  var x = http.with_headers(200, \"b\", list.range(0, 2000) |> list.fold(map.new(), (m, i) => map.set(m, \"k${{i}}\", \"v\")))
  var i = 0
  while i < {steps} {{
    x = http.status(x, a1())
    i = i + 1
  }}
  println(\"${{http.status_code(x)}} ${{map.len(http.headers(x))}}\")
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
        assert!(b < 256, "{b} bytes per `x = http.status(x, a1())` step at bound {steps} — the step copies the headers");
    }
}

/// Value semantics inside the unrolled lane: an alias taken before a rebind
/// keeps the old status, on both legs.
const ALIAS: &str = r#"import http

fn a1() -> Int = 201

effect fn main() -> Unit = {
  var x = http.with_headers(200, "b", map.set(map.new(), "k", "v"))
  let old = x
  var bad = 0
  var i = 0
  while i < 2048 {
    let before = x
    x = http.status(x, a1() + i)
    if http.status_code(before) != (if i == 0 then 200 else 200 + i) then bad = bad + 1 else ()
    i = i + 1
  }
  println("${bad} ${http.status_code(old)} ${http.status_code(x)} ${http.get_header(x, "k") ?? ""}")
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
