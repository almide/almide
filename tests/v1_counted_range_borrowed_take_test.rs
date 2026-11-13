//! #4701: two loop shapes the certified native render ran slower than the
//! standard codegen.
//!
//! 1. `for k in list.range(a, b)` built the list of every index and walked
//!    it (three million slots allocated, filled and read back only to
//!    count). Over bounds no iteration can change it is now the counted loop
//!    `for k in a..<b`, as the standard codegen spells it
//!    (`native_range_loops.rs`); a range too long to be a list still aborts.
//! 2. `fn rd(xs: List[Int], i: Int) = … ok(list.take(xs, i % 8))` took `xs`
//!    over (its by-value `list.take`), so the loop that called it with a list
//!    it kept copied all hundred elements per call where the lending `take`
//!    copies at most seven. A param handed to a by-value call is now taken
//!    over only where every call site hands over its own value
//!    (`native_move_params.rs`).
//!
//! Each program prints one fixed text on the native render (ownership
//! certificate required, rendered by the certified render itself) and on
//! wasm, and the rendered Rust is read for the shape.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-counted-range-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// (stdout, stderr) of `almide run <src>` on `target`, the certificate required.
fn run(src: &Path, target: Option<&str>) -> (String, String) {
    let mut cmd = Command::new(almide());
    cmd.arg("run").arg(src).env("ALMIDE_CERTIFY_OWNERSHIP", "fail");
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("almide");
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// `source` renders on the certified render and prints `expected` natively
/// and on wasm; returns the rendered Rust.
fn prints(tag: &str, source: &str, expected: &str) -> String {
    let rust = match almide_mir::pipeline::try_render_rust_source(source) {
        Ok(r) => r,
        Err(e) => panic!("{tag} left the certified render: {e:?}"),
    };
    let dir = scratch(tag);
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();
    for target in [None, Some("wasm")] {
        let (stdout, stderr) = run(&src, target);
        assert_eq!(stdout, expected, "{tag} on {target:?}:\n{stderr}");
    }
    let _ = std::fs::remove_dir_all(&dir);
    rust
}

/// The body of the rendered fn whose signature line starts with `head`.
fn body<'a>(rust: &'a str, head: &str) -> &'a str {
    let at = rust.find(head).unwrap_or_else(|| panic!("no `{head}` in the render:\n{rust}"));
    let rest = &rust[at..];
    &rest[..rest.find("\n}\n").map_or(rest.len(), |e| e + 2)]
}

const COUNTED: &str = r#"let lo = 3

fn main() -> Unit = {
  var t = 0
  for k in list.range(0, 300000) {
    t = (t + k % 1100 + 1) % 1000000007
  }
  var u = 0
  for k in list.range(lo * 2, lo * 2 - 5) {
    u = u + k
  }
  for k in list.range(lo - 1, lo + 4) {
    u = u * 10 + k
  }
  println("${t} ${u}")
}
"#;

#[test]
fn a_loop_over_a_fixed_range_counts_instead_of_building_the_list() {
    let rust = prints("counted", COUNTED, "165030000 23456\n");
    let main = body(&rust, "fn main()");
    // A bound that is not a literal keeps a guard that still builds a range
    // too long to be a list (and aborts); the literal one needs none.
    assert!(!main.contains("300000i64)"), "the loop still builds the index list:\n{main}");
}

const TAKE: &str = r#"fn rd(xs: List[Int], i: Int) -> Result[List[Int], String] =
  if i % 13 == 0 then err("bad ${int.to_string(i)}") else ok(list.take(xs, i % 8))

fn last_of(xs: List[Int], n: Int) -> List[Int] = list.take(xs, n)

effect fn main() -> Unit = {
  let xs = list.range(0, 100)
  var t = 0
  for k in list.range(0, 20000) {
    t = t + (match rd(xs, k) { ok(d) => list.len(d), err(_) => 7 })
  }
  // a site that hands its own list over
  let ys = last_of(list.range(0, 50), 4)
  println("${t} ${list.len(xs)} ${list.join(list.map(ys, (y) => int.to_string(y)), ",")}")
}
"#;

#[test]
fn a_kept_argument_is_lent_to_a_by_value_call() {
    let rust = prints("take", TAKE, "75390 100 0,1,2,3\n");
    let rd = body(&rust, "fn almd_rd(");
    let sig = rd.lines().next().unwrap();
    assert!(!sig.contains("Vec<i64>,"), "rd takes its list over, so the loop copies it per call: {sig}");
    let main = body(&rust, "fn main()");
    assert!(!main.contains(".clone()"), "the loop copies the list it keeps for each call:\n{main}");
}
