//! Structured `almide test` failure reporting — the agent feedback surface.
//!
//! Measured reality (FeedbackEval, arXiv 2504.06939): TEST-failure feedback
//! repairs better than compiler errors (repair@1 57.9% vs 49.2%), and the
//! winning format is *structured data + a one-line hint*, not prose. This
//! module turns whatever a failing test binary printed into that shape:
//!
//! ```text
//! FAILED spec/lang/x_test.almd
//!   test: string mismatch
//!   at:   spec/lang/x_test.almd:4
//!   hint: line 2 differs
//!   diff: -expected +found
//!       line one
//!     - line TWO
//!     + line two
//!       line three
//! ```
//!
//! Two failure spellings reach it, and both are parsed here rather than
//! anywhere else:
//!
//! 1. the T18 abort block (`Error: assertion failed` + `at:`/`expected:`/
//!    `found:` lines) — an assert in a plain `fn` called from a test body,
//!    byte-identical on both targets (C-153);
//! 2. libtest's panic payload for an assert INSIDE a `test` block, whose
//!    macro carries the `.almd` source line (`at line N`) so the report can
//!    name the source instead of the generated `main.rs`.
//!
//! Everything downstream (the human-readable render, the diff, `--json`) is
//! computed from one [`TestFailure`] record, so the two channels cannot drift.

use crate::{err, err_no_nl};

/// A finished test file: its path, its exit code, and its captured
/// stdout+stderr (empty when the file never got as far as running).
pub type TestRun = (String, i32, String);

/// The argv for a native (libtest) test binary.
///
/// `--nocapture` is load-bearing, not cosmetic: libtest DISCARDS a test's
/// captured buffer when the test exits the process, so a T18 assert abort (an
/// `assert` in a plain `fn` called from a test body) printed nothing at all —
/// the run reported a bare `FAILED` with no reason. `almide test` captures the
/// whole binary's output itself and prints it only on failure, so nothing leaks
/// into a passing run.
pub fn test_harness_args(run_filter: Option<&str>) -> std::sync::Arc<Vec<String>> {
    let mut args = vec!["--nocapture".to_string()];
    args.extend(run_filter.map(|f| f.to_string()));
    std::sync::Arc::new(args)
}

/// Report one failing test file as a STRUCTURED record: the assertion's `.almd`
/// site, expected/found, and a real diff for multi-line strings, lists and
/// records. Falls back to the raw captured output when nothing parses — a
/// failure this reporter does not understand must never be swallowed.
pub fn report_test_failure(file: &str, output: &str) {
    err(&format!("FAILED: {}", file));
    let source = std::fs::read_to_string(file).unwrap_or_default();
    let failures = parse(file, &source, output);
    if failures.is_empty() {
        if output.is_empty() || output.ends_with('\n') {
            err_no_nl(output);
        } else {
            err(output);
        }
        return;
    }
    for f in &failures {
        err_no_nl(&f.render());
    }
}

/// One failing assertion, normalized away from whichever harness printed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestFailure {
    /// The `.almd` file under test.
    pub file: String,
    /// The `test "…"` name, when it could be recovered.
    pub name: Option<String>,
    /// 1-based line of the assertion in `file`.
    pub line: Option<usize>,
    /// `assert_eq` / `assert_ne` / `assert` / `panic`.
    pub op: String,
    /// The right operand of `assert_eq` (the expectation, per the
    /// `assert_eq(actual, expected)` convention used throughout the docs).
    pub expected: Option<String>,
    /// The left operand — what the code actually produced.
    pub found: Option<String>,
    /// The raw payload, kept verbatim for anything not shaped like an assert.
    pub message: String,
}

