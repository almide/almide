use std::collections::HashMap;
use std::str::FromStr;
use crate::err;
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

/// Dependency source dirs cached per almide.toml, so a keystroke never
/// re-runs the fetcher (which shells out to git and writes almide.lock).
type DepCache = HashMap<std::path::PathBuf, Vec<(crate::project::PkgId, std::path::PathBuf)>>;

impl AnalyzedDoc {
    fn analyze(source: &str, file_path: Option<&str>, deps: &[(crate::project::PkgId, std::path::PathBuf)]) -> Self {
        let src_lines: Vec<&str> = source.lines().collect();
        let tokens = crate::lexer::Lexer::tokenize(source);
        let mut parser = crate::parser::Parser::new(tokens);
        let (mut program, parse_errors) = match parser.parse() {
            Ok(p) => {
                let errs: Vec<Diagnostic> = parser.errors.iter().map(|e| diag_from_almide(e, &src_lines)).collect();
                (p, errs)
            }
            Err(_) => {
                let errs = parser.errors.iter().map(|e| diag_from_almide(e, &src_lines)).collect();
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

        // Cross-file import resolution against the caller-provided dep list —
        // analyze itself never fetches.
        let resolved_modules = file_path
            .map(|fp| resolve_imports_cached(fp, &program, deps))
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
            diags.push(diag_from_almide(d, &src_lines));
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
    TypeDecl { display: String },
    TopLet { name: String, ty: String },
    VariantConstructor { name: String, type_name: String, fields: Vec<String> },
    StdlibCall { module: String, func: String, params: String, ret: String },
    UserIdent { name: String, ty: String },
    Param { name: String, ty: String },
}

/// Step 1 of `find_node`: language keyword hover info. Extracted verbatim,
/// then converted from a 22-arm `match` (which alone tripped
/// max-complexity — cyclomatic complexity counts one branch per arm
/// regardless of nesting) to a flat data table + linear scan: same
/// word→description mapping, same `None` fallback, genuinely lower
/// complexity (not just moved) since dispatch is now data, not branches.
fn lookup_keyword_info(word: &str) -> Option<&'static str> {
    const TABLE: &[(&str, &str)] = &[
        ("fn", "Function declaration"),
        ("let", "Immutable binding"),
        ("var", "Mutable binding"),
        ("mut", "Mutable parameter modifier — callers must pass a `var` binding"),
        ("type", "Type declaration"),
        ("match", "Pattern matching expression"),
        ("effect", "Effect function — can perform I/O"),
        ("test", "Test block"),
        ("import", "Module import"),
        ("if", "Conditional expression: `if cond then a else b`"),
        ("then", "Then branch of an if expression"),
        ("else", "Else branch of an if expression"),
        ("for", "For-in loop: `for item in collection { ... }`"),
        ("in", "Iterator binding in for loop"),
        ("true", "`Bool` literal (true)"),
        ("false", "`Bool` literal (false)"),
        ("none", "`Option[T]` — no value"),
        ("some", "`Option[T]` constructor — wraps a value"),
        ("ok", "`Result[T, E]` — success value"),
        ("err", "`Result[T, E]` — error value"),
        ("assert", "Test assertion: `assert(condition)` — fails the test if false"),
        ("assert_eq", "Test assertion: `assert_eq(actual, expected)` — fails if not equal"),
    ];
    TABLE.iter().find(|(k, _)| *k == word).map(|(_, v)| *v)
}

/// Step 1b of `find_node`: primitive/built-in type hover info. Extracted
/// verbatim, then converted to a data table for the same reason as
/// `lookup_keyword_info` above.
fn lookup_builtin_type_info(word: &str) -> Option<&'static str> {
    const TABLE: &[(&str, &str)] = &[
        ("Int", "64-bit signed integer"),
        ("Float", "64-bit floating point (IEEE 754)"),
        ("String", "UTF-8 string (immutable, reference-counted)"),
        ("Bool", "Boolean (`true` or `false`)"),
        ("Unit", "Unit type — no meaningful value (like void)"),
        ("Bytes", "Byte array (`List[Int]` of 0–255 values)"),
        ("List", "Ordered collection: `List[T]`"),
        ("Map", "Key-value map: `Map[K, V]`"),
        ("Set", "Unique value set: `Set[T]`"),
        ("Option", "Optional value: `Option[T]` = `Some(T)` | `None`"),
        ("Result", "Success or failure: `Result[T, E]` = `Ok(T)` | `Err(E)`"),
    ];
    TABLE.iter().find(|(k, _)| *k == word).map(|(_, v)| *v)
}

/// Step 2 of `find_node`: cursor is on the module name of `module.func`.
/// Extracted verbatim — reads only its parameters.
fn find_stdlib_call_on_module(line_text: &str, word: &str, end: usize) -> Option<Located> {
    if end < line_text.len() && line_text.as_bytes()[end] == b'.' {
        let func_start = end + 1;
        let func_end = func_start + line_text[func_start..].find(|c: char| !c.is_alphanumeric() && c != '_' && c != '?').unwrap_or(line_text.len() - func_start);
        let func = &line_text[func_start..func_end];
        if let Some(sig) = crate::stdlib::lookup_sig(word, func) {
            let params = sig.params.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect::<Vec<_>>().join(", ");
            return Some(Located::StdlibCall { module: word.to_string(), func: func.to_string(), params, ret: sig.ret.display().to_string() });
        }
    }
    None
}

/// Step 3 of `find_node`: cursor is on the func name of `module.func`.
/// Extracted verbatim — reads only its parameters.
fn find_stdlib_call_on_func(line_text: &str, word: &str, start: usize) -> Option<Located> {
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

/// Step 4a of `find_node`: variant constructor lookup. Extracted verbatim.
fn find_variant_constructor(doc: &AnalyzedDoc, word: &str) -> Option<Located> {
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

/// Step 4b of `find_node`: type declaration hover (shows variants/fields).
/// Extracted verbatim.
fn find_type_decl(doc: &AnalyzedDoc, word: &str) -> Option<Located> {
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
                return Some(Located::TypeDecl { display: detail });
            }
        }
    }
    None
}

