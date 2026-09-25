//! `almide survive` and `almide apply --if-survives` (#2147): the survival
//! delta of a proposed edit, answered BEFORE the edit is written.
//!
//! The failure that kills modification survival is an agent applying an edit
//! it did not verify. `almide check` and `almide test` only judge the tree as
//! written, so "would this edit survive?" used to cost a write, two calls and
//! a manual revert. `survive` answers it in one call and never writes:
//!
//! 1. the edit (`--with`: a unified diff or the file's full new text) is
//!    applied in memory;
//! 2. the reach set is found — the edited file and every `.almd` under the
//!    project root whose import closure loads it;
//! 3. each leg runs on both sides — `check --json` on every reached file, the
//!    `test` blocks of every reached file, and the native⇄wasm run of every
//!    reached `// @contract:` fixture — the after-side under the source
//!    overlay (`almide::source_overlay`), so the disk bytes are never touched;
//! 4. every diagnostic / test / contract outcome is paired across the two
//!    sides and bucketed `{unchanged, newly_broken, newly_fixed, removed}`
//!    (`survive_delta` holds the identity rule and the bucket table).
//!
//! The edit **survives** iff no ERROR diagnostic, no test and no contract is
//! `newly_broken`. A newly broken WARNING is reported but does not decide the
//! verdict, for the reason `almide check` itself exits 0 on one — and because
//! the unused-variable lint only runs on an error-free file, so fixing a
//! file's last error surfaces warnings that were there all along; gating on
//! them would refuse exactly the edits that fix things. `apply --if-survives`
//! is the same evaluation followed by an atomic write that happens only on
//! that verdict (or on an explicit `--force`).
//!
//! The JSON is versioned (`schema_version`); the three golden deltas in
//! `tests/survive/` pin it.

use super::survive_delta::{bucket, diagnostic_delta, Delta, LineMap};
use super::survive_edit::{after_text, detect, read_with, EditKind};
use super::survive_legs::{self as legs, OverlayEnv};
use crate::{err, out};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The version of the report shape. Bump on any change that is not purely
/// additive (a renamed / removed / retyped field, a changed bucket rule).
pub const SCHEMA_VERSION: u64 = 1;

/// What `survive` and `apply` share from the command line.
pub struct SurviveArgs {
    pub file: String,
    pub with: String,
    pub as_kind: String,
    pub json: bool,
    pub timeout_secs: u64,
}

/// The file and both texts, once the edit has been read and applied.
struct Prepared {
    file: String,
    before: String,
    after: String,
    kind: EditKind,
}

fn prepare(a: &SurviveArgs) -> Result<Prepared, String> {
    let path = Path::new(&a.file);
    if !path.is_file() {
        return Err(format!("`{}` is not a file — survive judges an edit to an existing .almd file", a.file));
    }
    let before = std::fs::read_to_string(path).map_err(|e| format!("cannot read `{}`: {}", a.file, e))?;
    let content = read_with(&a.with)?;
    let kind = EditKind::parse_flag(&a.as_kind)?.unwrap_or_else(|| detect(&content));
    let after = after_text(&before, &content, kind)?;
    Ok(Prepared { file: a.file.clone(), before, after, kind })
}

/// The after-text, parked OUTSIDE the tree for the child processes to read;
/// removed when dropped.
struct ParkedText {
    dir: PathBuf,
    file: PathBuf,
}

impl ParkedText {
    fn new(text: &str) -> Result<Self, String> {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("almide-survive-{}-{}", std::process::id(), nanos));
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {}", dir.display(), e))?;
        let file = dir.join("after.almd");
        std::fs::write(&file, text).map_err(|e| format!("cannot write {}: {}", file.display(), e))?;
        Ok(ParkedText { dir, file })
    }
}

