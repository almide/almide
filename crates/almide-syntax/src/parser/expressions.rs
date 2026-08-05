use crate::lexer::TokenType;
use crate::ast::*;
use crate::ast::ExprKind;
use crate::intern::{Sym, sym};
use super::Parser;

impl Parser {
    pub(crate) fn parse_expr(&mut self) -> Result<Expr, String> {
        self.enter_depth()?;
        let result = self.parse_expr_bp(0);
        self.exit_depth();
        result
    }

    /// Kept as public entry point for callers that expect parse_or level
    /// (e.g. parse_if condition). Now identical to parse_expr_bp(0).
    pub(crate) fn parse_or(&mut self) -> Result<Expr, String> {
        self.parse_expr_bp(0)
    }

    // ── Pratt parser core ───────────────────────────────────────

    /// Binding powers for infix operators.
    /// Returns (left_bp, right_bp). left_bp < right_bp = left-assoc.
    fn infix_bp(tt: &TokenType) -> Option<(u8, u8)> {
        match tt {
            //                             left  right
            TokenType::Or               => Some((2,  3)),   // left-assoc
            TokenType::And              => Some((4,  5)),   // left-assoc
            TokenType::EqEq  | TokenType::BangEq
            | TokenType::LAngle | TokenType::RAngle
            | TokenType::LtEq  | TokenType::GtEq
                                        => Some((6,  7)),   // non-assoc (enforced below)
            TokenType::PipeArrow        => Some((8,  25)),  // ★ asymmetric
            TokenType::DotDotLt         => Some((10, 10)),  // range, end-exclusive (#966)
            TokenType::DotDotDot        => Some((10, 10)),  // range, end-inclusive — infix only;
                                                            // prefix `...` is spread, consumed by the
                                                            // record/list item parsers before this table
            TokenType::DotDot           => Some((10, 10)),  // RETIRED range spelling → E031 fix-it
            TokenType::DotDotEq         => Some((10, 10)),  // RETIRED range spelling → E031 fix-it
            TokenType::Plus | TokenType::Minus
            | TokenType::PlusPlus       => Some((12, 13)),  // left-assoc
            TokenType::Star | TokenType::Slash
            | TokenType::Percent        => Some((14, 15)),  // left-assoc
            TokenType::Caret            => Some((17, 16)),  // right-assoc
            TokenType::ComposeArrow     => Some((25, 26)),  // left-assoc, inside |>'s right
            _ => None,
        }
    }

