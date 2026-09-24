// The recursion-shape rule (E088) of the `scoped` checker and the AST
// helpers its walks share — the second half of scoped.rs, spliced into the
// same module (check/mod.rs) right after it.

impl<'a, 'c> ScopedCx<'a, 'c> {
    // ── E088: recursion shape ─────────────────────────────────────────

    /// A scoped fn may recurse directly only: a re-entry through another
    /// scoped fn retains the caller's activation across the round trip.
    fn check_mutual_recursion(&mut self, name: &str, body: &ast::Expr) {
        let mut sites: Vec<(String, Option<BSpan>)> = Vec::new();
        ast::visit_expr(body, &mut |e| {
            if let ast::ExprKind::Call { callee, .. } = &e.kind
                && let ast::ExprKind::Ident { name: callee_name } = &callee.kind
                && callee_name.as_str() != name
                && self.fns.get(callee_name.as_str()).is_some_and(|f| f.scoped)
                && self.reaches(callee_name.as_str(), name)
            {
                sites.push((callee_name.to_string(), e.span));
            }
        });
        for (through, span) in sites {
            let d = scoped_err(
                "recursive call retains the current scoped activation".to_string(),
                &format!("make the recursion direct: fold `{through}` into `{name}`, or carry the state in an accumulator"),
                "E088",
                &self.ctx(),
                self.file,
                span,
            )
            .with_note(format!("`{name}` re-enters through `{through}` (mutual recursion is outside the stage-1 fragment)"))
            .with_note(self.required_by());
            self.diags.push(d);
        }
    }

    /// Does `from` reach `target` over scoped-fn call edges?
    fn reaches(&self, from: &str, target: &str) -> bool {
        let mut stack = vec![from.to_string()];
        let mut seen: HashSet<String> = HashSet::new();
        while let Some(f) = stack.pop() {
            if !seen.insert(f.clone()) {
                continue;
            }
            let Some(facts) = self.fns.get(f.as_str()) else { continue };
            let Some(body) = facts.body else { continue };
            let mut found = false;
            ast::visit_expr(body, &mut |e| {
                if let ast::ExprKind::Call { callee, .. } = &e.kind
                    && let ast::ExprKind::Ident { name } = &callee.kind
                {
                    if name.as_str() == target {
                        found = true;
                    } else if self.fns.get(name.as_str()).is_some_and(|g| g.scoped) {
                        stack.push(name.to_string());
                    }
                }
            });
            if found {
                return true;
            }
        }
        false
    }

    /// A single self-call that is not its arm's result retains the
    /// activation; with an accumulator it could be the result. Two or more
    /// in one arm (a tree walk) cannot, and are an ordinary worker.
    fn check_tail_shape(&mut self, name: &str, region: &ast::Expr, e: &ast::Expr) {
        use ast::ExprKind as K;
        match &e.kind {
            K::If { cond, then, else_ } => {
                self.check_tail_shape(name, region, cond);
                self.check_tail_shape(name, then, then);
                self.check_tail_shape(name, else_, else_);
            }
            K::Match { subject, arms } => {
                self.check_tail_shape(name, region, subject);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.check_tail_shape(name, &arm.body, g);
                    }
                    self.check_tail_shape(name, &arm.body, &arm.body);
                }
            }
            K::Call { callee, args, .. } => {
                if let K::Ident { name: n } = &callee.kind && n.as_str() == name && !self.tail_calls.contains(&e.id) {
                    self.report_tail_shape(name, region, e, "work");
                }
                let what = if matches!(callee.kind, K::TypeName { .. }) { "a constructor" } else { "a call" };
                for a in args {
                    if is_self_call(a, name) {
                        self.report_tail_shape(name, region, a, what);
                    } else {
                        self.check_tail_shape(name, region, a);
                    }
                }
            }
            K::Pipe { left, right } => {
                let is_self = match &right.kind {
                    K::Ident { name: n } => n.as_str() == name,
                    K::Call { callee, .. } => matches!(&callee.kind, K::Ident { name: n } if n.as_str() == name),
                    _ => false,
                };
                if is_self && !self.tail_calls.contains(&right.id) {
                    self.report_tail_shape(name, region, e, "work");
                }
                self.check_tail_shape(name, region, left);
                if let K::Call { args, .. } = &right.kind {
                    for a in args {
                        self.check_tail_shape(name, region, a);
                    }
                }
            }
            K::Binary { op, left, right } => {
                let what = match op.as_str() {
                    "+" => "addition",
                    "-" => "subtraction",
                    "*" => "multiplication",
                    "/" => "division",
                    _ => "an operator",
                };
                for side in [left, right] {
                    if is_self_call(side, name) {
                        self.report_tail_shape(name, region, side, what);
                    } else {
                        self.check_tail_shape(name, region, side);
                    }
                }
            }
            _ => {
                for c in expr_children(e) {
                    self.check_tail_shape(name, region, c);
                }
                for s in stmt_children(e) {
                    let binding = matches!(s, ast::Stmt::Let { .. } | ast::Stmt::Var { .. });
                    for c in stmt_exprs(s) {
                        if binding && is_self_call(c, name) {
                            self.report_tail_shape(name, region, c, "a binding");
                        } else {
                            self.check_tail_shape(name, region, c);
                        }
                    }
                }
            }
        }
    }

    fn report_tail_shape(&mut self, name: &str, region: &ast::Expr, site: &ast::Expr, what: &str) {
        if count_self_calls(region, name) != 1 {
            return;
        }
        let d = scoped_err(
            "recursive call retains the current scoped activation".to_string(),
            "carry the count in an accumulator and return the recursive call",
            "E088",
            &self.ctx(),
            self.file,
            site.span,
        )
        .with_note(format!("{what} remains after the recursive call"))
        .with_note(self.required_by());
        self.diags.push(d);
    }
}

