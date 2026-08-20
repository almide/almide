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

    /// #1404: an inline `/* */` binds to the node it is adjacent to, on the
    /// side it was written, and survives a format round-trip there.
    ///
    /// Before this, both spellings made fmt REFUSE (the conservation verifier
    /// counted a comment the printer had no slot for), so the file was left
    /// untouched with a "formatter bug" message.
    #[test]
    fn an_inline_block_comment_stays_on_the_node_it_annotates() {
        // LEADING: written before `3`, so it travels with `3`.
        let lead = roundtrip("fn f(a: Int, b: Int) -> Int = a + b\nfn main() -> Unit = {\n  let z = f(/* why */ 3, 4)\n}\n");
        assert!(lead.contains("f(/* why */ 3, 4)"), "leading comment misplaced:\n{lead}");

        // TRAILING: written after `1`, so it must NOT cross the comma onto `2`.
        // Taking the ruling's "attach to the following node" literally would
        // print `f(1, /* x */ 2)` — annotating a value its author never wrote
        // it against.
        let trail = roundtrip("fn f(a: Int, b: Int) -> Int = a + b\nfn main() -> Unit = {\n  let x = f(1 /* x */, 2)\n}\n");
        assert!(trail.contains("f(1 /* x */, 2)"), "trailing comment crossed the separator:\n{trail}");
    }

    /// #1404 is idempotent: re-reading the formatted output re-attaches the
    /// comment to the SAME node, so a second pass is a no-op. `almide fmt` is
    /// idempotent-by-contract, and a comment that drifts one node per run
    /// would satisfy the conservation count while corrupting the source.
    #[test]
    fn an_attached_comment_does_not_drift_on_a_second_pass() {
        let src = "fn f(a: Int, b: Int) -> Int = a + b\nfn main() -> Unit = {\n  let x = f(1 /* x */, 2)\n  let z = f(/* why */ 3, 4)\n}\n";
        let once = roundtrip(&src);
        let twice = roundtrip(&once);
        assert_eq!(once, twice, "formatting is not idempotent for attached comments");
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

/// #1323, the fmt(valid) → invalid family: a `module` header parses into
/// `decls[0]` yet the grammar demands it BEFORE every import, and it owns
/// comment_map slot 0. Emitting imports first therefore sank `module` below
/// them (unparseable) and handed the file header to the first import.
#[cfg(test)]
mod module_header_tests {
    use super::*;
    use almide_lang::lexer::Lexer;
    use almide_lang::parser::Parser;

    /// Format `src`, then assert the OUTPUT still parses cleanly — the
    /// property #1323 broke. Returns the formatted text.
    fn format_and_reparse(src: &str) -> String {
        let mut parser = Parser::new(Lexer::tokenize(src));
        let program = parser.parse().expect("source parses");
        assert!(parser.errors.is_empty(), "source has parse errors: {:?}",
            parser.errors.iter().map(|d| d.display()).collect::<Vec<_>>());
        let out = format_program(&program);
        let mut re = Parser::new(Lexer::tokenize(&out));
        re.parse().unwrap_or_else(|e| panic!("formatted output no longer parses: {e}\n---\n{out}"));
        assert!(re.errors.is_empty(), "formatted output has parse errors: {:?}\n---\n{out}",
            re.errors.iter().map(|d| d.display()).collect::<Vec<_>>());
        out
    }

    /// Both directions in one assertion: the output parses, and
    /// format(format(x)) == format(x).
    fn assert_stable(src: &str) -> String {
        let once = format_and_reparse(src);
        let twice = format_and_reparse(&once);
        assert_eq!(once, twice, "format must be idempotent; got:\n{once}");
        once
    }

    #[test]
    fn module_header_stays_above_imports() {
        let out = assert_stable("module app\nimport json\nfn f() -> Int = 1\n");
        let m = out.find("module app").expect("module kept");
        let i = out.find("import json").expect("import kept");
        assert!(m < i, "module must precede every import; got:\n{out}");
    }

    #[test]
    fn leading_comment_block_stays_above_module_not_above_imports() {
        let out = assert_stable(
            "// ALS-D1: module declaration\n// second line of the header block\nmodule app\n\nimport json\n\nfn f() -> Int = 1\n",
        );
        assert!(
            out.starts_with("// ALS-D1: module declaration\n// second line of the header block\nmodule app\n"),
            "file header must stay attached to the module it documents; got:\n{out}",
        );
    }

    /// The comment slots are POSITIONAL (module?, imports…, decls…): consuming
    /// slot 0 for the module is what keeps every later label on its own owner.
    #[test]
    fn per_import_comments_stay_on_their_own_import() {
        let out = assert_stable(
            "// header\nmodule app\n// why fs\nimport fs\n// why json\nimport json\n// the fn\nfn f() -> Int = 1\n",
        );
        assert!(out.contains("// why fs\nimport fs\n"), "fs label drifted:\n{out}");
        assert!(out.contains("// why json\nimport json\n"), "json label drifted:\n{out}");
        assert!(out.contains("// the fn\nfn f()"), "fn label drifted:\n{out}");
    }

    /// The import-less and module-less shapes must not regress: neither may
    /// grow a leading blank line, and both stay idempotent.
    #[test]
    fn module_without_imports_and_imports_without_module() {
        let no_imports = assert_stable("// header\nmodule app\n\nfn f() -> Int = 1\n");
        assert!(no_imports.starts_with("// header\nmodule app\n\nfn f()"), "got:\n{no_imports}");
        let no_module = assert_stable("// why fs\nimport fs\n\nfn f() -> Int = 1\n");
        assert!(no_module.starts_with("// why fs\nimport fs\n"), "got:\n{no_module}");
    }

    /// A trailing comment after the last decl has no owner — it belongs at the
    /// end, and must not be pulled forward by the header's slot shift.
    #[test]
    fn trailing_comment_survives_with_a_module_header() {
        let out = assert_stable("// header\nmodule app\nimport fs\nfn f() -> Int = 1\n// trailing\n");
        assert!(out.trim_end().ends_with("// trailing"), "trailing comment moved:\n{out}");
    }
}

#[cfg(test)]
mod marker_collapse {
    use almide_lang::lexer::Lexer;
    use almide_lang::parser::Parser;
    use crate::fmt::format_program;

    fn fmt_src(src: &str) -> String {
        let tokens = Lexer::tokenize(src);
        let mut parser = Parser::new(tokens);
        let program = parser.parse().expect("parse succeeds");
        assert!(parser.errors.is_empty(), "parse errors: {:?}",
            parser.errors.iter().map(|d| d.display()).collect::<Vec<_>>());
        format_program(&program)
    }

    /// ADR-0012 D3 as amended (#1194): the ONLY return-position normalization
    /// is the marker-internal collapse `T!String` → `T!`. `Result[T, E]`
    /// spellings stay verbatim — the marker and the explicit spelling are not
    /// behaviorally equivalent (ADR-0006 callback routing, slot acceptance,
    /// v1 wall parity; see the ADR's 2026-08-20 amendment), so a formatter
    /// respelling across that line would change program behavior.
    #[test]
    fn marker_string_collapses_and_nothing_else_moves() {
        let out = fmt_src(concat!(
            "fn a(x: Int) -> Int!String = ok(x)\n\n",
            "fn b(x: Int) -> Int!Err9 = err(Err9.X(\"no\"))\n\n",
            "fn c(x: Int) -> Result[Int, String] = ok(x)\n\n",
            "fn d(op: (Int) -> Int!String, x: Int) -> Int!String = op(x)\n",
        ));
        assert!(out.contains("fn a(x: Int) -> Int! ="), "collapse missing:\n{out}");
        assert!(out.contains("fn b(x: Int) -> Int!Err9 ="), "typed E must survive:\n{out}");
        assert!(
            out.contains("fn c(x: Int) -> Result[Int, String] ="),
            "explicit Result must stay verbatim (the amendment's whole point):\n{out}"
        );
        assert!(
            out.contains("op: (Int) -> Int!") && out.contains(") -> Int! ="),
            "slot + decl collapse:\n{out}"
        );
    }

    #[test]
    fn marker_collapse_is_idempotent() {
        let once = fmt_src("fn a(x: Int) -> Int!String = ok(x)\n");
        let twice = fmt_src(&once);
        assert_eq!(once, twice);
    }
}
