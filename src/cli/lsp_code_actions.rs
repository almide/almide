
// ══════════════════════════════════════════════════════════════
// Code Actions
// ══════════════════════════════════════════════════════════════

/// `compute_code_actions`'s E003 (unknown module) quickfix: suggest an
/// `import X` insertion after the last existing import. Extracted verbatim.
fn code_action_for_e003(diag: &Diagnostic, lines: &[&str], uri: &Uri) -> Option<CodeActionOrCommand> {
    let module = extract_quoted_name(&diag.message)?;
    let known = ["io", "json", "env", "fs", "http", "regex", "random", "testing", "datetime", "bytes", "html", "path", "channel"];
    if !known.contains(&module.as_str()) {
        return None;
    }
    let insert_line = lines.iter().enumerate()
        .filter(|(_, l)| l.trim().starts_with("import "))
        .map(|(i, _)| (i + 1) as u32).last().unwrap_or(0);
    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: format!("Import '{}'", module),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diag.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(HashMap::from([(uri.clone(), vec![TextEdit {
                range: Range { start: Position { line: insert_line, character: 0 }, end: Position { line: insert_line, character: 0 } },
                new_text: format!("import {}\n", module),
            }])])),
            ..Default::default()
        }),
        ..Default::default()
    }))
}

/// `compute_code_actions`'s E006 (non-effect fn) quickfix: mark the nearest
/// enclosing `fn` (scanning backward from the diagnostic) as `effect fn`.
/// Extracted verbatim — the backward scan + nested checks were also the
/// max-depth-7 nesting site; isolating it into its own function resets
/// that count. Once the first `fn `-without-`effect fn` line is found the
/// scan always stops there (action-or-not), matching the original's
/// unconditional `break` on that condition.
fn code_action_for_e006(diag: &Diagnostic, lines: &[&str], uri: &Uri) -> Option<CodeActionOrCommand> {
    for i in (0..=diag.range.start.line as usize).rev() {
        let lt = lines.get(i)?;
        if !lt.contains("fn ") || lt.contains("effect fn") {
            continue;
        }
        let c = byte_col_to_utf16(lt, lt.find("fn ")?);
        return Some(CodeActionOrCommand::CodeAction(CodeAction {
            title: "Mark as effect fn".to_string(),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diag.clone()]),
            edit: Some(WorkspaceEdit {
                changes: Some(HashMap::from([(uri.clone(), vec![TextEdit {
                    range: Range { start: Position { line: i as u32, character: c }, end: Position { line: i as u32, character: c + 2 } },
                    new_text: "effect fn".to_string(),
                }])])),
                ..Default::default()
            }),
            ..Default::default()
        }));
    }
    None
}

/// Materialize the compiler's machine-applicable fix (`try_replace_span`,
/// round-tripped through the diagnostic's `data` field — see
/// `diag_from_almide`) as a quickfix edit. The stored columns are the
/// compiler's 1-indexed char offsets; they convert to UTF-16 here, where the
/// line text is at hand.
fn code_action_for_try_fix(diag: &Diagnostic, lines: &[&str], uri: &Uri) -> Option<CodeActionOrCommand> {
    let data = diag.data.as_ref()?;
    let snippet = data.get("try")?.as_str()?;
    let line = (data.get("line")?.as_u64()? as usize).checked_sub(1)? as u32;
    let col = data.get("col")?.as_u64()? as usize;
    let end_col = data.get("endCol")?.as_u64()? as usize;
    let line_text = lines.get(line as usize)?;
    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: format!("Replace with `{}`", snippet),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diag.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(HashMap::from([(uri.clone(), vec![TextEdit {
                range: Range {
                    start: Position { line, character: char_col_to_utf16(line_text, col) },
                    end: Position { line, character: char_col_to_utf16(line_text, end_col) },
                },
                new_text: snippet.to_string(),
            }])])),
            ..Default::default()
        }),
        ..Default::default()
    }))
}

fn compute_code_actions(source: &str, diagnostics: &[Diagnostic], uri: &Uri) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for diag in diagnostics {
        let code = diag.code.as_ref().and_then(|c| match c { NumberOrString::String(s) => Some(s.as_str()), _ => None });
        let action = match code {
            Some("E003") => code_action_for_e003(diag, &lines, uri),
            Some("E006") => code_action_for_e006(diag, &lines, uri),
            _ => None,
        };
        if let Some(a) = action {
            actions.push(a);
        }
        if let Some(a) = code_action_for_try_fix(diag, &lines, uri) {
            actions.push(a);
        }
    }
    actions
}

