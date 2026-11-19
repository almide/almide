//! The route matrix of the decision-site ledger (proofs/decision-sites.toml),
//! condition c3: a decision runs on EVERY route that produces or runs code.
//!
//! The generalisation of tests/permissions_routes_test.rs. Each row that
//! names a `probe` points at `tests/decision-probes/<id>/`: a project
//! (`proj/almide.toml`, `proj/src/main.almd`, `proj/src/main_test.almd`) the
//! site must refuse, and `probe.toml` saying what the refusal looks like:
//!
//!   expect = "E006"          a substring every refusing route prints (for LSP
//!                            and MCP: a substring of the diagnostics they report)
//!   exit   = "fail"          "fail" (default) | "any" — a switch the lock ignores
//!                            leaves a program that may well succeed
//!   args   = [...]           appended after the route's own arguments
//!   global = [...]           before the subcommand (`--lock-switches`)
//!   env    = { NAME = "1" }  set for the route's process
//!   main / test              the files the routes take, relative to `proj/`
//!                            (default `src/main.almd`, `src/main_test.almd`)
//!   [argv] <column> = [...]  replaces a column's default command line
//!   [expect_by] <column> = "" replaces `expect` for one column (the test
//!                            route reports a passing refusal-asserting test)
//!   [mcp] <key> = "..."      extra `almide_check` arguments (`target = "wasm"`)
//!
//! For every route cell the row marks `runs`, the probe is driven from INSIDE
//! the project (cwd = proj, `src/main.almd`) and from OUTSIDE it (cwd = its
//! parent, `proj/src/main.almd` — the reader-disagreement shape of #4830 and
//! #4835), and each must refuse. `lsp` drives `almide lsp` over stdio to the
//! file's publishDiagnostics; `mcp` drives `almide mcp`'s `almide_check`.
//!
//! `observe_every_probe_on_every_route` (ignored; run it with `--ignored`)
//! drives every probe on all fourteen columns, whatever the row says, and
//! prints the matrix that actually happens — the instrument the ledger's cells
//! are corrected from.
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const LEDGER: &str = "proofs/decision-sites.toml";

