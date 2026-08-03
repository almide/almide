//! The `fan.*` surface heads — block forms that are NOT ordinary calls
//! (a call followed by `{` does not parse), so each gets its own parse:
//! `fan.bounded(budget) { body }`, `fan.timeout(deadline) { body }`,
//! `fan.race(budget?) { arms }`, `fan.settle { arms }`, `fan.any { arms }`,
//! plus the legacy-spelling rebuilds that route retired forms to the
//! checker's E027 signature-migration hints. Split out of `primary.rs`
//! (one file per surface family).

use crate::lexer::TokenType;
use crate::ast::*;
use crate::ast::ExprKind;
use crate::intern::sym;
use super::Parser;

impl Parser {
    /// Dispatch on the head name after `fan.` — each form owns its parse.
    /// A bare `fan.<other>` member (fan.map etc.) falls through as an
    /// identifier; `fan { … }` is the plain fan block.
    pub(crate) fn parse_fan_primary(&mut self) -> Result<Expr, String> {
        let head_is = |p: &Self, name: &str| {
            p.peek_at(1).map_or(false, |t| t.token_type == TokenType::Dot)
                && p.peek_at(2).map_or(false, |t| t.value == name)
        };
        let head_then = |p: &Self, tt: TokenType| {
            p.peek_at(3).map_or(false, |t| t.token_type == tt)
        };
        if head_is(self, "bounded") {
            return self.parse_fan_bounded_head();
        }
        if head_is(self, "timeout") && head_then(self, TokenType::LParen) {
            return self.parse_fan_timeout_head();
        }
        if head_is(self, "race") {
            return self.parse_fan_race_head();
        }
        if head_is(self, "settle") && head_then(self, TokenType::LBrace) {
            return self.parse_fan_settle_head();
        }
        if head_is(self, "any") && head_then(self, TokenType::LBrace) {
            return self.parse_fan_any_block_head();
        }
        // fan { ... } = fan block; fan.map/fan.race = module-like call
        if self.peek_at(1).map_or(false, |t| t.token_type == TokenType::Dot) {
            // Treat `fan` as an identifier for member access
            let span = Some(self.current_span());
            self.advance();
            return Ok(Expr::new(self.next_id(), span, ExprKind::Ident { name: sym("fan") }));
        }
        self.advance();
        self.parse_fan_block()
    }

    /// Consume `fan` `.` `<head>`, returning the head's span.
    fn eat_fan_head(&mut self) -> Option<Span> {
        let span = Some(self.current_span());
        self.advance(); // fan
        self.advance(); // .
        self.advance(); // head name
        span
    }

    /// Rebuild the legacy member CALL `fan.<head>(args)` verbatim, so the
    /// checker's E027 tombstone (a signature-migration hint) fires on the
    /// retired thunk-list spellings.
    fn legacy_fan_call(&mut self, span: Option<Span>, head: &str, args: Vec<Expr>) -> Expr {
        let fan_ident = Expr::new(self.next_id(), span, ExprKind::Ident { name: sym("fan") });
        let member = Expr::new(self.next_id(), span, ExprKind::Member {
            object: Box::new(fan_ident),
            field: sym(head),
        });
        Expr::new(self.next_id(), span, ExprKind::Call {
            callee: Box::new(member),
            args,
            named_args: Vec::new(),
            type_args: None,
        })
    }

    /// Scan a `{ arm; arm; … }` block shared by the fan arm heads. `label`
    /// names the construct in diagnostics ("fan.race"); `ban_hint` is the
    /// statement-ban hint tail (the heads phrase it differently and the
    /// harnesses pin the exact text). At least one arm is required.
    fn parse_fan_arm_block(&mut self, label: &str, ban_hint: &str) -> Result<Vec<Expr>, String> {
        let open = self.current().clone();
        self.expect(TokenType::LBrace)?;
        let mut arms = Vec::new();
        self.skip_newlines();
        while !self.check(TokenType::RBrace) && !self.check(TokenType::EOF) {
            let tok = self.current().clone();
            if matches!(tok.token_type, TokenType::Let | TokenType::Var | TokenType::For | TokenType::While) {
                return Err(format!("`{}` is not allowed inside {label} at line {}:{}\n  Hint: {ban_hint}", tok.value, tok.line, tok.col));
            }
            arms.push(self.parse_expr()?);
            self.skip_newlines();
            if self.check(TokenType::Semicolon) {
                self.advance();
                self.skip_newlines();
            }
        }
        self.expect_closing(TokenType::RBrace, open.line, open.col, &format!("{label} block"))?;
        if arms.is_empty() {
            return Err(format!("{label} must contain at least one arm at line {}:{}", open.line, open.col));
        }
        Ok(arms)
    }

