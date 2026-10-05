//! #3370: an indented line after a single-expression `fn … =` body.
//!
//! `fn main() -> Unit =` followed by two indented calls is the shape writers
//! from indentation-based languages reach for: the body after `=` is ONE
//! expression, so the second line is read as a new top-level item and fails
//! as "Expected top-level declaration". The parse is correct and stays as it
//! is; this module only names the cause and shows the braced rewrite, built
//! from the tokens the author wrote so that pasting it compiles.

use crate::diagnostic::Diagnostic;
use crate::lexer::{Token, TokenType};
use super::Parser;

/// What `parse_fn_decl_body` saw of the last `= <body>` it parsed: token
/// indices into the filtered stream.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FnBodyShape {
    pub eq_idx: usize,
    pub body_start_idx: usize,
    /// `= { ... }` — a block body. Only a braceless body can overflow.
    pub braced: bool,
}

/// The top-level fn parsed immediately before the current declaration, when
/// its body was a single braceless expression.
#[derive(Debug, Clone)]
pub(crate) struct ExprBodiedFn {
    pub name: String,
    pub effect: bool,
    /// First token of the declaration (`fn`, `effect`, `pub`, `@attr`, ...).
    pub start_idx: usize,
    pub eq_idx: usize,
    pub body_start_idx: usize,
}

impl Parser {
    /// The braces diagnostic for the token the top-level loop could not start
    /// a declaration with, or `None` when the shape is not the #3370 one:
    /// the previous item must be a braceless-bodied fn, and the token must
    /// open its own line at a column deeper than that fn's declaration.
    pub(crate) fn braceless_body_overflow(&self) -> Option<Diagnostic> {
        let prev = self.preceding_expr_fn.as_ref()?;
        let tok = self.current();
        let decl = self.tokens.get(prev.start_idx)?;
        let first_on_line = self.pos > 0 && self.tokens[self.pos - 1].line < tok.line;
        if !first_on_line || tok.col <= decl.col || tok.line == 0 {
            return None;
        }
        let kw = if prev.effect { "effect fn" } else { "fn" };
        let body_end_line = self.tokens[..self.pos]
            .iter()
            .rev()
            .find(|t| !matches!(t.token_type, TokenType::Newline | TokenType::Comment))
            .map_or(tok.line, |t| t.line);
        let mut d = self.diag_error(
            format!(
                "this indented line is not part of `{kw} {}`: a body without braces is a single expression",
                prev.name
            ),
            format!(
                "`{kw} {}` ends after its first expression (line {body_end_line}), so line {} is read as a \
                 new top-level declaration. To run several statements, wrap the body in braces",
                prev.name, tok.line
            ),
            "fn body",
        )
        .with_secondary(decl.line, Some(decl.col), "declared here");
        match self.braced_rewrite(prev) {
            Some(rewrite) => {
                d.hint.push(':');
                d = d.with_try(rewrite);
            }
            None => {
                d.hint.push_str(&format!(": `{kw} {}(...) -> Type = {{ ... }}`", prev.name));
            }
        }
        Some(d)
    }

    /// The fn rewritten with its body in braces: every source line from the
    /// declaration through the indented run that starts at the current token,
    /// reproduced from token positions, with ` {` after the `=` and a closing
    /// `}` at the declaration's indentation. `None` when a token's spelling
    /// cannot be reproduced exactly (a multi-line literal or comment, an
    /// inline comment the parser filtered out) — no guessed text is offered.
    fn braced_rewrite(&self, prev: &ExprBodiedFn) -> Option<String> {
        let decl_col = self.tokens.get(prev.start_idx)?.col;
        let end_idx = self.indented_run_end(decl_col);
        if (prev.start_idx..end_idx).any(|i| self.inline_comments.contains_key(&i)) {
            return None;
        }
        let eq = self.tokens.get(prev.eq_idx)?;
        let body_indent = self.current().col - 1;
        let mut out: Vec<String> = Vec::new();
        let mut line_no = self.tokens[prev.start_idx].line;
        let mut buf = String::new();
        let mut buf_col = 1usize;
        let mut body_shift: Option<usize> = None;
        for i in prev.start_idx..end_idx {
            let t = &self.tokens[i];
            if matches!(t.token_type, TokenType::Newline | TokenType::EOF) {
                continue;
            }
            let text = token_text(t)?;
            if t.line != line_no {
                out.push(std::mem::take(&mut buf));
                for _ in line_no + 1..t.line {
                    out.push(String::new());
                }
                line_no = t.line;
                buf_col = 1;
                body_shift = None;
            }
            // The body begins on the `=` line: break it onto its own line,
            // indented like the run below so the block reads as one body.
            if i == prev.body_start_idx && t.line == eq.line {
                out.push(std::mem::take(&mut buf));
                buf.push_str(&" ".repeat(body_indent));
                buf_col = body_indent + 1;
                body_shift = Some(t.col.saturating_sub(1 + body_indent));
            }
            let col = t.col - body_shift.unwrap_or(0);
            if col > buf_col {
                buf.push_str(&" ".repeat(col - buf_col));
                buf_col = col;
            }
            buf.push_str(&text);
            buf_col += text.chars().count();
            if i == prev.eq_idx {
                buf.push_str(" {");
                buf_col += 2;
            }
        }
        out.push(buf);
        while out.last().is_some_and(|l| l.trim().is_empty()) {
            out.pop();
        }
        out.push(format!("{}}}", " ".repeat(decl_col - 1)));
        Some(out.join("\n"))
    }

    /// One past the last token of the run of lines, starting at the current
    /// token, whose first token sits deeper than `decl_col` — the lines the
    /// author indented as part of the body. Blank lines inside the run belong
    /// to it; the run ends at the first line that dedents, or at EOF.
    fn indented_run_end(&self, decl_col: usize) -> usize {
        let mut end = self.pos;
        let mut i = self.pos;
        let mut line_start = true;
        while let Some(t) = self.tokens.get(i) {
            match t.token_type {
                TokenType::EOF => break,
                TokenType::Newline => line_start = true,
                _ => {
                    if line_start && t.col <= decl_col {
                        break;
                    }
                    line_start = false;
                    end = i + 1;
                }
            }
            i += 1;
        }
        end
    }
}

/// The token's exact source spelling, or `None` when it cannot be reproduced
/// on one line at its recorded width.
fn token_text(t: &Token) -> Option<String> {
    let text = t.raw.clone().unwrap_or_else(|| t.value.clone());
    if text.contains('\n') || text.chars().count() != t.end_col.saturating_sub(t.col) {
        return None;
    }
    Some(text)
}
