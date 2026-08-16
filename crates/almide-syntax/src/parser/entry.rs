/// Parser entry points: parse() and parse_single_expr().

use crate::lexer::TokenType;
use crate::ast::Program;
use super::Parser;

/// Extract the trailing consecutive block of `///` doc comment lines.
fn extract_doc_comment(comments: &[String]) -> Option<String> {
    let total = comments.len();
    let mut start = total;
    while start > 0 && comments[start - 1].starts_with("///") {
        start -= 1;
    }
    if start == total {
        return None;
    }
    let doc_lines: Vec<&str> = comments[start..].iter()
        .map(|c| c.strip_prefix("/// ").or_else(|| c.strip_prefix("///")).unwrap_or(""))
        .collect();
    Some(doc_lines.join("\n"))
}

impl Parser {
    pub fn parse_single_expr(&mut self) -> Result<crate::ast::Expr, String> {
        let expr = self.parse_expr()?;
        // A trailing `: Type` pins the expression's type, exactly as in call-arg
        // position. This matters for string interpolation: `"${[]: List[Int]}"`
        // sub-parses through here, and the annotation is the only thing that
        // gives an empty collection literal a concrete element type. (A `::`
        // is a path separator, not the start of an ascription.)
        if self.check(crate::lexer::TokenType::Colon)
            && self.peek_at(1).map(|t| &t.token_type) != Some(&crate::lexer::TokenType::Colon)
        {
            let span = expr.span;
            self.advance(); // skip ':'
            let ty = self.parse_type_expr()?;
            return Ok(crate::ast::Expr::new(
                self.next_id(),
                span,
                crate::ast::ExprKind::TypeAscription { expr: Box::new(expr), ty },
            ));
        }
        Ok(expr)
    }

    pub fn parse(&mut self) -> Result<Program, String> {
        // #1311 front-end phase accounting (no-op unless `--timings`).
        let _phase = almide_base::profile::phase_scope(almide_base::profile::Phase::Parse);
        self.report_invalid_escapes();
        let mut program = Program {
            dialect: None,
            module: None,
            imports: Vec::new(),
            decls: Vec::new(),
            comment_map: Vec::new(),
            doc_map: Vec::new(),
            blank_lines_map: Vec::new(),
            failed_fn_names: std::collections::HashSet::new(),
            expr_comments: std::collections::HashMap::new(),
        };

        let (mut pending, mut gap_blanks) = self.skip_newlines_collect_comments();

        // File-level dialect stamp: `@dialect(N)`, above everything else.
        // Claimed here rather than left to `parse_attribute` because an
        // attribute consumed by the declaration parser would bind to whatever
        // declaration happens to come first — reordering declarations would
        // silently move the file's stamp. `dialect` is reserved for this
        // position; anywhere else it is rejected by the checker.
        if self.check(TokenType::At)
            && self.peek_at(1).map(|t| t.value.as_str()) == Some("dialect")
        {
            let span = self.current_span();
            match self.parse_attribute() {
                Ok(attr) => {
                    let epoch = match attr.args.first().map(|a| &a.value) {
                        Some(crate::ast::AttrValue::Int { value }) if *value >= 0 => {
                            Some(*value as u32)
                        }
                        _ => None,
                    };
                    match epoch {
                        Some(epoch) => {
                            program.dialect =
                                Some(crate::ast::DialectStamp { epoch, span: Some(span) })
                        }
                        // Shape errors are the checker's to report (it owns the
                        // E-code and the hint); the parser only refuses to
                        // invent a stamp it could not read.
                        None => self.errors.push(self.string_to_diagnostic(
                            "`@dialect` takes one non-negative integer epoch, e.g. `@dialect(1)`",
                        )),
                    }
                }
                Err(msg) => {
                    let d = self.string_to_diagnostic(&msg);
                    self.errors.push(d);
                }
            }
            let (p, b) = self.skip_newlines_collect_comments();
            pending.extend(p);
            gap_blanks = gap_blanks.max(b);
        }

        // Legacy module declaration
        if self.check(TokenType::Module) {
            program.comment_map.push(std::mem::take(&mut pending));
            program.doc_map.push(None);
            program.blank_lines_map.push(0);
            let module_decl = self.parse_module_decl()?;
            program.decls.push(module_decl);
            let (p, b) = self.skip_newlines_collect_comments();
            pending = p;
            gap_blanks = b;
        }

        // Import declarations (with recovery)
        while self.check(TokenType::Import) {
            program.comment_map.push(std::mem::take(&mut pending));
            match self.parse_import_decl() {
                Ok(import) => program.imports.push(import),
                Err(msg) => {
                    self.errors.push(self.string_to_diagnostic(&msg));
                    self.skip_to_next_decl();
                }
            }
            let (p, b) = self.skip_newlines_collect_comments();
            pending = p;
            gap_blanks = b;
        }

        // Top-level declarations with error recovery
        while !self.check(TokenType::EOF) {
            let (more, more_blanks) = self.skip_newlines_collect_comments();
            gap_blanks = gap_blanks.max(more_blanks);
            pending.extend(more);
            if self.check(TokenType::EOF) { break; }

            let doc = extract_doc_comment(&pending);
            program.doc_map.push(doc);
            program.blank_lines_map.push(gap_blanks);
            program.comment_map.push(std::mem::take(&mut pending));

            let pre_err_len = self.errors.len();
            match self.parse_top_decl() {
                Ok(decl) => program.decls.push(decl),
                Err(msg) => {
                    // If parse_top_decl (or anything it called) already pushed
                    // a rich diagnostic, skip the string-form duplicate.
                    if self.errors.len() == pre_err_len {
                        self.errors.push(self.string_to_diagnostic(&msg));
                    }
                    self.skip_to_next_decl();
                }
            }
            let (p, b) = self.skip_newlines_collect_comments();
            pending = p;
            gap_blanks = b;
        }

        if !pending.is_empty() {
            program.comment_map.push(pending);
        }

        if !self.errors.is_empty() && program.decls.is_empty() && program.imports.is_empty() && program.module.is_none() {
            let messages: Vec<String> = self.errors.iter().map(|d| d.display()).collect();
            return Err(messages.join("\n"));
        }

        program.failed_fn_names = std::mem::take(&mut self.failed_fn_names);
        // #1404: hand the resolved expression-comment bindings to the Program.
        // Anything still sitting in `inline_comments` was never claimed by a
        // node — fmt's conservation verifier counts LEXER tokens, so an
        // unclaimed comment still makes fmt refuse rather than vanish.
        program.expr_comments = std::mem::take(&mut self.expr_comments);
        Ok(program)
    }
}
