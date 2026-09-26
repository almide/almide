//! Phase 4 of `docs/roadmap/active/diagnostics-here-try-hint.md`:
//! coverage report across the 3-layer diagnostic surface.
//!
//! For every `with_code("E###")` site under `crates/`, verify:
//!
//! 1. **Fixture**: `tests/diagnostics/<case>/meta.toml` declares
//!    `expects_code = "E###"` somewhere.
//! 2. **Doc**: `docs/diagnostics/E###.md` exists (Phase 5 registry).
//!
//! Missing coverage is surfaced via a printed report; the test
//! **does not currently fail** on gaps (soft gate). The intent is to
//! make the gap visible on every CI run so adding a new `with_code`
//! call without its fixture + doc file becomes an obvious
//! regression rather than silent drift.
//!
//! Promote to a hard gate by flipping the `SOFT_GATE` constant once
//! backfill completes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// When `true`, the test prints coverage gaps but passes. Flipped
/// to `false` once fixture backfill reached the allowlist-only
/// residual — the coverage gate now enforces at CI time.
const SOFT_GATE: bool = false;

/// Codes that intentionally skip the fixture requirement. Usually
/// because the diagnostic needs a multi-file setup the single-file
/// fixture harness can't express (E420 is cross-module visibility,
/// which requires an `import` graph). Keep this list short — each
/// entry is a gap the harness can't cover today.
// E420 and E033 left this list in #1528's multi-file sweep: a fixture dir
// may carry almide.toml + src/*.almd siblings, and the harness's
// `almide check broken.almd` resolves `import self.x` against them.
const FIXTURE_ALLOWLIST: &[&str] = &[
    // E054 (fmt verification failed) fires on an INTERNAL formatter defect —
    // no committed source file can (or should) trigger it deliberately, and
    // the harness runs `almide check`, which never formats. The verifier
    // logic is pinned by crates/almide-tools/tests/fmt_corpus_test.rs, which
    // feeds corrupted outputs straight into verify_format (#1464).
    "E054",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn scan_diagnostic_codes() -> BTreeSet<String> {
    // Scan every `crates/**/*.rs` for `with_code("E###")`; collect
    // unique codes. Deliberately doesn't use a regex crate — the
    // test crate stays dependency-light.
    let root = repo_root().join("crates");
    let mut codes = BTreeSet::new();
    fn walk(dir: &Path, codes: &mut BTreeSet<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().map_or(false, |n| n == "target") { continue; }
                walk(&path, codes);
            } else if path.extension().map_or(false, |e| e == "rs") {
                let Ok(text) = std::fs::read_to_string(&path) else { continue };
                let mut rest = text.as_str();
                while let Some(pos) = rest.find("with_code(\"E") {
                    rest = &rest[pos + "with_code(\"E".len()..];
                    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if !digits.is_empty() && rest[digits.len()..].starts_with('"') {
                        codes.insert(format!("E{:03}", digits.parse::<u32>().unwrap_or(0)));
                    }
                }
            }
        }
    }
    walk(&root, &mut codes);
    codes
}

fn scan_fixture_codes() -> BTreeMap<String, Vec<String>> {
    // Returns code → list of fixture names that declare it.
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let root = repo_root().join("tests/diagnostics");
    let Ok(entries) = std::fs::read_dir(&root) else { return out; };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let meta = path.join("meta.toml");
        let Ok(text) = std::fs::read_to_string(&meta) else { continue };
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("expects_code") {
                let value = rest
                    .trim_start_matches(|c: char| c == '=' || c.is_whitespace())
                    .trim_matches('"');
                let case = path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                out.entry(value.to_string()).or_default().push(case);
            }
        }
    }
    out
}

fn scan_doc_codes() -> BTreeSet<String> {
    let root = repo_root().join("docs/diagnostics");
    let Ok(entries) = std::fs::read_dir(&root) else { return BTreeSet::new(); };
    entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|n| n.strip_suffix(".md").map(|s| s.to_string()))
        .filter(|n| n.starts_with('E') && n.len() >= 4)
        .collect()
}

