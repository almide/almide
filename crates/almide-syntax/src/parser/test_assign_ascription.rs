//! #3051: the value of an assignment takes a trailing `: Type` ascription,
//! the inline form the E018 hint names. `s.xs = []` needs none (the field's
//! type is the expected type), but the ascribed spelling is accepted in every
//! assignment form — field, index and plain var — rather than failing to
//! parse at the `:`.

#[cfg(test)]
mod tests {
    use crate::ast::{Decl, ExprKind, Stmt};
    use crate::lexer::Lexer;
    use super::super::Parser;

    /// Parse `fn main` with `body` and return the statements of its block.
    fn stmts(body: &str) -> Vec<Stmt> {
        let src = format!("fn main() -> Unit = {{\n{body}\n  ()\n}}\n");
        let mut parser = Parser::new(Lexer::tokenize(&src));
        let prog = parser.parse().unwrap_or_else(|e| panic!("parse failed: {e}\n{src}"));
        assert!(parser.errors.is_empty(), "parse errors: {:?}\n{src}", parser.errors.iter().map(|d| &d.message).collect::<Vec<_>>());
        let Some(Decl::Fn { body: Some(body), .. }) = prog.decls.into_iter().next() else { panic!("no fn") };
        let ExprKind::Block { stmts, .. } = body.kind else { panic!("body is not a block") };
        stmts
    }

    fn value_is_ascribed(stmt: &Stmt) -> bool {
        let value = match stmt {
            Stmt::Assign { value, .. } | Stmt::FieldAssign { value, .. } | Stmt::IndexAssign { value, .. } => value,
            other => panic!("not an assignment: {other:?}"),
        };
        matches!(value.kind, ExprKind::TypeAscription { .. })
    }

    #[test]
    fn every_assignment_form_accepts_an_ascribed_value() {
        let got = stmts("  s.xs = []: List[Int]\n  m[\"k\"] = [:]: Map[String, Int]\n  x = []: List[String]");
        assert_eq!(got.len(), 3);
        assert!(got.iter().all(value_is_ascribed));
    }

    #[test]
    fn an_unascribed_value_is_unchanged() {
        let got = stmts("  s.xs = []\n  x = 1");
        assert!(got.iter().all(|s| !value_is_ascribed(s)));
    }
}
