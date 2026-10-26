//! An assert failure shows its operands escaped by C-418's rule, the same
//! text on every route (#4262).
//!
//! An `assert_eq` failure printed `expected:` / `found:` raw on both legs of a
//! program's own abort and on the wasm test lane, while the native test lane
//! printed Rust `Debug` (quoted, and a different escape). A program asserting
//! over a string with ESC, U+202E or a newline wrote those bytes to the
//! terminal and into CI logs, and the newline could forge a `found:` line.
//!
//! The rule now (C-153, C-418): an operand is its ALS-R2 rendering with every
//! C0, DEL, C1 and bidi control written as `\n` / `\u{<hex>}`, everything else
//! kept, unquoted. This test runs one failing `assert_eq` and one failing
//! `assert_ne` through every route: `almide run` on native and wasm, and
//! `almide test` on its default lane, `--target rust` and `--target wasm`.
//! On each it requires the escaped operand text and no byte of the escaped
//! class anywhere in the output. Skips cleanly when the binary is unavailable.

use std::path::{Path, PathBuf};
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

fn available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

/// The operand under test: ESC (a colour sequence), U+202E, a newline.
const HOSTILE: &str = "A\\u{1b}[31mB\\u{202e}C\\nD";
/// Its C-418 text: what every route must print.
const SHOWN: &str = "A\\u{1b}[31mB\\u{202e}C\\nD";
/// A single-line twin for `assert_ne` (a `\n` in both operands would make
/// the report show a line diff instead of the pair).
const HOSTILE_LINE: &str = "A\\u{1b}[31mB\\u{202e}C";
/// The other operand: a C1 control (U+0085) and an isolate (U+2066).
const OTHER: &str = "x\\u{85}y\\u{2066}";

/// C-418's escaped class: C0, DEL, C1, and the bidi controls.
fn raw_class(c: char) -> bool {
    let n = c as u32;
    (n <= 0x1f && c != '\n') || (0x7f..=0x9f).contains(&n) || n == 0x061c || n == 0x200e || n == 0x200f
        || (0x202a..=0x202e).contains(&n) || (0x2066..=0x2069).contains(&n)
}

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-4262-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn run(args: &[&str], cwd: &Path) -> (i32, String) {
    let out = Command::new(almide_bin())
        .current_dir(cwd)
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("spawn almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn check_route(route: &str, rc: i32, out: &str, want: &[&str]) {
    check_route_n(route, rc, out, want, false)
}

/// The failure block, from its header to the end of the output.
fn block(out: &str) -> &str {
    out.find("Error: assertion failed").map_or("", |i| &out[i..])
}

fn check_route_n(route: &str, rc: i32, out: &str, want: &[&str], ends_on_found: bool) {
    assert_ne!(rc, 0, "{route}: a failing assert passed:\n{out}");
    for w in want {
        assert!(out.contains(w), "{route}: the operand is not shown as `{w}`:\n{out}");
    }
    let raw: Vec<String> = out.chars().filter(|c| raw_class(*c)).map(|c| format!("U+{:04X}", c as u32)).collect();
    assert!(raw.is_empty(), "{route}: raw control/bidi characters reached the output: {raw:?}\n{}", out.escape_debug());
    // The newline inside the operand never splits the block: the operand is
    // one escaped line, and the block ends with it.
    if ends_on_found {
        assert!(block(out).trim_end().ends_with(&format!("  found: {SHOWN}")), "{route}: the block does not end on the found line:\n{out}");
    }
}

#[test]
fn a_program_abort_shows_its_operands_escaped_on_both_legs() {
    if !available() {
        return;
    }
    let d = dir("prog");
    std::fs::write(
        d.join("prog.almd"),
        format!("fn main() -> Unit = {{\n  let h = \"{HOSTILE}\"\n  assert_eq(h, \"{OTHER}\")\n}}\n"),
    )
    .unwrap();
    let want = format!("Error: assertion failed\n  at: line 3\n  expected: {OTHER}\n  found: {SHOWN}");
    let (rc, native) = run(&["run", "prog.almd"], &d);
    check_route_n("run native", rc, &native, &[&want], true);
    let (rc, wasm) = run(&["run", "prog.almd", "--target", "wasm"], &d);
    check_route_n("run wasm", rc, &wasm, &[&want], true);
    assert_eq!(block(&native), block(&wasm), "the two legs print different failure blocks");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_test_lane_failure_shows_the_same_operands_on_every_lane() {
    if !available() {
        return;
    }
    let d = dir("test");
    std::fs::write(
        d.join("eq_test.almd"),
        format!("test \"eq\" {{\n  let h = \"{HOSTILE}\"\n  assert_eq(h, \"{OTHER}\")\n}}\n"),
    )
    .unwrap();
    std::fs::write(d.join("ne_test.almd"), format!("test \"ne\" {{\n  let h = \"{HOSTILE_LINE}\"\n  assert_ne(h, h)\n}}\n")).unwrap();
    for lane in [&[][..], &["--target", "rust"][..], &["--target", "wasm"][..]] {
        let route = format!("almide test {}", lane.join(" "));
        let mut args = vec!["test", "eq_test.almd"];
        args.extend_from_slice(lane);
        let (rc, out) = run(&args, &d);
        check_route(&format!("{route} (assert_eq)"), rc, &out, &[&format!("expected: {OTHER}"), &format!("found:    {SHOWN}")]);
        let mut args = vec!["test", "ne_test.almd"];
        args.extend_from_slice(lane);
        let (rc, out) = run(&args, &d);
        check_route(&format!("{route} (assert_ne)"), rc, &out, &[&format!("expected: != {HOSTILE_LINE}")]);
    }
    let _ = std::fs::remove_dir_all(&d);
}
