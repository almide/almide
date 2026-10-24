// E102 (R-regex-02, #3942): a pattern written as a string literal in
// `regex.new("…")` is checked here, with the runtime's own checker
// (regex_runtime.rs), so a pattern that would abort at run time with
// `Error: invalid regex: <reason>` is refused before the program runs. A
// pattern built at run time is the program's to check: `regex.compile`
// answers err(reason). Spliced into check/mod.rs; runs once per program after
// inference.

impl Checker {
    pub(crate) fn check_regex_literals(&mut self, program: &ast::Program) {
        let aliases: Vec<Sym> = self.env.import_table.aliases.iter().filter(|(_, m)| m.as_str() == "regex").map(|(a, _)| *a).collect();
        if aliases.is_empty() {
            return;
        }
        let mut found: Vec<(String, Option<ast::Span>)> = Vec::new();
        let mut on = |e: &ast::Expr| {
            let ast::ExprKind::Call { callee, args, .. } = &e.kind else { return };
            let ast::ExprKind::Member { object, field } = &callee.kind else { return };
            if field.as_str() != "new" || args.len() != 1 {
                return;
            }
            if !matches!(&object.kind, ast::ExprKind::Ident { name } if aliases.contains(name)) {
                return;
            }
            if let ast::ExprKind::String { value, .. } = &args[0].kind {
                if let Some(reason) = regex_runtime::check(value) {
                    found.push((reason, args[0].span));
                }
            }
        };
        for decl in &program.decls {
            match decl {
                ast::Decl::Fn { params, body, .. } => {
                    for p in params {
                        if let Some(d) = &p.default {
                            ast::visit_expr(d, &mut on);
                        }
                    }
                    if let Some(b) = body {
                        ast::visit_expr(b, &mut on);
                    }
                }
                ast::Decl::TopLet { value, .. } => ast::visit_expr(value, &mut on),
                ast::Decl::Test { body, .. } => ast::visit_expr(body, &mut on),
                _ => {}
            }
        }
        for (reason, span) in found {
            let mut d = Diagnostic::error(
                format!("invalid regex: {reason}"),
                "write the pattern in the syntax docs/stdlib/regex.md lists; a pattern built at run time goes through regex.compile, which answers err(reason) instead of aborting".to_string(),
                "the pattern passed to regex.new",
            )
            .with_code("E102");
            d.file = self.source_file.clone();
            if let Some(s) = span {
                d.line = Some(s.line);
                d.col = Some(s.col);
                d.end_col = Some(s.end_col);
            }
            self.diagnostics.push(d);
        }
    }
}
