use std::collections::HashMap;
use std::str::FromStr;
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
        document_symbol_provider: Some(OneOf::Left(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        definition_provider: Some(OneOf::Left(true)),
        signature_help_provider: Some(SignatureHelpOptions {
            trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
            retrigger_characters: None,
            work_done_progress_options: Default::default(),
        }),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Left(true)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        ..Default::default()
    }).unwrap();

    let init_params = match connection.initialize(server_capabilities) {
        Ok(it) => it,
        Err(e) => {
            eprintln!("LSP init failed: {}", e);
            return;
        }
    };
    let init: InitializeParams = serde_json::from_value(init_params).unwrap();
    let workspace_root = init.root_uri
        .as_ref()
        .and_then(|u| u.path().to_string().strip_prefix('/').or(Some(u.path().as_str())).map(|s| std::path::PathBuf::from(s.to_string())))
        .or_else(|| std::env::current_dir().ok());

    let mut documents: HashMap<Uri, String> = HashMap::new();

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req).unwrap_or(false) {
                    return;
                }
                let resp = handle_request(&req, &documents, &workspace_root);
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

fn handle_request(req: &Request, documents: &HashMap<Uri, String>, workspace_root: &Option<std::path::PathBuf>) -> Option<Response> {
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
        "textDocument/documentSymbol" => {
            let params: DocumentSymbolParams = serde_json::from_value(req.params.clone()).ok()?;
            let source = documents.get(&params.text_document.uri)?;
            let symbols = compute_document_symbols(source);
            let result = serde_json::to_value(DocumentSymbolResponse::Flat(symbols)).unwrap();
            Some(Response { id: req.id.clone(), result: Some(result), error: None })
        }
        "textDocument/formatting" => {
            let params: DocumentFormattingParams = serde_json::from_value(req.params.clone()).ok()?;
            let source = documents.get(&params.text_document.uri)?;
            let edits = compute_formatting(source);
            let result = serde_json::to_value(edits).unwrap();
            Some(Response { id: req.id.clone(), result: Some(result), error: None })
        }
        "textDocument/definition" => {
            let params: GotoDefinitionParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = &params.text_document_position_params.text_document.uri;
            let pos = params.text_document_position_params.position;
            let source = documents.get(uri)?;
            let loc = compute_definition(source, pos, uri);
            let result = loc.map(|l| serde_json::to_value(GotoDefinitionResponse::Scalar(l)).unwrap())
                .unwrap_or(serde_json::Value::Null);
            Some(Response { id: req.id.clone(), result: Some(result), error: None })
        }
        "textDocument/signatureHelp" => {
            let params: SignatureHelpParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = &params.text_document_position_params.text_document.uri;
            let pos = params.text_document_position_params.position;
            let source = documents.get(uri)?;
            let help = compute_signature_help(source, pos);
            let result = help.map(|h| serde_json::to_value(h).unwrap()).unwrap_or(serde_json::Value::Null);
            Some(Response { id: req.id.clone(), result: Some(result), error: None })
        }
        "workspace/symbol" => {
            let params: WorkspaceSymbolParams = serde_json::from_value(req.params.clone()).ok()?;
            let symbols = compute_workspace_symbols(&params.query, workspace_root);
            let result = serde_json::to_value(symbols).unwrap();
            Some(Response { id: req.id.clone(), result: Some(result), error: None })
        }
        "textDocument/rename" => {
            let params: RenameParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = &params.text_document_position.text_document.uri;
            let pos = params.text_document_position.position;
            let source = documents.get(uri)?;
            let edit = compute_rename(source, pos, uri, &params.new_name);
            let result = edit.map(|e| serde_json::to_value(e).unwrap()).unwrap_or(serde_json::Value::Null);
            Some(Response { id: req.id.clone(), result: Some(result), error: None })
        }
        "textDocument/codeAction" => {
            let params: CodeActionParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = &params.text_document.uri;
            let source = documents.get(uri)?;
            let actions = compute_code_actions(source, &params.context.diagnostics, uri);
            let result = serde_json::to_value(actions).unwrap();
            Some(Response { id: req.id.clone(), result: Some(result), error: None })
        }
        _ => None,
    }
}

fn uri_to_path(uri: &Uri) -> Option<String> {
    let s = uri.as_str();
    s.strip_prefix("file://").map(|p| p.to_string())
}

