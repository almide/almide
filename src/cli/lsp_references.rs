// ══════════════════════════════════════════════════════════════
// References & rename (#1470) — scope-aware occurrence resolution
// ══════════════════════════════════════════════════════════════
//
// A binding-aware index over ONE document's value namespace: every `Ident`
// expression, assignment target, and binder is resolved through a lexical
// scope stack (shadowing-correct), keyed to the definition that governs it.
// `textDocument/references` reads the index; `textDocument/rename` edits it —
// and rename is guarded by a TOTAL-ACCOUNTING net: the resolver's occurrence
// count for a name must equal the lexer's `Ident`-token count for that name
// over the whole document, or the rename is REFUSED with the reason. A
// traversal gap can therefore never silently miss an edit (the failure that
// got the old textual rename withdrawn); it degrades to an honest refusal.
//
// Deliberate v1 scope: the VALUE namespace (params, let/var/guard-let
// binders, lambda/for/match binders, top-level fns and lets). Types, record
// fields, constructors, and module names refuse rename and answer references
// with an empty set. Occurrences inside interpolation holes are indexed (the
// scope walk descends into `${…}`) but their spans are hole-local, so a def
// with an in-hole occurrence refuses rename — caught automatically by the
// accounting net, since the lexer sees one InterpolatedString token there.

/// One resolved occurrence of a definition (1-based line, 1-based char cols —
/// `Span`'s coordinate system).
struct RefOcc {
    def: usize,
    line: usize,
    col: usize,
    end_col: usize,
}

/// A definition site. `precise` means the NAME token's own span is known
/// (directly from an `Ident` expr, or recovered by lexing the declaring
/// line and finding exactly one candidate token); rename requires it.
struct RefDef {
    name: crate::intern::Sym,
    line: usize,
    col: usize,
    precise: bool,
    /// Kind label for refusal messages ("parameter", "function", …).
    kind: &'static str,
}

#[derive(Default)]
struct OccIndex {
    defs: Vec<RefDef>,
    occs: Vec<RefOcc>,
    /// Value-namespace `Ident`s that resolved to NO local definition
    /// (module names in `list.map`, undefined names) — counted per name so
    /// the accounting net can balance the lexer's view.
    unresolved: std::collections::HashMap<crate::intern::Sym, usize>,
    /// Occurrences whose span is hole-local (inside `${…}`) — counted per
    /// DEF so rename can refuse; they are excluded from `occs`' edit set.
    in_hole: std::collections::HashMap<usize, usize>,
}

type Scope = std::collections::HashMap<crate::intern::Sym, usize>;

struct OccWalker<'a> {
    source_lines: Vec<&'a str>,
    idx: OccIndex,
    scopes: Vec<Scope>,
    in_hole_depth: u32,
}

impl<'a> OccWalker<'a> {
    fn resolve(&self, name: crate::intern::Sym) -> Option<usize> {
        self.scopes.iter().rev().find_map(|s| s.get(&name)).copied()
    }

    /// Record a use-site occurrence for `name` at `span`.
    fn use_at(&mut self, name: crate::intern::Sym, span: Option<crate::ast::Span>) {
        match self.resolve(name) {
            Some(def) => {
                if self.in_hole_depth > 0 {
                    *self.idx.in_hole.entry(def).or_insert(0) += 1;
                    return;
                }
                let Some(s) = span else {
                    // A synthesized node with no span cannot be edited —
                    // poison the def so rename refuses.
                    self.idx.defs[def].precise = false;
                    return;
                };
                self.idx.occs.push(RefOcc { def, line: s.line, col: s.col, end_col: s.end_col });
            }
            _ => {
                if self.in_hole_depth == 0 {
                    *self.idx.unresolved.entry(name).or_insert(0) += 1;
                }
            }
        }
    }

