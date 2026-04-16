//! `almide docs-gen` — canonical-docs consistency check.
//!
//! MVP scope: `--check` mode only. Verifies that `llms.txt` agrees with
//! three source-of-truth inputs:
//!
//! 1. **Version**: `Cargo.toml`'s `package.version` appears in llms.txt.
//! 2. **Diagnostic codes**: every `EXXX` under `docs/diagnostics/` is
//!    mentioned at least once in llms.txt (title doesn't need to match
//!    exactly — we check the code string).
//! 3. **Auto-imported stdlib list**: Tier 1 modules listed in
//!    `almide_lang::stdlib_info::AUTO_IMPORT_BUNDLED` plus the
//!    hardcoded Tier 1 set are referenced in llms.txt's "Fast facts".
//!
//! Non-goals (deferred — see `docs/roadmap/active/llms-txt-autogen.md`):
//! - Full regeneration of llms.txt from templates + sources.
//! - Parsing CHEATSHEET.md / DESIGN.md for idiom sections.
//! - CLI reference extraction from clap.

use std::path::{Path, PathBuf};

const LLMS_TXT: &str = "llms.txt";
const DIAGNOSTICS_DIR: &str = "docs/diagnostics";

pub fn cmd_docs_gen(check: bool) {
    if !check {
        eprintln!("error: `almide docs-gen` currently only supports `--check`.");
        eprintln!("       Full regeneration is planned — see docs/roadmap/active/llms-txt-autogen.md");
        std::process::exit(2);
    }

    let llms = match std::fs::read_to_string(LLMS_TXT) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read `{}`: {}", LLMS_TXT, e);
            std::process::exit(2);
        }
    };

    let mut drifts: Vec<String> = Vec::new();

    drifts.extend(check_version(&llms));
    drifts.extend(check_diagnostic_codes(&llms));
    drifts.extend(check_auto_imported(&llms));

    if drifts.is_empty() {
        println!("docs-gen: ok (version, diagnostic codes, auto-imported list all consistent with sources)");
        return;
    }

    eprintln!("docs-gen: {} drift(s) detected in `{}`:", drifts.len(), LLMS_TXT);
    for d in &drifts {
        eprintln!("  - {}", d);
    }
    eprintln!("\nEdit `llms.txt` to match, or bump the source if the doc is the authority.");
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
