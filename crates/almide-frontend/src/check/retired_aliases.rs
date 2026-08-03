//! #1075: the E040 retired-alias warning — the checker half of the
//! json.*/value.* namespace retirement. The table of retired names lives in
//! `almide_lang::stdlib_info::RETIRED_DYNAMIC_ALIASES` (shared with
//! `almide fix`'s AST rewrite and the namespace gate); this module turns a
//! resolved call to a retired name into the warning with a
//! machine-applicable rewrite where the callee is spelled qualified.

use super::Checker;

impl Checker {
    /// #1075: E040 deprecation warning for a call to a retired dynamic-surface
    /// alias (`json.null` → `value.null`, `json.as_string` → `value.as_string(...)?`,
    /// `value.get` → `value.field`, …) — table in
    /// `almide_lang::stdlib_info::RETIRED_DYNAMIC_ALIASES`. The rewrite is
    /// machine-applied (`almide fix`) only when the callee's source text is the
    /// qualified spelling itself: UFCS receivers (`v.get(k)`) and selective-import
    /// bare names would be corrupted by a span replace, so they warn display-only.
    pub(crate) fn warn_retired_dynamic_alias(&mut self, name: &str) {
        use almide_lang::stdlib_info::RetiredAliasKind;
        let Some((survivor, kind)) = almide_lang::stdlib_info::retired_dynamic_alias(name) else { return };
        let message = if name == "value.get" {
            format!(
                "value.get is retired — value.field is the same operation, and `get` \
                 means Option everywhere else (map.get / list.get)"
            )
        } else {
            format!(
                "{name} is retired — constructors and accessors live on value.*; \
                 json keeps the format (parse / stringify)"
            )
        };
        let hint = match kind {
            RetiredAliasKind::Rename => format!("rename the call: {survivor}(...)"),
            RetiredAliasKind::RenameAndNarrow => format!(
                "{survivor} returns a Result — append `?` for the Option this call produced: {survivor}(...)?"
            ),
        };
        let mut diag = crate::diagnostic::Diagnostic::warning(message, hint, format!("call to {name}()"))
            .with_code("E040");
        if let Some(span) = self.callee_span_hint {
            let callee_is_qualified_spelling =
                self.source_slice(span).as_deref() == Some(name);
            if callee_is_qualified_spelling {
                match kind {
                    RetiredAliasKind::Rename => {
                        diag = diag.with_try_replace(span.line, span.col, span.end_col, survivor.to_string());
                    }
                    RetiredAliasKind::RenameAndNarrow => {
                        if let Some((call_end_col, rest)) = self.single_line_call_extent(span) {
                            diag = diag.with_try_replace(
                                span.line, span.col, call_end_col,
                                format!("{survivor}{rest}?"),
                            );
                        } else {
                            diag = diag.with_try(format!("{survivor}(...)?"));
                        }
                    }
                }
            }
            diag.line = Some(span.line);
            diag.col = Some(span.col);
            diag.end_col = Some(span.end_col);
        }
        self.emit(diag);
    }

    /// For a callee span, find the exclusive end column of the whole call on
    /// the SAME line — the matching `)` of the argument list — scanning
    /// paren depth quote-aware (string literals in the args are opaque).
    /// Returns `(end_col, text_from_callee_end_to_call_end)`; `None` when the
    /// call spans multiple lines or the text after the callee is not `(`.
    fn single_line_call_extent(&self, callee_span: crate::ast::Span) -> Option<(usize, String)> {
        let text = self.source_text.as_deref()?;
        let line = text.lines().nth(callee_span.line - 1)?;
        let chars: Vec<char> = line.chars().collect();
        let mut i = callee_span.end_col - 1; // 0-based index one past the callee
        if chars.get(i) != Some(&'(') {
            return None;
        }
        let start = i;
        let mut depth = 0usize;
        let mut in_str: Option<char> = None;
        while i < chars.len() {
            let c = chars[i];
            match in_str {
                Some(q) => {
                    if c == '\\' { i += 1; } else if c == q { in_str = None; }
                }
                None => match c {
                    '"' | '\'' => in_str = Some(c),
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            let rest: String = chars[start..=i].iter().collect();
                            return Some((i + 2, rest)); // 1-based exclusive end col
                        }
                    }
                    _ => {}
                },
            }
            i += 1;
        }
        None
    }
}
