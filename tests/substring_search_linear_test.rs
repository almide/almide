//! #4206 / #4196: substring search and two bytes rebuilders ran in quadratic
//! time on attacker-shaped input.
//!
//! - `bytes.index_of`, `contains`, `split` compared the needle at every
//!   haystack position on BOTH targets (native `windows(m).position`, the
//!   wasm self-host and the structural `bytes.split` arm): a haystack of
//!   n `a`s and a needle of n/2 `a`s then `b` took 11.9 s native at n = 1.6M
//!   and 5.5 s on the embedded host at 200,000. `bytes.last_index_of`,
//!   `count_matches` and `replace_first` (pure Almide, both targets) compared
//!   a `list.slice` copy per position: 10.5 s native at 400,000.
//! - The string family (`index_of`, `contains`, `count`, `split`,
//!   `split_once`, `replace`, `replace_first`, `last_index_of`) did the same on
//!   wasm only — native uses `str::find` (Two-Way): 5.2 s for
//!   `string.index_of` and for `string.replace` at 200,000 on wasm.
//! - wasm `bytes.sort_with` merged through `acc + [y]` and wasm
//!   `bytes.replace` rebuilt through `acc + slice + to` per match: 2.8 s and
//!   4.9 s at 200,000.
//!
//! Every search is now Knuth–Morris–Pratt (runtime/rs/src/bytes.rs,
//! stdlib/bytes_core.almd, stdlib/string_search.almd,
//! crates/almide-wasm/src/kmp.rs); the bytes.almd functions go through
//! `bytes.index_of` / `bytes.split`.
//!
//! The wasm leg is pinned by wasmtime's FUEL at n and 2n (the 2n budget is
//! twice the n budget, so a super-linear body fails the larger cell first):
//! each budget is about three times what the linear body spends there
//! (measured at 40,000 / 80,000: bytes search 48M / 96M, bytes.replace 21M /
//! 43M, bytes.sort_with 344M / 730M, string search 53M / 107M). The native
//! leg gets a wall-clock bound at the issue's sizes. Every wasm answer is
//! differed against native, and a small-alphabet exhaustive program is run
//! on native, wasm and the interp oracle, which must agree (or abstain).
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

fn bytes_search_src(n: usize) -> String {
    let h = n / 2;
    format!(
        r#"fn main() -> Unit = {{
  let hay = bytes.from_string(string.repeat("a", {n}))
  let adv = bytes.from_string(string.repeat("a", {h}) + "b")
  let back = bytes.from_string("b" + string.repeat("a", {h}))
  let tail = bytes.from_string(string.repeat("a", {h}))
  println("${{bytes.index_of(hay, adv)}} ${{bytes.contains(hay, adv)}} ${{list.len(bytes.split(hay, adv))}} ${{bytes.count_matches(hay, adv)}}")
  println("${{bytes.last_index_of(hay, back)}} ${{bytes.last_index_of(hay, adv)}} ${{bytes.last_index_of(hay, tail)}} ${{bytes.len(bytes.replace_first(hay, adv, tail))}}")
}}
"#
    )
}

fn bytes_search_want(n: usize) -> String {
    format!("none false 1 0\nnone none some({}) {n}\n", n / 2)
}

fn bytes_replace_src(n: usize) -> String {
    let last = 2 * n - 1;
    format!(
        r#"fn main() -> Unit = {{
  let r = bytes.replace(bytes.from_string(string.repeat("a", {n})), bytes.from_string("a"), bytes.from_string("bc"))
  println("${{bytes.len(r)}} ${{bytes.get(r, 0)}} ${{bytes.get(r, {last})}}")
}}
"#
    )
}

fn bytes_replace_want(n: usize) -> String {
    format!("{} some(98) some(99)\n", 2 * n)
}