/// Step 4c (function half) of `find_node`. Extracted verbatim.
fn find_fn_decl(doc: &AnalyzedDoc, word: &str, sym: &crate::intern::Sym) -> Option<Located> {
    let sig = doc.checker.env.functions.get(sym)?;
    let params = sig.params.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect::<Vec<_>>().join(", ");
    Some(Located::FnDecl { name: word.to_string(), params, ret: sig.ret.display().to_string() })
}

/// Step 4c (top-level-let half) of `find_node`. Extracted verbatim.
fn find_top_let(doc: &AnalyzedDoc, word: &str, sym: &crate::intern::Sym) -> Option<Located> {
    let ty = doc.checker.env.top_lets.get(sym)?;
    Some(Located::TopLet { name: word.to_string(), ty: ty.display().to_string() })
}

/// Step 4d of `find_node`: function-parameter lookup (cursor inside a fn
/// body, heuristically within ~100 lines of its declaration). Extracted
/// verbatim.
fn find_fn_param(doc: &AnalyzedDoc, word: &str, line: u32) -> Option<Located> {
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

/// Step 4e of `find_node`: ExprId-based type lookup by walking expressions
/// for a matching `Ident`. Extracted verbatim.
fn find_user_ident(doc: &AnalyzedDoc, word: &str) -> Option<Located> {
    for decl in &doc.program.decls {
        if let Some(ty) = find_expr_type_by_name(&doc.program, decl, word, &doc.checker.type_map) {
            return Some(Located::UserIdent { name: word.to_string(), ty: ty.display().to_string() });
        }
    }
    None
}

/// `find_node`'s AST-based declaration lookups (steps 4a-4c): variant
/// constructor, type declaration, function declaration, top-level let.
/// Extracted verbatim — same early-return-on-first-match order.
fn find_node_decl_lookup(doc: &AnalyzedDoc, word: &str, sym: &crate::intern::Sym) -> Option<Located> {
    if let Some(loc) = find_variant_constructor(doc, word) {
        return Some(loc);
    }
    if let Some(loc) = find_type_decl(doc, word) {
        return Some(loc);
    }
    if let Some(loc) = find_fn_decl(doc, word, sym) {
        return Some(loc);
    }
    find_top_let(doc, word, sym)
}

/// `find_node`'s remaining lookups (steps 4d-4e): function parameters, then
/// ExprId-based ident type lookup. Extracted verbatim.
fn find_node_usage_lookup(doc: &AnalyzedDoc, word: &str, line: u32) -> Option<Located> {
    if let Some(loc) = find_fn_param(doc, word, line) {
        return Some(loc);
    }
    find_user_ident(doc, word)
}

fn find_node(doc: &AnalyzedDoc, line: u32, col_utf16: u32) -> Option<Located> {
    let source = &doc.source;
    let lines: Vec<&str> = source.lines().collect();
    let line_text = lines.get(line as usize)?;
    let col = utf16_col_to_byte(line_text, col_utf16);
    let (word, start, end) = word_at(line_text, col)?;

    // 1. Keywords
    if let Some(info) = lookup_keyword_info(word) {
        return Some(Located::Keyword { info });
    }

    // 1b. Primitive / built-in types
    if let Some(info) = lookup_builtin_type_info(word) {
        return Some(Located::Keyword { info });
    }

    // 2. module.func — cursor on module name
    if let Some(loc) = find_stdlib_call_on_module(line_text, word, end) {
        return Some(loc);
    }

    // 3. module.func — cursor on func name
    if let Some(loc) = find_stdlib_call_on_func(line_text, word, start) {
        return Some(loc);
    }

    // 4. AST-based lookup — walk declarations
    let sym = crate::intern::sym(word);
    if let Some(loc) = find_node_decl_lookup(doc, word, &sym) {
        return Some(loc);
    }

    find_node_usage_lookup(doc, word, line)
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

/// `run_lsp`'s server-capabilities declaration.
///
/// `position_encoding` declares UTF-16 explicitly — that is what an LSP
/// client sends by default, and every position this server reads or emits is
/// converted through the utf16/char-column helpers rather than sliced as raw
/// bytes (#927).
///
/// Sync is INCREMENTAL (#1470): the client sends range edits, not the whole
/// document per keystroke — `apply_content_change` splices them in UTF-16
/// coordinates.
///
/// Rename is BACK (#1470): the old textual find/replace was withdrawn for
/// silently corrupting code (matches inside strings, comments, shadowed
/// scopes); the new one is binding-aware (`lsp_references.rs`) and refuses —
/// with the reason — any rename its total-accounting net cannot prove safe.
fn lsp_server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        position_encoding: Some(PositionEncodingKind::UTF16),
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::INCREMENTAL)),
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
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        references_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Left(true)),
        ..Default::default()
    }
}

