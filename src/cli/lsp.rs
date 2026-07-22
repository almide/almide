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
        let is_stdlib = file_path.map_or(false, |fp| fp.contains("/stdlib/"));
        let mut diags = parse_errors;
        for d in &check_diags {
            // Suppress E015 (reimpl-lint) for stdlib source files
            if is_stdlib && d.code.as_deref() == Some("E015") { continue; }
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
    TypeDecl { name: String, display: String },
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

    if let Some(loc) = find_node_keyword(word) { return Some(loc); }
    if let Some(loc) = find_node_builtin_type(word) { return Some(loc); }
    if let Some(loc) = find_node_stdlib_call(line_text, word, start, end) { return Some(loc); }

    // 4. AST-based lookup — walk declarations
    let sym = crate::intern::sym(word);

    find_node_variant_ctor(doc, word)
        .or_else(|| find_node_type_decl(doc, word))
        .or_else(|| find_node_fn_decl(doc, sym, word))
        .or_else(|| find_node_top_let(doc, sym, word))
        .or_else(|| find_node_param(doc, word, line))
        .or_else(|| find_node_expr_ident(doc, word))
}

// 1. Keywords
fn find_node_keyword(word: &str) -> Option<Located> {
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
    kw.map(|info| Located::Keyword { info })
}

// 1b. Primitive / built-in types
fn find_node_builtin_type(word: &str) -> Option<Located> {
    let builtin = match word {
        "Int" => Some("64-bit signed integer"),
        "Float" => Some("64-bit floating point (IEEE 754)"),
        "String" => Some("UTF-8 string (immutable, reference-counted)"),
        "Bool" => Some("Boolean (`true` or `false`)"),
        "Unit" => Some("Unit type — no meaningful value (like void)"),
        "Bytes" => Some("Byte array (`List[Int]` of 0–255 values)"),
        "List" => Some("Ordered collection: `List[T]`"),
        "Map" => Some("Key-value map: `Map[K, V]`"),
        "Set" => Some("Unique value set: `Set[T]`"),
        "Option" => Some("Optional value: `Option[T]` = `Some(T)` | `None`"),
        "Result" => Some("Success or failure: `Result[T, E]` = `Ok(T)` | `Err(E)`"),
        _ => None,
    };
    builtin.map(|info| Located::Keyword { info })
}

// 2. module.func — cursor on module name; 3. module.func — cursor on func name
fn find_node_stdlib_call(line_text: &str, word: &str, start: usize, end: usize) -> Option<Located> {
    if end < line_text.len() && line_text.as_bytes()[end] == b'.' {
        let func_start = end + 1;
        let func_end = func_start + line_text[func_start..].find(|c: char| !c.is_alphanumeric() && c != '_' && c != '?').unwrap_or(line_text.len() - func_start);
        let func = &line_text[func_start..func_end];
        if let Some(sig) = crate::stdlib::lookup_sig(word, func) {
            let params = sig.params.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect::<Vec<_>>().join(", ");
            return Some(Located::StdlibCall { module: word.to_string(), func: func.to_string(), params, ret: sig.ret.display().to_string() });
        }
    }

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
    None
}

// 4a. Variant constructors
fn find_node_variant_ctor(doc: &AnalyzedDoc, word: &str) -> Option<Located> {
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
    None
}

// 4b. Type declarations — show variants/fields
fn find_node_type_decl(doc: &AnalyzedDoc, word: &str) -> Option<Located> {
    for decl in &doc.program.decls {
        if let crate::ast::Decl::Type { name, ty, .. } = decl {
            if name.as_str() == word {
                let detail = match ty {
                    crate::ast::TypeExpr::Variant { cases } => {
                        let case_strs: Vec<String> = cases.iter().map(|c| match c {
                            crate::ast::VariantCase::Unit { name } => format!("| {}", name.as_str()),
                            crate::ast::VariantCase::Tuple { name, fields } => format!("| {}({})", name.as_str(), fields.iter().map(|f| format_type_expr(f)).collect::<Vec<_>>().join(", ")),
                            crate::ast::VariantCase::Record { name, fields } => format!("| {} {{ {} }}", name.as_str(), fields.iter().map(|f| format!("{}: {}", f.name.as_str(), format_type_expr(&f.ty))).collect::<Vec<_>>().join(", ")),
                        }).collect();
                        format!("type {} =\n  {}", word, case_strs.join("\n  "))
                    }
                    crate::ast::TypeExpr::Record { fields } => {
                        let fs: Vec<String> = fields.iter().map(|f| format!("{}: {}", f.name.as_str(), format_type_expr(&f.ty))).collect();
                        format!("type {} = {{ {} }}", word, fs.join(", "))
                    }
                    _ => format!("type {} = {}", word, format_type_expr(ty)),
                };
                return Some(Located::TypeDecl { name: word.to_string(), display: detail });
            }
        }
    }
    None
}

