//! #4196: three stdlib functions that take attacker-shaped input ran in
//! quadratic time.
//!
//! - `url.parse_query` (and `http.query_params`, which is it, C-404) copied the
//!   remaining segments with `list.drop(parts, 1)` and prepended to its
//!   accumulator at every pair: 320,000 pairs took 27 s native, > 100 s wasm.
//! - `string.from_utf16` prepended `[c] + acc` per code unit: 640,000 units took
//!   24.9 s native, 12.8 s wasm.
//! - `string.sort_with` merged String characters through `acc + [y]`, a full
//!   copy per step on wasm: 160,000 characters took 45 s there (native 0.1 s).
//!
//! The wasm leg is pinned by wasmtime's FUEL (one unit per wasm instruction),
//! not by a clock, at n and 2n (the 2n budget is twice the n budget, so a
//! super-linear body fails the larger cell first): each budget is about three
//! times what the linear body spends there (one call measured 248M / 496M for
//! parse_query, 26M / 52M for from_utf16, 584M / 1232M for sort_with at
//! 40,000 / 80,000; the parse_query and from_utf16 probes make two calls), and
//! the quadratic bodies spent 51G, 3.3G and 54G per call already at 40,000. The native leg,
//! which has no fuel, gets a wall-clock bound at the sizes the issue measured,
//! a bound the quadratic bodies exceeded several times over. Every leg's answer
//! is differed against native, so the faster body is the same function.
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

fn has_wasmtime() -> bool {
    let ok = Command::new("wasmtime").arg("--version").output().is_ok();
    if !ok {
        assert!(
            std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(),
            "ALMIDE_EXPECT_TOOLS: wasmtime is not on PATH"
        );
        eprintln!("skipping: wasmtime is not on PATH");
    }
    ok
}

fn parse_query_src(n: usize) -> String {
    format!(
        r#"import url
fn main() -> Unit = {{
  match url.parse_query(string.repeat("a=1&", {n}) + "k=v%41&b") {{
    ok(ps) => println("${{list.len(ps)}} ${{list.last(ps) ?? ("", "")}} ${{list.first(ps) ?? ("", "")}}"),
    err(e) => println(e),
  }}
  match url.parse_query(string.repeat("a=1&", {n}) + "x=%zz&y=%qq") {{
    ok(ps) => println(int.to_string(list.len(ps))),
    err(e) => println(e),
  }}
}}
"#
    )
}

fn from_utf16_src(n: usize) -> String {
    format!(
        r#"fn main() -> Unit = {{
  match string.from_utf16(list.repeat(97, {n}) + [55357, 56832]) {{
    ok(s) => println("${{string.len(s)}} ${{string.slice(s, {n} - 1, {n} + 1)}}"),
    err(e) => println(e),
  }}
  match string.from_utf16(list.repeat(98, {n}) + [56832]) {{
    ok(s) => println(int.to_string(string.len(s))),
    err(e) => println(e),
  }}
}}
"#
    )
}

fn sort_with_src(n: usize) -> String {
    let half = n / 2;
    format!(
        r#"fn main() -> Unit = {{
  let r = string.sort_with(string.repeat("ba", {half}), (a, b) => string.compare(a, b))
  println("${{list.len(r)}} ${{list.first(r) ?? ""}} ${{list.last(r) ?? ""}} ${{list.get(r, {half} - 1) ?? ""}} ${{list.get(r, {half}) ?? ""}}")
}}
"#
    )
}

fn almide(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn almide")
}