    /// fan.bounded(budget) { body } — a HEAD with args + block.
    fn parse_fan_bounded_head(&mut self) -> Result<Expr, String> {
        let span = self.eat_fan_head();
        self.expect(TokenType::LParen)?;
        let budget = self.parse_expr()?;
        // The THUNK spelling from pre-Wave-1 training data:
        // `fan.bounded(budget, () => work())`. A bare "Missing ')'" sent
        // models in circles (dojo round 1) — teach the block form. A
        // MAPPER-looking spelling (a 1-param lambda) gets the matrix
        // answer instead: bounded has no mapper cell BY DESIGN.
        if self.check(TokenType::Comma) {
            let (line, col) = (self.current().line, self.current().col);
            if self.extra_head_args_end_in_mapper()? {
                return Err(format!(
                    "fan.bounded has no mapper form — it meters a single BODY, at line {line}:{col}\n  Hint: meter per element by composing: xs |> fan.map((x) => fan.bounded(compute.ms(100)) {{ work(x) }} )"));
            }
            return Err(format!(
                "fan.bounded takes a BLOCK, not a thunk argument, at line {line}:{col}\n  Hint: fan.bounded(compute.ms(100)) {{ work(x) }} — drop the `() =>` wrapper; the braces are the region"));
        }
        self.expect(TokenType::RParen)?;
        self.skip_newlines();
        if !self.check(TokenType::LBrace) {
            return Err(format!(
                "fan.bounded requires a body block at line {}:{}\n  Hint: fan.bounded(compute.ms(100)) {{ work(x) }}",
                self.current().line, self.current().col));
        }
        // The body is a full BLOCK (T2-1): statements + trailing value,
        // parsed by the same brace parser as any block expression.
        let body = self.parse_brace_expr()?;
        Ok(Expr::new(self.next_id(), span, ExprKind::FanBounded {
            budget: Box::new(budget),
            body: Box::new(body),
        }))
    }

    /// fan.timeout(deadline) { body } — the oracle-tier head (T5-1),
    /// parsed exactly like bounded (deadline in parens, block body).
    fn parse_fan_timeout_head(&mut self) -> Result<Expr, String> {
        let span = self.eat_fan_head();
        self.expect(TokenType::LParen)?;
        let deadline = self.parse_expr()?;
        // Legacy `fan.timeout(ms, thunk)` (2+ args) — rebuild the member
        // CALL so the checker's E027 signature-migration hint fires,
        // exactly like race's legacy rebuild. A MAPPER-looking spelling
        // (1-param lambda tail) instead gets the matrix answer — timeout
        // has no mapper cell BY DESIGN.
        if self.check(TokenType::Comma) {
            let (line, col) = (self.current().line, self.current().col);
            let mut args = vec![deadline];
            while self.check(TokenType::Comma) {
                self.advance();
                self.skip_newlines();
                args.push(self.parse_expr()?);
            }
            if matches!(&args.last().map(|a| &a.kind),
                Some(ExprKind::Lambda { params, .. }) if params.len() == 1)
            {
                return Err(format!(
                    "fan.timeout has no mapper form — it deadlines a single BODY, at line {line}:{col}\n  Hint: fan.timeout(duration.ms(5000)) {{ work(x) }}"));
            }
            self.expect(TokenType::RParen)?;
            return Ok(self.legacy_fan_call(span, "timeout", args));
        }
        self.expect(TokenType::RParen)?;
        self.skip_newlines();
        if !self.check(TokenType::LBrace) {
            // `fan.timeout(e)` with no block: same legacy rebuild -> E027.
            return Ok(self.legacy_fan_call(span, "timeout", vec![deadline]));
        }
        let body = self.parse_brace_expr()?;
        Ok(Expr::new(self.next_id(), span, ExprKind::FanTimeout {
            deadline: Box::new(deadline),
            body: Box::new(body),
        }))
    }

    /// fan.race(budget?) { arm; … } — like bounded, a head with an optional
    /// budget and an arm block. `fan.race(e)` NOT followed by `{` is the
    /// REMOVED 0.42.0 thunk-list form: reconstruct the legacy member call so
    /// the checker's E027 tombstone (now a signature-migration hint) fires.
    fn parse_fan_race_head(&mut self) -> Result<Expr, String> {
        let span = self.eat_fan_head();
        let budget = if self.check(TokenType::LParen) {
            self.advance();
            let b = self.parse_expr()?;
            if self.check(TokenType::Comma) {
                return self.parse_fan_race_comma_tail(span, b);
            }
            self.expect(TokenType::RParen)?;
            Some(b)
        } else {
            None
        };
        self.skip_newlines();
        if !self.check(TokenType::LBrace) {
            if let Some(b) = budget {
                // Legacy `fan.race(thunks)` call — rebuild it verbatim.
                return Ok(self.legacy_fan_call(span, "race", vec![b]));
            }
            return Err(format!(
                "fan.race requires an arm block at line {}:{}\n  Hint: fan.race {{ a(); b() }} or fan.race(compute.ms(5)) {{ a(); b() }}",
                self.current().line, self.current().col));
        }
        let arms = self.parse_fan_arm_block(
            "fan.race",
            "race arms are expressions — wrap statements in a block arm: { let x = f(); g(x) }",
        )?;
        Ok(Expr::new(self.next_id(), span, ExprKind::FanRace {
            budget: budget.map(Box::new),
            arms,
        }))
    }

