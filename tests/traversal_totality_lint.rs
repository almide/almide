//! Traversal-totality lint — Phase 1 of `docs/roadmap/active/codegen-traversal-totality.md`.
//!
//! Bans the one failure class behind the native↔WASM capture divergences (DIV2):
//! a `match` over an `IrExprKind` (`… .kind { … }`) whose **catch-all** arm
//! *silently drops the subtree* — `_ => {}` (does nothing) or `other => other`
//! (returns the node unrecursed). Such an arm skips the children of every
//! un-listed / future node kind, so a closure/var nested inside one is invisible
//! to the pass and native codegen diverges from WASM.
//!
//! The enemy is *silence*, not the wildcard. A catch-all is fine when it:
//!   - **delegates** to the exhaustive primitive (`walk_expr(_mut)`, `map_children`), or
//!   - **diverges loudly** (`unreachable!`/`todo!`/`panic!`/`unimplemented!` — the
//!     Swift-`Never` provisional), or
//!   - returns a *value* (`_ => None`, `_ => false`) in a decision match (not recursion).
//!
//! Only `_ => {}` / `binder => binder` / `_ => <scrutinee>.kind` are banned.
//!
//! Ratchet: `LEGACY_DEBT` grandfathers files still carrying the pattern. Migrate
//! them per the roadmap and REMOVE the entry — never add one. A clean file that
//! regresses, or any new file with the pattern, fails this test.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Files whose hand-rolled IR traversal still contains silent catch-alls.
/// Each must be migrated to a canonical primitive (`IrMutVisitor`/`map_children`)
/// or have its catch-all made loud/delegating; then delete it from this list.
const LEGACY_DEBT: &[&str] = &[
    "crates/almide-codegen/src/pass_anf.rs",
    "crates/almide-codegen/src/pass_auto_parallel.rs",
    "crates/almide-codegen/src/pass_borrow_inference.rs",
    "crates/almide-codegen/src/pass_box_deref.rs",
    "crates/almide-codegen/src/pass_builtin_lowering.rs",
    "crates/almide-codegen/src/pass_capture_clone.rs",
    "crates/almide-codegen/src/pass_clone.rs",
    "crates/almide-codegen/src/pass_closure_conversion.rs",
    "crates/almide-codegen/src/pass_concretize_types.rs",
    "crates/almide-codegen/src/pass_const_fold.rs",
    "crates/almide-codegen/src/pass_effect_inference.rs",
    "crates/almide-codegen/src/pass_fan_lowering.rs",
    "crates/almide-codegen/src/pass_lambda_type_resolve.rs",
    "crates/almide-codegen/src/pass_licm.rs",
    "crates/almide-codegen/src/pass_list_pattern.rs",
    "crates/almide-codegen/src/pass_match_lowering.rs",
    "crates/almide-codegen/src/pass_matrix_shape_spec.rs",
    "crates/almide-codegen/src/pass_mut_param_lowering.rs",
    "crates/almide-codegen/src/pass_peephole.rs",
    "crates/almide-codegen/src/pass_perceus.rs",
    "crates/almide-codegen/src/pass_result_erasure.rs",
    "crates/almide-codegen/src/pass_result_propagation.rs",
    "crates/almide-codegen/src/pass_rust_lowering.rs",
    "crates/almide-codegen/src/pass_shadow_resolve.rs",
    "crates/almide-codegen/src/pass_stdlib_lowering.rs",
    "crates/almide-codegen/src/pass_tco.rs",
];

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is the crate root (the repo root for this workspace member).
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The set of source files whose IR traversal we govern: every codegen pass plus
/// the almide-ir traversal primitives.
fn governed_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let pass_dir = root.join("crates/almide-codegen/src");
    if let Ok(entries) = std::fs::read_dir(&pass_dir) {
        for e in entries.flatten() {
            let p = e.path();
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("pass_") && name.ends_with(".rs") {
                    out.push(p);
                }
            }
        }
    }
    for f in ["fold.rs", "visit.rs", "visit_mut.rs", "substitute.rs", "free_vars.rs"] {
        out.push(root.join("crates/almide-ir/src").join(f));
    }
    out.sort();
    out
}

