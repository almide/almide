use std::collections::HashMap;
use std::str::FromStr;
use lsp_server::{Connection, Message, Request, Response, Notification};
use lsp_types::*;

// ══════════════════════════════════════════════════════════════
// Analyzed Document — Gleam-style cached analysis per file
// ══════════════════════════════════════════════════════════════

struct AnalyzedDoc {
    source: String,
    program: crate::ast::Program,
    checker: crate::check::Checker,
    lsp_diagnostics: Vec<Diagnostic>,
}

impl AnalyzedDoc {
    fn analyze(source: &str, file_path: Option<&str>) -> Self {
        let tokens = crate::lexer::Lexer::tokenize(source);
        let mut parser = crate::parser::Parser::new(tokens);
        let (mut program, parse_errors) = match parser.parse() {
            Ok(p) => {
                let errs: Vec<Diagnostic> = parser.errors.iter().map(|e| diag_from_almide(e)).collect();
                (p, errs)
            }
            Err(_) => {
                let errs = parser.errors.iter().map(|e| diag_from_almide(e)).collect();
                return AnalyzedDoc {
                    source: source.to_string(),
                    program: empty_program(),
                    checker: empty_checker(),
                    lsp_diagnostics: errs,
                };
            }
        };

        if parser.errors.iter().any(|e| e.level == crate::diagnostic::Level::Error) {
            return AnalyzedDoc {
                source: source.to_string(),
                program,
                checker: empty_checker(),
                lsp_diagnostics: parse_errors,
            };
        }

        // Cross-file import resolution
        let resolved_modules = file_path
            .map(|fp| resolve_imports_for_lsp(fp, &program))
            .unwrap_or_default();

        let canon = crate::canonicalize::canonicalize_program(
            &program,
            resolved_modules.iter().map(|(n, p, s)| (n.as_str(), p, *s)),
        );
        let mut checker = crate::check::Checker::from_env(canon.env);
        checker.source_text = Some(source.to_string());
        checker.diagnostics = canon.diagnostics;

        for (name, mod_prog, _) in &resolved_modules {
            let mut mod_prog_clone = mod_prog.clone();
            checker.infer_module(&mut mod_prog_clone, name);
        }

        let check_diags = checker.infer_program(&mut program);
        let mut diags = parse_errors;
        for d in &check_diags {
            diags.push(diag_from_almide(d));
        }

        AnalyzedDoc {
            source: source.to_string(),
            program,
            checker,
            lsp_diagnostics: diags,
        }
    }
}

// ══════════════════════════════════════════════════════════════
// Located — what the cursor is on (Gleam-style)
// ══════════════════════════════════════════════════════════════

enum Located {
    Keyword { info: &'static str },
    FnDecl { name: String, params: String, ret: String },
    TypeDecl { name: String },
    TopLet { name: String, ty: String },
    VariantConstructor { name: String, type_name: String, fields: Vec<String> },
    StdlibCall { module: String, func: String, params: String, ret: String },
    UserIdent { name: String, ty: String },
    Param { name: String, ty: String },
    Expr { ty: String },
}

fn span_contains(span: &crate::ast::Span, line: u32, col: u32) -> bool {
    let sl = span.line as u32;
    let sc = span.col.saturating_sub(1) as u32;
    let ec = span.end_col as u32;
    sl == line + 1 && col >= sc && col < ec
}

fn find_node(doc: &AnalyzedDoc, line: u32, col: u32) -> Option<Located> {
    let source = &doc.source;
    let lines: Vec<&str> = source.lines().collect();
    let line_text = lines.get(line as usize)?;
    let col_usize = col as usize;
    if col_usize >= line_text.len() { return None; }

    // Extract word at cursor
    let start = line_text[..col_usize].rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1).unwrap_or(0);
    let end = col_usize + line_text[col_usize..].find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(line_text.len() - col_usize);
    let word = &line_text[start..end];
    if word.is_empty() { return None; }

