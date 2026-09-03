//! `almide docs-gen` — canonical-docs consistency check.
//!
//! MVP scope: `--check` mode only. Verifies that `llms.txt` and the
//! `docs/diagnostics/` tree agree with the compiler's source of truth:
//!
//! 1. **Version**: `Cargo.toml`'s `package.version` appears in llms.txt.
//! 2. **Diagnostic codes referenced in llms.txt**: every `EXXX` under
//!    `docs/diagnostics/` is mentioned at least once in llms.txt.
//! 3. **Auto-imported stdlib list**: every module in
//!    `almide_lang::stdlib_info::AUTO_IMPORT_BUNDLED` appears in llms.txt.
//! 4. **Diagnostic registry bijection**: every `EXXX` emitted by the
//!    compiler (via `with_code("EXXX")` in source) has a matching
//!    `docs/diagnostics/EXXX.md`, and vice versa.
//!
//! Non-goals (deferred; the roadmap doc this used to cite was never written —
//! almide#1483 tracks the wider LLM-surface gate):
//! - Full regeneration of llms.txt from templates + sources.
//! - Parsing CHEATSHEET.md / DESIGN.md for idiom sections.
//! - CLI reference extraction from clap.

use std::path::{Path, PathBuf};
use crate::{err, out};

const LLMS_TXT: &str = "llms.txt";
const DIAGNOSTICS_DIR: &str = "docs/diagnostics";

pub fn cmd_docs_gen(check: bool) {
    if !check {
        err(&format!("error: `almide docs-gen` currently only supports `--check`."));
        err(&format!("       Full regeneration is deferred — almide#1483 tracks the LLM-surface gate."));
        std::process::exit(2);
    }

    let llms = match std::fs::read_to_string(LLMS_TXT) {
        Ok(s) => s,
        Err(e) => {
            err(&format!("error: cannot read `{}`: {}", LLMS_TXT, e));
            std::process::exit(2);
        }
    };

    let mut drifts: Vec<String> = Vec::new();

    drifts.extend(check_version(&llms));
    drifts.extend(check_diagnostic_codes(&llms));
    drifts.extend(check_auto_imported(&llms));
    drifts.extend(check_diagnostic_registry_bijection());
    drifts.extend(check_stdlib_fn_count());
    drifts.extend(check_numeric_conversion_matrix());

    if drifts.is_empty() {
        out(&format!("docs-gen: ok (version, llms.txt diagnostic refs, auto-imported, registry bijection, stdlib count, numeric matrix)"));
        return;
    }

    err(&format!("docs-gen: {} drift(s) detected in `{}`:", drifts.len(), LLMS_TXT));
    for d in &drifts {
        err(&format!("  - {}", d));
    }
    err(&format!("\nEdit `llms.txt` to match, or bump the source if the doc is the authority."));
    std::process::exit(1);
}

// ── Checks ──────────────────────────────────────────────────────────

fn check_version(llms: &str) -> Vec<String> {
    let version = env!("CARGO_PKG_VERSION");
    if !llms.contains(version) {
        vec![format!("Cargo.toml version `{}` not mentioned anywhere in llms.txt", version)]
    } else {
        vec![]
    }
}

fn check_diagnostic_codes(llms: &str) -> Vec<String> {
    let mut drifts = Vec::new();
    let codes = match collect_diagnostic_codes() {
        Ok(c) => c,
        Err(e) => {
            drifts.push(format!("cannot read {}: {}", DIAGNOSTICS_DIR, e));
            return drifts;
        }
    };
    for code in &codes {
        if !llms.contains(code) {
            drifts.push(format!("diagnostic code `{}` (file `{}/{}.md`) not referenced in llms.txt", code, DIAGNOSTICS_DIR, code));
        }
    }
    drifts
}

