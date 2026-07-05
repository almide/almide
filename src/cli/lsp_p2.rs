
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
        Located::TypeDecl { display, .. } =>
            format!("```almide\n{}\n```", display),
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
    if let Some(module) = module_name {
        if let Some(path) = find_stdlib_path(&module) {
            let target_line = func_name.as_ref()
                .and_then(|f| find_fn_line_in_file(&path, f))
                .unwrap_or(0);
            let file_uri = Uri::from_str(&format!("file://{}", path.display())).ok()?;
            return Some(Location {
                uri: file_uri,
                range: Range {
                    start: Position { line: target_line, character: 0 },
                    end: Position { line: target_line, character: 0 },
                },
            });
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
