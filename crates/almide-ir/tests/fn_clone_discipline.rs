//! The flag-preservation discipline for `IrFunction` construction —
//! the mechanized form of the c3 lesson (spec/gauntlet, 2026-08-26):
//! the mono specialization clone wrote `mutated_params: vec![]`, so the
//! move-mode rewrite AND the C-132 wall both missed the instance — a
//! silent wrong value that no behavior gate could see until a program
//! hit it.
//!
//! The invariant: `mutated_params` carries SEMANTICS (which params
//! write back to the caller). A site that assigns it EMPTY is either a
//! genuinely fresh synthesized function — and must say so with a
//! same-line `// fresh-fn: <why>` marker — or it is a clone dropping a
//! flag, which is exactly the bug class. Clones propagate
//! (`orig.mutated_params.clone()` or struct-update `..`); those pass
//! without a marker. Omission is unrepresentable: the struct has no
//! Default, so every literal names the field or spreads an original.

use std::path::{Path, PathBuf};

/// Fields whose EMPTY assignment demands the fresh-fn marker. Grows if
/// another semantic flag joins IrFunction.
const GUARDED_EMPTIES: &[&str] = &["mutated_params: vec![]", "mutated_params: Vec::new()"];

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).expect("readable dir").flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            rust_sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn empty_mutated_params_is_declared_fresh() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for e in std::fs::read_dir(&crates).expect("crates/").flatten() {
        let src = e.path().join("src");
        if src.is_dir() {
            rust_sources(&src, &mut files);
        }
    }
    assert!(files.len() > 50, "source sweep looks broken: {} files", files.len());
    let mut offences = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).expect("readable source");
        for (i, line) in text.lines().enumerate() {
            if GUARDED_EMPTIES.iter().any(|g| line.contains(g)) && !line.contains("fresh-fn:") {
                offences.push(format!("{}:{}: {}", f.display(), i + 1, line.trim()));
            }
        }
    }
    assert!(
        offences.is_empty(),
        "empty `mutated_params` without a `// fresh-fn: <why>` marker — a clone that drops \
         this flag silently loses mut-param writeback (the c3 class). Either propagate the \
         original's flags, or state why the function is genuinely fresh:\n{}",
        offences.join("\n")
    );
}
