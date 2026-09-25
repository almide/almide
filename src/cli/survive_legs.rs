//! The three legs `almide survive` runs on each side of an edit (#2147): the
//! reach scan that decides WHAT to run, and the check / test / contract runs.
//!
//! Every run is a child `almide` process (`current_exe`, the flags a human
//! would type), exactly as the MCP tools run it: the check leg reads the very
//! JSON `almide check --json` prints, so a field that output grows reaches the
//! caller untouched. The after-side child gets the source overlay
//! (`almide::source_overlay`) through its two switches; the before-side child
//! gets neither. A child is killed with its whole process group when it
//! outlives the timeout, so a hanging test binary cannot outlive the verdict.

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The after-side hand-off: which file is overlaid, and where its text is.
pub struct OverlayEnv {
    pub target: PathBuf,
    pub text_file: PathBuf,
}

/// One child run.
pub struct ChildRun {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

fn kill_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill")
            .args(["-KILL", &format!("-{}", child.id())])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    let _ = child.kill();
}

/// Run `almide <args>` with the overlay armed or not, bounded by `timeout`.
pub fn run_self(args: &[&str], overlay: Option<&OverlayEnv>, timeout: Duration) -> Result<ChildRun, String> {
    use std::io::Read;
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate the almide binary: {}", e))?;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args)
        .env_remove("ALMIDE_SURVIVE_OVERLAY")
        .env_remove("ALMIDE_SURVIVE_OVERLAY_TEXT")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(ov) = overlay {
        cmd.env("ALMIDE_SURVIVE_OVERLAY", &ov.target).env("ALMIDE_SURVIVE_OVERLAY_TEXT", &ov.text_file);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|e| format!("failed to run `almide {}`: {}", args.join(" "), e))?;
    let mut out_pipe = child.stdout.take().expect("piped stdout");
    let mut err_pipe = child.stderr.take().expect("piped stderr");
    let out_t = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = out_pipe.read_to_end(&mut s);
        String::from_utf8_lossy(&s).into_owned()
    });
    let err_t = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = err_pipe.read_to_end(&mut s);
        String::from_utf8_lossy(&s).into_owned()
    });
    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) if start.elapsed() >= timeout => {
                timed_out = true;
                kill_tree(&mut child);
                break child.wait().ok();
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(format!("waiting on `almide {}`: {}", args.join(" "), e)),
        }
    };
    Ok(ChildRun {
        code: status.and_then(|s| s.code()).unwrap_or(-1),
        stdout: out_t.join().unwrap_or_default(),
        stderr: err_t.join().unwrap_or_default(),
        timed_out,
    })
}

fn tail(s: &str, limit: usize) -> String {
    if s.len() <= limit {
        return s.to_string();
    }
    let start = s.len() - limit;
    let cut = s.char_indices().map(|(i, _)| i).find(|i| *i >= start).unwrap_or(s.len());
    s[cut..].to_string()
}

// ── reach ───────────────────────────────────────────────────────────────────

/// Does `text` carry a `test` block (the predicate `almide test` discovers by)?
pub fn has_tests(text: &str) -> bool {
    text.starts_with("test ") || text.contains("\ntest ")
}

/// The `C-NNN` ids a fixture declares on its `// @contract:` lines.
pub fn declared_contracts(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        let Some(rest) = t.strip_prefix("//").map(str::trim_start).and_then(|r| r.strip_prefix("@contract:")) else { continue };
        for tok in rest.split(',') {
            let tok = tok.trim();
            if tok.len() > 2 && tok.starts_with("C-") && tok[2..].chars().all(|c| c.is_ascii_digit()) && !ids.iter().any(|i| i == tok) {
                ids.push(tok.to_string());
            }
        }
    }
    ids
}

fn walk_almd(dir: &Path, out: &mut Vec<PathBuf>) {
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if (name.starts_with('.') && name != "." && name != "..") || name == "target" || name == "node_modules" {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            walk_almd(&p, out);
        } else if p.extension().is_some_and(|e| e == "almd") {
            out.push(p);
        }
    }
}

/// `./a/b.almd` → `a/b.almd`: the spelling a caller typed, not the walker's.
fn display_path(p: &Path) -> String {
    let s = p.to_string_lossy().to_string();
    s.strip_prefix("./").map(str::to_string).unwrap_or(s)
}

/// Does `file`'s import closure load `edited`? Parse errors and resolution
/// failures are a no: that file cannot reach anything on this side.
fn reaches(file: &Path, edited: &Path, deps: &[(almide::project::PkgId, PathBuf)]) -> bool {
    let Ok(src) = almide::source_overlay::read_to_string(file) else { return false };
    let tokens = almide::lexer::Lexer::tokenize(&src);
    let mut p = almide::parser::Parser::new(tokens);
    let Ok(program) = p.parse() else { return false };
    let file_s = file.to_string_lossy();
    match almide::resolve::resolve_imports_with_deps(&file_s, &program, deps) {
        Ok(r) => r.sources.values().any(|(path, _)| almide::source_overlay::canonical(Path::new(path)) == edited),
        Err(_) => false,
    }
}

