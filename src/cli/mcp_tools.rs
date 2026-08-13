//! The `almide mcp` tool catalog: what an agent may call, and how each call
//! reaches the compiler.
//!
//! **One path into the compiler.** Every tool runs THE CLI —
//! `std::env::current_exe()` with the flags a human would type — and reads the
//! machine-readable output the CLI already emits (`check --json`,
//! `test --json`, `fmt --check --json`, `ide outline --json`). There is
//! deliberately no second, MCP-only rendering of a diagnostic or a test result:
//! a parallel formatter drifts from the CLI's, and the drift is invisible
//! precisely because nobody reads the MCP output by eye.
//!
//! **Human text is never parsed.** Where a surface has no machine-readable
//! form (the test runner's per-assertion output, #1313), the raw text is passed
//! through verbatim in a field whose name says it is unstructured. Regex-
//! scraping a rendered diagnostic would reintroduce exactly the parsing step
//! this server exists to remove.
//!
//! **Read-only.** No tool here writes a source file. `fmt` is exposed only in
//! its `--check` form; the writing forms (`almide fmt`, `almide fix`) stay on
//! the CLI, where the edit is visible in the agent's own transcript.

use serde_json::{json, Value};

/// The captured result of one CLI invocation.
struct CliRun {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

/// Run the almide binary that is serving MCP (`current_exe`) with `args`.
///
/// Self-invocation is what makes "no second output path" true by construction:
/// the tool result is produced by the same build of the same compiler the user
/// would get from the shell, and a compiler panic on a malformed input kills a
/// subprocess instead of the session.
fn run_cli(args: &[String], cwd: Option<&str>) -> Result<CliRun, String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("cannot locate the almide binary: {}", e))?;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("failed to run `almide {}`: {}", args.join(" "), e))?;
    Ok(CliRun {
        exit_code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// Split NDJSON output into (parsed objects, lines that were not JSON).
///
/// The non-JSON remainder is never pattern-matched — it is handed back to the
/// caller as text so a tool can report it honestly rather than guess at it.
fn split_json_lines(text: &str) -> (Vec<Value>, Vec<&str>) {
    let mut parsed = Vec::new();
    let mut other = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(line) {
            Ok(v) if v.is_object() => parsed.push(v),
            _ => other.push(line),
        }
    }
    (parsed, other)
}

fn arg_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)?.as_str().map(|s| s.to_string())
}

fn require_str(args: &Value, key: &str) -> Result<String, String> {
    arg_str(args, key).ok_or_else(|| format!("missing required argument `{}` (string)", key))
}

