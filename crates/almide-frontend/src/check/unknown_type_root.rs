// Continuation of `impl Checker` — the E029 root-cause discipline (#2771).
// Spliced into check/mod.rs via `include!`, same module scope.
//
// An annotation naming an undeclared type (`fn f(e: Entyr) -> Int = e.count`)
// is ONE error. Every diagnostic the unknown type causes downstream — a field
// access with "values outside records have no fields", an undefined method —
// is a consequence, and its hint points away from the fix. A reader of the
// first N diagnostics (an LLM repair loop, an IDE problem list) used to see
// only those consequences, with the E029 last and without a location.
//
// The discipline: a consequence is HELD (`defer_type_cascade`) keyed by the
// unknown name; the post-solve E029 walk drops every held diagnostic whose
// name it reported, and emits the rest (so suppression can never erase a
// site's only error). The E029s themselves go ahead of the pass's body
// diagnostics, point at the annotation, and name the declared type within
// edit distance.

impl Checker {
    /// The undeclared type name `ty` is, when it is one: a `Ty::Named` that no
    /// declaration (or import) registered and that is not a built-in nominal.
    /// Exactly the names `validate_unknown_named_types` reports — a PROTOCOL
    /// used as a type included (#1590 reports it as E029 too).
    pub(crate) fn undeclared_type_name(&self, ty: &Ty) -> Option<Sym> {
        let Ty::Named(s, _) = ty else { return None };
        if s.as_str() == "Value" || self.env.types.contains_key(s) {
            return None;
        }
        if almide_lang::stdlib_info::runtime_backed_type_owner(s.as_str()).is_some() {
            return None;
        }
        Some(*s)
    }

    /// Hold `diag` when `ty` is an undeclared type name (the E029 walk decides
    /// whether it is a consequence); emit it now otherwise.
    pub(crate) fn emit_unless_unknown_type(&mut self, ty: &Ty, mut diag: Diagnostic) {
        let Some(name) = self.undeclared_type_name(ty) else {
            self.emit(diag);
            return;
        };
        if diag.line.is_none()
            && let Some(span) = self.current_span
        {
            diag.file = self.source_file.clone();
            diag.line = Some(span.line);
            diag.col = Some(span.col);
            if span.end_col > span.col {
                diag.end_col = Some(span.end_col);
            }
        }
        self.deferred_cascade_diags.push((name, diag));
    }

    /// Release the held consequences whose unknown name produced no E029
    /// (`reported` holds the names that did).
    fn release_type_cascade(&mut self, reported: &std::collections::HashSet<Sym>) {
        for (name, diag) in std::mem::take(&mut self.deferred_cascade_diags) {
            if !reported.contains(&name) {
                self.diagnostics.push(diag);
            }
        }
    }

    /// The first whole-word occurrence of `word` at or after `line:col`
    /// (1-indexed, char columns), within the next few lines — the annotation
    /// an E029 names, found from the span of the declaration that holds it.
    fn locate_type_name(&self, line: usize, col: usize, word: &str) -> Option<(usize, usize)> {
        const WINDOW: usize = 12;
        let text = self.source_text.as_deref()?;
        let is_ident = |c: char| c.is_alphanumeric() || c == '_';
        for (i, src_line) in text.lines().enumerate().skip(line.checked_sub(1)?).take(WINDOW) {
            let chars: Vec<char> = src_line.chars().collect();
            let wchars: Vec<char> = word.chars().collect();
            let start = if i + 1 == line { col.saturating_sub(1) } else { 0 };
            let mut c = start;
            while c + wchars.len() <= chars.len() {
                let hit = chars[c..c + wchars.len()] == wchars[..]
                    && (c == 0 || !is_ident(chars[c - 1]))
                    && chars.get(c + wchars.len()).is_none_or(|&n| !is_ident(n));
                if hit {
                    return Some((i + 1, c + 1));
                }
                c += 1;
            }
        }
        None
    }

