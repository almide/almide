//! The shape-matrix gate (#3309): `almide check` accepts ⇒ the native leg
//! builds and runs, the wasm leg builds and runs, and both print the bytes the
//! cell must print (the interp votes too when it does not abstain).
//!
//! Between 2026-10-03 and 10-04 a user porting a real app hit eleven accepted
//! programs that one leg could not build (#3283 #3286 #3287 #3290 #3296 #3297
//! #3303 #3304 #3305 #3306 #3307). None was found by `spec/`, the cross-target
//! fixtures or `tools/xtarget-fuzz`, because they sit on axes nothing varied:
//! several modules / a path-dependency package, module `var` access sites, a
//! top-level `let` of every value kind, names that collide with generated ones
//! or differ only in case, several `mut` args, pattern bindings moved into an
//! outer slot, and same-field records across modules. The cells are generated
//! from those axes by `tests/shape_matrix/gen.rs` (the table of families is at
//! its top); each cell has a stable name.
//!
//! Every cell knows its stdout by construction, so a leg is judged against it
//! alone — two legs that agree on a wrong answer are both convicted.
//!
//! Known-open cells are listed in `proofs/shape-matrix-baseline.txt`, one row
//! per cell: `<cell>  <column>  # #NNNN` (the open issue that will burn it).
//! The gate fails on
//!   * a failing cell that is not listed (a new bug — file it, list it),
//!   * a listed cell that passes (the fix landed — delete the row),
//!   * a listed cell that fails in a different set of columns than listed,
//!   * a row naming no generated cell (renamed axis — re-key the row).
//! Rows only shrink, except with an open issue cited
//! (`scripts/check-walled-real-growth.sh` with `LEDGER=` this file, in CI).
//!
//! Knobs (all optional):
//!   SHAPE_MATRIX_SLICE=k/N   run only the cells whose name hashes to k mod N
//!                            (the gate is still exact on the cells it ran)
//!   SHAPE_MATRIX_FILTER=sub  run only cells whose name contains `sub`
//!   SHAPE_MATRIX_JOBS=n      worker threads (default: min(cores, 4))
//!   SHAPE_MATRIX_DIR=path    where cell projects are written (kept on failure)
//!   SHAPE_MATRIX_LIST=1      print every cell name and exit
//!   SHAPE_MATRIX_RESULTS=f   also write every cell's verdict to `f`
//!                            (`<cell>\t<columns>\t<detail>\t<interp vote>`, one line per
//!                            cell, detail newlines as ` | ` — the input a
//!                            baseline refresh starts from)
//!
//! Every program run carries a timeout; a run that outlives it is a `*-hang`
//! column, never a pass. The column vocabulary is [`COLUMNS`].

#[path = "shape_matrix/gen.rs"]
mod shapes;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const BASELINE: &str = "proofs/shape-matrix-baseline.txt";

/// Budgets. A compile is not the program: rustc under a loaded CI runner can
/// take seconds, so the build budget is generous; the programs themselves are
/// a handful of statements.
const CHECK_SECS: u64 = 60;
const BUILD_SECS: u64 = 240;
const RUN_SECS: u64 = 10;
const INTERP_SECS: u64 = 10;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

struct Proc {
    ok: bool,
    code: Option<i32>,
    stdout: String,
    stderr: String,
    timed_out: bool,
}

/// Spawn `cmd`, drain both pipes on threads, kill it at the deadline.
fn run_timed(mut cmd: Command, secs: u64) -> Proc {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.env("NO_COLOR", "1");
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Proc { ok: false, code: None, stdout: String::new(), stderr: format!("spawn failed: {e}"), timed_out: false }
        }
    };
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let to = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = out.read_to_end(&mut b);
        b
    });
    let te = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = err.read_to_end(&mut b);
        b
    });
    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                timed_out = true;
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => break None,
        }
    };
    let stdout = String::from_utf8_lossy(&to.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&te.join().unwrap_or_default()).into_owned();
    let code = status.and_then(|s| s.code());
    Proc { ok: code == Some(0) && !timed_out, code, stdout, stderr, timed_out }
}

