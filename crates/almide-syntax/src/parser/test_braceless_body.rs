//! #3370: an indented line after a braceless `fn … =` body names the cause
//! and shows the braced rewrite; every other stray top-level token keeps the
//! generic "Expected top-level declaration".

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use crate::lexer::Lexer;
    use super::super::Parser;

    fn first_error(src: &str) -> Diagnostic {
        let mut parser = Parser::new(Lexer::tokenize(src));
        let _ = parser.parse();
        parser.errors.first().cloned().expect("an error")
    }

    fn is_braces_diag(d: &Diagnostic) -> bool {
        d.message.contains("a body without braces is a single expression")
    }

    #[test]
    fn indented_second_statement_suggests_the_braced_body() {
        let d = first_error(
            "fn main() -> Unit =\n  println(\"Hello, world!\")\n  hoge()\n\nfn hoge() -> Unit =\n  println(\"hogehoge\")\n",
        );
        assert!(is_braces_diag(&d), "{}", d.message);
        assert_eq!((d.line, d.col), (Some(3), Some(3)));
        assert!(d.hint.contains("`fn main` ends after its first expression"), "{}", d.hint);
        assert_eq!(
            d.try_snippet.as_deref(),
            Some("fn main() -> Unit = {\n  println(\"Hello, world!\")\n  hoge()\n}")
        );
    }

    #[test]
    fn body_on_the_eq_line_moves_under_the_brace() {
        let d = first_error("effect fn main() -> Unit = println(\"a\")\n    hoge()\n    let x = 1\n\nfn hoge() -> Unit = ()\n");
        assert!(is_braces_diag(&d), "{}", d.message);
        assert!(d.message.contains("`effect fn main`"), "{}", d.message);
        assert_eq!(
            d.try_snippet.as_deref(),
            Some("effect fn main() -> Unit = {\n    println(\"a\")\n    hoge()\n    let x = 1\n}")
        );
    }

    #[test]
    fn an_indented_loop_is_the_body_not_a_top_level_loop() {
        let d = first_error("fn main() -> Unit =\n  println(\"a\")\n  for i in 0..<3 {\n    println(\"${i}\")\n  }\n");
        assert!(is_braces_diag(&d), "{}", d.message);
    }

    #[test]
    fn column_one_stray_keeps_the_generic_message() {
        let d = first_error("fn main() -> Unit = println(\"a\")\nhoge()\n");
        assert!(d.message.starts_with("Expected top-level declaration"), "{}", d.message);
    }

    #[test]
    fn braced_body_keeps_the_generic_message() {
        let d = first_error("fn main() -> Unit = {\n  println(\"a\")\n}\n  hoge()\n");
        assert!(d.message.starts_with("Expected top-level declaration"), "{}", d.message);
    }

    #[test]
    fn same_line_trailing_token_is_not_this_shape() {
        // `2` does not open its own line, so indentation says nothing about it.
        let mut parser = Parser::new(Lexer::tokenize("fn main() -> Int = 1 2\n"));
        let _ = parser.parse();
        assert!(!parser.errors.iter().any(is_braces_diag));
    }

    #[test]
    fn keyword_typo_hint_still_wins() {
        let d = first_error("fn f() -> Int =\n  1\n  return 2\n");
        assert!(d.message.contains("'return' is not needed"), "{}", d.message);
    }
}