fn check_auto_imported(llms: &str) -> Vec<String> {
    // The Tier 1 (never-needs-import) set + bundled auto-imports.
    // If llms.txt lists a module as "needs explicit import" but source
    // of truth says otherwise (or vice versa), that's drift.
    let auto_bundled: Vec<&str> = almide_lang::stdlib_info::AUTO_IMPORT_BUNDLED
        .iter().copied().collect();
    let mut drifts = Vec::new();
    for m in &auto_bundled {
        if !llms.contains(m) {
            drifts.push(format!(
                "auto-imported stdlib module `{}` (from AUTO_IMPORT_BUNDLED) not mentioned in llms.txt",
                m
            ));
        }
    }
    drifts
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Bijection check: every diagnostic code emitted by the compiler
/// (`with_code("EXXX")` in source) must have a matching
/// `docs/diagnostics/EXXX.md`, and every doc file must correspond to
/// a code that is actually emitted. Catches E010/E011-class drift
/// (doc content describing a different code than what's emitted) and
/// E420-class drift (code emitted but never documented).
fn check_diagnostic_registry_bijection() -> Vec<String> {
    let mut drifts = Vec::new();
    let emitted = match collect_emitted_codes() {
        Ok(v) => v,
        Err(e) => { drifts.push(format!("cannot scan source for with_code(...): {}", e)); return drifts; }
    };
    let documented = match collect_diagnostic_codes() {
        Ok(v) => v,
        Err(e) => { drifts.push(format!("cannot read {}: {}", DIAGNOSTICS_DIR, e)); return drifts; }
    };
    let emitted_set: std::collections::HashSet<&String> = emitted.iter().collect();
    let documented_set: std::collections::HashSet<&String> = documented.iter().collect();

    for code in &emitted {
        if !documented_set.contains(code) {
            drifts.push(format!("code `{}` is emitted (via `with_code`) but has no `docs/diagnostics/{}.md`", code, code));
        }
    }
    for code in &documented {
        if !emitted_set.contains(code) {
            drifts.push(format!("doc `docs/diagnostics/{}.md` exists but the code is never emitted (`with_code(\"{}\")` not found in source)", code, code));
        }
    }
    drifts
}

/// Scan the `crates/` tree for `with_code("EXXX")` literals. We use a
/// simple substring match rather than a real parser because this is a
/// drift guard, not a refactoring tool — false negatives are unlikely
/// and false positives produce obvious error messages.
fn collect_emitted_codes() -> Result<Vec<String>, String> {
    let mut codes = std::collections::BTreeSet::new();
    let mut scan = |_path: &Path, contents: &str| {
        for mat in contents.match_indices("with_code(\"") {
            let start = mat.0 + "with_code(\"".len();
            let rest = &contents[start..];
            let end = rest.find('"').unwrap_or(0);
            if end == 0 { continue; }
            let code = &rest[..end];
            if code.len() > 1 && !code[1..].bytes().all(|b| b.is_ascii_digit()) {
                // Doc-comment placeholders (EXXX in this file's own docs).
                continue;
            }
            if looks_like_diag_code(code) {
                codes.insert(code.to_string());
            } else if !code.is_empty() {
                // Four-digit codes (E420 etc) are not canonical Almide
                // codes; flag them but include so the bijection check
                // can report them.
                codes.insert(code.to_string());
            }
        }
        // CLI-path diagnostics print the code as a literal
        // (`error[EXXX]:` — E081's build-time availability check); those
        // are emitted codes too.
        for mat in contents.match_indices("error[E") {
            let start = mat.0 + "error[".len();
            let rest = &contents[start..];
            let end = rest.find(']').unwrap_or(0);
            if end == 0 || end > 5 { continue; }
            let code = &rest[..end];
            if looks_like_diag_code(code)
                && code[1..].bytes().all(|b| b.is_ascii_digit())
            {
                codes.insert(code.to_string());
            }
        }
    };
    walk_rs_files(Path::new("crates"), &mut scan)?;
    walk_rs_files(Path::new("src"), &mut scan)?;
    Ok(codes.into_iter().collect())
}

fn walk_rs_files(
    dir: &Path,
    f: &mut impl FnMut(&Path, &str),
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("read_dir {}: {}", dir.display(), e))?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            // Skip target/ and similar build artifacts.
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name == "target" || name == ".git" { continue; }
            walk_rs_files(&p, f)?;
        } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
            if let Ok(text) = std::fs::read_to_string(&p) {
                f(&p, &text);
            }
        }
    }
    Ok(())
}

fn collect_diagnostic_codes() -> Result<Vec<String>, String> {
    let dir = Path::new(DIAGNOSTICS_DIR);
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("read_dir {}: {}", dir.display(), e))?;
    let mut codes: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let path: PathBuf = entry.path();
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if looks_like_diag_code(stem) {
            codes.push(stem.to_string());
        }
    }
    codes.sort();
    Ok(codes)
}