fn bytes_sort_with_src(n: usize) -> String {
    let h = n / 2;
    format!(
        r#"fn main() -> Unit = {{
  let r = bytes.sort_with(bytes.from_string(string.repeat("ab", {h})), (a, b) => b - a)
  println("${{list.len(r)}} ${{list.first(r) ?? 0}} ${{list.last(r) ?? 0}} ${{list.get(r, {h} - 1) ?? 0}} ${{list.get(r, {h}) ?? 0}}")
}}
"#
    )
}

fn bytes_sort_with_want(n: usize) -> String {
    format!("{n} 98 97 98 97\n")
}

fn string_search_src(n: usize) -> String {
    let h = n / 2;
    format!(
        r#"fn main() -> Unit = {{
  let hay = string.repeat("a", {n})
  let adv = string.repeat("a", {h}) + "b"
  println("${{string.index_of(hay, adv)}} ${{string.contains(hay, adv)}} ${{string.count_matches(hay, adv)}} ${{list.len(string.split(hay, adv))}} ${{string.last_index_of(hay, adv)}} ${{string.last_index_of(hay, string.repeat("a", {h}))}}")
  match string.split_once(hay, adv) {{
    some((a, b)) => println(a + b),
    none => println("none"),
  }}
  let (x, y) = string.split_once("b" + hay + "b", adv) ?? ("", "")
  println("${{string.len(x)}} ${{string.len(y)}} ${{string.len(string.replace(hay, adv, "c"))}} ${{string.len(string.replace_first(hay, adv, "c"))}}")
}}
"#
    )
}

