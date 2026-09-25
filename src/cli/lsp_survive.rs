// `almide/survive` (#2147): the survival delta of a proposed edit, as an LSP
// request. Included into lsp.rs.
//
// Params: `{ textDocument: { uri }, text?: string, patch?: string }`.
// - `text`  — the file's complete proposed contents;
// - `patch` — a unified diff against the file on disk;
// - neither — the editor's current BUFFER for that document is the proposal
//   (the natural question in an editor: "would my unsaved changes survive?").
//
// The answer is exactly what `almide survive <file> --with - --json` prints:
// the handler runs the CLI as a child process, the same single path the MCP
// tool takes, so the three surfaces cannot drift. The child runs in the
// file's project root (the nearest `almide.toml`), where the CLI resolves
// dependencies. Nothing is written.

fn survive_project_dir(path: &std::path::Path) -> std::path::PathBuf {
    let start = path.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
    let mut dir = start.clone();
    loop {
        if dir.join("almide.toml").exists() {
            return dir;
        }
        if !dir.pop() {
            return start;
        }
    }
}

#[allow(clippy::mutable_key_type)] // same Uri-key false positive as handle_notification
fn handle_survive(req: &Request, documents: &HashMap<Uri, String>) -> Response {
    let fail = |code: lsp_server::ErrorCode, msg: String| Response::new_err(req.id.clone(), code as i32, msg);
    let params = &req.params;
    let Some(uri) = params
        .get("textDocument")
        .and_then(|t| t.get("uri"))
        .and_then(|u| u.as_str())
        .and_then(|u| Uri::from_str(u).ok())
    else {
        return fail(lsp_server::ErrorCode::InvalidParams, "almide/survive needs textDocument.uri".into());
    };
    let Some(path) = uri_to_path(&uri) else {
        return fail(lsp_server::ErrorCode::InvalidParams, format!("not a file URI: {}", uri.as_str()));
    };
    let (edit, kind) = match (params.get("text").and_then(|v| v.as_str()), params.get("patch").and_then(|v| v.as_str())) {
        (Some(_), Some(_)) => {
            return fail(lsp_server::ErrorCode::InvalidParams, "pass `text` or `patch`, not both".into());
        }
        (Some(t), None) => (t.to_string(), "text"),
        (None, Some(p)) => (p.to_string(), "patch"),
        (None, None) => match documents.get(&uri) {
            Some(buf) => (buf.clone(), "text"),
            None => {
                return fail(
                    lsp_server::ErrorCode::InvalidParams,
                    "no `text` / `patch` given and the document is not open".into(),
                );
            }
        },
    };
    let abs = std::path::Path::new(&path);
    let cwd = survive_project_dir(abs);
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => return fail(lsp_server::ErrorCode::InternalError, format!("cannot locate the almide binary: {e}")),
    };
    let child = std::process::Command::new(exe)
        .args(["survive", &path, "--with", "-", "--json", "--as", kind])
        .current_dir(&cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return fail(lsp_server::ErrorCode::InternalError, format!("cannot run almide survive: {e}")),
    };
    if let Some(mut pipe) = child.stdin.take() {
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = pipe.write_all(edit.as_bytes());
        });
    }
    let out = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => return fail(lsp_server::ErrorCode::InternalError, format!("almide survive failed: {e}")),
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    match serde_json::from_str::<serde_json::Value>(stdout.trim()) {
        Ok(report) => match report.get("error").and_then(|e| e.as_str()) {
            Some(e) => fail(lsp_server::ErrorCode::RequestFailed, e.to_string()),
            None => Response::new_ok(req.id.clone(), report),
        },
        Err(_) => fail(
            lsp_server::ErrorCode::RequestFailed,
            format!("almide survive produced no JSON report: {}", String::from_utf8_lossy(&out.stderr).trim()),
        ),
    }
}
