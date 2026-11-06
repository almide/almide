//! The 2026 command set (#4590), end to end on the real binary:
//!
//! - every retired spelling is an error naming the new one, exit 2, with the
//!   same text as the one diagnostic of the `--json` envelope;
//! - a bare `almide` prints the grouped help and never reads stdin;
//! - every `--json` command prints the same envelope shape;
//! - `remove` undoes `add`, manifest and lock, byte for byte;
//! - `apply --dry-run` prints `survive`'s report, byte for byte.

use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-cli-reorg-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir scratch");
    dir
}

fn write(path: PathBuf, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, body).expect("write");
}

/// A tiny project: a library entry with one test, in `dir`.
fn project(dir: &Path) {
    write(dir.join("almide.toml"), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n");
    write(
        dir.join("src/main.almd"),
        "fn double(x: Int) -> Int = x * 2\n\neffect fn main() -> Unit = println(int.to_string(double(2)))\n\ntest \"double\" {\n  assert(double(2) == 4)\n}\n",
    );
}

fn run_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(almide())
        .current_dir(dir)
        .stdin(Stdio::null())
        .args(args)
        .output()
        .expect("spawn almide")
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// The envelope a `--json` run printed, after checking its shape: exactly one
/// line, exactly the four fields, `ok` agreeing with the exit code, and every
/// diagnostic carrying `level`, `code` and `message`.
fn envelope(o: &Output, what: &str) -> Value {
    let out = text(&o.stdout);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 1, "{what}: --json prints exactly one line\nstdout:\n{out}\nstderr:\n{}", text(&o.stderr));
    let v: Value = serde_json::from_str(lines[0]).unwrap_or_else(|e| panic!("{what}: not JSON ({e}): {out}"));
    let obj = v.as_object().unwrap_or_else(|| panic!("{what}: not an object: {out}"));
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["diagnostics", "ok", "result", "schema_version"], "{what}: {out}");
    assert_eq!(v["schema_version"], 1, "{what}");
    assert_eq!(v["ok"], o.status.code() == Some(0), "{what}: ok follows the exit code: {out}");
    for d in v["diagnostics"].as_array().unwrap_or_else(|| panic!("{what}: diagnostics is not an array")) {
        for k in ["level", "code", "message"] {
            assert!(d.get(k).is_some(), "{what}: diagnostic lacks `{k}`: {d}");
        }
    }
    if v["ok"] == false {
        assert!(
            v["diagnostics"].as_array().unwrap().iter().any(|d| d["level"] == "error"),
            "{what}: a failed run names its error: {out}"
        );
    }
    v
}

