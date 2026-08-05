
// Error-surface lints (ADR-0004 D1/D3, #1105). Purely syntactic — run after
// inference so they appear alongside the other post-solve diagnostics, but
// they read only the AST. Both are WARNINGS: existing code keeps compiling.
//
// E035 — branching on error message text. Inside a `match` arm that binds an
// err payload (`err(e) => …`), using `e` in a branch condition via
// `string.contains(e, …)` / `e.contains(…)` or `e ==`/`!=` a string literal
// is the Go-〜1.12 string-match pattern the doctrine forbids. Detection is
// deliberately conservative (err-pattern bindings only, direct uses only —
// no dataflow) so it never false-positives on ordinary string code.
//
// E036 — a `map_err` lambda that never reads its error parameter. The
// canonical context idiom is `map_err((e) => "ctx: ${e}")`; forgetting
// `${e}` type-checks and silently destroys the original error. `(_) => …`
// is the explicit-discard spelling and is exempt.

impl Checker {
    pub(crate) fn lint_error_surface(&mut self, program: &ast::Program) {
        let mut lint = ErrorSurfaceLint { diags: Vec::new() };
        for decl in &program.decls {
            match decl {
                ast::Decl::Fn { body: Some(body), .. } => lint.walk_expr(body, &[]),
                ast::Decl::Test { body, .. } => lint.walk_expr(body, &[]),
                ast::Decl::TopLet { value, .. } => lint.walk_expr(value, &[]),
                _ => {}
            }
        }
        for mut d in lint.diags {
            d.file = self.source_file.clone();
            self.diagnostics.push(d);
        }
    }
}

struct ErrorSurfaceLint {
    diags: Vec<Diagnostic>,
}

impl ErrorSurfaceLint {
    /// `err_binds` is the stack of in-scope err-pattern binding names.
    fn walk_expr(&mut self, expr: &ast::Expr, err_binds: &[Sym]) {
        use ast::ExprKind as EK;
        // E036: any call whose callee is a `.map_err` member with a lambda arg.
        if let EK::Call { callee, args, .. } = &expr.kind {
            if let EK::Member { field, .. } = &callee.kind {
                if field.as_str() == "map_err" {
                    for arg in args {
                        self.check_maperr_lambda(arg);
                    }
                }
            }
        }
        match &expr.kind {
            EK::Match { subject, arms } => {
                self.walk_expr(subject, err_binds);
                for arm in arms {
                    let mut binds = err_binds.to_vec();
                    collect_err_pattern_binds(&arm.pattern, &mut binds);
                    if let Some(guard) = &arm.guard {
                        self.check_condition(guard, &binds);
                        self.walk_expr(guard, &binds);
                    }
                    self.walk_expr(&arm.body, &binds);
                }
            }
            EK::If { cond, then, else_ } => {
                self.check_condition(cond, err_binds);
                self.walk_expr(cond, err_binds);
                self.walk_expr(then, err_binds);
                self.walk_expr(else_, err_binds);
            }
            _ => self.walk_children(expr, err_binds),
        }
    }

