//! Process-wide cache for parsed `.almd` source strings.
//!
//! Multiple downstream consumers (frontend `bundled_sigs`, codegen
//! `pass_stdlib_lowering` template extraction) parse the same
//! `include_str!`-backed bundled stdlib sources independently. Each
//! parse walks the lexer + parser end-to-end; for the ~30 bundled
//! modules touched per invocation that's a measurable startup cost
//! and, more importantly, a coherence hazard — two extracted views
//! of the same source can drift apart if one consumer caches and
//! the other re-parses.
//!
//! `parse_cached` keys the cache by the source's pointer address.
//! `include_str!` returns a unique `&'static str` per file, so each
//! bundled module collapses to one entry. Non-static callers (test
//! helpers passing freshly allocated strings) hit the slow path
//! every time, which is fine — the cache is tuned for the bundled
//! stdlib hot path, not arbitrary input.
//!
//! Errors are not cached: a parse failure leaves the entry empty so
//! a corrected source string parses afresh on the next call.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use crate::ast::Program;
use crate::lexer::Lexer;
use crate::parser::Parser;

fn cache() -> &'static RwLock<HashMap<usize, &'static Program>> {
    static CELL: OnceLock<RwLock<HashMap<usize, &'static Program>>> = OnceLock::new();
    CELL.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Parse a source string once, return a cached AST reference.
///
/// Returns `None` if parsing fails. The returned reference lives
/// for the process lifetime (via `Box::leak`); the leaked memory
/// is bounded by the number of distinct source pointers passed in
/// — for the bundled stdlib that's ≤ 35 entries.
pub fn parse_cached(source: &'static str) -> Option<&'static Program> {
    let key = source.as_ptr() as usize;
    if let Some(prog) = cache().read().ok().and_then(|g| g.get(&key).copied()) {
        return Some(prog);
    }
    let tokens = Lexer::tokenize(source);
    let mut parser = Parser::new(tokens);
    let program = parser.parse().ok()?;
    let leaked: &'static Program = Box::leak(Box::new(program));
    if let Ok(mut g) = cache().write() {
        g.insert(key, leaked);
    }
    Some(leaked)
}
