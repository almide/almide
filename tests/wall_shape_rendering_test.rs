//! A known-shape wall renders as headline + rewrite hint + caret + note (#931).
//!
//! The closure condition's rendering half: for a wall whose site stamped a
//! `WallShape`, the CLI diagnostic leads with a surface-language headline,
//! hints the documented rewrite (the while-heap-accumulator wall's hint is
//! the recursion idiom), points at the source with a caret, and demotes the
//! raw compiler-internal reason string to a trailing `note:`. Before, the
//! reason WAS the headline and the hint was only the file-an-issue pointer.
//!
//! Skips cleanly when the `almide` binary is unavailable (CI builds it in the
//! build step; locally run `cargo build --release` first).

use std::path::Path;
use std::process::Command;

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

/// The single most-reported wall (#931's own example class), reduced from
/// `examples/minesweeper.almd` by ddmin: a `while` body reassigning a heap
/// accumulator in a nested arm, past what the scalar-state loop admits. (The
/// simple `s = s + "x"` loop now EXECUTES — the subset absorbed it — so the
/// fixture needs the effect-call let + nested reassignment to still wall. If
/// the subset absorbs this shape too, the first assertion below fails: pick
/// the next still-walled shape from the skip ledger and update the fixture.)
const WHILE_HEAP_ACCUMULATOR: &str = r#"import io

effect fn pick() -> Result[List[Int], String] = ok([1, 2])

fn grow(xs: List[Int], n: Int) -> List[Int] = xs + [n]

effect fn main() -> Unit = {
  var adj: List[Int] = []
  var first = true
  var game_over = false
  while not game_over {
    let input = io.read_line()
    match int.parse(input) {
      err(msg) => {},
      ok(n) => {
        if n > 0 then {
          if first then {
            let mines = pick()
            adj = grow(mines, n)
            first = false
          } else ()
        } else {
          game_over = true
        }
      },
    }
  }
  println(int.to_string(list.len(adj)))
}
"#;

#[test]
fn while_heap_accumulator_wall_renders_headline_hint_caret_and_note() {
    if !tool_available() {
        eprintln!("skipping: almide binary not available");
        return;
    }
    let dir = std::env::temp_dir().join(format!("almide-wall-shape-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("wall.almd");
    std::fs::write(&src, WHILE_HEAP_ACCUMULATOR).unwrap();

    let output = Command::new(almide_bin())
        .args([
            "build",
            src.to_str().unwrap(),
            "--target",
            "wasm",
            "-o",
            dir.join("wall.wasm").to_str().unwrap(),
        ])
        .output()
        .expect("failed to spawn almide");
    let stderr = String::from_utf8_lossy(&output.stderr);
    std::fs::remove_dir_all(&dir).ok();

    assert!(
        !output.status.success(),
        "a walled build must fail (v0 was retired; a wall is an honest error), got success.\nstderr: {stderr}"
    );
    // Headline: what the user wrote, surface vocabulary — not the raw reason.
    assert!(
        stderr.contains("grows a heap value"),
        "headline missing from:\n{stderr}"
    );
    // Hint: the documented rewrite, not (only) the file-an-issue pointer.
    assert!(
        stderr.contains("hoist the accumulator into a recursive helper"),
        "rewrite hint missing from:\n{stderr}"
    );
    // The caret gutter points into the source.
    assert!(stderr.contains("^"), "caret missing from:\n{stderr}");
    assert!(stderr.contains("while"), "source line missing from:\n{stderr}");
    // The raw reason survives as a note (it still serves a bug report), and
    // is no longer the headline.
    assert!(
        stderr.contains("note: ") && stderr.contains("model-one-iteration"),
        "raw-reason note missing from:\n{stderr}"
    );
    let headline_line = stderr
        .lines()
        .find(|l| l.contains("error"))
        .unwrap_or_default();
    assert!(
        !headline_line.contains("model-one-iteration"),
        "the raw reason leaked back into the headline:\n{stderr}"
    );
}