    fn walk_children(&mut self, expr: &ast::Expr, err_binds: &[Sym]) {
        use ast::ExprKind as EK;
        match &expr.kind {
            EK::InterpolatedString { parts } => {
                for p in parts {
                    if let ast::StringPart::Expr { expr } = p { self.walk_expr(expr, err_binds); }
                }
            }
            EK::List { elements } | EK::Tuple { elements } => {
                for e in elements { self.walk_expr(e, err_binds); }
            }
            EK::MapLiteral { entries } => {
                for (k, v) in entries { self.walk_expr(k, err_binds); self.walk_expr(v, err_binds); }
            }
            EK::Record { fields, .. } => {
                for f in fields { self.walk_expr(&f.value, err_binds); }
            }
            EK::SpreadRecord { base, fields } => {
                self.walk_expr(base, err_binds);
                for f in fields { self.walk_expr(&f.value, err_binds); }
            }
            EK::Call { callee, args, named_args, .. } => {
                self.walk_expr(callee, err_binds);
                for a in args { self.walk_expr(a, err_binds); }
                for (_, a) in named_args { self.walk_expr(a, err_binds); }
            }
            EK::Member { object, .. } | EK::TupleIndex { object, .. } => self.walk_expr(object, err_binds),
            EK::IndexAccess { object, index } => {
                self.walk_expr(object, err_binds);
                self.walk_expr(index, err_binds);
            }
            EK::Pipe { left, right } | EK::Compose { left, right } => {
                self.walk_expr(left, err_binds);
                self.walk_expr(right, err_binds);
            }
            EK::IfLet { scrutinee, then, else_, .. } => {
                self.walk_expr(scrutinee, err_binds);
                self.walk_expr(then, err_binds);
                self.walk_expr(else_, err_binds);
            }
            EK::Block { stmts, expr } => {
                for s in stmts { self.walk_stmt(s, err_binds); }
                if let Some(e) = expr { self.walk_expr(e, err_binds); }
            }
            EK::Fan { exprs } | EK::FanRace { arms: exprs, .. } | EK::FanSettle { arms: exprs } => {
                for e in exprs { self.walk_expr(e, err_binds); }
            }
            EK::FanBounded { budget, body } => {
                self.walk_expr(budget, err_binds);
                self.walk_expr(body, err_binds);
            }
            EK::FanRaceMap { budget, list, mapper } => {
                if let Some(b) = budget { self.walk_expr(b, err_binds); }
                self.walk_expr(list, err_binds);
                self.walk_expr(mapper, err_binds);
            }
            EK::FanTimeout { deadline, body } => {
                self.walk_expr(deadline, err_binds);
                self.walk_expr(body, err_binds);
            }
            EK::ForIn { iterable, body, .. } => {
                self.walk_expr(iterable, err_binds);
                for s in body { self.walk_stmt(s, err_binds); }
            }
            EK::While { cond, body } => {
                self.check_condition(cond, err_binds);
                self.walk_expr(cond, err_binds);
                for s in body { self.walk_stmt(s, err_binds); }
            }
            EK::Lambda { body, .. } => self.walk_expr(body, err_binds),
            EK::Try { expr } | EK::Unwrap { expr } | EK::ToOption { expr }
            | EK::Paren { expr } | EK::Some { expr } | EK::Ok { expr } | EK::Err { expr }
            | EK::TypeAscription { expr, .. } | EK::OptionalChain { expr, .. } => {
                self.walk_expr(expr, err_binds)
            }
            EK::UnwrapOr { expr, fallback } => {
                self.walk_expr(expr, err_binds);
                self.walk_expr(fallback, err_binds);
            }
            EK::Binary { left, right, .. } => {
                self.walk_expr(left, err_binds);
                self.walk_expr(right, err_binds);
            }
            EK::Unary { operand, .. } => self.walk_expr(operand, err_binds),
            EK::Range { start, end, .. } => {
                self.walk_expr(start, err_binds);
                self.walk_expr(end, err_binds);
            }
            _ => {}
        }
    }

    fn walk_stmt(&mut self, stmt: &ast::Stmt, err_binds: &[Sym]) {
        use ast::Stmt as S;
        match stmt {
            S::Let { value, .. } | S::Var { value, .. } | S::Assign { value, .. }
            | S::LetDestructure { value, .. } | S::FieldAssign { value, .. } => {
                self.walk_expr(value, err_binds)
            }
            S::IndexAssign { index, value, .. } => {
                self.walk_expr(index, err_binds);
                self.walk_expr(value, err_binds);
            }
            S::Guard { cond, else_, .. } => {
                self.check_condition(cond, err_binds);
                self.walk_expr(cond, err_binds);
                self.walk_expr(else_, err_binds);
            }
            S::GuardLet { scrutinee, else_, .. } => {
                self.walk_expr(scrutinee, err_binds);
                self.walk_expr(else_, err_binds);
            }
            S::Expr { expr, .. } => self.walk_expr(expr, err_binds),
            _ => {}
        }
    }