fn publish_diagnostics(connection: &Connection, uri: &Uri, source: &str) {
    let file_path = uri_to_path(uri);
    let diags = check_source(source, file_path.as_deref());
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

fn check_source(source: &str, file_path: Option<&str>) -> Vec<Diagnostic> {
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

    // Try cross-file import resolution when file path is available
    let resolved_modules = if let Some(fp) = file_path {
        resolve_imports_for_lsp(fp, &program)
    } else {
        vec![]
    };

    let canon = crate::canonicalize::canonicalize_program(
        &program,
        resolved_modules.iter().map(|(n, p, s)| (n.as_str(), p, *s)),
    );
    let mut checker = crate::check::Checker::from_env(canon.env);
    checker.source_text = Some(source.to_string());
    checker.diagnostics = canon.diagnostics;

    // Register imported module types
    for (name, mod_prog, _) in &resolved_modules {
        let mut mod_prog_clone = mod_prog.clone();
        checker.infer_module(&mut mod_prog_clone, name);
    }

    let check_diags = checker.infer_program(&mut program);

    let mut result = parse_errors;
    for d in &check_diags {
        result.push(diag_from_almide(d));
    }
    result
}

fn resolve_imports_for_lsp(file_path: &str, program: &crate::ast::Program) -> Vec<(String, crate::ast::Program, bool)> {
    // Find project root by looking for almide.toml
    let file = std::path::Path::new(file_path);
    let project_dir = file.parent();

    let dep_paths: Vec<(crate::project::PkgId, std::path::PathBuf)> = project_dir
        .and_then(|dir| {
            let toml = dir.join("almide.toml");
            if !toml.exists() { return None; }
            let proj = crate::project::parse_toml(&toml).ok()?;
            crate::project_fetch::fetch_all_deps(&proj).ok().map(|deps| {
                deps.into_iter().map(|fd| (fd.pkg_id, fd.source_dir)).collect()
            })
        })
        .unwrap_or_default();

    match crate::resolve::resolve_imports_with_deps(file_path, program, &dep_paths) {
        Ok(resolved) => {
            resolved.modules.into_iter()
                .map(|(name, prog, _, is_stdlib)| (name, prog, is_stdlib))
                .collect()
        }
        Err(_) => vec![],
    }
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

    // Module.func hover — cursor on module name (word = "string", after = "contains")
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

    // Module.func hover — cursor on func name (word = "contains", before = "string")
    if start > 0 && line.as_bytes()[start - 1] == b'.' {
        let mod_end = start - 1;
        let mod_start = line[..mod_end].rfind(|c: char| !c.is_alphanumeric() && c != '_').map(|i| i + 1).unwrap_or(0);
        let module = &line[mod_start..mod_end];
        if !module.is_empty() {
            if let Some(sig) = crate::stdlib::lookup_sig(module, word) {
                let params = sig.params.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect::<Vec<_>>().join(", ");
                return Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("```almide\nfn {}.{}({}) -> {}\n```", module, word, params, sig.ret.display()),
                    }),
                    range: None,
                });
            }
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
        "if" => Some("Conditional expression: `if cond then a else b`"),
        "then" => Some("Then branch of an if expression"),
        "else" => Some("Else branch of an if expression"),
        "for" => Some("For-in loop: `for item in collection { ... }`"),
        "in" => Some("Iterator binding in for loop"),
        "true" => Some("`Bool` literal (true)"),
        "false" => Some("`Bool` literal (false)"),
        "none" => Some("`Option[T]` — no value"),
        "some" => Some("`Option[T]` constructor — wraps a value"),
        "ok" => Some("`Result[T, E]` — success value"),
        "err" => Some("`Result[T, E]` — error value"),
        "assert" => Some("Test assertion: `assert(condition)` — fails the test if false"),
        "assert_eq" => Some("Test assertion: `assert_eq(actual, expected)` — fails if not equal"),
        _ => None,
    };

    if let Some(text) = info {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: text.to_string(),
            }),
            range: None,
        });
    }

    // Variable / function type from checker
    hover_from_checker(source, word)
}