impl TestFailure {
    /// The terse agent-facing block. Values are shown as a DIFF when they
    /// decompose into comparable units (string lines, list items, record
    /// fields) and as a plain `expected:`/`found:` pair otherwise — never
    /// both, so a scalar mismatch stays four lines.
    pub fn render(&self) -> String {
        let mut out = String::new();
        if let Some(n) = &self.name {
            out.push_str(&format!("  test: {n}\n"));
        }
        if let Some(l) = self.line {
            out.push_str(&format!("  at:   {}:{}\n", self.file, l));
        }
        let (expected, found) = match (&self.expected, &self.found) {
            (Some(e), Some(f)) => (e, f),
            _ => {
                out.push_str(&format!("  error: {}\n", indent_continuation(&self.message)));
                return out;
            }
        };
        match Diff::of(expected, found) {
            Some(d) => {
                out.push_str(&format!("  hint: {}\n", d.hint()));
                out.push_str("  diff: -expected +found\n");
                out.push_str(&d.render());
            }
            None => {
                out.push_str(&format!("  expected: {}\n", indent_continuation(expected)));
                out.push_str(&format!("  found:    {}\n", indent_continuation(found)));
            }
        }
        out
    }

    /// One JSON object per failure: `{name, file, line, op, expected, found,
    /// diff}` — the record the dojo harness consumes.
    pub fn to_json(&self) -> String {
        let diff = self
            .expected
            .as_deref()
            .zip(self.found.as_deref())
            .and_then(|(e, f)| Diff::of(e, f))
            .map(|d| d.render());
        let v = serde_json::json!({
            "name": self.name,
            "file": self.file,
            "line": self.line,
            "op": self.op,
            "expected": self.expected,
            "found": self.found,
            "diff": diff,
            "message": self.message,
        });
        v.to_string()
    }
}

/// Indent every line after the first, so a multi-line value cannot be mistaken
/// for a new `key:` field by whatever reads the report next.
fn indent_continuation(v: &str) -> String {
    v.replace('\n', "\n    ")
}

// ── Parsing ─────────────────────────────────────────────────────────────────

/// Every failure in one test binary's combined output, in SOURCE ORDER.
///
/// libtest reports its failure blocks in thread-completion order, so the raw
/// transcript of a multi-failure file is different on every run and two runs of
/// the same suite cannot be diffed. Sorting by `(line, name)` makes the report
/// a function of the failures alone.
pub fn parse(file: &str, source: &str, output: &str) -> Vec<TestFailure> {
    let names = test_name_map(source);
    let mut out = parse_libtest(file, &names, output);
    out.extend(parse_t18(file, output));
    out.sort_by(|a, b| {
        (a.line.unwrap_or(usize::MAX), &a.name, &a.message)
            .cmp(&(b.line.unwrap_or(usize::MAX), &b.name, &b.message))
    });
    out
}

/// libtest's failure payloads, in EITHER framing:
///
/// - captured (`---- <path> stdout ----` … payload), and
/// - `--nocapture` (`thread '<path>' panicked at <loc>:` … payload), which is
///   how `almide test` runs the harness — libtest DISCARDS the captured buffer
///   when a test exits the process, so the T18 abort of an `assert` in a plain
///   `fn` called from a test body printed nothing at all under capture.
///
/// Only one framing is present in a given run; trying the captured one first
/// keeps the reporter working if the flag is ever dropped.
fn parse_libtest(file: &str, names: &[(String, String)], output: &str) -> Vec<TestFailure> {
    let lines: Vec<&str> = output.lines().collect();
    let captured = collect_blocks(&lines, block_header);
    let blocks = if captured.is_empty() { collect_blocks(&lines, panic_header) } else { captured };
    blocks
        .into_iter()
        .filter(|(_, payload)| !payload.is_empty())
        .map(|(raw, payload)| {
            let mut f = classify(file, &payload);
            f.name = Some(display_name(names, raw));
            f
        })
        .collect()
}

/// Every `(test path, payload)` whose block opens on a line `header` accepts.
fn collect_blocks<'a>(
    lines: &[&'a str],
    header: fn(&'a str) -> Option<&'a str>,
) -> Vec<(&'a str, Vec<String>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some(raw) = header(lines[i]) else {
            i += 1;
            continue;
        };
        i += 1;
        // The captured framing puts a blank line and libtest's panic banner
        // between the header and the payload; the `--nocapture` framing has the
        // banner AS the header. Skipping both leaves exactly the payload.
        while i < lines.len() && (lines[i].trim().is_empty() || panic_header(lines[i]).is_some()) {
            i += 1;
        }
        let start = i;
        while i < lines.len() && !is_block_end(lines[i]) {
            i += 1;
        }
        out.push((raw, lines[start..i].iter().map(|l| l.to_string()).collect()));
    }
    out
}