fn extract_quoted_name(msg: &str) -> Option<String> {
    let s = msg.find('\'')?;
    let rest = &msg[s + 1..];
    let e = rest.find('\'')?;
    Some(rest[..e].to_string())
}

// ══════════════════════════════════════════════════════════════
// Workspace Symbols
// ══════════════════════════════════════════════════════════════

/// `compute_workspace_symbols`'s per-file body: parse one `.almd` file and
/// append every decl whose name matches `query`/`query_lower` to `results`.
/// Extracted verbatim.
fn collect_workspace_symbols_from_file(file_path: &std::path::Path, query: &str, query_lower: &str, results: &mut Vec<SymbolInformation>) {
    let source = match std::fs::read_to_string(file_path) { Ok(s) => s, Err(_) => return };
    let tokens = crate::lexer::Lexer::tokenize(&source);
    let mut parser = crate::parser::Parser::new(tokens);
    let prog = match parser.parse() { Ok(p) => p, Err(_) => return };
    let file_uri = match Uri::from_str(&format!("file://{}", file_path.display())) { Ok(u) => u, Err(_) => return };
    for decl in &prog.decls {
        let (name, kind, span) = match decl {
            crate::ast::Decl::Fn { name, span, .. } => (name.as_str(), SymbolKind::FUNCTION, span),
            crate::ast::Decl::Type { name, span, .. } => (name.as_str(), SymbolKind::STRUCT, span),
            crate::ast::Decl::TopLet { name, span, .. } => (name.as_str(), SymbolKind::VARIABLE, span),
            _ => continue,
        };
        if !query.is_empty() && !name.to_lowercase().contains(query_lower) { continue; }
        let line = span.as_ref().map(|s| s.line.saturating_sub(1) as u32).unwrap_or(0);
        let line_text = source.lines().nth(line as usize).unwrap_or("");
        let col = span.as_ref().map(|s| char_col_to_utf16(line_text, s.col)).unwrap_or(0);
        #[allow(deprecated)]
        results.push(SymbolInformation {
            name: name.to_string(), kind,
            location: Location { uri: file_uri.clone(), range: Range { start: Position { line, character: col }, end: Position { line, character: col + name.len() as u32 } } },
            tags: None, deprecated: None,
            container_name: file_path.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string()),
        });
    }
}

fn compute_workspace_symbols(query: &str, workspace_root: &Option<std::path::PathBuf>) -> Vec<SymbolInformation> {
    let root = match workspace_root { Some(r) => r, None => return vec![] };
    let mut files = Vec::new();
    collect_almd_files(root, &mut files);
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();
    for file_path in &files {
        collect_workspace_symbols_from_file(file_path, query, &query_lower, &mut results);
    }
    results
}

fn collect_almd_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.starts_with('.') || name == "target" || name == "node_modules" { return; }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.is_dir() { collect_almd_files(&p, out); }
            else if p.extension().map_or(false, |e| e == "almd") { out.push(p); }
        }
    }
}

// ══════════════════════════════════════════════════════════════
// Helpers
// ══════════════════════════════════════════════════════════════

/// `file://` URI → filesystem path, in the three shapes real clients send:
/// `file:///tmp/x` (POSIX), `file:///C:/x`, and percent-encoded
/// `file:///c%3A/x` (VS Code on Windows encodes the drive colon). The old
/// bare `strip_prefix` left the leading slash and the `%3A` intact, so every
/// real-editor path on Windows failed to resolve (#1008) — the manifest was
/// never found and per-project analysis silently degraded.
fn uri_to_path(uri: &Uri) -> Option<String> {
    let rest = uri.as_str().strip_prefix("file://")?;
    let decoded = percent_decode(rest);
    // `/C:/…` — a POSIX-form absolute path naming a Windows drive → `C:/…`.
    let b = decoded.as_bytes();
    if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' {
        return Some(decoded[1..].to_string());
    }
    Some(decoded)
}

