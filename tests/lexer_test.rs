use almide::lexer::{Lexer, TokenType};

fn tokens(input: &str) -> Vec<(TokenType, String)> {
    Lexer::tokenize(input)
        .into_iter()
        .filter(|t| t.token_type != TokenType::Newline && t.token_type != TokenType::EOF)
        .map(|t| (t.token_type, t.value))
        .collect()
}

fn token_types(input: &str) -> Vec<TokenType> {
    Lexer::tokenize(input)
        .into_iter()
        .filter(|t| t.token_type != TokenType::Newline && t.token_type != TokenType::EOF)
        .map(|t| t.token_type)
        .collect()
}

// ---- Integer literals ----

#[test]
fn lex_integer() {
    let toks = tokens("42");
    assert_eq!(toks, vec![(TokenType::Int, "42".into())]);
}

// ---- Float literals ----

#[test]
fn lex_float() {
    let toks = tokens("3.14");
    assert_eq!(toks, vec![(TokenType::Float, "3.14".into())]);
}

#[test]
fn lex_scientific_notation() {
    let toks = tokens("1e10");
    assert_eq!(toks, vec![(TokenType::Float, "1e10".into())]);
}

#[test]
fn lex_scientific_with_sign() {
    let toks = tokens("6.674e-11");
    assert_eq!(toks, vec![(TokenType::Float, "6.674e-11".into())]);
}

#[test]
fn lex_scientific_positive_sign() {
    let toks = tokens("3E+8");
    assert_eq!(toks, vec![(TokenType::Float, "3E+8".into())]);
}

// ---- String literals ----

#[test]
fn lex_simple_string() {
    let toks = tokens("\"hello\"");
    assert_eq!(toks, vec![(TokenType::String, "hello".into())]);
}

#[test]
fn lex_string_escape_sequences() {
    let toks = tokens("\"a\\nb\\tc\"");
    assert_eq!(toks, vec![(TokenType::String, "a\nb\tc".into())]);
}

#[test]
fn lex_interpolated_string() {
    let toks = tokens("\"hello ${name}\"");
    assert_eq!(toks.len(), 1);
    assert_eq!(toks[0].0, TokenType::InterpolatedString);
    assert!(toks[0].1.contains("${name}"));
}

#[test]
fn lex_escaped_dollar_in_string() {
    let toks = tokens("\"price is \\$5\"");
    assert_eq!(toks, vec![(TokenType::String, "price is $5".into())]);
}

// ---- Keywords ----

#[test]
fn lex_all_keywords() {
    let keywords = vec![
        ("fn", TokenType::Fn),
        ("let", TokenType::Let),
        ("var", TokenType::Var),
        ("if", TokenType::If),
        ("then", TokenType::Then),
        ("else", TokenType::Else),
        ("match", TokenType::Match),
        ("for", TokenType::For),
        ("in", TokenType::In),
        ("import", TokenType::Import),
        ("module", TokenType::Module),
        ("type", TokenType::Type),
        ("trait", TokenType::Trait),
        ("impl", TokenType::Impl),
        ("true", TokenType::True),
        ("false", TokenType::False),
        ("and", TokenType::And),
        ("or", TokenType::Or),
        ("not", TokenType::Not),
        ("effect", TokenType::Effect),
        ("test", TokenType::Test),
        ("pub", TokenType::Pub),
        ("async", TokenType::Async),
        ("await", TokenType::Await),
        ("do", TokenType::Do),
        ("try", TokenType::Try),
        ("guard", TokenType::Guard),
        ("break", TokenType::Break),
        ("continue", TokenType::Continue),
        ("while", TokenType::While),
        ("todo", TokenType::Todo),
        ("unsafe", TokenType::Unsafe),
        ("ok", TokenType::Ok),
        ("err", TokenType::Err),
        ("some", TokenType::Some),
        ("none", TokenType::None),
        ("deriving", TokenType::Deriving),
        ("strict", TokenType::Strict),
        ("local", TokenType::Local),
        ("mod", TokenType::Mod),
        ("newtype", TokenType::Newtype),
    ];
    for (kw, expected) in keywords {
        let toks = token_types(kw);
        assert_eq!(toks, vec![expected.clone()], "keyword '{}' should produce {:?}", kw, expected);
    }
}