/// `---- tests::__test_almd_x stdout ----` → `tests::__test_almd_x`.
fn block_header(line: &str) -> Option<&str> {
    line.strip_prefix("---- ")?.strip_suffix(" stdout ----")
}

/// `thread 'tests::__test_almd_x' panicked at <generated>.rs:L:C:` →
/// `tests::__test_almd_x`. The location in that banner is the EMITTED Rust, not
/// the `.almd` an agent can edit, so it is dropped with the banner.
fn panic_header(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("thread '")?;
    let end = rest.find('\'')?;
    line.contains(" panicked at ").then(|| &rest[..end])
}

/// Any line that can only belong to the harness, never to a payload.
fn is_block_end(line: &str) -> bool {
    line.starts_with("---- ")
        || line == "failures:"
        || line.starts_with("test result:")
        || line.starts_with("note: run with `RUST_BACKTRACE=")
        || (line.starts_with("thread '") && line.contains(" panicked at "))
        || (line.starts_with("test ") && line.contains(" ... "))
        || line.trim().is_empty()
}

/// Turn one panic payload into a [`TestFailure`]. Rust's own assertion macros
/// print `assertion \`left == right\` failed[: <msg>]` + `  left:` / ` right:`,
/// and the `<msg>` is the `at line N` the builtin-lowering pass injected.
fn classify(file: &str, payload: &[String]) -> TestFailure {
    let head = payload[0].as_str();
    let (op, message) = head_op(head);
    let base = TestFailure {
        file: file.to_string(),
        name: None,
        line: site_line(head),
        op: op.into(),
        expected: None,
        found: None,
        // A payload this reporter does not understand keeps ALL of its lines.
        message: if op == "panic" { payload.join("\n") } else { message },
    };
    if !matches!(op, "assert_eq" | "assert_ne") {
        return base;
    }
    let Some((left, right)) = operand_pair(&payload[1..]) else { return base };
    // `assert_ne` fails on EQUAL operands: the expectation is the negation, and
    // spelling it `!= <r>` is what makes the pair read as a claim rather than
    // as two identical values.
    let expected = if op == "assert_ne" { format!("!= {right}") } else { right };
    TestFailure { expected: Some(expected), found: Some(left), ..base }
}

/// `(op, message)` for a panic payload's first line. The site marker the
/// builtin-lowering pass injected is STRIPPED from the message — it is already
/// reported on the `at:` line, and repeating it is the prose FeedbackEval
/// measures as noise.
fn head_op(head: &str) -> (&'static str, String) {
    match head.split(" failed").next() {
        Some("assertion `left == right`") => return ("assert_eq", head.to_string()),
        Some("assertion `left != right`") => return ("assert_ne", head.to_string()),
        _ => {}
    }
    if head == "assertion failed" || site_only(head) {
        return ("assert", "assertion failed".to_string());
    }
    match head.rfind(" (at line ") {
        Some(cut) if head.ends_with(')') => ("assert", head[..cut].to_string()),
        _ => ("panic", head.to_string()),
    }
}

