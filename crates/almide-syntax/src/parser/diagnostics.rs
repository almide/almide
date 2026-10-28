/// Hint integration and diagnostic helpers for parser error reporting.

use crate::lexer::TokenType;
use crate::diagnostic::Diagnostic;
use super::Parser;
use super::hints;

impl Parser {
    // ── Hint integration ──────────────────────────────────────────

    pub(crate) fn check_hint(&self, expected: Option<TokenType>, scope: hints::HintScope) -> Option<hints::HintResult> {
        let prev = if self.pos > 0 { Some(&self.tokens[self.pos - 1]) } else { None };
        let next = if self.pos + 1 < self.tokens.len() { Some(&self.tokens[self.pos + 1]) } else { None };
        let ctx = hints::HintContext {
            expected,
            got: self.current(),
            prev,
            next,
            scope,
        };
        hints::check_hint(&ctx)
    }

    pub(crate) fn check_hint_or_err(&self, expected: Option<TokenType>, scope: hints::HintScope, default_msg: &str) -> String {
        if let Some(result) = self.check_hint(expected, scope) {
            let tok = self.current();
            let msg = result.message.as_deref().unwrap_or(default_msg);
            format!("{} at line {}:{}\n  Hint: {}", msg, tok.line, tok.col, result.hint)
        } else {
            default_msg.to_string()
        }
    }

    // ── Diagnostic helpers ────────────────────────────────────────

    /// The unknown-character error (#1308): a character no lexer rule matched
    /// (full-width punctuation, invisible Unicode, ...). Pushes the rich
    /// diagnostic and returns the string form for the `Err` channel — callers
    /// that dedupe on `errors.len()` (entry.rs) will skip the string twin.
    pub(crate) fn unknown_char_error(&mut self, value: &str, line: usize, col: usize) -> String {
        let ch = value.chars().next().unwrap_or('\u{FFFD}');
        let msg = format!("Unexpected character '{}' (U+{:04X})", ch, ch as u32);
        // A backtick reaches this arm only as a MALFORMED escape: the lexer
        // declines an escape enclosing no identifier character, rather than
        // building a zero-length Ident (#1457). The generic copy-paste hint
        // would send the reader looking for an invisible character, so name the
        // construct that actually failed.
        let compat = super::caseless::compatibility_hint(ch);
        let hint = if ch == '`' {
            "A backtick escape wraps a name that would otherwise be a keyword — \
             `type`, `protocol`. It takes identifier characters and cannot be empty."
        } else {
            compat.as_deref().unwrap_or(
                "This character is not Almide syntax. Full-width or invisible Unicode \
                 characters often sneak in from copy-paste — delete it, or move it into \
                 a string or comment.",
            )
        };
        let diag = self.diag_error(msg.clone(), hint, "unknown-char");
        self.errors.push(diag);
        format!("{} at line {}:{}", msg, line, col)
    }

    /// The exact range `almide fix` deletes to turn an OCaml-style
    /// `let x = e in body` into Almide's newline-chained form: the `in`
    /// keyword plus the run of blanks up to the token that follows it on the
    /// SAME line.
    ///
    /// Every column here comes from the lexer's own token positions —
    /// nothing is derived by searching the text — so the range can only ever
    /// name the keyword and the blanks after it. When the next token starts
    /// on a later line there is no whitespace to absorb and the range is the
    /// keyword alone; when the token positions are unusable the caller gets
    /// `None` and emits no fix-it rather than guessing a span (#1312).
    pub(crate) fn letin_deletion_span(&self) -> Option<(usize, usize, usize)> {
        let tok = self.current();
        if tok.token_type != TokenType::In { return None; }
        if tok.line == 0 || tok.col == 0 || tok.end_col <= tok.col { return None; }
        let end_col = match self.peek_at(1) {
            Some(next) if next.line == tok.line && next.col > tok.end_col => next.col,
            _ => tok.end_col,
        };
        Some((tok.line, tok.col, end_col))
    }

    pub(crate) fn diag_error(&self, message: impl Into<String>, hint: impl Into<String>, context: impl Into<String>) -> Diagnostic {
        let mut d = Diagnostic::error(message, hint, context);
        let tok = self.current();
        if let Some(f) = &self.file {
            d.file = Some(f.clone());
        }
        d.line = Some(tok.line);
        d.col = Some(tok.col);
        if tok.end_col > tok.col {
            d.end_col = Some(tok.end_col);
        }
        d
    }

