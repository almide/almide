// ── Tests ────────────────────────────────────────────────────────────

/// #1261 + #1263 — `fmt` reprints a literal's SOURCE SPELLING.
///
/// The formatter used to render literals from the parsed VALUE, which is a
/// lossy direction: `1e10` came back `10000000000.0`, `1_000.25` lost its
/// separator, `"\u{3042}"` came back as a bare `あ`, a heredoc collapsed into
/// one quoted line — and `1e999` came back `inf.0`, which does not parse, so
/// `fmt` turned a valid file into an invalid one.
///
/// The table below is the CONTRACT: every spelling in it must survive a
/// format pass byte-for-byte. Adding a literal FORM to the language means
/// adding its row here — a form that is not listed is a form nobody promised
/// to preserve.
#[cfg(test)]
mod literal_spelling_tests {
    use super::*;
    use almide_lang::lexer::Lexer;
    use almide_lang::parser::Parser;

    /// Every literal spelling `fmt` must print back verbatim.
    const PRESERVED: &[&str] = &[
        // ── Float: exponent, separator, precision, overflow (#1261) ──
        "1e10",
        "1.5e-3",
        "1_000.25",
        "1E7",
        "2.5e+4",
        "0.5",
        "1.0",
        "100.0",
        // `1e999` parses to `inf`; printing the VALUE gave `inf.0`, which
        // re-lexes as an ident + tuple index. fmt(valid) must stay valid.
        "1e999",
        // ── Int: radix prefixes and separators (already preserved) ──
        "0xFF",
        "0b1010",
        "0o755",
        "1_000_000",
        // ── String: escape spelling, quote style, form (#1263) ──
        "\"\\u{3042}\"",
        "\"\\x41\"",
        "\"tab\\there\"",
        // The quote delimiter is the author's choice, not fmt's: it used to
        // switch sides to minimize escapes.
        "'single'",
        "\"he said \\\"hi\\\"\"",
        // A raw string has no escape layer — reprinting it as a cooked string
        // meant doubling every backslash.
        "r\"raw\\nnot\"",
        // Interpolation: the hole and the escapes around it both survive.
        "\"a\\u{3042}${x}b\"",
        "\"\\${literal}\"",
    ];

    fn format_expr_source(literal: &str) -> String {
        let src = format!("fn f(x: Int) -> Unit = {{\n  let v = {literal}\n  println(\"${{v}}\")\n}}\n");
        let tokens = Lexer::tokenize(&src);
        let mut parser = Parser::new(tokens);
        let program = parser.parse().expect("parse succeeds");
        assert!(
            parser.errors.is_empty(),
            "unexpected parse errors for {literal}: {:?}",
            parser.errors.iter().map(|d| d.display()).collect::<Vec<_>>()
        );
        format_program(&program)
    }

    #[test]
    fn every_listed_literal_form_reprints_verbatim() {
        for literal in PRESERVED {
            let out = format_expr_source(literal);
            assert!(
                out.contains(&format!("let v = {literal}\n")),
                "fmt rewrote the literal `{literal}`; output was:\n{out}"
            );
        }
    }

    #[test]
    fn every_listed_literal_form_is_idempotent() {
        for literal in PRESERVED {
            let once = format_expr_source(literal);
            let tokens = Lexer::tokenize(&once);
            let mut parser = Parser::new(tokens);
            let program = parser.parse().expect("reformat parses");
            let twice = format_program(&program);
            assert_eq!(once, twice, "fmt is not idempotent on `{literal}`");
        }
    }

    /// A heredoc stays a heredoc — body, indentation and delimiters intact.
    /// It used to collapse into a single-line double-quoted string.
    #[test]
    fn heredoc_survives_as_heredoc() {
        let src = "fn f() -> String = {\n  let s = \"\"\"\n    line one\n    line two\n    \"\"\"\n  s\n}\n";
        let tokens = Lexer::tokenize(src);
        let mut parser = Parser::new(tokens);
        let program = parser.parse().expect("parse succeeds");
        let out = format_program(&program);
        assert!(out.contains("\"\"\"\n    line one\n    line two\n    \"\"\""), "heredoc collapsed:\n{out}");
        assert_eq!(out, src, "a formatted heredoc file is already at fmt's fixed point");
    }