fn hover_from_checker(source: &str, word: &str) -> Option<Hover> {
    let tokens = crate::lexer::Lexer::tokenize(source);
    let mut parser = crate::parser::Parser::new(tokens);
    let mut program = parser.parse().ok()?;
    let canon = crate::canonicalize::canonicalize_program(
        &program,
        std::iter::empty::<(&str, &crate::ast::Program, bool)>(),
    );
    let mut checker = crate::check::Checker::from_env(canon.env);
    checker.source_text = Some(source.to_string());
    checker.diagnostics = canon.diagnostics;
    let _ = checker.infer_program(&mut program);

    let sym = crate::intern::sym(word);

    // Check user-defined functions
    if let Some(sig) = checker.env.functions.get(&sym) {
        let params = sig.params.iter()
            .map(|(n, t)| format!("{}: {}", n, t.display()))
            .collect::<Vec<_>>().join(", ");
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("```almide\nfn {}({}) -> {}\n```", word, params, sig.ret.display()),
            }),
            range: None,
        });
    }

    // Check top-level lets
    if let Some(ty) = checker.env.top_lets.get(&sym) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("```almide\nlet {}: {}\n```", word, ty.display()),
            }),
            range: None,
        });
    }

    // Check variant constructors in type declarations
    for decl in &program.decls {
        if let crate::ast::Decl::Type { name: type_name, ty: crate::ast::TypeExpr::Variant { cases }, .. } = decl {
            for case in cases {
                let (case_name, fields) = match case {
                    crate::ast::VariantCase::Unit { name } => (name.as_str(), vec![]),
                    crate::ast::VariantCase::Tuple { name, fields } => (name.as_str(), fields.iter().map(|f| format_type_expr(f)).collect()),
                    crate::ast::VariantCase::Record { name, fields } => (name.as_str(), fields.iter().map(|f| format!("{}: {}", f.name.as_str(), format_type_expr(&f.ty))).collect()),
                };
                if case_name == word {
                    let display = if fields.is_empty() {
                        format!("```almide\n{} (variant of {})\n```", word, type_name.as_str())
                    } else {
                        format!("```almide\n{}({}) (variant of {})\n```", word, fields.join(", "), type_name.as_str())
                    };
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: display,
                        }),
                        range: None,
                    });
                }
            }
        }
    }

    None
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

// ── Phase 2: Document Symbols ──

fn compute_document_symbols(source: &str) -> Vec<SymbolInformation> {
    let tokens = crate::lexer::Lexer::tokenize(source);
    let mut parser = crate::parser::Parser::new(tokens);
    let prog = match parser.parse() {
        Ok(p) => p,
        Err(_) => return vec![],
    };
    let mut symbols = Vec::new();
    for decl in &prog.decls {
        let (name, kind, span) = match decl {
            crate::ast::Decl::Fn { name, span, .. } => {
                (name.as_str().to_string(), SymbolKind::FUNCTION, span)
            }
            crate::ast::Decl::Type { name, span, .. } => {
                (name.as_str().to_string(), SymbolKind::STRUCT, span)
            }
            crate::ast::Decl::TopLet { name, span, .. } => {
                (name.as_str().to_string(), SymbolKind::VARIABLE, span)
            }
            crate::ast::Decl::Test { name, span, .. } => {
                (format!("test \"{}\"", name), SymbolKind::METHOD, span)
            }
            _ => continue,
        };
        let line = span.as_ref().map(|s| s.line.saturating_sub(1) as u32).unwrap_or(0);
        let col = span.as_ref().map(|s| s.col.saturating_sub(1) as u32).unwrap_or(0);
        #[allow(deprecated)]
        symbols.push(SymbolInformation {
            name,
            kind,
            location: Location {
                uri: Uri::from_str("file:///").unwrap(),
                range: Range {
                    start: Position { line, character: col },
                    end: Position { line, character: col },
                },
            },
            tags: None,
            deprecated: None,
            container_name: None,
        });
    }
    symbols
}

// ── Phase 2: Formatting ──

fn compute_formatting(source: &str) -> Vec<TextEdit> {
    let tokens = crate::lexer::Lexer::tokenize(source);
    let mut parser = crate::parser::Parser::new(tokens);
    let program = match parser.parse() {
        Ok(p) => p,
        Err(_) => return vec![],
    };
    let formatted = crate::fmt::format_program(&program);
    if formatted == source {
        return vec![];
    }
    let line_count = source.lines().count().max(1);
    vec![TextEdit {
        range: Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: line_count as u32, character: 0 },
        },
        new_text: formatted,
    }]
}

// ── Phase 2: Go to Definition ──

