//! #2802: JSON string quoting is ONE rule on every leg — the family matrix.
//!
//! The rule (RFC 8259 §7; C-095): `\` → `\\`, `"` → `\"`, U+000A / U+000D /
//! U+0009 → `\n` / `\r` / `\t`, every OTHER U+0000..U+001F → `\u00xx`
//! (lowercase hex, no `\b` / `\f` short forms), everything else raw UTF-8 —
//! U+007F included.
//!
//! The rule has separate copies that cannot share code: the native runtime
//! (`runtime/rs/src/value.rs` `almide_rt_value_json_escape_into`, used by the
//! compact stringify for values and keys, json.rs's pretty printer and the SSE
//! builder), the structural wasm helper `$vjson_quote`
//! (`crates/almide-wasm/src/json_helpers.rs`), the self-hosted `__json_quote`
//! the incumbent wasm leg links (`stdlib/value_core.almd`), and the interp's
//! `DynNode::to_json` (unit-tested in its own crate; the run-parity gate votes
//! it on the `json_stringify_control_chars` fixture). Before #2802 the first
//! three escaped only five characters and wrote the rest of the control range
//! raw — invalid JSON a strict parser refuses.
//!
//! The matrix: every one of the 32 control characters plus `"`, `\` and
//! U+007F, as a VALUE and as a KEY, through `json.stringify` and
//! `json.stringify_pretty`, on native and the wasm leg — each cell must equal the rule's bytes, computed here
//! independently of every implementation.

use std::process::Command;

/// The cells: the whole control range, the two characters JSON always
/// escapes, and DEL (which it does not).
fn codes() -> Vec<u32> {
    (0u32..32).chain([34, 92, 127]).collect()
}

/// The rule, written from RFC 8259 §7 — not from any implementation.
fn rule(c: u32) -> String {
    match c {
        0x5c => "\\\\".to_string(),
        0x22 => "\\\"".to_string(),
        0x0a => "\\n".to_string(),
        0x0d => "\\r".to_string(),
        0x09 => "\\t".to_string(),
        c if c < 0x20 => format!("\\u{c:04x}"),
        c => char::from_u32(c).expect("scalar").to_string(),
    }
}

/// One line per cell: `code|value|key|pretty value|pretty key`, the pretty
/// forms with their structural newlines folded to `/` (an escaped string
/// holds no raw newline, so the fold cannot collide with the payload).
const PROGRAM: &str = r#"
import json

fn cell(c: Int) -> String = {
  let s = string.from_codepoint(c)
  let v = value.str(s)
  let k = value.object([(s, value.int(1))])
  let pv = string.replace(json.stringify_pretty(value.array([v])), "\n", "/")
  let pk = string.replace(json.stringify_pretty(k), "\n", "/")
  "${c}|${json.stringify(v)}|${json.stringify(k)}|${pv}|${pk}"
}

fn main() -> Unit = {
  let codes = list.range(0, 32) + [34, 92, 127]
  for c in codes {
    println(cell(c))
  }
}
"#;

fn expected() -> String {
    codes()
        .into_iter()
        .map(|c| {
            let q = format!("\"{}\"", rule(c));
            format!("{c}|{q}|{{{q}:1}}|[/  {q}/]|{{/  {q}: 1/}}\n")
        })
        .collect()
}

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// Build and run PROGRAM on one leg; the stdout.
fn run_leg(target: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(&source, PROGRAM).expect("source");
    let artifact = dir.path().join(if target == "rust" { "native" } else { "m.wasm" });
    let mut build = Command::new(almide_bin());
    build
        .args(["build", source.to_str().expect("path"), "--target", target, "-o"])
        .arg(&artifact)
        .env_remove("ALMIDE_WASM_STRUCTURAL")
        .env_remove("ALMIDE_COMPONENT_P3");
    let built = build.output().expect("build");
    assert!(built.status.success(), "{target} build:\n{}", String::from_utf8_lossy(&built.stderr));
    let out = if target == "rust" {
        Command::new(&artifact).output().expect("run")
    } else {
        Command::new("wasmtime").arg("run").arg(&artifact).output().expect("run")
    };
    assert!(out.status.success(), "{target} exited {:?}: {}", out.status.code(), String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

/// Compare line by line so a failure names the code point and the leg.
fn assert_cells(leg: &str, got: &str, want: &str) {
    let got_lines: Vec<&str> = got.lines().collect();
    let want_lines: Vec<&str> = want.lines().collect();
    assert_eq!(got_lines.len(), want_lines.len(), "{leg}: cell count");
    for (g, w) in got_lines.iter().zip(&want_lines) {
        assert_eq!(g.as_bytes(), w.as_bytes(), "{leg}: cell `{}` differs from the RFC 8259 rule\n got: {g:?}\nwant: {w:?}", w.split('|').next().unwrap_or("?"));
    }
}

#[test]
fn the_rule_table_is_the_rfc_one() {
    // The oracle itself, pinned at the cells the issue named and the edges.
    assert_eq!(rule(0x1b), "\\u001b");
    assert_eq!(rule(0x01), "\\u0001");
    assert_eq!(rule(0x00), "\\u0000");
    assert_eq!(rule(0x08), "\\u0008");
    assert_eq!(rule(0x0c), "\\u000c");
    assert_eq!(rule(0x1f), "\\u001f");
    assert_eq!(rule(0x7f), "\u{7f}");
    assert_eq!(codes().len(), 35);
}

#[test]
fn every_leg_quotes_every_control_character_by_the_one_rule() {
    let want = expected();
    let native = run_leg("rust");
    assert_cells("native", &native, &want);
    if !wasmtime_available() {
        eprintln!("skipping the wasm legs: wasmtime not on PATH");
        return;
    }
    let structural = run_leg("wasm");
    assert_cells("wasm structural", &structural, &want);
    assert_eq!(native, structural, "native vs structural bytes");
}
