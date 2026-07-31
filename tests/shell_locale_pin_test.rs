//! Every repo script that SORTS must pin the collation locale (#1031).
//!
//! `sort`'s last-resort comparison is the whole line, and that comparison follows the
//! ambient locale — punctuation is largely ignored under `en_US.UTF-8` and is byte order
//! under `C`. A generator that sorts without pinning therefore emits DIFFERENT output on
//! differently-configured machines.
//!
//! That is not hypothetical. `docs/roadmap/generate-readme.sh` produced 63 lines of pure
//! row-order churn in the committed `README.md` depending on where it ran, and the churn is
//! indistinguishable from a real update in review. It surfaced only because a second
//! implementation existed to diff against (Unit 0.46's `tools/almide-gates`).
//!
//! The practice already existed — `proofs/output-parity.sh`, `proofs/corpus-wall.sh`,
//! `proofs/coverage.sh` and `scripts/check-ratchet-separation.sh` pinned it — it just was not
//! uniform. This test makes the family total: a NEW sorting script that forgets the pin fails
//! here rather than at the next reviewer's confusing diff.

use std::fs;
use std::path::{Path, PathBuf};

/// Directories holding the repo's own tooling. `tools/` is excluded: those are vendored or
/// third-party harnesses, not scripts whose output the repo commits.
const TOOLING_DIRS: &[&str] = &["scripts", "proofs", "docs"];

fn shell_scripts(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if p.is_dir() {
            if name == "target" || name == "node_modules" {
                continue;
            }
            shell_scripts(&p, out);
        } else if name.ends_with(".sh") {
            out.push(p);
        }
    }
}

/// A line that actually invokes `sort`, not one that merely mentions it in prose.
fn invokes_sort(src: &str) -> bool {
    src.lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with('#')
        })
        .any(|l| {
            l.contains("| sort") || l.contains("|sort") || l.trim_start().starts_with("sort ")
        })
}

#[test]
fn every_sorting_script_pins_the_collation_locale() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for d in TOOLING_DIRS {
        shell_scripts(&root.join(d), &mut files);
    }
    assert!(
        !files.is_empty(),
        "found no shell scripts under {TOOLING_DIRS:?} — the walker is broken, not the repo"
    );

    let mut offenders = Vec::new();
    for f in &files {
        let Ok(src) = fs::read_to_string(f) else { continue };
        if invokes_sort(&src) && !src.contains("LC_ALL") {
            offenders.push(f.strip_prefix(&root).unwrap().to_string_lossy().to_string());
        }
    }

    assert!(
        offenders.is_empty(),
        "these scripts sort without pinning the collation locale, so their output depends on \
         the machine that ran them (#1031). Add `export LC_ALL=C` next to `set -euo pipefail`:\n  {}",
        offenders.join("\n  ")
    );
}
