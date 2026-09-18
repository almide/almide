//! #2106: comments survive, retain their owner, and formatting is idempotent.
use almide::{fmt, lexer::Lexer, parser::Parser};

fn format(source: &str) -> String {
    let mut parser = Parser::new(Lexer::tokenize(source));
    let program = parser.parse().expect("parse");
    assert!(parser.errors.is_empty(), "{source}\n{:?}", parser.errors);
    fmt::format_program(&program)
}

#[test]
fn comments_survive_variant_record_call_and_tuple_containers() {
    for source in [
        "type O =\n// first\n| A(String)\n// second\n| B\n// third\n| C",
        "type O = | A(String) // trailing\n| B",
        "type O = A(String)\n// second\n| B",
        "type O = A\n// second\n| B",
        "type O = | A{n: Int}\n// second\n| B",
        "type C = { a: Int, b: Int }\nfn f() -> C = C { a: 1,\n// second\nb: 2 }",
        "type C = { a: Int, b: Int }\nfn f() -> C = C {\n// first\na: 1,\n// second\nb: 2 }",
        "type C = { a: Int, b: Int }\nfn f() -> C = {\n// first\na: 1,\n// second\nb: 2,\n// end\n}",
        "type C = { a: Int, b: Int }\nfn f(a: Int) -> C = C { a: a,\n// second\nb: 2 // trailing\n}",
        "type C = { a: Int, b: Int }\nfn f(c: C) -> C = C {\n// base\n...c,\n// second\nb: 2 }",
        "type C = { a: Int, b: Int }\nfn f(c: C) -> C = {\n// base\n...c,\n// second\nb: 2 }",
        "fn add(a: Int, b: Int) -> Int = a + b\nfn f() -> Int = add(1,\n// second\n2)",
        "fn add(a: Int, b: Int) -> Int = a + b\nfn f() -> Int = add(\n// first\n1,\n// second\nb: 2)",
        "fn f() -> (Int, Int) = (1,\n// second\n2)",
    ] {
        let output = format(source);
        for line in source.lines() {
            if let Some((_, comment)) = line.split_once("//") {
                assert_eq!(output.matches(comment.trim()).count(), 1, "lost/duplicated {comment}:\n{output}");
            }
        }
        assert_eq!(format(&output), output, "not idempotent:\n{source}");
    }
}

/// #1404's rule is what decides the container's shape: only a comment that
/// cannot share a line forces physical lines. An inline `/* */` leading a
/// member leaves the container on one line, next to the member its author
/// wrote it against — in every bracket kind, and stably across passes.
#[test]
fn an_inline_leading_block_comment_keeps_its_container_on_one_line() {
    for (source, want) in [
        (
            "fn add(a: Int, b: Int) -> Int = a + b\nfn f() -> Int = add(/* why */ 1, 2)",
            "add(/* why */ 1, 2)",
        ),
        ("fn f() -> (Int, Int) = (/* left */ 1, 2)", "(/* left */ 1, 2)"),
        (
            "type C = { a: Int, b: Int }\nfn f() -> C = C { a: /* one */ 1, b: 2 }",
            "a: /* one */ 1, b: 2",
        ),
        ("fn f() -> List[Int] = [/* one */ 1, 2]", "[/* one */ 1, 2]"),
        (
            "fn f() -> Map[String, Int] = [/* k */ \"a\": 1]",
            "[/* k */ \"a\": 1]",
        ),
    ] {
        let output = format(source);
        assert!(output.contains(want), "expected `{want}` in:\n{output}");
        assert_eq!(output.matches("/*").count(), 1, "lost/duplicated comment:\n{output}");
        assert_eq!(format(&output), output, "not idempotent:\n{source}");
    }
}

/// The twin: a `//` in the same position still forces the physical lines
/// #2106 added, so the two rules cannot collapse into one.
#[test]
fn an_own_line_leading_comment_still_breaks_its_container() {
    let output = format("fn add(a: Int, b: Int) -> Int = a + b\nfn f() -> Int = add(\n// why\n1, 2)");
    assert!(output.contains("// why\n"), "comment not on its own line:\n{output}");
    assert!(output.contains("add(\n"), "container not broken:\n{output}");
    assert_eq!(format(&output), output, "not idempotent:\n{output}");
}