    /// Define `name` in the innermost scope. The name-token position is
    /// recovered by LEXING the declaring line: comments and strings cannot
    /// match, and more than one candidate token on the line means the
    /// position is ambiguous → imprecise (rename refuses, references still
    /// serve the use sites).
    fn define(&mut self, name: crate::intern::Sym, decl_span: Option<crate::ast::Span>, kind: &'static str) {
        let def_id = self.idx.defs.len();
        let (line, col, end_col, precise) = match decl_span {
            Some(s) => match self.recover_name_token(name, s) {
                Some((col, end_col)) => (s.line, col, end_col, self.in_hole_depth == 0),
                _ => (s.line, s.col, s.end_col, false),
            },
            _ => (0, 0, 0, false),
        };
        self.idx.defs.push(RefDef { name, line, col, precise, kind });
        if precise {
            self.idx.occs.push(RefOcc { def: def_id, line, col, end_col });
        } else if self.in_hole_depth > 0 {
            *self.idx.in_hole.entry(def_id).or_insert(0) += 1;
        }
        self.scopes.last_mut().expect("scope stack never empty").insert(name, def_id);
    }

    /// Lex the 1-based `span.line` and find the single `Ident` token whose
    /// text is `name`. Returns 1-based char (col, end_col), or None when the
    /// line is missing or the candidate is not unique.
    fn recover_name_token(&self, name: crate::intern::Sym, span: crate::ast::Span) -> Option<(usize, usize)> {
        let line_text = self.source_lines.get(span.line.saturating_sub(1))?;
        let tokens = crate::lexer::Lexer::tokenize(line_text);
        let mut hit: Option<(usize, usize)> = None;
        for t in &tokens {
            if t.token_type == crate::lexer::TokenType::Ident && t.value == name.as_str() {
                if hit.is_some() {
                    return Option::None; // ambiguous — refuse precision
                }
                hit = Some((t.col, t.end_col));
            }
        }
        hit
    }

    fn scoped(&mut self, f: impl FnOnce(&mut Self)) {
        self.scopes.push(Scope::new());
        f(self);
        self.scopes.pop();
    }

    fn bind_pattern(&mut self, p: &crate::ast::Pattern, span: Option<crate::ast::Span>, kind: &'static str) {
        use crate::ast::Pattern as P;
        match p {
            P::Ident { name } => self.define(*name, span, kind),
            P::Constructor { args, .. } => for a in args { self.bind_pattern(a, span, kind); },
            P::RecordPattern { fields, .. } => for f in fields {
                // `{ x }` binds `x`; `{ x: p }` binds p's binders.
                match &f.pattern {
                    Some(inner) => self.bind_pattern(inner, span, kind),
                    _ => self.define(f.name, span, kind),
                }
            },
            P::Tuple { elements } | P::List { elements } => for e in elements { self.bind_pattern(e, span, kind); },
            P::Some { inner } | P::Ok { inner } | P::Err { inner } => self.bind_pattern(inner, span, kind),
            P::Or { alts } => for a in alts { self.bind_pattern(a, span, kind); },
            P::Wildcard | P::None | P::Literal { .. } => {}
        }
    }

    fn walk_stmts(&mut self, stmts: &[crate::ast::Stmt]) {
        for s in stmts { self.walk_stmt(s); }
    }

    fn walk_stmt(&mut self, stmt: &crate::ast::Stmt) {
        use crate::ast::Stmt as S;
        match stmt {
            S::Let { name, value, span, .. } | S::Var { name, value, span, .. } => {
                self.walk_expr(value); // RHS resolves against the OUTER binding
                self.define(*name, *span, "binding");
            }
            S::LetDestructure { pattern, value, span } => {
                self.walk_expr(value);
                self.bind_pattern(pattern, *span, "binding");
            }
            S::GuardLet { name, scrutinee, else_, span } => {
                self.walk_expr(scrutinee);
                self.walk_expr(else_);
                // binds for the REST of the enclosing block
                self.define(*name, *span, "binding");
            }
            S::Assign { name, value, span } => {
                self.use_at_recovered(*name, *span);
                self.walk_expr(value);
            }
            S::IndexAssign { target, index, value, span } => {
                self.use_at_recovered(*target, *span);
                self.walk_expr(index);
                self.walk_expr(value);
            }
            S::FieldAssign { target, value, span, .. } => {
                self.use_at_recovered(*target, *span);
                self.walk_expr(value);
            }
            S::Guard { cond, else_, .. } => {
                self.walk_expr(cond);
                self.walk_expr(else_);
            }
            S::Expr { expr, .. } => self.walk_expr(expr),
            S::Comment { .. } | S::Error { .. } => {}
        }
    }