fn is_self_call(e: &ast::Expr, name: &str) -> bool {
    match &e.kind {
        ast::ExprKind::Call { callee, .. } => matches!(&callee.kind, ast::ExprKind::Ident { name: n } if n.as_str() == name),
        ast::ExprKind::Paren { expr } => is_self_call(expr, name),
        _ => false,
    }
}

fn count_self_calls(region: &ast::Expr, name: &str) -> usize {
    let mut n = 0;
    ast::visit_expr(region, &mut |e| {
        let is_self = match &e.kind {
            ast::ExprKind::Call { callee, .. } => matches!(&callee.kind, ast::ExprKind::Ident { name: c } if c.as_str() == name),
            ast::ExprKind::Pipe { right, .. } => matches!(&right.kind, ast::ExprKind::Ident { name: c } if c.as_str() == name),
            _ => false,
        };
        if is_self {
            n += 1;
        }
    });
    n
}

fn block_tail(e: &ast::Expr) -> Option<&ast::Expr> {
    match &e.kind {
        ast::ExprKind::Block { expr: Some(t), .. } => Some(t),
        ast::ExprKind::Block { expr: None, stmts } => match stmts.last() {
            Some(ast::Stmt::Expr { expr, .. }) => Some(expr),
            _ => None,
        },
        _ => Some(e),
    }
}

fn pattern_binders(p: &ast::Pattern, out: &mut Vec<String>) {
    use ast::Pattern as P;
    match p {
        P::Ident { name } => out.push(name.to_string()),
        P::Constructor { args, .. } => args.iter().for_each(|a| pattern_binders(a, out)),
        P::RecordPattern { fields, .. } => {
            for f in fields {
                match &f.pattern {
                    Some(inner) => pattern_binders(inner, out),
                    None => out.push(f.name.to_string()),
                }
            }
        }
        P::Tuple { elements } | P::List { elements, .. } => elements.iter().for_each(|a| pattern_binders(a, out)),
        P::Some { inner } | P::Ok { inner } | P::Err { inner } => pattern_binders(inner, out),
        P::As { name, inner } => {
            out.push(name.to_string());
            pattern_binders(inner, out);
        }
        P::Or { alts } => alts.iter().for_each(|a| pattern_binders(a, out)),
        P::Wildcard | P::Literal { .. } | P::None => {}
    }
    if let P::List { rest: Some(Some(n)), .. } = p {
        out.push(n.to_string());
    }
}