/// `run_lsp`'s workspace-root derivation from `initialize`'s `root_uri`,
/// falling back to CWD. Extracted verbatim.
/// `root_uri` is deprecated upstream in favor of `workspace_folders`, but VS
/// Code still sends it and the single-root fallback is exactly this field —
/// keep reading it deliberately until multi-root support is a real need.
#[allow(deprecated)]
fn derive_workspace_root(init: &InitializeParams) -> Option<std::path::PathBuf> {
    init.root_uri.as_ref()
        .and_then(|u| u.path().to_string().strip_prefix('/').or(Some(u.path().as_str())).map(|s| std::path::PathBuf::from(s.to_string())))
        .or_else(|| std::env::current_dir().ok())
}

/// `run_lsp`'s notification dispatch (`didOpen`/`didChange`/`didClose`).
///
/// Dep fetching (git shell-out, almide.lock write) is allowed only on
/// didOpen — opening a file is a deliberate user action. didChange runs on
/// every keystroke and serves the dep list cached at open time; a project
/// never opened in this session analyzes against no deps rather than
/// touching the network (#927).
// `Uri` keys trip clippy::mutable_key_type across this server — a false
// positive (Uri hashes by its string form; no key is mutated in place), and
// the same shape the pre-existing documents/analyzed maps already carry.
#[allow(clippy::mutable_key_type)]
fn handle_notification(notif: Notification, connection: &Connection, documents: &mut HashMap<Uri, String>, analyzed: &mut HashMap<Uri, AnalyzedDoc>, dep_cache: &mut DepCache, dirty: &mut std::collections::HashSet<Uri>) {
    match notif.method.as_str() {
        "textDocument/didOpen" => {
            if let Ok(params) = serde_json::from_value::<DidOpenTextDocumentParams>(notif.params) {
                let uri = params.text_document.uri.clone();
                let source = params.text_document.text;
                let file_path = uri_to_path(&uri);
                let deps = project_deps_for(file_path.as_deref(), dep_cache, true);
                let doc = AnalyzedDoc::analyze(&source, file_path.as_deref(), &deps);
                publish_diagnostics(connection, &uri, &doc.lsp_diagnostics);
                documents.insert(uri.clone(), source);
                analyzed.insert(uri, doc);
            }
        }
        "textDocument/didChange" => {
            let trace = std::env::var("ALMIDE_LSP_TRACE").is_ok();
            match serde_json::from_value::<DidChangeTextDocumentParams>(notif.params) {
                Ok(params) => {
                    let uri = params.text_document.uri.clone();
                    // INCREMENTAL sync (#1470): splice each range edit into
                    // the stored text, in arrival order. Analysis does NOT
                    // run here — the doc is marked dirty and the main loop's
                    // 100 ms idle debounce (gleam's strategy) analyzes it,
                    // or the next request flushes it first.
                    let text = documents.entry(uri.clone()).or_default();
                    for change in params.content_changes {
                        apply_content_change(text, change);
                    }
                    if trace {
                        eprintln!("[lsp-trace] didChange applied, {} bytes now — analysis deferred to idle", text.len());
                    }
                    dirty.insert(uri);
                }
                // A notification with unparseable params is dropped by design
                // (no reply channel exists for notifications) — but never
                // SILENTLY: an undeserializable didChange means the analyzed
                // map silently stops updating (#1008's hang started life as
                // an invisible drop like this).
                Err(e) => eprintln!("[lsp] didChange params failed to parse — dropped: {e}"),
            }
        }
        "textDocument/didClose" => {
            if let Ok(params) = serde_json::from_value::<DidCloseTextDocumentParams>(notif.params) {
                documents.remove(&params.text_document.uri);
                analyzed.remove(&params.text_document.uri);
                dirty.remove(&params.text_document.uri);
            }
        }
        _ => {}
    }
}