const COLUMNS: [&str; 14] = [
    "check", "check_json", "check_effects", "run", "build", "test", "bench", "native", "wasm", "wasi", "component", "lsp",
    "mcp", "emit",
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn almide() -> String {
    std::env::var("ALMIDE_CONTROL_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

#[derive(Clone)]
struct Probe {
    id: String,
    dir: PathBuf,
    expect: String,
    exit_any: bool,
    args: Vec<String>,
    global: Vec<String>,
    env: Vec<(String, String)>,
    argv: BTreeMap<String, Vec<String>>,
    main: String,
    test: String,
    expect_by: BTreeMap<String, String>,
    mcp_args: BTreeMap<String, String>,
}

fn strs(v: Option<&toml::Value>) -> Vec<String> {
    v.and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

fn load_probe(id: &str, dir: &Path) -> Probe {
    let text = std::fs::read_to_string(dir.join("probe.toml")).unwrap_or_else(|e| panic!("{}: probe.toml: {e}", dir.display()));
    let t: toml::Table = toml::from_str(&text).unwrap_or_else(|e| panic!("{}: probe.toml: {e}", dir.display()));
    let expect = t.get("expect").and_then(|v| v.as_str()).unwrap_or("").to_string();
    assert!(!expect.is_empty(), "{id}: probe.toml has no `expect`");
    Probe {
        id: id.to_string(),
        dir: dir.to_path_buf(),
        expect,
        exit_any: t.get("exit").and_then(|v| v.as_str()) == Some("any"),
        args: strs(t.get("args")),
        global: strs(t.get("global")),
        env: t
            .get("env")
            .and_then(|v| v.as_table())
            .map(|e| e.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect())
            .unwrap_or_default(),
        argv: t
            .get("argv")
            .and_then(|v| v.as_table())
            .map(|a| a.iter().map(|(k, v)| (k.clone(), strs(Some(v)))).collect())
            .unwrap_or_default(),
        main: t.get("main").and_then(|v| v.as_str()).unwrap_or("src/main.almd").to_string(),
        test: t.get("test").and_then(|v| v.as_str()).unwrap_or("src/main_test.almd").to_string(),
        expect_by: t
            .get("expect_by")
            .and_then(|v| v.as_table())
            .map(|a| a.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect())
            .unwrap_or_default(),
        mcp_args: t
            .get("mcp")
            .and_then(|v| v.as_table())
            .map(|a| a.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect())
            .unwrap_or_default(),
    }
}

/// `(row id, probe, route cells)` for every row that names a probe.
fn rows() -> Vec<(Probe, BTreeMap<String, String>)> {
    let text = std::fs::read_to_string(root().join(LEDGER)).expect("ledger");
    let t: toml::Table = toml::from_str(&text).expect("ledger parses");
    let mut out = Vec::new();
    for r in t.get("site").and_then(|v| v.as_array()).into_iter().flatten() {
        let Some(p) = r.get("probe").and_then(|v| v.as_str()) else { continue };
        let id = r.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        // `DECISION_PROBES=a,b` narrows the observer to the named rows.
        if let Ok(only) = std::env::var("DECISION_PROBES") {
            if !only.split(',').any(|o| o == id) {
                continue;
            }
        }
        let cells = r
            .get("routes")
            .and_then(|v| v.as_table())
            .map(|c| c.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect())
            .unwrap_or_default();
        out.push((load_probe(&id, &root().join(p)), cells));
    }
    out
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        let dest = to.join(e.file_name());
        // A probe's symlink is part of its shape (the beneath walk's): keep it a link.
        #[cfg(unix)]
        if e.file_type().is_ok_and(|t| t.is_symlink()) {
            std::os::unix::fs::symlink(std::fs::read_link(&p).unwrap(), &dest).unwrap();
            continue;
        }
        if p.is_dir() {
            copy_dir(&p, &dest);
        } else {
            std::fs::copy(&p, &dest).unwrap();
        }
    }
}

/// Where a route is driven from: inside the project, or from its parent.
#[derive(Clone, Copy, PartialEq)]
enum Side {
    Inside,
    Outside,
}

struct Place {
    cwd: PathBuf,
    main: String,
    test: String,
    out: String,
    proj: PathBuf,
}

fn place(p: &Probe, work: &Path, side: Side) -> Place {
    let proj = work.join("proj");
    match side {
        Side::Inside => Place {
            cwd: proj.clone(),
            main: p.main.clone(),
            test: p.test.clone(),
            out: work.join("out-in").to_string_lossy().into_owned(),
            proj,
        },
        Side::Outside => Place {
            cwd: work.to_path_buf(),
            main: format!("proj/{}", p.main),
            test: format!("proj/{}", p.test),
            out: work.join("out-out").to_string_lossy().into_owned(),
            proj,
        },
    }
}

fn default_argv(col: &str) -> Vec<&'static str> {
    match col {
        "check" => vec!["check", "{main}"],
        "check_json" => vec!["check", "--json", "{main}"],
        "check_effects" => vec!["check", "--effects", "{main}"],
        "run" => vec!["run", "{main}"],
        "build" => vec!["build", "{main}", "-o", "{out}"],
        "test" => vec!["test", "{test}"],
        "bench" => vec!["bench", "{main}"],
        "native" => vec!["run", "{main}", "--target", "rust"],
        "wasm" => vec!["run", "{main}", "--target", "wasm"],
        "wasi" => vec!["build", "{main}", "--target", "wasi", "-o", "{out}.wasm"],
        "component" => vec!["build", "{main}", "--target", "wasm", "--component", "-o", "{out}.wasm"],
        "emit" => vec!["emit", "{main}", "--target", "rust"],
        other => panic!("no command line for column {other}"),
    }
}

/// The verdict of one cell: `Ok(())` refused as the probe says, `Err(why)` admitted.
fn drive(p: &Probe, col: &str, side: Side) -> Result<(), String> {
    let work = tempfile::TempDir::new().unwrap();
    copy_dir(&p.dir.join("proj"), &work.path().join("proj"));
    let at = place(p, work.path(), side);
    let text = match col {
        "lsp" => lsp(p, &at)?,
        "mcp" => mcp(p, &at)?,
        _ => {
            let template: Vec<String> = match p.argv.get(col) {
                Some(a) => a.clone(),
                None => default_argv(col).into_iter().map(str::to_string).collect(),
            };
            let sub = |s: &str| s.replace("{main}", &at.main).replace("{test}", &at.test).replace("{out}", &at.out);
            let mut args: Vec<String> = p.global.clone();
            args.extend(template.iter().map(|s| sub(s)));
            args.extend(p.args.iter().map(|s| sub(s)));
            let mut cmd = Command::new(almide());
            cmd.args(&args).current_dir(&at.cwd).stdin(Stdio::null());
            for (k, v) in &p.env {
                cmd.env(k, v);
            }
            let o = cmd.output().map_err(|e| format!("spawn: {e}"))?;
            let t = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
            if !p.exit_any && o.status.success() {
                return Err(format!("`almide {}` exited 0:\n{}", args.join(" "), tail(&t)));
            }
            t
        }
    };
    let expect = p.expect_by.get(col).unwrap_or(&p.expect);
    if text.contains(expect.as_str()) {
        Ok(())
    } else {
        Err(format!("no `{expect}` in the output:\n{}", tail(&text)))
    }
}

fn tail(t: &str) -> String {
    let lines: Vec<&str> = t.lines().collect();
    lines[lines.len().saturating_sub(12)..].join("\n")
}

fn file_uri(p: &Path) -> String {
    let s = p.display().to_string().replace('\\', "/");
    if s.starts_with('/') { format!("file://{s}") } else { format!("file:///{s}") }
}

/// `almide lsp` over stdio: the diagnostics published for the probe's main file.
fn lsp(p: &Probe, at: &Place) -> Result<String, String> {
    let main = at.cwd.join(&at.main).canonicalize().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(almide());
    cmd.args(&p.global).arg("lsp").current_dir(&at.cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in &p.env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().map_err(|e| format!("spawn lsp: {e}"))?;
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    // The server's stderr: where a refusal that is not a diagnostic (a switch
    // the lock ignores) is said.
    let log = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    {
        let log = log.clone();
        let mut stderr = child.stderr.take().unwrap();
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = stderr.read_to_string(&mut s);
            log.lock().unwrap().push_str(&s);
        });
    }
    let (tx, rx) = std::sync::mpsc::channel::<serde_json::Value>();
    std::thread::spawn(move || {
        let mut r = BufReader::new(stdout);
        loop {
            let mut len = None;
            loop {
                let mut h = String::new();
                if r.read_line(&mut h).unwrap_or(0) == 0 {
                    return;
                }
                let h = h.trim();
                if h.is_empty() {
                    break;
                }
                if let Some(v) = h.strip_prefix("Content-Length:") {
                    len = v.trim().parse::<usize>().ok();
                }
            }
            let Some(n) = len else { return };
            let mut buf = vec![0u8; n];
            if r.read_exact(&mut buf).is_err() {
                return;
            }
            if let Ok(v) = serde_json::from_slice(&buf) {
                if tx.send(v).is_err() {
                    return;
                }
            }
        }
    });
    let mut send = |v: serde_json::Value| {
        let b = v.to_string();
        let _ = write!(stdin, "Content-Length: {}\r\n\r\n{}", b.len(), b);
        let _ = stdin.flush();
    };
    let uri = file_uri(&main);
    let text = std::fs::read_to_string(&main).unwrap_or_default();
    send(serde_json::json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"processId":null,"capabilities":{},"rootUri":file_uri(&at.proj.canonicalize().unwrap_or(at.proj.clone()))}}));
    send(serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
    send(serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"almide","version":1,"text":text}}}));
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut result = Err("no publishDiagnostics for the file within 90s".to_string());
    while Instant::now() < deadline {
        let Ok(m) = rx.recv_timeout(Duration::from_secs(5)) else { continue };
        if m["method"] == "textDocument/publishDiagnostics" && m["params"]["uri"] == uri.as_str() {
            result = Ok(m["params"]["diagnostics"].to_string());
            break;
        }
    }
    send(serde_json::json!({"jsonrpc":"2.0","id":9,"method":"shutdown"}));
    send(serde_json::json!({"jsonrpc":"2.0","method":"exit"}));
    drop(send);
    drop(stdin);
    // Give the server the shutdown it asked for before reading its stderr.
    let stop = Instant::now() + Duration::from_secs(10);
    while Instant::now() < stop && child.try_wait().ok().flatten().is_none() {
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    std::thread::sleep(Duration::from_millis(100));
    let stderr = log.lock().unwrap().clone();
    result.map(|d| format!("{d}\n{stderr}"))
}

