//! C-277 family gate (#1416): every native-runtime fn with a RETURN-ONLY
//! generic — a type parameter appearing in no parameter type — must have a
//! turbofish arm in the walker's `render_runtime_ctor_turbofish`.
//!
//! Why: such a fn ships its type in the return position only, so the emitted
//! call site is the one place the checker's resolution can survive. Whenever
//! the surrounding context is erased (const-folding `if true then
//! list.with_capacity(4) else [1, 2]` in argument position) and the consumer
//! is element-agnostic (`list.is_empty`), a bare call leaves rustc nothing to
//! infer from: E0282 AFTER `almide check` accepted — the acceptance-gap class
//! (#809, #1416). The family so far: `almide_rt_map_new`, `almide_rt_set_new`
//! (nightly-fuzz seed 1785045556318379299), `almide_rt_list_with_capacity`
//! (#1416, both-arch dispatch seed 508666777783).
//!
//! Both sides are MEASURED from source at test time — the family from
//! `runtime/rs/src/*.rs` signatures, the arms from the walker — so adding a
//! new return-only-generic runtime fn without its emit arm (or leaving a
//! stale arm behind a removed fn) fails here, in the same PR, not in the next
//! fuzz night.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Split a Rust type/param string into identifier tokens.
fn ident_tokens(s: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut cur = String::new();
    for c in s.chars() {
        if c.is_alphanumeric() || c == '_' {
            cur.push(c);
        } else if !cur.is_empty() {
            out.insert(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.insert(cur);
    }
    out
}

/// Split a generic list on TOP-LEVEL commas only (`F: Fn(A, B) -> C` is one
/// entry; the comma inside `Fn(A, B)` must not split it).
fn split_generic_entries(generics: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    let mut prev = '\0';
    for c in generics.chars() {
        match c {
            '<' | '(' | '[' => depth += 1,
            // `->` in an Fn bound is an arrow, not a closer.
            '>' if prev != '-' => depth -= 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                prev = c;
                continue;
            }
            _ => {}
        }
        cur.push(c);
        prev = c;
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// Scan `runtime/rs/src/*.rs` for `pub fn almide_rt_*<G, ...>(params...)`
/// signatures and return the names whose generic list has at least one
/// parameter that occurs in NO function parameter type (return-only).
fn return_only_generic_runtime_fns() -> BTreeSet<String> {
    let dir = repo_root().join("runtime/rs/src");
    let mut family = BTreeSet::new();
    for entry in fs::read_dir(&dir).expect("runtime/rs/src must exist") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let src = fs::read_to_string(&path).expect("readable runtime source");
        let bytes = src.as_bytes();
        let mut from = 0;
        while let Some(rel) = src[from..].find("pub fn almide_rt_") {
            let fn_start = from + rel + "pub fn ".len();
            from = fn_start;
            // Name runs to `<` (generic fn) or `(` (monomorphic — skip).
            let rest = &src[fn_start..];
            let name_end = match rest.find(|c: char| c == '<' || c == '(') {
                Some(i) => i,
                None => continue,
            };
            if rest.as_bytes()[name_end] != b'<' {
                continue;
            }
            let name = rest[..name_end].trim().to_string();
            // Generic list: balance `<...>` (bounds can nest, e.g. `F: Fn(A) -> B`).
            let gen_open = fn_start + name_end;
            let mut depth = 0usize;
            let mut i = gen_open;
            let gen_close = loop {
                match bytes[i] {
                    b'<' => depth += 1,
                    // `->` in an Fn bound is an arrow, not a closer.
                    b'>' if bytes[i - 1] != b'-' => {
                        depth -= 1;
                        if depth == 0 {
                            break i;
                        }
                    }
                    _ => {}
                }
                i += 1;
            };
            let generics = &src[gen_open + 1..gen_close];
            // Param list: balance `(...)` right after the generics.
            let par_open = gen_close + src[gen_close..].find('(').expect("param list");
            let mut depth = 0usize;
            let mut i = par_open;
            let par_close = loop {
                match bytes[i] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break i;
                        }
                    }
                    _ => {}
                }
                i += 1;
            };
            let params = &src[par_open + 1..par_close];
            // A generic is PINNED by the arguments if it appears in a
            // parameter type, or transitively through another pinned generic's
            // bound: in `<A, F: Fn(A) -> A>(xs: Vec<X>, f: F)` the closure
            // argument's concrete type carries `A`, so rustc infers it —
            // that is not the E0282 family. Split each generic entry into
            // (name, bound idents), seed the pinned set from the param list,
            // and close over bounds to fixpoint.
            let entries: Vec<(String, BTreeSet<String>)> = split_generic_entries(generics)
                .into_iter()
                .filter_map(|e| {
                    let (name_part, bound) = match e.split_once(':') {
                        Some((n, b)) => (n, ident_tokens(b)),
                        None => (e.as_str(), BTreeSet::new()),
                    };
                    let n = name_part.trim().trim_start_matches("const ").trim();
                    (!n.is_empty()).then(|| (n.to_string(), bound))
                })
                .collect();
            let mut pinned = ident_tokens(params);
            loop {
                let before = pinned.len();
                for (n, bound) in &entries {
                    if pinned.contains(n) {
                        pinned.extend(bound.iter().cloned());
                    }
                }
                if pinned.len() == before {
                    break;
                }
            }
            if entries.iter().any(|(n, _)| !pinned.contains(n)) {
                family.insert(name.clone());
            }
        }
    }
    assert!(
        !family.is_empty(),
        "signature scan found no return-only-generic runtime fns — the parser \
         broke, not the family"
    );
    family
}