#[test]
fn diagnostic_fixture_and_doc_coverage_report() {
    let codes = scan_diagnostic_codes();
    assert!(!codes.is_empty(), "no diagnostic codes found under crates/");

    let fixtures = scan_fixture_codes();
    let docs = scan_doc_codes();

    let mut missing_fixture: Vec<&String> = Vec::new();
    let mut allowlisted_fixture: Vec<&String> = Vec::new();
    let mut missing_doc: Vec<&String> = Vec::new();
    for code in &codes {
        if !fixtures.contains_key(code) {
            if FIXTURE_ALLOWLIST.contains(&code.as_str()) {
                allowlisted_fixture.push(code);
            } else {
                missing_fixture.push(code);
            }
        }
        if !docs.contains(code)         { missing_doc.push(code); }
    }

    eprintln!();
    eprintln!("── Diagnostic coverage report ───────────────────────────");
    eprintln!("  Codes in source    : {}", codes.len());
    eprintln!("  Fixture-covered    : {} ({})",
        codes.len() - missing_fixture.len() - allowlisted_fixture.len(),
        fixtures.values().map(|v| v.len()).sum::<usize>());
    eprintln!("  Doc-covered        : {}", codes.len() - missing_doc.len());
    if !missing_fixture.is_empty() {
        eprintln!("  Missing fixtures    : {}", missing_fixture.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(", "));
    }
    if !allowlisted_fixture.is_empty() {
        eprintln!("  Allowlisted         : {}", allowlisted_fixture.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(", "));
    }
    if !missing_doc.is_empty() {
        eprintln!("  Missing docs        : {}", missing_doc.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(", "));
    }
    eprintln!("─────────────────────────────────────────────────────────");
    eprintln!();

    if !SOFT_GATE && (!missing_fixture.is_empty() || !missing_doc.is_empty()) {
        panic!(
            "diagnostic coverage incomplete:\n  missing fixtures: {:?}\n  missing docs: {:?}",
            missing_fixture, missing_doc
        );
    }
}

/// #1528 tier-1 floor, PROMOTED from "backlog, not a failure" to ENFORCED:
/// every fixture-covered code carries at least THREE families (distinct
/// broken/fixed/meta triples), so a code cannot sit at a single token case —
/// each hint variant needs its own family before the code counts as covered
/// in depth. The last below-bar codes (E059 at 1, E060 at 2) were brought to
/// the floor in the same change that added this gate; a new code lands with
/// its three families or turns this red.
#[test]
fn every_covered_code_has_at_least_three_fixture_families() {
    let fixtures = scan_fixture_codes();
    let below: Vec<String> = fixtures
        .iter()
        .filter(|(_, fams)| fams.len() < 3)
        .map(|(c, fams)| format!("{c}({})", fams.len()))
        .collect();
    assert!(
        below.is_empty(),
        "codes below the 3-family tier-1 floor: {} — add families under \
         tests/diagnostics/ (broken/fixed/meta) until each code has three",
        below.join(", ")
    );
}

/// #1486: every diagnostic doc declares its FIX-IT VERDICT — `mechanical`
/// (an unattended fixer may apply it), `conditional` (suggested, needs
/// review), or `not-fixable`. The verdict is the third piece of the
/// per-code checklist this gate already enforces (fixture + doc), so a new
/// code cannot ship without stating where it sits on the applicability
/// ladder.
#[test]
fn every_diagnostic_doc_declares_a_fix_it_verdict() {
    let (missing, _) = scan_fix_it_verdicts();
    assert!(
        missing.is_empty(),
        "diagnostic docs without a '## Fix-it verdict' section: {:?}",
        missing
    );
}

