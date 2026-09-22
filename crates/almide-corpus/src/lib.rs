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
    /// Keep this slice of an ALREADY SORTED list, balanced by the recorded
    /// weight of each fixture (#2457): `stem_of` names the fixture (the key
    /// of `proofs/corpus-weights.txt`), `gate` picks the weight column
    /// (`weight_column`). The partition is [`partition_by_weight`] — LPT over
    /// the sorted list, so it is deterministic, every fixture lands in exactly
    /// one slice, and a gate with no weight column degrades to the modulo
    /// slice (uniform weights ⇒ `i % N == k - 1`). Panics on an empty result:
    /// a shard that walks no fixture must be red, not a self-skip. `Merge`
    /// keeps nothing — a gate with no merge form must refuse it
    /// (`require_slice`).
    pub fn apply<T>(self, sorted: Vec<T>, gate: &str, stem_of: impl Fn(&T) -> String) -> Vec<T> {
        match self {
            CorpusShard::Slice { k, n } => {
                let total = sorted.len();
                let weights = corpus_weights(gate);
                let ws: Vec<u64> = sorted.iter().map(|t| weights.weight_of(&stem_of(t))).collect();
                let owner = partition_by_weight(&ws, n);
                let loads: Vec<u64> = (0..n).map(|s| ws.iter().zip(&owner).filter(|(_, o)| **o == s).map(|(w, _)| *w).sum()).collect();
                let kept: Vec<T> = sorted
                    .into_iter()
                    .zip(&owner)
                    .filter(|(_, o)| **o == k - 1)
                    .map(|(t, _)| t)
                    .collect();
                assert!(
                    !kept.is_empty(),
                    "ALMIDE_CORPUS_SHARD={k}/{n}: slice is empty over {total} fixture(s) — a shard that walks nothing is not a pass"
                );
                eprintln!(
                    "{gate}: corpus shard {k}/{n} keeps {} of {total} fixture(s), predicted weight {} of {:?} ({})",
                    kept.len(),
                    loads[k - 1],
                    loads,
                    weights.describe()
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

// ── Balanced slices by measured weight (#2457) ──
//
// The modulo slice halves a gate's COST, not its WALL: the per-fixture cost
// of the interpreter legs spans four orders of magnitude (a handful of
// fixtures run seconds to minutes, the rest milliseconds), so whichever
// residue class the heavy ones fall in sets the wall (run_parity 6 vs 16 min
// at N=2 on develop run 35645124173). `proofs/corpus-weights.txt` records the
// measured wall of every fixture per cost column; a slice is the LPT
// partition over those weights — sorted heaviest first, each fixture to the
// least-loaded shard — which is deterministic and, for uniform weights,
// exactly the modulo slice. A fixture with no row gets the column's MEDIAN,
// so a stale table cannot exclude a new fixture (it lands somewhere, and the
// coverage step still proves the union). The table is regenerated by
// `scripts/gen-corpus-weights.sh`: every gate records what it measured under
// `ALMIDE_CORPUS_WEIGHTS_DIR`, and the script renders the columns.
//
// THE COLUMN IS A CI MEASUREMENT, NOT A LAPTOP'S (#2502). The partition only
// uses the RATIOS between fixtures, and those ratios are a property of the
// machine, not of the program: the first table was measured on a Mac, where
// run_parity's halves came out 93 / 93 s, and the same table split the
// ubuntu-latest runner 479 / 1096 s — because the heavy fixtures there are
// allocation-bound on 7 GB while the Mac's are CPU-bound (#2387). The
// committed table is therefore rendered from a CI run's own recordings
// (`gen-corpus-weights.sh --from-ci`), its `# measured-on:` header says which
// run, and `--check` (weekly, .github/workflows/shard-balance.yml) fails when
// the legs it balances drift apart again. A local measurement is for
// experiments; committing one puts the 2× back.

/// The committed weight table, relative to the workspace root.
pub const WEIGHTS: &str = "proofs/corpus-weights.txt";

/// Which weight column a gate's slice balances on: the interpreter legs on
/// the interp sweep, run_parity on its own serial walk, the build-only gates
/// (cross_target, opt_parity) on the subprocess builds. A gate outside this
/// map balances uniformly (the modulo slice).
///
/// What balancing a column buys differs by gate, and the weekly ratchet says
/// so (#2502): `run_parity` and the two build gates walk their fixtures
/// SERIALLY, so an even column split is an even WALL split. The interp sweep
/// runs on a thread pool, where the wall is the makespan — floored by the
/// single heaviest fixture (`string_position_large_offset`, 492 s of the
/// leg's 1657 s on the runner). No partition can lower that floor, so the
/// interp legs' halves stay ≈1.8× apart however well the column is balanced;
/// re-slicing is not the lever there, making that fixture cheaper is.
pub fn weight_column(gate: &str) -> Option<&'static str> {
    match gate {
        "run_parity" => Some("run_parity"),
        "interp_ledger" | "wasm_runtime_interp_oracle" => Some("interp"),
        "wasm_runtime_cross_target" | "wasm_runtime_opt_parity" => Some("build"),
        _ => None,
    }
}

/// One column of the weight table, resolved for a gate.
pub struct CorpusWeights {
    column: Option<String>,
    rows: std::collections::BTreeMap<String, u64>,
    median: u64,
}

impl CorpusWeights {
    /// Uniform weights (every fixture 1) — the modulo slice.
    pub fn uniform() -> Self {
        CorpusWeights { column: None, rows: Default::default(), median: 1 }
    }

    /// Parse the table text for `column`; `None` when the column is not in
    /// the header (uniform). Rows are `stem<TAB>v1<TAB>v2…` under a
    /// `# columns:<TAB>stem<TAB>name…` line; an empty cell is "not measured".
    pub fn parse(text: &str, column: &str) -> Option<Self> {
        let header = text
            .lines()
            .find_map(|l| l.strip_prefix("# columns:"))
            .expect("corpus-weights: no `# columns:` header line");
        let names: Vec<&str> = header.split('\t').map(str::trim).filter(|s| !s.is_empty()).collect();
        assert_eq!(names.first().copied(), Some("stem"), "corpus-weights: the first column is `stem`");
        let idx = names.iter().position(|c| *c == column)?;
        let mut rows = std::collections::BTreeMap::new();
        for l in manifest_rows(text) {
            let cells: Vec<&str> = l.split('\t').collect();
            let stem = cells[0].trim();
            let Some(cell) = cells.get(idx).map(|c| c.trim()).filter(|c| !c.is_empty()) else { continue };
            let ms: u64 = cell.parse().unwrap_or_else(|e| panic!("corpus-weights: {stem} {column}={cell:?}: {e}"));
            rows.insert(stem.to_string(), ms);
        }
        let mut sorted: Vec<u64> = rows.values().copied().collect();
        sorted.sort_unstable();
        let median = sorted.get(sorted.len() / 2).copied().unwrap_or(1).max(1);
        Some(CorpusWeights { column: Some(column.to_string()), rows, median })
    }

    /// The recorded weight (never 0), or the median for an unrecorded stem.
    pub fn weight_of(&self, stem: &str) -> u64 {
        self.rows.get(stem).copied().unwrap_or(self.median).max(1)
    }

    fn describe(&self) -> String {
        match &self.column {
            Some(c) => format!("column `{c}` of {WEIGHTS}, {} row(s), median {} ms for an unrecorded stem", self.rows.len(), self.median),
            None => "uniform weights: the modulo slice".to_string(),
        }
    }
}

/// The weights `gate` balances on, read from the committed table. A gate
/// with no column is uniform; a missing table is an error (it is committed;
/// a slice that silently fell back to the modulo split would reintroduce the
/// wall this exists to remove).
pub fn corpus_weights(gate: &str) -> CorpusWeights {
    let Some(column) = weight_column(gate) else { return CorpusWeights::uniform() };
    let path = workspace_root(env!("CARGO_MANIFEST_DIR")).join(WEIGHTS);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e} (bash scripts/gen-corpus-weights.sh)", path.display()));
    CorpusWeights::parse(&text, column).unwrap_or_else(CorpusWeights::uniform)
}

/// LPT: the owner shard (0-based) of each of `weights`, in input order.
/// Items are taken heaviest first (ties in input order) and each goes to
/// the least-loaded shard (ties to the lowest index). Uniform weights give
/// `i % n` — the modulo slice — exactly.
pub fn partition_by_weight(weights: &[u64], n: usize) -> Vec<usize> {
    assert!(n >= 1);
    let mut order: Vec<usize> = (0..weights.len()).collect();
    order.sort_by(|&a, &b| weights[b].cmp(&weights[a]).then(a.cmp(&b)));
    let mut load = vec![0u64; n];
    let mut owner = vec![0usize; weights.len()];
    for i in order {
        let s = (0..n).min_by_key(|&s| (load[s], s)).expect("n >= 1");
        load[s] += weights[i];
        owner[i] = s;
    }
    owner
}

/// Where a gate's measured walls land under `ALMIDE_CORPUS_WEIGHTS_DIR`:
/// `<dir>/weights/<column>.<gate>[.<k>-of-<N>].txt`.
///
/// The `weights/` subdirectory is not decoration. CI points this switch at the
/// SAME directory as `ALMIDE_CORPUS_SHARD_DIR`, so one artifact carries both
/// families — and a flat layout makes them collide by SPELLING: the
/// `run_parity` gate's `run_parity` column writes `run_parity.<shard>.txt`
/// while its partials are `run_parity.fixtures.<shard>.txt` and
/// `run_parity.counts.<shard>.txt`, which a `run_parity.*.txt` glob happily
/// folds into the table (`identical 355`, `rows 367` and 735 fixture PATHS as
/// "stems" with weights). A directory separates the two families structurally,
/// so no future partial `kind` can be read as a weight.
pub fn weights_file(dir: &Path, column: &str, gate: &str, suffix: &str) -> PathBuf {
    dir.join("weights").join(format!("{column}.{gate}{suffix}.txt"))
}

/// `ALMIDE_CORPUS_WEIGHTS_DIR`: where a gate records the wall it measured per
/// fixture ([`weights_file`], `stem<TAB>ms`), for
/// `scripts/gen-corpus-weights.sh --render` to fold into
/// `proofs/corpus-weights.txt`. Unset ⇒ nothing is written. A sharded run
/// records what it walked under its shard suffix — CI sets the dir to the
/// shard-partials dir, so every solo job's artifact carries its measured
/// walls and the table IS rendered from a CI run (the runner whose ratios the
/// partition is for, #2502); the local generator runs the gates unsharded so
/// one file per column holds every stem.
pub fn record_weights(column: &str, gate: &str, rows: &[(String, std::time::Duration)]) {
    let Some(dir) = almide_base::env::var("ALMIDE_CORPUS_WEIGHTS_DIR").map(PathBuf::from) else { return };
    let suffix = corpus_shard().map(|s| format!(".{}", s.suffix())).unwrap_or_default();
    let path = weights_file(&dir, column, gate, &suffix);
    std::fs::create_dir_all(path.parent().expect("weights_file has a parent")).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    let mut text = String::new();
    for (stem, d) in rows {
        text.push_str(&format!("{stem}\t{}\n", d.as_millis()));
    }
    std::fs::write(&path, text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let total: std::time::Duration = rows.iter().map(|(_, d)| *d).sum();
    eprintln!("corpus weights: wrote {} `{column}` row(s) ({} ms summed) to {}", rows.len(), total.as_millis(), path.display());
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
        let a = CorpusShard::Slice { k: 1, n: 2 }.apply(all.clone(), "unweighted-gate", |i| i.to_string());
        let b = CorpusShard::Slice { k: 2, n: 2 }.apply(all.clone(), "unweighted-gate", |i| i.to_string());
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
    fn lpt_over_uniform_weights_is_the_modulo_slice() {
        let owner = partition_by_weight(&[1; 11], 3);
        assert_eq!(owner, (0..11).map(|i| i % 3).collect::<Vec<_>>());
    }

    #[test]
    fn lpt_balances_the_heavy_tail_and_keeps_every_item() {
        // One giant, one mid, eight tiny: the modulo split would put the
        // giant and the mid on the same residue.
        let w = [1000, 1, 300, 1, 1, 1, 1, 1, 1, 1];
        let owner = partition_by_weight(&w, 2);
        assert_eq!(owner.len(), w.len());
        let load = |s: usize| w.iter().zip(&owner).filter(|(_, o)| **o == s).map(|(w, _)| *w).sum::<u64>();
        assert_eq!(load(0), 1000, "the giant alone");
        assert_eq!(load(1), 300 + 8, "everything else");
        assert!(owner.iter().all(|o| *o < 2));
    }

    #[test]
    fn an_unrecorded_stem_gets_the_median_and_a_missing_column_is_uniform() {
        let text = "# corpus-weights\n# columns:\tstem\trun_parity\tinterp\nalpha\t10\t5\nbeta\t20\t\ngamma\t30\t7\n";
        let w = CorpusWeights::parse(text, "run_parity").expect("column present");
        assert_eq!(w.weight_of("alpha"), 10);
        assert_eq!(w.weight_of("never-measured"), 20, "median of 10/20/30");
        let i = CorpusWeights::parse(text, "interp").expect("column present");
        assert_eq!(i.weight_of("beta"), 7, "empty cell ⇒ median of 5/7");
        assert!(CorpusWeights::parse(text, "build").is_none());
        assert_eq!(CorpusWeights::uniform().weight_of("anything"), 1);
    }

    #[test]
    fn a_weight_file_cannot_be_read_as_a_partial_or_a_partial_as_a_weight() {
        // CI points ALMIDE_CORPUS_WEIGHTS_DIR at the shard-partials dir, and
        // the run_parity GATE writes the run_parity COLUMN: flat, the weight
        // file and the partials would share a `run_parity.*.txt` glob, and
        // the renderer would read `identical 355` and 735 fixture paths as
        // fixture weights. The subdirectory is what keeps them apart.
        let dir = Path::new("/tmp/corpus-shard");
        let weights = weights_file(dir, "run_parity", "run_parity", ".1-of-2");
        for kind in ["fixtures", "counts", "bridge"] {
            let partial = partial_path(dir, "run_parity", kind, "1-of-2");
            assert_ne!(weights, partial);
            assert_eq!(partial.parent(), Some(dir), "partials stay at the top level");
        }
        assert_eq!(weights.parent(), Some(dir.join("weights").as_path()));
        assert_eq!(weights.file_name().unwrap(), "run_parity.run_parity.1-of-2.txt");
        // Unsharded (the local generator): one file per column and gate.
        assert_eq!(weights_file(dir, "interp", "wasm_runtime_interp_ledger", ""), dir.join("weights/interp.wasm_runtime_interp_ledger.txt"));
    }

    #[test]
    fn an_empty_slice_is_red() {
        assert!(std::panic::catch_unwind(|| CorpusShard::Slice { k: 2, n: 2 }.apply(vec![1], "unweighted-gate", |i| i.to_string())).is_err());
    }
}