// 4c. Function declarations
fn find_node_fn_decl(doc: &AnalyzedDoc, sym: crate::intern::Sym, word: &str) -> Option<Located> {
    let sig = doc.checker.env.functions.get(&sym)?;
    let params = sig.params.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect::<Vec<_>>().join(", ");
    Some(Located::FnDecl { name: word.to_string(), params, ret: sig.ret.display().to_string() })
}

// 4c. Top-level lets
fn find_node_top_let(doc: &AnalyzedDoc, sym: crate::intern::Sym, word: &str) -> Option<Located> {
    let ty = doc.checker.env.top_lets.get(&sym)?;
    Some(Located::TopLet { name: word.to_string(), ty: ty.display().to_string() })
}

// 4d. Function parameters — check if cursor is inside a fn body
fn find_node_param(doc: &AnalyzedDoc, word: &str, line: u32) -> Option<Located> {
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
    None
}

// 4e. ExprId-based type lookup — walk expressions to find matching Ident
fn find_node_expr_ident(doc: &AnalyzedDoc, word: &str) -> Option<Located> {
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
            Some(Response::new_ok(req.id.clone(), result))
        }
        "textDocument/completion" => {
            let params: CompletionParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = &params.text_document_position.text_document.uri;
            let pos = params.text_document_position.position;
            let source = documents.get(uri)?;
            let items = compute_completions(source, pos);
            let result = serde_json::to_value(CompletionResponse::Array(items)).unwrap();
            Some(Response::new_ok(req.id.clone(), result))
        }
        "textDocument/documentSymbol" => {
            let params: DocumentSymbolParams = serde_json::from_value(req.params.clone()).ok()?;
            let doc = analyzed.get(&params.text_document.uri)?;
            let symbols = compute_document_symbols(doc);
            let result = serde_json::to_value(DocumentSymbolResponse::Flat(symbols)).unwrap();
            Some(Response::new_ok(req.id.clone(), result))
        }
        "textDocument/formatting" => {
            let params: DocumentFormattingParams = serde_json::from_value(req.params.clone()).ok()?;
            let doc = analyzed.get(&params.text_document.uri)?;
            let edits = compute_formatting(doc);
            let result = serde_json::to_value(edits).unwrap();
            Some(Response::new_ok(req.id.clone(), result))
        }
        "textDocument/definition" => {
            let params: GotoDefinitionParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = &params.text_document_position_params.text_document.uri;
            let pos = params.text_document_position_params.position;
            let doc = analyzed.get(uri)?;
            let loc = compute_definition(doc, pos, uri);
            let result = loc.map(|l| serde_json::to_value(GotoDefinitionResponse::Scalar(l)).unwrap())
                .unwrap_or(serde_json::Value::Null);
            Some(Response::new_ok(req.id.clone(), result))
        }
        "textDocument/signatureHelp" => {
            let params: SignatureHelpParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = &params.text_document_position_params.text_document.uri;
            let pos = params.text_document_position_params.position;
            let source = documents.get(uri)?;
            let doc = analyzed.get(uri);
            let help = compute_signature_help(source, pos, doc);
            let result = help.map(|h| serde_json::to_value(h).unwrap()).unwrap_or(serde_json::Value::Null);
            Some(Response::new_ok(req.id.clone(), result))
        }
        "workspace/symbol" => {
            let params: WorkspaceSymbolParams = serde_json::from_value(req.params.clone()).ok()?;
            let symbols = compute_workspace_symbols(&params.query, workspace_root);
            let result = serde_json::to_value(symbols).unwrap();
            Some(Response::new_ok(req.id.clone(), result))
        }
        "textDocument/rename" => {
            let params: RenameParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = &params.text_document_position.text_document.uri;
            let pos = params.text_document_position.position;
            let source = documents.get(uri)?;
            let edit = compute_rename(source, pos, uri, &params.new_name);
            let result = edit.map(|e| serde_json::to_value(e).unwrap()).unwrap_or(serde_json::Value::Null);
            Some(Response::new_ok(req.id.clone(), result))
        }
        "textDocument/codeAction" => {
            let params: CodeActionParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = &params.text_document.uri;
            let source = documents.get(uri)?;
            let actions = compute_code_actions(source, &params.context.diagnostics, uri);
            let result = serde_json::to_value(actions).unwrap();
            Some(Response::new_ok(req.id.clone(), result))
        }
        _ => None,
    }
}

include!("lsp_p2.rs");
include!("lsp_p3.rs");