/// `(docs without a verdict section, codes whose verdict is **mechanical**)`.
fn scan_fix_it_verdicts() -> (Vec<String>, BTreeSet<String>) {
    let root = repo_root().join("docs/diagnostics");
    let mut missing: Vec<String> = Vec::new();
    let mut mechanical: BTreeSet<String> = BTreeSet::new();
    for entry in std::fs::read_dir(&root).expect("read docs/diagnostics").flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !name.starts_with('E') || !name.ends_with(".md") { continue; }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let Some(verdict) = text.split("## Fix-it verdict").nth(1) else {
            missing.push(name.to_string());
            continue;
        };
        if verdict.contains("**mechanical**") {
            mechanical.insert(name.trim_end_matches(".md").to_string());
        }
    }
    (missing, mechanical)
}

/// #2149: a `**mechanical**` verdict is a promise that `almide fix` repairs
/// the code unattended — so it is judged from the BINARY, not from a list.
/// (The literal this replaced still named five codes after the engine
/// learned all seven in c6f77330e, and only ever `eprintln!`ed the gap.)
///
/// For every mechanical code, over its own fixture families
/// (`tests/diagnostics/<code>-*`):
///
/// 1. at least one family's diagnostic of that code carries a
///    `repair.primary` tagged `machine-applicable` in `almide check --json`;
/// 2. every such family ROUND-TRIPS: `almide fix` on a copy of `broken.almd`
///    yields a program `almide check` accepts (judged by exit status —
///    warnings print first and are not a verdict).
///
/// The reverse direction (no NON-mechanical code emits a machine-applicable
/// repair) needs every fixture's JSON and lives beside the population floor
/// in `diagnostic_harness_test.rs`.
#[test]
fn every_mechanical_verdict_emits_a_machine_applicable_repair_that_round_trips() {
    let (_, mechanical) = scan_fix_it_verdicts();
    assert!(!mechanical.is_empty(), "no mechanical verdicts found — the scan went vacuous");
    let bin = env!("CARGO_BIN_EXE_almide");
    let fixtures = repo_root().join("tests/diagnostics");
    let mut backlog: Vec<String> = Vec::new();
    let mut broken_round_trips: Vec<String> = Vec::new();
    let mut proven: Vec<String> = Vec::new();
    for code in &mechanical {
        let prefix = format!("{}-", code.to_ascii_lowercase());
        let mut families: Vec<PathBuf> = std::fs::read_dir(&fixtures).expect("read tests/diagnostics")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&prefix)))
            .collect();
        families.sort();
        let mut machine_families = 0usize;
        for family in &families {
            let out = std::process::Command::new(bin)
                .args(["check", "--json"])
                .arg(family.join("broken.almd"))
                .output()
                .expect("almide check --json");
            let stdout = String::from_utf8_lossy(&out.stdout);
            let carries = stdout.lines()
                .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
                .any(|d| {
                    d["code"] == code.as_str()
                        && d["repair"]["primary"]["applicability"] == "machine-applicable"
                });
            if !carries { continue; }
            machine_families += 1;
            // Round-trip on a scratch copy of the whole family (a fixture may
            // carry almide.toml + src/ siblings the check resolves against).
            let scratch = tempfile::tempdir().expect("tempdir");
            copy_dir(family, scratch.path());
            let fixed = std::process::Command::new(bin)
                .args(["fix", "broken.almd"])
                .current_dir(scratch.path())
                .output()
                .expect("almide fix");
            let check = std::process::Command::new(bin)
                .args(["check", "broken.almd"])
                .current_dir(scratch.path())
                .output()
                .expect("almide check");
            if !fixed.status.success() || !check.status.success() {
                broken_round_trips.push(format!(
                    "{}: after `almide fix` the program does not check:\n{}{}",
                    family.display(),
                    String::from_utf8_lossy(&check.stdout),
                    String::from_utf8_lossy(&check.stderr),
                ));
            }
        }
        if machine_families == 0 {
            backlog.push(code.clone());
        } else {
            proven.push(format!("{code}({machine_families})"));
        }
    }
    eprintln!("mechanical verdicts with a machine-applicable repair: {}", proven.join(", "));
    assert!(
        backlog.is_empty(),
        "fix-it backlog: {} mechanical-verdict code(s) emit no machine-applicable \
         `repair.primary` on any of their fixtures: {:?} — attach one with \
         `with_machine_fix`, or change the doc's verdict to `conditional`",
        backlog.len(),
        backlog
    );
    assert!(
        broken_round_trips.is_empty(),
        "machine-applicable repairs that `almide fix` cannot round-trip:\n{}",
        broken_round_trips.join("\n")
    );
}