    /// An occurrence whose own token span the AST does not carry (assignment
    /// targets): recover it from the statement's line, imprecise-poisoning
    /// the def when the token is ambiguous there.
    fn use_at_recovered(&mut self, name: crate::intern::Sym, span: Option<crate::ast::Span>) {
        let Some(def) = self.resolve(name) else {
            if self.in_hole_depth == 0 {
                *self.idx.unresolved.entry(name).or_insert(0) += 1;
            }
            return;
        };
        if self.in_hole_depth > 0 {
            *self.idx.in_hole.entry(def).or_insert(0) += 1;
            return;
        }
        match span.and_then(|s| self.recover_name_token(name, s).map(|c| (s.line, c))) {
            Some((line, (col, end_col))) => self.idx.occs.push(RefOcc { def, line, col, end_col }),
            _ => self.idx.defs[def].precise = false,
        }
    }

    fn walk_expr(&mut self, expr: &crate::ast::Expr) {
        use crate::ast::ExprKind as E;
        match &expr.kind {
            E::Ident { name } => self.use_at(*name, expr.span),
            E::Lambda { params, body } => self.scoped(|w| {
                for p in params {
                    w.define(p.name, expr.span, "parameter");
                    if let Some(tn) = &p.tuple_names {
                        for n in tn { w.define(*n, expr.span, "parameter"); }
                    }
                }
                w.walk_expr(body);
            }),
            E::Block { stmts, expr: tail } => self.scoped(|w| {
                w.walk_stmts(stmts);
                if let Some(t) = tail { w.walk_expr(t); }
            }),
            E::Match { subject, arms } => {
                self.walk_expr(subject);
                for arm in arms {
                    self.scoped(|w| {
                        w.bind_pattern(&arm.pattern, arm.body.span, "match binder");
                        if let Some(g) = &arm.guard { w.walk_expr(g); }
                        w.walk_expr(&arm.body);
                    });
                }
            }
            E::IfLet { name, scrutinee, then, else_ } => {
                self.walk_expr(scrutinee);
                self.scoped(|w| {
                    w.define(*name, expr.span, "binding");
                    w.walk_expr(then);
                });
                self.walk_expr(else_);
            }
            E::ForIn { var, var_tuple, iterable, body } => {
                self.walk_expr(iterable);
                self.scoped(|w| {
                    w.define(*var, expr.span, "loop binder");
                    if let Some(tn) = var_tuple {
                        for n in tn { w.define(*n, expr.span, "loop binder"); }
                    }
                    w.walk_stmts(body);
                });
            }
            E::While { cond, body } => {
                self.walk_expr(cond);
                self.scoped(|w| w.walk_stmts(body));
            }
            E::InterpolatedString { parts, .. } => {
                for part in parts {
                    if let crate::ast::StringPart::Expr { expr } = part {
                        self.in_hole_depth += 1;
                        self.walk_expr(expr);
                        self.in_hole_depth -= 1;
                    }
                }
            }
            E::Call { callee, args, named_args, .. } => {
                self.walk_expr(callee);
                for a in args { self.walk_expr(a); }
                for (_, a) in named_args { self.walk_expr(a); }
            }
            E::Member { object, .. } | E::OptionalChain { expr: object, .. } | E::TupleIndex { object, .. } => self.walk_expr(object),
            E::IndexAccess { object, index } => { self.walk_expr(object); self.walk_expr(index); }
            E::Pipe { left, right } | E::Compose { left, right } | E::Binary { left, right, .. } => {
                self.walk_expr(left);
                self.walk_expr(right);
            }
            E::If { cond, then, else_ } => {
                self.walk_expr(cond);
                self.walk_expr(then);
                self.walk_expr(else_);
            }
            E::List { elements } | E::Tuple { elements } | E::Fan { exprs: elements } | E::FanSettle { arms: elements } => {
                for e in elements { self.walk_expr(e); }
            }
            E::MapLiteral { entries } => for (k, v) in entries { self.walk_expr(k); self.walk_expr(v); },
            E::Record { fields, .. } => for f in fields { self.walk_expr(&f.value); },
            E::SpreadRecord { base, fields } => {
                self.walk_expr(base);
                for f in fields { self.walk_expr(&f.value); }
            }
            E::FanBounded { budget, body } => { self.walk_expr(budget); self.walk_expr(body); }
            E::FanRace { budget, arms } => {
                if let Some(b) = budget { self.walk_expr(b); }
                for a in arms { self.walk_expr(a); }
            }
            E::FanRaceMap { budget, list, mapper } => {
                if let Some(b) = budget { self.walk_expr(b); }
                self.walk_expr(list);
                self.walk_expr(mapper);
            }
            E::FanTimeout { deadline, body } => { self.walk_expr(deadline); self.walk_expr(body); }
            E::Unary { operand, .. } | E::Paren { expr: operand } | E::Try { expr: operand }
            | E::Unwrap { expr: operand } | E::ToOption { expr: operand }
            | E::Some { expr: operand } | E::Ok { expr: operand } | E::Err { expr: operand } => self.walk_expr(operand),
            E::UnwrapOr { expr: e, fallback } => { self.walk_expr(e); self.walk_expr(fallback); }
            E::TypeAscription { expr: e, .. } => self.walk_expr(e),
            E::Range { start, end, .. } => { self.walk_expr(start); self.walk_expr(end); }
            E::Int { .. } | E::Float { .. } | E::String { .. } | E::Bool { .. }
            | E::TypeName { .. } | E::EmptyMap | E::Hole | E::Todo { .. }
            | E::Break | E::Continue | E::Placeholder | E::Unit | E::None | E::Error => {}
        }
    }
}

