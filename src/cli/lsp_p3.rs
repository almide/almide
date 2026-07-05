
// ══════════════════════════════════════════════════════════════
// Code Actions
// ══════════════════════════════════════════════════════════════

fn compute_code_actions(source: &str, diagnostics: &[Diagnostic], uri: &Uri) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for diag in diagnostics {
        let code = diag.code.as_ref().and_then(|c| match c { NumberOrString::String(s) => Some(s.as_str()), _ => None });
        match code {
            Some("E003") => {
                if let Some(module) = extract_quoted_name(&diag.message) {
                    let known = ["io", "json", "env", "fs", "http", "regex", "random", "testing", "datetime", "bytes", "html", "path", "channel"];
                    if known.contains(&module.as_str()) {
                        let insert_line = lines.iter().enumerate()
                            .filter(|(_, l)| l.trim().starts_with("import "))
                            .map(|(i, _)| (i + 1) as u32).last().unwrap_or(0);
                        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
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
                        }));
                    }
                }
            }
            Some("E006") => {
                for i in (0..=diag.range.start.line as usize).rev() {
                    if let Some(lt) = lines.get(i) {
                        if lt.contains("fn ") && !lt.contains("effect fn") {
                            if let Some(c) = lt.find("fn ") {
                                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                                    title: "Mark as effect fn".to_string(),
                                    kind: Some(CodeActionKind::QUICKFIX),
                                    diagnostics: Some(vec![diag.clone()]),
                                    edit: Some(WorkspaceEdit {
                                        changes: Some(HashMap::from([(uri.clone(), vec![TextEdit {
                                            range: Range { start: Position { line: i as u32, character: c as u32 }, end: Position { line: i as u32, character: c as u32 + 2 } },
                                            new_text: "effect fn".to_string(),
                                        }])])),
                                        ..Default::default()
                                    }),
                                    ..Default::default()
                                }));
                            }
                            break;
                        }
                    }
                }
            }
            _ => {}
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

fn compute_workspace_symbols(query: &str, workspace_root: &Option<std::path::PathBuf>) -> Vec<SymbolInformation> {
    let root = match workspace_root { Some(r) => r, None => return vec![] };
    let mut files = Vec::new();
    collect_almd_files(root, &mut files);
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();
    for file_path in &files {
        let source = match std::fs::read_to_string(file_path) { Ok(s) => s, Err(_) => continue };
        let tokens = crate::lexer::Lexer::tokenize(&source);
        let mut parser = crate::parser::Parser::new(tokens);
        let prog = match parser.parse() { Ok(p) => p, Err(_) => continue };
        let file_uri = match Uri::from_str(&format!("file://{}", file_path.display())) { Ok(u) => u, Err(_) => continue };
        for decl in &prog.decls {
            let (name, kind, span) = match decl {
                crate::ast::Decl::Fn { name, span, .. } => (name.as_str(), SymbolKind::FUNCTION, span),
                crate::ast::Decl::Type { name, span, .. } => (name.as_str(), SymbolKind::STRUCT, span),
                crate::ast::Decl::TopLet { name, span, .. } => (name.as_str(), SymbolKind::VARIABLE, span),
                _ => continue,
            };
            if !query.is_empty() && !name.to_lowercase().contains(&query_lower) { continue; }
            let line = span.as_ref().map(|s| s.line.saturating_sub(1) as u32).unwrap_or(0);
            let col = span.as_ref().map(|s| s.col.saturating_sub(1) as u32).unwrap_or(0);
            #[allow(deprecated)]
            results.push(SymbolInformation {
                name: name.to_string(), kind,
                location: Location { uri: file_uri.clone(), range: Range { start: Position { line, character: col }, end: Position { line, character: col + name.len() as u32 } } },
                tags: None, deprecated: None,
                container_name: file_path.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string()),
            });
        }
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

fn uri_to_path(uri: &Uri) -> Option<String> {
    uri.as_str().strip_prefix("file://").map(|p| p.to_string())
}

fn publish_diagnostics(connection: &Connection, uri: &Uri, diags: &[Diagnostic]) {
    let params = PublishDiagnosticsParams { uri: uri.clone(), diagnostics: diags.to_vec(), version: None };
    let notif = Notification { method: "textDocument/publishDiagnostics".to_string(), params: serde_json::to_value(params).unwrap() };
    connection.sender.send(Message::Notification(notif)).ok();
}

fn diag_from_almide(d: &crate::diagnostic::Diagnostic) -> Diagnostic {
    let line = d.line.unwrap_or(1).saturating_sub(1) as u32;
    let col = d.col.unwrap_or(1).saturating_sub(1) as u32;
    let end_col = d.end_col.map(|c| c as u32).unwrap_or(col + 1);
    Diagnostic {
        range: Range { start: Position { line, character: col }, end: Position { line, character: end_col } },
        severity: Some(if d.level == crate::diagnostic::Level::Error { DiagnosticSeverity::ERROR } else { DiagnosticSeverity::WARNING }),
        code: d.code.as_ref().map(|c| NumberOrString::String(c.to_string())),
        source: Some("almide".to_string()),
        message: if d.hint.is_empty() { d.message.clone() } else { format!("{}\nhint: {}", d.message, d.hint) },
        ..Default::default()
    }
}

fn resolve_imports_for_lsp(file_path: &str, program: &crate::ast::Program) -> Vec<(String, crate::ast::Program, bool)> {
    let file = std::path::Path::new(file_path);
    let dep_paths: Vec<(crate::project::PkgId, std::path::PathBuf)> = file.parent()
        .and_then(|dir| {
            // Walk up to find almide.toml
            let mut d = dir;
            loop {
                let toml = d.join("almide.toml");
                if toml.exists() {
                    let proj = crate::project::parse_toml(&toml).ok()?;
                    return crate::project_fetch::fetch_all_deps(&proj).ok().map(|deps| {
                        deps.into_iter().map(|fd| (fd.pkg_id, fd.source_dir)).collect()
                    });
                }
                d = d.parent()?;
            }
        })
        .unwrap_or_default();
    match crate::resolve::resolve_imports_with_deps(file_path, program, &dep_paths) {
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
        crate::ast::TypeExpr::Fn { params, ret } => {
            format!("({}) -> {}", params.iter().map(|p| format_type_expr(p)).collect::<Vec<_>>().join(", "), format_type_expr(ret))
        }
        _ => "?".to_string(),
    }
}
