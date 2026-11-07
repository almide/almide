//! #4543: the guard-let forms. The plain `else err("…")` (a raise, no
//! arrow) must keep parsing as the else VALUE — only `pattern =>` after
//! `else` is a failure arm.

#[cfg(test)]
mod tests {
    use crate::ast::{Decl, ExprKind, Pattern, Stmt};
    use crate::lexer::Lexer;
    use super::super::Parser;

    fn parse(src: &str) -> (Option<Stmt>, Vec<String>) {
        let mut parser = Parser::new(Lexer::tokenize(src));
        let prog = parser.parse();
        let errors: Vec<String> = parser.errors.iter().map(|d| d.message.clone()).collect();
        let stmt = prog.ok().and_then(|p| {
            p.decls.into_iter().find_map(|d| match d {
                Decl::Fn { body: Some(body), .. } => match body.kind {
                    ExprKind::Block { stmts, .. } => stmts.into_iter().find(|s| matches!(s, Stmt::GuardLet { .. })),
                    _ => None,
                },
                _ => None,
            })
        });
        (stmt, errors)
    }

    fn wrap(stmt: &str) -> String {
        format!("fn f(r: Result[Int, String]) -> Int = {{\n  {stmt}\n  x\n}}\n")
    }

    #[test]
    fn a_raising_else_is_still_the_else_value() {
        let (s, errs) = parse(&wrap("guard let x = r else err(\"bad\")"));
        assert!(errs.is_empty(), "{errs:?}");
        let Some(Stmt::GuardLet { fail, mutable, shorthand, else_, .. }) = s else { panic!("no guard let") };
        assert!(fail.is_none() && !mutable && !shorthand);
        assert!(matches!(else_.kind, ExprKind::Err { .. }));
    }

    #[test]
    fn the_err_arm_binds_the_error() {
        let (s, errs) = parse(&wrap("guard let x = r else err(e) => 0"));
        assert!(errs.is_empty(), "{errs:?}");
        let Some(Stmt::GuardLet { fail: Some(Pattern::Err { inner }), .. }) = s else { panic!("no err arm") };
        assert!(matches!(*inner, Pattern::Ident { .. }));
    }

    #[test]
    fn the_none_arm_and_err_wildcard_parse() {
        let (s, errs) = parse(&wrap("guard let x = r else none => 0"));
        assert!(errs.is_empty(), "{errs:?}");
        assert!(matches!(s, Some(Stmt::GuardLet { fail: Some(Pattern::None), .. })));
        let (_, errs) = parse(&wrap("guard let x = r else err(_) => 0"));
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn shorthand_and_guard_var() {
        let (s, errs) = parse(&wrap("guard var r else err(e) => 0"));
        assert!(errs.is_empty(), "{errs:?}");
        let Some(Stmt::GuardLet { mutable, shorthand, scrutinee, fail, .. }) = s else { panic!("no guard var") };
        assert!(mutable && shorthand && fail.is_some());
        assert!(matches!(scrutinee.kind, ExprKind::Ident { name } if name.as_str() == "r"));
    }

    #[test]
    fn refusals_are_one_diagnostic_each() {
        let refused = [
            ("guard let r.x else 0", "names a field path"),
            ("guard let x = r else err(Some(p)) => 0", "binds the whole error"),
            ("guard let x = r else err(e) => 0, err(e) => 1", "has one failure arm"),
            ("guard let x = r else ok(v) => v", "names the failure side"),
            ("guard let x = r else err(e) if true => 0", "takes no `if` guard"),
            ("guard let x = r else _ => 0", "is not a `guard let` failure arm"),
        ];
        for (src, msg) in refused {
            let (_, errs) = parse(&wrap(src));
            assert_eq!(errs.iter().filter(|e| e.contains(msg)).count(), 1, "{src}: {errs:?}");
        }
    }
}