/// Build the occurrence index for one analyzed document.
fn build_occ_index(doc: &AnalyzedDoc) -> OccIndex {
    let mut w = OccWalker {
        source_lines: doc.source.lines().collect(),
        idx: OccIndex::default(),
        scopes: vec![Scope::new()],
        in_hole_depth: 0,
    };
    // Pass 1: the whole top-level value namespace is mutually visible.
    for d in &doc.program.decls {
        match d {
            crate::ast::Decl::Fn { name, span, .. } => w.define(*name, *span, "function"),
            crate::ast::Decl::TopLet { name, span, .. } => w.define(*name, *span, "top-level binding"),
            _ => {}
        }
    }
    // Pass 2: bodies.
    for d in &doc.program.decls {
        match d {
            crate::ast::Decl::Fn { params, body, span, .. } => w.scoped(|w| {
                for p in params {
                    w.define(p.name, *span, "parameter");
                    if let Some(dv) = &p.default { w.walk_expr(dv); }
                }
                if let Some(b) = body { w.walk_expr(b); }
            }),
            crate::ast::Decl::TopLet { value, .. } => w.walk_expr(value),
            crate::ast::Decl::Test { body, .. } => w.scoped(|w| w.walk_expr(body)),
            _ => {}
        }
    }
    w.idx
}

/// The definition governing the UTF-16 `pos`, via the occurrence whose span
/// contains it.
fn def_at_position(idx: &OccIndex, doc: &AnalyzedDoc, pos: Position) -> Option<usize> {
    let lines: Vec<&str> = doc.source.lines().collect();
    let line_text = lines.get(pos.line as usize)?;
    let byte = utf16_col_to_byte(line_text, pos.character);
    let char_col = line_text[..byte.min(line_text.len())].chars().count() + 1; // 1-based
    idx.occs.iter()
        .find(|o| o.line == pos.line as usize + 1 && o.col <= char_col && char_col <= o.end_col)
        .map(|o| o.def)
}

fn occ_to_location(o: &RefOcc, doc: &AnalyzedDoc, uri: &Uri) -> Option<Location> {
    let lines: Vec<&str> = doc.source.lines().collect();
    let line_text = lines.get(o.line.saturating_sub(1))?;
    Some(Location {
        uri: uri.clone(),
        range: Range {
            start: Position { line: (o.line - 1) as u32, character: char_col_to_utf16(line_text, o.col) },
            end: Position { line: (o.line - 1) as u32, character: char_col_to_utf16(line_text, o.end_col) },
        },
    })
}

fn compute_references(doc: &AnalyzedDoc, pos: Position, uri: &Uri, include_decl: bool) -> Vec<Location> {
    let idx = build_occ_index(doc);
    let Some(def) = def_at_position(&idx, doc, pos) else { return Vec::new() };
    let d = &idx.defs[def];
    idx.occs.iter()
        .filter(|o| o.def == def)
        .filter(|o| include_decl || !(o.line == d.line && o.col == d.col))
        .filter_map(|o| occ_to_location(o, doc, uri))
        .collect()
}