fn arg_strs(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

/// Keep the TAIL of a long runner dump: failures print last, and an agent's
/// context is the scarce resource. Returns (text, truncated).
fn tail(text: &str, limit: usize) -> (String, bool) {
    if text.len() <= limit {
        return (text.to_string(), false);
    }
    let start = text.len() - limit;
    let cut = text
        .char_indices()
        .map(|(i, _)| i)
        .find(|i| *i >= start)
        .unwrap_or(text.len());
    (text[cut..].to_string(), true)
}

fn count_level(diags: &[Value], level: &str) -> usize {
    diags
        .iter()
        .filter(|d| d.get("level").and_then(|l| l.as_str()) == Some(level))
        .count()
}

/// Attach the CLI's stderr when it said something, labelled as the
/// unstructured channel it is.
fn with_stderr(obj: &mut Value, run: &CliRun) {
    if !run.stderr.trim().is_empty() {
        let (text, truncated) = tail(run.stderr.trim_end(), 4000);
        obj["stderr_unstructured"] = json!(text);
        if truncated {
            obj["stderr_truncated"] = json!(true);
        }
    }
}

// ── tools ──

/// `almide check --json` → the diagnostic list, verbatim.
///
/// Each element is the compiler's own JSON diagnostic:
/// `{level, code, message, hint, here, try, try_replace, context, file, line,
/// col, end_col, secondary}` — `try`/`try_replace` are the fix-it snippet and
/// the span it replaces.
fn tool_check(args: &Value) -> Result<Value, String> {
    let file = require_str(args, "file")?;
    let cwd = arg_str(args, "cwd");
    let run = run_cli(&["check".into(), file.clone(), "--json".into()], cwd.as_deref())?;
    let (diagnostics, unparsed) = split_json_lines(&run.stdout);
    let errors = count_level(&diagnostics, "error");
    let warnings = count_level(&diagnostics, "warning");
    let mut obj = json!({
        "file": file,
        "ok": errors == 0 && run.exit_code == 0,
        "errors": errors,
        "warnings": warnings,
        "diagnostics": diagnostics,
        "exit_code": run.exit_code,
    });
    if !unparsed.is_empty() {
        obj["stdout_unstructured"] = json!(unparsed.join("\n"));
    }
    with_stderr(&mut obj, &run);
    Ok(obj)
}

/// `almide test --json` → per-FILE pass/fail, plus the runner's raw text.
///
/// Per-TEST structure (name / expected / found / diff) does not exist yet
/// (#1313), so the runner's own output is returned verbatim under
/// `runner_output_unstructured` rather than parsed into a shape the CLI does
/// not actually promise.
fn tool_test(args: &Value) -> Result<Value, String> {
    let mut argv: Vec<String> = vec!["test".into()];
    if let Some(path) = arg_str(args, "path") {
        argv.push(path);
    }
    argv.push("--json".into());
    if let Some(filter) = arg_str(args, "run") {
        argv.push("--run".into());
        argv.push(filter);
    }
    let cwd = arg_str(args, "cwd");
    let run = run_cli(&argv, cwd.as_deref())?;
    let (rows, other) = split_json_lines(&run.stdout);
    let files: Vec<Value> = rows.into_iter().filter(|v| v.get("file").is_some()).collect();
    let passed = files.iter().filter(|v| v.get("status").and_then(|s| s.as_str()) == Some("pass")).count();
    let failed = files.len() - passed;
    let raw = format!("{}\n{}", other.join("\n"), run.stderr);
    let (raw, truncated) = tail(raw.trim(), 8000);
    Ok(json!({
        "ok": failed == 0 && run.exit_code == 0,
        "files_passed": passed,
        "files_failed": failed,
        "files": files,
        "exit_code": run.exit_code,
        "runner_output_unstructured": raw,
        "runner_output_truncated": truncated,
        "note": "per-file status is structured; per-test expected/found is not (#1313) — read runner_output_unstructured for failure detail",
    }))
}

/// `almide ide outline --json` / `almide ide stdlib-snapshot --json` → the API
/// surface of a file or a stdlib module.
fn tool_api(args: &Value) -> Result<Value, String> {
    let target = require_str(args, "target")?;
    let cwd = arg_str(args, "cwd");
    let mut argv: Vec<String> = vec!["ide".into()];
    if target == "@stdlib" {
        argv.push("stdlib-snapshot".into());
        if let Some(modules) = arg_str(args, "modules") {
            argv.push("--modules".into());
            argv.push(modules);
        }
    } else {
        argv.push("outline".into());
        argv.push(target.clone());
        if let Some(filter) = arg_str(args, "filter") {
            argv.push("--filter".into());
            argv.push(filter);
        }
    }
    argv.push("--json".into());
    let run = run_cli(&argv, cwd.as_deref())?;
    if run.exit_code != 0 {
        return Err(format!(
            "{}\n(hint: a file target must type-check first — call almide_check on it)",
            run.stderr.trim()
        ));
    }
    let outline: Value = serde_json::from_str(run.stdout.trim())
        .map_err(|e| format!("almide ide emitted output this server could not parse as JSON: {}", e))?;
    Ok(json!({ "target": target, "outline": outline }))
}

/// `almide explain <CODE>` → the diagnostic's reference page.
///
/// The payload is documentation, returned as the markdown the CLI prints; it is
/// not parsed into fields, because it does not have any.
fn tool_explain(args: &Value) -> Result<Value, String> {
    let code = require_str(args, "code")?;
    let run = run_cli(&["explain".into(), code.clone()], None)?;
    if run.exit_code != 0 {
        return Err(run.stderr.trim().to_string());
    }
    Ok(json!({ "code": code.to_uppercase(), "doc": run.stdout }))
}

/// `almide fmt --check --json` → which files are not formatted. Never writes.
fn tool_fmt_check(args: &Value) -> Result<Value, String> {
    let mut argv: Vec<String> = vec!["fmt".into(), "--check".into(), "--json".into()];
    argv.extend(arg_strs(args, "paths"));
    let cwd = arg_str(args, "cwd");
    let run = run_cli(&argv, cwd.as_deref())?;
    let report: Value = serde_json::from_str(run.stdout.trim()).map_err(|_| {
        format!(
            "almide fmt produced no JSON report (exit {}): {}",
            run.exit_code,
            run.stderr.trim()
        )
    })?;
    let mut obj = json!({ "report": report, "exit_code": run.exit_code });
    with_stderr(&mut obj, &run);
    Ok(obj)
}

/// Dispatch a `tools/call`. `Err` becomes an MCP tool error (`isError: true`),
/// which is what an agent should see for "the compiler said no" — as opposed to
/// a protocol error, which means the request itself was malformed.
pub fn call(name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "almide_check" => tool_check(args),
        "almide_test" => tool_test(args),
        "almide_api" => tool_api(args),
        "almide_explain" => tool_explain(args),
        "almide_fmt_check" => tool_fmt_check(args),
        _ => Err(format!(
            "unknown tool `{}` — call tools/list for the catalog",
            name
        )),
    }
}