/// The files an edit to `edited` can change the verdict of: the edited file
/// itself, then every `.almd` under `root` whose import closure loads it on
/// either side of the edit (an importer that fails to resolve against the
/// before-text may resolve against the after-text, and vice versa).
pub fn reach_set(root: &Path, edited_display: &str, edited: &Path, after_text: &str) -> Vec<String> {
    let deps = super::dep_paths_from_cwd_toml();
    let mut files = Vec::new();
    walk_almd(root, &mut files);
    let mut out = vec![edited_display.to_string()];
    for f in files {
        if almide::source_overlay::canonical(&f) == edited {
            continue;
        }
        almide::source_overlay::clear();
        let mut hit = reaches(&f, edited, &deps);
        if !hit {
            almide::source_overlay::set(edited, after_text.to_string());
            hit = reaches(&f, edited, &deps);
            almide::source_overlay::clear();
        }
        if hit {
            out.push(display_path(&f));
        }
    }
    out
}

// ── check leg ───────────────────────────────────────────────────────────────

/// `almide check <entry> --json` for every entry, into [`Diag`]s. A run that
/// printed no JSON and still failed (a total failure before the JSON path,
/// e.g. an imported module's own error) is recorded in `unstructured`, with
/// its stderr, rather than dropped — it cannot be paired, but it must not
/// read as clean.
pub fn check_leg(
    entries: &[String],
    edited: &Path,
    overlay: Option<&OverlayEnv>,
    timeout: Duration,
    side: &str,
    unstructured: &mut Vec<Value>,
) -> Result<Vec<super::survive_delta::Diag>, String> {
    let mut diags = Vec::new();
    for entry in entries {
        let run = run_self(&["check", entry, "--json"], overlay, timeout)?;
        let mut rows = 0usize;
        for line in run.stdout.lines().filter(|l| !l.trim().is_empty()) {
            match serde_json::from_str::<Value>(line) {
                Ok(v) if v.is_object() => {
                    rows += 1;
                    let file = v.get("file").and_then(|f| f.as_str()).filter(|f| !f.is_empty()).unwrap_or(entry).to_string();
                    let in_edited = almide::source_overlay::canonical(Path::new(&file)) == edited;
                    diags.push(super::survive_delta::Diag { checked: entry.clone(), file, in_edited, json: v });
                }
                _ => unstructured.push(json!({ "side": side, "checked": entry, "stdout_unstructured": line })),
            }
        }
        if rows == 0 && run.code != 0 {
            unstructured.push(json!({
                "side": side,
                "checked": entry,
                "exit_code": run.code,
                "timed_out": run.timed_out,
                "stderr_unstructured": tail(run.stderr.trim_end(), 4000),
            }));
        }
    }
    Ok(diags)
}

// ── test leg ────────────────────────────────────────────────────────────────

/// One test's outcome on one side, and its failure record when it failed.
pub struct TestOutcome {
    pub status: &'static str,
    pub failure: Option<Value>,
}

pub fn test_good(s: &str) -> bool {
    s == "pass" || s == "ignored"
}

/// The libtest verdict lines of a captured, single-threaded run:
/// `test <path> ... ok|FAILED|ignored`. A header with no verdict is the test
/// that took the process down (an assert abort exits the process under
/// capture), reported as `fail`.
fn verdicts(output: &str, exit_code: i32) -> Vec<(String, &'static str)> {
    let mut out: Vec<(String, &'static str)> = Vec::new();
    let mut pending: Option<String> = None;
    for line in output.lines() {
        let Some(rest) = line.strip_prefix("test ") else { continue };
        let Some((path, verdict)) = rest.split_once(" ... ") else { continue };
        if path.contains(' ') {
            continue;
        }
        match verdict.trim() {
            "ok" => out.push((path.to_string(), "pass")),
            "FAILED" => out.push((path.to_string(), "fail")),
            "ignored" => out.push((path.to_string(), "ignored")),
            _ => pending = Some(path.to_string()),
        }
    }
    if let Some(p) = pending
        && exit_code != 0
        && !out.iter().any(|(q, _)| *q == p)
    {
        out.push((p, "fail"));
    }
    out
}

/// Compile and run one test file on one side; every test the file's source
/// declares gets a status: `pass` / `fail` / `ignored`, `not_run` (the binary
/// stopped before reaching it), `compile_error`, or `timeout`.
pub fn test_leg(file: &str, source: &str, overlay: Option<&OverlayEnv>, timeout: Duration) -> Result<BTreeMap<String, TestOutcome>, String> {
    let names = super::test_report::test_name_map(source);
    let run = run_self(&["survive-test-leg", file], overlay, timeout)?;
    let leg: Option<Value> = run.stdout.lines().rev().find_map(|l| serde_json::from_str::<Value>(l).ok());
    let mut out: BTreeMap<String, TestOutcome> = BTreeMap::new();
    let all = |status: &'static str, out: &mut BTreeMap<String, TestOutcome>| {
        for (_, name) in &names {
            out.insert(name.clone(), TestOutcome { status, failure: None });
        }
    };
    if run.timed_out {
        all("timeout", &mut out);
        return Ok(out);
    }
    let Some(leg) = leg.filter(|l| l["compiled"] == true) else {
        all("compile_error", &mut out);
        return Ok(out);
    };
    let exit = leg["exit_code"].as_i64().unwrap_or(1) as i32;
    let output = leg["output"].as_str().unwrap_or("");
    all("not_run", &mut out);
    let failures = super::test_report::parse(file, source, output);
    for (path, status) in verdicts(output, exit) {
        let name = super::test_report::display_name(&names, &path);
        let failure = (status == "fail")
            .then(|| failures.iter().find(|f| f.name.as_deref() == Some(name.as_str())))
            .flatten()
            .and_then(|f| serde_json::from_str::<Value>(&f.to_json()).ok());
        out.insert(name, TestOutcome { status, failure });
    }
    Ok(out)
}

