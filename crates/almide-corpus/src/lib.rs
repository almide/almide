//! Where the judge lives (ARCHITECTURE.md §4, "the judge is external").
//!
//! The language's normative text, contract ledger and conformance corpus are
//! the separate repository almide/als, mounted as the git submodule `als/` and
//! pinned by commit. Implementation-resident fixtures (`spec/churn`,
//! `spec/pass_isolated` — tied to this compiler's internals) stay in this
//! tree. Every gate addresses a fixture by its CORPUS-RELATIVE path
//! (`spec/lang/x.almd`): that is the name the oracle saw, the name embedded
//! in emitted diagnostics and ASTs, and the name the ledger cites — so the
//! mount is a pure location indirection, never a rename.

use std::path::{Path, PathBuf};

/// The submodule mount point of almide/als, relative to the workspace root.
pub const JUDGE_MOUNT: &str = "als";

/// The workspace root, from a member crate's `CARGO_MANIFEST_DIR`.
pub fn workspace_root(manifest_dir: &str) -> PathBuf {
    PathBuf::from(manifest_dir)
        .join("../..")
        .canonicalize()
        .expect("workspace root resolves")
}

/// Where a corpus-relative path lives: the implementation tree first, then
/// the judge mount (when this tree carries one — the main repo holds the
/// whole corpus locally and has no mount). A path in neither says which of
/// the two forms this tree is in.
pub fn resolve(root: &Path, rel: &str) -> PathBuf {
    let local = root.join(rel);
    if local.exists() {
        return local;
    }
    let mounted = root.join(JUDGE_MOUNT).join(rel);
    assert!(
        mounted.exists(),
        "{rel}: not in this tree{} — a corpus path every gate cites must exist",
        if root.join(JUDGE_MOUNT).is_dir() {
            format!(" and not under {JUDGE_MOUNT}/")
        } else {
            format!(" (no {JUDGE_MOUNT}/ mount here; the corpus is expected in-tree)")
        }
    );
    mounted
}

/// Every `.almd` under `spec/` in both roots, as (corpus-relative path,
/// location), sorted by path. The two roots are a PARTITION of the corpus: a
/// path present in both is a boundary violation and panics. A tree without
/// the judge mount (the main repo: the corpus lives in-tree) walks only its
/// local `spec/`.
pub fn walk_spec(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for base in [root.to_path_buf(), root.join(JUDGE_MOUNT)] {
        let spec = base.join("spec");
        if !spec.is_dir() && base.ends_with(JUDGE_MOUNT) {
            continue;
        }
        assert!(
            spec.is_dir(),
            "{}: missing (judge submodule not initialised? git submodule update --init)",
            spec.display()
        );
        let mut files = Vec::new();
        walk(&spec, &mut files);
        for abs in files {
            let rel = abs
                .strip_prefix(&base)
                .expect("walked under base")
                .to_string_lossy()
                .into_owned();
            out.push((rel, abs));
        }
    }
    out.sort();
    for pair in out.windows(2) {
        assert!(
            pair[0].0 != pair[1].0,
            "{}: present in both the implementation tree and {JUDGE_MOUNT}/ — the boundary is a partition",
            pair[0].0
        );
    }
    out
}

/// The data rows of a parity manifest (`spec-{ast,check,run}-manifest.txt`):
/// every line except the `# oracle: …` header the generators write (which
/// names the `almide --version` and git HEAD the rows were recorded from —
/// informational, never compared, since a rebase changes the SHA) and blanks.
///
/// Every reader of a manifest goes through here, whether it compares against
/// the recorded hash (run-parity, backend-parity, the WASI gate) or only uses
/// the manifest as the corpus list (exercised surface, allocation, size,
/// witness floor) — so the header cannot be mistaken for a row by any of them.
pub fn manifest_rows(text: &str) -> impl Iterator<Item = &str> {
    text.lines().filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
}

/// A shrink-only ceiling recorded under `proofs/` as `name<TAB>value` rows
/// (comment lines start with `#`). The test that enforces the ceiling reads
/// it here instead of carrying a `const`, so the number lives in a ratchet
/// artifact `scripts/check-ratchet-separation.sh` sees, in its own commit.
pub fn ratchet_ceiling(root: &Path, file: &str, name: &str) -> usize {
    let path = root.join(file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    manifest_rows(&text)
        .find_map(|l| {
            let (k, v) = l.split_once('\t').expect("name<TAB>value");
            (k == name).then(|| v.trim().parse::<usize>().unwrap_or_else(|e| panic!("{file}: {name}: {e}")))
        })
        .unwrap_or_else(|| panic!("{}: no `{name}` row", path.display()))
}

/// Fixtures whose ORACLE row in the run manifest predates a fix that postdates
/// the port SHA — `scripts/lib/run-oracle-stale.txt`, as `path -> reason`.
pub fn stale_oracle_rows(root: &Path) -> std::collections::BTreeMap<String, String> {
    let path = root.join("scripts/lib/run-oracle-stale.txt");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let (p, why) = l.split_once('\t').expect("path<TAB>reason");
            (p.to_string(), why.to_string())
        })
        .collect()
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("readable directory") {
        let p = entry.expect("directory entry").path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|e| e == "almd") {
            out.push(p);
        }
    }
}