#[test]
fn every_retired_spelling_names_its_replacement() {
    let dir = scratch("retired");
    project(&dir);
    let cases: &[(&[&str], &str, &str)] = &[
        (&["survive", "src/main.almd", "--with", "x.patch"], "almide survive", "almide apply --dry-run"),
        (&["ide", "outline"], "almide ide outline", "almide query outline"),
        (&["ide", "doc", "list.map"], "almide ide doc", "almide query doc"),
        (&["ide", "stdlib-snapshot"], "almide ide stdlib-snapshot", "almide query stdlib"),
        (&["self-update"], "almide self-update", "almide upgrade"),
        (&["install", "github.com/almide/porta"], "almide install", "almide tool install"),
        (&["dep-path", "json"], "almide dep-path", "almide deps --path json"),
        (&["check", "--explain", "E001"], "almide check --explain", "almide explain E001"),
        (&["compile", "--json"], "almide compile --json", "almide query interface --json"),
        (&["compile", "--dry-run"], "almide compile --dry-run", "almide query interface"),
    ];
    for (args, old, new) in cases {
        let want = format!("error: `{old}` was removed (#4590); use `{new}`");
        let o = run_in(&dir, args);
        assert_eq!(o.status.code(), Some(2), "{args:?}: exit 2\n{}", text(&o.stderr));
        assert_eq!(text(&o.stderr).trim_end(), want, "{args:?}");
        assert!(o.stdout.is_empty(), "{args:?}: nothing on stdout");

        let mut json_args: Vec<&str> = args.to_vec();
        json_args.push("--json");
        let o = run_in(&dir, &json_args);
        assert_eq!(o.status.code(), Some(2), "{json_args:?}: exit 2 under --json too");
        let v = envelope(&o, &format!("{json_args:?}"));
        assert_eq!(v["ok"], false);
        let diags = v["diagnostics"].as_array().unwrap();
        assert_eq!(diags.len(), 1, "{json_args:?}");
        assert_eq!(diags[0]["message"], want.as_str(), "{json_args:?}: the same text as without --json");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_bare_almide_prints_grouped_help_and_never_reads_stdin() {
    // stdin stays open and is never written: a REPL would wait forever.
    let mut child = Command::new(almide())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn almide");
    let _stdin = child.stdin.take();
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().expect("wait") {
            break s;
        }
        if start.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            panic!("a bare `almide` is still running after 60s: it waits on stdin");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut out = String::new();
    child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
    assert_eq!(status.code(), Some(0), "{out}");
    let mut at = 0;
    for group in ["Daily:", "Dependencies:", "Agents:", "Toolchain:"] {
        let pos = out[at..].find(group).unwrap_or_else(|| panic!("`{group}` missing, or out of order:\n{out}"));
        at += pos;
    }
    for cmd in ["  repl", "  remove", "  apply", "  query", "  upgrade", "  tool"] {
        assert!(out.contains(cmd), "`{cmd}` missing:\n{out}");
    }
    for hidden in ["  survive", "  ide ", "  self-update", "  dep-path", "  docs-gen", "  budgets", "  compile", "  emit"] {
        assert!(!out.contains(hidden), "`{hidden}` must not be listed:\n{out}");
    }
}

#[test]
fn every_json_command_prints_the_same_envelope() {
    let dir = scratch("envelope");
    project(&dir);
    let cases: &[&[&str]] = &[
        &["check", "src/main.almd", "--json"],
        &["fmt", "src/main.almd", "--json"],
        &["fix", "src/main.almd", "--dry-run", "--json"],
        &["explain", "E001", "--json"],
        &["explain", "--list", "--json"],
        &["query", "outline", "src/main.almd", "--json"],
        &["query", "doc", "double", "--file", "src/main.almd", "--json"],
        &["query", "interface", "src/main.almd", "--json"],
        &["query", "stdlib", "--modules", "string", "--json"],
        &["deps", "--json"],
        &["deps", "--path", "nothing", "--json"],
        &["remove", "nothing", "--json"],
        &["update", "--json"],
        &["trust", "github.com/somebody", "--json"],
        &["build", "missing.almd", "--json"],
        &["bench", "missing.almd", "--json"],
        &["tool", "install", "./no-such-dir", "--json"],
        &["test", "src/main.almd", "--json"],
        // A usage error under --json is the envelope too.
        &["check", "--no-such-flag", "--json"],
    ];
    for args in cases {
        let o = run_in(&dir, args);
        let v = envelope(&o, &format!("{args:?}"));
        match args[0] {
            "check" if args[1] != "--no-such-flag" => {
                assert_eq!(v["ok"], true, "{args:?}: {v}");
                assert_eq!(v["result"]["errors"], 0, "{args:?}: {v}");
                assert!(v["result"]["warnings"].is_u64(), "{args:?}: {v}");
            }
            "explain" if args[1] == "E001" => assert_eq!(v["result"]["code"], "E001", "{v}"),
            "explain" => assert!(v["result"]["codes"].as_array().is_some_and(|a| !a.is_empty()), "{v}"),
            "query" if args[1] == "outline" => assert!(v["result"]["decls"].is_array() || v["result"].is_object(), "{v}"),
            "query" if args[1] == "interface" => assert!(v["result"].is_object(), "{v}"),
            "test" => assert!(v["result"]["files"].as_array().is_some_and(|a| a.len() == 1), "{v}"),
            "build" | "bench" | "tool" | "remove" => assert_eq!(v["ok"], false, "{args:?}: {v}"),
            _ => {}
        }
    }
    // Under --json, a check error is a structured row: code and span.
    write(dir.join("src/bad.almd"), "fn f() -> Int = \"s\"\n");
    let o = run_in(&dir, &["check", "src/bad.almd", "--json"]);
    let v = envelope(&o, "check bad");
    let row = &v["diagnostics"][0];
    assert!(row["code"].as_str().is_some_and(|c| c.starts_with('E')), "{v}");
    assert_eq!(row["line"], 1, "{v}");
    assert_eq!(v["result"]["errors"], v["diagnostics"].as_array().unwrap().iter().filter(|d| d["level"] == "error").count());
    let _ = std::fs::remove_dir_all(&dir);
}

// ── `remove` undoes `add` ─────────────────────────────────────────────────
// No network: github.com is mapped onto local bare repositories through
// git's `url.<base>.insteadOf`, as tests/dependency_trust_test.rs does.

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.email=t@example.com", "-c", "user.name=t", "-c", "init.defaultBranch=main"])
        .args(args)
        .output()
        .expect("spawn git");
    assert!(out.status.success(), "git {args:?}: {}", text(&out.stderr));
}

fn bare_repo(root: &Path, owner: &str, pkg: &str) {
    let work = root.join("work").join(owner).join(pkg);
    write(work.join("almide.toml"), &format!("[package]\nname = \"{pkg}\"\nversion = \"0.1.0\"\n"));
    write(work.join("src/mod.almd"), "fn value() -> Int = 1\n");
    git(&work, &["init", "-q"]);
    git(&work, &["add", "-A"]);
    git(&work, &["commit", "-qm", "init"]);
    git(&work, &["tag", "v0.1.0"]);
    let bare = root.join("hosts").join(owner);
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["clone", "-q", "--bare", work.to_str().unwrap(), pkg]);
}