/// The verdict columns. Each leg is judged ALONE against the cell's expected
/// stdout, and BOTH legs are always run — a native failure must not hide a
/// wasm one behind it — so a cell's verdict is the `+`-joined set of the
/// columns it fails, in this order:
///
/// | column         | meaning                                                         |
/// |----------------|-----------------------------------------------------------------|
/// | `check`        | `almide check` rejected it: a generator bug (nothing else runs) |
/// | `native-build` | `almide build` failed (rustc error, ICE, refusal)                |
/// | `native-run`   | the binary exited non-zero or printed other bytes               |
/// | `native-hang`  | the native build or run outlived its budget                      |
/// | `wasm-build`   | `almide build --target wasm` failed (a wall counts: check accepted) |
/// | `wasm-run`     | wasmtime exited non-zero or printed other bytes                 |
/// | `wasm-hang`    | the wasm build or run outlived its budget                        |
/// | `interp`       | the interp ran it (did not abstain) and printed other bytes     |
const COLUMNS: [&str; 8] =
    ["check", "native-build", "native-run", "native-hang", "wasm-build", "wasm-run", "wasm-hang", "interp"];

struct Verdict {
    /// The failed columns, `+`-joined in [`COLUMNS`] order; empty = pass.
    columns: String,
    detail: String,
    interp_voted: bool,
}

fn first_lines(s: &str, n: usize) -> String {
    let err_lines: Vec<&str> = s.lines().filter(|l| l.contains("error") || l.contains("wall:")).take(n).collect();
    if !err_lines.is_empty() {
        return err_lines.join("\n");
    }
    s.lines().take(n).collect::<Vec<_>>().join("\n")
}

/// The interp's vote: `Some(stdout)` when it ran the program to completion,
/// `None` when it abstained (unsupported shape, a package layout it cannot
/// resolve, a panic, or its own wall clock).
fn interp_vote(entry: &Path) -> Option<String> {
    let entry = entry.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let r = std::panic::catch_unwind(|| {
            let source = std::fs::read_to_string(&entry).ok()?;
            let ir = almide::wasm_leg::lower_to_ir(entry.to_str()?, &source).ok()?;
            let out = almide_interp::Interpreter::new(&ir).run_main();
            (out.status == almide_interp::RunStatus::Ok).then_some(out.stdout)
        });
        let _ = tx.send(r.ok().flatten());
    });
    rx.recv_timeout(Duration::from_secs(INTERP_SECS)).ok().flatten()
}

/// Build + run one leg: `None` when it printed the expected bytes and exited
/// 0, else `(column, detail)`.
fn leg(
    target: &str,
    build: Proc,
    run: impl FnOnce() -> Proc,
    expected: &str,
) -> Option<(String, String)> {
    if build.timed_out {
        return Some((format!("{target}-hang"), format!("{target} build timed out")));
    }
    if !build.ok {
        return Some((format!("{target}-build"), first_lines(&build.stderr, 6)));
    }
    let r = run();
    if r.timed_out {
        return Some((format!("{target}-hang"), format!("{target} run timed out after {RUN_SECS}s")));
    }
    if !r.ok || r.stdout != expected {
        return Some((
            format!("{target}-run"),
            format!("{target} exit {:?} stdout {:?} (expected {expected:?})\n{}", r.code, r.stdout, first_lines(&r.stderr, 4)),
        ));
    }
    None
}

fn judge(cell: &shapes::Cell, dir: &Path, scratch: &Path, wasm: bool) -> Verdict {
    let almide = almide_bin();
    for (rel, text) in &cell.files {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
    }
    let entry = dir.join(&cell.entry);
    // A project is built from its root (the dir holding `almide.toml`).
    let cwd = if cell.entry.contains("src/") { entry.parent().unwrap().parent().unwrap().to_path_buf() } else { dir.to_path_buf() };
    let almide_cmd = |args: &[&str]| {
        let mut c = Command::new(&almide);
        c.args(args).current_dir(&cwd).env("ALMIDE_RUN_PROJECT_DIR", scratch);
        c
    };
    let entry_s = entry.to_string_lossy().into_owned();

    let check = run_timed(almide_cmd(&["check", &entry_s]), CHECK_SECS);
    if !check.ok {
        let why = if check.timed_out { "almide check timed out".to_string() } else { first_lines(&check.stderr, 6) };
        return Verdict { columns: "check".into(), detail: why, interp_voted: false };
    }

    let mut fails: Vec<(String, String)> = Vec::new();
    let bin = dir.join("native.bin");
    let nb = run_timed(almide_cmd(&["build", &entry_s, "-o", &bin.to_string_lossy()]), BUILD_SECS);
    fails.extend(leg("native", nb, || run_timed(Command::new(&bin), RUN_SECS), &cell.expected));
    if wasm {
        let wfile = dir.join("out.wasm");
        let wb = run_timed(almide_cmd(&["build", &entry_s, "--target", "wasm", "-o", &wfile.to_string_lossy()]), BUILD_SECS);
        let run_w = || {
            let mut wc = Command::new("wasmtime");
            wc.arg(&wfile);
            run_timed(wc, RUN_SECS)
        };
        fails.extend(leg("wasm", wb, run_w, &cell.expected));
    }
    let vote = interp_vote(&entry);
    if let Some(v) = &vote {
        if v != &cell.expected {
            fails.push(("interp".into(), format!("interp stdout {v:?} (expected {:?})", cell.expected)));
        }
    }
    fails.sort_by_key(|(c, _)| COLUMNS.iter().position(|k| k == c));
    Verdict {
        columns: fails.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>().join("+"),
        detail: fails.iter().map(|(c, d)| format!("[{c}] {d}")).collect::<Vec<_>>().join("\n"),
        interp_voted: vote.is_some(),
    }
}

