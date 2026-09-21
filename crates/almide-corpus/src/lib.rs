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

// ── Fixture-range sharding of the corpus giants (#2381) ──
//
// Six gates walk the whole `spec/wasm_cross` corpus (or the run manifest over
// it) in ONE test each, 18–28 minutes serially. `ALMIDE_CORPUS_SHARD=k/N`
// (1-based `k`) keeps every N-th fixture of the SORTED enumeration starting
// at `k` — a modulo partition, so ∪ over k = 1..N is the whole list and the
// slices are disjoint by construction. It is read AFTER the sort, and only
// there, so no gate can shard an unsorted or pre-filtered list.
//
// What a partial corpus does to each gate's verdict is the whole design:
//
//   - a per-fixture assertion (native == wasm, wasm == wasm-opt, interp ==
//     consensus, run-parity mismatch) is sound on any subset — judged in the
//     shard;
//   - a whole-corpus assertion (run_parity's shrink-only ceilings and its
//     `rows > 550` floor; the bridge-fallback ledger's NAME-keyed set) is
//     NOT: judging a partial count against the whole ceiling passes N times
//     over N fractions of the count — a silent loosening. Those gates write a
//     partial under `ALMIDE_CORPUS_SHARD_DIR` and judge ONLY the sound half in
//     the shard; `ALMIDE_CORPUS_SHARD=merge/N` in the aggregation job reads the
//     N partials, sums / unions them and judges the whole — with the same
//     code and the same failure text as the unsharded gate;
//   - the abstain ledger is fixture-keyed both ways: `stale` is judged
//     against the ledger ∩ this shard's fixtures (a ledgered fixture in
//     another shard is not "no longer abstaining").
//
// Every sharded gate also writes the fixture list it actually walked; the
// aggregation job asserts the union of the lists equals `ls spec/wasm_cross`
// (and the manifest, for run_parity) with no duplicates — a partition that
// drops a fixture goes RED, never green-and-faster (the doctrine of
// `scripts/ci-test-shard.sh`). Unset ⇒ every gate behaves exactly as before.

/// The parsed `ALMIDE_CORPUS_SHARD` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorpusShard {
    /// `k/N`: walk the k-th of N modulo slices of the sorted corpus.
    Slice { k: usize, n: usize },
    /// `merge/N`: walk nothing; read the N partials and judge the whole.
    Merge { n: usize },
}

/// `ALMIDE_CORPUS_SHARD`, parsed; `None` when unset (the unsharded gate).
/// A malformed value panics — a shard spelling that silently meant "all"
/// would be the failure this exists to prevent.
pub fn corpus_shard() -> Option<CorpusShard> {
    let raw = almide_base::env::var("ALMIDE_CORPUS_SHARD")?;
    Some(parse_shard(raw.trim()).unwrap_or_else(|| {
        panic!("ALMIDE_CORPUS_SHARD={raw:?}: expected `k/N` (1 <= k <= N) or `merge/N` (N >= 1)")
    }))
}

fn parse_shard(s: &str) -> Option<CorpusShard> {
    let (k, n) = s.split_once('/')?;
    let n: usize = n.trim().parse().ok().filter(|n| *n >= 1)?;
    if k.trim() == "merge" {
        return Some(CorpusShard::Merge { n });
    }
    let k: usize = k.trim().parse().ok().filter(|k| (1..=n).contains(k))?;
    Some(CorpusShard::Slice { k, n })
}

impl CorpusShard {
    /// Keep this slice of an ALREADY SORTED list (index `i` stays when
    /// `i % N == k - 1`). Panics on an empty result: a shard that walks no
    /// fixture must be red, not a self-skip. `Merge` keeps nothing — a gate
    /// with no merge form must refuse it (`require_slice`).
    pub fn apply<T>(self, sorted: Vec<T>) -> Vec<T> {
        match self {
            CorpusShard::Slice { k, n } => {
                let total = sorted.len();
                let kept: Vec<T> = sorted
                    .into_iter()
                    .enumerate()
                    .filter(|(i, _)| i % n == k - 1)
                    .map(|(_, t)| t)
                    .collect();
                assert!(
                    !kept.is_empty(),
                    "ALMIDE_CORPUS_SHARD={k}/{n}: slice is empty over {total} fixture(s) — a shard that walks nothing is not a pass"
                );
                kept
            }
            CorpusShard::Merge { .. } => Vec::new(),
        }
    }

    /// The `(k, n)` of a slice; a gate whose every assertion is per fixture
    /// has no merge form and refuses `merge/N` here rather than walking an
    /// empty list.
    pub fn require_slice(self, gate: &str) -> (usize, usize) {
        match self {
            CorpusShard::Slice { k, n } => (k, n),
            CorpusShard::Merge { n } => panic!(
                "ALMIDE_CORPUS_SHARD=merge/{n}: {gate} has no merge form — its assertions are per fixture and were judged in the shards; the coverage step unions its fixture lists"
            ),
        }
    }