// ---- Identifiers ----

#[test]
fn lex_identifier() {
    let toks = tokens("my_var");
    assert_eq!(toks, vec![(TokenType::Ident, "my_var".into())]);
}

#[test]
fn lex_type_name() {
    let toks = tokens("MyType");
    assert_eq!(toks, vec![(TokenType::TypeName, "MyType".into())]);
}

#[test]
fn lex_predicate_identifier() {
    let toks = tokens("empty?");
    assert_eq!(toks, vec![(TokenType::IdentQ, "empty?".into())]);
}

#[test]
fn lex_underscore_prefixed_ident() {
    let toks = tokens("_unused");
    assert_eq!(toks, vec![(TokenType::Ident, "_unused".into())]);
}

// ---- Symbols ----

#[test]
fn lex_two_char_symbols() {
    assert_eq!(token_types("->"), vec![TokenType::Arrow]);
    assert_eq!(token_types("=>"), vec![TokenType::FatArrow]);
    assert_eq!(token_types("=="), vec![TokenType::EqEq]);
    assert_eq!(token_types("!="), vec![TokenType::BangEq]);
    assert_eq!(token_types("<="), vec![TokenType::LtEq]);
    assert_eq!(token_types(">="), vec![TokenType::GtEq]);
    assert_eq!(token_types("++"), vec![TokenType::PlusPlus]);
    assert_eq!(token_types("|>"), vec![TokenType::PipeArrow]);
    assert_eq!(token_types("&&"), vec![TokenType::AmpAmp]);
    assert_eq!(token_types("||"), vec![TokenType::PipePipe]);
}

#[test]
fn lex_single_char_symbols() {
    assert_eq!(token_types("("), vec![TokenType::LParen]);
    assert_eq!(token_types(")"), vec![TokenType::RParen]);
    assert_eq!(token_types("{"), vec![TokenType::LBrace]);
    assert_eq!(token_types("}"), vec![TokenType::RBrace]);
    assert_eq!(token_types("["), vec![TokenType::LBracket]);
    assert_eq!(token_types("]"), vec![TokenType::RBracket]);
    assert_eq!(token_types(","), vec![TokenType::Comma]);
    assert_eq!(token_types("."), vec![TokenType::Dot]);
    assert_eq!(token_types(":"), vec![TokenType::Colon]);
    assert_eq!(token_types(";"), vec![TokenType::Semicolon]);
    assert_eq!(token_types("="), vec![TokenType::Eq]);
    assert_eq!(token_types("+"), vec![TokenType::Plus]);
    assert_eq!(token_types("-"), vec![TokenType::Minus]);
    assert_eq!(token_types("*"), vec![TokenType::Star]);
    assert_eq!(token_types("/"), vec![TokenType::Slash]);
    assert_eq!(token_types("%"), vec![TokenType::Percent]);
    assert_eq!(token_types("|"), vec![TokenType::Pipe]);
    assert_eq!(token_types("^"), vec![TokenType::Caret]);
    assert_eq!(token_types("!"), vec![TokenType::Bang]);
    assert_eq!(token_types("@"), vec![TokenType::At]);
}

#[test]
fn lex_range_operators() {
    assert_eq!(token_types(".."), vec![TokenType::DotDot]);
    assert_eq!(token_types("..="), vec![TokenType::DotDotEq]);
    assert_eq!(token_types("..."), vec![TokenType::DotDotDot]);
}

// ---- Comments ----

#[test]
fn lex_line_comment() {
    let all = Lexer::tokenize("// this is a comment\nfn");
    let comment = all.iter().find(|t| t.token_type == TokenType::Comment);
    assert!(comment.is_some());
    assert_eq!(comment.unwrap().value, "// this is a comment");
}

#[test]
fn lex_comment_preserves_line_info() {
    let all = Lexer::tokenize("// line 1\nfn");
    let fn_tok = all.iter().find(|t| t.token_type == TokenType::Fn).unwrap();
    assert_eq!(fn_tok.line, 2);
}

// ---- Newline handling ----

