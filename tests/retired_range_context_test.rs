//! #2109: invalid range positions explain the grammar without unsafe fix-its.
use almide::{lexer::Lexer, parser::Parser};

#[test]
fn retired_ranges_report_the_enclosing_grammar() {
    for (source, hint) in [
        ("fn f(n: Int) -> Int = match n { 0..3 => 1, _ => 0 }", "Range patterns do not exist"),
        ("fn f(n: Int) -> Int = match n { 0..=3 => 1, _ => 0 }", "Range patterns do not exist"),
        ("fn f(ys: List[Int]) -> List[Int] = [..ys, 1]", "To combine lists"),
        ("fn f(ys: List[Int]) -> Int = list.len(..ys)", "Calls do not support argument spread"),
        ("let xs: List[0..3] = [1]", "Range types do not exist"),
        ("let xs: List[0..=3] = [1]", "Range types do not exist"),
    ] {
        let mut parser = Parser::new(Lexer::tokenize(source));
        let _ = parser.parse();
        let diagnostic = parser.errors.iter().find(|d| d.code.as_deref() == Some("E031"))
            .unwrap_or_else(|| panic!("{source}\n{:?}", parser.errors));
        assert!(diagnostic.hint.contains(hint), "{diagnostic:?}");
        assert_eq!(diagnostic.line, Some(1));
        assert_eq!(diagnostic.col, source.find("..").map(|offset| offset + 1));
        assert!(diagnostic.machine_fix().is_none(), "unsupported forms must not be rewritten");
    }
}

#[test]
fn legal_rest_patterns_and_ranges_still_parse() {
    for source in [
        "fn f(xs: List[Int]) -> Int = match xs { [h, ..t] => h, _ => 0 }",
        "fn f() -> List[Int] = 0..<3",
        "fn f() -> List[Int] = 0...3",
        "type R = { x: Int, y: Int }\nfn f(r: R) -> Int = match r { R { x, .. } => x }",
    ] {
        let mut parser = Parser::new(Lexer::tokenize(source));
        let result = parser.parse();
        assert!(result.is_ok() && parser.errors.is_empty(), "{source}\n{:?}", parser.errors);
    }
}
