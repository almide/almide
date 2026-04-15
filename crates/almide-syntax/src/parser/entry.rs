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
        self.parse_expr()
    }

    pub fn parse(&mut self) -> Result<Program, String> {
        let mut program = Program {
            module: None,
            imports: Vec::new(),
            decls: Vec::new(),
            comment_map: Vec::new(),
            doc_map: Vec::new(),
            blank_lines_map: Vec::new(),
            failed_fn_names: std::collections::HashSet::new(),
        };

        let (mut pending, mut gap_blanks) = self.skip_newlines_collect_comments();

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
            gap_blanks = 0;
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
            gap_blanks = 0;

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
        Ok(program)
    }
}