    /// The whole point of #1261: `fmt` of a VALID file must stay valid. The
    /// post-format verifier is what caught `inf.0`, so assert it clean.
    #[test]
    fn overflowing_float_round_trips_through_the_verifier() {
        let src = "fn f() -> Float = {\n  let v = 1e999\n  v\n}\n";
        let tokens = Lexer::tokenize(src);
        let mut parser = Parser::new(tokens);
        let program = parser.parse().expect("parse succeeds");
        let out = format_program(&program);
        assert!(out.contains("let v = 1e999"), "spelling lost:\n{out}");
        crate::fmt::verify_format(src, &program, &out)
            .expect("formatted output still parses to the same program");
    }

    /// A `${…}` hole is CODE, not a literal run. Preserving the spelling of
    /// the literal must not turn the formatter off inside its holes — that
    /// would leave a blind spot exactly where interpolation-heavy Almide is
    /// written. The escape AROUND the hole still survives.
    #[test]
    fn interpolation_holes_are_reformatted_but_literal_runs_are_not() {
        let out = format_expr_source("\"\\u{3042}${ x   +   1 }b\"");
        assert!(
            out.contains("let v = \"\\u{3042}${x + 1}b\"\n"),
            "hole not reformatted (or escape lost):\n{out}"
        );
    }

    /// The hole scan walks the RAW, the parser walks the DECODED template, so
    /// a nested literal reaches the two in different spellings. `\"?\"` used
    /// to open a nested-string scan that ran past the hole and swallowed the
    /// literal's own closing quote, emitting an unterminated string.
    #[test]
    fn an_escaped_nested_quote_inside_a_hole_keeps_the_literal_closed() {
        let src = "fn f(v: String?) -> String = {\n  let s = \"a/${v ?? \\\"?\\\"}\"\n  s\n}\n";
        let tokens = Lexer::tokenize(src);
        let mut parser = Parser::new(tokens);
        let program = parser.parse().expect("parse succeeds");
        let out = format_program(&program);
        crate::fmt::verify_format(src, &program, &out)
            .expect("formatted output still parses to the same program");
    }

    /// A heredoc's value is its lines minus their COMMON leading indent, so a
    /// hole that re-renders across lines can change the strip amount — i.e.
    /// change the string. Holes there stay verbatim: losing a re-format is
    /// cosmetic, changing a value is a miscompile.
    #[test]
    fn heredoc_holes_are_left_verbatim() {
        let src = "fn f(x: Int) -> String = {\n  let s = \"\"\"\n    a ${ x  +  1 } b\n    \"\"\"\n  s\n}\n";
        let tokens = Lexer::tokenize(src);
        let mut parser = Parser::new(tokens);
        let program = parser.parse().expect("parse succeeds");
        let out = format_program(&program);
        assert!(out.contains("a ${ x  +  1 } b"), "heredoc hole was re-rendered:\n{out}");
        assert_eq!(out, src, "a formatted heredoc file is already at fmt's fixed point");
    }

    /// Dropping the spelling cache falls back to value rendering — the escape
    /// hatch every AST-rewriting tool takes before it re-renders.
    #[test]
    fn strip_literal_raw_falls_back_to_value_rendering() {
        let src = "fn f() -> Float = {\n  let v = 1e10\n  v\n}\n";
        let tokens = Lexer::tokenize(src);
        let mut parser = Parser::new(tokens);
        let mut program = parser.parse().expect("parse succeeds");
        almide_lang::ast::strip_literal_raw(&mut program);
        let out = format_program(&program);
        assert!(out.contains("let v = 10000000000.0"), "expected value rendering:\n{out}");
    }
}

#[cfg(test)]
mod attr_tests {
    use super::*;
    use almide_lang::lexer::Lexer;
    use almide_lang::parser::Parser;

