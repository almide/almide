//! The declaration index (`almide_types::decl_index`, #4458) is a text scan;
//! the front end trusts it in place of parsing an unreached bundled module.
//! This gate parses every stdlib source both ways and refuses any difference:
//! the function names must be the parser's top-level `fn` declarations, in
//! order, and the types view must parse to exactly the type declarations of
//! the whole source (spans and expression ids included) and nothing else.

use almide_lang::ast::Decl;
use almide_lang::decl_index::{fn_names, types_source, DECL_INDEX};

fn parse(src: &str) -> almide_lang::ast::Program {
    let mut p = almide_lang::parser::Parser::new(almide_lang::lexer::Lexer::tokenize(src));
    p.parse().unwrap_or_else(|e| panic!("parse: {e}"))
}

#[test]
fn the_index_reads_every_stdlib_source_as_the_parser_does() {
    let mut failures = Vec::new();
    for (stem, ..) in DECL_INDEX {
        let src = almide_lang::embedded::source_of(stem).expect("an indexed stem has a source");
        let whole = parse(src);
        let fns: Vec<String> = whole
            .decls
            .iter()
            .filter_map(|d| match d {
                Decl::Fn { name, .. } => Some(name.to_string()),
                _ => None,
            })
            .collect();
        let indexed: Vec<String> = fn_names(stem).unwrap_or_default().iter().map(|s| s.to_string()).collect();
        if fns != indexed {
            failures.push(format!("{stem}: fns {indexed:?} != parsed {fns:?}"));
        }
        let types: Vec<String> =
            whole.decls.iter().filter(|d| matches!(d, Decl::Type { .. })).map(|d| format!("{d:?}")).collect();
        match types_source(stem) {
            None if types.is_empty() => {}
            None => failures.push(format!("{stem}: declares {} type(s) the index misses", types.len())),
            Some(view) => {
                if view.split('\n').count() != src.split('\n').count() {
                    failures.push(format!("{stem}: the types view changes the line count"));
                }
                let viewed = parse(view);
                let got: Vec<String> =
                    viewed.decls.iter().filter(|d| matches!(d, Decl::Type { .. })).map(|d| format!("{d:?}")).collect();
                let others = viewed.decls.iter().filter(|d| !matches!(d, Decl::Type { .. })).count();
                if got != types || (others != 0 && !std::ptr::eq(view, src)) {
                    failures.push(format!("{stem}: the types view parses to {got:?} (+{others} other decls), the source to {types:?}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "decl index drift:\n{}", failures.join("\n"));
}
