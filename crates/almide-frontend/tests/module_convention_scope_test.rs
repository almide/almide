//! Temporary module aliases must not become new convention definitions.
use almide_frontend::{canonicalize, check::Checker};

fn parse(source: &str) -> almide_lang::ast::Program {
    let mut parser = almide_lang::parser::Parser::new(almide_lang::lexer::Lexer::tokenize(source));
    parser.parse().expect("parse fixture")
}

#[test]
fn checking_then_loading_another_module_keeps_canonical_convention_identity() {
    let root = parse("fn main() -> Unit = ()");
    let mut alpha = parse("type Alpha = { n: Int }\nfn Alpha.repr(self) -> String = \"alpha\"");
    let beta = parse("fn identity(n: Int) -> Int = n");
    let canonical = canonicalize::canonicalize_program(&root, [("alpha", &alpha, false)].into_iter());
    assert!(canonical.diagnostics.is_empty(), "{:?}", canonical.diagnostics);
    let mut checker = Checker::from_env(canonical.env);
    let declared = checker.env.explicit_convention_fns.clone();
    for _ in 0..2 {
        checker.infer_module(&mut alpha, "alpha");
        assert!(checker.diagnostics.is_empty(), "{:?}", checker.diagnostics);
        assert_eq!(checker.env.explicit_convention_fns, declared,
            "temporary bare convention aliases must not survive module inference");
    }
    canonicalize::register_module(&mut checker.env, &mut checker.diagnostics, "beta", &beta, false);
    assert!(checker.diagnostics.is_empty(), "{:?}", checker.diagnostics);
}
