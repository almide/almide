// ADR-0032 (row 11e): a response's Content-Type is its body's type. Two
// check-time rules read off stdlib declarations, never off function names:
//
// - E116: a fn declared `@header_write(name, value, owned = "<fn>")` (or
//   `(headers, owned = …)` for a list of pairs) writes header fields; the
//   names `<fn>`'s list holds are owned by the body (`Content-Type`), so a
//   literal one is refused here, and its body refuses a computed one at run
//   time. A generic `[M: HttpMessage]` writer is judged by the per-type
//   requirement its body dispatches to for the argument's type, so
//   `set_header` on a request is not a response write.
// - E117: a fn declared `@literal_media(p)` takes parameter `p` only as a
//   string literal naming a well-formed media type that no `Body` type owns.
//   Ownership is the union of the `@media_owns(patterns…)` declarations on
//   the stdlib's body requirement fns (RFC 6839 patterns: `*` is any type, a
//   subtype `*<suffix>` a longer subtype ending in it). Naming such a fn as
//   a value is E117 too: its argument could then never be read.
//
// Spliced into check/mod.rs; runs once per program after inference.

/// The header-write declaration of a stdlib fn: the parameters holding header
/// fields and the header names the body owns.
struct HeaderWrite {
    fields: Vec<Sym>,
    owned: Vec<String>,
}

const HEADER_WRITE_ATTR: &str = "header_write";
const LITERAL_MEDIA_ATTR: &str = "literal_media";
const MEDIA_OWNS_ATTR: &str = "media_owns";

/// One stdlib fn's declaration, from its bundled module source.
fn stdlib_fn_decl(module: &str, func: &str) -> Option<&'static ast::Decl> {
    if !almide_lang::stdlib_info::is_bundled_module(module) {
        return None;
    }
    let program = almide_lang::parse_cached(almide_lang::stdlib_info::bundled_source(module)?)?;
    program.decls.iter().find(|d| matches!(d, ast::Decl::Fn { name, .. } if name.as_str() == func))
}

fn fn_attr<'a>(decl: &'a ast::Decl, name: &str) -> Option<&'a ast::Attribute> {
    let ast::Decl::Fn { attrs, .. } = decl else { return None };
    attrs.iter().find(|a| a.name.as_str() == name)
}

fn positional_idents(attr: &ast::Attribute) -> Vec<Sym> {
    attr.args.iter()
        .filter(|a| a.name.is_none())
        .filter_map(|a| match &a.value { ast::AttrValue::Ident { name } => Some(*name), _ => None })
        .collect()
}

/// The string literals of the list a stdlib fn of `module` returns.
fn listed_strings(module: &str, list_fn: &str) -> Vec<String> {
    let Some(ast::Decl::Fn { body: Some(body), .. }) = stdlib_fn_decl(module, list_fn) else { return Vec::new() };
    let ast::ExprKind::List { elements } = &body.kind else { return Vec::new() };
    elements.iter()
        .filter_map(|e| match &e.kind { ast::ExprKind::String { value, .. } => Some(value.clone()), _ => None })
        .collect()
}

/// The header-write declaration of `module.func`, if any.
fn header_write_of(module: &str, func: &str) -> Option<HeaderWrite> {
    let attr = fn_attr(stdlib_fn_decl(module, func)?, HEADER_WRITE_ATTR)?;
    let owned_fn = attr.args.iter()
        .filter(|a| a.name.is_some_and(|n| n.as_str() == "owned"))
        .find_map(|a| match &a.value { ast::AttrValue::String { value } => Some(value.clone()), _ => None })?;
    let fields = positional_idents(attr);
    let owned = listed_strings(module, &owned_fn);
    (!fields.is_empty() && !owned.is_empty()).then_some(HeaderWrite { fields, owned })
}

/// Every media-type pattern a stdlib body type owns: the union of the
/// `@media_owns` declarations across the bundled modules, with the fn that
/// declares each (`html.__safe_html_media_type`).
fn owned_media_patterns() -> &'static [(String, String)] {
    static TABLE: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut out = Vec::new();
        for &module in almide_lang::stdlib_info::BUNDLED_MODULES {
            let Some(program) = almide_lang::stdlib_info::bundled_source(module).and_then(almide_lang::parse_cached) else { continue };
            for d in &program.decls {
                let ast::Decl::Fn { name, attrs, .. } = d else { continue };
                for a in attrs.iter().filter(|a| a.name.as_str() == MEDIA_OWNS_ATTR) {
                    for arg in &a.args {
                        if let ast::AttrValue::String { value } = &arg.value {
                            out.push((value.to_ascii_lowercase(), format!("{module}.{name}")));
                        }
                    }
                }
            }
        }
        out
    })
}