    /// E035: fire on `string.contains(e, …)` / `e.contains(…)` /
    /// `e == "lit"` / `e != "lit"` inside a branch condition, where `e` is an
    /// err-pattern binding. Only the condition's top-level structure (through
    /// parens and and/or chains) is inspected — conservative by design.
    fn check_condition(&mut self, cond: &ast::Expr, err_binds: &[Sym]) {
        use ast::ExprKind as EK;
        if err_binds.is_empty() { return; }
        match &cond.kind {
            EK::Paren { expr } => self.check_condition(expr, err_binds),
            EK::Unary { operand, .. } => self.check_condition(operand, err_binds),
            EK::Binary { op, left, right } => {
                let op = op.as_str();
                if op == "and" || op == "or" {
                    self.check_condition(left, err_binds);
                    self.check_condition(right, err_binds);
                } else if op == "==" || op == "!=" {
                    let is_err_ident = |e: &ast::Expr| matches!(&e.kind,
                        EK::Ident { name } if err_binds.contains(name));
                    let is_str_lit = |e: &ast::Expr| matches!(&e.kind, EK::String { .. });
                    if (is_err_ident(left) && is_str_lit(right))
                        || (is_str_lit(left) && is_err_ident(right)) {
                        self.emit_e035(cond);
                    }
                }
            }
            EK::Call { callee, args, .. } => {
                let arg_is_err = |e: &ast::Expr| matches!(&e.kind,
                    EK::Ident { name } if err_binds.contains(name));
                match &callee.kind {
                    // string.contains(e, …) — module spelling
                    EK::Member { object, field } if field.as_str() == "contains" => {
                        let module_form = matches!(&object.kind,
                            EK::Ident { name } if name.as_str() == "string");
                        if (module_form && args.first().is_some_and(arg_is_err))
                            // e.contains(…) — UFCS spelling
                            || arg_is_err(object) {
                            self.emit_e035(cond);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn emit_e035(&mut self, at: &ast::Expr) {
        let mut d = Diagnostic::warning(
            "branching on the text of an error message",
            "Error message text is a report, not an API — it may be reworded. \
             If callers must branch on the failure kind, define a variant error type \
             (`type MyError = | NotFound(String) | …`) and match on it (ADR-0004). \
             For a kind-independent fallback use `??`",
            "error-text comparison",
        ).with_code("E035");
        if let Some(s) = at.span {
            d.line = Some(s.line);
            d.col = Some(s.col);
        }
        self.diags.push(d);
    }

    /// E036: `map_err` lambda whose (named) error parameter is never read.
    fn check_maperr_lambda(&mut self, arg: &ast::Expr) {
        use ast::ExprKind as EK;
        let EK::Lambda { params, body } = &arg.kind else { return };
        let [param] = params.as_slice() else { return };
        if param.name.as_str() == "_" { return; }
        if !expr_uses_ident(body, param.name) {
            let mut d = Diagnostic::warning(
                format!("map_err's lambda never uses the error value '{}' — the original error is silently discarded", param.name.as_str()),
                format!("To keep the original error, interpolate it: (({p}) => \"context: ${{{p}}}\"). \
                         To discard it deliberately, write (_) => …", p = param.name.as_str()),
                "map_err lambda",
            ).with_code("E036");
            if let Some(s) = arg.span {
                d.line = Some(s.line);
                d.col = Some(s.col);
            }
            self.diags.push(d);
        }
    }
}

fn collect_err_pattern_binds(pat: &ast::Pattern, out: &mut Vec<Sym>) {
    use ast::Pattern as P;
    match pat {
        P::Err { inner } => {
            if let P::Ident { name } = inner.as_ref() { out.push(*name); }
        }
        P::Constructor { args, .. } => {
            for a in args { collect_err_pattern_binds(a, out); }
        }
        P::Tuple { elements } | P::List { elements } => {
            for e in elements { collect_err_pattern_binds(e, out); }
        }
        P::Some { inner } | P::Ok { inner } => collect_err_pattern_binds(inner, out),
        _ => {}
    }
}

/// True when `name` appears as an identifier anywhere in `expr` (including
/// interpolation segments). Shadowing is ignored on purpose — any appearance
/// counts as a use, keeping the lint free of false positives.
fn expr_uses_ident(expr: &ast::Expr, name: Sym) -> bool {
    use ast::ExprKind as EK;
    match &expr.kind {
        EK::Ident { name: n } => *n == name,
        EK::InterpolatedString { parts } => parts.iter().any(|p| match p {
            ast::StringPart::Expr { expr } => expr_uses_ident(expr, name),
            _ => false,
        }),
        EK::List { elements } | EK::Tuple { elements } =>
            elements.iter().any(|e| expr_uses_ident(e, name)),
        EK::MapLiteral { entries } =>
            entries.iter().any(|(k, v)| expr_uses_ident(k, name) || expr_uses_ident(v, name)),
        EK::Record { fields, .. } => fields.iter().any(|f| expr_uses_ident(&f.value, name)),
        EK::SpreadRecord { base, fields } =>
            expr_uses_ident(base, name) || fields.iter().any(|f| expr_uses_ident(&f.value, name)),
        EK::Call { callee, args, named_args, .. } =>
            expr_uses_ident(callee, name)
                || args.iter().any(|a| expr_uses_ident(a, name))
                || named_args.iter().any(|(_, a)| expr_uses_ident(a, name)),
        EK::Member { object, .. } | EK::TupleIndex { object, .. } => expr_uses_ident(object, name),
        EK::IndexAccess { object, index } =>
            expr_uses_ident(object, name) || expr_uses_ident(index, name),
        EK::Pipe { left, right } | EK::Compose { left, right } =>
            expr_uses_ident(left, name) || expr_uses_ident(right, name),
        EK::If { cond, then, else_ } =>
            expr_uses_ident(cond, name) || expr_uses_ident(then, name) || expr_uses_ident(else_, name),
        EK::IfLet { scrutinee, then, else_, .. } =>
            expr_uses_ident(scrutinee, name) || expr_uses_ident(then, name) || expr_uses_ident(else_, name),
        EK::Match { subject, arms } =>
            expr_uses_ident(subject, name) || arms.iter().any(|a|
                a.guard.as_ref().is_some_and(|g| expr_uses_ident(g, name))
                    || expr_uses_ident(&a.body, name)),
        EK::Block { stmts, expr } => {
            stmts.iter().any(|s| stmt_uses_ident(s, name))
                || expr.as_ref().is_some_and(|e| expr_uses_ident(e, name))
        }
        EK::Fan { exprs } | EK::FanRace { arms: exprs, .. } | EK::FanSettle { arms: exprs } =>
            exprs.iter().any(|e| expr_uses_ident(e, name)),
        EK::FanBounded { budget, body } =>
            expr_uses_ident(budget, name) || expr_uses_ident(body, name),
        EK::FanRaceMap { budget, list, mapper } =>
            budget.as_ref().is_some_and(|b| expr_uses_ident(b, name))
                || expr_uses_ident(list, name) || expr_uses_ident(mapper, name),
        EK::FanTimeout { deadline, body } =>
            expr_uses_ident(deadline, name) || expr_uses_ident(body, name),
        EK::ForIn { iterable, body, .. } =>
            expr_uses_ident(iterable, name) || body.iter().any(|s| stmt_uses_ident(s, name)),
        EK::While { cond, body } =>
            expr_uses_ident(cond, name) || body.iter().any(|s| stmt_uses_ident(s, name)),
        EK::Lambda { body, .. } => expr_uses_ident(body, name),
        EK::Try { expr } | EK::Unwrap { expr } | EK::ToOption { expr }
        | EK::Paren { expr } | EK::Some { expr } | EK::Ok { expr } | EK::Err { expr }
        | EK::TypeAscription { expr, .. } | EK::OptionalChain { expr, .. } =>
            expr_uses_ident(expr, name),
        EK::UnwrapOr { expr, fallback } =>
            expr_uses_ident(expr, name) || expr_uses_ident(fallback, name),
        EK::Binary { left, right, .. } =>
            expr_uses_ident(left, name) || expr_uses_ident(right, name),
        EK::Unary { operand, .. } => expr_uses_ident(operand, name),
        EK::Range { start, end, .. } =>
            expr_uses_ident(start, name) || expr_uses_ident(end, name),
        _ => false,
    }
}

fn stmt_uses_ident(stmt: &ast::Stmt, name: Sym) -> bool {
    use ast::Stmt as S;
    match stmt {
        S::Let { value, .. } | S::Var { value, .. } | S::Assign { value, .. }
        | S::LetDestructure { value, .. } | S::FieldAssign { value, .. } =>
            expr_uses_ident(value, name),
        S::IndexAssign { index, value, .. } =>
            expr_uses_ident(index, name) || expr_uses_ident(value, name),
        S::Guard { cond, else_, .. } =>
            expr_uses_ident(cond, name) || expr_uses_ident(else_, name),
        S::GuardLet { scrutinee, else_, .. } =>
            expr_uses_ident(scrutinee, name) || expr_uses_ident(else_, name),
        S::Expr { expr, .. } => expr_uses_ident(expr, name),
        _ => false,
    }
}
