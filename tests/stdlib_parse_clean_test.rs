//! Every stdlib `.almd` source must parse with ZERO recovered errors.
//!
//! The parser never crashes — it records diagnostics in `Parser::errors` and
//! skips to the next declaration. For user code that is the right behavior,
//! but for the bundled stdlib it silently DROPS declarations: `args.flag?`
//! and `path.is_absolute?` shipped for weeks as unparseable decls that
//! error-recovery swallowed, so the functions simply did not exist and no
//! test noticed. This gate makes any such rot a hard test failure.

use almide::lexer::Lexer;
use almide::parser::Parser;

fn assert_parses_clean(name: &str, source: &str) {
    let tokens = Lexer::tokenize(source);
    let mut parser = Parser::new(tokens);
    let result = parser.parse();
    assert!(
        result.is_ok(),
        "stdlib source '{}' failed to parse outright: {:?}",
        name,
        result.err()
    );
    assert!(
        parser.errors.is_empty(),
        "stdlib source '{}' parsed only via error recovery — {} declaration(s) \
         were silently dropped:\n{}",
        name,
        parser.errors.len(),
        parser
            .errors
            .iter()
            .map(|d| format!("  - {}", d.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Every module body wired into the compiler via bundled_source().
#[test]
fn bundled_module_sources_parse_clean() {
    let mut checked = 0;
    for name in almide::stdlib_info::BUNDLED_MODULES {
        if let Some(src) = almide::stdlib_info::bundled_source(name) {
            assert_parses_clean(name, src);
            checked += 1;
        }
    }
    assert!(checked > 0, "no bundled sources found — wiring broken?");
}

/// Every .almd file under stdlib/ on disk, including the WASM self-host
/// split files that bundled_source() does not cover.
#[test]
fn stdlib_dir_sources_parse_clean() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib");
    let mut checked = 0;
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("stdlib/ dir missing")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "almd"))
        .collect();
    entries.sort();
    for path in entries {
        let src = std::fs::read_to_string(&path).expect("read stdlib file");
        assert_parses_clean(&path.display().to_string(), &src);
        checked += 1;
    }
    assert!(checked > 100, "expected the full stdlib sweep, got {checked}");
}