impl Drop for ParkedText {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Where the reach scan looks: the project (`./almide.toml`), else the edited
/// file's own directory.
fn reach_root(file: &str) -> PathBuf {
    if Path::new("almide.toml").exists() {
        return PathBuf::from(".");
    }
    match Path::new(file).parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// The text of `f` on one side: the edited file's before/after text, the disk
/// otherwise.
fn side_text(f: &str, edited: &Path, p: &Prepared, after: bool) -> String {
    if almide::source_overlay::canonical(Path::new(f)) == edited {
        return if after { p.after.clone() } else { p.before.clone() };
    }
    std::fs::read_to_string(f).unwrap_or_default()
}

fn test_delta(files: &[String], edited: &Path, p: &Prepared, ov: &OverlayEnv, timeout: Duration) -> Result<Delta, String> {
    let mut d = Delta::default();
    for f in files {
        let (bt, at) = (side_text(f, edited, p, false), side_text(f, edited, p, true));
        let before = if legs::has_tests(&bt) { legs::test_leg(f, &bt, None, timeout)? } else { Default::default() };
        let after = if legs::has_tests(&at) { legs::test_leg(f, &at, Some(ov), timeout)? } else { Default::default() };
        let mut names: Vec<&String> = before.keys().chain(after.keys()).collect();
        names.sort();
        names.dedup();
        for name in names {
            let (b, a) = (before.get(name), after.get(name));
            let mut entry = json!({
                "file": f,
                "name": name,
                "before": b.map(|o| o.status),
                "after": a.map(|o| o.status),
            });
            if let Some(fail) = a.and_then(|o| o.failure.clone()) {
                entry["failure"] = fail;
            }
            d.push(bucket(b.map(|o| o.status), a.map(|o| o.status), legs::test_good), entry);
        }
    }
    Ok(d)
}

fn contract_delta(files: &[String], edited: &Path, p: &Prepared, ov: &OverlayEnv, timeout: Duration) -> Result<Delta, String> {
    let mut d = Delta::default();
    for f in files {
        let (bc, ac) = (
            legs::declared_contracts(&side_text(f, edited, p, false)),
            legs::declared_contracts(&side_text(f, edited, p, true)),
        );
        let before = if bc.is_empty() { None } else { Some(legs::contract_leg(f, None, timeout)?) };
        let after = if ac.is_empty() { None } else { Some(legs::contract_leg(f, Some(ov), timeout)?) };
        let mut ids: Vec<&String> = bc.iter().chain(ac.iter()).collect();
        ids.sort();
        ids.dedup();
        for id in ids {
            let b = before.as_ref().filter(|_| bc.contains(id)).map(|(s, _)| *s);
            let a = after.as_ref().filter(|_| ac.contains(id));
            let mut entry = json!({ "fixture": f, "contract": id, "before": b, "after": a.map(|(s, _)| *s) });
            if let Some((_, detail)) = a {
                entry["detail"] = detail.clone();
            }
            d.push(bucket(b, a.map(|(s, _)| *s), legs::contract_good), entry);
        }
    }
    Ok(d)
}

/// The whole evaluation: reach, three legs on two sides, the report.
fn evaluate(p: &Prepared, timeout: Duration) -> Result<Value, String> {
    let edited = almide::source_overlay::canonical(Path::new(&p.file));
    let parked = ParkedText::new(&p.after)?;
    let ov = OverlayEnv { target: edited.clone(), text_file: parked.file.clone() };
    let root = reach_root(&p.file);
    let reach = legs::reach_set(&root, &p.file, &edited, &p.after);

    let mut unstructured = Vec::new();
    let before_diags = legs::check_leg(&reach, &edited, None, timeout, "before", &mut unstructured)?;
    let after_diags = legs::check_leg(&reach, &edited, Some(&ov), timeout, "after", &mut unstructured)?;
    let check = diagnostic_delta(&before_diags, &after_diags, &LineMap::new(&p.before, &p.after));

    let with_tests: Vec<String> = reach
        .iter()
        .filter(|f| legs::has_tests(&side_text(f, &edited, p, false)) || legs::has_tests(&side_text(f, &edited, p, true)))
        .cloned()
        .collect();
    let with_contracts: Vec<String> = reach
        .iter()
        .filter(|f| {
            !legs::declared_contracts(&side_text(f, &edited, p, false)).is_empty()
                || !legs::declared_contracts(&side_text(f, &edited, p, true)).is_empty()
        })
        .cloned()
        .collect();
    let tests = test_delta(&with_tests, &edited, p, &ov, timeout)?;
    let contracts = contract_delta(&with_contracts, &edited, p, &ov, timeout)?;

    let broken_errors = check.newly_broken.iter().filter(|e| e["after"]["level"] == "error").count();
    let survives = broken_errors == 0 && tests.newly_broken.is_empty() && contracts.newly_broken.is_empty();
    let mut check_json = check.to_json();
    if !unstructured.is_empty() {
        check_json["unstructured"] = json!(unstructured);
    }
    Ok(json!({
        "schema_version": SCHEMA_VERSION,
        "file": p.file,
        "edit": {
            "kind": p.kind.as_str(),
            "lines_before": p.before.lines().count(),
            "lines_after": p.after.lines().count(),
        },
        "written": false,
        "survives": survives,
        "reach": {
            "root": root.to_string_lossy(),
            "checked": reach,
            "tests": with_tests,
            "contracts": with_contracts,
        },
        "summary": {
            "check": check.counts(),
            "tests": tests.counts(),
            "contracts": contracts.counts(),
        },
        "check": check_json,
        "tests": tests.to_json(),
        "contracts": contracts.to_json(),
    }))
}

/// Exit 2 with the reason — as a JSON object on stdout under `--json`, so a
/// harness never has to parse prose to learn the call itself failed.
fn fail(json_mode: bool, msg: &str) -> ! {
    if json_mode {
        out(&json!({ "schema_version": SCHEMA_VERSION, "error": msg }).to_string());
    } else {
        err(&format!("error: {}", msg));
    }
    std::process::exit(2);
}

fn describe(e: &Value, leg: &str) -> String {
    let side = if e["after"].is_null() { &e["before"] } else { &e["after"] };
    match leg {
        "check" => format!(
            "{}:{}:{} {} {}",
            side["file"].as_str().unwrap_or(""),
            side["line"],
            side["col"],
            side["code"].as_str().unwrap_or(""),
            side["message"].as_str().unwrap_or("")
        ),
        "tests" => format!("{} \"{}\" ({} → {})", e["file"].as_str().unwrap_or(""), e["name"].as_str().unwrap_or(""), e["before"], e["after"]),
        _ => format!("{} {} ({} → {})", e["fixture"].as_str().unwrap_or(""), e["contract"].as_str().unwrap_or(""), e["before"], e["after"]),
    }
}

fn render_human(r: &Value) {
    let verdict = if r["survives"] == true { "survives" } else { "does NOT survive" };
    err(&format!("{}: the edit {}", r["file"].as_str().unwrap_or(""), verdict));
    for leg in ["check", "tests", "contracts"] {
        let s = &r["summary"][leg];
        err(&format!(
            "  {:<9} {} newly broken, {} newly fixed, {} unchanged, {} removed",
            format!("{}:", leg),
            s["newly_broken"], s["newly_fixed"], s["unchanged"], s["removed"]
        ));
    }
    for (bucket, label) in [("newly_broken", "newly broken"), ("newly_fixed", "newly fixed")] {
        for leg in ["check", "tests", "contracts"] {
            for e in r[leg][bucket].as_array().into_iter().flatten() {
                err(&format!("  {} [{}] {}", label, leg, describe(e, leg)));
            }
        }
    }
}

/// `almide survive FILE --with EDIT [--json]`. Exit 0 = survives, 1 = does
/// not, 2 = the edit could not be judged (unreadable, a hunk that does not
/// apply).
pub fn cmd_survive(a: SurviveArgs) {
    let p = prepare(&a).unwrap_or_else(|e| fail(a.json, &e));
    let report = evaluate(&p, Duration::from_secs(a.timeout_secs)).unwrap_or_else(|e| fail(a.json, &e));
    if a.json {
        out(&report.to_string());
    } else {
        render_human(&report);
    }
    std::process::exit(if report["survives"] == true { 0 } else { 1 });
}

/// Replace `path`'s contents with `text` atomically: a sibling temp file,
/// flushed, given the original's permissions, renamed over it. A reader sees
/// the old bytes or the new ones, never a torn file.
fn atomic_write(path: &Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = dir.join(format!(".{}.almide-apply.{}.tmp", name, std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        if let Ok(meta) = std::fs::metadata(path) {
            std::fs::set_permissions(&tmp, meta.permissions())?;
        }
        std::fs::rename(&tmp, path)
    })();
    result.map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("cannot write `{}`: {}", path.display(), e)
    })
}