fn compute_definition(source: &str, pos: Position, uri: &Uri) -> Option<Location> {
    let lines: Vec<&str> = source.lines().collect();
    let line = lines.get(pos.line as usize)?;
    let col = pos.character as usize;
    if col >= line.len() { return None; }

    let start = line[..col].rfind(|c: char| !c.is_alphanumeric() && c != '_').map(|i| i + 1).unwrap_or(0);
    let end = col + line[col..].find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(line.len() - col);
    let word = &line[start..end];
    if word.is_empty() { return None; }

    // Search for declaration of this name in the source
    let tokens = crate::lexer::Lexer::tokenize(source);
    let mut parser = crate::parser::Parser::new(tokens);
    let prog = match parser.parse() {
        Ok(p) => p,
        Err(_) => return None,
    };

    for decl in &prog.decls {
        // Direct declaration match
        let (name, span) = match decl {
            crate::ast::Decl::Fn { name, span, .. } => (name.as_str(), span),
            crate::ast::Decl::Type { name, span, .. } => (name.as_str(), span),
            crate::ast::Decl::TopLet { name, span, .. } => (name.as_str(), span),
            _ => continue,
        };
        if name == word {
            let def_line = span.as_ref().map(|s| s.line.saturating_sub(1) as u32).unwrap_or(0);
            let def_col = span.as_ref().map(|s| s.col.saturating_sub(1) as u32).unwrap_or(0);
            return Some(Location {
                uri: uri.clone(),
                range: Range {
                    start: Position { line: def_line, character: def_col },
                    end: Position { line: def_line, character: def_col + name.len() as u32 },
                },
            });
        }
        // Variant constructor match — jump to the parent type declaration
        if let crate::ast::Decl::Type { ty: crate::ast::TypeExpr::Variant { cases }, span, .. } = decl {
            for case in cases {
                let case_name = match case {
                    crate::ast::VariantCase::Unit { name } => name.as_str(),
                    crate::ast::VariantCase::Tuple { name, .. } => name.as_str(),
                    crate::ast::VariantCase::Record { name, .. } => name.as_str(),
                };
                if case_name == word {
                    let def_line = span.as_ref().map(|s| s.line.saturating_sub(1) as u32).unwrap_or(0);
                    let def_col = span.as_ref().map(|s| s.col.saturating_sub(1) as u32).unwrap_or(0);
                    return Some(Location {
                        uri: uri.clone(),
                        range: Range {
                            start: Position { line: def_line, character: def_col },
                            end: Position { line: def_line, character: def_col },
                        },
                    });
                }
            }
        }
    }
    None
}

// ── Phase 2: Signature Help ──

fn compute_signature_help(source: &str, pos: Position) -> Option<SignatureHelp> {
    let lines: Vec<&str> = source.lines().collect();
    let line = lines.get(pos.line as usize)?;
    let col = pos.character as usize;
    let prefix = &line[..col.min(line.len())];

    // Find the innermost unclosed `(` to determine the function being called
    let mut depth = 0i32;
    let mut call_end = None;
    let mut active_param = 0u32;
    for (i, ch) in prefix.char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' => {
                if depth == 0 {
                    call_end = Some(i);
                    break;
                }
                depth -= 1;
            }
            ',' if depth == 0 => active_param += 1,
            _ => {}
        }
    }
    let paren_pos = call_end?;
    let before_paren = prefix[..paren_pos].trim_end();

    // Extract function name (possibly module.func)
    let name_start = before_paren.rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .map(|i| i + 1).unwrap_or(0);
    let func_name = &before_paren[name_start..];
    if func_name.is_empty() { return None; }

    // Try module.func lookup
    if let Some(dot) = func_name.rfind('.') {
        let module = &func_name[..dot];
        let func = &func_name[dot + 1..];
        if let Some(sig) = crate::stdlib::lookup_sig(module, func) {
            let params: Vec<ParameterInformation> = sig.params.iter().map(|(n, t)| {
                ParameterInformation {
                    label: ParameterLabel::Simple(format!("{}: {}", n, t.display())),
                    documentation: None,
                }
            }).collect();
            let params_str = sig.params.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect::<Vec<_>>().join(", ");
            return Some(SignatureHelp {
                signatures: vec![SignatureInformation {
                    label: format!("fn {}.{}({}) -> {}", module, func, params_str, sig.ret.display()),
                    documentation: None,
                    parameters: Some(params),
                    active_parameter: Some(active_param),
                }],
                active_signature: Some(0),
                active_parameter: Some(active_param),
            });
        }
    }

    // Try user-defined function lookup
    let tokens = crate::lexer::Lexer::tokenize(source);
    let mut parser = crate::parser::Parser::new(tokens);
    if let Ok(prog) = parser.parse() {
        for decl in &prog.decls {
            if let crate::ast::Decl::Fn { name, params, return_type, .. } = decl {
                if name.as_str() == func_name {
                    let param_infos: Vec<ParameterInformation> = params.iter().map(|p| {
                        ParameterInformation {
                            label: ParameterLabel::Simple(format!("{}: {}", p.name.as_str(), format_type_expr(&p.ty))),
                            documentation: None,
                        }
                    }).collect();
                    let params_str = params.iter().map(|p| format!("{}: {}", p.name.as_str(), format_type_expr(&p.ty))).collect::<Vec<_>>().join(", ");
                    return Some(SignatureHelp {
                        signatures: vec![SignatureInformation {
                            label: format!("fn {}({}) -> {}", func_name, params_str, format_type_expr(return_type)),
                            documentation: None,
                            parameters: Some(param_infos),
                            active_parameter: Some(active_param),
                        }],
                        active_signature: Some(0),
                        active_parameter: Some(active_param),
                    });
                }
            }
        }
    }
    None
}

