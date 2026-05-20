use std::collections::HashMap;
use lsp_server::{Connection, Message, Request, Response, Notification};
use lsp_types::*;

pub fn run_lsp() {
    let (connection, io_threads) = Connection::stdio();

    let server_capabilities = serde_json::to_value(ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![".".to_string()]),
            ..Default::default()
        }),
        ..Default::default()
    }).unwrap();

    let init_params = match connection.initialize(server_capabilities) {
        Ok(it) => it,
        Err(e) => {
            eprintln!("LSP init failed: {}", e);
            return;
        }
    };
    let _init: InitializeParams = serde_json::from_value(init_params).unwrap();

    let mut documents: HashMap<Uri, String> = HashMap::new();

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req).unwrap_or(false) {
                    return;
                }
                let resp = handle_request(&req, &documents);
                if let Some(r) = resp {
                    connection.sender.send(Message::Response(r)).ok();
                }
            }
            Message::Notification(notif) => {
                match notif.method.as_str() {
                    "textDocument/didOpen" => {
                        if let Ok(params) = serde_json::from_value::<DidOpenTextDocumentParams>(notif.params) {
                            let uri = params.text_document.uri.clone();
                            documents.insert(uri.clone(), params.text_document.text);
                            publish_diagnostics(&connection, &uri, documents.get(&uri).unwrap());
                        }
                    }
                    "textDocument/didChange" => {
                        if let Ok(params) = serde_json::from_value::<DidChangeTextDocumentParams>(notif.params) {
                            let uri = params.text_document.uri.clone();
                            if let Some(change) = params.content_changes.into_iter().last() {
                                documents.insert(uri.clone(), change.text);
                                publish_diagnostics(&connection, &uri, documents.get(&uri).unwrap());
                            }
                        }
                    }
                    "textDocument/didClose" => {
                        if let Ok(params) = serde_json::from_value::<DidCloseTextDocumentParams>(notif.params) {
                            documents.remove(&params.text_document.uri);
                        }
                    }
                    _ => {}
                }
            }
            Message::Response(_) => {}
        }
    }

    io_threads.join().ok();
}

fn handle_request(req: &Request, documents: &HashMap<Uri, String>) -> Option<Response> {
    match req.method.as_str() {
        "textDocument/hover" => {
            let params: HoverParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = &params.text_document_position_params.text_document.uri;
            let pos = params.text_document_position_params.position;
            let source = documents.get(uri)?;
            let hover = compute_hover(source, pos);
            let result = hover.map(|h| serde_json::to_value(h).unwrap()).unwrap_or(serde_json::Value::Null);
            Some(Response { id: req.id.clone(), result: Some(result), error: None })
        }
        "textDocument/completion" => {
            let params: CompletionParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = &params.text_document_position.text_document.uri;
            let pos = params.text_document_position.position;
            let source = documents.get(uri)?;
            let items = compute_completions(source, pos);
            let result = serde_json::to_value(CompletionResponse::Array(items)).unwrap();
            Some(Response { id: req.id.clone(), result: Some(result), error: None })
        }
        _ => None,
    }
}

fn publish_diagnostics(connection: &Connection, uri: &Uri, source: &str) {
    let diags = check_source(source);
    let params = PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics: diags,
        version: None,
    };
    let notif = Notification {
        method: "textDocument/publishDiagnostics".to_string(),
        params: serde_json::to_value(params).unwrap(),
    };
    connection.sender.send(Message::Notification(notif)).ok();
}

fn check_source(source: &str) -> Vec<Diagnostic> {
    let tokens = crate::lexer::Lexer::tokenize(source);
    let mut parser = crate::parser::Parser::new(tokens);
    let prog = match parser.parse() {
        Ok(p) => p,
        Err(_) => return parser.errors.iter().map(|e| diag_from_almide(e)).collect(),
    };
    let parse_errors: Vec<Diagnostic> = parser.errors.iter().map(|e| diag_from_almide(e)).collect();

    if parser.errors.iter().any(|e| e.level == crate::diagnostic::Level::Error) {
        return parse_errors;
    }

    let mut program = prog;
    let canon = crate::canonicalize::canonicalize_program(
        &program,
        std::iter::empty::<(&str, &crate::ast::Program, bool)>(),
    );
    let mut checker = crate::check::Checker::from_env(canon.env);
    checker.source_text = Some(source.to_string());
    checker.diagnostics = canon.diagnostics;
    let check_diags = checker.infer_program(&mut program);

    let mut result = parse_errors;
    for d in &check_diags {
        result.push(diag_from_almide(d));
    }
    result
}

