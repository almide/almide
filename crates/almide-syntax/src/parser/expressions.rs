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
            TokenType::DotDot           => Some((10, 10)),  // range
            TokenType::DotDotEq         => Some((10, 10)),
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
    const INFIX_TOKENS: &'static [TokenType] = &[
        TokenType::Or, TokenType::And,
        TokenType::EqEq, TokenType::BangEq, TokenType::LtEq, TokenType::GtEq,
        TokenType::PipeArrow, TokenType::ComposeArrow,
        TokenType::DotDot, TokenType::DotDotEq,
        TokenType::Plus, TokenType::Minus, TokenType::PlusPlus,
        TokenType::Star, TokenType::Slash, TokenType::Percent,
        TokenType::Caret,
    ];

    fn parse_expr_bp(&mut self, min_bp: u8) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;

        loop {
            // Allow operators on next line for multiline expressions
            self.skip_newlines_if_followed_by_any(Self::INFIX_TOKENS);

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
            let op_value = self.current().value.clone();
            self.advance();
            self.skip_newlines();

            // ── Special: |> match { ... } ──
            if tt == TokenType::PipeArrow
                && self.check(TokenType::Match)
                && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::LBrace)
            {
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
                left = Expr::new(self.next_id(), span, ExprKind::Match {
                    subject: Box::new(left), arms,
                });
                continue;
            }

            let right = self.parse_expr_bp(r_bp)?;

            // ── Build AST node ──
            left = match tt {
                TokenType::PipeArrow => Expr::new(self.next_id(), span, ExprKind::Pipe {
                    left: Box::new(left), right: Box::new(right),
                }),
                TokenType::ComposeArrow => Expr::new(self.next_id(), span, ExprKind::Compose {
                    left: Box::new(left), right: Box::new(right),
                }),
                TokenType::DotDot => Expr::new(self.next_id(), span, ExprKind::Range {
                    start: Box::new(left), end: Box::new(right), inclusive: false,
                }),
                TokenType::DotDotEq => Expr::new(self.next_id(), span, ExprKind::Range {
                    start: Box::new(left), end: Box::new(right), inclusive: true,
                }),
                _ => Expr::new(self.next_id(), span, ExprKind::Binary {
                    op: sym(&op_value), left: Box::new(left), right: Box::new(right),
                }),
            };

            // ── Reject chained comparisons: a < b < c ──
            if matches!(tt, TokenType::EqEq | TokenType::BangEq
                | TokenType::LAngle | TokenType::RAngle
                | TokenType::LtEq | TokenType::GtEq)
            {
                if matches!(self.current().token_type,
                    TokenType::EqEq | TokenType::BangEq
                    | TokenType::LAngle | TokenType::RAngle
                    | TokenType::LtEq | TokenType::GtEq)
                {
                    let tok = self.current();
                    return Err(format!(
                        "Chained comparison operators are not allowed at line {}:{}\n  Hint: Use 'and' to combine comparisons. Write: a < b and b < c",
                        tok.line, tok.col
                    ));
                }
            }
        }

        Ok(left)
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
            if self.check(TokenType::Dot) {
                let dot_span = self.current_span();
                self.advance();
                if self.check(TokenType::Int) {
                    let idx_str = self.current().value.clone();
                    self.advance();
                    let index = idx_str.parse::<usize>().map_err(|_| {
                        format!("invalid tuple index '{}' at line {:?}", idx_str, dot_span)
                    })?;
                    expr = Expr::new(self.next_id(), Some(dot_span), ExprKind::TupleIndex {
                        object: Box::new(expr), index,
                    });
                } else {
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
                    expr = Expr::new(self.next_id(), Some(full_span), ExprKind::Member {
                        object: Box::new(expr), field,
                    });
                }
            } else if self.check(TokenType::LBracket) && self.peek_type_args_call() {
                let span = Some(self.current_span());
                let ta = self.parse_type_args()?;
                self.expect(TokenType::LParen)?;
                let (args, named_args) = self.parse_call_args()?;
                self.expect(TokenType::RParen)?;
                expr = Expr::new(self.next_id(), span, ExprKind::Call {
                    callee: Box::new(expr), args, named_args, type_args: Some(ta),
                });
            } else if self.check(TokenType::LBracket) && !self.newline_before_current() {
                let span = Some(self.current_span());
                let open = self.current().clone();
                self.advance();
                let index = self.parse_expr()?;
                self.expect_closing(TokenType::RBracket, open.line, open.col, "index access")?;
                expr = Expr::new(self.next_id(), span, ExprKind::IndexAccess {
                    object: Box::new(expr), index: Box::new(index),
                });
            } else if self.check(TokenType::LParen) && !self.newline_before_current() {
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
                expr = Expr::new(self.next_id(), full_span, ExprKind::Call {
                    callee: Box::new(expr), args, named_args, type_args: None,
                });
            } else if self.check(TokenType::Bang) && !self.newline_before_current() {
                // expr! — unwrap with error propagation
                let span = Some(self.current_span());
                self.advance();
                expr = Expr::new(self.next_id(), span, ExprKind::Unwrap {
                    expr: Box::new(expr),
                });
            } else if self.check(TokenType::QuestionQuestion) {
                // expr ?? fallback — unwrap with default
                let span = Some(self.current_span());
                self.advance();
                self.skip_newlines();
                let fallback = self.parse_unary()?;
                expr = Expr::new(self.next_id(), span, ExprKind::UnwrapOr {
                    expr: Box::new(expr), fallback: Box::new(fallback),
                });
            } else if self.check(TokenType::QuestionDot) && !self.newline_before_current() {
                // expr?.field — optional chaining
                let span = Some(self.current_span());
                self.advance();
                let field = self.expect_any_name()?;
                expr = Expr::new(self.next_id(), span, ExprKind::OptionalChain {
                    expr: Box::new(expr), field,
                });
            } else if self.check(TokenType::Question) && !self.newline_before_current() {
                // expr? — convert to Option
                let span = Some(self.current_span());
                self.advance();
                expr = Expr::new(self.next_id(), span, ExprKind::ToOption {
                    expr: Box::new(expr),
                });
            } else {
                break;
            }
        }
        Ok(expr)
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
            args.push(self.parse_expr()?);
        }
        Ok(())
    }
}