/// The direct child EXPRESSIONS of a node (statement bodies via `stmt_children`).
fn expr_children(e: &ast::Expr) -> Vec<&ast::Expr> {
    use ast::ExprKind as K;
    match &e.kind {
        K::Call { callee, args, named_args, .. } => {
            let mut v: Vec<&ast::Expr> = vec![callee];
            v.extend(args.iter());
            v.extend(named_args.iter().map(|(_, a)| a));
            v
        }
        K::Binary { left, right, .. } | K::Pipe { left, right } | K::Compose { left, right }
        | K::UnwrapOr { expr: left, fallback: right } | K::IndexAccess { object: left, index: right }
        | K::Range { start: left, end: right, .. } => vec![left, right],
        K::If { cond, then, else_ } | K::IfLet { scrutinee: cond, then, else_, .. } => vec![cond, then, else_],
        K::Match { subject, arms } => {
            let mut v: Vec<&ast::Expr> = vec![subject];
            for a in arms {
                v.extend(a.guard.iter());
                v.push(&a.body);
            }
            v
        }
        K::Block { expr, .. } => expr.iter().map(|b| b.as_ref()).collect(),
        K::ForIn { iterable: x, .. } | K::While { cond: x, .. } => vec![x],
        K::List { elements } | K::Tuple { elements } | K::Fan { exprs: elements } | K::FanSettle { arms: elements } => elements.iter().collect(),
        K::MapLiteral { entries } => entries.iter().flat_map(|(k, v)| [k, v]).collect(),
        K::Record { fields, .. } => fields.iter().map(|f| &f.value).collect(),
        K::SpreadRecord { base, fields } => std::iter::once(base.as_ref()).chain(fields.iter().map(|f| &f.value)).collect(),
        K::InterpolatedString { parts, .. } => parts.iter().filter_map(|p| match p { ast::StringPart::Expr { expr } => Some(expr.as_ref()), _ => None }).collect(),
        K::FanBounded { budget: a, body: b } | K::FanTimeout { deadline: a, body: b } => vec![a, b],
        K::FanRace { budget, arms } => budget.iter().map(|b| b.as_ref()).chain(arms.iter()).collect(),
        K::FanRaceMap { budget, list, mapper } => budget.iter().map(|b| b.as_ref()).chain([list.as_ref(), mapper.as_ref()]).collect(),
        K::Member { object: x, .. } | K::TupleIndex { object: x, .. } | K::OptionalChain { expr: x, .. }
        | K::Unary { operand: x, .. } | K::Lambda { body: x, .. } | K::Try { expr: x } | K::Unwrap { expr: x }
        | K::ToOption { expr: x } | K::Paren { expr: x } | K::Some { expr: x } | K::Ok { expr: x } | K::Err { expr: x }
        | K::TypeAscription { expr: x, .. } | K::Scoped { body: x, .. } => vec![x],
        K::Int { .. } | K::Float { .. } | K::String { .. } | K::Bool { .. } | K::Ident { .. } | K::TypeName { .. }
        | K::EmptyMap | K::Hole | K::Todo { .. } | K::Break | K::Continue | K::Placeholder | K::Unit | K::None
        | K::Error => Vec::new(),
    }
}

fn stmt_children(e: &ast::Expr) -> &[ast::Stmt] {
    match &e.kind {
        ast::ExprKind::Block { stmts, .. } | ast::ExprKind::ForIn { body: stmts, .. } | ast::ExprKind::While { body: stmts, .. } => stmts,
        _ => &[],
    }
}

fn stmt_exprs(s: &ast::Stmt) -> Vec<&ast::Expr> {
    match s {
        ast::Stmt::Let { value, .. } | ast::Stmt::Var { value, .. } | ast::Stmt::LetDestructure { value, .. }
        | ast::Stmt::Assign { value, .. } | ast::Stmt::FieldAssign { value, .. } | ast::Stmt::Expr { expr: value, .. } => vec![value],
        ast::Stmt::IndexAssign { index, value, .. } => vec![index, value],
        ast::Stmt::Guard { cond, else_, .. } => vec![cond, else_],
        ast::Stmt::GuardLet { scrutinee, else_, .. } => vec![scrutinee, else_],
        ast::Stmt::Comment { .. } | ast::Stmt::Error { .. } => Vec::new(),
    }
}

fn type_expr_text(t: &ast::TypeExpr) -> String {
    match t {
        ast::TypeExpr::Simple { name } => name.to_string(),
        ast::TypeExpr::Generic { name, args } => format!("{name}[{}]", args.iter().map(type_expr_text).collect::<Vec<_>>().join(", ")),
        ast::TypeExpr::Tuple { elements } => format!("({})", elements.iter().map(type_expr_text).collect::<Vec<_>>().join(", ")),
        ast::TypeExpr::Fn { .. } => "a function type".to_string(),
        ast::TypeExpr::Record { .. } | ast::TypeExpr::OpenRecord { .. } => "a record type".to_string(),
        ast::TypeExpr::Variant { .. } => "a variant type".to_string(),
        ast::TypeExpr::Union { .. } => "a union type".to_string(),
        ast::TypeExpr::ConstLit { value } => value.to_string(),
    }
}