/// Minimal RFC 3986 percent-decoding (multi-byte UTF-8 sequences decode
/// byte-wise, then re-validate as a string). Invalid escapes pass through
/// literally rather than erroring — a path lookup on the raw text then fails
/// visibly downstream instead of the URI being dropped here.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = |c: u8| (c as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod uri_to_path_tests {
    use super::*;
    use std::str::FromStr;

    fn conv(s: &str) -> Option<String> {
        uri_to_path(&Uri::from_str(s).unwrap())
    }

    #[test]
    fn posix_path() {
        assert_eq!(conv("file:///tmp/x.almd").as_deref(), Some("/tmp/x.almd"));
    }

    #[test]
    fn windows_drive_plain() {
        assert_eq!(conv("file:///C:/Users/x.almd").as_deref(), Some("C:/Users/x.almd"));
    }

    #[test]
    fn windows_drive_percent_encoded_colon() {
        // The exact shape VS Code sends on Windows.
        assert_eq!(conv("file:///c%3A/Users/x.almd").as_deref(), Some("c:/Users/x.almd"));
    }

    #[test]
    fn percent_encoded_space_and_utf8() {
        assert_eq!(conv("file:///tmp/a%20b/%E3%81%82.almd").as_deref(), Some("/tmp/a b/あ.almd"));
    }

    #[test]
    fn non_file_scheme_is_none() {
        assert_eq!(conv("untitled:Untitled-1"), None);
    }
}

fn publish_diagnostics(connection: &Connection, uri: &Uri, diags: &[Diagnostic]) {
    let params = PublishDiagnosticsParams { uri: uri.clone(), diagnostics: diags.to_vec(), version: None };
    let notif = Notification { method: "textDocument/publishDiagnostics".to_string(), params: serde_json::to_value(params).unwrap() };
    connection.sender.send(Message::Notification(notif)).ok();
}

/// Compiler diagnostic → LSP diagnostic. The compiler's line/col are
/// 1-indexed char offsets; LSP wants 0-based lines and UTF-16 columns, so the
/// conversion needs the source line text. A machine-applicable fix
/// (`try_snippet` + `try_replace_span`) rides along in the `data` field —
/// the client echoes `data` back on `textDocument/codeAction`, where
/// `code_action_for_try_fix` materializes it as a quickfix edit. Before
/// this, the already-computed fix-its were dropped here and never reached
/// the client (#927).
fn diag_from_almide(d: &crate::diagnostic::Diagnostic, lines: &[&str]) -> Diagnostic {
    let line = d.line.unwrap_or(1).saturating_sub(1) as u32;
    let line_text = lines.get(line as usize).copied().unwrap_or("");
    let col = char_col_to_utf16(line_text, d.col.unwrap_or(1));
    let end_col = d.end_col.map(|c| char_col_to_utf16(line_text, c)).unwrap_or(col + 1);
    let data = match (&d.try_snippet, d.try_replace_span) {
        (Some(snippet), Some((l, c, ec))) => Some(serde_json::json!({
            "try": snippet, "line": l, "col": c, "endCol": ec,
        })),
        _ => None,
    };
    Diagnostic {
        range: Range { start: Position { line, character: col }, end: Position { line, character: end_col } },
        severity: Some(if d.level == crate::diagnostic::Level::Error { DiagnosticSeverity::ERROR } else { DiagnosticSeverity::WARNING }),
        code: d.code.as_ref().map(|c| NumberOrString::String(c.to_string())),
        source: Some("almide".to_string()),
        message: if d.hint.is_empty() { d.message.clone() } else { format!("{}\nhint: {}", d.message, d.hint) },
        data,
        ..Default::default()
    }
}

/// Walk up from `file_path` to the almide.toml governing it, if any.
fn find_project_toml(file_path: &str) -> Option<std::path::PathBuf> {
    let mut dir = std::path::Path::new(file_path).parent()?;
    loop {
        let toml = dir.join("almide.toml");
        if toml.exists() {
            return Some(toml);
        }
        dir = dir.parent()?;
    }
}

