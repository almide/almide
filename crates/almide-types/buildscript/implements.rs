//! Generate `src/generated/implements.rs`: the stdlib's `@implements("m.f")`
//! declarations, the self-hosted bodies that say which stdlib function they
//! are (#3620). `self_host_registry::self_host_runtime()` serves these rows
//! beside its hand-written table, so a new wasm body is registered where it is
//! written, not by adding its name to a list (the name-keyed gate,
//! tests/name_keyed_sites_test.rs, holds that table to shrinking).
//!
//! The rows are written in the registry's own syntax —
//! `(crate::embedded::SRC_<STEM>, &[("impl", "m.f"), ..])` — so every reader
//! of the registry text (scripts/lib/impl-count-enumerate.py) reads them the
//! same way.
//!
//! Each declared body also records whether it reaches `prim.`: its own lines,
//! or those of any function of its file it calls, transitively (#4204 — the
//! file was the unit before, so one prim-using helper in a file kept every
//! other body of that file out). A body that does not builds every value
//! through stdlib surface the wasm emitter lowers itself, so it cannot couple
//! to a foreign block layout: the property the emitter's hand-audited
//! admission tiers certify one name at a time
//! (crates/almide-wasm/src/whitelist.rs). The emitter admits such bodies
//! without a list entry.
//!
//! A text scan, as the other tables here are: the attribute lines directly
//! above a `fn` / `effect fn`, and the fn's name. The build fails on a
//! declaration that names no stdlib function, sits above no `fn`, or repeats a
//! name another body already claims.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

struct Decl {
    stem: String,
    impl_fn: String,
    surface: String,
    prim_free: bool,
}

/// The file's top-level functions as `(name, body lines)`, comment lines
/// dropped: a function runs from its `fn` line to the next top-level one.
fn fn_bodies(src: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with("//") {
            continue;
        }
        let top = !line.starts_with(' ') && !line.starts_with('\t');
        match fn_name(line).filter(|_| top) {
            Some(name) => out.push((name, format!("{line}\n"))),
            None => {
                if let Some(last) = out.last_mut() {
                    last.1.push_str(line);
                    last.1.push('\n');
                }
            }
        }
    }
    out
}

/// Does `name` reach `prim.`, through its own body or a same-file callee?
fn reaches_prim(bodies: &[(String, String)], name: &str) -> bool {
    let mut todo = vec![name.to_string()];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(n) = todo.pop() {
        if !seen.insert(n.clone()) {
            continue;
        }
        let Some((_, body)) = bodies.iter().find(|(f, _)| *f == n) else { continue };
        if body.contains("prim.") {
            return true;
        }
        for (f, _) in bodies {
            if !seen.contains(f) && calls(body, f) {
                todo.push(f.clone());
            }
        }
    }
    false
}

/// Does `body` call `f` (`f(` with no identifier character before it)?
fn calls(body: &str, f: &str) -> bool {
    body.match_indices(&format!("{f}(")).any(|(i, _)| {
        !body[..i].chars().next_back().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
    })
}

/// `@implements("m.f")` -> Some("m.f").
fn implements_arg(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix("@implements")?;
    let inner = rest.trim().strip_prefix('(')?.rsplit_once(')')?.0;
    Some(inner.trim().trim_matches('"').to_string())
}

/// The name of `[effect ]fn name[..](..)`.
fn fn_name(line: &str) -> Option<String> {
    let t = line.trim();
    let t = t.strip_prefix("pub ").unwrap_or(t);
    let t = t.strip_prefix("effect ").unwrap_or(t);
    let after = t.strip_prefix("fn ")?;
    let name: String = after.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    (!name.is_empty()).then_some(name)
}

fn scan(path: &Path) -> Vec<Decl> {
    let stem = path.file_stem().expect("stem").to_string_lossy().to_string();
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut out = Vec::new();
    let mut pending: Option<String> = None;
    let bodies = fn_bodies(&src);
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with("//") {
            continue;
        }
        if let Some(surface) = implements_arg(t) {
            pending = Some(surface);
            continue;
        }
        if t.starts_with('@') {
            continue;
        }
        if let Some(surface) = pending.take() {
            let impl_fn = fn_name(t)
                .unwrap_or_else(|| panic!("{}: @implements(\"{surface}\") is not above a `fn`: {t}", path.display()));
            let prim_free = !reaches_prim(&bodies, &impl_fn);
            out.push(Decl { stem: stem.clone(), impl_fn, surface, prim_free });
        }
    }
    if let Some(surface) = pending {
        panic!("{}: @implements(\"{surface}\") is not above a `fn`", path.display());
    }
    out
}

/// Does `stdlib/<module>.almd` declare `fn <func>`?
fn declares(stdlib: &Path, module: &str, func: &str) -> bool {
    let Ok(src) = std::fs::read_to_string(stdlib.join(format!("{module}.almd"))) else {
        return false;
    };
    src.lines().any(|l| fn_name(l).as_deref() == Some(func))
}

pub fn generate(root: &Path, files: &[PathBuf], out_path: &Path) {
    let stdlib = root.join("stdlib");
    let mut rows: Vec<(String, Vec<Decl>)> = Vec::new();
    for f in files {
        let decls = scan(f);
        if !decls.is_empty() {
            rows.push((decls[0].stem.clone(), decls));
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let mut seen = std::collections::BTreeSet::new();
    for d in rows.iter().flat_map(|r| &r.1) {
        let (module, func) = d
            .surface
            .split_once('.')
            .unwrap_or_else(|| panic!("{}.almd: @implements(\"{}\") is not `module.fn`", d.stem, d.surface));
        assert!(
            declares(&stdlib, module, func),
            "{}.almd: @implements(\"{}\") names no function declared in stdlib/{module}.almd",
            d.stem,
            d.surface
        );
        assert!(seen.insert(d.surface.clone()), "@implements(\"{}\") is declared twice", d.surface);
    }
    let mut out = String::from(
        "// @generated by crates/almide-types/buildscript/implements.rs from the stdlib's @implements declarations — do not edit.\n\n",
    );
    out.push_str("pub const DECLARED: &[(&str, &[(&str, &str)])] = &[\n");
    for (stem, decls) in &rows {
        let pairs: Vec<String> = decls.iter().map(|d| format!("({:?}, {:?})", d.impl_fn, d.surface)).collect();
        writeln!(out, "    (crate::embedded::SRC_{}, &[{}]),", stem.to_uppercase(), pairs.join(", ")).expect("write");
    }
    out.push_str("];\n\n/// The declared impl fns that never reach `prim.`, through their own body or a same-file callee.\n");
    out.push_str("pub const PRIM_FREE: &[&str] = &[\n");
    for d in rows.iter().flat_map(|r| &r.1).filter(|d| d.prim_free) {
        writeln!(out, "    {:?},", d.impl_fn).expect("write");
    }
    out.push_str("];\n");
    super::in_checkout::write_if_changed(out_path, &out);
}