/// `almide survive-test-leg <file>` (hidden): compile the file's test harness
/// and run it captured and single-threaded, printing one JSON line —
/// `{compiled, exit_code, output}` or `{compiled: false, error}`. The parent
/// reads the verdicts from `output`; this process only exists so a compiler
/// `exit` or panic on the proposed text ends a child, not the verdict.
pub fn cmd_survive_test_leg(file: &str) {
    let args = vec!["--test-threads=1".to_string()];
    let line = match super::run::compile_to_binary(file, false, true, false, None) {
        Ok(bin) => {
            let (code, output) = super::run::run_binary_captured(&bin, &args);
            json!({ "compiled": true, "exit_code": code, "output": output })
        }
        Err(e) => json!({ "compiled": false, "error": e }),
    };
    crate::out(&line.to_string());
}

// ── contract leg ────────────────────────────────────────────────────────────

/// One contract fixture on one side: native `almide run` against
/// `almide run --target wasm`, stdout and exit code byte-for-byte — the
/// cross-target promise every `C-NNN` fixture carries. `holds` / `diverges` /
/// `timeout`.
pub fn contract_leg(fixture: &str, overlay: Option<&OverlayEnv>, timeout: Duration) -> Result<(&'static str, Value), String> {
    let native = run_self(&["run", fixture], overlay, timeout)?;
    let wasm = run_self(&["run", fixture, "--target", "wasm"], overlay, timeout)?;
    if native.timed_out || wasm.timed_out {
        return Ok(("timeout", json!({ "native_timed_out": native.timed_out, "wasm_timed_out": wasm.timed_out })));
    }
    if native.code == wasm.code && native.stdout == wasm.stdout {
        return Ok(("holds", json!({ "exit_code": native.code })));
    }
    let first_diff = native
        .stdout
        .lines()
        .zip(wasm.stdout.lines())
        .position(|(a, b)| a != b)
        .map(|i| i + 1)
        .or_else(|| (native.stdout.lines().count() != wasm.stdout.lines().count()).then(|| native.stdout.lines().count().min(wasm.stdout.lines().count()) + 1));
    Ok(("diverges", json!({ "native_exit": native.code, "wasm_exit": wasm.code, "first_differing_stdout_line": first_diff })))
}

pub fn contract_good(s: &str) -> bool {
    s == "holds"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_lines_are_read_and_an_unfinished_header_is_the_abort() {
        let out = "running 3 tests\ntest tests::__test_almd_a ... ok\ntest tests::__test_almd_b ... FAILED\ntest tests::__test_almd_c ... ";
        let v = verdicts(out, 1);
        assert_eq!(
            v,
            vec![
                ("tests::__test_almd_a".to_string(), "pass"),
                ("tests::__test_almd_b".to_string(), "fail"),
                ("tests::__test_almd_c".to_string(), "fail"),
            ]
        );
    }

    #[test]
    fn contract_headers_are_parsed_like_the_ledger_gate_reads_them() {
        assert_eq!(declared_contracts("// @contract: C-302, C-170\nfn main() -> Unit = ()\n"), vec!["C-302", "C-170"]);
        assert!(declared_contracts("// no contract here\n").is_empty());
    }

    #[test]
    fn test_discovery_uses_the_runner_predicate() {
        assert!(has_tests("test \"x\" { }"));
        assert!(has_tests("fn f() -> Int = 1\ntest \"x\" { }"));
        assert!(!has_tests("fn tester() -> Int = 1\n"));
    }
}
