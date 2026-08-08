
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

    /// Recurse into every sub-expression of a node the lint has no rule for.
    ///
    /// Arms are grouped by CHILD SHAPE, not by node family: every variant whose
    /// traversal is "walk one child" shares one arm, "walk two children" the
    /// next, and so on. Or-pattern binding renames (`Member { object: e, .. }`)
    /// line the field names up, and the shapes with their own order (blocks,
    /// loops, calls) delegate to a named helper.
    fn walk_children(&mut self, expr: &ast::Expr, err_binds: &[Sym]) {
        use ast::ExprKind as EK;
        match &expr.kind {
            // ── One child ──
            EK::Member { object: e, .. } | EK::TupleIndex { object: e, .. }
            | EK::Lambda { body: e, .. } | EK::Unary { operand: e, .. }
            | EK::Try { expr: e } | EK::Unwrap { expr: e } | EK::ToOption { expr: e }
            | EK::Paren { expr: e } | EK::Some { expr: e } | EK::Ok { expr: e }
            | EK::Err { expr: e } | EK::TypeAscription { expr: e, .. }
            | EK::OptionalChain { expr: e, .. } => self.walk_expr(e, err_binds),

            // ── Two children, left to right ──
            EK::IndexAccess { object: a, index: b }
            | EK::Pipe { left: a, right: b } | EK::Compose { left: a, right: b }
            | EK::UnwrapOr { expr: a, fallback: b }
            | EK::Binary { left: a, right: b, .. }
            | EK::Range { start: a, end: b, .. }
            | EK::FanBounded { budget: a, body: b }
            | EK::FanTimeout { deadline: a, body: b } => {
                self.walk_expr(a, err_binds);
                self.walk_expr(b, err_binds);
            }

            // ── Three children ──
            EK::IfLet { scrutinee: a, then: b, else_: c, .. }
            | EK::FanRaceMap { budget: Some(a), list: b, mapper: c } => {
                self.walk_expr(a, err_binds);
                self.walk_expr(b, err_binds);
                self.walk_expr(c, err_binds);
            }
            EK::FanRaceMap { budget: None, list, mapper } => {
                self.walk_expr(list, err_binds);
                self.walk_expr(mapper, err_binds);
            }

            // ── A flat sequence of children ──
            EK::List { elements: xs } | EK::Tuple { elements: xs } | EK::Fan { exprs: xs }
            | EK::FanRace { arms: xs, .. } | EK::FanSettle { arms: xs } => {
                for e in xs {
                    self.walk_expr(e, err_binds);
                }
            }

            // ── Shapes with their own traversal order ──
            EK::InterpolatedString { parts } => self.walk_interpolation(parts, err_binds),
            EK::MapLiteral { entries } => {
                for (k, v) in entries {
                    self.walk_expr(k, err_binds);
                    self.walk_expr(v, err_binds);
                }
            }
            EK::Record { fields, .. } => self.walk_record_fields(fields, err_binds),
            EK::SpreadRecord { base, fields } => {
                self.walk_expr(base, err_binds);
                self.walk_record_fields(fields, err_binds);
            }
            EK::Call { callee, args, named_args, .. } => {
                self.walk_expr(callee, err_binds);
                for a in args.iter().chain(named_args.iter().map(|(_, a)| a)) {
                    self.walk_expr(a, err_binds);
                }
            }
            EK::Block { stmts, expr } => self.walk_block(stmts, expr.as_deref(), err_binds),
            EK::ForIn { iterable, body, .. } => {
                self.walk_expr(iterable, err_binds);
                self.walk_body(body, err_binds);
            }
            // A `while` condition is a branch condition, so it gets the E035 check.
            EK::While { cond, body } => {
                self.check_condition(cond, err_binds);
                self.walk_expr(cond, err_binds);
                self.walk_body(body, err_binds);
            }
            _ => {}
        }
    }

    /// The interpolated sub-expressions of a `"${..}"` literal.
    fn walk_interpolation(&mut self, parts: &[ast::StringPart], err_binds: &[Sym]) {
        for p in parts {
            if let ast::StringPart::Expr { expr } = p {
                self.walk_expr(expr, err_binds);
            }
        }
    }

    /// The value of every field in a record or spread-record literal.
    fn walk_record_fields(&mut self, fields: &[ast::FieldInit], err_binds: &[Sym]) {
        for f in fields {
            self.walk_expr(&f.value, err_binds);
        }
    }

    /// A block: every statement, then the tail expression.
    fn walk_block(&mut self, stmts: &[ast::Stmt], tail: Option<&ast::Expr>, err_binds: &[Sym]) {
        self.walk_body(stmts, err_binds);
        if let Some(e) = tail {
            self.walk_expr(e, err_binds);
        }
    }

    /// A statement list (block body, loop body).
    fn walk_body(&mut self, stmts: &[ast::Stmt], err_binds: &[Sym]) {
        for s in stmts {
            self.walk_stmt(s, err_binds);
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
        if err_binds.is_empty() {
            return;
        }
        match &cond.kind {
            // Transparent wrappers: keep looking at what they wrap.
            EK::Paren { expr: inner } | EK::Unary { operand: inner, .. } => {
                self.check_condition(inner, err_binds)
            }
            EK::Binary { op, left, right } => {
                self.check_binary_condition(cond, op.as_str(), left, right, err_binds)
            }
            EK::Call { callee, args, .. } => {
                if contains_call_on_err(callee, args, err_binds) {
                    self.emit_e035(cond);
                }
            }
            _ => {}
        }
    }

    /// The `Binary` arm of [`Self::check_condition`]: `and`/`or` keep the walk
    /// going down both sides, `==`/`!=` between an err binding and a string
    /// literal IS the report.
    fn check_binary_condition(
        &mut self,
        cond: &ast::Expr,
        op: &str,
        left: &ast::Expr,
        right: &ast::Expr,
        err_binds: &[Sym],
    ) {
        if op == "and" || op == "or" {
            self.check_condition(left, err_binds);
            self.check_condition(right, err_binds);
            return;
        }
        let compares_err_to_literal = (op == "==" || op == "!=")
            && ((is_err_ident(left, err_binds) && is_str_lit(right))
                || (is_str_lit(left) && is_err_ident(right, err_binds)));
        if compares_err_to_literal {
            self.emit_e035(cond);
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

/// `e`, where `e` is one of the err-pattern bindings in scope.
fn is_err_ident(e: &ast::Expr, err_binds: &[Sym]) -> bool {
    matches!(&e.kind, ast::ExprKind::Ident { name } if err_binds.contains(name))
}

/// A string literal — the right-hand side of the comparison E035 reports.
fn is_str_lit(e: &ast::Expr) -> bool {
    matches!(&e.kind, ast::ExprKind::String { .. })
}

/// `string.contains(e, …)` (module spelling) or `e.contains(…)` (UFCS), where
/// `e` is an err-pattern binding.
fn contains_call_on_err(callee: &ast::Expr, args: &[ast::Expr], err_binds: &[Sym]) -> bool {
    use ast::ExprKind as EK;
    let EK::Member { object, field } = &callee.kind else { return false };
    if field.as_str() != "contains" {
        return false;
    }
    let module_form = matches!(&object.kind, EK::Ident { name } if name.as_str() == "string");
    (module_form && args.first().is_some_and(|a| is_err_ident(a, err_binds)))
        || is_err_ident(object, err_binds)
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
    let uses = |e: &ast::Expr| expr_uses_ident(e, name);
    match &expr.kind {
        EK::Ident { name: n } => *n == name,

        // ── One child ──
        EK::Member { object: e, .. } | EK::TupleIndex { object: e, .. }
        | EK::Lambda { body: e, .. } | EK::Unary { operand: e, .. }
        | EK::Try { expr: e } | EK::Unwrap { expr: e } | EK::ToOption { expr: e }
        | EK::Paren { expr: e } | EK::Some { expr: e } | EK::Ok { expr: e }
        | EK::Err { expr: e } | EK::TypeAscription { expr: e, .. }
        | EK::OptionalChain { expr: e, .. } => uses(e),

        // ── Two children ──
        EK::IndexAccess { object: a, index: b }
        | EK::Pipe { left: a, right: b } | EK::Compose { left: a, right: b }
        | EK::UnwrapOr { expr: a, fallback: b }
        | EK::Binary { left: a, right: b, .. }
        | EK::Range { start: a, end: b, .. }
        | EK::FanBounded { budget: a, body: b }
        | EK::FanTimeout { deadline: a, body: b } => uses(a) || uses(b),

        // ── Three children ──
        EK::If { cond: a, then: b, else_: c }
        | EK::IfLet { scrutinee: a, then: b, else_: c, .. } => uses(a) || uses(b) || uses(c),
        EK::FanRaceMap { budget, list, mapper } => {
            budget.as_ref().is_some_and(|b| uses(b)) || uses(list) || uses(mapper)
        }

        // ── A flat sequence of children ──
        EK::List { elements: xs } | EK::Tuple { elements: xs } | EK::Fan { exprs: xs }
        | EK::FanRace { arms: xs, .. } | EK::FanSettle { arms: xs } => xs.iter().any(uses),

        // ── Shapes with their own traversal ──
        EK::InterpolatedString { parts } => parts.iter().any(|p| match p {
            ast::StringPart::Expr { expr } => uses(expr),
            _ => false,
        }),
        EK::MapLiteral { entries } => entries.iter().any(|(k, v)| uses(k) || uses(v)),
        EK::Record { fields, .. } => fields.iter().any(|f| uses(&f.value)),
        EK::SpreadRecord { base, fields } => uses(base) || fields.iter().any(|f| uses(&f.value)),
        EK::Call { callee, args, named_args, .. } => {
            uses(callee)
                || args.iter().any(uses)
                || named_args.iter().any(|(_, a)| uses(a))
        }
        EK::Match { subject, arms } => {
            uses(subject)
                || arms.iter().any(|a| {
                    a.guard.as_ref().is_some_and(|g| uses(g)) || uses(&a.body)
                })
        }
        EK::Block { stmts, expr } => {
            stmts.iter().any(|s| stmt_uses_ident(s, name))
                || expr.as_ref().is_some_and(|e| uses(e))
        }
        EK::ForIn { iterable: lead, body, .. } | EK::While { cond: lead, body } => {
            uses(lead) || body.iter().any(|s| stmt_uses_ident(s, name))
        }
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