/// `cell -> (column, issue)` from the baseline.
fn read_baseline() -> BTreeMap<String, (String, u32)> {
    let text = std::fs::read_to_string(repo_root().join(BASELINE)).unwrap_or_default();
    let mut rows = BTreeMap::new();
    for (i, line) in text.lines().enumerate() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let (key, cite) = t.split_once("# #").unwrap_or_else(|| {
            panic!("{BASELINE}:{}: a row must end in `  # #NNNN` naming the open issue that owns it: {t}", i + 1)
        });
        let issue: u32 = cite.trim().parse().unwrap_or_else(|_| panic!("{BASELINE}:{}: bad issue number in {t}", i + 1));
        let mut parts = key.split_whitespace();
        let (Some(cell), Some(col), None) = (parts.next(), parts.next(), parts.next()) else {
            panic!("{BASELINE}:{}: a row is `<cell>  <column>  # #NNNN`: {t}", i + 1)
        };
        assert!(rows.insert(cell.to_string(), (col.to_string(), issue)).is_none(), "{BASELINE}:{}: duplicate row {cell}", i + 1);
    }
    rows
}

#[test]
fn every_generated_cell_has_a_unique_stable_name() {
    let cells = shapes::all_cells();
    let mut seen = BTreeSet::new();
    for c in &cells {
        assert!(seen.insert(c.name.clone()), "duplicate cell name {}", c.name);
        assert!(c.name.chars().all(|ch| ch.is_ascii_alphanumeric() || "_./".contains(ch)), "cell name {}", c.name);
    }
}

#[test]
fn every_baseline_row_names_a_generated_cell() {
    let names: BTreeSet<String> = shapes::all_cells().into_iter().map(|c| c.name).collect();
    for (cell, (col, _)) in read_baseline() {
        assert!(names.contains(&cell), "{BASELINE}: row `{cell}` names no generated cell — re-key or delete it");
        assert!(
            col.split('+').all(|c| COLUMNS.contains(&c)),
            "{BASELINE}: row `{cell}` has unknown column `{col}` (columns: {COLUMNS:?}, `+`-joined)"
        );
    }
}

