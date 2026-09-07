//! `almide test --update-snapshots` — the accept step of
//! `testing.assert_snapshot` (#1314).
//!
//! The expectation lives IN the source, as the second argument of the call
//! (the expect-test shape: no sidecar files, the test stays self-contained
//! for the writer). A mismatch aborts the run with the T18 block the frontend
//! desugar prints on every lane:
//!
//! ```text
//! Error: snapshot mismatch
//!   at: line <N>
//!   expected: <literal's value>
//!   found: <actual>
//! ```
//!
//! This module turns that block back into a source edit: parse the file, find
//! the `testing.assert_snapshot` call on line N, and splice a new literal over
//! the OLD one's verbatim spelling (`raw`) — nothing else in the file moves.
//! The new literal is re-lexed before it is written, so a value the chosen
//! spelling cannot carry (a heredoc whose lines share leading whitespace, a
//! value containing `"""`) falls back to the escaped one-line form rather than
//! landing as a literal that reads back differently.

use crate::{ast, lexer};

/// The snapshot record of an aborted run, byte-exact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotMismatch {
    /// 1-based line of the `testing.assert_snapshot` call.
    pub line: usize,
    /// The literal's value as the program printed it (the reporter's view —
    /// `refine_found` re-splits against the source's own literal).
    pub expected: String,
    /// The actual value, verbatim to the end of the output.
    pub found: String,
}

/// The LAST snapshot block in `output` — the abort is the final thing the
/// program prints, and stderr is appended after stdout by the capture, so
/// `found` runs to the end of the text minus `eprintln`'s own newline. Blank
/// lines and a trailing newline in the value survive, which the reporter's
/// line-oriented T18 parse cannot promise.
pub fn parse_snapshot_mismatch(output: &str) -> Option<SnapshotMismatch> {
    const HEAD: &str = "Error: snapshot mismatch\n  at: line ";
    let start = output.rfind(HEAD)?;
    let rest = &output[start + HEAD.len()..];
    let (line, rest) = rest.split_once("\n  expected: ")?;
    let line: usize = line.trim().parse().ok()?;
    let (expected, found) = rest.split_once("\n  found: ")?;
    let found = found.strip_suffix('\n').unwrap_or(found);
    Some(SnapshotMismatch { line, expected: expected.to_string(), found: found.to_string() })
}

/// Re-split `expected`/`found` against the literal the SOURCE holds: the
/// output-side split takes the first `\n  found: `, which is wrong exactly
/// when the expected value itself contains that text. Returns the found
/// value when the block is consistent with `expected_src`, else `None`.
fn refine_found(m: &SnapshotMismatch, expected_src: &str) -> Option<String> {
    if m.expected == expected_src {
        return Some(m.found.clone());
    }
    let joined = format!("{}\n  found: {}", m.expected, m.found);
    let rest = joined.strip_prefix(expected_src)?;
    rest.strip_prefix("\n  found: ").map(|s| s.to_string())
}

/// What one accepted snapshot looked like, for the run's transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewrite {
    pub line: usize,
    /// The expectation was the empty string — a NEW snapshot, not a changed one.
    pub was_new: bool,
}

/// Rewrite the `expected` literal of the `testing.assert_snapshot` call at
/// `line` in `path` to carry `m.found`. The rest of the file is untouched
/// byte-for-byte.
pub fn rewrite_snapshot(path: &str, m: &SnapshotMismatch) -> Result<Rewrite, String> {
    let source = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let rewritten = rewrite_source(&source, m)?;
    std::fs::write(path, rewritten.0).map_err(|e| format!("cannot write {path}: {e}"))?;
    Ok(rewritten.1)
}

/// The pure half of [`rewrite_snapshot`]: `(new source, what changed)`.
pub fn rewrite_source(source: &str, m: &SnapshotMismatch) -> Result<(String, Rewrite), String> {
    let tokens = lexer::Lexer::tokenize(source);
    let mut parser = crate::parser::Parser::new(tokens);
    let mut program = parser.parse().map_err(|e| format!("cannot parse the file: {e}"))?;
    let lit = find_expected_literal(&mut program, m.line)?;
    let found = refine_found(m, &lit.value).ok_or_else(|| {
        format!("the printed expectation does not match the literal on line {}", m.line)
    })?;
    let start = char_offset(source, lit.line, lit.col)
        .ok_or_else(|| format!("line {}:{} is outside the file", lit.line, lit.col))?;
    let tail = &source[start..];
    if !tail.starts_with(&lit.raw) {
        return Err(format!("the literal on line {} is not where the parser said it was", lit.line));
    }
    let indent: String = line_indent(source, m.line);
    let new_raw = render_literal(&found, &indent);
    let mut out = String::with_capacity(source.len() + new_raw.len());
    out.push_str(&source[..start]);
    out.push_str(&new_raw);
    out.push_str(&tail[lit.raw.len()..]);
    Ok((out, Rewrite { line: m.line, was_new: lit.value.is_empty() }))
}