fn format_type_expr(te: &crate::ast::TypeExpr) -> String {
    match te {
        crate::ast::TypeExpr::Simple { name } => name.as_str().to_string(),
        crate::ast::TypeExpr::Generic { name, args } => {
            let args_str = args.iter().map(|a| format_type_expr(a)).collect::<Vec<_>>().join(", ");
            format!("{}[{}]", name.as_str(), args_str)
        }
        crate::ast::TypeExpr::Tuple { elements } => {
            let s = elements.iter().map(|e| format_type_expr(e)).collect::<Vec<_>>().join(", ");
            format!("({})", s)
        }
        crate::ast::TypeExpr::Fn { params, ret } => {
            let s = params.iter().map(|p| format_type_expr(p)).collect::<Vec<_>>().join(", ");
            format!("({}) -> {}", s, format_type_expr(ret))
        }
        _ => "?".to_string(),
    }
}

// ── Rename ──

fn compute_rename(source: &str, pos: Position, uri: &Uri, new_name: &str) -> Option<WorkspaceEdit> {
    let lines: Vec<&str> = source.lines().collect();
    let line = lines.get(pos.line as usize)?;
    let col = pos.character as usize;
    if col >= line.len() { return None; }

    let start = line[..col].rfind(|c: char| !c.is_alphanumeric() && c != '_').map(|i| i + 1).unwrap_or(0);
    let end = col + line[col..].find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(line.len() - col);
    let old_name = &line[start..end];
    if old_name.is_empty() { return None; }

    // Find all word-boundary occurrences of old_name in the source
    let mut edits = Vec::new();
    for (line_idx, line_text) in lines.iter().enumerate() {
        let mut search_from = 0;
        while let Some(found) = line_text[search_from..].find(old_name) {
            let abs_pos = search_from + found;
            let before_ok = abs_pos == 0 || !line_text.as_bytes()[abs_pos - 1].is_ascii_alphanumeric() && line_text.as_bytes()[abs_pos - 1] != b'_';
            let after_pos = abs_pos + old_name.len();
            let after_ok = after_pos >= line_text.len() || !line_text.as_bytes()[after_pos].is_ascii_alphanumeric() && line_text.as_bytes()[after_pos] != b'_';
            if before_ok && after_ok {
                edits.push(TextEdit {
                    range: Range {
                        start: Position { line: line_idx as u32, character: abs_pos as u32 },
                        end: Position { line: line_idx as u32, character: after_pos as u32 },
                    },
                    new_text: new_name.to_string(),
                });
            }
            search_from = abs_pos + old_name.len();
        }
    }

    if edits.is_empty() { return None; }
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);
    Some(WorkspaceEdit { changes: Some(changes), ..Default::default() })
}

// ── Code Actions ──