/// A media type against one RFC 6839 pattern.
fn media_matches(t: &str, pattern: &str) -> bool {
    let (Some((tt, ts)), Some((pt, ps))) = (t.split_once('/'), pattern.split_once('/')) else { return false };
    let subtype = match ps.strip_prefix('*') {
        Some(suffix) => ts.chars().count() > suffix.chars().count() && ts.ends_with(suffix),
        None => ps == ts,
    };
    (pt == "*" || pt == tt) && subtype
}

/// An RFC 9110 `token` character.
fn is_tchar(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c)
}

/// Why a literal media type cannot be `@literal_media`'s argument: malformed
/// (not `type/subtype[; params]`), or owned by a stdlib body type.
fn media_refusal(value: &str) -> Option<String> {
    let essence = value.split(';').next().unwrap_or("").trim();
    let Some((ty, sub)) = essence.split_once('/') else {
        return Some(format!("`{value}` is not a media type: write `type/subtype`, as `text/csv`"));
    };
    if ty.is_empty() || sub.is_empty() || !ty.chars().all(is_tchar) || !sub.chars().all(is_tchar) || value.contains(',') {
        return Some(format!("`{value}` is not a media type: write one `type/subtype`, as `text/csv`, with `; name=value` parameters after it"));
    }
    let t = essence.to_ascii_lowercase();
    owned_media_patterns().iter().find(|(p, _)| media_matches(&t, p)).map(|(_, owner)| {
        let module = owner.split('.').next().unwrap_or(owner);
        format!("`{essence}` runs script in a browser and is owned by `{module}`: build the body with the `{module}` builders, whose type serves it")
    })
}

impl Checker {
    pub(crate) fn check_body_rules(&mut self, program: &ast::Program) {
        // A call that is a pipe's right side is judged with the pipe, where
        // its first argument is known; a callee is judged with its call.
        let mut piped: std::collections::HashSet<ast::ExprId> = std::collections::HashSet::new();
        let mut callees: std::collections::HashSet<ast::ExprId> = std::collections::HashSet::new();
        let mut collect = |e: &ast::Expr| match &e.kind {
            ast::ExprKind::Pipe { right, .. } => {
                piped.insert(right.id);
            }
            ast::ExprKind::Call { callee, .. } => {
                callees.insert(callee.id);
            }
            _ => {}
        };
        for_each_body(program, &mut collect);
        let mut found: Vec<Diagnostic> = Vec::new();
        let mut on = |e: &ast::Expr| {
            // A `@literal_media` fn named as a value.
            if matches!(e.kind, ast::ExprKind::Member { .. } | ast::ExprKind::Ident { .. }) && !callees.contains(&e.id) {
                if let Some((module, func)) = self.stdlib_callee(e)
                    && let Some(decl) = stdlib_fn_decl(&module, &func)
                    && fn_attr(decl, LITERAL_MEDIA_ATTR).is_some()
                {
                    found.push(self.e117(&format!("{module}.{func}"), "it is named as a value here, so its media type is never a literal the checker can read; call it with the media type written in place", e.span));
                }
                return;
            }
            let (call, first) = match &e.kind {
                ast::ExprKind::Pipe { left, right } => (&**right, Some(&**left)),
                ast::ExprKind::Call { .. } if !piped.contains(&e.id) => (e, None),
                _ => return,
            };
            let ast::ExprKind::Call { callee, args, named_args, .. } = &call.kind else { return };
            let Some((module, func)) = self.stdlib_callee(callee) else { return };
            let Some(decl @ ast::Decl::Fn { params, .. }) = stdlib_fn_decl(&module, &func) else { return };
            let bound: Vec<Option<&ast::Expr>> = params.iter().enumerate().map(|(i, p)| {
                if let Some((_, a)) = named_args.iter().find(|(n, _)| *n == p.name) {
                    return Some(a);
                }
                match first {
                    Some(l) if i == 0 => Some(l),
                    Some(_) => args.get(i - 1),
                    None => args.get(i),
                }
            }).collect();
            let arg_named = |n: Sym| params.iter().position(|p| p.name == n).and_then(|i| bound[i]);
            let callee_name = format!("{module}.{func}");
            // E117: the media type of a `@literal_media` fn.
            if let Some(attr) = fn_attr(decl, LITERAL_MEDIA_ATTR) {
                for p in positional_idents(attr) {
                    match arg_named(p).map(|a| (&a.kind, a.span)) {
                        Some((ast::ExprKind::String { value, .. }, span)) => {
                            if let Some(why) = media_refusal(value) {
                                found.push(self.e117(&callee_name, &why, span.or(call.span)));
                            }
                        }
                        Some((_, span)) => found.push(self.e117(&callee_name, "its media type must be a string literal: the checker reads it to refuse a type only a safe module may serve; pick the type with a `match` over literals", span.or(call.span))),
                        None => {}
                    }
                }
            }
            // E116: an owned header written by name.
            let Some(hw) = header_write_of(&module, &func).or_else(|| self.dispatched_header_write(&module, &func, &bound)) else {
                return;
            };
            let owned = |n: &ast::Expr| match &n.kind {
                ast::ExprKind::String { value, .. } => hw.owned.iter().find(|o| o.eq_ignore_ascii_case(value)).map(|_| value.clone()),
                _ => None,
            };
            let hit = match hw.fields.as_slice() {
                [name, _value] => arg_named(*name).and_then(owned),
                [pairs] => match arg_named(*pairs).map(|a| &a.kind) {
                    Some(ast::ExprKind::List { elements }) => elements.iter().find_map(|p| match &p.kind {
                        ast::ExprKind::Tuple { elements: kv } if kv.len() == 2 => owned(&kv[0]),
                        _ => None,
                    }),
                    _ => None,
                },
                _ => None,
            };
            if let Some(header) = hit {
                let fix = self.owned_header_fix(callee, call, first, &bound, params, &hw);
                found.push(self.e116(&callee_name, &header, call.span.or(e.span), fix));
            }
        };
        for_each_body(program, &mut on);
        for d in found {
            self.diagnostics.push(d);
        }
    }

