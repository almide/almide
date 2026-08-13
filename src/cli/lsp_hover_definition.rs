
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
        Located::TypeDecl { display } =>
            format!("```almide\n{}\n```", display),
        Located::UserIdent { name, ty } =>
            format!("```almide\n{}: {}\n```", name, ty),
        Located::Param { name, ty } =>
            format!("```almide\n{}: {} (parameter)\n```", name, ty),
    };
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value: md }),
        range: None,
    })
}

// ══════════════════════════════════════════════════════════════
// Go to Definition — dispatches on Located word, walks AST for declaration
// ══════════════════════════════════════════════════════════════

/// LSP positions arrive in UTF-16 code units — the encoding this server
/// declares in its capabilities. Convert one to a byte offset into `line`,
/// clamping past-EOL positions to the line end. Slicing with the raw
/// `character` value treated bytes as columns: any non-ASCII text before the
/// cursor produced wrong offsets, and a cursor past a char boundary or past
/// EOL panicked the server (#927). The returned offset is always a char
/// boundary.
fn utf16_col_to_byte(line: &str, col: u32) -> usize {
    let mut units = 0u32;
    for (byte, ch) in line.char_indices() {
        if units >= col {
            return byte;
        }
        units += ch.len_utf16() as u32;
    }
    line.len()
}

/// 1-indexed char column — the lexer's span/diagnostic convention — to
/// 0-based UTF-16 code units, for every position this server emits.
fn char_col_to_utf16(line: &str, char_col: usize) -> u32 {
    line.chars().take(char_col.saturating_sub(1)).map(|c| c.len_utf16() as u32).sum()
}

/// Byte offset into `line` to 0-based UTF-16 code units, for emitted
/// positions derived from byte scans (`str::find` results).
fn byte_col_to_utf16(line: &str, byte: usize) -> u32 {
    line[..byte.min(line.len())].chars().map(|c| c.len_utf16() as u32).sum()
}

/// Extract the identifier word (`[A-Za-z0-9_]+`) touching byte column `col`
/// in `line`, plus its `[start, end)` byte range. `col` must be a char
/// boundary (convert LSP positions through `utf16_col_to_byte` first).
fn word_at(line: &str, col: usize) -> Option<(&str, usize, usize)> {
    if col >= line.len() { return None; }
    let start = line[..col].rfind(|c: char| !c.is_alphanumeric() && c != '_').map(|i| i + 1).unwrap_or(0);
    let end = col + line[col..].find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(line.len() - col);
    let word = &line[start..end];
    if word.is_empty() { return None; }
    Some((word, start, end))
}

/// `compute_definition`'s first phase: search the current file's own
/// declarations (fn/type/top-let name, or a variant constructor name) for
/// a match. Extracted verbatim.
fn find_decl_definition(program: &crate::ast::Program, word: &str, uri: &Uri, lines: &[&str]) -> Option<Location> {
    for decl in &program.decls {
        let (name, span) = match decl {
            crate::ast::Decl::Fn { name, span, .. } => (name.as_str(), span),
            crate::ast::Decl::Type { name, span, .. } => (name.as_str(), span),
            crate::ast::Decl::TopLet { name, span, .. } => (name.as_str(), span),
            _ => continue,
        };
        if name == word {
            return span_to_location(span, uri, lines);
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
                    return span_to_location(span, uri, lines);
                }
            }
        }
    }
    None
}

/// `compute_definition`'s second phase: when the word isn't a local
/// declaration, treat it as a stdlib type name, or the module/func half of
/// a `module.func` stdlib call, and jump into the bundled stdlib source.
/// Extracted verbatim.
fn find_stdlib_definition(line: &str, word: &str, start: usize, end: usize) -> Option<Location> {
    // Stdlib module jump: type name → stdlib source, module.func → specific fn line
    let (module_name, func_name) = if let Some(m) = type_to_module(word) {
        (Some(m), None)
    } else if end < line.len() && line.as_bytes()[end] == b'.' {
        // cursor on module name in module.func
        let func_start = end + 1;
        let func_end = func_start + line[func_start..].find(|c: char| !c.is_alphanumeric() && c != '_' && c != '?').unwrap_or(line.len() - func_start);
        (Some(word.to_string()), Some(line[func_start..func_end].to_string()))
    } else if start > 0 && line.as_bytes()[start - 1] == b'.' {
        // cursor on func name in module.func
        let mod_end = start - 1;
        let mod_start = line[..mod_end].rfind(|c: char| !c.is_alphanumeric() && c != '_').map(|i| i + 1).unwrap_or(0);
        (Some(line[mod_start..mod_end].to_string()), Some(word.to_string()))
    } else {
        (None, None)
    };
    let module = module_name?;
    let path = find_stdlib_path(&module)?;
    let target_line = func_name.as_ref()
        .and_then(|f| find_fn_line_in_file(&path, f))
        .unwrap_or(0);
    let file_uri = Uri::from_str(&format!("file://{}", path.display())).ok()?;
    Some(Location {
        uri: file_uri,
        range: Range {
            start: Position { line: target_line, character: 0 },
            end: Position { line: target_line, character: 0 },
        },
    })
}

