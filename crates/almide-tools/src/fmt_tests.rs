// ── Tests ────────────────────────────────────────────────────────────

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
