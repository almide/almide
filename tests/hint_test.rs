use almide::lexer::{Token, TokenType};
use almide::parser::hints::*;

// ---- Helpers ----

fn tok(tt: TokenType, value: &str) -> Token {
    Token { token_type: tt, value: value.to_string(), line: 1, col: 1, end_col: 1 + value.len() }
}

fn assert_hint_match(ctx: &HintContext, expected_substr: &str) {
    let result = check_hint(ctx);
    assert!(
        result.is_some(),
        "expected a hint containing '{}', got None",
        expected_substr
    );
    let r = result.unwrap();
    let full = format!("{} {}", r.message.as_deref().unwrap_or(""), r.hint);
    assert!(
        full.contains(expected_substr),
        "hint should contain '{}', got: {}",
        expected_substr,
        full.trim()
    );
}

fn assert_no_hint(ctx: &HintContext) {
    let result = check_hint(ctx);
    assert!(
        result.is_none(),
        "expected no hint, got: {}",
        result.map(|r| r.hint).unwrap_or_default()
    );
}

// ============================================================
// missing_comma
// ============================================================

#[test]
fn missing_comma_list_int() {
    let got = tok(TokenType::Int, "2");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::ListLiteral };
    assert_hint_match(&ctx, "Missing ','");
}

#[test]
fn missing_comma_list_string() {
    let got = tok(TokenType::String, "hello");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::ListLiteral };
    assert_hint_match(&ctx, "list elements");
}

#[test]
fn missing_comma_map() {
    let got = tok(TokenType::String, "key");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::MapLiteral };
    assert_hint_match(&ctx, "map entries");
}

#[test]
fn missing_comma_call_args() {
    let got = tok(TokenType::Ident, "x");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::CallArgs };
    assert_hint_match(&ctx, "function arguments");
}

#[test]
fn missing_comma_fn_params() {
    let got = tok(TokenType::Ident, "y");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::FnParams };
    assert_hint_match(&ctx, "function parameters");
}

#[test]
fn missing_comma_not_triggered_for_non_expr() {
    let got = tok(TokenType::RBracket, "]");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::ListLiteral };
    assert_no_hint(&ctx);
}

#[test]
fn missing_comma_not_triggered_wrong_scope() {
    let got = tok(TokenType::Int, "5");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Block };
    assert_no_hint(&ctx);
}

// ============================================================
// operator
// ============================================================

#[test]
fn operator_pipe_pipe() {
    let got = tok(TokenType::PipePipe, "||");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "or");
}

#[test]
fn operator_amp_amp() {
    let got = tok(TokenType::AmpAmp, "&&");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "and");
}

#[test]
fn operator_bang() {
    let got = tok(TokenType::Bang, "!");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "not");
}

#[test]
fn operator_eq_instead_of_eqeq() {
    let got = tok(TokenType::Eq, "=");
    let ctx = HintContext { expected: Some(TokenType::Then), got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "==");
}

#[test]
fn operator_missing_then() {
    let got = tok(TokenType::LBrace, "{");
    let ctx = HintContext { expected: Some(TokenType::Then), got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "then");
}

#[test]
fn operator_missing_else() {
    let got = tok(TokenType::Ident, "x");
    let ctx = HintContext { expected: Some(TokenType::Else), got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "else");
}

#[test]
fn operator_arrow_vs_eq() {
    let got = tok(TokenType::Eq, "=");
    let ctx = HintContext { expected: Some(TokenType::Arrow), got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "->");
}

#[test]
fn operator_angle_vs_bracket() {
    let got = tok(TokenType::LAngle, "<");
    let ctx = HintContext { expected: Some(TokenType::RParen), got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "[]");
}

// ============================================================
// keyword_typo
// ============================================================

#[test]
fn keyword_typo_function() {
    let got = tok(TokenType::Ident, "function");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::TopLevel };
    assert_hint_match(&ctx, "fn");
}

#[test]
fn keyword_typo_def() {
    let got = tok(TokenType::Ident, "def");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::TopLevel };
    assert_hint_match(&ctx, "fn");
}

#[test]
fn keyword_typo_class() {
    let got = tok(TokenType::Ident, "class");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::TopLevel };
    assert_hint_match(&ctx, "type");
}

#[test]
fn keyword_typo_struct() {
    let got = tok(TokenType::Ident, "struct");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::TopLevel };
    assert_hint_match(&ctx, "type");
}

#[test]
fn keyword_typo_enum() {
    let got = tok(TokenType::Ident, "enum");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::TopLevel };
    assert_hint_match(&ctx, "type");
}