/// Replace `//`-comments, `/* */`-comments and string-literal contents with
/// spaces (newlines preserved) so the scanner never trips on doc examples.
fn blank_comments_and_strings(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = vec![b' '; b.len()];
    // Preserve newlines for line numbering.
    for (i, &c) in b.iter().enumerate() {
        if c == b'\n' { out[i] = b'\n'; }
    }
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            while i < b.len() && b[i] != b'\n' { i += 1; }
        } else if c == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') { i += 1; }
            i += 2;
        } else if c == b'"' {
            i += 1;
            while i < b.len() && b[i] != b'"' {
                if b[i] == b'\\' { i += 1; }
                i += 1;
            }
            i += 1;
        } else if c == b'\'' {
            // char literal or lifetime; only treat as literal when it closes quickly
            if i + 2 < b.len() && b[i + 1] != b'\\' && b[i + 2] == b'\'' {
                i += 3;
            } else if i + 3 < b.len() && b[i + 1] == b'\\' {
                i += 4;
            } else {
                out[i] = c; // lifetime tick — keep
                i += 1;
            }
        } else {
            out[i] = c;
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

#[derive(Debug)]
struct Violation {
    line: usize,
    arm: String,
}

/// Scan `src` (already comment/string-blanked) for silent catch-alls inside
/// `match … .kind { … }` blocks.
fn find_violations(src: &str) -> Vec<Violation> {
    let b = src.as_bytes();
    let mut violations = Vec::new();

    // Find every `match` keyword (word-boundary).
    let mut search = 0;
    while let Some(rel) = src[search..].find("match") {
        let m = search + rel;
        search = m + 5;
        // word boundary before/after
        let before_ok = m == 0 || !is_ident_byte(b[m - 1]);
        let after_ok = m + 5 >= b.len() || !is_ident_byte(b[m + 5]);
        if !(before_ok && after_ok) { continue; }

        // Scrutinee runs from after `match` to the body-opening `{`.
        let mut j = m + 5;
        let mut depth_paren = 0i32; // (), [], <> we ignore; only stop at top-level `{`
        let body_open;
        loop {
            if j >= b.len() { body_open = None; break; }
            match b[j] {
                b'(' | b'[' => depth_paren += 1,
                b')' | b']' => depth_paren -= 1,
                b'{' if depth_paren == 0 => { body_open = Some(j); break; }
                _ => {}
            }
            j += 1;
        }
        let Some(open) = body_open else { continue };
        let scrutinee = &src[m + 5..open];
        if !scrutinee.contains(".kind") { continue; }

        // Find the matching `}` for the match body.
        let mut depth = 0i32;
        let mut k = open;
        let mut body_close = None;
        while k < b.len() {
            match b[k] {
                b'{' => depth += 1,
                b'}' => { depth -= 1; if depth == 0 { body_close = Some(k); break; } }
                _ => {}
            }
            k += 1;
        }
        let Some(close) = body_close else { continue };

        // A catch-all is only a *recursion* drop when this match IS the descent.
        // If the enclosing function delegates the rest of the walk to a canonical
        // primitive (`walk_expr(_mut)` / `map_children` / `self.visit_*`), the
        // match is a side-table override (binding collection, var shifting, …) and
        // its `_ => {}` is correct — recursion happens via the delegation.
        if enclosing_fn_delegates(src, b, open) {
            continue;
        }

        // Within (open, close), find catch-all arms at arm-level (depth == 1).
        scan_arms(src, b, open, close, &mut violations);
    }

    violations
}

/// Canonical-walk delegations: their presence in the enclosing function means
/// the function descends via the exhaustive primitive, so a hand-written
/// `.kind` match within it is a safe partial override, not the recursion point.
const DELEGATIONS: &[&str] = &[
    "walk_expr", "walk_stmt", "walk_pattern", "walk_expr_mut", "walk_stmt_mut",
    "map_children", "map_exprs", ".visit_expr", ".visit_stmt",
];

/// Does the function body enclosing byte `pos` call a canonical-walk primitive?
fn enclosing_fn_delegates(src: &str, b: &[u8], pos: usize) -> bool {
    // Find the innermost `fn …(…) … { … }` whose body brackets `pos`.
    let mut best: Option<(usize, usize)> = None;
    let mut search = 0;
    while let Some(rel) = src[search..].find("fn ") {
        let f = search + rel;
        search = f + 3;
        let before_ok = f == 0 || !is_ident_byte(b[f - 1]);
        if !before_ok { continue; }
        // Body opens at the first `{` after the signature's `)` (skip `where`…).
        let mut j = f + 3;
        let mut paren = 0i32;
        let mut seen_params = false;
        let mut body_open = None;
        while j < b.len() {
            match b[j] {
                b'(' => { paren += 1; seen_params = true; }
                b')' => paren -= 1,
                b'{' if paren == 0 && seen_params => { body_open = Some(j); break; }
                b';' if paren == 0 => break, // fn-type / trait decl, no body
                _ => {}
            }
            j += 1;
        }
        let Some(bo) = body_open else { continue };
        let mut depth = 0i32;
        let mut k = bo;
        let mut body_close = None;
        while k < b.len() {
            match b[k] {
                b'{' => depth += 1,
                b'}' => { depth -= 1; if depth == 0 { body_close = Some(k); break; } }
                _ => {}
            }
            k += 1;
        }
        let Some(bc) = body_close else { continue };
        if bo < pos && pos < bc {
            // innermost = smallest containing range
            if best.map_or(true, |(s, e)| bo > s || bc < e) {
                best = Some((bo, bc));
            }
        }
    }
    if let Some((s, e)) = best {
        let body = &src[s..e];
        return DELEGATIONS.iter().any(|d| body.contains(d));
    }
    false
}

fn scan_arms(src: &str, b: &[u8], open: usize, close: usize, out: &mut Vec<Violation>) {
    // Walk the body; an arm pattern begins at arm-level (depth 1) right after the
    // body `{`, or after a top-level `,`/`}` of a previous arm. We detect a `=>`
    // at depth 1 and inspect the preceding pattern + following body.
    let mut depth = 0i32;
    let mut i = open;
    let mut arm_pat_start = open + 1;
    while i < close {
        match b[i] {
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => depth -= 1,
            b'=' if depth == 1 && i + 1 < close && b[i + 1] == b'>' => {
                let pat = src[arm_pat_start..i].trim().to_string();
                // Body: from after `=>` to the arm terminator at depth 1
                // (a top-level `,`, or the next arm — approximated by the next
                // depth-1 `,`; a block body ends at its `}` then a `,`).
                let body_start = i + 2;
                let (body, next) = read_arm_body(b, src, body_start, close);
                if let Some(binder) = catchall_binder(&pat) {
                    if let Some(reason) = silent_body(&body, binder.as_deref()) {
                        let line = src[..arm_pat_start].bytes().filter(|&c| c == b'\n').count() + 1;
                        out.push(Violation { line, arm: format!("{} => {}  [{}]", pat, body.trim(), reason) });
                    }
                }
                i = next;
                arm_pat_start = next;
                continue;
            }
            _ => {}
        }
        i += 1;
        if depth == 1 && (b[i.saturating_sub(1)] == b',' ) {
            arm_pat_start = i;
        }
    }
}

/// Read an arm body starting at `start`; returns (body_text, index_after_body).
fn read_arm_body(b: &[u8], src: &str, start: usize, close: usize) -> (String, usize) {
    let mut i = start;
    while i < close && (b[i] == b' ' || b[i] == b'\n' || b[i] == b'\t') { i += 1; }
    if i < close && b[i] == b'{' {
        // block body — consume to matching `}`
        let mut depth = 0i32;
        let bstart = i;
        while i < close {
            match b[i] {
                b'{' => depth += 1,
                b'}' => { depth -= 1; if depth == 0 { i += 1; break; } }
                _ => {}
            }
            i += 1;
        }
        let mut j = i;
        if j < close && b[j] == b',' { j += 1; }
        (src[bstart..i].to_string(), j)
    } else {
        // expression body — to next depth-0 `,`
        let estart = i;
        let mut depth = 0i32;
        while i < close {
            match b[i] {
                b'{' | b'(' | b'[' => depth += 1,
                b'}' | b')' | b']' => depth -= 1,
                b',' if depth == 0 => break,
                _ => {}
            }
            i += 1;
        }
        let body = src[estart..i].to_string();
        if i < close && b[i] == b',' { i += 1; }
        (body, i)
    }
}

fn is_ident_byte(c: u8) -> bool {
    c == b'_' || c.is_ascii_alphanumeric()
}

/// If `pat` is a catch-all (`_` or a single bare binder), return Some(binder
/// name or None for `_`). Variant patterns (`Foo`, `Foo(..)`, `A | B`) return None.
fn catchall_binder(pat: &str) -> Option<Option<String>> {
    let p = pat.trim();
    if p == "_" { return Some(None); }
    // bare lowercase identifier, no `::`, `(`, `{`, `|`, `@`, `.`
    if !p.is_empty()
        && p.chars().next().map_or(false, |c| c.is_ascii_lowercase() || c == '_')
        && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Some(Some(p.to_string()));
    }
    None
}