/// A payload that is NOTHING but the injected site (a bare `assert(cond)`).
fn site_only(head: &str) -> bool {
    head.strip_prefix("at line ").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// `  left: <v>` / ` right: <v>` — `left` runs until the ` right: ` marker,
/// `right` to the end, so a multi-line `Debug` value survives intact.
fn operand_pair(rest: &[String]) -> Option<(String, String)> {
    let split = rest.iter().position(|l| l.starts_with(" right: "))?;
    let left = rest.first()?.strip_prefix("  left: ")?;
    let mut l = vec![left.to_string()];
    l.extend(rest[1..split].iter().cloned());
    let mut r = vec![rest[split].strip_prefix(" right: ")?.to_string()];
    r.extend(rest[split + 1..].iter().cloned());
    Some((l.join("\n"), r.join("\n")))
}

/// `… at line 42` → `42`.
fn site_line(head: &str) -> Option<usize> {
    let at = head.rfind("at line ")?;
    head[at + "at line ".len()..]
        .trim_end_matches(|c: char| !c.is_ascii_digit())
        .parse()
        .ok()
}

/// The T18 abort block (C-153), byte-identical on native and wasm:
/// ```text
/// Error: assertion failed
///   at: line 7
///   expected: 3
///   found: 4
/// ```
fn parse_t18(file: &str, output: &str) -> Vec<TestFailure> {
    let mut out = Vec::new();
    let lines: Vec<&str> = output.lines().collect();
    for (i, l) in lines.iter().enumerate() {
        let Some(head) = l.strip_prefix("Error: assertion failed") else { continue };
        let mut f = TestFailure {
            file: file.to_string(),
            name: None,
            line: None,
            op: if head.starts_with(": ") { "assert".into() } else { "assert_eq".into() },
            expected: None,
            found: None,
            message: l.trim_start_matches("Error: ").to_string(),
        };
        fill_t18_fields(&mut f, &lines[i + 1..]);
        out.push(f);
    }
    out
}

/// The `  key: value` tail of a T18 block. `expected` ends where `  found: `
/// begins; `found` runs to the END OF THE OUTPUT — the desugar orders the
/// fields that way precisely so the one field with no terminator is last, and
/// the abort's `process.exit(1)` guarantees nothing is printed after it.
fn fill_t18_fields(f: &mut TestFailure, rest: &[&str]) {
    let end = rest.iter().position(|l| is_block_end(l)).unwrap_or(rest.len());
    let body = &rest[..end];
    if let Some(at) = body.iter().find_map(|l| l.strip_prefix("  at: line ")) {
        f.line = at.trim().parse().ok();
    }
    let Some(e) = body.iter().position(|l| l.starts_with("  expected: ")) else { return };
    let Some(v) = body.iter().position(|l| l.starts_with("  found: ")) else { return };
    if v < e {
        return;
    }
    f.expected = Some(join_field(&body[e..v], "  expected: "));
    f.found = Some(join_field(&body[v..], "  found: "));
    if f.expected.as_deref().is_some_and(|s| s.starts_with("!= ")) {
        f.op = "assert_ne".into();
    }
}

fn join_field(lines: &[&str], key: &str) -> String {
    let mut v = vec![lines[0].strip_prefix(key).unwrap_or(lines[0]).to_string()];
    v.extend(lines[1..].iter().map(|l| l.to_string()));
    v.join("\n")
}

// ── Test-name recovery ──────────────────────────────────────────────────────

/// `(mangled, original)` for every `test "…"` in the source. Lowering prefixes
/// test fns with `__test_almd_` and the Rust walker then sanitizes the name, so
/// the mapping is only invertible by mangling FORWARD from the source.
fn test_name_map(source: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in source.lines() {
        let t = line.trim_start();
        let Some(rest) = t.strip_prefix("test ") else { continue };
        let Some(rest) = rest.trim_start().strip_prefix('"') else { continue };
        let Some(end) = rest.find('"') else { continue };
        let name = &rest[..end];
        out.push((format!("__test_almd_{}", mangle(name)), name.to_string()));
    }
    out
}

/// The Rust walker's fn-name sanitizer, forward-applied (walker/mod.rs).
fn mangle(name: &str) -> String {
    let s = name
        .replace('+', "_plus_")
        .replace('/', "_div_")
        .replace('*', "_mul_")
        .replace('(', "")
        .replace(')', "")
        .replace('=', "_eq_")
        .replace('!', "_bang_")
        .replace('?', "_q_")
        .replace('<', "_lt_")
        .replace('>', "_gt_")
        .replace('|', "_pipe_")
        .replace('&', "_amp_")
        .replace('%', "_mod_");
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect()
}

/// `tests::__test_almd_string_mismatch` → `string mismatch` when the source
/// offers a match, else the mangled tail (still better than the full path).
fn display_name(names: &[(String, String)], raw: &str) -> String {
    let tail = raw.rsplit("::").next().unwrap_or(raw);
    names
        .iter()
        .find(|(m, _)| m == tail)
        .map(|(_, orig)| orig.clone())
        .unwrap_or_else(|| tail.trim_start_matches("__test_almd_").to_string())
}

// ── Diffing ─────────────────────────────────────────────────────────────────

/// A unit-wise diff of two rendered values. The unit is chosen by SHAPE —
/// string lines, list items, record fields — so one differ serves all three
/// and the report never shows two opaque blobs.
pub struct Diff {
    unit: &'static str,
    ops: Vec<(char, String)>,
}

/// Above this many units the quadratic LCS is not worth it (and the rendered
/// diff would be unreadable anyway) — fall back to the plain value pair.
const MAX_UNITS: usize = 600;
/// Rendered diff rows before elision.
const MAX_ROWS: usize = 30;

impl Diff {
    /// `None` when the values do not decompose into comparable units — a
    /// scalar mismatch reads better as `expected:`/`found:`.
    pub fn of(expected: &str, found: &str) -> Option<Diff> {
        let (unit, e) = units(expected)?;
        let (u2, f) = units(found)?;
        if unit != u2 || e.len() > MAX_UNITS || f.len() > MAX_UNITS {
            return None;
        }
        let ops = lcs_diff(&e, &f);
        ops.iter().any(|(m, _)| *m != ' ').then_some(Diff { unit, ops })
    }

    /// The one-line hint FeedbackEval pairs with the structured data.
    pub fn hint(&self) -> String {
        let e = self.ops.iter().filter(|(m, _)| *m != '+').count();
        let f = self.ops.iter().filter(|(m, _)| *m != '-').count();
        let noun = self.unit;
        if e != f {
            return format!("expected {e} {noun}s, found {f}");
        }
        let at = self.ops.iter().position(|(m, _)| *m != ' ').unwrap_or(0);
        match self.unit {
            "field" => {
                let name = self.ops[at].1.split(':').next().unwrap_or("").trim();
                format!("field `{name}` differs")
            }
            "item" => format!("item {at} differs"),
            _ => format!("line {} differs", at + 1),
        }
    }

    /// Unified-style rows, `-` = expected, `+` = found, elided past
    /// [`MAX_ROWS`] so one runaway value cannot bury the other failures.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (mark, text) in self.ops.iter().take(MAX_ROWS) {
            out.push_str(&format!("    {mark} {text}\n"));
        }
        if self.ops.len() > MAX_ROWS {
            out.push_str(&format!("    … {} more\n", self.ops.len() - MAX_ROWS));
        }
        out
    }
}