/// Native answer, and how long the built binary took to produce it.
fn native(dir: &Path, src: &str) -> (String, Duration) {
    std::fs::write(dir.join("probe.almd"), src).expect("write");
    let b = almide(dir, &["build", "probe.almd", "-o", "probe.bin"]);
    assert!(
        b.status.success(),
        "native build failed:\n{}",
        String::from_utf8_lossy(&b.stderr)
    );
    let t = Instant::now();
    let o = Command::new(dir.join("probe.bin"))
        .output()
        .expect("spawn native");
    let took = t.elapsed();
    assert!(
        o.status.success(),
        "native run failed:\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    (String::from_utf8(o.stdout).expect("utf8"), took)
}

/// The stock wasm artifact's answer under a fuel budget.
fn stock_fueled(dir: &Path, src: &str, fuel: u64) -> Result<String, String> {
    std::fs::write(dir.join("probe.almd"), src).expect("write");
    let b = almide(
        dir,
        &[
            "build",
            "probe.almd",
            "--target",
            "wasm",
            "-o",
            "probe.wasm",
        ],
    );
    assert!(
        b.status.success(),
        "wasm build failed:\n{}",
        String::from_utf8_lossy(&b.stderr)
    );
    let o = Command::new("wasmtime")
        .args(["run", "--dir=.", "-W"])
        .arg(format!("fuel={fuel}"))
        .arg("probe.wasm")
        .current_dir(dir)
        .output()
        .expect("spawn wasmtime");
    if o.status.success() {
        Ok(String::from_utf8(o.stdout).expect("utf8"))
    } else {
        Err(String::from_utf8_lossy(&o.stderr)
            .chars()
            .take(300)
            .collect())
    }
}

fn parse_query_want(n: usize) -> String {
    format!(
        "{} (\"b\", \"\") (\"a\", \"1\")\nurl.parse_query: invalid percent escape\n",
        n + 2
    )
}

fn from_utf16_want(n: usize) -> String {
    format!(
        "{} a😀\nstring.from_utf16: unpaired surrogate at {n}\n",
        n + 1
    )
}

fn sort_with_want(n: usize) -> String {
    format!("{n} a b a b\n")
}

type Cell = (&'static str, fn(usize) -> String, fn(usize) -> String);
const PARSE_QUERY: Cell = ("url.parse_query", parse_query_src, parse_query_want);
const FROM_UTF16: Cell = ("string.from_utf16", from_utf16_src, from_utf16_want);
const SORT_WITH: Cell = ("string.sort_with", sort_with_src, sort_with_want);

/// The wasm cells at n and 2n, each under its budget, each equal to native.
fn wasm_doubling((name, src, want): Cell, n: usize, fuel_n: u64) {
    if !has_wasmtime() {
        return;
    }
    let dir = tempfile::tempdir().expect("scratch");
    for (size, fuel) in [(n, fuel_n), (2 * n, fuel_n * 2)] {
        let program = src(size);
        let (native_out, _) = native(dir.path(), &program);
        assert_eq!(native_out, want(size), "{name} native answer at {size}");
        match stock_fueled(dir.path(), &program, fuel) {
            Ok(got) => assert_eq!(got, native_out, "{name} wasm differs from native at {size}"),
            Err(e) => panic!("{name} at {size} ran out of its {fuel} fuel budget on wasm — quadratic again?\n{e}"),
        }
    }
}

#[test]
fn url_parse_query_is_linear_on_wasm() {
    wasm_doubling(PARSE_QUERY, 40_000, 1_500_000_000);
}

#[test]
fn string_from_utf16_is_linear_on_wasm() {
    wasm_doubling(FROM_UTF16, 40_000, 160_000_000);
}

#[test]
fn string_sort_with_is_n_log_n_on_wasm() {
    wasm_doubling(SORT_WITH, 40_000, 1_800_000_000);
}

#[test]
fn native_bodies_stay_fast_at_the_issue_sizes() {
    let dir = tempfile::tempdir().expect("scratch");
    // The issue's sizes: the quadratic bodies took 27 s and 24.9 s here
    // (native sort_with was already fast and must stay so).
    for ((name, src, want), size) in [
        (PARSE_QUERY, 320_000),
        (FROM_UTF16, 640_000),
        (SORT_WITH, 640_000),
    ] {
        let (got, took) = native(dir.path(), &src(size));
        assert_eq!(got, want(size), "{name} native answer");
        assert!(
            took < Duration::from_secs(8),
            "{name} took {took:?} on native at {size} — quadratic again?"
        );
    }
}