fn obj_schema(props: Value, required: Value) -> Value {
    json!({ "type": "object", "properties": props, "required": required })
}

/// Read-only, no network, same answer for the same input.
fn read_only() -> Value {
    json!({ "readOnlyHint": true, "idempotentHint": true, "openWorldHint": false })
}

fn cwd_prop() -> Value {
    json!({ "type": "string", "description": "Directory to run in (defaults to the server's cwd). Use the project root so almide.toml is found." })
}

fn tool_check_def() -> Value {
    json!({
        "name": "almide_check",
        "title": "Type-check an Almide file",
        "description": "Type-check one .almd file and return the compiler's structured diagnostics: {level, code, message, hint, here, try, try_replace, file, line, col, end_col}. `try` is a copy-pasteable fix snippet and `try_replace` is the span it replaces. Nothing is written. Runs `almide check --json`.",
        "inputSchema": obj_schema(
            json!({
                "file": { "type": "string", "description": "Path to the .almd file to check" },
                "cwd": cwd_prop(),
            }),
            json!(["file"]),
        ),
        "annotations": read_only(),
    })
}

fn tool_test_def() -> Value {
    json!({
        "name": "almide_test",
        "title": "Run Almide tests",
        "description": "Run the .almd test blocks under a path and return per-file pass/fail plus the runner's raw output for failures. COMPILES AND EXECUTES the tests (writes build artifacts under target/, never edits sources) and can take minutes on a cold cache. Runs `almide test --json`.",
        "inputSchema": obj_schema(
            json!({
                "path": { "type": "string", "description": "File or directory (default: recursive from cwd)" },
                "run": { "type": "string", "description": "Only run tests whose name matches this substring" },
                "cwd": cwd_prop(),
            }),
            json!([]),
        ),
        "annotations": json!({ "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }),
    })
}

fn tool_api_def() -> Value {
    json!({
        "name": "almide_api",
        "title": "List the API surface of a module",
        "description": "List every public declaration with its exact signature — use this instead of guessing a stdlib name or grepping. `target` is a .almd path, `@stdlib/<module>` (e.g. @stdlib/string), or `@stdlib` for a snapshot of the core modules. Runs `almide ide outline --json` / `almide ide stdlib-snapshot --json`.",
        "inputSchema": obj_schema(
            json!({
                "target": { "type": "string", "description": "A .almd file path, `@stdlib/<module>`, or `@stdlib`" },
                "filter": { "type": "string", "description": "Keep only names containing this substring (file / single-module targets)" },
                "modules": { "type": "string", "description": "Comma-separated module list for the `@stdlib` snapshot (default: string,list,int,option,result,map,set)" },
                "cwd": cwd_prop(),
            }),
            json!(["target"]),
        ),
        "annotations": read_only(),
    })
}

fn tool_explain_def() -> Value {
    json!({
        "name": "almide_explain",
        "title": "Explain a diagnostic code",
        "description": "Return the reference page for a diagnostic code (e.g. E042) — what it means, why it fires, and the sanctioned fix. Pair it with the `code` field of an almide_check diagnostic. Runs `almide explain <CODE>`.",
        "inputSchema": obj_schema(
            json!({ "code": { "type": "string", "description": "Diagnostic code, e.g. E001" } }),
            json!(["code"]),
        ),
        "annotations": read_only(),
    })
}

fn tool_fmt_check_def() -> Value {
    json!({
        "name": "almide_fmt_check",
        "title": "Check formatting",
        "description": "Report which .almd files are not canonically formatted. Read-only: it never rewrites a file — run `almide fmt <path>` in a shell to apply. Runs `almide fmt --check --json`.",
        "inputSchema": obj_schema(
            json!({
                "paths": { "type": "array", "items": { "type": "string" }, "description": "Files or directories (default: src/**/*.almd)" },
                "cwd": cwd_prop(),
            }),
            json!([]),
        ),
        "annotations": read_only(),
    })
}

/// The `tools/list` catalog. Five tools, each one a surface the CLI already
/// answers in a machine-readable form.
pub fn catalog() -> Value {
    json!([
        tool_check_def(),
        tool_test_def(),
        tool_api_def(),
        tool_explain_def(),
        tool_fmt_check_def(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_entries_are_well_formed() {
        let tools = catalog();
        let tools = tools.as_array().expect("catalog is an array");
        assert_eq!(tools.len(), 5, "keep the tool count small and deliberate");
        for t in tools {
            let name = t["name"].as_str().expect("name");
            assert!(name.starts_with("almide_"), "{} is not namespaced", name);
            assert!(!t["description"].as_str().unwrap_or("").is_empty());
            assert_eq!(t["inputSchema"]["type"], "object", "{}", name);
            assert!(t["inputSchema"]["properties"].is_object(), "{}", name);
            assert!(t["annotations"].is_object(), "{}", name);
        }
    }

    #[test]
    fn only_the_test_tool_declares_a_side_effect() {
        let tools = catalog();
        for t in tools.as_array().unwrap() {
            let read_only = t["annotations"]["readOnlyHint"].as_bool().unwrap_or(false);
            let expect_read_only = t["name"] != "almide_test";
            assert_eq!(read_only, expect_read_only, "{}", t["name"]);
        }
    }

    #[test]
    fn unknown_tool_is_a_tool_error_not_a_panic() {
        let e = call("almide_deploy", &json!({})).unwrap_err();
        assert!(e.contains("unknown tool"), "{}", e);
    }

    #[test]
    fn missing_required_argument_is_reported_by_name() {
        let e = call("almide_check", &json!({})).unwrap_err();
        assert!(e.contains("`file`"), "{}", e);
    }

    #[test]
    fn non_json_lines_are_kept_apart_from_parsed_ones() {
        let (parsed, other) = split_json_lines("{\"a\":1}\nplain text\n\n{\"b\":2}\n");
        assert_eq!(parsed.len(), 2);
        assert_eq!(other, vec!["plain text"]);
    }

    #[test]
    fn tail_keeps_the_end_and_flags_the_cut() {
        let (text, truncated) = tail("abcdef", 3);
        assert_eq!((text.as_str(), truncated), ("def", true));
        let (text, truncated) = tail("ab", 3);
        assert_eq!((text.as_str(), truncated), ("ab", false));
    }
}
