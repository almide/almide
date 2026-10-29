//! The CI slice: ONE switch for every corpus-iterating gate (owner decision,
//! 2026-10-25: "heavy CI jobs run thoroughly only when going into main; on
//! other PRs, run them light").
//!
//! `ALMIDE_CI_SLICE=k` (k > 1) makes a gate judge a commit-rotated 1-in-k
//! slice of its fixtures plus every fixture the change itself touches. Unset
//! or `1` is the full run, and a full run behaves exactly as before the
//! switch existed. CI sets `k` on `pull_request` / `merge_group` events whose
//! base is not `main`; pushes to develop and main, tags, the schedule,
//! `workflow_dispatch` and every PR into main run full.
//!
//! The selection is a property of the FIXTURE, not of the list it sits in: a
//! fixture is in the slice when `(hash(key) + offset) % k == 0`, where `key`
//! is its repo-relative path and `offset` comes from the first 7 hex digits of
//! `ALMIDE_CI_SEED` (the commit sha). So one fixture is in or out of the slice
//! in every gate at once, a gate over the run manifest and a gate over
//! `ls spec/wasm_cross` agree, and consecutive commits judge different
//! residue classes. `scripts/lib/ci-slice.sh` computes the same hash (the
//! `bash_twin_selects_what_rust_selects` test pins the two to each other).
//!
//! What a slice must NOT do, and what the helpers below exist for:
//!   - judge a whole-corpus property against a partial: staleness ("no row
//!     without a fixture"), completeness ("every fixture has a row") and
//!     whole-corpus ceilings run only when [`full_run`] is true;
//!   - rewrite a committed ledger: [`refuse_regen`] panics under a slice;
//!   - pass vacuously: [`select`] never returns an empty slice of a non-empty
//!     list, and prints `kept N of M`, so the number judged is on the log.
//!
//! The change's own fixtures: `ALMIDE_CI_TOUCHED` (a file of repo-relative
//! paths, written once per job by `scripts/ci-slice-prepare.sh`), else
//! `git diff --name-only $ALMIDE_CI_BASE HEAD`. Under CI (`CI=true`) a slice
//! with neither is refused — a slice that silently left out the fixture the
//! PR changed would be the cheapest way to land a broken one.

use std::collections::BTreeSet;
use std::sync::OnceLock;

/// The parsed switch: `k`, the rotation, and the change's own paths.
#[derive(Debug)]
pub struct CiSlice {
    pub k: u64,
    pub offset: u64,
    touched: BTreeSet<String>,
}

/// The polynomial hash both twins compute over the key's bytes:
/// `h = (h * 31 + byte) mod 1_000_000_007`. Chosen because awk's doubles
/// carry it exactly (every intermediate stays below 2^35).
pub fn slice_hash(key: &str) -> u64 {
    key.bytes().fold(0u64, |h, b| (h * 31 + b as u64) % 1_000_000_007)
}

/// The first 7 hex digits of a seed, as a number (`0` for none).
pub fn seed_value(seed: &str) -> u64 {
    let hex: String = seed.chars().filter(|c| c.is_ascii_hexdigit()).take(7).collect();
    u64::from_str_radix(if hex.is_empty() { "0" } else { &hex }, 16).expect("hex digits")
}

/// `ALMIDE_CI_SLICE`, parsed once per process; `None` on a full run.
pub fn ci_slice() -> Option<&'static CiSlice> {
    static SLICE: OnceLock<Option<CiSlice>> = OnceLock::new();
    SLICE
        .get_or_init(|| {
            let raw = almide_base::env::var("ALMIDE_CI_SLICE")?;
            let k: u64 = raw
                .trim()
                .parse()
                .unwrap_or_else(|_| panic!("ALMIDE_CI_SLICE={raw:?}: expected a whole number k >= 1 (1 = the full run)"));
            if k <= 1 {
                return None;
            }
            let offset = seed_value(&almide_base::env::var("ALMIDE_CI_SEED").unwrap_or_default()) % k;
            Some(CiSlice { k, offset, touched: touched_paths() })
        })
        .as_ref()
}

/// True on a full run (no slice): the only run that judges whole-corpus
/// staleness, completeness and ceilings.
pub fn full_run() -> bool {
    ci_slice().is_none()
}

/// A gate that would REWRITE a committed ledger calls this first: a slice
/// regenerates rows for the fixtures it ran and drops the rest, so a ledger
/// written under one would delete every row outside the slice.
pub fn refuse_regen(gate: &str) {
    if let Some(s) = ci_slice() {
        panic!("{gate}: refusing to regenerate a committed ledger under ALMIDE_CI_SLICE={} — a slice sees 1 in {} fixtures; unset it to regenerate", s.k, s.k);
    }
}