    fn located(&self, mut d: Diagnostic, span: Option<ast::Span>) -> Diagnostic {
        d.file = self.source_file.clone();
        if let Some(s) = span {
            d.line = Some(s.line);
            d.col = Some(s.col);
            d.end_col = Some(s.end_col);
        }
        d
    }

    fn e117(&self, callee: &str, why: &str, span: Option<ast::Span>) -> Diagnostic {
        let d = Diagnostic::error(
            format!("`{callee}`: {why}"),
            "a body of another media type is `http.typed(\"text/csv\", bytes)` with the type written as a literal; HTML is `html.*` and SVG is `svg.*`, served by `http.response(status, body)`".to_string(),
            format!("the media type passed to {callee}"),
        )
        .with_code("E117");
        self.located(d, span)
    }

    fn e116(&self, callee: &str, header: &str, span: Option<ast::Span>, fix: Option<(ast::Span, String)>) -> Diagnostic {
        let mut d = Diagnostic::error(
            format!("`{callee}` writes `{header}` by name, and a response's `{header}` is written from its body's type"),
            "pass the body as its type: a String is text/plain; charset=utf-8, `html.*` builds text/html, `svg.*` image/svg+xml, a `Value` application/json, and any other type is `http.typed(\"text/csv\", bytes)`; `http.response(status, body)` and `http.with_body(resp, body)` write it".to_string(),
            format!("the {header} header written by {callee}"),
        )
        .with_code("E116");
        if let Some((s, text)) = fix {
            d = self.located(d, Some(s));
            return d.with_machine_fix(s.line, s.col, s.end_col, text);
        }
        self.located(d, span)
    }

    /// The machine fix for an owned Content-Type written as a literal pair on
    /// one line: `m.with_headers(st, s, [("Content-Type", "t"), rest…])` →
    /// `m.with_headers(st, m.typed("t", bytes.from_string(s)), [rest…])`, and
    /// `m.set_header(r, "Content-Type", "t")` with `r` a name →
    /// `m.with_body(r, m.typed("t", m.body_bytes(r)))`. Only when the body
    /// is a String, the type is one `typed` accepts, and every piece reads
    /// back from the source; otherwise the diagnostic stands without a fix.
    #[allow(clippy::too_many_arguments)]
    fn owned_header_fix(
        &self,
        callee: &ast::Expr,
        call: &ast::Expr,
        first: Option<&ast::Expr>,
        bound: &[Option<&ast::Expr>],
        params: &[ast::Param],
        hw: &HeaderWrite,
    ) -> Option<(ast::Span, String)> {
        if first.is_some() {
            return None;
        }
        let ast::ExprKind::Member { object, .. } = &callee.kind else { return None };
        let ast::ExprKind::Ident { name: alias } = &object.kind else { return None };
        let span = call.span?;
        let text = self.source_slice(span)?;
        let callee_text = self.source_slice(callee.span?)?;
        if !text.starts_with(&callee_text) || !text.ends_with(')') {
            return None;
        }
        let lit = |e: &ast::Expr| match &e.kind { ast::ExprKind::String { value, .. } => Some(value.clone()), _ => None };
        let slice = |e: &ast::Expr| e.span.and_then(|s| (s.line == span.line).then_some(s)).and_then(|s| self.source_slice(s));
        let is_owned = |n: &str| hw.owned.iter().any(|o| o.eq_ignore_ascii_case(n));
        match hw.fields.as_slice() {
            [pairs] => {
                let pos = params.iter().position(|p| p.name == *pairs)?;
                if pos != 2 || params.len() != 3 {
                    return None;
                }
                let (status, body, list) = (bound[0]?, bound[1]?, bound[2]?);
                if resolve_ty(self.type_map.get(&body.id)?, &self.uf) != Ty::String || !single_token(status) || !single_token(body) {
                    return None;
                }
                // Only a list holding the one owned pair: the pieces of a
                // longer list do not read back from their spans.
                let ast::ExprKind::List { elements } = &list.kind else { return None };
                let [p] = elements.as_slice() else { return None };
                let ast::ExprKind::Tuple { elements: kv } = &p.kind else { return None };
                if kv.len() != 2 || !lit(&kv[0]).is_some_and(|n| is_owned(&n)) {
                    return None;
                }
                let media = lit(&kv[1])?;
                if media_refusal(&media).is_some() {
                    return None;
                }
                Some((span, format!(
                    "{callee_text}({}, {alias}.typed(\"{media}\", bytes.from_string({})), [])",
                    slice(status)?, slice(body)?,
                )))
            }
            [_name, value] => {
                let resp = bound[0]?;
                if !single_token(resp) {
                    return None;
                }
                let pos = params.iter().position(|p| p.name == *value)?;
                let media = lit(bound.get(pos).copied().flatten()?)?;
                if media_refusal(&media).is_some() {
                    return None;
                }
                let r = slice(resp)?;
                Some((span, format!("{alias}.with_body({r}, {alias}.typed(\"{media}\", {alias}.body_bytes({r})))")))
            }
            _ => None,
        }
    }