/// The second argument of the `testing.assert_snapshot` call on `line`, as
/// the parser saw it.
struct Literal {
    line: usize,
    col: usize,
    raw: String,
    value: String,
}

fn find_expected_literal(program: &mut ast::Program, line: usize) -> Result<Literal, String> {
    let mut hit: Option<Result<Literal, String>> = None;
    ast::visit_exprs_mut(program, &mut |e: &mut ast::Expr| {
        if hit.is_some() || e.span.map(|s| s.line) != Some(line) {
            return;
        }
        let ast::ExprKind::Call { callee, args, .. } = &e.kind else { return };
        if !is_assert_snapshot(callee) {
            return;
        }
        let Some(arg) = args.get(1) else { return };
        hit = Some(literal_of(arg));
    });
    hit.unwrap_or_else(|| Err(format!("no testing.assert_snapshot call on line {line}")))
}

fn is_assert_snapshot(callee: &ast::Expr) -> bool {
    let ast::ExprKind::Member { object, field } = &callee.kind else { return false };
    field.as_str() == "assert_snapshot"
        && matches!(&object.kind, ast::ExprKind::Ident { name } if name.as_str() == "testing")
}

fn literal_of(arg: &ast::Expr) -> Result<Literal, String> {
    let Some(span) = arg.span else { return Err("the expectation has no source span".into()) };
    match &arg.kind {
        ast::ExprKind::String { value, raw: Some(raw) } => {
            Ok(Literal { line: span.line, col: span.col, raw: raw.clone(), value: value.clone() })
        }
        ast::ExprKind::String { .. } | ast::ExprKind::InterpolatedString { .. } => {
            Err("the expectation is an interpolated string — only a plain literal can be rewritten".into())
        }
        _ => Err("the expectation is not a string literal — only a literal can be rewritten".into()),
    }
}

/// Char offset of a 1-based `(line, col)` — the lexer counts columns in chars.
fn char_offset(source: &str, line: usize, col: usize) -> Option<usize> {
    let mut cur_line = 1;
    let mut cur_col = 1;
    for (i, c) in source.char_indices() {
        if cur_line == line && cur_col == col {
            return Some(i);
        }
        if c == '\n' {
            cur_line += 1;
            cur_col = 1;
        } else {
            cur_col += 1;
        }
    }
    (cur_line == line && cur_col == col).then_some(source.len())
}

/// Leading whitespace of the 1-based `line`.
fn line_indent(source: &str, line: usize) -> String {
    source
        .lines()
        .nth(line.saturating_sub(1))
        .map(|l| l[..l.len() - l.trim_start().len()].to_string())
        .unwrap_or_default()
}

/// The source spelling for `value`: a heredoc for a multi-line value (the
/// readable form — the same text the program printed, indented two past the
/// call), the escaped one-line form otherwise or whenever the heredoc would
/// not read back as `value` (checked by lexing it).
pub fn render_literal(value: &str, indent: &str) -> String {
    if value.contains('\n') {
        let heredoc = render_heredoc(value, indent);
        if lexes_to(&heredoc, value) {
            return heredoc;
        }
    }
    render_quoted(value)
}

fn render_heredoc(value: &str, indent: &str) -> String {
    let inner = format!("{indent}  ");
    let mut out = String::from("\"\"\"\n");
    for line in value.split('\n') {
        if line.is_empty() {
            out.push('\n');
        } else {
            out.push_str(&inner);
            out.push_str(&escape_heredoc(line));
            out.push('\n');
        }
    }
    out.push_str(&inner);
    out.push_str("\"\"\"");
    out
}

/// A heredoc keeps `\n`/`\t`/`\r` raw; only the escape introducer and the
/// interpolation sigil need escaping.
fn escape_heredoc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '$' => out.push_str("\\$"),
            other => out.push(other),
        }
    }
    out
}