/// Extract the `almide_rt_*` symbols matched inside the walker's
/// `render_runtime_ctor_turbofish`.
fn turbofish_arm_symbols() -> BTreeSet<String> {
    let path = repo_root().join("crates/almide-codegen/src/walker/expressions.rs");
    let src = fs::read_to_string(&path).expect("walker source");
    let start = src
        .find("fn render_runtime_ctor_turbofish")
        .expect("render_runtime_ctor_turbofish must exist in the walker");
    // The next top-level `fn ` ends the function — string-literal scan only
    // needs a window that covers the whole body.
    let end = src[start + 1..]
        .find("\nfn ")
        .map(|i| start + 1 + i)
        .unwrap_or(src.len());
    let body = &src[start..end];
    let mut arms = BTreeSet::new();
    let mut from = 0;
    while let Some(rel) = body[from..].find("\"almide_rt_") {
        let lit_start = from + rel + 1;
        // Both the match-pattern literal (`"almide_rt_map_new"`) and the
        // format string (`"almide_rt_map_new::<{}, {}>()"`) begin with the
        // symbol — take the identifier prefix so they normalize to one name.
        let name: String = body[lit_start..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        from = lit_start + name.len();
        arms.insert(name);
    }
    arms
}

#[test]
fn every_return_only_generic_ctor_has_a_turbofish_arm() {
    let family = return_only_generic_runtime_fns();
    let arms = turbofish_arm_symbols();

    let missing: Vec<_> = family.difference(&arms).collect();
    assert!(
        missing.is_empty(),
        "runtime fns with a return-only generic but NO turbofish arm in \
         render_runtime_ctor_turbofish: {missing:?}\n\
         A bare call to these can E0282 after `check` accepted whenever \
         const-folding erases the typing context (#1416). Add an arm that pins \
         the node's resolved type — see the almide_rt_list_with_capacity arm."
    );

    let stale: Vec<_> = arms.difference(&family).collect();
    assert!(
        stale.is_empty(),
        "turbofish arms in render_runtime_ctor_turbofish for symbols that are \
         no longer return-only-generic runtime fns: {stale:?}\n\
         Remove the arm (or the signature scan in this test broke)."
    );
}
