//! #2344: the closed set the pool's dedup safety argument rests on.
//!
//! `Pool::intern` deduplicates, so a string literal block is SHARED between
//! every site that names it instead of being private to one. The audit that
//! licensed that change enumerated why it is safe, and the load-bearing
//! clause is a COUNT: `cow_fn_of` has exactly four call sites, three of them
//! list paths that cannot see a pooled block and one that reads a `mut` var
//! where only a Str is reachable.
//!
//! A count is exactly the kind of premise that expires without anyone
//! noticing. Add a fifth cow-then-write site, or pool a list, and the
//! argument is void while every test stays green — so the premise is checked
//! here rather than trusted. This test does not prove the dedup is safe; it
//! fails when the reasoning that did stops applying.

use std::collections::BTreeMap;
use std::path::PathBuf;

fn src_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_sources() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![src_dir()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read src dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let name = path
                    .strip_prefix(src_dir())
                    .expect("under src")
                    .to_string_lossy()
                    .into_owned();
                out.push((name, std::fs::read_to_string(&path).expect("read source")));
            }
        }
    }
    out.sort();
    out
}

/// The four sites, by file. `cow_fn_of`'s own definition and the helper body
/// `emit_cow_elems` (which calls `F_COW` as the implementation of the variant
/// `cow_fn_of` returns for a list of heap elements) are not call sites and
/// are excluded by matching on the CALL spelling.
const DECLARED_COW_CALLERS: &[(&str, &str)] = &[
    ("emitter_vars.rs", "reads a `mut` var — only a Str is a reachable static"),
    ("stmts_append.rs", "$list_push — lists are never pooled"),
    ("stmts_index.rs", "element slot — lists are never pooled"),
    ("tail_append.rs", "$list_push — lists are never pooled"),
];

#[test]
fn cow_then_write_is_still_the_declared_set_of_four() {
    let mut found: BTreeMap<String, usize> = BTreeMap::new();
    for (name, text) in rust_sources() {
        // `self.cow_fn_of(` is the call; `fn cow_fn_of(` is the definition.
        let n = text.matches("self.cow_fn_of(").count();
        if n > 0 {
            *found.entry(name).or_default() += n;
        }
    }
    let declared: BTreeMap<&str, &str> = DECLARED_COW_CALLERS.iter().copied().collect();

    let unexpected: Vec<&String> = found.keys().filter(|f| !declared.contains_key(f.as_str())).collect();
    assert!(
        unexpected.is_empty(),
        "a new cow-then-write site appeared in {unexpected:?}.\n\
         The #2344 pool-dedup argument rests on the four declared sites being \
         the only ones, because a deduped literal block is shared between the \
         sites that name it. Before adding this file to DECLARED_COW_CALLERS, \
         establish that the new site cannot reach a POOLED block — either its \
         value is never a string literal, or it tests the static boundary \
         itself the way emit_str_append and try_map_set_in_place do. `$cow` \
         is not the protection: emit_cow returns a static uncopied."
    );

    let missing: Vec<&&str> = declared
        .keys()
        .filter(|f| !found.contains_key(**f))
        .collect();
    assert!(
        missing.is_empty(),
        "declared cow-then-write site(s) {missing:?} no longer call cow_fn_of.\n\
         If the site is gone the entry should go with it; a declaration that \
         matches nothing is how this gate would quietly stop checking anything."
    );
    assert_eq!(found.len(), DECLARED_COW_CALLERS.len(), "found: {found:?}");
}

/// Clause 1: the pool takes strings and opaque payload bytes, and nothing
/// else. A `pub(crate) fn` on `Pool` that admits a third kind of content is
/// the other way the argument expires.
#[test]
fn the_pool_still_admits_only_strings_and_block_payloads() {
    let func_rs = std::fs::read_to_string(src_dir().join("func.rs")).expect("read func.rs");
    let pool_impl = func_rs
        .split_once("impl Pool {")
        .expect("Pool impl")
        .1
        .split_once("\n}\n")
        .expect("end of Pool impl")
        .0;
    let entry_points: Vec<&str> = pool_impl
        .lines()
        .filter(|l| l.trim_start().starts_with("pub(crate) fn "))
        .map(|l| l.trim())
        .collect();
    assert_eq!(
        entry_points,
        vec![
            "pub(crate) fn new() -> Self {",
            "pub(crate) fn intern(&mut self, s: &str) -> u32 {",
            "pub(crate) fn intern_block(&mut self, payload: &[u8]) -> u32 {",
        ],
        "the Pool's entry points changed. Dedup is safe because the pool holds \
         string literals and capture-free closure blocks — neither has element \
         slots a program can assign into. A new way in needs that argument \
         re-made before this list is updated."
    );
}

/// Clause 2, as an assertion rather than a comment: `emit_cow` returns a
/// static UNCOPIED, so a reader must not treat `cow(x)` as "safe to write".
/// If this early return is ever removed, the invariant comment on
/// `Pool::intern` becomes wrong and should be rewritten, not silently left.
#[test]
fn cow_still_returns_a_static_uncopied() {
    let alloc_rs =
        std::fs::read_to_string(src_dir().join("runtime_alloc.rs")).expect("read runtime_alloc.rs");
    let body = alloc_rs
        .split_once("pub(crate) fn emit_cow(")
        .expect("emit_cow")
        .1;
    let head: String = body.lines().take(14).collect::<Vec<_>>().join("\n");
    assert!(
        head.contains("G_LINE_END"),
        "emit_cow no longer tests the static boundary up front:\n{head}"
    );
}