fn render_quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '$' => out.push_str("\\$"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Does `literal` lex as ONE plain string token whose value is `value`?
fn lexes_to(literal: &str, value: &str) -> bool {
    let tokens = lexer::Lexer::tokenize(literal);
    let strings: Vec<&lexer::Token> =
        tokens.iter().filter(|t| t.token_type == lexer::TokenType::String).collect();
    let non_string = tokens
        .iter()
        .filter(|t| !matches!(t.token_type, lexer::TokenType::String | lexer::TokenType::EOF | lexer::TokenType::Newline))
        .count();
    strings.len() == 1 && non_string == 0 && strings[0].value == value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mismatch(line: usize, expected: &str, found: &str) -> SnapshotMismatch {
        SnapshotMismatch { line, expected: expected.into(), found: found.into() }
    }

    #[test]
    fn the_block_is_parsed_to_the_end_of_the_output() {
        let out = "test: a\nError: snapshot mismatch\n  at: line 4\n  expected: \n  found: one\n\nthree\n\n";
        let m = parse_snapshot_mismatch(out).unwrap();
        assert_eq!(m.line, 4);
        assert_eq!(m.expected, "");
        assert_eq!(m.found, "one\n\nthree\n");
    }

    #[test]
    fn a_new_snapshot_is_written_as_a_one_line_literal() {
        let src = "import testing\n\ntest \"t\" {\n  testing.assert_snapshot(f(), \"\")\n}\n";
        let (out, rw) = rewrite_source(src, &mismatch(4, "", "a \"b\" $c\\d")).unwrap();
        assert_eq!(out, "import testing\n\ntest \"t\" {\n  testing.assert_snapshot(f(), \"a \\\"b\\\" \\$c\\\\d\")\n}\n");
        assert!(rw.was_new);
    }

    #[test]
    fn a_multi_line_value_becomes_a_heredoc_that_reads_back_exactly() {
        let src = "test \"t\" {\n  testing.assert_snapshot(f(), \"old\")\n}\n";
        let value = "line one\n  indented\n\nlast\n";
        let (out, rw) = rewrite_source(src, &mismatch(2, "old", value)).unwrap();
        assert!(!rw.was_new);
        assert_eq!(
            out,
            "test \"t\" {\n  testing.assert_snapshot(f(), \"\"\"\n    line one\n      indented\n\n    last\n\n    \"\"\")\n}\n"
        );
        let toks = lexer::Lexer::tokenize(&out);
        let lit = toks.iter().filter(|t| t.token_type == lexer::TokenType::String).nth(1).unwrap();
        assert_eq!(lit.value, value);
    }

    #[test]
    fn a_heredoc_that_cannot_carry_the_value_falls_back_to_escapes() {
        // Every line shares leading whitespace — the heredoc would strip it.
        assert_eq!(render_literal("  a\n  b", ""), "\"  a\\n  b\"");
        // The closing delimiter inside the value.
        assert_eq!(render_literal("x\n\"\"\"\ny", ""), "\"x\\n\\\"\\\"\\\"\\ny\"");
    }

    #[test]
    fn an_existing_heredoc_is_replaced_whole() {
        let src = "test \"t\" {\n  testing.assert_snapshot(f(), \"\"\"\n    a\n    \"\"\")\n}\n";
        let (out, _) = rewrite_source(src, &mismatch(2, "a", "b")).unwrap();
        assert_eq!(out, "test \"t\" {\n  testing.assert_snapshot(f(), \"b\")\n}\n");
    }

    #[test]
    fn a_non_literal_expectation_is_refused() {
        let src = "test \"t\" {\n  let e = \"\"\n  testing.assert_snapshot(f(), e)\n}\n";
        let err = rewrite_source(src, &mismatch(3, "", "x")).unwrap_err();
        assert!(err.contains("not a string literal"), "{err}");
    }

    #[test]
    fn an_expected_value_containing_the_found_marker_still_splits_right() {
        let m = parse_snapshot_mismatch("Error: snapshot mismatch\n  at: line 1\n  expected: a\n  found: b\n  found: c\n").unwrap();
        assert_eq!(refine_found(&m, "a\n  found: b").as_deref(), Some("c"));
        assert_eq!(refine_found(&m, "a").as_deref(), Some("b\n  found: c"));
    }
}
