/// Almide parser: Token stream → AST.
///
/// Input:    Vec<Token>
/// Output:   Program (decls, imports)
/// Owns:     syntax validation, operator precedence, ExprId assignment, depth limiting
/// Does NOT: type checking, name resolution, semantic validation

mod compounds;
mod declarations;
mod expressions;
pub mod hints;
mod patterns;
mod primary;
mod statements;
mod types;

use crate::lexer::{Token, TokenType};
use crate::ast::*;
use crate::diagnostic::Diagnostic;

const MAX_DEPTH: usize = 500;

pub struct Parser {
    pub(crate) tokens: Vec<Token>,
    pub(crate) pos: usize,
    pub errors: Vec<Diagnostic>,
    pub(crate) file: Option<String>,
    pub(crate) next_expr_id: u32,
    pub(crate) depth: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0, errors: Vec::new(), file: None, next_expr_id: 0, depth: 0 }
    }

    pub(crate) fn next_id(&mut self) -> ExprId {
        let id = ExprId(self.next_expr_id);
        self.next_expr_id += 1;
        id
    }

    pub fn expr_id_counter(&self) -> u32 { self.next_expr_id }

    pub(crate) fn enter_depth(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            Err("expression nesting too deep (max 500)".to_string())
        } else {
            Ok(())
        }
    }

    pub(crate) fn exit_depth(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    pub fn new_with_id_offset(tokens: Vec<Token>, id_offset: u32) -> Self {
        let mut p = Self::new(tokens);
        p.next_expr_id = id_offset;
        p
    }

    pub fn with_file(mut self, file: &str) -> Self {
        self.file = Some(file.to_string());
        self
    }

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

    pub(crate) fn diag_error(&self, message: impl Into<String>, hint: impl Into<String>, context: impl Into<String>) -> Diagnostic {
        let mut d = Diagnostic::error(message, hint, context);
        let tok = self.current();
        if let Some(f) = &self.file {
            d.file = Some(f.clone());
        }
        d.line = Some(tok.line);
        d.col = Some(tok.col);
        d
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
        let (message, hint) = if let Some(idx) = msg.find("\n  Hint: ") {
            (msg[..idx].to_string(), msg[idx + 9..].to_string())
        } else {
            (msg.to_string(), String::new())
        };
        let mut d = Diagnostic::error(message, hint, "");
        if let Some(f) = &self.file {
            d.file = Some(f.clone());
        }
        d.line = line;
        d.col = col;
        d
    }

    // ── Token helpers ─────────────────────────────────────────────

    pub(crate) fn current_span(&self) -> Span {
        let tok = self.current();
        Span { line: tok.line, col: tok.col }
    }

    pub(crate) fn current(&self) -> &Token {
        if self.pos < self.tokens.len() {
            &self.tokens[self.pos]
        } else if let Some(last) = self.tokens.last() {
            last
        } else {
            static EOF_TOKEN: Token = Token {
                token_type: TokenType::EOF,
                value: String::new(),
                line: 0,
                col: 0,
            };
            &EOF_TOKEN
        }
    }

    pub(crate) fn peek_at(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.pos + offset)
    }

    pub(crate) fn newline_before_current(&self) -> bool {
        if self.pos == 0 { return false; }
        self.tokens[self.pos - 1].line < self.current().line
    }

    pub(crate) fn peek_type_args_call(&self) -> bool {
        if self.current().token_type != TokenType::LBracket { return false; }
        let mut depth = 0;
        let mut i = 0;
        loop {
            let tok = match self.peek_at(i) {
                Some(t) => t,
                None => return false,
            };
            match tok.token_type {
                TokenType::LBracket => depth += 1,
                TokenType::RBracket => {
                    depth -= 1;
                    if depth == 0 {
                        return self.peek_at(i + 1).map(|t| t.token_type == TokenType::LParen).unwrap_or(false);
                    }
                }
                TokenType::EOF => return false,
                _ => {}
            }
            i += 1;
        }
    }

    pub(crate) fn peek_paren_lambda(&self) -> bool {
        if self.current().token_type != TokenType::LParen { return false; }
        let mut depth = 1;
        let mut i = 1;
        loop {
            let tok = match self.peek_at(i) {
                Some(t) => t,
                None => return false,
            };
            match tok.token_type {
                TokenType::LParen => depth += 1,
                TokenType::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        return self.peek_at(i + 1)
                            .map(|t| t.token_type == TokenType::FatArrow)
                            .unwrap_or(false);
                    }
                }
                TokenType::EOF => return false,
                _ => {}
            }
            i += 1;
        }
    }

    pub(crate) fn check(&self, token_type: TokenType) -> bool {
        self.current().token_type == token_type
    }

    pub(crate) fn check_ident(&self, name: &str) -> bool {
        self.current().token_type == TokenType::Ident && self.current().value == name
    }

    pub(crate) fn advance(&mut self) -> &Token {
        let pos = self.pos;
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        &self.tokens[pos]
    }

    pub(crate) fn advance_and_get_value(&mut self) -> String {
        let val = self.current().value.clone();
        self.advance();
        val
    }

    pub(crate) fn expect(&mut self, token_type: TokenType) -> Result<&Token, String> {
        if !self.check(token_type.clone()) {
            let tok = self.current();
            let hint = self.hint_for_expected(&token_type, tok);
            let mut msg = format!(
                "Expected {:?} at line {}:{} (got {:?} '{}')",
                token_type, tok.line, tok.col, tok.token_type, tok.value
            );
            if !hint.is_empty() {
                msg.push_str(&format!("\n  Hint: {}", hint));
            }
            return Err(msg);
        }
        Ok(self.advance())
    }

    fn hint_for_expected(&self, expected: &TokenType, _got: &Token) -> String {
        if let Some(result) = self.check_hint(Some(expected.clone()), hints::HintScope::Expression) {
            result.hint
        } else {
            String::new()
        }
    }

    pub(crate) fn expect_ident(&mut self) -> Result<String, String> {
        if self.check(TokenType::Ident) {
            return Ok(self.advance_and_get_value());
        }
        let tok = self.current();
        let hint = match (&tok.token_type, tok.value.as_str()) {
            (TokenType::Underscore, _) => "\n  Hint: '_' can only be used in match patterns, not as a variable name.",
            (TokenType::Test, _) => "\n  Hint: 'test' is a reserved keyword.",
            _ => "",
        };
        Err(format!(
            "Expected identifier at line {}:{} (got {:?} '{}'){}",
            tok.line, tok.col, tok.token_type, tok.value, hint
        ))
    }

    pub(crate) fn expect_type_name(&mut self) -> Result<String, String> {
        if self.check(TokenType::TypeName) {
            return Ok(self.advance_and_get_value());
        }
        let tok = self.current();
        let hint = if tok.token_type == TokenType::Ident {
            "\n  Hint: Type names must start with an uppercase letter, e.g. Int, String, MyType"
        } else {
            ""
        };
        Err(format!(
            "Expected type name at line {}:{} (got {:?} '{}'){}",
            tok.line, tok.col, tok.token_type, tok.value, hint
        ))
    }

    pub(crate) fn expect_any_name(&mut self) -> Result<String, String> {
        if self.check(TokenType::Ident) || self.check(TokenType::IdentQ) || self.check(TokenType::TypeName) {
            return Ok(self.advance_and_get_value());
        }
        let tok = self.current();
        let hint = match &tok.token_type {
            TokenType::Int | TokenType::Float | TokenType::String => {
                "\n  Hint: Expected a name (identifier), not a literal value"
            }
            _ if tok.value == "=" || tok.value == ":" => {
                "\n  Hint: A name is required before '='. Example: fn my_func() -> Int = ..."
            }
            _ => "",
        };
        Err(format!(
            "Expected name at line {}:{} (got {:?} '{}'){}",
            tok.line, tok.col, tok.token_type, tok.value, hint
        ))
    }

    pub(crate) fn expect_any_fn_name(&mut self) -> Result<String, String> {
        // Convention method: fn Dog.eq(...) → name = "Dog.eq"
        if self.check(TokenType::TypeName)
            && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Dot)
        {
            let type_name = self.advance_and_get_value();
            self.advance(); // skip .
            let method = if self.check(TokenType::Ident) || self.check(TokenType::IdentQ) {
                self.advance_and_get_value()
            } else {
                let tok = self.current();
                return Err(format!("Expected method name after '{}.', got {:?} at line {}:{}", type_name, tok.token_type, tok.line, tok.col));
            };
            return Ok(format!("{}.{}", type_name, method));
        }
        if self.check(TokenType::Ident) || self.check(TokenType::IdentQ) {
            return Ok(self.advance_and_get_value());
        }
        let tok = self.current();
        let hint = if tok.token_type == TokenType::TypeName {
            "\n  Hint: Function names must start with a lowercase letter. Use camelCase, e.g. fn myFunc()"
        } else {
            ""
        };
        Err(format!(
            "Expected function name at line {}:{} (got {:?} '{}'){}",
            tok.line, tok.col, tok.token_type, tok.value, hint
        ))
    }

    pub(crate) fn expect_any_param_name(&mut self) -> Result<String, String> {
        if self.check(TokenType::Ident) || self.check(TokenType::Var) {
            return Ok(self.advance_and_get_value());
        }
        let tok = self.current();
        let hint = if tok.token_type == TokenType::TypeName {
            "\n  Hint: Parameter names must start with a lowercase letter. Example: fn greet(name: String)"
        } else if tok.value == ")" {
            "\n  Hint: Trailing comma before ')' is not allowed"
        } else {
            ""
        };
        Err(format!(
            "Expected parameter name at line {}:{} (got {:?} '{}'){}",
            tok.line, tok.col, tok.token_type, tok.value, hint
        ))
    }

    pub(crate) fn expect_closing(&mut self, close: TokenType, open_line: usize, open_col: usize, context: &str) -> Result<&Token, String> {
        if self.check(close.clone()) { return Ok(self.advance()); }
        let (tok_line, tok_col) = (self.current().line, self.current().col);
        let (close_name, open_name) = match close {
            TokenType::RParen => ("')'", "'('"),
            TokenType::RBracket => ("']'", "'['"),
            TokenType::RBrace => ("'}'", "'{'"),
            _ => ("closing delimiter", "opening delimiter"),
        };
        let msg = format!("Expected {} to close {} opened at line {}:{}", close_name, context, open_line, open_col);
        let hint = format!("Add {} or check for a missing delimiter inside the {}", close_name, context);
        let mut diag = self.diag_error(&msg, &hint, "");
        diag.secondary.push(crate::diagnostic::SecondarySpan {
            line: open_line, col: Some(open_col),
            label: format!("{} opened here", open_name),
        });
        self.errors.push(diag);
        Err(format!("{} at line {}:{}", msg, tok_line, tok_col))
    }

    // ── Newline / comment skipping ────────────────────────────────

    pub(crate) fn skip_newlines(&mut self) {
        while self.check(TokenType::Newline) || self.check(TokenType::Comment) {
            self.advance();
        }
    }

    pub(crate) fn skip_newlines_into_stmts(&mut self, stmts: &mut Vec<Stmt>) {
        while self.check(TokenType::Newline) || self.check(TokenType::Comment) {
            if self.check(TokenType::Comment) {
                stmts.push(Stmt::Comment { text: self.current().value.clone() });
            }
            self.advance();
        }
    }

    pub(crate) fn skip_newlines_collect_comments(&mut self) -> Vec<String> {
        let mut comments = Vec::new();
        while self.check(TokenType::Newline) || self.check(TokenType::Comment) {
            if self.check(TokenType::Comment) {
                comments.push(self.current().value.clone());
            }
            self.advance();
        }
        comments
    }

    // ── Entry points ──────────────────────────────────────────────

    pub fn parse_single_expr(&mut self) -> Result<Expr, String> {
        self.parse_expr()
    }

    pub fn parse(&mut self) -> Result<Program, String> {
        let mut program = Program {
            module: None,
            imports: Vec::new(),
            decls: Vec::new(),
            comment_map: Vec::new(),
        };

        let mut pending = self.skip_newlines_collect_comments();

        // Legacy module declaration
        if self.check(TokenType::Module) {
            program.comment_map.push(std::mem::take(&mut pending));
            let module_decl = self.parse_module_decl()?;
            program.decls.push(module_decl);
            pending = self.skip_newlines_collect_comments();
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
            pending = self.skip_newlines_collect_comments();
        }

        // Top-level declarations with error recovery
        while !self.check(TokenType::EOF) {
            let more = self.skip_newlines_collect_comments();
            pending.extend(more);
            if self.check(TokenType::EOF) { break; }
            program.comment_map.push(std::mem::take(&mut pending));

            match self.parse_top_decl() {
                Ok(decl) => program.decls.push(decl),
                Err(msg) => {
                    self.errors.push(self.string_to_diagnostic(&msg));
                    self.skip_to_next_decl();
                }
            }
            pending = self.skip_newlines_collect_comments();
        }

        if !pending.is_empty() {
            program.comment_map.push(pending);
        }

        if !self.errors.is_empty() && program.decls.is_empty() && program.imports.is_empty() && program.module.is_none() {
            let messages: Vec<String> = self.errors.iter().map(|d| d.display()).collect();
            return Err(messages.join("\n"));
        }

        Ok(program)
    }

    // ── Error recovery ────────────────────────────────────────────

    /// Unified sync-point recovery: skip tokens until a statement or declaration boundary.
    /// Used by block parsers to continue after a syntax error.
    /// `in_block`: true when inside `{ }`, stops at `}`. false for braceless/top-level contexts.
    pub(crate) fn recover_to_sync_point(&mut self, in_block: bool) {
        loop {
            let tt = &self.current().token_type;
            match tt {
                TokenType::EOF => break,
                TokenType::RBrace if in_block => break,
                TokenType::Newline => {
                    self.advance();
                    if matches!(self.current().token_type,
                        // Statement-level sync points
                        TokenType::Let | TokenType::Var | TokenType::Guard
                        | TokenType::If | TokenType::Match | TokenType::For
                        | TokenType::While | TokenType::Do
                        | TokenType::Ident | TokenType::TypeName
                        | TokenType::RBrace | TokenType::EOF
                        // Declaration-level sync points
                        | TokenType::Fn | TokenType::Effect | TokenType::Async
                        | TokenType::Type | TokenType::Test | TokenType::Pub
                        | TokenType::Trait | TokenType::Impl
                        | TokenType::Local | TokenType::Mod
                        | TokenType::Strict | TokenType::At
                    ) {
                        break;
                    }
                }
                _ => { self.advance(); }
            }
        }
    }

    pub(crate) fn skip_to_next_stmt(&mut self) {
        loop {
            let tt = &self.current().token_type;
            match tt {
                TokenType::EOF | TokenType::RBrace => break,
                TokenType::Newline => {
                    self.advance();
                    if matches!(self.current().token_type,
                        TokenType::Let | TokenType::Var | TokenType::Guard
                        | TokenType::If | TokenType::Match | TokenType::For
                        | TokenType::While | TokenType::Do
                        | TokenType::Ident | TokenType::TypeName
                        | TokenType::RBrace | TokenType::EOF
                        | TokenType::Fn | TokenType::Effect | TokenType::Async
                        | TokenType::Type | TokenType::Test | TokenType::Pub
                    ) {
                        break;
                    }
                }
                _ => { self.advance(); }
            }
        }
    }

    fn skip_to_next_decl(&mut self) {
        loop {
            let tt = &self.current().token_type;
            match tt {
                TokenType::EOF => break,
                TokenType::Fn | TokenType::Effect | TokenType::Async
                | TokenType::Pub | TokenType::Local | TokenType::Mod
                | TokenType::Type | TokenType::Trait | TokenType::Impl
                | TokenType::Test | TokenType::Strict | TokenType::At => {
                    break;
                }
                TokenType::Newline => {
                    self.advance();
                    if matches!(self.current().token_type,
                        TokenType::Fn | TokenType::Effect | TokenType::Async
                        | TokenType::Pub | TokenType::Local | TokenType::Mod
                        | TokenType::Type | TokenType::Trait | TokenType::Impl
                        | TokenType::Test | TokenType::Strict | TokenType::At
                        | TokenType::EOF
                    ) {
                        break;
                    }
                }
                _ => { self.advance(); }
            }
        }
    }
}