fn touched_paths() -> BTreeSet<String> {
    if let Some(file) = almide_base::env::var("ALMIDE_CI_TOUCHED") {
        let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("ALMIDE_CI_TOUCHED={file}: {e}"));
        return text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
    }
    if let Some(base) = almide_base::env::var("ALMIDE_CI_BASE") {
        let out = std::process::Command::new("git")
            .args(["diff", "--name-only", base.trim(), "HEAD"])
            .output()
            .unwrap_or_else(|e| panic!("ALMIDE_CI_BASE={base}: git did not start: {e}"));
        assert!(
            out.status.success(),
            "ALMIDE_CI_BASE={base}: `git diff --name-only {base} HEAD` failed ({}) — the base commit is not in this clone; run scripts/ci-slice-prepare.sh first (it fetches the base and writes ALMIDE_CI_TOUCHED)",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return String::from_utf8_lossy(&out.stdout).lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
    }
    assert!(
        std::env::var("CI").map_or(true, |v| v != "true"),
        "ALMIDE_CI_SLICE is set under CI with neither ALMIDE_CI_TOUCHED nor ALMIDE_CI_BASE — the slice would leave out the fixtures the change touches; run scripts/ci-slice-prepare.sh in this job"
    );
    BTreeSet::new()
}

/// Does `path` (a changed file) touch the fixture `key`? The fixture itself,
/// a file under it (a case directory), or a sibling sharing its stem
/// (`foo.almd` and `foo.stdout`, `foo.expected`).
fn touches(path: &str, key: &str) -> bool {
    let path = path.strip_prefix("als/").unwrap_or(path);
    let key = key.strip_prefix("./").unwrap_or(key);
    if path == key || path.strip_prefix(key).is_some_and(|r| r.starts_with('/')) {
        return true;
    }
    match key.rsplit_once('.') {
        Some((stem, _)) if !stem.ends_with('/') && !stem.is_empty() => path.strip_prefix(stem).is_some_and(|r| r.starts_with('.')),
        _ => false,
    }
}

impl CiSlice {
    /// The rotated residue: 0 means "in the slice".
    fn rank(&self, key: &str) -> u64 {
        (slice_hash(key.strip_prefix("./").unwrap_or(key)) + self.offset) % self.k
    }

    /// Is the fixture `key` (a repo-relative path) in this run's slice?
    pub fn keeps(&self, key: &str) -> bool {
        self.rank(key) == 0 || self.touched_by_change(key)
    }

    /// Does the change itself touch the fixture `key`?
    pub fn touched_by_change(&self, key: &str) -> bool {
        self.touched.iter().any(|p| touches(p, key))
    }

    /// The slice of `items` (in input order). Never empty for a non-empty
    /// input: when no item falls in the residue class and none is touched,
    /// the item of lowest rotated residue (the first on a tie) is kept, so a
    /// short list still judges something.
    pub fn apply<T>(&self, items: Vec<T>, key: impl Fn(&T) -> String) -> (Vec<T>, usize) {
        let total = items.len();
        let keys: Vec<String> = items.iter().map(&key).collect();
        let mut keep: Vec<bool> = keys.iter().map(|k| self.keeps(k)).collect();
        if total > 0 && !keep.iter().any(|b| *b) {
            let best = (0..total).min_by_key(|&i| (self.rank(&keys[i]), i)).expect("non-empty");
            keep[best] = true;
        }
        let touched = keys.iter().zip(&keep).filter(|(k, kept)| **kept && self.touched_by_change(k)).count();
        let kept = items.into_iter().zip(keep).filter(|(_, b)| *b).map(|(t, _)| t).collect();
        (kept, touched)
    }
}

/// The ONE selection point for a gate's fixture list. Full run: `items`
/// unchanged, and the log says `full run, M of M`. Slice: the fixtures in the
/// slice, and the log says how many of how many — the count a reader holds
/// the gate's verdicts against. `key` is the fixture's repo-relative path
/// (a file or a case directory).
pub fn select<T>(items: Vec<T>, gate: &str, key: impl Fn(&T) -> String) -> Vec<T> {
    let total = items.len();
    match ci_slice() {
        None => {
            eprintln!("{gate}: full run, {total} of {total} fixture(s)");
            items
        }
        Some(s) => {
            let (kept, touched) = s.apply(items, key);
            eprintln!(
                "{gate}: CI slice 1/{} (offset {}) keeps {} of {total} fixture(s), {touched} of them touched by the change; the full run (develop, main, PRs into main) judges all {total}",
                s.k,
                s.offset,
                kept.len()
            );
            kept
        }
    }
}