pub fn run_lsp() {
    let (connection, io_threads) = Connection::stdio();

    let server_capabilities = serde_json::to_value(lsp_server_capabilities()).unwrap();

    let init_params = match connection.initialize(server_capabilities) {
        Ok(it) => it,
        Err(e) => { err(&format!("LSP init failed: {}", e)); return; }
    };
    let init: InitializeParams = serde_json::from_value(init_params).unwrap();
    let workspace_root = derive_workspace_root(&init);

    let mut documents: HashMap<Uri, String> = HashMap::new();
    let mut analyzed: HashMap<Uri, AnalyzedDoc> = HashMap::new();
    let mut dep_cache: DepCache = HashMap::new();
    // Docs edited since their last analysis (#1470). The gleam strategy:
    // a 100 ms idle debounce batches keystrokes, and any REQUEST flushes
    // first (compile-before-answering), so answers are never stale.
    let mut dirty: std::collections::HashSet<Uri> = std::collections::HashSet::new();

    loop {
        let msg = if dirty.is_empty() {
            match connection.receiver.recv() {
                Ok(m) => m,
                Err(_) => break,
            }
        } else {
            match connection.receiver.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(m) => m,
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    flush_dirty(&connection, &documents, &mut analyzed, &mut dep_cache, &mut dirty);
                    continue;
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            }
        };
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req).unwrap_or(false) { return; }
                if std::env::var("ALMIDE_LSP_TRACE").is_ok() {
                    eprintln!("[lsp-trace] request  {} id={:?}", req.method, req.id);
                }
                flush_dirty(&connection, &documents, &mut analyzed, &mut dep_cache, &mut dirty);
                let resp = handle_request(&req, &documents, &analyzed, &workspace_root);
                // A REQUEST must always get a response (JSON-RPC). A `None`
                // here used to be a SILENT DROP — a client blocking on the
                // reply waited forever, which is how the Windows CI leg burned
                // 6h runs inside lsp_test since #961 (#1008). Anything the
                // handler cannot answer (param-parse failure, no analyzed
                // doc) is an honest empty result, and the drop is LOGGED so
                // it can never be invisible again.
                let r = resp.unwrap_or_else(|| {
                    eprintln!(
                        "[lsp] request {} id={:?} unanswerable (bad params or no analyzed doc) — replying null (#1008)",
                        req.method, req.id
                    );
                    Response::new_ok(req.id.clone(), serde_json::Value::Null)
                });
                connection.sender.send(Message::Response(r)).ok();
            }
            Message::Notification(notif) => {
                if std::env::var("ALMIDE_LSP_TRACE").is_ok() {
                    eprintln!("[lsp-trace] notification {}", notif.method);
                }
                handle_notification(notif, &connection, &mut documents, &mut analyzed, &mut dep_cache, &mut dirty)
            }
            Message::Response(_) => {}
        }
    }
    io_threads.join().ok();
}

