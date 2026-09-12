//! #2107: trivia must not change record lookahead or require a trailing comma.
use almide::{lexer::Lexer, parser::Parser};

#[test]
fn record_comments_parse_in_expression_and_pattern_positions() {
    for source in [
        "type C = { a: Int, b: Int }\nfn f() -> C = C {\n// first\na: 1, b: 2\n}",
        "type C = { a: Int, b: Int }\nfn f(c: C) -> C = C {\n// spread\n...c, a: 2\n}",
        "type C = { a: Int, b: Int }\nfn f(c: C) -> Int = match c { C {\n// first\na,\n// next\nb\n// end\n} => a + b }",
        "type C = { a: Int, b: Int }\nfn f(c: C) -> Int = match c { C {\na: x, b: y\n} => x + y }",
    ] {
        let mut parser = Parser::new(Lexer::tokenize(source));
        let result = parser.parse();
        assert!(result.is_ok() && parser.errors.is_empty(), "{source}\n{:?}", parser.errors);
    }
}
