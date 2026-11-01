//! The prim floor's machine ops are DECLARED, once, at each prim fn (#4006).
//!
//! `stdlib/prim.almd` gives every fn exactly one `@prim_op(…)`, and MIR
//! lowers a prim call by reading it (`almide_mir::prim_ops`). This gate holds
//! both halves:
//!
//! * every prim fn carries exactly one `@prim_op` that decodes, and a fn
//!   without one is reported (the negative control runs the same check on
//!   prim.almd with one attribute removed);
//! * MIR's source names no prim fn — a prim fn's name as a string literal
//!   anywhere in `crates/almide-mir/src` would be a second, name-keyed copy of
//!   what the declaration says. The only exemption is a callee being BUILT
//!   (`sym("handle")` in a desugar that synthesizes a `prim.handle` call),
//!   which names the call it emits rather than deciding what a call does.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn prim_source() -> String {
    std::fs::read_to_string(root().join("stdlib/prim.almd")).expect("read stdlib/prim.almd")
}

/// Every prim fn that does not carry exactly one decodable `@prim_op`.
fn violations(source: &str) -> Vec<String> {
    let decls = almide_mir::prim_ops::declarations_in(source).expect("prim.almd parses");
    decls
        .iter()
        .filter_map(|(func, ops)| match &ops[..] {
            [Ok(_)] => None,
            [Err(e)] => Some(format!("prim.{func}: {e}")),
            [] => Some(format!("prim.{func}: no @prim_op")),
            more => Some(format!("prim.{func}: {} @prim_op declarations", more.len())),
        })
        .collect()
}

#[test]
fn every_prim_fn_declares_exactly_one_op() {
    let source = prim_source();
    let decls = almide_mir::prim_ops::declarations_in(&source).expect("prim.almd parses");
    assert!(decls.len() >= 80, "only {} fns found in prim.almd — the scan is blind", decls.len());
    let bad = violations(&source);
    assert!(bad.is_empty(), "prim fns without exactly one decodable @prim_op:\n  {}", bad.join("\n  "));
}

#[test]
fn a_prim_fn_missing_its_op_is_reported() {
    let source = prim_source();
    // The first attribute LINE (the file's header comment also spells `@prim_op(`).
    let at = source.find("\n@prim_op(").expect("prim.almd declares ops") + 1;
    let end = at + source[at..].find('\n').expect("attribute line ends");
    let mutated = format!("{}{}", &source[..at], &source[end + 1..]);
    let bad = violations(&mutated);
    assert_eq!(bad.len(), 1, "removing one @prim_op must report exactly that fn: {bad:?}");
    assert!(bad[0].ends_with("no @prim_op"), "{bad:?}");
    let doubled = format!("{}{}", &source[..end + 1], &source[at..]);
    assert_eq!(violations(&doubled).len(), 1, "a doubled @prim_op must be reported");
}

#[test]
fn the_embedded_copy_is_the_declaration_the_lowering_reads() {
    // Every prim fn resolves through the table the lowering consults.
    let source = prim_source();
    for (func, _) in almide_mir::prim_ops::declarations_in(&source).expect("parses") {
        assert!(
            almide_mir::prim_ops::declared(&func).is_ok(),
            "prim.{func} is declared on disk but the lowering's table does not resolve it: {:?}",
            almide_mir::prim_ops::declared(&func)
        );
    }
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).expect("read dir") {
        let p = e.expect("entry").path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn mir_names_no_prim_fn() {
    let source = prim_source();
    let names: Vec<String> = almide_mir::prim_ops::declarations_in(&source)
        .expect("parses")
        .into_iter()
        .map(|(f, _)| f)
        .collect();
    let mut files = Vec::new();
    rust_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    assert!(files.len() > 50, "the scan found only {} files", files.len());
    let mut found = Vec::new();
    for f in &files {
        // A `tests_*.rs` file is a `#[cfg(test)]` module of the crate, not the lowering.
        if f.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.starts_with("tests_")) {
            continue;
        }
        let text = std::fs::read_to_string(f).expect("read source");
        for name in &names {
            let lit = format!("\"{name}\"");
            for (i, _) in text.match_indices(&lit) {
                let before = &text[..i];
                if before.ends_with("sym(") {
                    continue;
                }
                // `("fs", "rename")`: another module's fn that shares a prim fn's
                // name — the pair names that module's fn, not the prim.
                let head = before.trim_end();
                if let Some(rest) = head.strip_suffix(',') {
                    let rest = rest.trim_end();
                    if let Some(q) = rest.strip_suffix('"') {
                        let module = &q[q.rfind('"').map_or(0, |k| k + 1)..];
                        if module != "prim" && !module.is_empty() && module.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                            continue;
                        }
                    }
                }
                let line = before.matches('\n').count() + 1;
                found.push(format!("{}:{line}: {lit}", f.display()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "MIR names prim fns — read the op from its @prim_op declaration instead:\n  {}",
        found.join("\n  ")
    );
}