#[test]
fn shape_matrix() {
    let all = shapes::all_cells();
    if std::env::var("SHAPE_MATRIX_LIST").is_ok() {
        for c in &all {
            println!("{}", c.name);
        }
        return;
    }
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        eprintln!("shape-matrix: no almide binary — skipped");
        return;
    }
    let wasm = Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success());
    if !wasm {
        assert!(std::env::var("ALMIDE_EXPECT_TOOLS").is_err(), "shape-matrix: wasmtime missing on a leg that claims full tools");
        eprintln!("shape-matrix: wasmtime not on PATH — the wasm columns are not judged here (CI installs it)");
    }

    let slice = std::env::var("SHAPE_MATRIX_SLICE").ok().map(|s| {
        let (k, n) = s.split_once('/').expect("SHAPE_MATRIX_SLICE=k/N");
        (k.parse::<u64>().unwrap(), n.parse::<u64>().unwrap())
    });
    let filter = std::env::var("SHAPE_MATRIX_FILTER").ok();
    let cells: Vec<shapes::Cell> = all
        .into_iter()
        .filter(|c| slice.is_none_or(|(k, n)| shapes::name_hash(&c.name) % n == k))
        .filter(|c| filter.as_ref().is_none_or(|f| c.name.contains(f.as_str())))
        .collect();

    let jobs: usize = std::env::var("SHAPE_MATRIX_JOBS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).min(4));
    let root = std::env::var("SHAPE_MATRIX_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join(format!("almide-shape-matrix-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let started = Instant::now();
    let queue = Arc::new(Mutex::new(cells.iter().cloned().enumerate().collect::<Vec<_>>()));
    let results: Arc<Mutex<Vec<(String, Option<(String, String)>, bool, PathBuf)>>> = Arc::new(Mutex::new(Vec::new()));
    let handles: Vec<_> = (0..jobs)
        .map(|w| {
            let queue = queue.clone();
            let results = results.clone();
            let root = root.clone();
            std::thread::spawn(move || {
                // Each worker builds in its own scratch dir, so the native
                // builds do not serialize on the shared build-dir lock.
                let scratch = root.join(format!(".scratch-{w}"));
                std::fs::create_dir_all(&scratch).unwrap();
                loop {
                    let Some((i, cell)) = queue.lock().unwrap().pop() else { break };
                    let dir = root.join(format!("{i:04}-{}", cell.name.replace('/', "-")));
                    let v = judge(&cell, &dir, &scratch, wasm);
                    if v.columns.is_empty() {
                        let _ = std::fs::remove_dir_all(&dir);
                    }
                    let fail = (!v.columns.is_empty()).then_some((v.columns, v.detail));
                    results.lock().unwrap().push((cell.name.clone(), fail, v.interp_voted, dir));
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let mut results = Arc::try_unwrap(results).ok().unwrap().into_inner().unwrap();
    results.sort_by(|a, b| a.0.cmp(&b.0));

    if let Ok(path) = std::env::var("SHAPE_MATRIX_RESULTS") {
        let rows: String = results
            .iter()
            .map(|(name, fail, voted, _)| {
                let vote = if *voted { "interp-voted" } else { "interp-abstained" };
                match fail {
                    None => format!("{name}\tpass\t\t{vote}\n"),
                    Some((col, why)) => format!("{name}\t{col}\t{}\t{vote}\n", why.replace('\n', " | ")),
                }
            })
            .collect();
        std::fs::write(&path, rows).expect("write SHAPE_MATRIX_RESULTS");
    }
    let baseline = read_baseline();
    let mut problems = Vec::new();
    let (mut pass, mut known, mut voted) = (0, 0, 0);
    let mut by_col: BTreeMap<String, usize> = BTreeMap::new();
    for (name, fail, interp_voted, dir) in &results {
        if *interp_voted {
            voted += 1;
        }
        match (fail, baseline.get(name)) {
            (None, None) => pass += 1,
            (None, Some((col, issue))) => problems.push(format!(
                "PASSES BUT LISTED {name} (listed {col} under #{issue}) — the fix landed: delete its row from {BASELINE}"
            )),
            (Some((col, _)), Some((lcol, issue))) if col == lcol => {
                known += 1;
                *by_col.entry(col.clone()).or_default() += 1;
                eprintln!("known-open {name} [{lcol}] #{issue}");
                let _ = std::fs::remove_dir_all(dir);
            }
            (Some((col, why)), Some((lcol, issue))) => problems.push(format!(
                "FAIL {name} [{col}] but listed [{lcol}] under #{issue} — the failure moved: re-list it (same issue if the cause is the same)\n  project: {}\n{}",
                dir.display(),
                indent2(why)
            )),
            (Some((col, why)), None) => {
                *by_col.entry(col.clone()).or_default() += 1;
                problems.push(format!("FAIL {name} [{col}]\n  project: {}\n{}", dir.display(), indent2(why)))
            }
        }
    }
    for (col, n) in &by_col {
        eprintln!("shape-matrix:   {n:5} failing [{col}]");
    }
    eprintln!(
        "shape-matrix: {} cells run ({} generated), {pass} pass, {known} known-open, {} new/moved/stale; interp voted on {voted}; {jobs} jobs, {:.1}s",
        results.len(),
        shapes::all_cells().len(),
        problems.len(),
        started.elapsed().as_secs_f64()
    );
    if problems.is_empty() {
        let _ = std::fs::remove_dir_all(&root);
    }
    assert!(
        problems.is_empty(),
        "shape matrix: {} problem(s) — an accepted program one leg cannot build or run is a compiler bug. \
         File it, then list the cell in {BASELINE} as `<cell>  <columns>  # #NNNN`:\n\n{}",
        problems.len(),
        problems.join("\n\n")
    );
}

fn indent2(s: &str) -> String {
    s.lines().map(|l| format!("    {l}")).collect::<Vec<_>>().join("\n")
}