/// Dependency source dirs for the project owning `file_path`, cached per
/// almide.toml. `allow_fetch` is true only on didOpen: `fetch_all_deps`
/// shells out to git and writes almide.lock, which must never run on a
/// keystroke (#927). With `allow_fetch` false a cache miss resolves to no
/// deps instead of touching the network. A failed fetch caches the empty
/// list, so one broken manifest cannot re-trigger fetch storms.
fn project_deps_for(file_path: Option<&str>, cache: &mut DepCache, allow_fetch: bool) -> Vec<(crate::project::PkgId, std::path::PathBuf)> {
    let Some(toml) = file_path.and_then(find_project_toml) else { return Vec::new() };
    if let Some(cached) = cache.get(&toml) {
        return cached.clone();
    }
    if !allow_fetch {
        return Vec::new();
    }
    let deps: Vec<(crate::project::PkgId, std::path::PathBuf)> = crate::project::parse_toml(&toml).ok()
        .and_then(|proj| crate::project_fetch::fetch_all_deps(&proj).ok())
        .map(|deps| deps.into_iter().map(|fd| (fd.pkg_id, fd.source_dir)).collect())
        .unwrap_or_default();
    cache.insert(toml, deps.clone());
    deps
}

fn resolve_imports_cached(file_path: &str, program: &crate::ast::Program, deps: &[(crate::project::PkgId, std::path::PathBuf)]) -> Vec<(String, crate::ast::Program, bool)> {
    match crate::resolve::resolve_imports_with_deps(file_path, program, deps) {
        Ok(r) => r.modules.into_iter().map(|(n, p, _, s)| (n, p, s)).collect(),
        Err(_) => vec![],
    }
}

fn type_to_module(type_name: &str) -> Option<String> {
    match type_name {
        "Int" => Some("int".to_string()),
        "Float" => Some("float".to_string()),
        "String" => Some("string".to_string()),
        "Bool" => Some("bool".to_string()),
        "List" => Some("list".to_string()),
        "Map" => Some("map".to_string()),
        "Set" => Some("set".to_string()),
        "Option" => Some("option".to_string()),
        "Result" => Some("result".to_string()),
        "Bytes" => Some("bytes".to_string()),
        _ => None,
    }
}

fn find_stdlib_path(module: &str) -> Option<std::path::PathBuf> {
    let filename = format!("{}.almd", module);
    // Walk up from the almide binary to find stdlib/
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent();
        for _ in 0..6 {
            let Some(d) = dir else { break };
            let stdlib = d.join("stdlib").join(&filename);
            if stdlib.exists() { return Some(stdlib); }
            dir = d.parent();
        }
    }
    // Fallback: check known install locations
    let home = std::env::var("HOME").ok()?;
    let candidates = [
        format!("{}/.local/almide/stdlib/{}.almd", home, module),
        format!("{}/.almide/stdlib/{}.almd", home, module),
    ];
    for c in &candidates {
        let p = std::path::PathBuf::from(c);
        if p.exists() { return Some(p); }
    }
    None
}

fn find_fn_line_in_file(path: &std::path::Path, func_name: &str) -> Option<u32> {
    let source = std::fs::read_to_string(path).ok()?;
    let pattern = format!("fn {}(", func_name);
    for (i, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&pattern)
            || trimmed.starts_with(&format!("effect fn {}(", func_name))
        {
            return Some(i as u32);
        }
    }
    None
}

fn empty_program() -> crate::ast::Program {
    crate::ast::Program {
        module: None, imports: vec![], decls: vec![],
        comment_map: vec![], doc_map: vec![], blank_lines_map: vec![],
        failed_fn_names: std::collections::HashSet::new(),
    }
}

fn empty_checker() -> crate::check::Checker {
    let canon = crate::canonicalize::canonicalize_program(
        &empty_program(),
        std::iter::empty::<(&str, &crate::ast::Program, bool)>(),
    );
    crate::check::Checker::from_env(canon.env)
}

fn format_type_expr(te: &crate::ast::TypeExpr) -> String {
    match te {
        crate::ast::TypeExpr::Simple { name } => name.as_str().to_string(),
        crate::ast::TypeExpr::Generic { name, args } => {
            format!("{}[{}]", name.as_str(), args.iter().map(|a| format_type_expr(a)).collect::<Vec<_>>().join(", "))
        }
        crate::ast::TypeExpr::Tuple { elements } => {
            format!("({})", elements.iter().map(|e| format_type_expr(e)).collect::<Vec<_>>().join(", "))
        }
        crate::ast::TypeExpr::Fn { is_effect: _, params, ret } => {
            format!("({}) -> {}", params.iter().map(|p| format_type_expr(p)).collect::<Vec<_>>().join(", "), format_type_expr(ret))
        }
        _ => "?".to_string(),
    }
}
