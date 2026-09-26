//! #2643: `rec` is an ordinary name. Only `let rec NAME …` — OCaml's
//! recursive binding — gets the "Almide fns are recursive" diagnostic;
//! `let rec = …` and `let rec: T = …` bind a local called `rec`.

#[cfg(test)]
mod tests {
    use crate::lexer::Lexer;
    use super::super::Parser;

    fn let_rec_diagnosed(src: &str) -> bool {
        let tokens = Lexer::tokenize(src);
        let mut parser = Parser::new(tokens);
        let _ = parser.parse();
        parser.errors.iter().any(|d| d.message.contains("`let rec`"))
    }

    fn parses_clean(src: &str) -> bool {
        let tokens = Lexer::tokenize(src);
        let mut parser = Parser::new(tokens);
        parser.parse().is_ok() && parser.errors.is_empty()
    }

    #[test]
    fn a_local_named_rec_binds() {
        assert!(parses_clean("fn f(x: Int) -> Int = {\n  let rec = x + 1\n  rec\n}\n"));
        assert!(parses_clean("fn f(x: Int) -> Int = {\n  let rec: Int = x + 1\n  rec\n}\n"));
    }

    #[test]
    fn ocaml_let_rec_is_still_diagnosed() {
        assert!(let_rec_diagnosed("fn main() -> Unit = {\n  let rec go = (n: Int) => n\n  ()\n}\n"));
        assert!(let_rec_diagnosed("fn main() -> Unit = {\n  let rec fact(n) = n\n  ()\n}\n"));
    }
}