    // 1. Keywords
    let kw = match word {
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
    if let Some(info) = kw {
        return Some(Located::Keyword { info });
    }

    // 2. module.func — cursor on module name
    if end < line_text.len() && line_text.as_bytes()[end] == b'.' {
        let func_start = end + 1;
        let func_end = func_start + line_text[func_start..].find(|c: char| !c.is_alphanumeric() && c != '_' && c != '?').unwrap_or(line_text.len() - func_start);
        let func = &line_text[func_start..func_end];
        if let Some(sig) = crate::stdlib::lookup_sig(word, func) {
            let params = sig.params.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect::<Vec<_>>().join(", ");
            return Some(Located::StdlibCall { module: word.to_string(), func: func.to_string(), params, ret: sig.ret.display().to_string() });
        }
    }

    // 3. module.func — cursor on func name
    if start > 0 && line_text.as_bytes()[start - 1] == b'.' {
        let mod_end = start - 1;
        let mod_start = line_text[..mod_end].rfind(|c: char| !c.is_alphanumeric() && c != '_').map(|i| i + 1).unwrap_or(0);
        let module = &line_text[mod_start..mod_end];
        if !module.is_empty() {
            if let Some(sig) = crate::stdlib::lookup_sig(module, word) {
                let params = sig.params.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect::<Vec<_>>().join(", ");
                return Some(Located::StdlibCall { module: module.to_string(), func: word.to_string(), params, ret: sig.ret.display().to_string() });
            }
        }
    }

    // 4. AST-based lookup — walk declarations
    let sym = crate::intern::sym(word);

    // 4a. Variant constructors
    for decl in &doc.program.decls {
        if let crate::ast::Decl::Type { name: type_name, ty: crate::ast::TypeExpr::Variant { cases }, .. } = decl {
            for case in cases {
                let (case_name, fields) = match case {
                    crate::ast::VariantCase::Unit { name } => (name.as_str(), vec![]),
                    crate::ast::VariantCase::Tuple { name, fields } => (name.as_str(), fields.iter().map(|f| format_type_expr(f)).collect()),
                    crate::ast::VariantCase::Record { name, fields } => (name.as_str(), fields.iter().map(|f| format!("{}: {}", f.name.as_str(), format_type_expr(&f.ty))).collect()),
                };
                if case_name == word {
                    return Some(Located::VariantConstructor {
                        name: word.to_string(),
                        type_name: type_name.as_str().to_string(),
                        fields,
                    });
                }
            }
        }
    }

    // 4b. Function declarations
    if let Some(sig) = doc.checker.env.functions.get(&sym) {
        let params = sig.params.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect::<Vec<_>>().join(", ");
        return Some(Located::FnDecl { name: word.to_string(), params, ret: sig.ret.display().to_string() });
    }

    // 4c. Top-level lets
    if let Some(ty) = doc.checker.env.top_lets.get(&sym) {
        return Some(Located::TopLet { name: word.to_string(), ty: ty.display().to_string() });
    }

    // 4d. Function parameters — check if cursor is inside a fn body
    for decl in &doc.program.decls {
        if let crate::ast::Decl::Fn { params, span, .. } = decl {
            let fn_line = span.as_ref().map(|s| s.line as u32).unwrap_or(0);
            // Heuristic: if cursor is within ~100 lines of fn declaration, check params
            if line + 1 >= fn_line && line + 1 < fn_line + 100 {
                for p in params {
                    if p.name.as_str() == word {
                        return Some(Located::Param {
                            name: word.to_string(),
                            ty: format_type_expr(&p.ty),
                        });
                    }
                }
            }
        }
    }

    // 4e. ExprId-based type lookup — walk expressions to find matching Ident
    for decl in &doc.program.decls {
        if let Some(ty) = find_expr_type_by_name(&doc.program, decl, word, &doc.checker.type_map) {
            return Some(Located::UserIdent { name: word.to_string(), ty: ty.display().to_string() });
        }
    }

    None
}

fn find_expr_type_by_name(
    _program: &crate::ast::Program,
    decl: &crate::ast::Decl,
    name: &str,
    type_map: &crate::types::TypeMap,
) -> Option<crate::types::Ty> {
    let body = match decl {
        crate::ast::Decl::Fn { body: Some(body), .. } => body,
        crate::ast::Decl::TopLet { value, .. } => value,
        crate::ast::Decl::Test { body, .. } => body,
        _ => return None,
    };
    find_ident_type(body, name, type_map)
}

fn find_ident_type(expr: &crate::ast::Expr, name: &str, type_map: &crate::types::TypeMap) -> Option<crate::types::Ty> {
    match &expr.kind {
        crate::ast::ExprKind::Ident { name: n } if n.as_str() == name => {
            type_map.get(&expr.id).cloned()
        }
        crate::ast::ExprKind::Block { stmts, expr: tail } => {
            for stmt in stmts {
                if let Some(ty) = find_ident_in_stmt(stmt, name, type_map) {
                    return Some(ty);
                }
            }
            if let Some(e) = tail {
                find_ident_type(e, name, type_map)
            } else {
                None
            }
        }
        crate::ast::ExprKind::Call { callee, args, .. } => {
            find_ident_type(callee, name, type_map)
                .or_else(|| args.iter().find_map(|a| find_ident_type(a, name, type_map)))
        }
        crate::ast::ExprKind::If { cond, then, else_ } => {
            find_ident_type(cond, name, type_map)
                .or_else(|| find_ident_type(then, name, type_map))
                .or_else(|| find_ident_type(else_, name, type_map))
        }
        crate::ast::ExprKind::Lambda { body, .. } => find_ident_type(body, name, type_map),
        crate::ast::ExprKind::Pipe { left, right } => {
            find_ident_type(left, name, type_map)
                .or_else(|| find_ident_type(right, name, type_map))
        }
        crate::ast::ExprKind::Member { object, .. } => find_ident_type(object, name, type_map),
        crate::ast::ExprKind::Match { subject, arms } => {
            find_ident_type(subject, name, type_map)
                .or_else(|| arms.iter().find_map(|a| find_ident_type(&a.body, name, type_map)))
        }
        crate::ast::ExprKind::List { elements } => {
            elements.iter().find_map(|e| find_ident_type(e, name, type_map))
        }
        _ => None,
    }
}

fn find_ident_in_stmt(stmt: &crate::ast::Stmt, name: &str, type_map: &crate::types::TypeMap) -> Option<crate::types::Ty> {
    match stmt {
        crate::ast::Stmt::Let { value, .. }
        | crate::ast::Stmt::Var { value, .. } => find_ident_type(value, name, type_map),
        crate::ast::Stmt::Assign { value, .. } => find_ident_type(value, name, type_map),
        crate::ast::Stmt::Expr { expr, .. } => find_ident_type(expr, name, type_map),
        crate::ast::Stmt::Guard { cond, else_, .. } => {
            find_ident_type(cond, name, type_map)
                .or_else(|| find_ident_type(else_, name, type_map))
        }
        _ => None,
    }
}

// ══════════════════════════════════════════════════════════════
// LSP Server
// ══════════════════════════════════════════════════════════════

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
        Err(e) => { eprintln!("LSP init failed: {}", e); return; }
    };
    let init: InitializeParams = serde_json::from_value(init_params).unwrap();
    let workspace_root = init.root_uri.as_ref()
        .and_then(|u| u.path().to_string().strip_prefix('/').or(Some(u.path().as_str())).map(|s| std::path::PathBuf::from(s.to_string())))
        .or_else(|| std::env::current_dir().ok());

    let mut documents: HashMap<Uri, String> = HashMap::new();
    let mut analyzed: HashMap<Uri, AnalyzedDoc> = HashMap::new();

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req).unwrap_or(false) { return; }
                let resp = handle_request(&req, &documents, &analyzed, &workspace_root);
                if let Some(r) = resp {
                    connection.sender.send(Message::Response(r)).ok();
                }
            }
            Message::Notification(notif) => {
                match notif.method.as_str() {
                    "textDocument/didOpen" => {
                        if let Ok(params) = serde_json::from_value::<DidOpenTextDocumentParams>(notif.params) {
                            let uri = params.text_document.uri.clone();
                            let source = params.text_document.text;
                            let file_path = uri_to_path(&uri);
                            let doc = AnalyzedDoc::analyze(&source, file_path.as_deref());
                            publish_diagnostics(&connection, &uri, &doc.lsp_diagnostics);
                            documents.insert(uri.clone(), source);
                            analyzed.insert(uri, doc);
                        }
                    }
                    "textDocument/didChange" => {
                        if let Ok(params) = serde_json::from_value::<DidChangeTextDocumentParams>(notif.params) {
                            let uri = params.text_document.uri.clone();
                            if let Some(change) = params.content_changes.into_iter().last() {
                                let source = change.text;
                                let file_path = uri_to_path(&uri);
                                let doc = AnalyzedDoc::analyze(&source, file_path.as_deref());
                                publish_diagnostics(&connection, &uri, &doc.lsp_diagnostics);
                                documents.insert(uri.clone(), source);
                                analyzed.insert(uri, doc);
                            }
                        }
                    }
                    "textDocument/didClose" => {
                        if let Ok(params) = serde_json::from_value::<DidCloseTextDocumentParams>(notif.params) {
                            documents.remove(&params.text_document.uri);
                            analyzed.remove(&params.text_document.uri);
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

fn handle_request(req: &Request, documents: &HashMap<Uri, String>, analyzed: &HashMap<Uri, AnalyzedDoc>, workspace_root: &Option<std::path::PathBuf>) -> Option<Response> {
    match req.method.as_str() {
        "textDocument/hover" => {
            let params: HoverParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = &params.text_document_position_params.text_document.uri;
            let pos = params.text_document_position_params.position;
            let doc = analyzed.get(uri)?;
            let hover = compute_hover(doc, pos);
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
            let doc = analyzed.get(&params.text_document.uri)?;
            let symbols = compute_document_symbols(doc);
            let result = serde_json::to_value(DocumentSymbolResponse::Flat(symbols)).unwrap();
            Some(Response { id: req.id.clone(), result: Some(result), error: None })
        }
        "textDocument/formatting" => {
            let params: DocumentFormattingParams = serde_json::from_value(req.params.clone()).ok()?;
            let doc = analyzed.get(&params.text_document.uri)?;
            let edits = compute_formatting(doc);
            let result = serde_json::to_value(edits).unwrap();
            Some(Response { id: req.id.clone(), result: Some(result), error: None })
        }
        "textDocument/definition" => {
            let params: GotoDefinitionParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = &params.text_document_position_params.text_document.uri;
            let pos = params.text_document_position_params.position;
            let doc = analyzed.get(uri)?;
            let loc = compute_definition(doc, pos, uri);
            let result = loc.map(|l| serde_json::to_value(GotoDefinitionResponse::Scalar(l)).unwrap())
                .unwrap_or(serde_json::Value::Null);
            Some(Response { id: req.id.clone(), result: Some(result), error: None })
        }
        "textDocument/signatureHelp" => {
            let params: SignatureHelpParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = &params.text_document_position_params.text_document.uri;
            let pos = params.text_document_position_params.position;
            let source = documents.get(uri)?;
            let doc = analyzed.get(uri);
            let help = compute_signature_help(source, pos, doc);
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

// ══════════════════════════════════════════════════════════════
// Hover — dispatches on Located
// ══════════════════════════════════════════════════════════════

fn compute_hover(doc: &AnalyzedDoc, pos: Position) -> Option<Hover> {
    let located = find_node(doc, pos.line, pos.character)?;
    let md = match located {
        Located::Keyword { info } => info.to_string(),
        Located::StdlibCall { module, func, params, ret } =>
            format!("```almide\nfn {}.{}({}) -> {}\n```", module, func, params, ret),
        Located::FnDecl { name, params, ret } =>
            format!("```almide\nfn {}({}) -> {}\n```", name, params, ret),
        Located::TopLet { name, ty } =>
            format!("```almide\nlet {}: {}\n```", name, ty),
        Located::VariantConstructor { name, type_name, fields } => {
            if fields.is_empty() {
                format!("```almide\n{} (variant of {})\n```", name, type_name)
            } else {
                format!("```almide\n{}({}) (variant of {})\n```", name, fields.join(", "), type_name)
            }
        }
        Located::TypeDecl { name } =>
            format!("```almide\ntype {}\n```", name),
        Located::UserIdent { name, ty } =>
            format!("```almide\n{}: {}\n```", name, ty),
        Located::Param { name, ty } =>
            format!("```almide\n{}: {} (parameter)\n```", name, ty),
        Located::Expr { ty } =>
            format!("```almide\n{}\n```", ty),
    };
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value: md }),
        range: None,
    })
}

// ══════════════════════════════════════════════════════════════
// Go to Definition — dispatches on Located word, walks AST for declaration
// ══════════════════════════════════════════════════════════════

fn compute_definition(doc: &AnalyzedDoc, pos: Position, uri: &Uri) -> Option<Location> {
    let lines: Vec<&str> = doc.source.lines().collect();
    let line = lines.get(pos.line as usize)?;
    let col = pos.character as usize;
    if col >= line.len() { return None; }
    let start = line[..col].rfind(|c: char| !c.is_alphanumeric() && c != '_').map(|i| i + 1).unwrap_or(0);
    let end = col + line[col..].find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(line.len() - col);
    let word = &line[start..end];
    if word.is_empty() { return None; }

    for decl in &doc.program.decls {
        let (name, span) = match decl {
            crate::ast::Decl::Fn { name, span, .. } => (name.as_str(), span),
            crate::ast::Decl::Type { name, span, .. } => (name.as_str(), span),
            crate::ast::Decl::TopLet { name, span, .. } => (name.as_str(), span),
            _ => continue,
        };
        if name == word {
            return span_to_location(span, uri);
        }
        // Variant constructors
        if let crate::ast::Decl::Type { ty: crate::ast::TypeExpr::Variant { cases }, span, .. } = decl {
            for case in cases {
                let case_name = match case {
                    crate::ast::VariantCase::Unit { name } => name.as_str(),
                    crate::ast::VariantCase::Tuple { name, .. } => name.as_str(),
                    crate::ast::VariantCase::Record { name, .. } => name.as_str(),
                };
                if case_name == word {
                    return span_to_location(span, uri);
                }
            }
        }
    }
    None
}

fn span_to_location(span: &Option<crate::ast::Span>, uri: &Uri) -> Option<Location> {
    let s = span.as_ref()?;
    let line = s.line.saturating_sub(1) as u32;
    let col = s.col.saturating_sub(1) as u32;
    Some(Location {
        uri: uri.clone(),
        range: Range {
            start: Position { line, character: col },
            end: Position { line, character: s.end_col as u32 },
        },
    })
}

// ══════════════════════════════════════════════════════════════
// Completion — text-based (fast, doesn't need analysis)
// ══════════════════════════════════════════════════════════════

fn compute_completions(source: &str, pos: Position) -> Vec<CompletionItem> {
    let lines: Vec<&str> = source.lines().collect();
    let line = match lines.get(pos.line as usize) { Some(l) => *l, None => return vec![] };
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
        .map(|k| CompletionItem { label: k.to_string(), kind: Some(CompletionItemKind::KEYWORD), ..Default::default() })
        .collect()
}

// ══════════════════════════════════════════════════════════════
// Document Symbols
// ══════════════════════════════════════════════════════════════

fn compute_document_symbols(doc: &AnalyzedDoc) -> Vec<SymbolInformation> {
    let mut symbols = Vec::new();
    for decl in &doc.program.decls {
        let (name, kind, span) = match decl {
            crate::ast::Decl::Fn { name, span, .. } => (name.as_str().to_string(), SymbolKind::FUNCTION, span),
            crate::ast::Decl::Type { name, span, .. } => (name.as_str().to_string(), SymbolKind::STRUCT, span),
            crate::ast::Decl::TopLet { name, span, .. } => (name.as_str().to_string(), SymbolKind::VARIABLE, span),
            crate::ast::Decl::Test { name, span, .. } => (format!("test \"{}\"", name), SymbolKind::METHOD, span),
            _ => continue,
        };
        let line = span.as_ref().map(|s| s.line.saturating_sub(1) as u32).unwrap_or(0);
        let col = span.as_ref().map(|s| s.col.saturating_sub(1) as u32).unwrap_or(0);
        #[allow(deprecated)]
        symbols.push(SymbolInformation {
            name, kind,
            location: Location { uri: Uri::from_str("file:///").unwrap(), range: Range { start: Position { line, character: col }, end: Position { line, character: col } } },
            tags: None, deprecated: None, container_name: None,
        });
    }
    symbols
}

// ══════════════════════════════════════════════════════════════
// Formatting
// ══════════════════════════════════════════════════════════════

fn compute_formatting(doc: &AnalyzedDoc) -> Vec<TextEdit> {
    let formatted = crate::fmt::format_program(&doc.program);
    if formatted == doc.source { return vec![]; }
    let line_count = doc.source.lines().count().max(1);
    vec![TextEdit {
        range: Range { start: Position { line: 0, character: 0 }, end: Position { line: line_count as u32, character: 0 } },
        new_text: formatted,
    }]
}

// ══════════════════════════════════════════════════════════════
// Signature Help
// ══════════════════════════════════════════════════════════════

fn compute_signature_help(source: &str, pos: Position, doc: Option<&AnalyzedDoc>) -> Option<SignatureHelp> {
    let lines: Vec<&str> = source.lines().collect();
    let line = lines.get(pos.line as usize)?;
    let prefix = &line[..pos.character as usize];

    let mut depth = 0i32;
    let mut call_end = None;
    let mut active_param = 0u32;
    for (i, ch) in prefix.char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' => { if depth == 0 { call_end = Some(i); break; } depth -= 1; }
            ',' if depth == 0 => active_param += 1,
            _ => {}
        }
    }
    let paren_pos = call_end?;
    let before = prefix[..paren_pos].trim_end();
    let name_start = before.rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.').map(|i| i + 1).unwrap_or(0);
    let func_name = &before[name_start..];
    if func_name.is_empty() { return None; }

    // stdlib module.func
    if let Some(dot) = func_name.rfind('.') {
        let module = &func_name[..dot];
        let func = &func_name[dot + 1..];
        if let Some(sig) = crate::stdlib::lookup_sig(module, func) {
            return Some(make_sig_help(
                &format!("fn {}.{}", module, func), &sig.params, &sig.ret.display().to_string(), active_param,
            ));
        }
    }

    // user-defined fn from cached analysis
    if let Some(doc) = doc {
        let sym = crate::intern::sym(func_name);
        if let Some(sig) = doc.checker.env.functions.get(&sym) {
            return Some(make_sig_help(
                &format!("fn {}", func_name), &sig.params, &sig.ret.display().to_string(), active_param,
            ));
        }
    }
    None
}

fn make_sig_help(prefix: &str, params: &[(crate::intern::Sym, crate::types::Ty)], ret: &str, active: u32) -> SignatureHelp {
    let param_infos: Vec<ParameterInformation> = params.iter().map(|(n, t)| {
        ParameterInformation { label: ParameterLabel::Simple(format!("{}: {}", n, t.display())), documentation: None }
    }).collect();
    let params_str = params.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect::<Vec<_>>().join(", ");
    SignatureHelp {
        signatures: vec![SignatureInformation {
            label: format!("{}({}) -> {}", prefix, params_str, ret),
            documentation: None,
            parameters: Some(param_infos),
            active_parameter: Some(active),
        }],
        active_signature: Some(0),
        active_parameter: Some(active),
    }
}

// ══════════════════════════════════════════════════════════════
// Rename
// ══════════════════════════════════════════════════════════════

fn compute_rename(source: &str, pos: Position, uri: &Uri, new_name: &str) -> Option<WorkspaceEdit> {
    let lines: Vec<&str> = source.lines().collect();
    let line = lines.get(pos.line as usize)?;
    let col = pos.character as usize;
    if col >= line.len() { return None; }
    let start = line[..col].rfind(|c: char| !c.is_alphanumeric() && c != '_').map(|i| i + 1).unwrap_or(0);
    let end = col + line[col..].find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(line.len() - col);
    let old_name = &line[start..end];
    if old_name.is_empty() { return None; }

    let mut edits = Vec::new();
    for (line_idx, line_text) in lines.iter().enumerate() {
        let mut search_from = 0;
        while let Some(found) = line_text[search_from..].find(old_name) {
            let abs = search_from + found;
            let before_ok = abs == 0 || { let b = line_text.as_bytes()[abs - 1]; !b.is_ascii_alphanumeric() && b != b'_' };
            let after = abs + old_name.len();
            let after_ok = after >= line_text.len() || { let b = line_text.as_bytes()[after]; !b.is_ascii_alphanumeric() && b != b'_' };
            if before_ok && after_ok {
                edits.push(TextEdit {
                    range: Range { start: Position { line: line_idx as u32, character: abs as u32 }, end: Position { line: line_idx as u32, character: after as u32 } },
                    new_text: new_name.to_string(),
                });
            }
            search_from = after;
        }
    }
    if edits.is_empty() { return None; }
    Some(WorkspaceEdit { changes: Some(HashMap::from([(uri.clone(), edits)])), ..Default::default() })
}

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
