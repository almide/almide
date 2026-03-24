use proptest::prelude::*;
use almide::lexer::Lexer;
use almide::parser::Parser;
use almide::check::Checker;

// ── Generators ──────────────────────────────────────────────────

/// Random bytes interpreted as UTF-8 (catches encoding edge cases)
fn arbitrary_source() -> impl Strategy<Value = String> {
    prop::string::string_regex(".{0,500}").unwrap()
}

/// Structured source that resembles valid Almide code
fn almide_like_source() -> impl Strategy<Value = String> {
    let keywords = prop::sample::select(vec![
        "let", "var", "fn", "if", "then", "else", "match", "for", "in",
        "type", "module", "import", "protocol", "impl", "do", "test",
        "ok", "err", "some", "none", "true", "false", "not", "and", "or",
        "effect", "pub", "strict", "guard", "break", "continue", "while",
        "local", "mod", "newtype", "fan", "todo",
    ]);
    let ops = prop::sample::select(vec![
        "+", "-", "*", "/", "%", "**", "==", "!=", "<", ">", "<=", ">=",
        "->", "=>", "=", "|>", ">>", "++", "&&", "||", "..", "..=", "...",
        "|", "^", "@", "!", ".",
    ]);
    let delimiters = prop::sample::select(vec![
        "(", ")", "{", "}", "[", "]", "<", ">", ",", ":", ";",
    ]);
    let idents = prop::sample::select(vec![
        "x", "y", "foo", "bar", "baz", "list", "map", "result",
        "Int", "String", "Bool", "Float", "List", "Option", "Result",
        "MyType", "Some", "None", "Ok", "Err",
    ]);
    let literals = prop_oneof![
        (0i64..10000).prop_map(|n| n.to_string()),
        prop::sample::select(vec![
            "\"hello\"".to_string(),
            "\"\"".to_string(),
            "\"test \\n value\"".to_string(),
            "\"interp: \\(x)\"".to_string(),
        ]),
        Just("true".to_string()),
        Just("false".to_string()),
    ];
    let whitespace = prop::sample::select(vec![" ", "\n", "  ", "\n\n"]);

    let token = prop_oneof![
        keywords.prop_map(|s| s.to_string()),
        ops.prop_map(|s| s.to_string()),
        delimiters.prop_map(|s| s.to_string()),
        idents.prop_map(|s| s.to_string()),
        literals,
    ];

    prop::collection::vec((token, whitespace.prop_map(|s| s.to_string())), 1..80)
        .prop_map(|pairs| {
            pairs.into_iter()
                .flat_map(|(tok, ws)| [tok, ws])
                .collect::<String>()
        })
}

/// Syntactically plausible Almide snippets (higher chance of reaching checker)
fn structured_source() -> impl Strategy<Value = String> {
    let simple_expr = prop::sample::select(vec![
        "42", "3.14", "\"hello\"", "true", "false", "x", "foo", "none",
    ]);

    let let_stmt = simple_expr.clone().prop_map(|e| format!("let x = {e}"));
    let fn_decl = simple_expr.clone().prop_map(|e| format!("fn foo(x: Int) -> Int {{\n  {e}\n}}"));
    let type_decl = Just("type Color = Red | Green | Blue".to_string());
    let match_expr = Just("match x {\n  Some(v) => v\n  None => 0\n}".to_string());
    let for_loop = Just("for x in [1, 2, 3] {\n  println(x)\n}".to_string());
    let test_block = simple_expr.prop_map(|e| format!("test \"fuzz\" {{\n  assert_eq({e}, {e})\n}}"));

    prop_oneof![let_stmt, fn_decl, type_decl, match_expr, for_loop, test_block]
}

// ── Fuzz targets ────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10000))]

    // Lexer must never panic on any input
    #[test]
    fn fuzz_lexer_arbitrary(src in arbitrary_source()) {
        let _ = Lexer::tokenize(&src);
    }

    #[test]
    fn fuzz_lexer_structured(src in almide_like_source()) {
        let _ = Lexer::tokenize(&src);
    }

    // Parser must never panic on any token stream
    #[test]
    fn fuzz_parser_arbitrary(src in arbitrary_source()) {
        let tokens = Lexer::tokenize(&src);
        let mut parser = Parser::new(tokens);
        let _ = parser.parse();
    }

    #[test]
    fn fuzz_parser_structured(src in almide_like_source()) {
        let tokens = Lexer::tokenize(&src);
        let mut parser = Parser::new(tokens);
        let _ = parser.parse();
    }

    // Checker must never panic on any parseable program
    #[test]
    fn fuzz_checker_structured(src in almide_like_source()) {
        let tokens = Lexer::tokenize(&src);
        let mut parser = Parser::new(tokens);
        if let Ok(mut program) = parser.parse() {
            let mut checker = Checker::new();
            let _ = checker.check_program(&mut program);
        }
    }

    #[test]
    fn fuzz_checker_plausible(src in structured_source()) {
        let tokens = Lexer::tokenize(&src);
        let mut parser = Parser::new(tokens);
        if let Ok(mut program) = parser.parse() {
            let mut checker = Checker::new();
            let _ = checker.check_program(&mut program);
        }
    }
}
