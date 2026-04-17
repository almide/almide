//! Attribute parser tests.
//!
//! Covers the generic `@name(args)` syntax that will host stdlib
//! unification attributes (`@inline_rust`, `@pure`, `@schedule`,
//! `@rewrite`, ...) plus backward compatibility with the legacy
//! typed `@extern` / `@export` forms.
//!
//! Each test parses a source fragment, pulls the first top-level `fn`
//! decl, and asserts the three attribute buckets (extern, export,
//! generic) have the expected shape. Format round-trip is verified
//! separately in `almide-tools` so we don't pull that crate into
//! almide-syntax's dep graph.

#[cfg(test)]
mod tests {
    use crate::ast::{AttrValue, Decl, Program};
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn parse_program(src: &str) -> Program {
        let tokens = Lexer::tokenize(src);
        let mut parser = Parser::new(tokens);
        parser.parse().expect("parse succeeds")
    }

    fn first_fn(program: &Program) -> &Decl {
        program
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Fn { .. }))
            .expect("at least one fn decl")
    }

    // ── Legacy @extern / @export backward compat ─────────────────

    #[test]
    fn extern_attr_still_goes_to_typed_struct() {
        let prog = parse_program(
            r#"@extern(rust, "libfoo", "bar")
fn ext(x: Int) -> Int"#,
        );
        let Decl::Fn { extern_attrs, export_attrs, attrs, .. } = first_fn(&prog) else {
            panic!("expected fn")
        };
        assert_eq!(extern_attrs.len(), 1, "extern routed to typed struct");
        assert_eq!(extern_attrs[0].target.as_str(), "rust");
        assert_eq!(extern_attrs[0].module.as_str(), "libfoo");
        assert_eq!(extern_attrs[0].function.as_str(), "bar");
        assert!(export_attrs.is_empty());
        assert!(
            attrs.is_empty(),
            "@extern must not also leak into generic attrs",
        );
    }

    #[test]
    fn export_attr_still_goes_to_typed_struct() {
        let prog = parse_program(
            r#"@export(c, "bridge_add")
fn add(a: Int, b: Int) -> Int"#,
        );
        let Decl::Fn { extern_attrs, export_attrs, attrs, .. } = first_fn(&prog) else {
            panic!("expected fn")
        };
        assert!(extern_attrs.is_empty());
        assert_eq!(export_attrs.len(), 1);
        assert_eq!(export_attrs[0].target.as_str(), "c");
        assert_eq!(export_attrs[0].symbol.as_str(), "bridge_add");
        assert!(attrs.is_empty());
    }

    // ── Generic attribute shapes ─────────────────────────────────

    #[test]
    fn pure_attribute_with_no_parens() {
        let prog = parse_program("@pure\nfn f(x: Int) -> Int");
        let Decl::Fn { attrs, .. } = first_fn(&prog) else {
            panic!("expected fn")
        };
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].name.as_str(), "pure");
        assert!(
            attrs[0].args.is_empty(),
            "@pure with no parens has no args",
        );
    }

    #[test]
    fn inline_rust_template_string_arg() {
        let prog = parse_program(
            r#"@inline_rust("almide_rt_int_to_string({n})")
fn to_string(n: Int) -> String"#,
        );
        let Decl::Fn { attrs, .. } = first_fn(&prog) else {
            panic!("expected fn")
        };
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].name.as_str(), "inline_rust");
        assert_eq!(attrs[0].args.len(), 1);
        assert!(attrs[0].args[0].name.is_none(), "positional arg");
        match &attrs[0].args[0].value {
            AttrValue::String { value } => {
                assert_eq!(value, "almide_rt_int_to_string({n})");
            }
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn schedule_with_named_args() {
        let prog = parse_program(
            r#"@schedule(device=gpu, tile=32, unroll=true)
fn gemm(a: Matrix, b: Matrix) -> Matrix"#,
        );
        let Decl::Fn { attrs, .. } = first_fn(&prog) else {
            panic!("expected fn")
        };
        let attr = &attrs[0];
        assert_eq!(attr.name.as_str(), "schedule");
        assert_eq!(attr.args.len(), 3);

        assert_eq!(attr.args[0].name.as_ref().unwrap().as_str(), "device");
        assert!(matches!(&attr.args[0].value, AttrValue::Ident { name } if name.as_str() == "gpu"));

        assert_eq!(attr.args[1].name.as_ref().unwrap().as_str(), "tile");
        assert!(matches!(&attr.args[1].value, AttrValue::Int { value: 32 }));

        assert_eq!(attr.args[2].name.as_ref().unwrap().as_str(), "unroll");
        assert!(matches!(&attr.args[2].value, AttrValue::Bool { value: true }));
    }

    #[test]
    fn multiple_attributes_stack_and_preserve_order() {
        let prog = parse_program(
            r#"@pure
@inline_rust("a")
@schedule(tile=8)
fn multi(x: Int) -> Int"#,
        );
        let Decl::Fn { attrs, .. } = first_fn(&prog) else {
            panic!("expected fn")
        };
        assert_eq!(attrs.len(), 3);
        assert_eq!(attrs[0].name.as_str(), "pure");
        assert_eq!(attrs[1].name.as_str(), "inline_rust");
        assert_eq!(attrs[2].name.as_str(), "schedule");
    }

    #[test]
    fn extern_mixed_with_generic_attrs() {
        // Typed legacy + generic coexist on the same fn.
        let prog = parse_program(
            r#"@extern(rust, "m", "f")
@pure
fn mixed(x: Int) -> Int"#,
        );
        let Decl::Fn { extern_attrs, attrs, .. } = first_fn(&prog) else {
            panic!("expected fn")
        };
        assert_eq!(extern_attrs.len(), 1);
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].name.as_str(), "pure");
    }

    #[test]
    fn negative_int_value_parses() {
        let prog = parse_program(
            r#"@meta(threshold=-1)
fn f(x: Int) -> Int"#,
        );
        let Decl::Fn { attrs, .. } = first_fn(&prog) else {
            panic!("expected fn")
        };
        match &attrs[0].args[0].value {
            AttrValue::Int { value } => assert_eq!(*value, -1),
            other => panic!("expected int, got {other:?}"),
        }
    }

    #[test]
    fn hex_int_value_parses() {
        let prog = parse_program(
            r#"@align(mask=0xff)
fn f(x: Int) -> Int"#,
        );
        let Decl::Fn { attrs, .. } = first_fn(&prog) else {
            panic!("expected fn")
        };
        match &attrs[0].args[0].value {
            AttrValue::Int { value } => assert_eq!(*value, 0xff),
            other => panic!("expected int, got {other:?}"),
        }
    }

    #[test]
    fn empty_parens_attribute() {
        let prog = parse_program(
            r#"@inline()
fn f(x: Int) -> Int"#,
        );
        let Decl::Fn { attrs, .. } = first_fn(&prog) else {
            panic!("expected fn")
        };
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].name.as_str(), "inline");
        assert!(attrs[0].args.is_empty());
    }

    // ── Error diagnostics ────────────────────────────────────────

    #[test]
    fn malformed_extern_arity_is_diagnostic_not_crash() {
        // parse() runs with recovery, so malformed @extern adds a
        // diagnostic and moves on rather than aborting the whole
        // parse. The error must surface in `parser.errors`, not as a
        // top-level Err from `parse()`.
        let src = r#"@extern(rust, "only_one_string")
fn broken(x: Int) -> Int"#;
        let tokens = Lexer::tokenize(src);
        let mut parser = Parser::new(tokens);
        let _ = parser.parse();
        let joined: String = parser
            .errors
            .iter()
            .map(|d| d.display())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            joined.contains("@extern expects 3 positional arguments"),
            "error should mention extern arity, got: {joined}",
        );
    }

    #[test]
    fn unquoted_string_is_rejected_as_ident() {
        // `@inline_rust(foo)` — foo is a bare ident, not a string;
        // the parser accepts it as Ident. This is the documented
        // behavior (string vs ident distinguished by quoting).
        let prog = parse_program(
            r#"@inline_rust(foo)
fn f(x: Int) -> Int"#,
        );
        let Decl::Fn { attrs, .. } = first_fn(&prog) else {
            panic!("expected fn")
        };
        match &attrs[0].args[0].value {
            AttrValue::Ident { name } => assert_eq!(name.as_str(), "foo"),
            other => panic!("expected ident, got {other:?}"),
        }
    }

    #[test]
    fn lonely_at_is_diagnostic() {
        let src = "@";
        let tokens = Lexer::tokenize(src);
        let mut parser = Parser::new(tokens);
        let _ = parser.parse();
        let joined: String = parser
            .errors
            .iter()
            .map(|d| d.display())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            joined.contains("Expected attribute name")
                || joined.contains("Expected")
                || !joined.is_empty(),
            "expected at least one diagnostic for lonely '@', got: {joined}",
        );
    }
}