/// A count for a sliced run: the full count on a full run, else 1 in `k`
/// rounded up (never below `floor`). For gates whose unit is a seed or a
/// mutant rather than a fixture (the differential fuzzers).
pub fn scaled_count(full: usize, floor: usize, gate: &str) -> usize {
    match ci_slice() {
        None => full,
        Some(s) => {
            let n = full.div_ceil(s.k as usize).max(floor).min(full);
            eprintln!("{gate}: CI slice 1/{} runs {n} of {full} (the full run runs all {full})", s.k);
            n
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slice(k: u64, offset: u64, touched: &[&str]) -> CiSlice {
        CiSlice { k, offset, touched: touched.iter().map(|s| s.to_string()).collect() }
    }

    #[test]
    fn the_k_rotations_partition_any_list() {
        let keys: Vec<String> = (0..500).map(|i| format!("spec/wasm_cross/f{i}.almd")).collect();
        let mut seen = vec![0usize; keys.len()];
        for off in 0..8 {
            let (kept, _) = slice(8, off, &[]).apply((0..keys.len()).collect::<Vec<_>>(), |i| keys[*i].clone());
            for i in kept {
                seen[i] += 1;
            }
        }
        assert!(seen.iter().all(|c| *c == 1), "every fixture lands in exactly one of the 8 rotations");
    }

    #[test]
    fn the_change_s_own_fixtures_join_the_slice() {
        let s = slice(8, 0, &["spec/wasm_cross/foo.almd", "tests/diagnostics/e001/broken.almd", "spec/lang/bar.stdout"]);
        assert!(s.keeps("spec/wasm_cross/foo.almd"));
        assert!(s.keeps("tests/diagnostics/e001"), "a file under a case directory touches the case");
        assert!(s.keeps("spec/lang/bar.almd"), "a sibling sharing the stem touches the fixture");
        assert!(!s.touched_by_change("spec/wasm_cross/foobar.almd"));
        assert!(!s.touched_by_change("tests/diagnostics/e0011"));
    }

    #[test]
    fn a_short_list_still_judges_one() {
        let s = slice(1000, 3, &[]);
        let (kept, _) = s.apply(vec!["a", "b", "c"], |x| x.to_string());
        assert_eq!(kept.len(), 1);
        assert!(s.apply(Vec::<&str>::new(), |x| x.to_string()).0.is_empty());
    }

    #[test]
    fn seeds_read_seven_hex_digits() {
        assert_eq!(seed_value("abcdef0123"), 0xabcdef0);
        assert_eq!(seed_value(""), 0);
        assert_eq!(slice_hash(""), 0);
        assert_eq!(slice_hash("a"), 97);
        assert_eq!(slice_hash("ab"), 97 * 31 + 98);
    }

    /// The bash twin (scripts/lib/ci-slice.sh) selects exactly what this
    /// module selects, over the real corpus file names and several offsets.
    #[test]
    fn bash_twin_selects_what_rust_selects() {
        let root = crate::workspace_root(env!("CARGO_MANIFEST_DIR"));
        let lib = root.join("scripts/lib/ci-slice.sh");
        let mut keys: Vec<String> = crate::walk_spec(&root).into_iter().map(|(rel, _)| rel).collect();
        keys.truncate(400);
        keys.push("tests/diagnostics/e001-some-case".into());
        for (k, seed) in [(8u64, "0000000"), (8, "40b1c3f"), (3, "fffffff"), (5, "1234567")] {
            let s = slice(k, seed_value(seed) % k, &[]);
            let want: Vec<String> = s.apply(keys.clone(), |x| x.clone()).0;
            let script = format!(". '{}'; ci_slice_filter twin-test", lib.display());
            let mut child = std::process::Command::new("bash")
                .args(["-c", &script])
                .env("ALMIDE_CI_SLICE", k.to_string())
                .env("ALMIDE_CI_SEED", seed)
                .env("ALMIDE_CI_TOUCHED", "/dev/null")
                .env_remove("ALMIDE_CI_BASE")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("bash");
            use std::io::Write;
            child.stdin.take().expect("stdin").write_all((keys.join("\n") + "\n").as_bytes()).expect("write");
            let out = child.wait_with_output().expect("bash ran");
            assert!(out.status.success(), "ci_slice_filter failed");
            let got: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().map(String::from).collect();
            assert_eq!(got, want, "k={k} seed={seed}: the bash twin disagrees with the Rust selection");
        }
    }
}