    /// E047 (#1264): reject every escape the string decoders declined instead
    /// of letting it through as literal text.
    ///
    /// `"bad:\q"` used to evaluate to the two characters `\` `q` and
    /// `"\u{110000}"` to its own ten-character spelling — no diagnostic, no
    /// trace, the exact silent-reinterpretation shape E024 exists to forbid for
    /// integer literals. Runs ONCE over the whole token stream rather than at
    /// each literal's parse site, so string tokens the grammar consumes outside
    /// expression position (test names, `@extern` symbols, import aliases) are
    /// covered by the same pass.
    pub(crate) fn report_invalid_escapes(&mut self) {
        use crate::lexer::{EscapeIssueKind, TokenType};
        let mut diags = Vec::new();
        for tok in &self.tokens {
            if !matches!(tok.token_type, TokenType::String | TokenType::InterpolatedString) {
                continue;
            }
            let Some(raw) = &tok.raw else { continue };
            for issue in crate::lexer::validate_literal_escapes(raw) {
                let line = tok.line + issue.line_offset;
                let col = if issue.line_offset == 0 { tok.col + issue.col_offset } else { issue.col_offset + 1 };
                let (message, hint) = match issue.kind {
                    EscapeIssueKind::Unknown => (
                        format!("unknown escape sequence `{}` in a string literal", issue.text),
                        "valid escapes are \\n \\t \\r \\\\ \\\" \\$ \\xNN \\u{...} \
                         (and \\' inside '...'); write `\\\\` for a literal backslash"
                            .to_string(),
                    ),
                    EscapeIssueKind::OutOfRange => (
                        format!("`{}` is not a Unicode scalar value", issue.text),
                        "a \\u{...} escape names a codepoint in U+0000..U+D7FF or \
                         U+E000..U+10FFFF — surrogates and anything above U+10FFFF \
                         have no character to denote"
                            .to_string(),
                    ),
                };
                let mut d = Diagnostic::error(message, hint, "string literal")
                    .with_code("E047");
                if let Some(f) = &self.file { d.file = Some(f.clone()); }
                d.line = Some(line);
                d.col = Some(col);
                d.end_col = Some(col + issue.text.chars().count());
                diags.push(d);
            }
        }
        self.errors.extend(diags);
    }

    /// E101 (#4092, #4266): refuse every raw hidden character
    /// (`lexer::is_hidden_source_char`: the bidi controls, U+2028 / U+2029,
    /// the zero-width and other Cf characters, the Tags block, raw C0 / C1
    /// controls other than TAB / LF / CR) anywhere in the file — comments,
    /// string literals, heredocs, stray characters. With one in the source,
    /// the text a reviewer reads is not the text the compiler reads (Trojan
    /// Source, CVE-2021-42574; ASCII smuggling). The escape `\u{202E}` is
    /// unaffected: its spelling is ASCII.
    pub(crate) fn report_hidden_chars(&mut self) {
        let found = std::mem::take(&mut self.hidden_chars);
        for (ch, line, col) in found {
            let message = format!(
                "raw hidden character U+{:04X} {} at line {}, column {}",
                ch as u32,
                crate::lexer::hidden_char_name(ch),
                line,
                col
            );
            let hint = format!(
                "this character makes the source display differently from how it compiles \
                 (Trojan Source), or hides text from a reader that a model still reads. Remove it \
                 from a comment; inside a string literal, write the escape `\\u{{{:04X}}}` if \
                 the character is intended",
                ch as u32
            );
            self.push_e101(message, hint, line, col, 1);
        }
    }