fn compute_definition(doc: &AnalyzedDoc, pos: Position, uri: &Uri) -> Option<Location> {
    let lines: Vec<&str> = doc.source.lines().collect();
    let line = lines.get(pos.line as usize)?;
    let col = utf16_col_to_byte(line, pos.character);
    let (word, start, end) = word_at(line, col)?;

    if let Some(loc) = find_decl_definition(&doc.program, word, uri, &lines) {
        return Some(loc);
    }

    find_stdlib_definition(line, word, start, end)
}

fn span_to_location(span: &Option<crate::ast::Span>, uri: &Uri, lines: &[&str]) -> Option<Location> {
    let s = span.as_ref()?;
    let line = s.line.saturating_sub(1) as u32;
    let line_text = lines.get(line as usize).copied().unwrap_or("");
    Some(Location {
        uri: uri.clone(),
        range: Range {
            start: Position { line, character: char_col_to_utf16(line_text, s.col) },
            end: Position { line, character: char_col_to_utf16(line_text, s.end_col) },
        },
    })
}

// ══════════════════════════════════════════════════════════════
// Completion — text-based (fast, doesn't need analysis)
// ══════════════════════════════════════════════════════════════

fn compute_completions(source: &str, pos: Position) -> Vec<CompletionItem> {
    let lines: Vec<&str> = source.lines().collect();
    let line = match lines.get(pos.line as usize) { Some(l) => *l, None => return vec![] };
    let col = utf16_col_to_byte(line, pos.character);
    let prefix = &line[..col];

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

fn compute_document_symbols(doc: &AnalyzedDoc, uri: &Uri) -> Vec<SymbolInformation> {
    let lines: Vec<&str> = doc.source.lines().collect();
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
        let line_text = lines.get(line as usize).copied().unwrap_or("");
        let col = span.as_ref().map(|s| char_col_to_utf16(line_text, s.col)).unwrap_or(0);
        #[allow(deprecated)]
        symbols.push(SymbolInformation {
            name, kind,
            location: Location { uri: uri.clone(), range: Range { start: Position { line, character: col }, end: Position { line, character: col } } },
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
    // #1309: the editor path must obey the same safety verifier as the CLI —
    // a corrupting format becomes "no edits", never a silent rewrite.
    if crate::fmt::verify_format(&doc.source, &doc.program, &formatted).is_err() {
        return vec![];
    }
    let line_count = doc.source.lines().count().max(1);
    vec![TextEdit {
        range: Range { start: Position { line: 0, character: 0 }, end: Position { line: line_count as u32, character: 0 } },
        new_text: formatted,
    }]
}

// ══════════════════════════════════════════════════════════════
// Signature Help
// ══════════════════════════════════════════════════════════════

/// Scan `prefix` (the text before the cursor) backward for the nearest
/// unmatched `(`, counting `,` at depth 0 to determine the active
/// parameter index. Extracted verbatim from `compute_signature_help`.
fn find_active_call(prefix: &str) -> Option<(usize, u32)> {
    let mut depth = 0i32;
    let mut active_param = 0u32;
    for (i, ch) in prefix.char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' => { if depth == 0 { return Some((i, active_param)); } depth -= 1; }
            ',' if depth == 0 => active_param += 1,
            _ => {}
        }
    }
    None
}

fn compute_signature_help(source: &str, pos: Position, doc: Option<&AnalyzedDoc>) -> Option<SignatureHelp> {
    let lines: Vec<&str> = source.lines().collect();
    let line = lines.get(pos.line as usize)?;
    let prefix = &line[..utf16_col_to_byte(line, pos.character)];

    let (paren_pos, active_param) = find_active_call(prefix)?;
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

// Rename was removed from the advertised capabilities: the previous
// implementation was an unscoped textual find/replace over the raw buffer —
// no scope resolution, no shadowing, matches inside string literals and
// comments, single-file only — which silently corrupts user code (#927).
// It returns when it can be binding-aware via the Checker.