fn looks_like_diag_code(s: &str) -> bool {
    // E + 3 digits, e.g. E001..E999. The `docs/diagnostics/README.md`
    // and similar non-code files are filtered out by this shape check.
    s.len() == 4
        && s.starts_with('E')
        && s[1..].chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diag_code_shape() {
        assert!(looks_like_diag_code("E001"));
        assert!(looks_like_diag_code("E013"));
        assert!(looks_like_diag_code("E999"));
        assert!(!looks_like_diag_code("E0001"));       // 4-digit = rustc
        assert!(!looks_like_diag_code("README"));
        assert!(!looks_like_diag_code("E01"));
        assert!(!looks_like_diag_code(""));
    }
}

/// The "N functions across M modules" claim, DERIVED from the compiler's own
/// module interfaces (#918). The number had forked three ways — README said
/// 847, SPEC and WASM-OUTPUT said 834, the real surface was 869 — because it
/// was hand-written in five places and its old derivation (`stdlib/defs/*.toml`)
/// died with the self-hosting migration. This computes the truth the same way
/// `almide compile <module> --json` does — one interface extraction per module
/// documented under docs/stdlib/ — and asserts every claim site quotes exactly
/// that pair. Runs in-process, so the CI job that runs `docs-gen --check`
/// enforces it wherever the binary exists.
fn check_stdlib_fn_count() -> Vec<String> {
    let mut drifts = Vec::new();
    let mut modules: Vec<String> = match std::fs::read_dir("docs/stdlib") {
        Ok(rd) => rd
            .filter_map(|e| {
                let p = e.ok()?.path();
                (p.extension()? == "md").then(|| p.file_stem()?.to_str().map(str::to_string))?
            })
            .collect(),
        Err(e) => return vec![format!("cannot read docs/stdlib: {e}")],
    };
    modules.sort();
    // The clock-constructor pages (compute / duration) document CHECKER
    // surface, not stdlib module interfaces — recognized via the same table
    // the checker resolves them from (ADR-0001), so this carve-out cannot
    // drift into a hand list.
    modules.retain(|m| almide_lang::time_units::clock_type_of_module(m).is_none());
    let mut total = 0usize;
    for m in &modules {
        let (file, bundled_module) = super::compile::resolve_module_to_file(m);
        let (program, source_text, checker) =
            super::compile::parse_and_typecheck_for_compile(&file, bundled_module.as_deref());
        let ir = almide::lower::lower_program(&program, &checker.env, &checker.type_map);
        let iface = almide::interface::extract(&ir, m, Some(&source_text));
        // `__`-prefixed fns are INTERNAL carriers (the fallibility-polymorphic
        // `__fallible_*` bodies, ADR-0006 D3) — the public claim counts the public
        // surface only, matching the doc index generator's filter.
        total += iface.functions.iter().filter(|f| !f.name.starts_with("__")).count();
    }
    let want = format!("{} functions across {} modules", total, modules.len());
    for doc in ["README.md", "docs/SPEC.md", "docs/wasm/WASM-OUTPUT.md"] {
        let Ok(text) = std::fs::read_to_string(doc) else {
            drifts.push(format!("cannot read {doc}"));
            continue;
        };
        for line in text.lines() {
            if let Some(idx) = line.find(" functions across ") {
                // Reconstruct the claimed "<n> functions across <m> modules" span.
                let head = &line[..idx];
                let n_start = head.rfind(|c: char| !c.is_ascii_digit()).map_or(0, |i| i + 1);
                let tail = &line[idx + " functions across ".len()..];
                let m_end = tail.find(|c: char| !c.is_ascii_digit()).unwrap_or(tail.len());
                if !tail[..m_end].is_empty() && !tail[m_end..].trim_start().starts_with("modules") {
                    continue; // "functions across targets" or similar — not this claim
                }
                let claimed = format!(
                    "{} functions across {} modules",
                    &head[n_start..],
                    &tail[..m_end]
                );
                if claimed != want {
                    drifts.push(format!(
                        "{doc} claims `{claimed}` but the module interfaces total `{want}`"
                    ));
                }
            }
        }
    }
    drifts
}

