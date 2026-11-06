//! The one `--json` shape every command shares (#4590):
//!
//! ```text
//! {"schema_version":1,"ok":<bool>,"diagnostics":[...],"result":<value>}
//! ```
//!
//! printed as ONE line on stdout. `ok` is true exactly when the exit code is
//! 0, and the exit code is the command's own. `diagnostics` rows have the
//! shape `almide check --json` always printed (`level`, `code`, `message`,
//! `hint`, the fix fields, `file`/`line`/`col` …); a failure that carries no
//! structured diagnostic becomes one row whose `message` is the error text.
//! `result` holds what the command used to print under `--json`, its fields
//! unchanged (see [`Shape`]).
//!
//! The envelope is built around the command rather than inside it: the
//! parent runs the same command line as a child with `--raw-json` (the
//! command's own output, exactly as before), captures stdout, stderr and the
//! exit code, and wraps them. So every exit path of every command — the
//! hundreds of `process::exit`s included — lands in the envelope, and a
//! command gains `--json` by naming its [`Shape`], not by rewriting its
//! printing.

use serde_json::{json, Value};

/// The envelope's version. A change to the envelope itself bumps it; a
/// command's `result` evolves additively under it.
pub const SCHEMA_VERSION: u32 = 1;

/// How a command's raw `--json` stdout becomes `diagnostics` and `result`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// One diagnostic row per line (`check --json`): the rows are the
    /// `diagnostics`; `result` counts them, `{errors, warnings}`.
    Diagnostics,
    /// One JSON record per line (`test --json`): `result` is `{<key>: [records]}`.
    Records(&'static str),
    /// One JSON value: an object is `result` itself, anything else is
    /// `{<key>: value}`.
    Value(&'static str),
    /// Human text: `result` is `{stdout, stderr}`.
    Text,
}

/// The hint of a row made from an error's text rather than from a
/// structured diagnostic.
pub const UNSTRUCTURED_HINT: &str =
    "the command stopped before it could report a structured diagnostic; `message` is its error text";

/// The envelope as one JSON value.
pub fn envelope(ok: bool, diagnostics: Vec<Value>, result: Value) -> Value {
    json!({
        "schema_version": SCHEMA_VERSION,
        "ok": ok,
        "diagnostics": diagnostics,
        "result": result,
    })
}

/// Print the envelope as one line on stdout.
pub fn print(ok: bool, diagnostics: Vec<Value>, result: Value) {
    crate::out(&envelope(ok, diagnostics, result).to_string());
}

/// An error that carries no structured diagnostic, as a row of the shape
/// `check --json` prints: code "", no span, the text as `message`, and a
/// hint saying so (every row carries one).
pub fn error_row(message: &str) -> Value {
    let d = crate::diagnostic::Diagnostic::error(message, UNSTRUCTURED_HINT, "");
    serde_json::from_str(&crate::diagnostic_render::to_json(&d))
        .unwrap_or_else(|_| json!({ "level": "error", "code": "", "message": message }))
}

/// Wrap a finished run: its exit code, its raw stdout and its stderr.
/// Returns `(ok, diagnostics, result)`.
pub fn wrap(shape: Shape, code: i32, stdout: &str, stderr: &str) -> (bool, Vec<Value>, Value) {
    let ok = code == 0;
    let mut diagnostics: Vec<Value> = Vec::new();
    let mut unparsed: Vec<&str> = Vec::new();
    let result = match shape {
        Shape::Diagnostics => {
            for line in stdout.lines().filter(|l| !l.trim().is_empty()) {
                match serde_json::from_str::<Value>(line) {
                    Ok(v) if v.is_object() => diagnostics.push(v),
                    _ => unparsed.push(line),
                }
            }
            let count = |level: &str| diagnostics.iter().filter(|d| d["level"] == level).count();
            json!({ "errors": count("error"), "warnings": count("warning") })
        }
        Shape::Records(key) => {
            let mut records = Vec::new();
            for line in stdout.lines().filter(|l| !l.trim().is_empty()) {
                match serde_json::from_str::<Value>(line) {
                    Ok(v) => records.push(v),
                    Err(_) => unparsed.push(line),
                }
            }
            json!({ key: records })
        }
        Shape::Value(key) => match serde_json::from_str::<Value>(stdout.trim()) {
            Ok(v) if v.is_object() => {
                // A command that reports its own failure as `{"error": …}`
                // (apply's could-not-judge exit 2) also lists it as a diagnostic.
                if !ok && let Some(e) = v.get("error").and_then(Value::as_str) {
                    diagnostics.push(error_row(e));
                }
                v
            }
            Ok(v) => json!({ key: v }),
            Err(_) if stdout.trim().is_empty() => Value::Null,
            Err(_) => {
                unparsed.push(stdout.trim_end());
                Value::Null
            }
        },
        Shape::Text => json!({ "stdout": stdout, "stderr": stderr }),
    };
    let mut result = result;
    if !unparsed.is_empty() && let Some(obj) = result.as_object_mut() {
        obj.insert("stdout".to_string(), Value::from(unparsed.join("\n")));
    } else if !unparsed.is_empty() {
        result = json!({ "stdout": unparsed.join("\n") });
    }
    if !ok && diagnostics.iter().all(|d| d["level"] != "error") {
        let text = stderr.trim();
        let text = if text.is_empty() { format!("exited with code {code}") } else { text.to_string() };
        diagnostics.push(error_row(&text));
    }
    (ok, diagnostics, result)
}

/// Run this same command line as a child with `--raw-json`, wrap what it
/// printed and exit with its code. The child inherits stdin (`apply --with -`
/// reads the edit there); its stderr is passed through after the run, so a
/// person watching still sees the progress lines.
pub fn run_enveloped(shape: Shape) -> ! {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => fail_now(&format!("error: cannot locate the almide binary: {e}")),
    };
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let output = std::process::Command::new(exe)
        .arg("--raw-json")
        .args(&args)
        .stdin(std::process::Stdio::inherit())
        .output();
    let output = match output {
        Ok(o) => o,
        Err(e) => fail_now(&format!("error: cannot run the command: {e}")),
    };
    let code = output.status.code().unwrap_or(1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    crate::err_no_nl(&stderr);
    let (ok, diagnostics, result) = wrap(shape, code, &stdout, &stderr);
    print(ok, diagnostics, result);
    std::process::exit(code);
}

/// An error before the command could run, as the envelope; exit 2.
pub fn fail_now(message: &str) -> ! {
    crate::err(message);
    print(false, vec![error_row(message)], Value::Null);
    std::process::exit(2);
}

/// Does this command line ask for the envelope? `--json` anywhere, and not
/// the raw child (`--raw-json`).
pub fn requested(args: &[std::ffi::OsString]) -> bool {
    args.iter().any(|a| a == "--json") && !args.iter().any(|a| a == "--raw-json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_rows_become_diagnostics_and_are_counted() {
        let rows = "{\"level\":\"error\",\"code\":\"E001\",\"message\":\"m\"}\n{\"level\":\"warning\",\"code\":\"W1\",\"message\":\"w\"}\n";
        let (ok, d, r) = wrap(Shape::Diagnostics, 1, rows, "");
        assert!(!ok);
        assert_eq!(d.len(), 2);
        assert_eq!(r, json!({"errors": 1, "warnings": 1}));
    }

    #[test]
    fn a_failure_without_rows_carries_its_stderr_as_one_row() {
        let (ok, d, r) = wrap(Shape::Text, 1, "", "error: almide.toml not found\n");
        assert!(!ok);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0]["message"], "error: almide.toml not found");
        assert_eq!(d[0]["level"], "error");
        assert_eq!(r["stderr"], "error: almide.toml not found\n");
    }

    #[test]
    fn records_and_values_keep_their_fields() {
        let (_, _, r) = wrap(Shape::Records("files"), 0, "{\"file\":\"a\"}\n{\"file\":\"b\"}\n", "");
        assert_eq!(r, json!({"files": [{"file": "a"}, {"file": "b"}]}));
        let (_, _, r) = wrap(Shape::Value("codes"), 0, "[1,2]", "");
        assert_eq!(r, json!({"codes": [1, 2]}));
        let (_, d, r) = wrap(Shape::Value("report"), 2, "{\"schema_version\":1,\"error\":\"bad hunk\"}", "");
        assert_eq!(r["error"], "bad hunk");
        assert_eq!(d[0]["message"], "bad hunk");
    }

    #[test]
    fn the_envelope_has_exactly_four_fields() {
        let v = envelope(true, vec![], json!({}));
        let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        assert_eq!(keys.len(), 4);
        for k in ["schema_version", "ok", "diagnostics", "result"] {
            assert!(v.get(k).is_some(), "{k}");
        }
    }
}
