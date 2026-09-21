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

/// What the `# oracle:` header of a parity manifest records: the
/// `almide --version` line of the binary that produced the rows, split into
/// the parts the tests can check, and the git HEAD the generator ran at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleHeader {
    /// `0.63.0` — the `[package]` version the binary was built with.
    pub version: String,
    /// `dev` or `release` (#2384's provenance stamp).
    pub kind: String,
    /// The commit `make install` stamped into the binary, when it did.
    pub build_sha: Option<String>,
    /// The generator's HEAD — informational: a rebase changes it.
    pub head: String,
}

/// Parse the first line of a manifest as its `# oracle:` header:
/// `# oracle: almide <version> (<kind>[, <sha>]) at <head> — …`.
pub fn oracle_header(text: &str) -> Result<OracleHeader, String> {
    let line = text.lines().next().unwrap_or("");
    let rest = line
        .strip_prefix("# oracle: almide ")
        .ok_or_else(|| format!("no `# oracle: almide …` header on the first line (got: {line:?})"))?;
    let (version, rest) = rest.split_once(' ').ok_or("header ends after the version")?;
    let (paren, rest) = rest
        .strip_prefix('(')
        .and_then(|r| r.split_once(')'))
        .ok_or("no `(kind[, sha])` after the version")?;
    let (kind, build_sha) = match paren.split_once(", ") {
        Some((k, s)) => (k.to_string(), Some(s.to_string())),
        None => (paren.to_string(), None),
    };
    let head = rest
        .trim_start()
        .strip_prefix("at ")
        .and_then(|r| r.split_whitespace().next())
        .ok_or("no `at <head>` after the build kind")?;
    Ok(OracleHeader { version: version.to_string(), kind, build_sha, head: head.to_string() })
}

/// The `[package]` version of the root `Cargo.toml` — what `almide --version`
/// prints. `[workspace.package]` comes first in that file with a different
/// number, so the first `version =` is the wrong one (f84bb6aae).
pub fn tree_version(root: &Path) -> String {
    let text = std::fs::read_to_string(root.join("Cargo.toml")).expect("root Cargo.toml");
    let mut in_package = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[package]";
            continue;
        }
        if in_package && let Some(v) = t.strip_prefix("version") {
            let v = v.trim_start().strip_prefix('=').unwrap_or("").trim().trim_matches('"');
            return v.to_string();
        }
    }
    panic!("root Cargo.toml: no `version =` under [package]")
}

/// #2405: the header used to be the one field nothing read. The parity tests
/// now refuse a manifest whose rows were recorded by a binary that is not the
/// CLI built from this tree: the version must be this tree's `Cargo.toml`
/// version and the kind `dev` (a `release` binary comes from the release
/// workflow, never from a checkout). The head SHA stays informational — a
/// rebase changes it — so it is parsed, not compared. What the tests cannot
/// see (a stale but same-version build) the generators refuse at write time
/// (`scripts/lib/oracle-header.sh`, `refuse_stale_tree`).
pub fn verify_oracle_header(root: &Path, text: &str) -> Result<OracleHeader, String> {
    let h = oracle_header(text)?;
    let want = tree_version(root);
    if h.version != want {
        return Err(format!(
            "recorded by almide {} ({}) but this tree is version {want} — the rows come from a released or other-tree binary; regenerate with the CLI built from this tree (make install; ORACLE=target/release/almide)",
            h.version, h.kind
        ));
    }
    if h.kind != "dev" {
        return Err(format!(
            "recorded by a `{}` binary (almide {} {}) — a release build comes from the release workflow, never from this tree; regenerate with the CLI built here",
            h.kind, h.version, h.kind
        ));
    }
    Ok(h)
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

#[cfg(test)]
mod oracle_header_tests {
    use super::*;

    const GOOD: &str = "# oracle: almide 0.63.0 (dev) at e8c95e285 — the CLI built from this tree; informational\nabc\t0\tspec/x.almd\n";

    #[test]
    fn parses_the_generators_line() {
        let h = oracle_header(GOOD).unwrap();
        assert_eq!(h.version, "0.63.0");
        assert_eq!(h.kind, "dev");
        assert_eq!(h.build_sha, None);
        assert_eq!(h.head, "e8c95e285");
        let stamped = oracle_header("# oracle: almide 0.63.0 (dev, ac10929aa) at ac10929aa — x\n").unwrap();
        assert_eq!(stamped.build_sha.as_deref(), Some("ac10929aa"));
    }

    #[test]
    fn a_missing_or_malformed_header_is_an_error_not_a_row() {
        assert!(oracle_header("abc\t0\tspec/x.almd\n").is_err());
        assert!(oracle_header("# oracle: almide 0.63.0\n").is_err());
        assert!(oracle_header("").is_err());
    }

    #[test]
    fn a_forged_released_or_wrong_version_header_is_refused_against_this_tree() {
        let root = workspace_root(env!("CARGO_MANIFEST_DIR"));
        let want = tree_version(&root);
        assert!(!want.is_empty() && want != "0.12.2", "read the [package] version, not [workspace.package]: {want}");
        let ok = format!("# oracle: almide {want} (dev) at 000000000 — x\n");
        assert!(verify_oracle_header(&root, &ok).is_ok());
        let stamped = format!("# oracle: almide {want} (dev, 000000000) at 000000000 — x\n");
        assert!(verify_oracle_header(&root, &stamped).is_ok());
        let released = format!("# oracle: almide {want} (release, 000000000) at 000000000 — x\n");
        let e = verify_oracle_header(&root, &released).unwrap_err();
        assert!(e.contains("release build comes from the release workflow"), "{e}");
        let other = "# oracle: almide 0.62.0 (dev) at 000000000 — x\n";
        let e = verify_oracle_header(&root, other).unwrap_err();
        assert!(e.contains("recorded by almide 0.62.0 (dev)") && e.contains(&format!("this tree is version {want}")), "{e}");
    }
}