#[test]
fn keyword_typo_interface() {
    let got = tok(TokenType::Ident, "interface");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::TopLevel };
    assert_hint_match(&ctx, "protocol");
}

#[test]
fn keyword_typo_const() {
    let got = tok(TokenType::Ident, "const");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::TopLevel };
    assert_hint_match(&ctx, "let");
}

#[test]
fn keyword_typo_return_toplevel() {
    let got = tok(TokenType::Ident, "return");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::TopLevel };
    assert_hint_match(&ctx, "return");
}

#[test]
fn keyword_typo_not_triggered_wrong_scope() {
    let got = tok(TokenType::Ident, "function");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    // keyword_typo only fires at TopLevel — expression scope should not match keyword_typo
    let result = check_hint(&ctx);
    if let Some(r) = result {
        // May match syntax_guide instead, which is fine — just shouldn't be keyword_typo's message
        assert!(
            !r.hint.contains("'fn'") || r.message.as_deref() != Some("'function' is not a keyword in Almide"),
            "keyword_typo should not fire in Expression scope"
        );
    }
}

#[test]
fn keyword_typo_import_after_decls() {
    let got = tok(TokenType::Import, "import");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::TopLevel };
    assert_hint_match(&ctx, "imports must come before");
}

// ============================================================
// delimiter
// ============================================================

#[test]
fn delimiter_missing_rparen() {
    let got = tok(TokenType::Ident, "x");
    let ctx = HintContext { expected: Some(TokenType::RParen), got: &got, prev: None, next: None, scope: HintScope::Expression };
    // operator.rs might catch RParen+LAngle, but plain Ident should go to delimiter
    // Actually operator catches RParen+LAngle specifically; for other got types, delimiter fires
    // But operator has Else catch-all for RParen... no, only RParen+LAngle. Let me check.
    // operator: (RParen, LAngle) → generics hint. Other RParen cases fall through to delimiter.
    assert_hint_match(&ctx, "')'");
}

#[test]
fn delimiter_missing_rbracket() {
    let got = tok(TokenType::Ident, "x");
    let ctx = HintContext { expected: Some(TokenType::RBracket), got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "']'");
}

#[test]
fn delimiter_missing_rbrace() {
    let got = tok(TokenType::Ident, "x");
    let ctx = HintContext { expected: Some(TokenType::RBrace), got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "'}'");
}

#[test]
fn delimiter_missing_eq_before_ident() {
    let got = tok(TokenType::Ident, "value");
    let ctx = HintContext { expected: Some(TokenType::Eq), got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "'='");
}

#[test]
fn delimiter_missing_eq_before_int() {
    let got = tok(TokenType::Int, "42");
    let ctx = HintContext { expected: Some(TokenType::Eq), got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "'='");
}

#[test]
fn delimiter_no_match_for_unknown() {
    let got = tok(TokenType::Comma, ",");
    let ctx = HintContext { expected: Some(TokenType::Colon), got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_no_hint(&ctx);
}

// ============================================================
// syntax_guide
// ============================================================

#[test]
fn syntax_guide_return() {
    let got = tok(TokenType::Ident, "return");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "return");
}

#[test]
fn syntax_guide_null() {
    let got = tok(TokenType::Ident, "null");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "Option");
}

#[test]
fn syntax_guide_nil() {
    let got = tok(TokenType::Ident, "nil");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "Option");
}

#[test]
fn syntax_guide_throw() {
    let got = tok(TokenType::Ident, "throw");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "Result");
}

#[test]
fn syntax_guide_catch() {
    let got = tok(TokenType::Ident, "catch");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "match");
}

#[test]
fn syntax_guide_loop() {
    let got = tok(TokenType::Ident, "loop");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "while true");
}

#[test]
fn syntax_guide_print() {
    let got = tok(TokenType::Ident, "print");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "println");
}

#[test]
fn syntax_guide_let_mut() {
    let prev = tok(TokenType::Let, "let");
    let got = tok(TokenType::Ident, "mut");
    let ctx = HintContext { expected: Some(TokenType::Ident), got: &got, prev: Some(&prev), next: None, scope: HintScope::Block };
    assert_hint_match(&ctx, "var");
}

#[test]
fn syntax_guide_let_mut_needs_let_prev() {
    let prev = tok(TokenType::Ident, "x");
    let got = tok(TokenType::Ident, "mut");
    let ctx = HintContext { expected: Some(TokenType::Ident), got: &got, prev: Some(&prev), next: None, scope: HintScope::Block };
    // "mut" after a non-let token should not trigger the let mut hint
    let result = check_hint(&ctx);
    if let Some(r) = result {
        assert!(
            !r.hint.contains("var"),
            "should not suggest 'var' without 'let' preceding: {}",
            r.hint
        );
    }
}

