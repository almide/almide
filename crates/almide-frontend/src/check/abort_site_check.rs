// E098 / E099 (ADR-0029 D2): where `testing.assert_aborts` may be called and
// what its closure may read. The closure runs in a child process (native) or a
// fresh instance (wasm) that starts the program afresh, so the test's locals
// do not exist there; and a call outside a test block has no harness to report
// to. Spliced into check/mod.rs; runs once per program after inference.

impl Checker {
    pub(crate) fn check_abort_sites(&mut self, program: &ast::Program) {
        let table = &self.env.import_table;
        let names = AbortNames {
            aliases: table.aliases.iter().filter(|(_, m)| m.as_str() == "testing").map(|(a, _)| *a).collect(),
            direct: table.direct.get(&sym("assert_aborts")).is_some_and(|m| m.as_str() == "testing"),
        };
        if names.aliases.is_empty() && !names.direct {
            return;
        }
        let mut found: Vec<AbortFinding> = Vec::new();
        let test_where: Vec<String> = program.decls.iter().flat_map(test_where_def_binders).collect();
        for decl in &program.decls {
            match decl {
                ast::Decl::Test { body, where_clauses, .. } => {
                    let mut locals = test_where.clone();
                    locals.extend(where_clauses.iter().flat_map(where_binders));
                    collect_binders(body, &mut locals);
                    names.check_test_body(body, &locals, &mut found);
                }
                ast::Decl::Fn { body: Some(body), .. } | ast::Decl::TopLet { value: body, .. } => {
                    names.each_reference(body, &mut |e| found.push(AbortFinding::OutsideTest(e.span)));
                }
                _ => {}
            }
        }
        for f in found {
            let d = abort_site_diagnostic(f, self.source_file.clone());
            self.diagnostics.push(d);
        }
    }
}

/// One refused `assert_aborts` shape.
enum AbortFinding {
    /// E098: the closure reads `name`, a local of the test.
    Captures(Sym, Option<ast::Span>),
    /// E098: the closure argument is not a lambda written in place.
    NotALambda(Option<String>, Option<ast::Span>),
    /// E099: a call in a `fn`, `main` or a top-level `let`.
    OutsideTest(Option<ast::Span>),
    /// E099: the fn named as a value.
    FirstClass(Option<ast::Span>),
    /// E099: a call inside another call's closure.
    Nested(Option<ast::Span>),
}

/// How this file spells `testing.assert_aborts`.
struct AbortNames {
    /// The import aliases of `testing` (`import testing as t` gives `t`).
    aliases: Vec<Sym>,
    /// `import testing.{assert_aborts}`: the bare name is the fn.
    direct: bool,
}

impl AbortNames {
    /// Is `e` a spelling of the fn itself?
    fn names_it(&self, e: &ast::Expr) -> bool {
        match &e.kind {
            ast::ExprKind::Member { object, field } if field.as_str() == "assert_aborts" => {
                matches!(&object.kind, ast::ExprKind::Ident { name } if self.aliases.contains(name))
            }
            ast::ExprKind::Ident { name } => self.direct && name.as_str() == "assert_aborts",
            _ => false,
        }
    }

    /// Every spelling of the fn under `root`, in source order.
    fn each_reference(&self, root: &ast::Expr, on: &mut impl FnMut(&ast::Expr)) {
        ast::visit_expr(root, &mut |e| {
            if self.names_it(e) {
                on(e);
            }
        });
    }

    /// A test body: each direct call is checked (its closure's captures, a
    /// call nested in it); any other spelling is a first-class use.
    fn check_test_body(&self, body: &ast::Expr, locals: &[String], found: &mut Vec<AbortFinding>) {
        let mut callees: Vec<*const ast::Expr> = Vec::new();
        let mut nested: Vec<*const ast::Expr> = Vec::new();
        ast::visit_expr(body, &mut |e| {
            let ast::ExprKind::Call { callee, args, .. } = &e.kind else { return };
            if !self.names_it(callee) || nested.contains(&(&**callee as *const ast::Expr)) {
                return;
            }
            callees.push(&**callee);
            if let Some(closure) = args.last() {
                self.each_reference(closure, &mut |inner| nested.push(inner));
                check_closure(closure, locals, found);
            }
        });
        self.each_reference(body, &mut |e| {
            let ptr = e as *const ast::Expr;
            if nested.contains(&ptr) {
                found.push(AbortFinding::Nested(e.span));
            } else if !callees.contains(&ptr) {
                found.push(AbortFinding::FirstClass(e.span));
            }
        });
    }
}

/// E098 over one closure argument: a lambda whose free names are not locals
/// of the test. A name the lambda binds anywhere inside it counts as its own.
fn check_closure(closure: &ast::Expr, test_locals: &[String], found: &mut Vec<AbortFinding>) {
    let closure = match &closure.kind {
        ast::ExprKind::Paren { expr } => &**expr,
        _ => closure,
    };
    let ast::ExprKind::Lambda { .. } = &closure.kind else {
        let name = match &closure.kind {
            ast::ExprKind::Ident { name } => Some(name.to_string()),
            _ => None,
        };
        found.push(AbortFinding::NotALambda(name, closure.span));
        return;
    };
    let mut own: Vec<String> = Vec::new();
    collect_binders(closure, &mut own);
    let mut seen: Vec<Sym> = Vec::new();
    ast::visit_expr(closure, &mut |e| {
        let ast::ExprKind::Ident { name } = &e.kind else { return };
        let n = name.as_str();
        if test_locals.iter().any(|l| l == n) && !own.iter().any(|l| l == n) && !seen.contains(name) {
            seen.push(*name);
            found.push(AbortFinding::Captures(*name, e.span));
        }
    });
}