/// Rename, or the REASON it is refused. Every refusal is a message the
/// editor shows — a silent null reads as "broken", an explained refusal
/// reads as a guardrail.
fn compute_rename(doc: &AnalyzedDoc, pos: Position, new_name: &str, uri: &Uri) -> Result<WorkspaceEdit, String> {
    // The new name must lex as exactly one plain identifier.
    let toks = crate::lexer::Lexer::tokenize(new_name);
    let idents: Vec<_> = toks.iter().filter(|t| t.token_type != crate::lexer::TokenType::EOF && t.token_type != crate::lexer::TokenType::Newline).collect();
    if idents.len() != 1 || idents[0].token_type != crate::lexer::TokenType::Ident || idents[0].value != new_name {
        return Err(format!("'{new_name}' is not a plain identifier"));
    }
    let idx = build_occ_index(doc);
    let def = def_at_position(&idx, doc, pos)
        .ok_or_else(|| "no renameable value binding at this position (types, fields and modules are not in scope for rename yet)".to_string())?;
    let d = &idx.defs[def];
    if !d.precise {
        return Err(format!("cannot rename this {}: its declaration position is ambiguous on that line", d.kind));
    }
    if let Some(n) = idx.in_hole.get(&def) {
        return Err(format!("'{}' occurs {n} time(s) inside string interpolation — rename there is not span-precise yet", d.name));
    }
    // TOTAL ACCOUNTING: every Ident token in the document spelling this name
    // must be accounted for by the resolver (this def, a shadowing def, or a
    // recorded unresolved use). A mismatch means the scope walk missed a
    // site — refuse rather than risk a partial rename.
    let lexer_count = crate::lexer::Lexer::tokenize(&doc.source).iter()
        .filter(|t| t.token_type == crate::lexer::TokenType::Ident && t.value == d.name.as_str())
        .count();
    let resolver_count = idx.occs.iter().filter(|o| idx.defs[o.def].name == d.name).count()
        + idx.unresolved.get(&d.name).copied().unwrap_or(0)
        + idx.defs.iter().enumerate()
            .filter(|(i, dd)| dd.name == d.name && !dd.precise && *i != def)
            .count(); // an imprecise same-named def still owns its (uncounted) decl token
    if lexer_count != resolver_count {
        return Err(format!(
            "rename refused: {lexer_count} occurrence(s) of '{}' in the document but only {resolver_count} resolved — the resolver cannot yet account for every site, and a partial rename corrupts code",
            d.name
        ));
    }
    // Collision guard: the new name must not be bound or used anywhere the
    // renamed symbol appears (conservative — refuses some legal renames,
    // never permits a capturing one).
    let clash = idx.defs.iter().any(|dd| dd.name.as_str() == new_name)
        || idx.unresolved.keys().any(|n| n.as_str() == new_name);
    if clash {
        return Err(format!("'{new_name}' already appears in this document — refusing a rename that could capture or shadow it"));
    }
    let lines: Vec<&str> = doc.source.lines().collect();
    let mut edits: Vec<TextEdit> = idx.occs.iter()
        .filter(|o| o.def == def)
        .filter_map(|o| {
            let line_text = lines.get(o.line.saturating_sub(1))?;
            Some(TextEdit {
                range: Range {
                    start: Position { line: (o.line - 1) as u32, character: char_col_to_utf16(line_text, o.col) },
                    end: Position { line: (o.line - 1) as u32, character: char_col_to_utf16(line_text, o.end_col) },
                },
                new_text: new_name.to_string(),
            })
        })
        .collect();
    edits.sort_by_key(|e| (e.range.start.line, e.range.start.character));
    edits.dedup_by_key(|e| (e.range.start.line, e.range.start.character));
    // clippy::mutable_key_type fires on `Uri` keys throughout this server —
    // a false positive here: lsp_types::Uri hashes by its string form and
    // nothing mutates a key in place (the pre-existing documents/analyzed
    // maps carry the same shape).
    #[allow(clippy::mutable_key_type)]
    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), edits);
    Ok(WorkspaceEdit { changes: Some(changes), ..Default::default() })
}