    /// E101 (#4266): an identifier a reader cannot tell from another
    /// (CVE-2021-42694, homoglyph identifiers). Two rules, both from UTS #39
    /// and judged only for identifiers holding a non-ASCII character, as
    /// rustc's `mixed_script_confusables` / `confusable_idents` do:
    ///
    /// - restriction level: a non-ASCII identifier must be at most UTS #39
    ///   §5.2 Moderately Restrictive — one script; Latin with Han, Hiragana
    ///   and Katakana, Han + Bopomofo or Han + Hangul; or Latin with one other
    ///   Recommended script except Cyrillic and Greek — and every character
    ///   must be General Security Profile `Allowed`. `pаypal` with a Cyrillic
    ///   `а` (U+0430) is Latin + Cyrillic: refused.
    /// - confusables: two distinct spellings in one file with the same UTS #39
    ///   skeleton, at least one non-ASCII (`асе` all-Cyrillic beside `ace`).
    ///   The later spelling's first occurrence is the error and names the
    ///   earlier one.
    pub(crate) fn report_confusable_idents(&mut self) {
        use unicode_security::{RestrictionLevel, RestrictionLevelDetection};
        let idents = std::mem::take(&mut self.idents);
        let mut judged: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut by_skeleton: std::collections::HashMap<String, (String, usize)> = std::collections::HashMap::new();
        for (name, line, col) in idents {
            if !judged.insert(name.clone()) {
                continue;
            }
            let width = name.chars().count();
            let non_ascii = !name.is_ascii();
            if non_ascii
                && matches!(
                    name.as_str().detect_restriction_level(),
                    RestrictionLevel::MinimallyRestrictive | RestrictionLevel::Unrestricted
                )
            {
                let message = format!(
                    "identifier `{}` mixes scripts a reader cannot tell apart at line {}, column {}",
                    name, line, col
                );
                let hint = format!(
                    "its non-ASCII characters are {}. An identifier must be written in one script, or \
                     in a combination UTS #39 calls moderately restrictive (Latin with Han, Hiragana, \
                     Katakana, Hangul or Bopomofo, or Latin with one other script except Cyrillic and \
                     Greek); a look-alike letter from another script makes a second name that reads \
                     like the first (homoglyph identifiers, CVE-2021-42694). Write the letter from \
                     the name's own script",
                    describe_non_ascii(&name)
                );
                self.push_e101(message, hint, line, col, width);
                continue;
            }
            let skeleton: String = unicode_security::skeleton(&name).collect();
            match by_skeleton.get(&skeleton) {
                Some((first, first_line)) if non_ascii || !first.is_ascii() => {
                    let message = format!(
                        "identifier `{}` at line {}, column {} looks the same as `{}` (line {})",
                        name, line, col, first, first_line
                    );
                    let odd = if non_ascii { &name } else { first };
                    let hint = format!(
                        "the two names are different identifiers that display alike (UTS #39 \
                         confusables; homoglyph identifiers, CVE-2021-42694), so a reader cannot \
                         tell which one a use names. `{}` holds {}. Rename one of them",
                        odd,
                        describe_non_ascii(odd)
                    );
                    self.push_e101(message, hint, line, col, width);
                }
                Some(_) => {}
                None => {
                    by_skeleton.insert(skeleton, (name, line));
                }
            }
        }
    }

    fn push_e101(&mut self, message: String, hint: String, line: usize, col: usize, width: usize) {
        let mut d = Diagnostic::error(message, hint, "source text").with_code("E101");
        if let Some(f) = &self.file { d.file = Some(f.clone()); }
        d.line = Some(line);
        d.col = Some(col);
        d.end_col = Some(col + width.max(1));
        self.errors.push(d);
    }

    pub(crate) fn string_to_diagnostic(&self, msg: &str) -> Diagnostic {
        let (line, col) = if let Some(idx) = msg.find("at line ") {
            let rest = &msg[idx + 8..];
            let nums: Vec<&str> = rest.splitn(3, |c: char| !c.is_ascii_digit()).collect();
            let l = nums.first().and_then(|s| s.parse::<usize>().ok());
            let c = nums.get(1).and_then(|s| s.parse::<usize>().ok());
            (l, c)
        } else {
            (None, None)
        };
        // A parse-error string without an embedded marker used to land with an
        // EMPTY hint — the one production path that defeated "hint is
        // mandatory by type" (#1471). The targeted hints stay embedded where a
        // site knows something specific; this floor guarantees the reader is
        // never told nothing.
        let (message, hint) = if let Some(idx) = msg.find("\n  Hint: ") {
            (msg[..idx].to_string(), msg[idx + 9..].to_string())
        } else {
            let floor = if msg.contains("got EOF") {
                "The file ended while a form was still open. Count delimiters upward \
                 from here — an unclosed '(', '[', '{' or a dangling operator on the \
                 last line is the usual cause."
            } else {
                "The parser expected a different token here. The message names what it \
                 wanted; check the token just BEFORE this position too — an unclosed \
                 delimiter or an idiom from another language usually breaks the shape \
                 one token earlier (grammar reference: docs/GRAMMAR.md)."
            };
            (msg.to_string(), floor.to_string())
        };
        let mut d = Diagnostic::error(message, hint, "");
        if let Some(f) = &self.file {
            d.file = Some(f.clone());
        }
        d.line = line;
        d.col = col;
        d
    }
}

/// `` `а` U+0430 (Cyrillic), … `` for each distinct non-ASCII character of `name`.
fn describe_non_ascii(name: &str) -> String {
    use unicode_script::UnicodeScript;
    let mut seen = Vec::new();
    for c in name.chars().filter(|c| !c.is_ascii()) {
        if !seen.contains(&c) {
            seen.push(c);
        }
    }
    seen.iter()
        .map(|c| format!("`{}` U+{:04X} ({})", c, *c as u32, c.script().full_name()))
        .collect::<Vec<_>>()
        .join(", ")
}
