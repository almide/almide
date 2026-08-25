//! #1570: a tuple literal spans lines like every other bracketed literal —
//! newlines are insignificant between `(` and `)`, around elements and after
//! commas (trailing comma included). Pinned at the PARSER level because the
//! formatter collapses the wrapped spelling to one line, so a spec-corpus
//! file cannot hold the shape.
//!
//! #1569 rides along: the fallibility marker in a PROTOCOL method's return
//! position (`-> R!E` / `-> R!`) parses exactly as on a free fn.

#[cfg(test)]
mod tests {
    use crate::ast::{Decl, ExprKind};
    use crate::lexer::Lexer;
    use super::super::Parser;

    fn parse_expr_kind(src: &str) -> Result<ExprKind, String> {
        let tokens = Lexer::tokenize(src);
        let mut parser = Parser::new(tokens);
        parser.parse_expr().map(|e| e.kind)
    }

    fn parse_program_ok(src: &str) -> Result<(), String> {
        let tokens = Lexer::tokenize(src);
        let mut parser = Parser::new(tokens);
        parser.parse().map(|_| ()).map_err(|e| format!("{e:?}"))
    }

    #[test]
    fn tuple_literal_spans_lines() {
        let k = parse_expr_kind("(1,\n  2)").expect("wrapped pair must parse");
        assert!(matches!(k, ExprKind::Tuple { ref elements } if elements.len() == 2), "{k:?}");
        let k = parse_expr_kind("(\n  1,\n  \"two\",\n  3,\n)").expect("open-paren style must parse");
        assert!(matches!(k, ExprKind::Tuple { ref elements } if elements.len() == 3), "{k:?}");
        // A parenthesized GROUP wraps too, staying a group (not a tuple).
        let k = parse_expr_kind("(\n  1\n)").expect("wrapped group must parse");
        assert!(matches!(k, ExprKind::Paren { .. }), "{k:?}");
    }

    #[test]
    fn protocol_method_fallible_return_parses() {
        let src = "protocol P {\n  fn get(self, id: String) -> R!E\n  fn head(self) -> R!\n  effect fn tick(self) -> Int!\n}\n";
        parse_program_ok(src).expect("protocol fallible returns must parse");
        // The marker parses into the same `!` pseudo-generic a free fn gets.
        let tokens = Lexer::tokenize(src);
        let mut parser = Parser::new(tokens);
        let prog = parser.parse().unwrap();
        let Some(Decl::Protocol { methods, .. }) =
            prog.decls.iter().find(|d| matches!(d, Decl::Protocol { .. }))
        else {
            panic!("no protocol decl parsed");
        };
        assert_eq!(methods.len(), 3);
    }
}