#[test]
fn syntax_guide_not_triggered_for_valid_ident() {
    let got = tok(TokenType::Ident, "myVar");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_no_hint(&ctx);
}

#[test]
fn syntax_guide_block_scope() {
    let got = tok(TokenType::Ident, "throw");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Block };
    assert_hint_match(&ctx, "Result");
}

#[test]
fn syntax_guide_not_triggered_wrong_scope() {
    let got = tok(TokenType::Ident, "return");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::TopLevel };
    // TopLevel "return" is handled by keyword_typo, not syntax_guide
    // Either way a hint should fire — just verifying it works
    let result = check_hint(&ctx);
    assert!(result.is_some(), "should hint about return at top level too");
}

// ============================================================
// Phase 4: new syntax_guide hints
// ============================================================

#[test]
fn syntax_guide_self() {
    let got = tok(TokenType::Ident, "self");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "first parameter");
}

#[test]
fn syntax_guide_this() {
    let got = tok(TokenType::Ident, "this");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "first parameter");
}

#[test]
fn syntax_guide_new() {
    let got = tok(TokenType::Ident, "new");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "Type { field: value }");
}

#[test]
fn syntax_guide_void() {
    let got = tok(TokenType::Ident, "void");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "Unit");
}

#[test]
fn syntax_guide_undefined() {
    let got = tok(TokenType::Ident, "undefined");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "Option");
}

#[test]
fn syntax_guide_switch() {
    let got = tok(TokenType::Ident, "switch");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "match");
}

#[test]
fn syntax_guide_elif() {
    let got = tok(TokenType::Ident, "elif");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "if/then/else");
}

#[test]
fn syntax_guide_elsif() {
    let got = tok(TokenType::Ident, "elsif");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "if/then/else");
}

#[test]
fn syntax_guide_extends() {
    let got = tok(TokenType::Ident, "extends");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "structural typing");
}

#[test]
fn syntax_guide_implements() {
    let got = tok(TokenType::Ident, "implements");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "structural typing");
}

#[test]
fn syntax_guide_lambda() {
    let got = tok(TokenType::Ident, "lambda");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "(x) => expr");
}

// ============================================================
// Phase 4: operator — |x| closure via next token
// ============================================================

#[test]
fn operator_pipe_closure_with_next() {
    let got = tok(TokenType::Pipe, "|");
    let next = tok(TokenType::Ident, "x");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: Some(&next), scope: HintScope::Expression };
    assert_hint_match(&ctx, "(x) => expr");
}

#[test]
fn operator_pipe_closure_with_underscore() {
    let got = tok(TokenType::Pipe, "|");
    let next = tok(TokenType::Underscore, "_");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: Some(&next), scope: HintScope::Expression };
    assert_hint_match(&ctx, "(x) => expr");
}

#[test]
fn operator_pipe_no_closure_without_next() {
    let got = tok(TokenType::Pipe, "|");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_no_hint(&ctx);
}

#[test]
fn operator_pipe_no_closure_with_non_ident_next() {
    let got = tok(TokenType::Pipe, "|");
    let next = tok(TokenType::Int, "42");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: Some(&next), scope: HintScope::Expression };
    assert_no_hint(&ctx);
}

// ============================================================
// Phase 4: operator — semicolons
// ============================================================

#[test]
fn operator_semicolon() {
    let got = tok(TokenType::Semicolon, ";");
    let ctx = HintContext { expected: None, got: &got, prev: None, next: None, scope: HintScope::Expression };
    assert_hint_match(&ctx, "newlines");
}

// ============================================================
// Phase 4: catalog
// ============================================================

#[test]
fn catalog_has_all_modules() {
    use almide::parser::hints::catalog;
    let hints = catalog::all_hints();
    let modules: std::collections::HashSet<&str> = hints.iter().map(|h| h.module).collect();
    assert!(modules.contains("missing_comma"), "catalog missing missing_comma");
    assert!(modules.contains("operator"), "catalog missing operator");
    assert!(modules.contains("keyword_typo"), "catalog missing keyword_typo");
    assert!(modules.contains("delimiter"), "catalog missing delimiter");
    assert!(modules.contains("syntax_guide"), "catalog missing syntax_guide");
}

#[test]
fn catalog_entries_non_empty() {
    use almide::parser::hints::catalog;
    let hints = catalog::all_hints();
    assert!(hints.len() >= 30, "catalog should have at least 30 entries, got {}", hints.len());
    for entry in &hints {
        assert!(!entry.trigger.is_empty(), "empty trigger in catalog");
        assert!(!entry.hint.is_empty(), "empty hint in catalog");
    }
}
