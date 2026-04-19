//! Diagnostic fixture harness — the Phase 2 substrate of
//! `docs/roadmap/active/diagnostics-here-try-hint.md`.
//!
//! Each directory under `tests/diagnostics/<case>/` is one fixture:
//!
//! ```
//! tests/diagnostics/bang-not/
//!   broken.almd   // fails to compile with a specific diagnostic
//!   fixed.almd    // compiles cleanly — the suggested fix
//!   meta.toml     // expected diagnostic substrings / code
//! ```
//!
//! The harness enforces two invariants per case:
//!
//! 1. `broken.almd` produces at least one error, and (if `meta.toml`
//!    declares expectations) the error payload contains the expected
//!    code / substring / hint.
//! 2. `fixed.almd` compiles cleanly — zero errors from `almide check`.
//!
//! This is the foundation the `Try:` snippet auto-apply phase will
//! build on; right now the fixtures only pin "fix exists and
//! compiles". When individual diagnostics gain populated `try:`
//! snippets, a follow-up test will mechanically apply them and
//! assert equality with `fixed.almd`.
//!
//! Running: `cargo test --test diagnostic_harness_test`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

#[derive(Default, Debug)]
struct Meta {
    expects_error: Option<String>,
    expects_code: Option<String>,
    hint_substring: Option<String>,
}

fn parse_meta(path: &Path) -> Meta {
    let Ok(text) = std::fs::read_to_string(path) else { return Meta::default(); };
    let mut fields: HashMap<&str, String> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let Some((key, rest)) = line.split_once('=') else { continue };
        let key = key.trim();
        let value = rest.trim().trim_matches('"').to_string();
        fields.insert(
            match key {
                "expects_error" => "expects_error",
                "expects_code" => "expects_code",
                "hint_substring" => "hint_substring",
                _ => continue,
            },
            value,
        );
    }
    Meta {
        expects_error: fields.remove("expects_error"),
        expects_code: fields.remove("expects_code"),
        hint_substring: fields.remove("hint_substring"),
    }
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/diagnostics")
}

fn collect_cases() -> Vec<PathBuf> {
    let root = fixtures_root();
    let mut cases = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            if entry.file_type().map_or(false, |t| t.is_dir()) {
                cases.push(entry.path());
            }
        }
    }
    cases.sort();
    cases
}

fn run_check(file: &Path) -> (bool, String, String) {
    let out = Command::new(almide())
        .args(["check", file.to_str().unwrap()])
        .output()
        .expect("almide check");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (out.status.success(), stdout, stderr)
}

#[test]
fn every_case_has_broken_and_fixed_pair() {
    let cases = collect_cases();
    assert!(!cases.is_empty(), "no fixtures found under tests/diagnostics/");
    for case in &cases {
        assert!(
            case.join("broken.almd").exists(),
            "missing broken.almd in {}", case.display()
        );
        assert!(
            case.join("fixed.almd").exists(),
            "missing fixed.almd in {}", case.display()
        );
    }
}

#[test]
fn broken_files_produce_expected_diagnostics() {
    let cases = collect_cases();
    for case in &cases {
        let broken = case.join("broken.almd");
        let meta = parse_meta(&case.join("meta.toml"));
        let (success, stdout, stderr) = run_check(&broken);
        let combined = format!("{}{}", stdout, stderr);
        assert!(
            !success || combined.contains("error"),
            "broken.almd in {} unexpectedly passed:\nstdout: {}\nstderr: {}",
            case.display(), stdout, stderr
        );
        if let Some(code) = &meta.expects_code {
            assert!(
                combined.contains(&format!("[{}]", code))
                || combined.contains(&format!("error[{}]", code)),
                "expected code {} not found in diagnostic for {}:\n{}",
                code, case.display(), combined
            );
        }
        if let Some(err) = &meta.expects_error {
            assert!(
                combined.contains(err),
                "expected error substring {:?} not in diagnostic for {}:\n{}",
                err, case.display(), combined
            );
        }
        if let Some(hint) = &meta.hint_substring {
            assert!(
                combined.to_lowercase().contains(&hint.to_lowercase()),
                "expected hint substring {:?} not in diagnostic for {}:\n{}",
                hint, case.display(), combined
            );
        }
    }
}

#[test]
fn fixed_files_compile_cleanly() {
    let cases = collect_cases();
    for case in &cases {
        let fixed = case.join("fixed.almd");
        let (success, stdout, stderr) = run_check(&fixed);
        assert!(
            success,
            "fixed.almd in {} failed to compile:\nstdout: {}\nstderr: {}",
            case.display(), stdout, stderr
        );
    }
}

fn run_check_json(file: &Path) -> Vec<DiagJson> {
    let out = Command::new(almide())
        .args(["check", "--json", file.to_str().unwrap()])
        .output()
        .expect("almide check --json");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    stdout.lines().filter_map(|l| DiagJson::parse(l)).collect()
}

/// Minimal subset of the `almide check --json` payload the harness
/// consumes. Intentionally hand-parsed so the test crate stays
/// serde-free (matches the emitter side in `diagnostic_render.rs`).
#[derive(Debug)]
struct DiagJson {
    try_snippet: Option<String>,
    try_replace: Option<(usize, usize, usize)>,
}