/// Split a rendered value into diffable units, or `None` for a scalar.
fn units(v: &str) -> Option<(&'static str, Vec<String>)> {
    let t = v.trim();
    if let Some(s) = unquote_debug(t) {
        return s.contains('\n').then(|| ("line", lines_of(&s)));
    }
    if let Some(inner) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return Some(("item", split_top_level(inner)));
    }
    if let Some(inner) = record_body(t) {
        return Some(("field", split_top_level(inner)));
    }
    v.contains('\n').then(|| ("line", lines_of(v)))
}

fn lines_of(s: &str) -> Vec<String> {
    s.split('\n').map(|l| l.to_string()).collect()
}

/// `Point { x: 1, y: 2 }` → `x: 1, y: 2`.
fn record_body(t: &str) -> Option<&str> {
    let open = t.find(" { ")?;
    let inner = t[open + 3..].strip_suffix(" }")?;
    (!t[..open].contains(' ')).then_some(inner)
}

/// Rust `Debug` string → its contents. `None` when `t` is not one.
fn unquote_debug(t: &str) -> Option<String> {
    let inner = t.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some(other) => out.push(other),
            None => return None,
        }
    }
    Some(out)
}

/// Split on top-level `, ` — nesting brackets and quoted strings are opaque.
fn split_top_level(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut quoted = false;
    let mut esc = false;
    let mut cur = String::new();
    for c in inner.chars() {
        if esc {
            cur.push(c);
            esc = false;
            continue;
        }
        match c {
            '\\' if quoted => esc = true,
            '"' => quoted = !quoted,
            '[' | '{' | '(' if !quoted => depth += 1,
            ']' | '}' | ')' if !quoted => depth -= 1,
            ',' if !quoted && depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() || out.is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// Longest-common-subsequence diff — deterministic for a fixed input pair,
/// which is what makes two runs of a suite diffable against each other.
fn lcs_diff(a: &[String], b: &[String]) -> Vec<(char, String)> {
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] { dp[i + 1][j + 1] + 1 } else { dp[i + 1][j].max(dp[i][j + 1]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((' ', a[i].clone()));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            out.push(('-', a[i].clone()));
            i += 1;
        } else {
            out.push(('+', b[j].clone()));
            j += 1;
        }
    }
    out.extend(a[i..].iter().map(|l| ('-', l.clone())));
    out.extend(b[j..].iter().map(|l| ('+', l.clone())));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIBTEST: &str = "\nrunning 1 test\ntest tests::__test_almd_string_mismatch ... FAILED\n\nfailures:\n\n---- tests::__test_almd_string_mismatch stdout ----\n\nthread 'tests::__test_almd_string_mismatch' panicked at /tmp/x/almide_test_main.rs:1372:9:\nassertion `left == right` failed: at line 4\n  left: \"a\\nb\"\n right: \"a\\nB\"\nnote: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n\n\nfailures:\n    tests::__test_almd_string_mismatch\n\ntest result: FAILED. 0 passed; 1 failed\n";

    #[test]
    fn libtest_block_yields_a_structured_record() {
        let src = "test \"string mismatch\" {\n  assert_eq(a, b)\n}\n";
        let fs = parse("x_test.almd", src, LIBTEST);
        assert_eq!(fs.len(), 1);
        assert_eq!(fs[0].name.as_deref(), Some("string mismatch"));
        assert_eq!(fs[0].line, Some(4));
        assert_eq!(fs[0].op, "assert_eq");
        assert_eq!(fs[0].found.as_deref(), Some("\"a\\nb\""));
        assert_eq!(fs[0].expected.as_deref(), Some("\"a\\nB\""));
    }

    #[test]
    fn the_generated_rs_location_never_reaches_the_report() {
        let fs = parse("x_test.almd", "", LIBTEST);
        let r = fs[0].render();
        assert!(!r.contains("almide_test_main.rs"), "generated-source path leaked:\n{r}");
        assert!(r.contains("x_test.almd:4"), "almd site missing:\n{r}");
    }

    #[test]
    fn t18_abort_block_parses_on_either_target() {
        let out = "before\nError: assertion failed\n  at: line 7\n  expected: 3\n  found: 4\n";
        let fs = parse("m.almd", "", out);
        assert_eq!(fs.len(), 1);
        assert_eq!(fs[0].line, Some(7));
        assert_eq!(fs[0].expected.as_deref(), Some("3"));
        assert_eq!(fs[0].found.as_deref(), Some("4"));
    }

    #[test]
    fn t18_multiline_found_runs_to_the_end_of_the_block() {
        let out = "Error: assertion failed\n  at: line 4\n  expected: one\nTWO\n  found: one\ntwo\n";
        let fs = parse("m.almd", "", out);
        assert_eq!(fs[0].expected.as_deref(), Some("one\nTWO"));
        assert_eq!(fs[0].found.as_deref(), Some("one\ntwo"));
    }

    #[test]
    fn t18_ne_is_recognized_by_its_expected_marker() {
        let out = "Error: assertion failed\n  at: line 2\n  expected: != x\n  found: x\n";
        assert_eq!(parse("m.almd", "", out)[0].op, "assert_ne");
    }

    #[test]
    fn multiline_strings_diff_by_line() {
        let d = Diff::of("\"a\\nB\\nc\"", "\"a\\nb\\nc\"").expect("line diff");
        assert_eq!(d.hint(), "line 2 differs");
        assert_eq!(d.render(), "      a\n    - B\n    + b\n      c\n");
    }

    #[test]
    fn lists_diff_by_item() {
        let d = Diff::of("[1, 2, 3]", "[1, 9, 3]").expect("item diff");
        assert_eq!(d.hint(), "item 1 differs");
        assert_eq!(d.render(), "      1\n    - 2\n    + 9\n      3\n");
    }

    #[test]
    fn a_length_mismatch_is_reported_as_a_count() {
        let d = Diff::of("[1, 2, 3]", "[1, 2]").expect("item diff");
        assert_eq!(d.hint(), "expected 3 items, found 2");
    }

    #[test]
    fn records_diff_by_field() {
        let d = Diff::of("Point { x: 1, y: 2 }", "Point { x: 1, y: 5 }").expect("field diff");
        assert_eq!(d.hint(), "field `y` differs");
        assert_eq!(d.render(), "      x: 1\n    - y: 2\n    + y: 5\n");
    }

    #[test]
    fn nested_commas_do_not_split_items() {
        assert_eq!(split_top_level("[1, 2], [3]"), vec!["[1, 2]", "[3]"]);
        assert_eq!(split_top_level("\"a, b\", c"), vec!["\"a, b\"", "c"]);
    }

    #[test]
    fn scalars_have_no_diff_and_render_as_a_pair() {
        assert!(Diff::of("3", "4").is_none());
        let f = TestFailure {
            file: "m.almd".into(),
            name: Some("sum".into()),
            line: Some(9),
            op: "assert_eq".into(),
            expected: Some("3".into()),
            found: Some("4".into()),
            message: String::new(),
        };
        assert_eq!(f.render(), "  test: sum\n  at:   m.almd:9\n  expected: 3\n  found:    4\n");
    }

    #[test]
    fn json_carries_the_full_record() {
        let f = TestFailure {
            file: "m.almd".into(),
            name: Some("s".into()),
            line: Some(2),
            op: "assert_eq".into(),
            expected: Some("\"a\\nB\"".into()),
            found: Some("\"a\\nb\"".into()),
            message: String::new(),
        };
        let v: serde_json::Value = serde_json::from_str(&f.to_json()).unwrap();
        assert_eq!(v["name"], "s");
        assert_eq!(v["line"], 2);
        assert_eq!(v["file"], "m.almd");
        assert!(v["diff"].as_str().unwrap().contains("- B"));
    }

    #[test]
    fn the_injected_site_never_doubles_as_the_message() {
        assert_eq!(head_op("at line 4"), ("assert", "assertion failed".to_string()));
        assert_eq!(head_op("one exceeds two (at line 8)"), ("assert", "one exceeds two".to_string()));
        assert_eq!(site_line("one exceeds two (at line 8)"), Some(8));
    }

    #[test]
    fn assert_ne_states_the_negation_it_expected() {
        let out = "---- tests::__test_almd_ne stdout ----\nassertion `left != right` failed: at line 12\n  left: 4\n right: 4\n";
        let f = &parse("m.almd", "", out)[0];
        assert_eq!(f.op, "assert_ne");
        assert_eq!(f.expected.as_deref(), Some("!= 4"));
        assert_eq!(f.found.as_deref(), Some("4"));
    }

    #[test]
    fn failures_are_reported_in_source_order() {
        let out = "---- tests::__test_almd_b stdout ----\nassertion `left == right` failed: at line 9\n  left: 1\n right: 2\n---- tests::__test_almd_a stdout ----\nassertion `left == right` failed: at line 3\n  left: 1\n right: 2\n";
        let lines: Vec<_> = parse("m.almd", "", out).iter().map(|f| f.line).collect();
        assert_eq!(lines, vec![Some(3), Some(9)]);
    }

    #[test]
    fn a_bare_panic_keeps_its_payload() {
        let out = "---- tests::__test_almd_boom stdout ----\nthread 'x' panicked at a.rs:1:1:\nPANIC: boom\n";
        let fs = parse("m.almd", "test \"boom\" {}\n", out);
        assert_eq!(fs[0].op, "panic");
        assert_eq!(fs[0].message, "PANIC: boom");
        assert!(fs[0].render().contains("error: PANIC: boom"));
    }

    #[test]
    fn mangled_names_round_trip_through_the_walker_sanitizer() {
        let src = "test \"a + b (fast)\" {\n}\n";
        let names = test_name_map(src);
        assert_eq!(names[0].0, "__test_almd_a__plus__b_fast");
        assert_eq!(display_name(&names, "tests::__test_almd_a__plus__b_fast"), "a + b (fast)");
    }
}