fn almide_hosted(root: &Path, dir: &Path, args: &[&str]) -> Output {
    let base = format!("file://{}/", root.join("hosts").display());
    Command::new(almide())
        .current_dir(dir)
        .env("HOME", root.join("home"))
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", format!("url.{base}.insteadOf"))
        .env("GIT_CONFIG_VALUE_0", "https://github.com/")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .args(args)
        .output()
        .expect("spawn almide")
}

fn files(dir: &Path) -> (Option<String>, Option<String>) {
    (std::fs::read_to_string(dir.join("almide.toml")).ok(), std::fs::read_to_string(dir.join("almide.lock")).ok())
}

#[test]
fn remove_undoes_add_in_the_manifest_and_the_lock() {
    let root = scratch("remove");
    bare_repo(&root, "almide", "csv");
    bare_repo(&root, "almide", "svg");
    let dir = root.join("app");
    project(&dir);

    // From no dependency at all: back to the exact bytes, and no lock.
    let before = files(&dir);
    let o = almide_hosted(&root, &dir, &["add", "csv@v0.1.0"]);
    assert_eq!(o.status.code(), Some(0), "add: {}", text(&o.stderr));
    let added = files(&dir);
    assert_ne!(added, before, "add changed nothing");
    assert!(added.1.as_deref().is_some_and(|l| l.contains("csv = ")), "add locked csv: {added:?}");
    let o = almide_hosted(&root, &dir, &["remove", "csv"]);
    assert_eq!(o.status.code(), Some(0), "remove: {}", text(&o.stderr));
    assert_eq!(files(&dir), before, "remove after add is the identity (manifest, lock)");

    // Next to another dependency: that one's manifest line and lock entry stay.
    let o = almide_hosted(&root, &dir, &["add", "svg@v0.1.0"]);
    assert_eq!(o.status.code(), Some(0), "add svg: {}", text(&o.stderr));
    let with_svg = files(&dir);
    let o = almide_hosted(&root, &dir, &["add", "csv@v0.1.0"]);
    assert_eq!(o.status.code(), Some(0), "add csv: {}", text(&o.stderr));
    let o = almide_hosted(&root, &dir, &["remove", "csv", "--json"]);
    let v = envelope(&o, "remove --json");
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(files(&dir), with_svg, "remove csv leaves svg exactly as it was");

    // A name that is not a dependency is an error and writes nothing.
    let o = almide_hosted(&root, &dir, &["remove", "csv"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(text(&o.stderr).contains("`csv` is not in almide.toml"), "{}", text(&o.stderr));
    assert_eq!(files(&dir), with_svg);
    let _ = std::fs::remove_dir_all(&root);
}

// ── `apply --dry-run` is `survive` ────────────────────────────────────────
// The four golden deltas of tests/survive_test.rs were produced by `almide
// survive --json`. `apply --dry-run --json` must carry exactly that object
// in `result`, and the human report must be the bytes `survive` rendered
// from it — `survive_report` below is `survive`'s renderer, copied verbatim
// from src/cli/survive.rs as it stood before #4590.

const CASES: [(&str, &str, &str); 4] = [
    ("breaks_test", "calc.almd", "edit.patch"),
    ("breaks_diagnostic", "sum.almd", "edit.patch"),
    ("fixes_diagnostic", "shapes.almd", "edit.txt"),
    ("neutral", "greet.almd", "edit.patch"),
];

fn survive_copy(case: &str, tag: &str) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/survive").join(case);
    let dst = scratch(&format!("survive-{case}-{tag}"));
    for e in std::fs::read_dir(&src).expect("fixture dir") {
        let p = e.expect("entry").path();
        std::fs::copy(&p, dst.join(p.file_name().unwrap())).expect("copy fixture file");
    }
    dst
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

fn survive_report(r: &Value, allow_removed: bool) -> String {
    let mut lines: Vec<String> = Vec::new();
    let verdict = if r["survives"].as_bool().unwrap_or(false) { "survives" } else { "does NOT survive" };
    lines.push(format!("{}: the edit {}", r["file"].as_str().unwrap_or(""), verdict));
    let removed = ["tests", "contracts"].iter().any(|leg| r["summary"][leg]["removed"].as_u64().unwrap_or(0) > 0);
    if removed {
        lines.push(
            if allow_removed {
                "  removed tests / contracts are allowed (--allow-removed)"
            } else {
                "  a test or contract the edit removes counts against it — pass --allow-removed if the removal is deliberate"
            }
            .to_string(),
        );
    }
    for leg in ["check", "tests", "contracts"] {
        let s = &r["summary"][leg];
        lines.push(format!(
            "  {:<9} {} newly broken, {} newly fixed, {} unchanged, {} removed",
            format!("{}:", leg),
            s["newly_broken"], s["newly_fixed"], s["unchanged"], s["removed"]
        ));
    }
    for (bucket, label) in [("newly_broken", "newly broken"), ("newly_fixed", "newly fixed"), ("removed", "removed")] {
        for leg in ["check", "tests", "contracts"] {
            for e in r[leg][bucket].as_array().into_iter().flatten() {
                lines.push(format!("  {} [{}] {}", label, leg, describe(e, leg)));
            }
        }
    }
    lines.iter().map(|l| format!("{l}\n")).collect()
}

#[test]
fn apply_dry_run_gives_survives_report_byte_for_byte() {
    for (case, file, edit) in CASES {
        let dir = survive_copy(case, "dry");
        let expected: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("expected.json")).unwrap()).unwrap();
        let want_code = if expected["survives"] == true { 0 } else { 1 };

        let o = run_in(&dir, &["apply", file, "--with", edit, "--dry-run"]);
        assert_eq!(o.status.code(), Some(want_code), "{case}: exit code\n{}", text(&o.stderr));
        assert!(o.stdout.is_empty(), "{case}: the report is on stderr, as survive's was");
        assert_eq!(text(&o.stderr), survive_report(&expected, false), "{case}: the human report drifted from survive's");

        let o = run_in(&dir, &["apply", file, "--with", edit, "--dry-run", "--json"]);
        assert_eq!(o.status.code(), Some(want_code), "{case}: exit code under --json");
        let v = envelope(&o, case);
        assert_eq!(v["result"], expected, "{case}: result is survive's JSON report");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn apply_dry_run_never_writes_and_refuses_a_write_gate() {
    let (case, file, edit) = CASES[0];
    let dir = survive_copy(case, "nowrite");
    let before = std::fs::read(dir.join(file)).unwrap();
    let o = run_in(&dir, &["apply", file, "--with", edit, "--dry-run"]);
    assert!(matches!(o.status.code(), Some(0 | 1)), "{}", text(&o.stderr));
    assert_eq!(std::fs::read(dir.join(file)).unwrap(), before, "--dry-run wrote the file");
    for gate in ["--force", "--if-survives"] {
        let o = run_in(&dir, &["apply", file, "--with", edit, "--dry-run", gate]);
        assert_eq!(o.status.code(), Some(2), "--dry-run with {gate} is a usage error");
        assert_eq!(std::fs::read(dir.join(file)).unwrap(), before);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
