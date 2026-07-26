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