    /// `k-of-N` / `merge-of-N`, the file-name suffix of a partial.
    fn suffix(self) -> String {
        match self {
            CorpusShard::Slice { k, n } => format!("{k}-of-{n}"),
            CorpusShard::Merge { n } => format!("merge-of-{n}"),
        }
    }
}

/// `ALMIDE_CORPUS_SHARD_DIR`: where a sharded gate leaves its partials and
/// where `merge/N` reads them. Unset ⇒ nothing is written (a developer's
/// local slice) — and `merge/N` panics, since it has nothing to read.
pub fn corpus_shard_dir() -> Option<PathBuf> {
    almide_base::env::var("ALMIDE_CORPUS_SHARD_DIR").map(PathBuf::from)
}

fn partial_path(dir: &Path, gate: &str, kind: &str, suffix: &str) -> PathBuf {
    dir.join(format!("{gate}.{kind}.{suffix}.txt"))
}

/// Write a partial (`<dir>/<gate>.<kind>.<k>-of-<N>.txt`, one line per
/// entry) when a shard dir is set; a no-op otherwise. `kind` is
/// `fixtures` (the walked list — every sharded gate writes it), `counts`
/// (run_parity) or `bridge` (the fallback ledger).
pub fn write_partial(shard: CorpusShard, gate: &str, kind: &str, lines: &[String]) {
    let Some(dir) = corpus_shard_dir() else { return };
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    let path = partial_path(&dir, gate, kind, &shard.suffix());
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    std::fs::write(&path, text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    eprintln!("{gate}: wrote {} line(s) to {}", lines.len(), path.display());
}

/// Read the N partials of `kind` for `gate` (`1-of-N` … `N-of-N`), in shard
/// order, each as its lines. A missing partial panics: a merge over fewer
/// than N shards would judge a fraction of the corpus as the whole.
pub fn read_partials(n: usize, gate: &str, kind: &str) -> Vec<Vec<String>> {
    let dir = corpus_shard_dir().unwrap_or_else(|| {
        panic!("ALMIDE_CORPUS_SHARD=merge/{n} needs ALMIDE_CORPUS_SHARD_DIR (where the {n} shards left their partials)")
    });
    (1..=n)
        .map(|k| {
            let path = partial_path(&dir, gate, kind, &format!("{k}-of-{n}"));
            let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                panic!(
                    "{}: {e} — shard {k}/{n} of {gate} left no `{kind}` partial; a merge over an incomplete set of shards is not a verdict",
                    path.display()
                )
            });
            text.lines().map(str::to_string).collect()
        })
        .collect()
}

/// The union of N fixture-list partials, checked to be a PARTITION of
/// `expected` (sorted, no duplicates, nothing missing, nothing extra). The
/// in-process twin of the workflow's coverage step, so a merge verdict never
/// rests on a coverage assertion that ran somewhere else.
pub fn assert_partials_cover(n: usize, gate: &str, expected: &[String]) {
    let mut seen = std::collections::BTreeMap::<String, usize>::new();
    for lines in read_partials(n, gate, "fixtures") {
        for l in lines {
            *seen.entry(l).or_default() += 1;
        }
    }
    let dupes: Vec<&String> = seen.iter().filter(|(_, c)| **c > 1).map(|(f, _)| f).collect();
    assert!(dupes.is_empty(), "{gate}: {} fixture(s) walked by more than one shard: {dupes:?}", dupes.len());
    let want: std::collections::BTreeSet<&String> = expected.iter().collect();
    let got: std::collections::BTreeSet<&String> = seen.keys().collect();
    let missing: Vec<&&String> = want.difference(&got).collect();
    let extra: Vec<&&String> = got.difference(&want).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "{gate}: the {n} shards' union is not the corpus — {} fixture(s) NEVER WALKED {missing:?}, {} not in the corpus {extra:?}",
        missing.len(),
        extra.len()
    );
}

#[cfg(test)]
mod shard_tests {
    use super::*;

    #[test]
    fn slices_partition_the_sorted_list() {
        let all: Vec<usize> = (0..7).collect();
        let a = CorpusShard::Slice { k: 1, n: 2 }.apply(all.clone());
        let b = CorpusShard::Slice { k: 2, n: 2 }.apply(all.clone());
        assert_eq!(a, vec![0, 2, 4, 6]);
        assert_eq!(b, vec![1, 3, 5]);
        let mut u = a;
        u.extend(b);
        u.sort();
        assert_eq!(u, all);
    }

    #[test]
    fn spellings() {
        assert_eq!(parse_shard("1/2"), Some(CorpusShard::Slice { k: 1, n: 2 }));
        assert_eq!(parse_shard("merge/3"), Some(CorpusShard::Merge { n: 3 }));
        for bad in ["0/2", "3/2", "2", "a/2", "1/0", "merge/0", ""] {
            assert_eq!(parse_shard(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn an_empty_slice_is_red() {
        assert!(std::panic::catch_unwind(|| CorpusShard::Slice { k: 2, n: 2 }.apply(vec![1])).is_err());
    }
}