/// `almide mcp` over stdio: the `almide_check` tool's payload for the main file.
fn mcp(p: &Probe, at: &Place) -> Result<String, String> {
    let main = at.cwd.join(&at.main);
    let mut cmd = Command::new(almide());
    cmd.args(&p.global).arg("mcp").current_dir(&at.cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in &p.env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().map_err(|e| format!("spawn mcp: {e}"))?;
    {
        let stdin = child.stdin.as_mut().unwrap();
        let init = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"decision_route_matrix","version":"1"}}});
        let mut arguments = serde_json::json!({"file":main.to_string_lossy()});
        for (k, v) in &p.mcp_args {
            arguments[k.as_str()] = serde_json::Value::String(v.clone());
        }
        let call = serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"almide_check","arguments":arguments}});
        let _ = writeln!(stdin, "{init}");
        let _ = writeln!(stdin, "{call}");
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    Ok(format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

fn sides() -> [Side; 2] {
    [Side::Inside, Side::Outside]
}

fn side_name(s: Side) -> &'static str {
    if s == Side::Inside { "inside" } else { "outside" }
}

#[test]
fn every_runs_cell_refuses_its_probe() {
    let rows = rows();
    let mut failures = Vec::new();
    let mut ran = 0;
    for (p, cells) in &rows {
        for col in COLUMNS {
            let cell = cells.get(col).map(String::as_str).unwrap_or("");
            if !cell.starts_with("runs") {
                continue;
            }
            for side in sides() {
                ran += 1;
                if let Err(why) = drive(p, col, side) {
                    failures.push(format!("{} / {col} / {}: ADMITTED — {why}", p.id, side_name(side)));
                }
            }
        }
    }
    eprintln!("decision route matrix: {} probes, {ran} cells driven", rows.len());
    assert!(failures.is_empty(), "a `runs` cell admitted its probe:\n{}", failures.join("\n\n"));
}

/// The instrument: every probe on every column, printed. `cargo test --test
/// decision_route_matrix_test -- --ignored --nocapture observe`.
#[test]
#[ignore]
fn observe_every_probe_on_every_route() {
    for (p, cells) in rows() {
        for col in COLUMNS {
            let v: Vec<String> = sides()
                .iter()
                .map(|s| match drive(&p, col, *s) {
                    Ok(()) => "refuses".to_string(),
                    Err(why) => format!("ADMITS ({})", why.lines().next().unwrap_or("")),
                })
                .collect();
            println!("{}\t{col}\t{}\t{}\tledger={}", p.id, v[0], v[1], cells.get(col).cloned().unwrap_or_default());
        }
    }
}