    fn roundtrip(src: &str) -> String {
        let tokens = Lexer::tokenize(src);
        let mut parser = Parser::new(tokens);
        let program = parser.parse().expect("parse succeeds");
        assert!(
            parser.errors.is_empty(),
            "unexpected parse errors: {:?}",
            parser.errors.iter().map(|d| d.display()).collect::<Vec<_>>()
        );
        format_program(&program)
    }

    /// Parse → format → parse round-trip: the second parse must
    /// produce the same attribute structure as the first. This is
    /// the formatter's idempotency contract, stricter than matching
    /// byte strings (which would break on cosmetic diffs like quote
    /// style).
    fn shape_of_first_fn(src: &str) -> String {
        let tokens = Lexer::tokenize(src);
        let mut parser = Parser::new(tokens);
        let program = parser.parse().expect("parse succeeds");
        let fn_decl = program
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Fn { .. }))
            .expect("at least one fn");
        match fn_decl {
            Decl::Fn { extern_attrs, export_attrs, attrs, name, .. } => {
                let mut out = format!("fn={} ext=[", name);
                for (i, a) in extern_attrs.iter().enumerate() {
                    if i > 0 { out.push_str(","); }
                    out.push_str(&format!("{}|{}|{}", a.target, a.module, a.function));
                }
                out.push_str("] exp=[");
                for (i, a) in export_attrs.iter().enumerate() {
                    if i > 0 { out.push_str(","); }
                    out.push_str(&format!("{}|{}", a.target, a.symbol));
                }
                out.push_str("] attrs=[");
                for (i, a) in attrs.iter().enumerate() {
                    if i > 0 { out.push_str(","); }
                    out.push_str(&format_attribute(a));
                }
                out.push(']');
                out
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn format_roundtrip_pure_no_parens() {
        let src = "@pure\nfn f(x: Int) -> Int";
        let formatted = roundtrip(src);
        assert!(
            formatted.contains("@pure"),
            "formatter must keep @pure; got: {formatted}",
        );
        let before = shape_of_first_fn(src);
        let after = shape_of_first_fn(&formatted);
        assert_eq!(before, after, "parse tree must be identical after format");
    }

    #[test]
    fn format_roundtrip_inline_rust_string() {
        let src = "@inline_rust(\"almide_rt_int_to_string({n})\")\nfn to_string(n: Int) -> String";
        let formatted = roundtrip(src);
        let before = shape_of_first_fn(src);
        let after = shape_of_first_fn(&formatted);
        assert_eq!(before, after);
    }

    #[test]
    fn format_roundtrip_schedule_named_args() {
        let src = "@schedule(device=gpu, tile=32, unroll=true)\nfn gemm(x: Int) -> Int";
        let formatted = roundtrip(src);
        let before = shape_of_first_fn(src);
        let after = shape_of_first_fn(&formatted);
        assert_eq!(before, after);
    }

    #[test]
    fn format_roundtrip_extern_still_emits_typed() {
        let src = "@extern(rust, \"libfoo\", \"bar\")\nfn ext(x: Int) -> Int";
        let formatted = roundtrip(src);
        assert!(formatted.contains("@extern(rust, \"libfoo\", \"bar\")"));
        let before = shape_of_first_fn(src);
        let after = shape_of_first_fn(&formatted);
        assert_eq!(before, after);
    }

    #[test]
    fn format_preserves_mixed_extern_and_generic_ordering() {
        // `@extern` prints first, then generic attrs. The parse tree
        // is identical after a round-trip regardless of source order
        // (since extern is routed to its own bucket).
        let src = "@pure\n@extern(rust, \"m\", \"f\")\nfn mixed(x: Int) -> Int";
        let formatted = roundtrip(src);
        let before = shape_of_first_fn(src);
        let after = shape_of_first_fn(&formatted);
        assert_eq!(before, after);
    }

    /// Idempotency: format(format(x)) == format(x). Formatter contract.
    #[test]
    fn format_is_idempotent_on_attributes() {
        let src = "@pure\n@inline_rust(\"x\")\n@schedule(device=gpu, tile=-4)\nfn f(x: Int) -> Int";
        let once = roundtrip(src);
        let twice = roundtrip(&once);
        assert_eq!(once, twice, "format must be idempotent");
    }
}