    /// `module.func` for a callee written `m.f` (an imported stdlib module)
    /// or as a bare selectively imported name.
    fn stdlib_callee(&self, callee: &ast::Expr) -> Option<(String, String)> {
        match &callee.kind {
            ast::ExprKind::Member { object, field } => {
                let ast::ExprKind::Ident { name } = &object.kind else { return None };
                let table = &self.env.import_table;
                let module = table.aliases.get(name).copied()
                    .or_else(|| table.stdlib.contains(name).then_some(*name))?;
                Some((module.to_string(), field.to_string()))
            }
            ast::ExprKind::Ident { name } => {
                if self.env.lookup_var(name.as_str()).is_some() {
                    return None;
                }
                let q = self.env.import_table.resolve_direct(name.as_str())?;
                let (m, f) = q.rsplit_once('.')?;
                Some((m.to_string(), f.to_string()))
            }
            _ => None,
        }
    }

    /// The header-write declaration of the per-type requirement a generic
    /// writer dispatches to: its body calls `p.__self_<verb>(…)` on a
    /// parameter `p`, and the argument passed for `p` has a named type whose
    /// requirement fn (`requirement_fn_name`) declares one.
    fn dispatched_header_write(&self, module: &str, func: &str, bound: &[Option<&ast::Expr>]) -> Option<HeaderWrite> {
        let ast::Decl::Fn { params, body: Some(body), .. } = stdlib_fn_decl(module, func)? else { return None };
        let prefix = almide_lang::stdlib_info::PER_TYPE_REQUIREMENT_PREFIX;
        let mut dispatch: Option<(Sym, Sym)> = None;
        ast::visit_expr(body, &mut |e: &ast::Expr| {
            if dispatch.is_none()
                && let ast::ExprKind::Member { object, field } = &e.kind
                && field.as_str().starts_with(prefix)
                && let ast::ExprKind::Ident { name } = &object.kind
            {
                dispatch = Some((*name, *field));
            }
        });
        let (receiver, method) = dispatch?;
        let pos = params.iter().position(|p| p.name == receiver)?;
        let arg = (*bound.get(pos)?)?;
        let ty = resolve_ty(self.type_map.get(&arg.id)?, &self.uf);
        let Ty::Named(type_name, _) = ty else { return None };
        let impl_fn = almide_lang::stdlib_info::requirement_fn_name(type_name.as_str(), method.as_str());
        header_write_of(module, &impl_fn)
    }
}

/// An argument whose span reads back as its whole source: a name or a
/// literal.
fn single_token(e: &ast::Expr) -> bool {
    matches!(e.kind, ast::ExprKind::Ident { .. } | ast::ExprKind::Int { .. } | ast::ExprKind::String { .. })
}

/// Every expression body of the program: fns (and their parameter
/// defaults), top-level lets and tests.
fn for_each_body(program: &ast::Program, f: &mut impl FnMut(&ast::Expr)) {
    for decl in &program.decls {
        match decl {
            ast::Decl::Fn { params, body, .. } => {
                for p in params {
                    if let Some(d) = &p.default {
                        ast::visit_expr(d, f);
                    }
                }
                if let Some(b) = body {
                    ast::visit_expr(b, f);
                }
            }
            ast::Decl::TopLet { value, .. } => ast::visit_expr(value, f),
            ast::Decl::Test { body, .. } => ast::visit_expr(body, f),
            _ => {}
        }
    }
}