impl DiagJson {
    fn parse(line: &str) -> Option<Self> {
        if !line.trim_start().starts_with('{') { return None; }
        Some(DiagJson {
            try_snippet: json_string(line, "\"try\":"),
            try_replace: json_obj_uints(line, "\"try_replace\":", &["line", "col", "end_col"])
                .map(|v| (v[0], v[1], v[2])),
        })
    }
}

fn json_string(blob: &str, key: &str) -> Option<String> {
    let i = blob.find(key)?;
    let rest = &blob[i + key.len()..];
    let rest = rest.trim_start();
    if rest.starts_with("null") { return None; }
    if !rest.starts_with('"') { return None; }
    let body = &rest[1..];
    let mut out = String::new();
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                other => { out.push(other); }
            }
        } else if c == '"' {
            return Some(out);
        } else {
            out.push(c);
        }
    }
    None
}

fn json_obj_uints(blob: &str, key: &str, fields: &[&str]) -> Option<Vec<usize>> {
    let i = blob.find(key)?;
    let rest = &blob[i + key.len()..].trim_start();
    if rest.starts_with("null") { return None; }
    if !rest.starts_with('{') { return None; }
    let end = rest.find('}')?;
    let inner = &rest[1..end];
    let mut out = Vec::with_capacity(fields.len());
    for f in fields {
        let needle = format!("\"{}\":", f);
        let pos = inner.find(&needle)?;
        let tail = &inner[pos + needle.len()..];
        let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
        out.push(digits.parse().ok()?);
    }
    Some(out)
}

/// Apply a `try_replace` span to `source` — mirrors
/// `Diagnostic::apply_try_to` on the harness side so the test can run
/// without linking the compiler crate. Kept tiny so when the internal
/// method changes we just mirror the change here. Same 1-indexed,
/// end-exclusive semantics.
fn apply_try(source: &str, line: usize, col: usize, end_col: usize, snippet: &str) -> Option<String> {
    if line == 0 || col == 0 || end_col < col { return None; }
    let mut line_start = 0usize;
    let mut cur_line = 1usize;
    for (i, b) in source.bytes().enumerate() {
        if cur_line == line { break; }
        if b == b'\n' {
            cur_line += 1;
            line_start = i + 1;
        }
    }
    if cur_line != line { return None; }
    let line_tail = &source[line_start..];
    let line_end = line_tail.find('\n').map(|i| line_start + i).unwrap_or(source.len());
    let line_slice = &source[line_start..line_end];
    let col_to_byte = |target: usize| -> Option<usize> {
        match line_slice.char_indices().nth(target - 1) {
            Some((b, _)) => Some(b),
            None => {
                let n = line_slice.chars().count();
                if target == n + 1 { Some(line_slice.len()) } else { None }
            }
        }
    };
    let start_off = line_start + col_to_byte(col)?;
    let end_off = line_start + col_to_byte(end_col)?;
    if end_off < start_off || end_off > line_end { return None; }
    let mut out = String::with_capacity(source.len() + snippet.len());
    out.push_str(&source[..start_off]);
    out.push_str(snippet);
    out.push_str(&source[end_off..]);
    Some(out)
}

#[test]
fn try_snippets_with_replace_span_apply_cleanly() {
    // For each fixture, any diagnostic that emits both `try:` and
    // `try_replace:` is expected to rewrite `broken.almd` into
    // `fixed.almd` (whitespace-normalised) AND have the rewritten
    // source compile. Until per-diagnostic migrations in Phase 3 land,
    // this loop may be a no-op — that's fine; it's the regression
    // gate for when migrations do arrive.
    let cases = collect_cases();
    for case in &cases {
        let broken = case.join("broken.almd");
        let fixed = case.join("fixed.almd");
        let diagnostics = run_check_json(&broken);
        for d in &diagnostics {
            let (Some(snippet), Some((line, col, end_col))) = (&d.try_snippet, d.try_replace) else { continue };
            let broken_src = std::fs::read_to_string(&broken).unwrap();
            let rewritten = apply_try(&broken_src, line, col, end_col, snippet)
                .unwrap_or_else(|| panic!(
                    "try_replace failed to apply in {}: line={}, col={}..{}, snippet={:?}",
                    case.display(), line, col, end_col, snippet
                ));
            // Write to a scratch file and compile it.
            let scratch = case.join("_applied.almd");
            std::fs::write(&scratch, &rewritten).unwrap();
            let (success, stdout, stderr) = run_check(&scratch);
            let _ = std::fs::remove_file(&scratch);
            assert!(
                success,
                "applied try: did not compile in {}:\nrewritten:\n{}\nstdout: {}\nstderr: {}",
                case.display(), rewritten, stdout, stderr
            );
            let fixed_src = std::fs::read_to_string(&fixed).unwrap();
            let norm = |s: &str| s.replace("\r\n", "\n").trim_end().to_string();
            assert_eq!(
                norm(&rewritten), norm(&fixed_src),
                "applied try: does not match fixed.almd in {}", case.display()
            );
        }
    }
}