fn string_search_want(n: usize) -> String {
    let h = n / 2;
    format!("none false 0 1 none some({h})\nnone\n{} 0 {n} {n}\n", h + 1)
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

/// The stock wasm artifact's answer, under a fuel budget when one is given.
fn stock(dir: &Path, src: &str, fuel: Option<u64>) -> Result<String, String> {
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
    let mut cmd = Command::new("wasmtime");
    cmd.args(["run", "--dir=."]);
    if let Some(f) = fuel {
        cmd.arg("-W").arg(format!("fuel={f}"));
    }
    let o = cmd
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

type Cell = (&'static str, fn(usize) -> String, fn(usize) -> String);
const BYTES_SEARCH: Cell = ("bytes search", bytes_search_src, bytes_search_want);
const BYTES_REPLACE: Cell = ("bytes.replace", bytes_replace_src, bytes_replace_want);
const BYTES_SORT_WITH: Cell = ("bytes.sort_with", bytes_sort_with_src, bytes_sort_with_want);
const STRING_SEARCH: Cell = ("string search", string_search_src, string_search_want);

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
        match stock(dir.path(), &program, Some(fuel)) {
            Ok(got) => assert_eq!(got, native_out, "{name} wasm differs from native at {size}"),
            Err(e) => panic!("{name} at {size} ran out of its {fuel} fuel budget on wasm — quadratic again?\n{e}"),
        }
    }
}

#[test]
fn bytes_search_is_linear_on_wasm() {
    wasm_doubling(BYTES_SEARCH, 40_000, 150_000_000);
}

#[test]
fn bytes_replace_is_linear_on_wasm() {
    wasm_doubling(BYTES_REPLACE, 40_000, 70_000_000);
}

#[test]
fn bytes_sort_with_is_n_log_n_on_wasm() {
    wasm_doubling(BYTES_SORT_WITH, 40_000, 1_100_000_000);
}

#[test]
fn string_search_is_linear_on_wasm() {
    wasm_doubling(STRING_SEARCH, 40_000, 160_000_000);
}

#[test]
fn native_bodies_stay_fast_at_the_issue_sizes() {
    let dir = tempfile::tempdir().expect("scratch");
    // The quadratic native bytes search took 3.2 s per call at 800,000 (this
    // cell makes seven), last_index_of and count_matches 10 s at 400,000.
    for ((name, src, want), size) in [
        (BYTES_SEARCH, 800_000),
        (BYTES_REPLACE, 400_000),
        (BYTES_SORT_WITH, 400_000),
        (STRING_SEARCH, 400_000),
    ] {
        let (got, took) = native(dir.path(), &src(size));
        assert_eq!(got, want(size), "{name} native answer");
        assert!(
            took < Duration::from_secs(8),
            "{name} took {took:?} on native at {size} — quadratic again?"
        );
    }
}

/// Every haystack and needle up to `len` characters over `alpha`, through
/// every search function, one line per pair.
fn exhaustive_src(len: usize, alpha: &str) -> String {
    format!(
        r#"fn level(k: Int, alpha: List[String]) -> List[String] =
  if k == 0 then [""]
  else level(k - 1, alpha) |> list.flat_map((x) => alpha |> list.map((c) => x + c))

fn upto(n: Int, alpha: List[String]) -> List[String] =
  if n < 0 then [] else upto(n - 1, alpha) + level(n, alpha)

fn so(h: String, p: String) -> String = match string.split_once(h, p) {{
  some((a, b)) => a + "|" + b,
  none => "-",
}}

fn s(b: Bytes) -> String = bytes.to_string_lossy(b)

fn main() -> Unit = {{
  let xs = upto({len}, [{alpha}])
  for h in xs {{
    let hb = bytes.from_string(h)
    for p in xs {{
      let pb = bytes.from_string(p)
      println("${{h}}/${{p}} ${{string.index_of(h, p)}} ${{string.last_index_of(h, p)}} ${{string.contains(h, p)}} ${{string.count_matches(h, p)}} ${{string.split(h, p)}} ${{string.split_once(h, p)}} ${{so(h, p)}} ${{string.replace(h, p, "X")}} ${{string.replace_first(h, p, "YZ")}}")
      let parts = bytes.split(hb, pb) |> list.map((x) => s(x))
      println("${{bytes.index_of(hb, pb)}} ${{bytes.last_index_of(hb, pb)}} ${{bytes.contains(hb, pb)}} ${{bytes.count_matches(hb, pb)}} ${{parts}} ${{s(bytes.replace(hb, pb, bytes.from_string("X")))}} ${{s(bytes.replace_first(hb, pb, bytes.from_string("YZ")))}}")
    }}
  }}
}}
"#
    )
}

/// Native, wasm and the interp oracle give one answer for every small input
/// (the interp may abstain, never dissent): the linear searches return what
/// the position-by-position ones returned — the old outputs were differed
/// byte-for-byte against these on both targets over {a,b}^<=6 when the
/// change landed.
#[test]
fn every_small_input_agrees_on_native_wasm_and_the_interp() {
    let dir = tempfile::tempdir().expect("scratch");
    // The {a,b} program must run whole on the interp; over "é" it abstains
    // part-way (a String block read-back it declines), which is allowed.
    for (len, alpha, interp_whole) in [(4, r#""a", "b""#, true), (3, r#""a", "é""#, false)] {
        let program = exhaustive_src(len, alpha);
        let (native_out, _) = native(dir.path(), &program);
        if has_wasmtime() {
            let wasm_out = stock(dir.path(), &program, None).expect("wasm run");
            assert_eq!(
                wasm_out, native_out,
                "wasm differs from native over {alpha}"
            );
        }
        let ir = almide::wasm_leg::lower_to_ir("spec/wasm_cross/probe.almd", &program)
            .expect("front failed");
        let out = almide_interp::Interpreter::new(&ir).run_main();
        match out.status {
            almide_interp::RunStatus::Ok => {
                assert_eq!(out.stdout, native_out, "the interp differs over {alpha}")
            }
            ref st => {
                assert!(!interp_whole, "the interp abstained over {alpha}: {st:?}");
                assert!(
                    native_out.starts_with(&out.stdout),
                    "the interp printed a wrong prefix over {alpha}"
                )
            }
        }
    }
}