    /// The comma tail of `fan.race(b, …)`: a 1-param lambda tail is the
    /// MAPPER form `fan.race(xs, f)` / `fan.race(budget, xs, f)`; anything
    /// else with a comma is the retired thunk spelling — the migration hint.
    fn parse_fan_race_comma_tail(&mut self, span: Option<Span>, b: Expr) -> Result<Expr, String> {
        let (line, col) = (self.current().line, self.current().col);
        let mut rest = Vec::new();
        while self.check(TokenType::Comma) {
            self.advance();
            self.skip_newlines();
            if self.check(TokenType::RParen) {
                break;
            }
            rest.push(self.parse_expr()?);
        }
        let mapper_tail = matches!(
            rest.last().map(|a| &a.kind),
            Some(ExprKind::Lambda { params, .. }) if params.len() == 1
        );
        if mapper_tail && rest.len() <= 2 {
            self.expect(TokenType::RParen)?;
            let Some(mapper) = rest.pop() else {
                unreachable!("mapper_tail implies a last argument")
            };
            let (budget, list) = match rest.pop() {
                // fan.race(budget, xs, f)
                Some(xs) => (Some(Box::new(b)), Box::new(xs)),
                // fan.race(xs, f)
                None => (None, Box::new(b)),
            };
            return Ok(Expr::new(self.next_id(), span, ExprKind::FanRaceMap {
                budget,
                list,
                mapper: Box::new(mapper),
            }));
        }
        Err(format!(
            "fan.race takes a BLOCK of arms, not thunk arguments, at line {line}:{col}\n  Hint: fan.race(compute.ms(5)) {{ exact(p); heuristic(p) }} — arms are expressions separated by `;`, no `() =>` wrappers; a dynamic list races via the mapper form fan.race(xs, (x) => ok(work(x)))"))
    }

    /// fan.settle { arms } — a REAL node (T2-4): the value is a TUPLE of
    /// per-arm Results (heterogeneous arms allowed), so the thunk-list
    /// desugar (which forces one list element type) cannot carry it.
    fn parse_fan_settle_head(&mut self) -> Result<Expr, String> {
        let span = self.eat_fan_head();
        let arms = self.parse_fan_arm_block(
            "fan.settle",
            "arms are expressions — wrap statements in a block arm: { let x = f(); g(x) }",
        )?;
        Ok(Expr::new(self.next_id(), span, ExprKind::FanSettle { arms }))
    }

    /// fan.any { arms } — the Wave 1 block form.
    /// DESUGARS AT PARSE TIME to the literal thunk-list call the checker and
    /// the MIR inliner already handle (`fan.any([() => a(), () => b()])`),
    /// so the entire downstream pipeline is unchanged on both targets. The
    /// user-facing thunk-list SPELLING is tombstoned in the checker; this
    /// synthesized form is the one internal producer of it.
    fn parse_fan_any_block_head(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        self.advance(); // fan
        self.advance(); // .
        let head = self.current().value.clone();
        self.advance(); // any
        let arms = self.parse_fan_arm_block(&format!("fan.{head}"), "arms are expressions")?;
        let thunks: Vec<Expr> = arms
            .into_iter()
            .map(|arm| {
                let arm_span = arm.span.clone();
                Expr::new(self.next_id(), arm_span, ExprKind::Lambda {
                    params: Vec::new(),
                    body: Box::new(arm),
                })
            })
            .collect();
        let list = Expr::new(self.next_id(), span, ExprKind::List { elements: thunks });
        let fan_ident = Expr::new(self.next_id(), span, ExprKind::Ident { name: sym("fan") });
        let member = Expr::new(self.next_id(), span, ExprKind::Member {
            object: Box::new(fan_ident),
            // INTERNAL name: the checker types `__any_block`/`__settle_block`
            // exactly like any/settle, while the PUBLIC 1-arg thunk-list
            // spelling is tombstoned — this synthesized node is the only
            // producer of the legacy shape. Frontend lowering normalizes the
            // name back so the MIR inliner is untouched.
            field: sym(&format!("__{head}_block")),
        });
        Ok(Expr::new(self.next_id(), span, ExprKind::Call {
            callee: Box::new(member),
            args: vec![list],
            named_args: Vec::new(),
            type_args: None,
        }))
    }

    /// After the first head argument's comma: scan the remaining args and
    /// report whether the LAST one is a 1-param lambda (the mapper shape) —
    /// distinguishes "wanted a mapper cell that doesn't exist" from the
    /// retired thunk spelling in the E-hints above.
    fn extra_head_args_end_in_mapper(&mut self) -> Result<bool, String> {
        let mut last_is_mapper = false;
        while self.check(TokenType::Comma) {
            self.advance();
            if self.check(TokenType::RParen) {
                break;
            }
            let arg = self.parse_expr()?;
            last_is_mapper = matches!(&arg.kind, ExprKind::Lambda { params, .. } if params.len() == 1);
        }
        Ok(last_is_mapper)
    }
}
