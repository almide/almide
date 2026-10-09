// E025, generic-call edition (#3495). `include!`d by post_solve_validation.rs;
// it shares that module's scope and imports.
//
// A call to a generic fn opens one fresh `?` var per type parameter. The
// arguments, the expected type and explicit type args (`tag[Int](..)`) are
// what pin them. A parameter that NONE of them reaches — `fn tag[T](s: String)
// -> Int`, called as `tag("abc")` — survives the solve as a bare `?` var, and
// nothing downstream can choose it: native emitted a `tag::<_>` rustc could not
// infer, wasm walled E082, and the cross-module call panicked in ResolveCalls.
// `check` passed all three. When the parameter also reaches the call's RESULT
// (`fn dekode[T](s: String) -> Option[T]`), the call-result E025 already names
// the site; this check covers the parameter nothing at all mentions, and stays
// quiet where that one has spoken.

/// One generic call, pending the post-solve "is every type parameter
/// determined" check.
#[derive(Debug, Clone)]
pub(crate) struct TypeParamSite {
    /// The callee as written at the call (`tag`, `util.tag`).
    pub callee: String,
    /// The callee's type parameters in declaration order, each with the type
    /// it was instantiated to at this call (inference vars intact).
    pub generics: Vec<(Sym, Ty)>,
    /// The argument types at the call (inference vars intact).
    pub args: Vec<Ty>,
    pub span: Option<ast::Span>,
}

impl Checker {
    /// Record a generic call's instantiation for [`Self::validate_type_param_inference`].
    pub(crate) fn defer_type_param_check(
        &mut self, name: &str, generics: &[Sym], bindings: &HashMap<Sym, Ty>, args: &[Ty],
    ) {
        if generics.is_empty() {
            return;
        }
        let generics = generics.iter()
            .map(|g| (*g, bindings.get(g).cloned().unwrap_or(Ty::Unknown)))
            .collect();
        // The whole `callee(args)` expression — the span the call-result E025
        // reports — not `current_span`, which by now names the last argument.
        let span = self.call_span_hint.or(self.current_span);
        self.deferred_type_param_checks.push(TypeParamSite {
            callee: name.to_string(), generics, args: args.to_vec(), span,
        });
    }

    /// Post-solve: a generic call whose type parameter resolved to nothing but
    /// an unbound `?` var had no source for it anywhere in the program — the
    /// Rust E0282 "cannot infer type of the type parameter" case. A rigid `T`
    /// (the caller's own parameter, no `?`) is concrete in its scope and an
    /// `Unknown` is error recovery; neither fires. A partially-pinned binding
    /// (`List[?a]`) has its hole reported at the site that opened it (E018 /
    /// E025), so only a WHOLLY unbound parameter belongs to this call.
    fn validate_type_param_inference(&mut self) {
        let checks = std::mem::take(&mut self.deferred_type_param_checks);
        let mut reported: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
        for site in checks {
            let resolved: Vec<(Sym, Ty)> = site.generics.iter()
                .map(|(g, t)| (*g, resolve_ty(t, &self.uf)))
                .collect();
            let args: Vec<Ty> = site.args.iter().map(|t| resolve_ty(t, &self.uf)).collect();
            let stuck: Vec<Sym> = resolved.iter()
                .filter(|(_, t)| Self::undetermined_by_args(t, &args))
                .map(|(g, _)| *g)
                .collect();
            if stuck.is_empty() || self.error_reported_within(site.span) {
                continue;
            }
            let key = site.span.map(|s| (s.line, s.col)).unwrap_or((0, 0));
            if !reported.insert(key) {
                continue;
            }
            self.emit_uninferable_type_param(&site, &resolved, &stuck);
        }
    }

    /// `t` is an unbound `?` var no argument's type mentions. One an argument
    /// DOES mention (`list.len(mk())`: `A` is `mk`'s own unpinned element)
    /// is undetermined because that argument is, and is reported there; an
    /// `Unknown` argument is error recovery that may have held it.
    fn undetermined_by_args(t: &Ty, args: &[Ty]) -> bool {
        let Ty::TypeVar(n) = t else { return false };
        if !n.as_str().starts_with('?') {
            return false;
        }
        let holds = |u: &Ty| u == t || matches!(u, Ty::Unknown);
        !args.iter().any(|a| holds(a) || a.any_child_recursive(&holds))
    }

    /// An error already sits inside this call's span: the call-result E025
    /// (the parameter reaches the result's type), or the error whose recovery
    /// left the parameter unbound (an undefined arg fn, an arity mismatch).
    /// One error per site, and no cascade.
    fn error_reported_within(&self, span: Option<ast::Span>) -> bool {
        let Some(s) = span else { return false };
        let end = s.end_col.max(s.col + 1);
        self.diagnostics.iter().any(|d| {
            matches!(d.level, almide_base::diagnostic::Level::Error)
                && d.line == Some(s.line)
                && d.col.is_some_and(|c| c >= s.col && c < end)
        })
    }

    /// The call's own source with `[type_args]` inserted after the callee, when
    /// the span covers a single-line `callee(...)` (not a pipe or a UFCS
    /// receiver, whose text does not start with the name).
    fn call_with_type_args(&self, site: &TypeParamSite, type_args: &str) -> Option<(ast::Span, String)> {
        let s = site.span.filter(|s| s.end_col > s.col)?;
        let text = self.source_slice(s)?;
        let rest = text.strip_prefix(site.callee.as_str())?;
        rest.starts_with('(').then(|| (s, format!("{}[{}]{}", site.callee, type_args, rest)))
    }

    fn emit_uninferable_type_param(&mut self, site: &TypeParamSite, resolved: &[(Sym, Ty)], stuck: &[Sym]) {
        let names = stuck.iter().map(|g| format!("`{}`", g.as_str())).collect::<Vec<_>>().join(", ");
        let noun = if stuck.len() == 1 { "the type parameter" } else { "the type parameters" };
        // Every parameter is spelled in the example, in declaration order: the
        // determined ones as what this call pinned them to, the stuck ones as an
        // example type (`fill_example_ty`'s `Int`, marked `e.g.`).
        let example_args = resolved.iter()
            .map(|(_, t)| fill_example_ty(t).display())
            .collect::<Vec<_>>()
            .join(", ");
        let example = format!("{}[{}](...)", site.callee, example_args);
        let hint = format!(
            "Nothing at this call determines {names}: no argument's type mentions it and \
             the call's result does not pin it. Name it at the call, e.g. `{example}`, or \
             drop it from `{callee}`'s declaration if the signature never uses it. An \
             undetermined type parameter is never silently defaulted (Almide follows \
             Rust/Swift; cf. Rust E0282).",
            callee = site.callee,
        );
        let mut diag = err(
            format!("cannot infer type of {} {} declared on the function `{}`", noun, names, site.callee),
            hint,
            format!("call to {}()", site.callee),
        ).with_code("E025");
        // Span-exact when the call is spelled `callee(...)` on one line: the
        // fix inserts the type arguments after the name. Suggested, never
        // machine-applied — the example type is a guess the author confirms.
        diag = match self.call_with_type_args(site, &example_args) {
            Some((s, fixed)) => diag.with_suggested_fix(s.line, s.col, s.end_col, fixed),
            None => diag.with_try(example),
        };
        if let Some(s) = site.span {
            diag.file = self.source_file.clone();
            diag.line = Some(s.line);
            diag.col = Some(s.col);
            if s.end_col > s.col { diag.end_col = Some(s.end_col); }
        }
        self.diagnostics.push(diag);
    }
}