    /// All token types that are infix operators (for newline lookahead).
    /// `DotDotDot` is deliberately ABSENT: a line starting with `...` is a
    /// spread inside a multiline record/list literal, and joining it to the
    /// previous item as an inclusive range would rewrite those programs. An
    /// inclusive range therefore cannot break the line BEFORE its `...`.
    const INFIX_TOKENS: &'static [TokenType] = &[
        TokenType::Or, TokenType::And,
        TokenType::EqEq, TokenType::BangEq, TokenType::LtEq, TokenType::GtEq,
        TokenType::PipeArrow, TokenType::ComposeArrow,
        TokenType::DotDot, TokenType::DotDotEq, TokenType::DotDotLt,
        TokenType::Plus, TokenType::Minus, TokenType::PlusPlus,
        TokenType::Star, TokenType::Slash, TokenType::Percent,
        TokenType::Caret,
    ];

    fn parse_expr_bp(&mut self, min_bp: u8) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;

        loop {
            // Allow operators on next line for multiline expressions
            let saved_before_skip = self.pos;
            self.skip_newlines_if_followed_by_any(Self::INFIX_TOKENS);
            // Disambiguate: a leading `-` on a new line that is attached to its
            // operand (no whitespace, e.g. `-1`, `-x`) is a unary on a new
            // statement, not binary continuation. `a\n  - b` (with spaces) still
            // continues as binary subtraction.
            if self.pos != saved_before_skip
                && self.check(TokenType::Minus)
                && self.minus_attached_to_operand()
            {
                self.pos = saved_before_skip;
            }

            // Error hints for invalid operators
            if self.check(TokenType::PipePipe) {
                return Err(self.check_hint_or_err(
                    None, super::hints::HintScope::Expression,
                    "'||' is not valid in Almide",
                ));
            }
            if self.check(TokenType::AmpAmp) {
                return Err(self.check_hint_or_err(
                    None, super::hints::HintScope::Expression,
                    "'&&' is not valid in Almide",
                ));
            }

            let tt = self.current().token_type.clone();
            let Some((l_bp, r_bp)) = Self::infix_bp(&tt) else { break };
            if l_bp < min_bp { break; }

            let span = Some(self.current_span());
            let op_tok = self.current().clone();
            self.advance();
            self.skip_newlines();

            // ── Special: |> match { ... } ──
            if tt == TokenType::PipeArrow
                && self.check(TokenType::Match)
                && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::LBrace)
            {
                left = self.parse_pipe_match(left, span)?;
                continue;
            }

            let right = self.parse_expr_bp(r_bp)?;

            // ── Build AST node ──
            left = self.build_infix_node(tt, span, &op_tok, left, right);

            // ── Reject chained comparisons: a < b < c ──
            let is_cmp = |t: &TokenType| matches!(t, TokenType::EqEq | TokenType::BangEq
                | TokenType::LAngle | TokenType::RAngle
                | TokenType::LtEq | TokenType::GtEq);
            if is_cmp(&tt) && is_cmp(&self.current().token_type) {
                let tok = self.current();
                return Err(format!(
                    "Chained comparison operators are not allowed at line {}:{}\n  Hint: Use 'and' to combine comparisons. Write: a < b and b < c",
                    tok.line, tok.col
                ));
            }

            // ── Reject chained ranges: 0..1..2 ──
            // `..` is non-associative for the same reason `<` is: there is no
            // reading of `0..1..2` that means anything. It used to parse (`..`
            // binds right, so as `0..(1..2)`), pass `almide check`, and only
            // fall over in codegen as a rustc type error in generated code
            // (#899). Inspecting the built node rather than the next token
            // catches it whichever way the operator associates.
            if let ExprKind::Range { start, end, .. } = &left.kind {
                if matches!(start.kind, ExprKind::Range { .. }) || matches!(end.kind, ExprKind::Range { .. }) {
                    let (line, col) = left.span.map(|s| (s.line, s.col)).unwrap_or((0, 0));
                    return Err(format!(
                        "Chained range operators are not allowed at line {}:{}\n  Hint: A range has one start and one end. Write: 0..<2",
                        line, col
                    ));
                }
            }
        }

        Ok(left)
    }

    /// Whether the current `Minus` token is immediately followed by its operand
    /// with no whitespace (e.g. `-1`, `-x`). Used to disambiguate a leading `-`
    /// on a new line: attached → unary on a new statement; spaced → binary
    /// continuation of the previous expression.
    fn minus_attached_to_operand(&self) -> bool {
        if !self.check(TokenType::Minus) { return false; }
        let cur = self.current();
        let Some(next) = self.peek_at(1) else { return false; };
        next.line == cur.line && next.col == cur.end_col
    }

    /// `left |> match { arms }` — pipe into a subject-less match: the piped
    /// value becomes the subject.
    fn parse_pipe_match(&mut self, left: Expr, span: Option<Span>) -> Result<Expr, String> {
        self.advance(); // consume 'match'
        self.skip_newlines();
        self.expect(TokenType::LBrace)?;
        self.skip_newlines();
        let mut arms = Vec::new();
        while !self.check(TokenType::RBrace) {
            arms.push(self.parse_match_arm()?);
            self.skip_newlines();
            if self.check(TokenType::Comma) { self.advance(); self.skip_newlines(); }
        }
        self.expect(TokenType::RBrace)?;
        Ok(Expr::new(self.next_id(), span, ExprKind::Match {
            subject: Box::new(left), arms,
        }))
    }

    /// Build the infix AST node for `left <op> right`. `op_tok` is the
    /// operator token — its value names Binary ops, its position anchors the
    /// E031 retired-range tombstone's machine-applicable fix-it.
    fn build_infix_node(
        &mut self,
        tt: TokenType,
        span: Option<Span>,
        op_tok: &crate::lexer::Token,
        left: Expr,
        right: Expr,
    ) -> Expr {
        match tt {
            TokenType::PipeArrow => Expr::new(self.next_id(), span, ExprKind::Pipe {
                left: Box::new(left), right: Box::new(right),
            }),
            TokenType::ComposeArrow => Expr::new(self.next_id(), span, ExprKind::Compose {
                left: Box::new(left), right: Box::new(right),
            }),
            TokenType::DotDotLt => Expr::new(self.next_id(), span, ExprKind::Range {
                start: Box::new(left), end: Box::new(right), inclusive: false,
            }),
            TokenType::DotDotDot => Expr::new(self.next_id(), span, ExprKind::Range {
                start: Box::new(left), end: Box::new(right), inclusive: true,
            }),
            // #966: the Rust-style range spellings were retired for the
            // self-describing Swift pair. The range still PARSES — `almide
            // fix`/`fmt` need the AST to migrate the file, and downstream
            // sees the program — but the E031 tombstone makes every
            // compile path fail loudly, with a machine-applicable fix-it
            // on the operator span.
            TokenType::DotDot | TokenType::DotDotEq => {
                let (op_line, op_col, op_end_col) = (op_tok.line, op_tok.col, op_tok.end_col);
                let inclusive = tt == TokenType::DotDotEq;
                let new_spelling = if inclusive { "..." } else { "..<" };
                let mut d = crate::diagnostic::Diagnostic::error(
                    format!(
                        "'{}' was retired: {} ranges are written '{}'",
                        op_tok.value,
                        if inclusive { "end-inclusive" } else { "end-exclusive" },
                        new_spelling,
                    ),
                    "run `almide fix` on this file to migrate every range mechanically",
                    "range expression",
                ).with_code("E031")
                 .with_try_replace(op_line, op_col, op_end_col, new_spelling);
                if let Some(f) = &self.file { d.file = Some(f.clone()); }
                d.line = Some(op_line);
                d.col = Some(op_col);
                d.end_col = Some(op_end_col);
                self.errors.push(d);
                Expr::new(self.next_id(), span, ExprKind::Range {
                    start: Box::new(left), end: Box::new(right), inclusive,
                })
            }
            _ => Expr::new(self.next_id(), span, ExprKind::Binary {
                op: sym(&op_tok.value), left: Box::new(left), right: Box::new(right),
            }),
        }
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        if self.check(TokenType::Minus) {
            let span = Some(self.current_span());
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(Expr::new(self.next_id(), span, ExprKind::Unary {
                op: sym("-"), operand: Box::new(operand),
            }));
        }
        if self.check(TokenType::Not) {
            let span = Some(self.current_span());
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(Expr::new(self.next_id(), span, ExprKind::Unary {
                op: sym("not"), operand: Box::new(operand),
            }));
        }
        if self.check(TokenType::Bang) {
            let tok = self.current();
            return Err(format!(
                "'!' is not valid in Almide at line {}:{}\n  Hint: Use 'not' for boolean negation. Write: not x",
                tok.line, tok.col
            ));
        }
        self.parse_postfix()
    }

    pub(crate) fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;
        loop {
            // `obj\n  .method()` — the leading-dot half of the chain
            // continuation rule (#1091).
            self.skip_newlines_if_method_chain();
            if self.check(TokenType::Dot) {
                expr = self.parse_postfix_dot(expr)?;
            } else if self.check(TokenType::LBracket) && self.peek_type_args_call() {
                expr = self.parse_postfix_type_args_call(expr)?;
            } else if self.check(TokenType::LBracket) && !self.newline_before_current() {
                expr = self.parse_postfix_index(expr)?;
            } else if self.check(TokenType::LParen) && !self.newline_before_current() {
                expr = self.parse_postfix_call(expr)?;
            } else {
                let (next, consumed) = self.parse_postfix_unwrap_op(expr)?;
                expr = next;
                if !consumed { break; }
            }
        }
        Ok(expr)
    }

    /// `expr[T](args)` — a call with explicit type arguments.
    fn parse_postfix_type_args_call(&mut self, expr: Expr) -> Result<Expr, String> {
        let span = Some(self.current_span());
        let ta = self.parse_type_args()?;
        self.expect(TokenType::LParen)?;
        let (args, named_args) = self.parse_call_args()?;
        self.expect(TokenType::RParen)?;
        Ok(Expr::new(self.next_id(), span, ExprKind::Call {
            callee: Box::new(expr), args, named_args, type_args: Some(ta),
        }))
    }

    /// `expr[index]` — index access.
    fn parse_postfix_index(&mut self, expr: Expr) -> Result<Expr, String> {
        let span = Some(self.current_span());
        let open = self.current().clone();
        self.advance();
        let index = self.parse_expr()?;
        self.expect_closing(TokenType::RBracket, open.line, open.col, "index access")?;
        Ok(Expr::new(self.next_id(), span, ExprKind::IndexAccess {
            object: Box::new(expr), index: Box::new(index),
        }))
    }

    /// The unwrap-family postfixes: `expr!` (propagating unwrap),
    /// `expr ?? fallback`, `expr?.field` (optional chain), `expr?`
    /// (Result→Option). Returns `(expr, consumed)`; `consumed = false` hands
    /// the unchanged expr back to the caller's loop to break.
    fn parse_postfix_unwrap_op(&mut self, expr: Expr) -> Result<(Expr, bool), String> {
        if self.check(TokenType::Bang) && !self.newline_before_current() {
            // expr! — unwrap with error propagation
            let span = Some(self.current_span());
            self.advance();
            return Ok((Expr::new(self.next_id(), span, ExprKind::Unwrap {
                expr: Box::new(expr),
            }), true));
        }
        if self.check(TokenType::QuestionQuestion) {
            // expr ?? fallback — unwrap with default
            let span = Some(self.current_span());
            let qq_tok = (self.current().line, self.current().col, self.current().end_col);
            let qq_after_newline = self.newline_before_current();
            self.advance();
            self.skip_newlines();
            // Terminal `??` — nothing that can start an expression follows.
            if matches!(self.current().token_type, TokenType::RBrace | TokenType::EOF) {
                let mut d = crate::diagnostic::Diagnostic::error(
                    "`??` is missing its fallback operand",
                    "Write the default after ??: `expr ?? fallback`",
                    "?? fallback",
                ).with_code("E038");
                if let Some(f) = &self.file { d.file = Some(f.clone()); }
                d.line = Some(qq_tok.0);
                d.col = Some(qq_tok.1);
                d.end_col = Some(qq_tok.2);
                self.errors.push(d);
                return Ok((expr, true));
            }
            // #1112: at statement level (delimiter depth 0) the fallback must
            // start on the SAME line as `??`, and `??` on the same line as its
            // operand — otherwise the next statement is silently swallowed as
            // the fallback (`let v = f()??\n-5` parsed as `f() ?? -5`, and a
            // following `println(v)` became the fallback expr, surfacing as a
            // distant E003). Multiline fallbacks are spelled with parens.
            if self.delim_depth == 0
                && (qq_after_newline || self.current().line != qq_tok.0)
            {
                let mut d = crate::diagnostic::Diagnostic::error(
                    "the ?? fallback is not on this line",
                    "Write the fallback on the same line as `??`; for a multiline \
                     fallback wrap the whole expression in parentheses with `??` \
                     trailing:\n        (expr ??\n          fallback)",
                    "?? fallback",
                ).with_code("E038");
                if let Some(f) = &self.file { d.file = Some(f.clone()); }
                d.line = Some(qq_tok.0);
                d.col = Some(qq_tok.1);
                d.end_col = Some(qq_tok.2);
                self.errors.push(d);
            }
            let fallback = self.parse_unary()?;
            return Ok((Expr::new(self.next_id(), span, ExprKind::UnwrapOr {
                expr: Box::new(expr), fallback: Box::new(fallback),
            }), true));
        }
        if self.check(TokenType::QuestionDot) && !self.newline_before_current() {
            // expr?.field — optional chaining
            let span = Some(self.current_span());
            self.advance();
            let field = self.expect_any_name()?;
            return Ok((Expr::new(self.next_id(), span, ExprKind::OptionalChain {
                expr: Box::new(expr), field,
            }), true));
        }
        if self.check(TokenType::Question) && !self.newline_before_current() {
            // expr? — convert to Option
            let span = Some(self.current_span());
            self.advance();
            return Ok((Expr::new(self.next_id(), span, ExprKind::ToOption {
                expr: Box::new(expr),
            }), true));
        }
        Ok((expr, false))
    }

    /// Handles the `.` postfix branch: tuple index (`expr.0`), field member
    /// access (`expr.field`), and module-qualified record literals
    /// (`module.TypeName { .. }`). Takes/returns the loop-carried postfix
    /// accumulator so the caller's loop shape stays intact.
    fn parse_postfix_dot(&mut self, expr: Expr) -> Result<Expr, String> {
        let dot_span = self.current_span();
        self.advance();
        // `obj.\n  method()` — the trailing-dot half of the chain continuation
        // rule (#1091). Once the `.` is consumed a continuation is mandatory,
        // so the newlines cannot be significant and the skip is unconditional
        // (same shape as the trailing-operator skip in `parse_expr_bp`).
        self.skip_newlines();
        if self.check(TokenType::Int) {
            let idx_str = self.current().value.clone();
            self.advance();
            let index = idx_str.parse::<usize>().map_err(|_| {
                format!("invalid tuple index '{}' at line {:?}", idx_str, dot_span)
            })?;
            return Ok(Expr::new(self.next_id(), Some(dot_span), ExprKind::TupleIndex {
                object: Box::new(expr), index,
            }));
        }
        // Capture the field's token span BEFORE `expect_any_name`
        // advances past it. The Member's span then covers the
        // full `object.field` — from the object's starting
        // column to the field token's end column (same-line
        // assumption: Almide's lexer never emits a Dot across
        // lines, since `.` before a newline is a syntax error).
        // This precise span powers E002's `try_replace` for
        // rename suggestions like `string.length` → `string.len`.
        let field_span = self.current_span();
        let field = self.expect_any_name()?;
        let full_span = match expr.span {
            Some(obj_span) if obj_span.line == field_span.line => Span {
                line: field_span.line,
                col: obj_span.col,
                end_col: field_span.end_col,
            },
            _ => field_span,
        };
        // Module-qualified record literal: module.TypeName { field: value }
        // Detected when: the object is an Ident, the field starts with
        // uppercase, and the next token is { followed by ident: (record pattern)
        let is_qualified_record = field.as_str().starts_with(char::is_uppercase)
            && matches!(&expr.kind, ExprKind::Ident { .. })
            && self.peek_named_record();
        if is_qualified_record {
            return self.parse_postfix_qualified_record(expr, field, full_span);
        }
        Ok(Expr::new(self.next_id(), Some(full_span), ExprKind::Member {
            object: Box::new(expr), field,
        }))
    }

    /// Parses the `{ .. }` body of a module-qualified record literal
    /// (`module.TypeName { .. }` or `module.TypeName { ...base, .. }`),
    /// reached from `parse_postfix_dot` once the qualified-record shape is
    /// confirmed. `expr` is the already-consumed `module` ident, used only
    /// to derive the qualified name.
    fn parse_postfix_qualified_record(&mut self, expr: Expr, field: Sym, full_span: Span) -> Result<Expr, String> {
        // Compose the qualified name: "module.TypeName"
        let module_name = match &expr.kind {
            ExprKind::Ident { name } => name.as_str().to_string(),
            _ => String::new(),
        };
        let qualified = sym(&format!("{}.{}", module_name, field));
        let open_rec = self.current().clone();
        self.advance(); // skip {
        self.skip_newlines();
        if self.check(TokenType::DotDotDot) {
            self.advance();
            let base = self.parse_expr()?;
            let mut fields = Vec::new();
            while self.check(TokenType::Comma) {
                self.advance(); self.skip_newlines();
                if self.check(TokenType::RBrace) { break; }
                let fname = self.expect_any_name()?;
                self.expect(TokenType::Colon)?;
                self.skip_newlines();
                let fval = self.parse_expr()?;
                fields.push(FieldInit { name: fname, value: fval });
            }
            self.skip_newlines();
            self.expect_closing(TokenType::RBrace, open_rec.line, open_rec.col, "spread record")?;
            Ok(Expr::new(self.next_id(), Some(full_span), ExprKind::SpreadRecord {
                base: Box::new(base), fields,
            }))
        } else {
            let mut fields = Vec::new();
            while !self.check(TokenType::RBrace) {
                self.skip_newlines();
                let fname = self.expect_any_name()?;
                if self.check(TokenType::Colon) {
                    self.advance(); self.skip_newlines();
                    let fval = self.parse_expr()?;
                    fields.push(FieldInit { name: fname, value: fval });
                } else {
                    fields.push(FieldInit {
                        name: fname.clone(),
                        value: Expr::new(self.next_id(), None, ExprKind::Ident { name: fname }),
                    });
                }
                self.skip_newlines();
                if self.check(TokenType::Comma) { self.advance(); self.skip_newlines(); }
            }
            self.expect_closing(TokenType::RBrace, open_rec.line, open_rec.col, "record construction")?;
            Ok(Expr::new(self.next_id(), Some(full_span), ExprKind::Record { name: Some(qualified), fields }))
        }
    }

    /// Handles the `(` postfix branch: a call on the current postfix
    /// accumulator. Takes/returns `expr` like `parse_postfix_dot`.
    fn parse_postfix_call(&mut self, expr: Expr) -> Result<Expr, String> {
        let open = self.current().clone();
        let open_span = self.current_span();
        self.advance();
        let (args, named_args) = self.parse_call_args()?;
        // Capture the closing `)` span BEFORE `expect_closing`
        // so we can compute the full call range (callee-start ..
        // `)`-end) even on single-line calls. Multi-line calls
        // fall back to the `(`-span since `Span` is single-line.
        let close_span = self.current_span();
        self.expect_closing(TokenType::RParen, open.line, open.col, "function call")?;
        let full_span = match (expr.span, close_span.line == open_span.line) {
            (Some(callee_span), true) if callee_span.line == open_span.line => Some(Span {
                line: callee_span.line,
                col: callee_span.col,
                end_col: close_span.end_col,
            }),
            _ => Some(open_span),
        };
        Ok(Expr::new(self.next_id(), full_span, ExprKind::Call {
            callee: Box::new(expr), args, named_args, type_args: None,
        }))
    }

    pub(crate) fn parse_call_args(&mut self) -> Result<(Vec<Expr>, Vec<(Sym, Expr)>), String> {
        let mut args = Vec::new();
        let mut named_args = Vec::new();
        self.skip_newlines();
        if self.check(TokenType::RParen) { return Ok((args, named_args)); }
        self.parse_one_call_arg(&mut args, &mut named_args)?;
        while self.check(TokenType::Comma) {
            self.advance();
            self.skip_newlines();
            if self.check(TokenType::RParen) { break; }
            self.parse_one_call_arg(&mut args, &mut named_args)?;
        }
        self.skip_newlines();
        if !self.check(TokenType::RParen) && !self.check(TokenType::EOF) {
            if let Some(result) = self.check_hint(None, super::hints::HintScope::CallArgs) {
                let tok = self.current();
                let msg = result.message.as_deref().unwrap_or("Unexpected token in arguments");
                return Err(format!("{} at line {}:{}\n  Hint: {}", msg, tok.line, tok.col, result.hint));
            }
        }
        Ok((args, named_args))
    }

    fn parse_one_call_arg(&mut self, args: &mut Vec<Expr>, named_args: &mut Vec<(Sym, Expr)>) -> Result<(), String> {
        if self.check(TokenType::Underscore) {
            let span = Some(self.current_span());
            self.advance();
            args.push(Expr::new(self.next_id(), span, ExprKind::Placeholder));
            return Ok(());
        }
        // Named argument: `name: expr`
        if self.check(TokenType::Ident)
            && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Colon)
            && self.peek_at(2).map(|t| &t.token_type) != Some(&TokenType::Colon) // not ::
        {
            let name = self.advance_and_get_sym();
            self.advance(); // skip :
            self.skip_newlines();
            let value = self.parse_expr()?;
            if !named_args.is_empty() || true {
                // Once named starts, check no positional after
                if named_args.is_empty() && !args.is_empty() {
                    // First named arg after positional — OK
                }
                named_args.push((name, value));
            }
        } else {
            if !named_args.is_empty() {
                let tok = self.current();
                return Err(format!("positional argument after named argument at line {}:{}", tok.line, tok.col));
            }
            let expr = self.parse_expr()?;
            // Type ascription: `[]: List[Int]`, `[:]: Map[K,V]`, `(expr): Type`
            // Disambiguated from named args because those are consumed above
            // (only triggered when expr is NOT a bare Ident).
            if self.check(TokenType::Colon) && self.peek_at(1).map(|t| &t.token_type) != Some(&TokenType::Colon) {
                let span = expr.span;
                self.advance(); // skip :
                let ty = self.parse_type_expr()?;
                args.push(Expr::new(self.next_id(), span, ExprKind::TypeAscription {
                    expr: Box::new(expr), ty,
                }));
            } else {
                args.push(expr);
            }
        }
        Ok(())
    }
}