fn diag_from_almide(d: &crate::diagnostic::Diagnostic) -> Diagnostic {
    let line = d.line.unwrap_or(1).saturating_sub(1) as u32;
    let col = d.col.unwrap_or(1).saturating_sub(1) as u32;
    let end_col = d.end_col.map(|c| c as u32).unwrap_or(col + 1);
    let severity = if d.level == crate::diagnostic::Level::Error {
        DiagnosticSeverity::ERROR
    } else {
        DiagnosticSeverity::WARNING
    };
    Diagnostic {
        range: Range {
            start: Position { line, character: col },
            end: Position { line, character: end_col },
        },
        severity: Some(severity),
        code: d.code.as_ref().map(|c| NumberOrString::String(c.to_string())),
        source: Some("almide".to_string()),
        message: if d.hint.is_empty() {
            d.message.clone()
        } else {
            format!("{}\nhint: {}", d.message, d.hint)
        },
        ..Default::default()
    }
}

fn compute_hover(source: &str, pos: Position) -> Option<Hover> {
    let lines: Vec<&str> = source.lines().collect();
    let line = lines.get(pos.line as usize)?;
    let col = pos.character as usize;
    if col >= line.len() { return None; }

    let start = line[..col].rfind(|c: char| !c.is_alphanumeric() && c != '_').map(|i| i + 1).unwrap_or(0);
    let end = col + line[col..].find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(line.len() - col);
    let word = &line[start..end];
    if word.is_empty() { return None; }

    // Module.func hover
    let after = if end < line.len() && line.as_bytes()[end] == b'.' {
        let func_start = end + 1;
        let func_end = func_start + line[func_start..].find(|c: char| !c.is_alphanumeric() && c != '_' && c != '?').unwrap_or(line.len() - func_start);
        Some(&line[func_start..func_end])
    } else {
        None
    };

    if let Some(func) = after {
        if let Some(sig) = crate::stdlib::lookup_sig(word, func) {
            let params = sig.params.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect::<Vec<_>>().join(", ");
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("```almide\nfn {}.{}({}) -> {}\n```", word, func, params, sig.ret.display()),
                }),
                range: None,
            });
        }
    }

    let info = match word {
        "fn" => Some("Function declaration"),
        "let" => Some("Immutable binding"),
        "var" => Some("Mutable binding"),
        "mut" => Some("Mutable parameter modifier — callers must pass a `var` binding"),
        "type" => Some("Type declaration"),
        "match" => Some("Pattern matching expression"),
        "effect" => Some("Effect function — can perform I/O"),
        "test" => Some("Test block"),
        "import" => Some("Module import"),
        _ => None,
    };

    info.map(|text| Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: text.to_string(),
        }),
        range: None,
    })
}

fn compute_completions(source: &str, pos: Position) -> Vec<CompletionItem> {
    let lines: Vec<&str> = source.lines().collect();
    let line = match lines.get(pos.line as usize) {
        Some(l) => *l,
        None => return vec![],
    };
    let col = pos.character as usize;
    let prefix = &line[..col.min(line.len())];

    if let Some(dot_pos) = prefix.rfind('.') {
        let module_start = prefix[..dot_pos].rfind(|c: char| !c.is_alphanumeric() && c != '_').map(|i| i + 1).unwrap_or(0);
        let module = &prefix[module_start..dot_pos];
        let partial = &prefix[dot_pos + 1..];
        let funcs = crate::stdlib::module_functions_all(module);
        return funcs.iter()
            .filter(|f| f.starts_with(partial))
            .map(|f| CompletionItem {
                label: f.to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: crate::stdlib::lookup_sig(module, f).map(|sig| {
                    let params = sig.params.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect::<Vec<_>>().join(", ");
                    format!("fn {}({}) -> {}", f, params, sig.ret.display())
                }),
                ..Default::default()
            })
            .collect();
    }

    let keywords = ["fn", "let", "var", "type", "match", "if", "then", "else", "for", "in",
                     "test", "import", "effect", "true", "false", "none", "some", "ok", "err", "mut"];
    let word_start = prefix.rfind(|c: char| !c.is_alphanumeric() && c != '_').map(|i| i + 1).unwrap_or(0);
    let partial = &prefix[word_start..];
    if partial.is_empty() { return vec![]; }

    keywords.iter()
        .filter(|k| k.starts_with(partial) && **k != partial)
        .map(|k| CompletionItem {
            label: k.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            ..Default::default()
        })
        .collect()
}