/// Returns Some(reason) if `body` is a silent recursion-drop.
fn silent_body(body: &str, binder: Option<&str>) -> Option<&'static str> {
    let t = body.trim().trim_end_matches(',').trim();
    if t == "{}" || t == "{ }" { return Some("empty"); }
    if let Some(name) = binder {
        if t == name { return Some("returns-binding-unrecursed"); }
    }
    // `_ => expr.kind` / `self.kind` (returns scrutinee unmodified)
    if t.ends_with(".kind")
        && t.split(['.', ' ']).next().map_or(false, |h| !h.is_empty())
        && !t.contains('(')
    {
        return Some("returns-scrutinee-kind");
    }
    None
}

#[test]
fn no_silent_catchalls_in_ir_traversal() {
    let root = repo_root();
    let files = governed_files(&root);
    assert!(!files.is_empty(), "no governed files found under {}", root.display());

    let mut offenders: BTreeMap<String, Vec<Violation>> = BTreeMap::new();
    let mut seen_legacy_clean: Vec<String> = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let src = std::fs::read_to_string(path).unwrap_or_default();
        let blanked = blank_comments_and_strings(&src);
        let v = find_violations(&blanked);
        let is_legacy = LEGACY_DEBT.contains(&rel.as_str());
        if !v.is_empty() && !is_legacy {
            offenders.insert(rel, v);
        } else if v.is_empty() && is_legacy {
            seen_legacy_clean.push(rel);
        }
    }

    let mut msg = String::new();
    if !offenders.is_empty() {
        msg.push_str(
            "\nSilent catch-all(s) found in IR-recursion match(es). A `_ => {}` / \
             `other => other` over `expr.kind` drops the children of un-handled node \
             kinds — the native↔WASM divergence class (DIV2).\n\
             Fix: delegate the default arm to the exhaustive primitive \
             (`walk_expr_mut(self, e)` / `e.map_children(..)`), or make it loud \
             (`unreachable!(..)` / `todo!()`), or list the variants explicitly.\n\
             See docs/roadmap/active/codegen-traversal-totality.md.\n\n",
        );
        for (file, vs) in &offenders {
            for v in vs {
                msg.push_str(&format!("  {}:{}  {}\n", file, v.line, v.arm));
            }
        }
    }
    if !seen_legacy_clean.is_empty() {
        msg.push_str(
            "\nThe following files are listed in LEGACY_DEBT but are now CLEAN — \
             remove them from the list so a regression is caught:\n",
        );
        for f in &seen_legacy_clean {
            msg.push_str(&format!("  {}\n", f));
        }
    }

    assert!(offenders.is_empty() && seen_legacy_clean.is_empty(), "{}", msg);
}
