// ADR-0008 over the positions whose VALUE is another expression's value —
// extracted alongside `infer_statements.rs`; `include!`d into `infer.rs`, so
// imports come from there.
//
// Propagation is explicit in every position (`expr!`), and the E041/E042
// report is queued wherever the checker would otherwise strip a `Result`
// silently. Three sites stripped without queueing (#2182): an effect fn's
// tail value, a `match` arm's value and an `if` branch's value (the `else`
// side, since the `if` types as its `then` arm), and a `guard`'s else. All of
// them are TAIL positions — the value flows out of a block, a branch or an
// arm — so one walk over the tail leaves serves every site, and the
// post-solve report dedupes a leaf that several sites reach.

impl Checker {
    /// The leaves an expression's value comes from: a block's tail, both `if`
    /// branches and every `match` arm body, through parentheses. Anything
    /// else is itself a leaf.
    pub(crate) fn tail_leaves<'e>(expr: &'e ast::Expr, out: &mut Vec<&'e ast::Expr>) {
        match &expr.kind {
            ExprKind::Paren { expr } => Self::tail_leaves(expr, out),
            ExprKind::Block { expr: Some(tail), .. } => Self::tail_leaves(tail, out),
            ExprKind::Block { expr: None, .. } => {}
            ExprKind::If { then, else_, .. } => {
                Self::tail_leaves(then, out);
                Self::tail_leaves(else_, out);
            }
            ExprKind::Match { arms, .. } => {
                for arm in arms {
                    Self::tail_leaves(&arm.body, out);
                }
            }
            _ => out.push(expr),
        }
    }

    /// The effect calls an operator leaf reaches through its operands —
    /// `f() + 1`, `-f()`, `(f()) * 2` — through parentheses and nested
    /// operators. The operand strip (`operand_effect_unwrap`) already queued
    /// each as E041 at its own span; a position that DISCARDS the operator's
    /// value re-queues them as must-use so the post-solve report upgrades
    /// that same span to E042 (#2196). Not part of [`Self::tail_leaves`]: an
    /// operand is never the position's value, so the withdrawal for a
    /// `-> Result` tail must not reach it.
    fn operand_call_leaves<'e>(expr: &'e ast::Expr, out: &mut Vec<&'e ast::Expr>) {
        match &expr.kind {
            ExprKind::Paren { expr } => Self::operand_call_leaves(expr, out),
            ExprKind::Binary { left, right, .. } => {
                Self::operand_call_leaves(left, out);
                Self::operand_call_leaves(right, out);
            }
            ExprKind::Unary { operand, .. } => Self::operand_call_leaves(operand, out),
            ExprKind::Call { .. } => out.push(expr),
            _ => {}
        }
    }

    /// Does the value of `expr` come from more than the expression itself?
    fn is_branching(expr: &ast::Expr) -> bool {
        let mut leaves = Vec::new();
        Self::tail_leaves(expr, &mut leaves);
        !matches!(leaves.as_slice(), [only] if std::ptr::eq(*only, expr))
    }

    /// Queue every Result-typed tail leaf of `expr` for the post-solve
    /// E041/E042 report (`validate_implicit_propagation`). `must_use` says the
    /// value is DISCARDED at this position (E042 — a statement, the tail of a
    /// `-> Unit` fn); otherwise it is used as a value (E041). An explicit
    /// `ok(..)` / `err(..)` leaf is the spelled Result and never queued. The
    /// `!` insertion is mechanical for a plain call where `!` is legal (the
    /// E022 predicate: an effect fn body or a test block).
    pub(crate) fn queue_implicit_prop_leaves(&mut self, expr: &ast::Expr, what: &'static str, must_use: bool) {
        let mut leaves = Vec::new();
        Self::tail_leaves(expr, &mut leaves);
        let operators: Vec<&ast::Expr> = leaves.iter().copied()
            .filter(|leaf| matches!(leaf.kind, ExprKind::Binary { .. } | ExprKind::Unary { .. }))
            .collect();
        for operator in operators {
            Self::operand_call_leaves(operator, &mut leaves);
        }
        let bang_legal = self.env.auto_unwrap || self.env.in_test_block;
        for leaf in leaves {
            if matches!(leaf.kind, ExprKind::Ok { .. } | ExprKind::Err { .. }) {
                continue;
            }
            let Some(ty) = self.type_map.get(&leaf.id).cloned() else { continue };
            let mechanical = matches!(leaf.kind, ExprKind::Call { .. }) && bang_legal;
            self.deferred_implicit_prop_checks.push((ty, leaf.span, what, mechanical, must_use));
        }
    }

    /// The inverse of [`Self::queue_implicit_prop_leaves`] for a position
    /// where a Result-typed leaf IS the value the fn returns: the tail of a fn
    /// declared `-> Result[..]`, and a guard's else in one. The arm join and
    /// the `if` comparison queued those leaves as they stripped them for the
    /// join (they cannot see the fn's tail); here the Result flows out as the
    /// declared return — `match cmd { "greet" => greet(name), _ => ok(()) }`
    /// in a `-> Result[Unit, E]` main — so the entries are withdrawn. A leaf
    /// without a span is never matched (it was never reportable anyway).
    pub(crate) fn unqueue_implicit_prop_leaves(&mut self, expr: &ast::Expr) {
        let mut leaves = Vec::new();
        Self::tail_leaves(expr, &mut leaves);
        let spans: Vec<ast::Span> = leaves.iter().filter_map(|l| l.span).collect();
        self.deferred_implicit_prop_checks
            .retain(|(_, span, ..)| !span.is_some_and(|s| spans.contains(&s)));
    }
}