/// The `--if-survives` value. Only the two boolean spellings are accepted:
/// anything else fails closed (no evaluation, no write).
fn parse_gate(v: Option<&str>) -> Result<Option<bool>, String> {
    match v {
        None => Ok(None),
        Some("true") => Ok(Some(true)),
        Some("false") => Ok(Some(false)),
        Some(other) => Err(format!("--if-survives takes `true` or `false`, got `{}` — refusing to write", other)),
    }
}

/// `almide apply FILE --with EDIT --if-survives [--force]`: verify, then
/// write, as one step. Writes iff the edit survives, or `--force` says to
/// write it anyway (the evaluation still runs and is reported). Exit 0 =
/// written, 1 = refused because the edit does not survive, 2 = refused before
/// judging (bad flags, an edit that does not apply, the file changed under
/// the evaluation, a failed write).
pub fn cmd_apply(a: SurviveArgs, if_survives: Option<String>, force: bool) {
    let gate = parse_gate(if_survives.as_deref()).unwrap_or_else(|e| fail(a.json, &e));
    match (gate, force) {
        (None, false) => fail(
            a.json,
            "apply writes only behind an explicit gate: pass `--if-survives` (verify, then write) or `--force` (write whatever the verdict)",
        ),
        (Some(false), false) => fail(a.json, "`--if-survives=false` turns the gate off; pass `--force` to write an unverified edit"),
        _ => {}
    }
    let p = prepare(&a).unwrap_or_else(|e| fail(a.json, &e));
    let mut report = evaluate(&p, Duration::from_secs(a.timeout_secs)).unwrap_or_else(|e| fail(a.json, &e));
    let survives = report["survives"] == true;
    let write = survives || force;
    if write {
        // The verdict is about the bytes read at the start; refuse to write
        // over anything else.
        let now = std::fs::read_to_string(&p.file).unwrap_or_default();
        if now != p.before {
            fail(a.json, &format!("`{}` changed on disk while the edit was being judged — nothing written", p.file));
        }
        atomic_write(Path::new(&p.file), &p.after).unwrap_or_else(|e| fail(a.json, &e));
    }
    report["written"] = json!(write);
    report["forced"] = json!(force && !survives);
    if a.json {
        out(&report.to_string());
    } else {
        render_human(&report);
        err(&if write {
            format!("  wrote {}{}", p.file, if force && !survives { " (--force)" } else { "" })
        } else {
            format!("  nothing written: {} does not survive the edit (pass --force to write it anyway)", p.file)
        });
    }
    std::process::exit(if write { 0 } else { 1 });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gate_accepts_exactly_two_spellings() {
        assert_eq!(parse_gate(None), Ok(None));
        assert_eq!(parse_gate(Some("true")), Ok(Some(true)));
        assert_eq!(parse_gate(Some("false")), Ok(Some(false)));
        for bad in ["yes", "1", "TRUE", "", "on"] {
            assert!(parse_gate(Some(bad)).is_err(), "{:?} must fail closed", bad);
        }
    }

    #[test]
    fn an_atomic_write_replaces_the_bytes_and_leaves_no_temp_file() {
        let dir = std::env::temp_dir().join(format!("almide-apply-unit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("x.almd");
        std::fs::write(&f, "old\n").unwrap();
        atomic_write(&f, "new\n").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "new\n");
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names.len(), 1, "{:?}", names);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