/// The integer-conversion MATRIX gate (#956). The conversion family is defined
/// by a rule, not by enumeration-by-hand: over the carriers {Int, Int8..Int64,
/// UInt8..UInt64}, every ordered pair has the plain (truncating) form, and a
/// pair whose source range does not fit the destination — a LOSSY pair — must
/// also have `_checked` (Option on overflow) and `_saturating` (clamp), while a
/// lossless pair must NOT (an always-`Some` checked variant is noise). Every
/// carrier module also carries `min_value`/`max_value`. This computes the
/// expected surface from the range table and diffs it against the compiler's
/// own module interfaces in both directions, so the family can never again be
/// extended point-wise: a new carrier or a forgotten cell is a drift here.
fn check_numeric_conversion_matrix() -> Vec<String> {
    // (module, min, max) — i128 so u64::MAX fits.
    const CARRIERS: &[(&str, i128, i128)] = &[
        ("int8", i8::MIN as i128, i8::MAX as i128),
        ("int16", i16::MIN as i128, i16::MAX as i128),
        ("int32", i32::MIN as i128, i32::MAX as i128),
        ("int64", i64::MIN as i128, i64::MAX as i128),
        ("uint8", 0, u8::MAX as i128),
        ("uint16", 0, u16::MAX as i128),
        ("uint32", 0, u32::MAX as i128),
        ("uint64", 0, u64::MAX as i128),
    ];
    const INT: (&str, i128, i128) = ("int", i64::MIN as i128, i64::MAX as i128);
    let lossy = |s: &(&str, i128, i128), d: &(&str, i128, i128)| !(d.1 <= s.1 && s.2 <= d.2);

    let mut expect_present: Vec<(&str, String)> = Vec::new();
    let mut expect_absent: Vec<(&str, String)> = Vec::new();
    for s in CARRIERS {
        for d in CARRIERS.iter().filter(|d| d.0 != s.0) {
            expect_present.push((s.0, format!("to_{}", d.0)));
            let target = if lossy(s, d) { &mut expect_present } else { &mut expect_absent };
            target.push((s.0, format!("to_{}_checked", d.0)));
            target.push((s.0, format!("to_{}_saturating", d.0)));
        }
        // The Int column is spelled on the `int` module: plain `from_<s>`, and
        // the trio tail exactly for the one lossy widening (UInt64 → Int).
        expect_present.push(("int", format!("from_{}", s.0)));
        let target = if lossy(s, &INT) { &mut expect_present } else { &mut expect_absent };
        target.push(("int", format!("from_{}_checked", s.0)));
        target.push(("int", format!("from_{}_saturating", s.0)));
        // The Int row: plain `to_<d>` always; trio iff lossy (Int → Int64 is
        // the identity range, so its checked/saturating must not exist).
        expect_present.push(("int", format!("to_{}", s.0)));
        let target = if lossy(&INT, s) { &mut expect_present } else { &mut expect_absent };
        target.push(("int", format!("to_{}_checked", s.0)));
        target.push(("int", format!("to_{}_saturating", s.0)));
    }
    for m in CARRIERS.iter().map(|c| c.0).chain(["int"]) {
        expect_present.push((m, "min_value".to_string()));
        expect_present.push((m, "max_value".to_string()));
    }

    let mut drifts = Vec::new();
    let mut surface: std::collections::HashMap<&str, std::collections::HashSet<String>> =
        std::collections::HashMap::new();
    for m in CARRIERS.iter().map(|c| c.0).chain(["int"]) {
        let (file, bundled_module) = super::compile::resolve_module_to_file(m);
        let (program, source_text, checker) =
            super::compile::parse_and_typecheck_for_compile(&file, bundled_module.as_deref());
        let ir = almide::lower::lower_program(&program, &checker.env, &checker.type_map);
        let iface = almide::interface::extract(&ir, m, Some(&source_text));
        surface.insert(m, iface.functions.iter().map(|f| f.name.clone()).collect());
    }
    for (m, f) in &expect_present {
        if !surface[m].contains(f) {
            drifts.push(format!("numeric matrix: `{m}.{f}` is required (lossy pair or bounds) but missing"));
        }
    }
    for (m, f) in &expect_absent {
        if surface[m].contains(f) {
            drifts.push(format!("numeric matrix: `{m}.{f}` exists but the pair is lossless — a checked/saturating form there is noise; remove it or fix the range table"));
        }
    }
    drifts
}
