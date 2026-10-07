//! A string or list accumulated through a record field or an interpolation
//! extends in place (#3454).
//!
//! #3404 made `s = s + piece` move `s` into its own concat, so a `var`
//! accumulator is linear. Two neighbouring shapes still copied the whole
//! accumulated value on every step — 50k / 100k / 200k appends took
//! 0.09 / 0.25 / 0.72 s:
//!
//! 1. **Field accumulator**: `b.text = b.text + "ab"` emitted
//!    `AlmideConcat::concat(b.text.clone(), …)`. The overwrite kills the old
//!    field value, so the borrow lowering now spells that one read
//!    `std::mem::take(&mut b.text)` (the take a `mut` record param already
//!    got, #3170) when `b` is a plain `let mut` local. List `+` the same.
//! 2. **Interpolation accumulator**: `s = "${s}ab"` emitted
//!    `format!("{}ab", s)`. When the FIRST piece is the overwritten place and
//!    no later piece mentions it, the clone pass reads it as `s + "ab"` —
//!    the same bytes — so the reassignment move (or the field take) applies.
//!
//! The negative cells pin where the copy must stay: a right side that reads
//! the place twice (`b.text = b.text + b.text`, `s = "${s}+${s}"`), a place
//! that is not the first piece (`s = "<${s}>"`), and a record a closure
//! captures (a shared cell — the take would reach through the cell). If a
//! positive assertion fails, one of the two rules is gone — fix the pass,
//! don't relax the assertion. The perf cell runs N and 4N appends of each
//! shape and requires the time ratio to stay under 5 (copying measured 10+).
//!
//! Skips cleanly when the `almide` binary is unavailable (CI builds it in the
//! build step; locally run `cargo build --release` first).

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn tool_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

const PROGRAM: &str = r#"type Buf = { text: String, xs: List[Int], n: Int }

fn fields(n: Int) -> Buf = {
  var b = Buf { text: "", xs: [], n: 0 }
  for i in 0..<n {
    b.text = b.text + "ab"
    b.xs = b.xs + [i]
    b.text = "${b.text}-${i}"
    b.n = b.n + 1
  }
  b
}

fn doubled(n: Int) -> Buf = {
  var b = Buf { text: "x", xs: [1], n: 0 }
  for _ in 0..<n {
    b.text = b.text + b.text
    b.xs = b.xs + b.xs
    b.text = "${b.text}${b.n}${b.text}"
  }
  b
}

fn interp(n: Int) -> String = {
  var s = ""
  for i in 0..<n {
    s = "${s}[${i}]"
    s = "${s}."
  }
  s
}

fn interp_reread(n: Int) -> String = {
  var s = "a"
  for _ in 0..<n {
    s = "${s}+${s}"
    s = "<${s}>"
  }
  s
}

fn captured(n: Int) -> String = {
  var b = Buf { text: "", xs: [], n: 0 }
  let peek = () => b.n
  for _ in 0..<n {
    b.text = b.text + "c"
  }
  "${b.text}${peek()}"
}

fn keep_old(n: Int) -> (String, String) = {
  var b = Buf { text: "k", xs: [], n: 0 }
  let old = b
  for _ in 0..<n {
    b.text = "${b.text}k"
  }
  (old.text, b.text)
}

effect fn main() -> Unit = {
  let a = fields(4)
  println("${a.text} ${a.xs} ${a.n}")
  let d = doubled(2)
  println("${d.text} ${d.xs}")
  println(interp(3))
  println(interp_reread(2))
  println(captured(3))
  let (o, k) = keep_old(3)
  println("${o} ${k}")
}
"#;

const EXPECTED: &str = "ab-0ab-1ab-2ab-3 [0, 1, 2, 3] 4\n\
xx0xxxx0xx0xx0xxxx0xx [1, 1, 1, 1]\n\
[0].[1].[2].\n\
<<a+a>+<a+a>>\n\
ccc0\n\
k kkkk\n";

/// N appends through a record field (`field`) or an interpolation (`interp`).
const PERF: &str = r#"import env

type Buf = { text: String, n: Int }

fn field(n: Int) -> Int = {
  var b = Buf { text: "", n: 0 }
  for _ in 0..<n {
    b.text = b.text + "ab"
    b.n = b.n + 1
  }
  b.n
}

fn interp(n: Int) -> Int = {
  var s = ""
  for _ in 0..<n {
    s = "${s}ab"
  }
  string.len(s)
}

effect fn main() -> Unit = {
  let args = env.args()
  let n = int.parse(list.get(args, 1) ?? "0") ?? 0
  let r = match list.get(args, 0) ?? "" {
    "field" => field(n),
    _ => interp(n),
  }
  println(int.to_string(r))
}
"#;

fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-accumulate-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn emitted() -> String {
    let dir = scratch("emit");
    let src = dir.join("prog.almd");
    std::fs::write(&src, PROGRAM).unwrap();
    let output = Command::new(almide_bin())
        .args([src.to_str().unwrap(), "--target", "rust"])
        .output()
        .expect("failed to spawn almide");
    std::fs::remove_dir_all(&dir).ok();
    assert!(output.status.success(), "--target rust emit failed:\n{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// The body of `pub fn <name>(` up to the first column-zero `}`.
fn body<'a>(rust: &'a str, name: &str) -> &'a str {
    let needle = format!("pub fn {name}(");
    let start = rust.find(&needle).unwrap_or_else(|| panic!("emitted Rust has no `{needle}`"));
    let rest = &rust[start..];
    let end = rest.find("\n}").map(|i| i + 2).unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn a_field_accumulator_takes_the_field_and_extends_it() {
    if !tool_available() { return; }
    let rust = emitted();
    let fields = body(&rust, "fields");
    assert!(fields.contains("b.text = AlmideConcat::concat(std::mem::take(&mut b.text), \"ab\""),
        "`b.text = b.text + \"ab\"` must take the field:\n{fields}");
    assert!(fields.contains("b.xs = AlmideConcat::concat(std::mem::take(&mut b.xs), vec![i])"),
        "`b.xs = b.xs + [i]` must take the field:\n{fields}");
    assert!(fields.contains("b.text = AlmideConcat::concat(std::mem::take(&mut b.text), format!(\"-{}\", i))"),
        "`b.text = \"${{b.text}}-${{i}}\"` must append to the taken field:\n{fields}");
    assert!(!fields.contains(".clone()"), "nothing in `fields` needs a copy:\n{fields}");
    let keep_old = body(&rust, "keep_old");
    assert!(keep_old.contains("std::mem::take(&mut b.text)"),
        "a copy taken BEFORE the loop does not stop the take:\n{keep_old}");
}

#[test]
fn an_interpolation_led_by_the_accumulator_appends() {
    if !tool_available() { return; }
    let rust = emitted();
    let interp = body(&rust, "interp");
    assert!(interp.contains("s = AlmideConcat::concat(s, format!(\"[{}]\", i))"),
        "`s = \"${{s}}[${{i}}]\"` must move `s` and append:\n{interp}");
    assert!(interp.contains("s = AlmideConcat::concat(s, \".\""),
        "`s = \"${{s}}.\"` must move `s` and append:\n{interp}");
    assert!(!interp.contains("format!(\"{}"), "no interpolation re-formats `s`:\n{interp}");
}

#[test]
fn the_copy_stays_where_the_place_is_read_again() {
    if !tool_available() { return; }
    let rust = emitted();
    let doubled = body(&rust, "doubled");
    assert!(!doubled.contains("std::mem::take"), "a right side reading the field twice must not take it:\n{doubled}");
    assert!(doubled.contains("AlmideConcat::concat(b.text.clone(), b.text.clone())"), "`b.text + b.text` copies:\n{doubled}");
    assert!(doubled.contains("AlmideConcat::concat(b.xs.clone(), b.xs.clone())"), "`b.xs + b.xs` copies:\n{doubled}");
    let reread = body(&rust, "interp_reread");
    assert!(reread.contains("format!(\"{}+{}\", s, s)"), "`\"${{s}}+${{s}}\"` keeps the interpolation:\n{reread}");
    assert!(reread.contains("format!(\"<{}>\", s)"), "`s` not first keeps the interpolation:\n{reread}");
    let captured = body(&rust, "captured");
    assert!(!captured.contains("std::mem::take"), "a record a closure captures is a shared cell, never taken:\n{captured}");
}

#[test]
fn the_program_prints_what_value_semantics_say() {
    if !tool_available() { return; }
    let dir = scratch("run");
    let src = dir.join("prog.almd");
    std::fs::write(&src, PROGRAM).unwrap();
    let output = Command::new(almide_bin()).args(["run", src.to_str().unwrap()]).output().expect("spawn almide run");
    std::fs::remove_dir_all(&dir).ok();
    assert!(output.status.success(), "run failed:\n{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), EXPECTED);
}

/// Best of three wall-clock runs of `bin shape n`, checking the printed count.
fn best_of_three(bin: &Path, shape: &str, n: u64, expect: u64) -> Duration {
    (0..3).map(|_| {
        let t = Instant::now();
        let out = Command::new(bin).args([shape, &n.to_string()]).output().expect("spawn perf binary");
        let dt = t.elapsed();
        assert!(out.status.success(), "{shape} {n} failed:\n{}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), expect.to_string(), "{shape} {n} printed the wrong count");
        dt
    }).min().unwrap()
}

#[test]
fn four_times_the_appends_is_not_sixteen_times_the_time() {
    if !tool_available() { return; }
    let dir = scratch("perf");
    let src = dir.join("perf.almd");
    let bin = dir.join("perf");
    std::fs::write(&src, PERF).unwrap();
    let build = Command::new(almide_bin())
        .args(["build", src.to_str().unwrap(), "-o", bin.to_str().unwrap()])
        .output()
        .expect("spawn almide build");
    assert!(build.status.success(), "build failed:\n{}", String::from_utf8_lossy(&build.stderr));
    // Measured A/B on the emitted Rust, release, best of three: the copying
    // form took 0.25 s → 2.6 s for 100k → 400k appends (ratio 10.2–10.6 for
    // both shapes; 16× in the limit), the in-place form 2.0 ms → 2.5 ms
    // (ratio 1.2 — the process start dominates). A bound of 5 leaves a 2×
    // margin under the copying ratio and 4× over the in-place one.
    let (n, big) = (100_000u64, 400_000u64);
    for (shape, per) in [("field", 1u64), ("interp", 2u64)] {
        let small = best_of_three(&bin, shape, n, n * per);
        let large = best_of_three(&bin, shape, big, big * per);
        let ratio = large.as_secs_f64() / small.as_secs_f64().max(1e-4);
        assert!(ratio < 5.0, "{shape}: {big} appends took {large:?} against {small:?} for {n} — ratio {ratio:.1}, quadratic again");
    }
    std::fs::remove_dir_all(&dir).ok();
}