/// Every name bound anywhere under `root`: lambda parameters, `let` / `var`
/// (destructuring included), `guard let`, match-arm patterns, `for` and
/// `if let` binders.
fn collect_binders(root: &ast::Expr, out: &mut Vec<String>) {
    ast::visit_expr(root, &mut |e| match &e.kind {
        ast::ExprKind::Lambda { params, .. } => {
            for p in params {
                out.push(p.name.to_string());
                out.extend(p.tuple_names.iter().flatten().map(|n| n.to_string()));
            }
        }
        ast::ExprKind::Block { stmts, .. } => stmts.iter().for_each(|s| stmt_binders(s, out)),
        ast::ExprKind::ForIn { var, var_tuple, body, .. } => {
            out.push(var.to_string());
            out.extend(var_tuple.iter().flatten().map(|n| n.to_string()));
            body.iter().for_each(|s| stmt_binders(s, out));
        }
        ast::ExprKind::While { body, .. } => body.iter().for_each(|s| stmt_binders(s, out)),
        ast::ExprKind::Match { arms, .. } => arms.iter().for_each(|a| pattern_binders(&a.pattern, out)),
        ast::ExprKind::IfLet { name, .. } => out.push(name.to_string()),
        _ => {}
    });
}

fn stmt_binders(s: &ast::Stmt, out: &mut Vec<String>) {
    match s {
        ast::Stmt::Let { name, .. } | ast::Stmt::Var { name, .. } | ast::Stmt::GuardLet { name, .. } => out.push(name.to_string()),
        ast::Stmt::LetDestructure { pattern, .. } => pattern_binders(pattern, out),
        _ => {}
    }
}

/// The names a test's own `where` clauses bind (a table case's bindings too).
fn where_binders(w: &ast::TestWhere) -> Vec<String> {
    match w {
        ast::TestWhere::Bind { name, .. } => vec![name.to_string()],
        ast::TestWhere::Case { bindings, .. } => bindings.iter().flat_map(where_binders).collect(),
        _ => Vec::new(),
    }
}

/// The names a file-level `local test where { … }` binds for every test.
fn test_where_def_binders(d: &ast::Decl) -> Vec<String> {
    match d {
        ast::Decl::TestWhereDef { clauses, .. } => clauses.iter().flat_map(where_binders).collect(),
        _ => Vec::new(),
    }
}

fn abort_site_diagnostic(f: AbortFinding, file: Option<String>) -> Diagnostic {
    const ELSEWHERE: &str = "the closure runs in a separate process (native) or instance (wasm), where the test's locals do not exist.";
    let (code, message, hint, span) = match f {
        AbortFinding::Captures(name, span) => (
            "E098",
            format!("the closure passed to testing.assert_aborts captures `{name}`, a local of the test"),
            format!("{ELSEWHERE} Inline the value into the closure, or make it a top-level `let`."),
            span,
        ),
        AbortFinding::NotALambda(name, span) => (
            "E098",
            match name {
                Some(n) => format!("the closure passed to testing.assert_aborts must be a lambda written in place; `{n}` is a fn value"),
                None => "the closure passed to testing.assert_aborts must be a lambda written in place".to_string(),
            },
            format!("{ELSEWHERE} What a fn value captures cannot be seen here. Write the lambda in the call: `testing.assert_aborts(\"…\", () => …)`."),
            span,
        ),
        AbortFinding::OutsideTest(span) => (
            "E099",
            "testing.assert_aborts is called outside a test block".to_string(),
            "assert_aborts runs only in a test: move the call into a `test \"…\" { }` block. A program that wants to abort just aborts.".to_string(),
            span,
        ),
        AbortFinding::FirstClass(span) => (
            "E099",
            "testing.assert_aborts is used as a value".to_string(),
            "call it directly in a test block: `testing.assert_aborts(\"expected\", () => …)` — the call site is what names the closure the child runs".to_string(),
            span,
        ),
        AbortFinding::Nested(span) => (
            "E099",
            "testing.assert_aborts is called inside another assert_aborts closure".to_string(),
            "the closure already runs in a child, and a call there would need a grandchild: write one call per abort, side by side in the test".to_string(),
            span,
        ),
    };
    // Spelled out per code: the coverage gates read `with_code("E…")` literals.
    let d = Diagnostic::error(message, hint, "a call of testing.assert_aborts");
    let mut d = if code == "E098" { d.with_code("E098") } else { d.with_code("E099") };
    d.file = file;
    if let Some(s) = span {
        d.line = Some(s.line);
        d.col = Some(s.col);
        d.end_col = Some(s.end_col);
    }
    d
}