fn compute_code_actions(source: &str, diagnostics: &[Diagnostic], uri: &Uri) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for diag in diagnostics {
        let code = diag.code.as_ref().and_then(|c| match c {
            NumberOrString::String(s) => Some(s.as_str()),
            _ => None,
        });

        match code {
            // E003: undefined variable — suggest import if it's a known module
            Some("E003") => {
                let word = extract_word_from_diag(&diag.message);
                if let Some(module) = word {
                    let known_modules = ["io", "json", "env", "fs", "http", "regex", "random",
                                         "testing", "datetime", "bytes", "html", "path", "channel"];
                    if known_modules.contains(&module.as_str()) {
                        let import_line = format!("import {}\n", module);
                        // Insert at line 0 (or after existing imports)
                        let insert_line = find_import_insert_line(&lines);
                        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                            title: format!("Import '{}'", module),
                            kind: Some(CodeActionKind::QUICKFIX),
                            diagnostics: Some(vec![diag.clone()]),
                            edit: Some(WorkspaceEdit {
                                changes: Some(HashMap::from([(uri.clone(), vec![TextEdit {
                                    range: Range {
                                        start: Position { line: insert_line, character: 0 },
                                        end: Position { line: insert_line, character: 0 },
                                    },
                                    new_text: import_line,
                                }])])),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }));
                    }
                }
            }
            // E006: effect isolation — suggest marking function as effect fn
            Some("E006") => {
                if let Some(fn_line) = find_enclosing_fn_line(&lines, diag.range.start.line) {
                    let line_text = lines[fn_line as usize];
                    if line_text.contains("fn ") && !line_text.contains("effect fn") {
                        let col = line_text.find("fn ").unwrap() as u32;
                        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                            title: "Mark as effect fn".to_string(),
                            kind: Some(CodeActionKind::QUICKFIX),
                            diagnostics: Some(vec![diag.clone()]),
                            edit: Some(WorkspaceEdit {
                                changes: Some(HashMap::from([(uri.clone(), vec![TextEdit {
                                    range: Range {
                                        start: Position { line: fn_line, character: col },
                                        end: Position { line: fn_line, character: col + 2 },
                                    },
                                    new_text: "effect fn".to_string(),
                                }])])),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }));
                    }
                }
            }
            _ => {}
        }
    }
    actions
}

fn extract_word_from_diag(message: &str) -> Option<String> {
    // Extract 'name' from "undefined variable 'name'" or similar
    if let Some(start) = message.find('\'') {
        let rest = &message[start + 1..];
        if let Some(end) = rest.find('\'') {
            return Some(rest[..end].to_string());
        }
    }
    None
}

fn find_import_insert_line(lines: &[&str]) -> u32 {
    let mut last_import = 0u32;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") {
            last_import = (i + 1) as u32;
        }
    }
    last_import
}

fn find_enclosing_fn_line(lines: &[&str], diag_line: u32) -> Option<u32> {
    for i in (0..=diag_line as usize).rev() {
        let trimmed = lines.get(i)?.trim();
        if trimmed.starts_with("fn ") || trimmed.starts_with("effect fn ")
            || trimmed.contains(" fn ") {
            return Some(i as u32);
        }
    }
    None
}

// ── Phase 3: Workspace Symbols ──

fn compute_workspace_symbols(query: &str, workspace_root: &Option<std::path::PathBuf>) -> Vec<SymbolInformation> {
    let root = match workspace_root {
        Some(r) => r.clone(),
        None => return vec![],
    };
    let mut results = Vec::new();
    let mut files = Vec::new();
    collect_almd_files(&root, &mut files);

    let query_lower = query.to_lowercase();
    for file_path in &files {
        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let tokens = crate::lexer::Lexer::tokenize(&source);
        let mut parser = crate::parser::Parser::new(tokens);
        let prog = match parser.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let file_uri = Uri::from_str(&format!("file://{}", file_path.display())).ok();
        let file_uri = match file_uri {
            Some(u) => u,
            None => continue,
        };
        for decl in &prog.decls {
            let (name, kind, span) = match decl {
                crate::ast::Decl::Fn { name, span, .. } => (name.as_str(), SymbolKind::FUNCTION, span),
                crate::ast::Decl::Type { name, span, .. } => (name.as_str(), SymbolKind::STRUCT, span),
                crate::ast::Decl::TopLet { name, span, .. } => (name.as_str(), SymbolKind::VARIABLE, span),
                _ => continue,
            };
            if !query.is_empty() && !name.to_lowercase().contains(&query_lower) {
                continue;
            }
            let line = span.as_ref().map(|s| s.line.saturating_sub(1) as u32).unwrap_or(0);
            let col = span.as_ref().map(|s| s.col.saturating_sub(1) as u32).unwrap_or(0);
            #[allow(deprecated)]
            results.push(SymbolInformation {
                name: name.to_string(),
                kind,
                location: Location {
                    uri: file_uri.clone(),
                    range: Range {
                        start: Position { line, character: col },
                        end: Position { line, character: col + name.len() as u32 },
                    },
                },
                tags: None,
                deprecated: None,
                container_name: file_path.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string()),
            });
        }
    }
    results
}

fn collect_almd_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if dir_name.starts_with('.') || dir_name == "target" || dir_name == "node_modules" {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                collect_almd_files(&path, out);
            } else if path.extension().map_or(false, |e| e == "almd") {
                out.push(path);
            }
        }
    }
}