/// Re-analyze every dirty document and publish its diagnostics — the single
/// analysis point behind both the idle debounce and compile-before-answering.
#[allow(clippy::mutable_key_type)] // same Uri-key false positive as handle_notification
fn flush_dirty(connection: &Connection, documents: &HashMap<Uri, String>, analyzed: &mut HashMap<Uri, AnalyzedDoc>, dep_cache: &mut DepCache, dirty: &mut std::collections::HashSet<Uri>) {
    for uri in dirty.drain() {
        let Some(source) = documents.get(&uri) else { continue };
        let file_path = uri_to_path(&uri);
        let deps = project_deps_for(file_path.as_deref(), dep_cache, false);
        let doc = AnalyzedDoc::analyze(source, file_path.as_deref(), &deps);
        publish_diagnostics(connection, &uri, &doc.lsp_diagnostics);
        analyzed.insert(uri, doc);
    }
}

/// Splice one LSP content change into `text`. `range` is UTF-16 (the
/// encoding this server declares); a change with no range replaces the
/// whole document (the FULL-sync form clients may still send).
fn apply_content_change(text: &mut String, change: TextDocumentContentChangeEvent) {
    let Some(range) = change.range else {
        *text = change.text;
        return;
    };
    let start = lsp_pos_to_byte_offset(text, range.start);
    let end = lsp_pos_to_byte_offset(text, range.end);
    if start <= end && end <= text.len() {
        text.replace_range(start..end, &change.text);
    } else {
        // A malformed range must never corrupt the buffer. Loudly keep the
        // old text; the next full analysis will surface any drift.
        eprintln!("[lsp] didChange range {:?} out of bounds ({} bytes) — edit dropped", range, text.len());
    }
}