    /// A declared type spelled within edit distance of the unknown `name`.
    fn nearest_declared_type(&self, name: &str) -> Option<String> {
        let qualified = name.contains('.');
        let candidates: Vec<&str> = self
            .env
            .types
            .keys()
            .map(|k| k.as_str())
            .filter(|k| qualified == k.contains('.') && !k.starts_with("__"))
            .collect();
        almide_base::diagnostic::suggest(name, candidates.into_iter())
    }

    /// The plain E029 for an undeclared `name`, located at its annotation
    /// (searched from `span`) and naming the nearest declared type.
    fn unknown_type_diag(&self, name: &str, span: Option<crate::ast::Span>, ctx: String) -> Diagnostic {
        let near = self.nearest_declared_type(name);
        let hint = match &near {
            Some(t) => format!(
                "no `type {name}` is declared (or imported) in this program — did you mean `{t}`? \
                 Otherwise declare `type {name}`"
            ),
            None => format!("no `type {name}` is declared (or imported) in this program — declare it, or check the spelling"),
        };
        let mut diag = err(format!("unknown type '{}'", name), hint, ctx).with_code("E029");
        let Some(sp) = span else { return diag };
        diag.file = self.source_file.clone();
        diag.line = Some(sp.line);
        diag.col = Some(sp.col);
        if let Some((line, col)) = self.locate_type_name(sp.line, sp.col, name) {
            let end_col = col + name.chars().count();
            diag.line = Some(line);
            diag.col = Some(col);
            diag.end_col = Some(end_col);
            // SUGGESTION, not machine-applicable: the rename comes from an
            // edit distance, and the missing declaration is the other reading.
            if let Some(t) = &near {
                diag = diag.with_suggested_fix(line, col, end_col, t.clone());
            }
        }
        diag
    }

    /// E093 (#3403): every application `X[A, ..]` in an annotation whose
    /// argument count differs from the declared parameter count of the type
    /// it names. A generic alias applied to its own count was already
    /// expanded by the resolver; one applied to another count stays `Named`
    /// under its key, and so does a generic record or variant. A bare
    /// spelling (no brackets) is not an application and is never reported.
    fn type_arity_diags(&self, ty: &Ty, span: Option<crate::ast::Span>, ctx: &str, seen: &mut std::collections::HashSet<(Sym, usize)>) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        let mut stack = vec![ty];
        while let Some(t) = stack.pop() {
            if let Ty::Named(s, args) = t
                && !args.is_empty()
                && let Some(params) = crate::canonicalize::resolve::declared_type_params(s.as_str(), &self.env.types)
                && params.len() != args.len()
                && seen.insert((*s, args.len()))
            {
                out.push(self.type_arity_diag(s.as_str(), params, args.len(), span, ctx));
            }
            stack.extend(t.children());
        }
        out
    }

    fn type_arity_diag(&self, name: &str, params: &[Ty], given: usize, span: Option<crate::ast::Span>, ctx: &str) -> Diagnostic {
        let letters: Vec<String> = params.iter().map(|p| p.display()).collect();
        let (msg, hint) = if letters.is_empty() {
            (
                format!("type '{}' takes no type arguments, but is applied to {}", name, given),
                format!("Drop the brackets: write `{}` — only a type declared with parameters (`type {}[T] = ...`) takes arguments", name, name),
            )
        } else {
            let spelled = format!("{}[{}]", name, letters.join(", "));
            (
                format!("type '{}' takes {}, but is applied to {}", spelled, plural_args(letters.len()), given),
                format!("Write exactly one type per parameter: `{}`", spelled),
            )
        };
        let mut diag = err(msg, hint, ctx.to_string()).with_code("E093");
        let Some(sp) = span else { return diag };
        diag.file = self.source_file.clone();
        diag.line = Some(sp.line);
        diag.col = Some(sp.col);
        let bare = name.rsplit('.').next().unwrap_or(name);
        if let Some((line, col)) = self.locate_type_name(sp.line, sp.col, bare) {
            diag.line = Some(line);
            diag.col = Some(col);
            diag.end_col = Some(col + bare.chars().count());
        }
        diag
    }
}

fn plural_args(n: usize) -> String {
    if n == 1 { "1 type argument".to_string() } else { format!("{} type arguments", n) }
}
