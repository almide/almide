//! #2250: every position `almide check --json` emits names a real place in
//! the file. Two shapes did not: a hole inside a heredoc was stamped with the
//! string's first line and a column counted over the whole decoded template,
//! and a call whose `)` sits on a later line (a `Span` has no end line, so it
//! carries the span of its `(` alone) offered its `!` right after the `(`. A
//! harness applying `suggestions[]` corrupted the source either way.
use std::path::Path;
use std::process::Command;

fn check_json(tag: &str, src: &str) -> Vec<serde_json::Value> {
    let dir = std::env::temp_dir().join(format!("almide-fixit-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("prog.almd");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("check").arg("--json").arg(&file)
        .output().expect("almide");
    let _ = std::fs::remove_dir_all(Path::new(&dir));
    String::from_utf8_lossy(&out.stdout).lines()
        .filter(|l| l.starts_with('{'))
        .map(|l| serde_json::from_str(l).expect("one JSON diagnostic per line"))
        .collect()
}

/// Every `try_replace` / `suggestions[]` position lies on a line of `src`, at
/// or before one past its last character (a zero-width insertion at the end).
fn assert_positions_on_lines(diags: &[serde_json::Value], src: &str) {
    let lines: Vec<usize> = src.lines().map(|l| l.chars().count()).collect();
    let mut seen = 0;
    for d in diags {
        let mut spots: Vec<&serde_json::Value> = Vec::new();
        if let Some(t) = d.get("try_replace").filter(|v| !v.is_null()) { spots.push(t); }
        if let Some(arr) = d.get("suggestions").and_then(|v| v.as_array()) { spots.extend(arr); }
        for s in spots {
            let line = s["line"].as_u64().unwrap() as usize;
            let col = s["col"].as_u64().unwrap() as usize;
            let end_col = s["end_col"].as_u64().unwrap() as usize;
            assert!(line >= 1 && line <= lines.len(), "{}: line {line} is not in the file ({} lines)", d["code"], lines.len());
            let len = lines[line - 1];
            assert!(col >= 1 && col <= len + 1, "{}: col {col} on line {line} of {len} chars", d["code"]);
            assert!(end_col >= col && end_col <= len + 1, "{}: end_col {end_col} on line {line} of {len} chars", d["code"]);
            seen += 1;
        }
    }
    let _ = seen;
}

/// The primary position of every diagnostic lies on a line of `src`.
fn assert_diagnostics_on_lines(diags: &[serde_json::Value], src: &str) {
    let lines: Vec<usize> = src.lines().map(|l| l.chars().count()).collect();
    for d in diags {
        let line = d["line"].as_u64().unwrap() as usize;
        let col = d["col"].as_u64().unwrap() as usize;
        assert!(line >= 1 && line <= lines.len(), "{}: line {line} is not in the file", d["code"]);
        let len = lines[line - 1];
        assert!(col >= 1 && col <= len + 1, "{}: col {col} on line {line} of {len} chars", d["code"]);
        if let Some(end_col) = d["end_col"].as_u64() {
            let end_col = end_col as usize;
            assert!(end_col >= col && end_col <= len + 1, "{}: end_col {end_col} on line {line} of {len} chars", d["code"]);
        }
    }
}

const MULTI_LINE_CALL: &str = "import fs\n\n\
effect fn main() -> Unit = {\n\
  let t = fs.read_text(\n\
    \"a-path-that-is-long-enough-to-put-the-end-column-past-the-first-line.txt\"\n\
  )\n\
  // a comment where a wrong column would land\n\
  println(t)\n\
}\n";

#[test]
fn a_call_spanning_lines_offers_no_position_off_its_first_line() {
    let diags = check_json("multi", MULTI_LINE_CALL);
    let e041: Vec<_> = diags.iter().filter(|d| d["code"] == "E041").collect();
    assert_eq!(e041.len(), 1, "one E041 for the un-propagated read_text:\n{diags:?}");
    assert!(e041[0]["try_replace"].is_null(), "no positional fix for a span the file cannot locate: {:?}", e041[0]);
    assert_eq!(e041[0]["suggestions"].as_array().map_or(0, |a| a.len()), 0, "{:?}", e041[0]);
    assert_positions_on_lines(&diags, MULTI_LINE_CALL);
}

const ONE_LINE_CALL: &str = "import fs\n\neffect fn main() -> Unit = {\n  let t = fs.read_text(\"x.txt\")\n  println(t)\n}\n";

#[test]
fn a_call_on_one_line_still_offers_the_insertion_at_its_end() {
    let diags = check_json("single", ONE_LINE_CALL);
    let e041: Vec<_> = diags.iter().filter(|d| d["code"] == "E041").collect();
    assert_eq!(e041.len(), 1, "{diags:?}");
    let t = &e041[0]["try_replace"];
    assert_eq!((t["line"].as_u64(), t["col"].as_u64()), (Some(4), Some(32)), "insertion right after `)`: {t:?}");
    assert_positions_on_lines(&diags, ONE_LINE_CALL);
}

/// A hole on the third line of a heredoc. The decoded template has lost the
/// blank first line and the common indent, so counting columns from the
/// opening quotes put this call on the `"""` line, past its end.
const HEREDOC_HOLE: &str = "import fs\n\n\
effect fn main() -> Unit = {\n\
  let h = \"\"\"\n\
\x20   usage: tool [file]\n\
\x20   ${fs.read_text(\"x.txt\")}\n\
\x20   reads the file named above\n\
\x20   \"\"\"\n\
  println(h)\n\
}\n";

#[test]
fn a_hole_inside_a_heredoc_is_reported_on_its_own_line() {
    let diags = check_json("heredoc", HEREDOC_HOLE);
    let e041: Vec<_> = diags.iter().filter(|d| d["code"] == "E041").collect();
    assert_eq!(e041.len(), 1, "one E041 for the un-propagated read_text:\n{diags:?}");
    // `    ${fs.read_text("x.txt")}`: the call starts at column 7 of line 6.
    assert_eq!((e041[0]["line"].as_u64(), e041[0]["col"].as_u64()), (Some(6), Some(7)), "{:?}", e041[0]);
    assert_eq!(e041[0]["end_col"].as_u64(), Some(28), "{:?}", e041[0]);
    assert_diagnostics_on_lines(&diags, HEREDOC_HOLE);
    assert_positions_on_lines(&diags, HEREDOC_HOLE);
}

/// Two holes on different heredoc lines, and one after a hole on the same
/// line: each is found from the previous one on, so a repeated spelling maps
/// to its own occurrence.
const HEREDOC_MANY_HOLES: &str = "import fs\n\n\
effect fn main() -> Unit = {\n\
  let h = \"\"\"\n\
\x20   a ${fs.read_text(\"x\")} b ${fs.read_text(\"x\")}\n\
\x20   c ${fs.read_text(\"x\")}\n\
\x20   \"\"\"\n\
  println(h)\n\
}\n";

#[test]
fn repeated_holes_in_a_heredoc_each_map_to_their_own_occurrence() {
    let diags = check_json("heredoc-many", HEREDOC_MANY_HOLES);
    let mut spots: Vec<(u64, u64)> = diags.iter().filter(|d| d["code"] == "E041")
        .map(|d| (d["line"].as_u64().unwrap(), d["col"].as_u64().unwrap())).collect();
    spots.sort();
    assert_eq!(spots, vec![(5, 9), (5, 32), (6, 9)], "{diags:?}");
    assert_diagnostics_on_lines(&diags, HEREDOC_MANY_HOLES);
}