/// UTF-16 LSP `Position` → byte offset into `text`. Past-the-end positions
/// clamp (LSP expresses newline inclusion as `(line+1, 0)`, which lands on
/// the next iteration's line start).
fn lsp_pos_to_byte_offset(text: &str, pos: Position) -> usize {
    let mut off = 0usize;
    for (i, seg) in text.split_inclusive('\n').enumerate() {
        if i as u32 == pos.line {
            let content = seg.strip_suffix('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).unwrap_or(seg);
            let mut units = 0u32;
            for (bidx, ch) in content.char_indices() {
                if units >= pos.character {
                    return off + bidx;
                }
                units += ch.len_utf16() as u32;
            }
            return off + content.len();
        }
        off += seg.len();
    }
    text.len()
}

fn handle_request(req: &Request, documents: &HashMap<Uri, String>, analyzed: &HashMap<Uri, AnalyzedDoc>, workspace_root: &Option<std::path::PathBuf>) -> Option<Response> {
    match req.method.as_str() {
        "textDocument/hover" => {
            let params: HoverParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = &params.text_document_position_params.text_document.uri;
            let pos = params.text_document_position_params.position;
            let doc = analyzed.get(uri)?;
            let hover = compute_hover(doc, pos);
            let result = hover.map(|h| serde_json::to_value(h).unwrap_or(serde_json::Value::Null)).unwrap_or(serde_json::Value::Null);
            Some(Response::new_ok(req.id.clone(), result))
        }
        "textDocument/completion" => {
            let params: CompletionParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = &params.text_document_position.text_document.uri;
            let pos = params.text_document_position.position;
            let source = documents.get(uri)?;
            let items = compute_completions(source, pos);
            let result = serde_json::to_value(CompletionResponse::Array(items)).ok()?;
            Some(Response::new_ok(req.id.clone(), result))
        }
        "textDocument/documentSymbol" => {
            let params: DocumentSymbolParams = serde_json::from_value(req.params.clone()).ok()?;
            let doc = analyzed.get(&params.text_document.uri)?;
            let symbols = compute_document_symbols(doc, &params.text_document.uri);
            let result = serde_json::to_value(DocumentSymbolResponse::Flat(symbols)).ok()?;
            Some(Response::new_ok(req.id.clone(), result))
        }
        "textDocument/formatting" => {
            let params: DocumentFormattingParams = serde_json::from_value(req.params.clone()).ok()?;
            let doc = analyzed.get(&params.text_document.uri)?;
            let edits = compute_formatting(doc);
            let result = serde_json::to_value(edits).ok()?;
            Some(Response::new_ok(req.id.clone(), result))
        }
        "textDocument/definition" => {
            let params: GotoDefinitionParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = &params.text_document_position_params.text_document.uri;
            let pos = params.text_document_position_params.position;
            let doc = analyzed.get(uri)?;
            let loc = compute_definition(doc, pos, uri);
            let result = loc.map(|l| serde_json::to_value(GotoDefinitionResponse::Scalar(l)).unwrap_or(serde_json::Value::Null))
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
            let result = help.map(|h| serde_json::to_value(h).unwrap_or(serde_json::Value::Null)).unwrap_or(serde_json::Value::Null);
            Some(Response::new_ok(req.id.clone(), result))
        }
        "workspace/symbol" => {
            let params: WorkspaceSymbolParams = serde_json::from_value(req.params.clone()).ok()?;
            let symbols = compute_workspace_symbols(&params.query, workspace_root);
            let result = serde_json::to_value(symbols).ok()?;
            Some(Response::new_ok(req.id.clone(), result))
        }
        "textDocument/references" => {
            let params: ReferenceParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = params.text_document_position.text_document.uri.clone();
            let pos = params.text_document_position.position;
            let doc = analyzed.get(&uri)?;
            let locs = compute_references(doc, pos, &uri, params.context.include_declaration);
            let result = serde_json::to_value(locs).ok()?;
            Some(Response::new_ok(req.id.clone(), result))
        }
        "textDocument/rename" => {
            let params: RenameParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = params.text_document_position.text_document.uri.clone();
            let pos = params.text_document_position.position;
            let doc = analyzed.get(&uri)?;
            match compute_rename(doc, pos, &params.new_name, &uri) {
                Ok(edit) => {
                    let result = serde_json::to_value(edit).ok()?;
                    Some(Response::new_ok(req.id.clone(), result))
                }
                // A refusal is a VISIBLE error (editors surface the message),
                // never a silent null — the guardrail must read as one.
                Err(why) => Some(Response::new_err(
                    req.id.clone(),
                    lsp_server::ErrorCode::RequestFailed as i32,
                    why,
                )),
            }
        }
        "textDocument/codeAction" => {
            let params: CodeActionParams = serde_json::from_value(req.params.clone()).ok()?;
            let uri = &params.text_document.uri;
            let source = documents.get(uri)?;
            let actions = compute_code_actions(source, &params.context.diagnostics, uri);
            let result = serde_json::to_value(actions).ok()?;
            Some(Response::new_ok(req.id.clone(), result))
        }
        _ => None,
    }
}

include!("lsp_hover_definition.rs");
include!("lsp_references.rs");
include!("lsp_code_actions.rs");