#[test]
fn lex_newline_between_statements() {
    let toks = Lexer::tokenize("a\nb");
    let newline_count = toks.iter().filter(|t| t.token_type == TokenType::Newline).count();
    assert_eq!(newline_count, 1);
}

// ---- Line/column tracking ----

#[test]
fn lex_tracks_line_numbers() {
    let all = Lexer::tokenize("fn\nlet\nvar");
    let lines: Vec<usize> = all.iter()
        .filter(|t| t.token_type != TokenType::Newline && t.token_type != TokenType::EOF)
        .map(|t| t.line)
        .collect();
    assert_eq!(lines, vec![1, 2, 3]);
}

#[test]
fn lex_tracks_columns() {
    let all = Lexer::tokenize("fn add");
    let fn_tok = &all[0];
    assert_eq!(fn_tok.col, 1);
    let add_tok = &all[1];
    assert_eq!(add_tok.col, 4);
}

// ---- Empty and edge cases ----

#[test]
fn lex_empty_input() {
    let toks = Lexer::tokenize("");
    assert_eq!(toks.len(), 1); // just EOF
    assert_eq!(toks[0].token_type, TokenType::EOF);
}

#[test]
fn lex_whitespace_only() {
    let toks = Lexer::tokenize("   \t  ");
    assert_eq!(toks.len(), 1); // just EOF
}

#[test]
fn lex_complex_expression() {
    let types = token_types("1 + 2 * 3");
    assert_eq!(types, vec![
        TokenType::Int, TokenType::Plus,
        TokenType::Int, TokenType::Star,
        TokenType::Int,
    ]);
}

#[test]
fn lex_function_declaration() {
    let types = token_types("fn add(a: Int, b: Int) -> Int = a + b");
    assert_eq!(types[0], TokenType::Fn);
    assert_eq!(types[1], TokenType::Ident); // add
    assert_eq!(types[2], TokenType::LParen);
    assert!(types.contains(&TokenType::Arrow));
    assert!(types.contains(&TokenType::Eq));
}

// ---- Nested interpolation ----

#[test]
fn lex_nested_interpolation() {
    let toks = tokens("\"${if true then \"yes\" else \"no\"}\"");
    assert_eq!(toks.len(), 1);
    assert_eq!(toks[0].0, TokenType::InterpolatedString);
}

// ---- Multiple tokens in expression ----

#[test]
fn lex_pipe_operator() {
    let types = token_types("xs |> f");
    assert!(types.contains(&TokenType::PipeArrow));
}

#[test]
fn lex_fat_arrow() {
    let types = token_types("x => y");
    assert!(types.contains(&TokenType::FatArrow));
}

#[test]
fn lex_arrow() {
    let types = token_types("x -> y");
    assert!(types.contains(&TokenType::Arrow));
}

// ---- Negative numbers ----

#[test]
fn lex_negative_start() {
    // Lexer tokenizes `-` and `42` separately
    let types = token_types("-42");
    assert_eq!(types, vec![TokenType::Minus, TokenType::Int]);
}

// ---- Float edge cases ----

#[test]
fn lex_zero_float() {
    let toks = tokens("0.0");
    assert_eq!(toks, vec![(TokenType::Float, "0.0".into())]);
}

// ---- String edge cases ----

#[test]
fn lex_empty_string() {
    let toks = tokens("\"\"");
    assert_eq!(toks, vec![(TokenType::String, "".into())]);
}

#[test]
fn lex_escaped_backslash() {
    let toks = tokens("\"\\\\\"");
    assert_eq!(toks, vec![(TokenType::String, "\\".into())]);
}

#[test]
fn lex_escaped_quote() {
    let toks = tokens("\"he said \\\"hi\\\"\"");
    assert_eq!(toks, vec![(TokenType::String, "he said \"hi\"".into())]);
}

// ---- Carriage return handling ----

#[test]
fn lex_strips_carriage_returns() {
    let toks = tokens("fn\r\nadd");
    assert_eq!(toks.len(), 2);
    assert_eq!(toks[0].0, TokenType::Fn);
    assert_eq!(toks[1].0, TokenType::Ident);
}
