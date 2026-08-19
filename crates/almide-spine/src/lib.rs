//! SPIKE S1 (ARCHITECTURE.md §6.5): the smallest honest salsa spine over the
//! verbatim-ported parser, built to MEASURE three claims on the real corpus:
//!   (a) editing one file re-derives only that file's parse query,
//!   (b) salsa's cold overhead over raw batch parsing is small,
//!   (c) warm re-derive beats batch front-end re-parse by ≥10x.
//! It deliberately proves nothing about the check phase — that is unit 4.

pub mod s2;
pub mod s3;

use std::sync::atomic::{AtomicUsize, Ordering};

/// Global count of actual `parse_digest` EXECUTIONS (not memo hits) — the
/// witness for claim (a). Reset per measurement round by the bench.
pub static PARSE_EXECUTIONS: AtomicUsize = AtomicUsize::new(0);

#[salsa::input]
pub struct SourceFile {
    #[returns(ref)]
    pub path: String,
    #[returns(ref)]
    pub text: String,
}

#[salsa::input]
pub struct Project {
    #[returns(ref)]
    pub files: Vec<SourceFile>,
}

/// Small memoizable summary of one file's parse. The full `Program` does not
/// implement `Eq`, and the spike's aggregator only needs a fingerprint — the
/// parse itself (the cost being measured) runs inside and is memoized.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ParseDigest {
    pub items: u32,
    pub parse_errors: u32,
    pub ok: bool,
}

#[salsa::tracked]
pub fn parse_digest(db: &dyn salsa::Database, file: SourceFile) -> ParseDigest {
    PARSE_EXECUTIONS.fetch_add(1, Ordering::Relaxed);
    let tokens = almide_syntax::lexer::Lexer::tokenize(file.text(db));
    let mut parser = almide_syntax::parser::Parser::new(tokens).with_file(file.path(db));
    match parser.parse() {
        Ok(prog) => ParseDigest {
            items: prog.decls.len() as u32,
            parse_errors: parser.errors.len() as u32,
            ok: true,
        },
        Err(_) => ParseDigest { items: 0, parse_errors: 1, ok: false },
    }
}

/// Whole-project rollup: depends on every file's `parse_digest`, so editing
/// one file re-runs exactly one parse plus this cheap fold.
#[salsa::tracked]
pub fn project_digest(db: &dyn salsa::Database, project: Project) -> u64 {
    let mut acc: u64 = 0;
    for f in project.files(db) {
        let d = parse_digest(db, *f);
        acc = acc
            .wrapping_mul(1_000_003)
            .wrapping_add(u64::from(d.items) << 2 | u64::from(d.parse_errors) << 1 | u64::from(d.ok));
    }
    acc
}

#[salsa::db]
#[derive(Default, Clone)]
pub struct SpineDb {
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl salsa::Database for SpineDb {}