fn copy_dir(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).expect("read fixture dir").flatten() {
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            std::fs::create_dir_all(&dst).expect("mkdir");
            copy_dir(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).expect("copy fixture file");
        }
    }
}

#[test]
fn every_fixture_meta_declares_known_code() {
    // Reverse gate: every `meta.toml` with `expects_code` must name a
    // code that actually exists in source. Catches typos like
    // `E02` → fixture orphaned.
    let source_codes = scan_diagnostic_codes();
    let fixtures = scan_fixture_codes();
    let mut orphans: Vec<String> = Vec::new();
    for (code, cases) in &fixtures {
        if !source_codes.contains(code) {
            for case in cases {
                orphans.push(format!("{} declares {}", case, code));
            }
        }
    }
    if !orphans.is_empty() {
        panic!("fixtures declare unknown codes:\n  {}", orphans.join("\n  "));
    }
}


/// Ratchet: the number of `super::err(` construction sites WITHOUT a
/// `.with_code(...)` attached. Every user-facing error should carry a
/// stable E-code (the `--explain` / dojo-feedback surface); this count
/// may only go DOWN. When you add a code to an existing site, lower the
/// constant. Adding a NEW uncoded error site fails this gate — attach a
/// code (and its fixture + doc, enforced above) instead.
/// Baseline measured 2026-08-06 (#1113); lowered to 39 when the eq
/// "compares" site gained E037 (#1116).
const UNCODED_ERR_BASELINE: usize = 39;

#[test]
fn uncoded_error_sites_ratchet() {
    let root = repo_root().join("crates");
    let mut uncoded = 0usize;
    let mut by_file: BTreeMap<String, usize> = BTreeMap::new();
    fn walk(dir: &Path, f: &mut impl FnMut(&Path, &str)) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().map_or(false, |n| n == "target") { continue; }
                walk(&path, f);
            } else if path.extension().map_or(false, |e| e == "rs") {
                if let Ok(text) = std::fs::read_to_string(&path) { f(&path, &text); }
            }
        }
    }
    walk(&root, &mut |path, text| {
        let mut i = 0usize;
        while let Some(j) = text[i..].find("super::err(") {
            let j = i + j;
            // The 600-byte window must not split a multi-byte char (an
            // em-dash in a diagnostic message sat exactly on the boundary).
            let mut end = (j + 600).min(text.len());
            while !text.is_char_boundary(end) { end -= 1; }
            let window = &text[j..end];
            let seg = match window.find("));") {
                Some(end) => &window[..end + 3],
                None => window,
            };
            if !seg.contains(".with_code(") {
                uncoded += 1;
                *by_file.entry(path.display().to_string()).or_default() += 1;
            }
            i = j + "super::err(".len();
        }
    });
    for (file, n) in &by_file {
        eprintln!("uncoded err sites: {:3}  {}", n, file);
    }
    assert!(
        uncoded <= UNCODED_ERR_BASELINE,
        "uncoded diagnostic sites grew: {} > baseline {} — attach .with_code (+ fixture + doc) to new errors",
        uncoded, UNCODED_ERR_BASELINE
    );
    if uncoded < UNCODED_ERR_BASELINE {
        eprintln!(
            "NOTE: uncoded sites = {} < baseline {} — lower UNCODED_ERR_BASELINE to lock in the progress",
            uncoded, UNCODED_ERR_BASELINE
        );
    }
}
