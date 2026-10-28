// E115 (row 11e): HTML is served only from `SafeHtml`. A stdlib fn declared
// `@content_type_guard(name, value, media = "<fn>")` (or `(headers, …)` for a
// list of header pairs) writes header fields into a response it serves, and
// its body aborts when a `Content-Type` field names a media type that the
// module's `<fn>` lists (`http.set_header`, `http.with_headers`); `http.html`
// is the one writer of an HTML type, and its body is `SafeHtml`. When the
// field is written as literals, the same refusal is made here, before the
// program runs. Everything is read off the declarations: the guarded
// parameters off the attribute, the media types off the list the attribute
// names, and a generic `[M: HttpMessage]` writer is guarded exactly when the
// per-type requirement its body dispatches to for the argument's type is.
// A value computed at run time is left to the run-time refusal. Spliced into
// check/mod.rs; runs once per program after inference.

/// The guard a stdlib fn declares: the parameters holding header fields and
/// the media types refused as their `Content-Type`.
struct ContentTypeGuard {
    fields: Vec<Sym>,
    media: Vec<String>,
}

/// The attribute that declares a guard.
const CONTENT_TYPE_GUARD_ATTR: &str = "content_type_guard";

/// One stdlib fn's declaration, from its bundled module source.
fn stdlib_fn_decl(module: &str, func: &str) -> Option<&'static ast::Decl> {
    if !almide_lang::stdlib_info::is_bundled_module(module) {
        return None;
    }
    let program = almide_lang::parse_cached(almide_lang::stdlib_info::bundled_source(module)?)?;
    program.decls.iter().find(|d| matches!(d, ast::Decl::Fn { name, .. } if name.as_str() == func))
}

/// The guard `module.func` declares, if any.
fn content_type_guard_of(module: &str, func: &str) -> Option<ContentTypeGuard> {
    let ast::Decl::Fn { attrs, .. } = stdlib_fn_decl(module, func)? else { return None };
    let attr = attrs.iter().find(|a| a.name.as_str() == CONTENT_TYPE_GUARD_ATTR)?;
    let fields: Vec<Sym> = attr.args.iter()
        .filter(|a| a.name.is_none())
        .filter_map(|a| match &a.value { ast::AttrValue::Ident { name } => Some(*name), _ => None })
        .collect();
    let list_fn = attr.args.iter()
        .filter(|a| a.name.is_some_and(|n| n.as_str() == "media"))
        .find_map(|a| match &a.value { ast::AttrValue::String { value } => Some(value.clone()), _ => None })?;
    let ast::Decl::Fn { body: Some(body), .. } = stdlib_fn_decl(module, &list_fn)? else { return None };
    let ast::ExprKind::List { elements } = &body.kind else { return None };
    let media: Vec<String> = elements.iter()
        .filter_map(|e| match &e.kind { ast::ExprKind::String { value, .. } => Some(value.clone()), _ => None })
        .collect();
    (!fields.is_empty() && !media.is_empty()).then_some(ContentTypeGuard { fields, media })
}

/// Does a `Content-Type` value name one of `media`? Read as the run-time
/// refusal reads it: each comma-separated entry up to its `;`, trimmed, in any
/// case. (The run time trims and folds Unicode; on the ASCII spellings this
/// matches, they agree, so a literal refused here is refused there too.)
fn names_media(value: &str, media: &[String]) -> bool {
    value.split(',').any(|entry| {
        let t = entry.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
        media.iter().any(|m| *m == t)
    })
}

/// A literal header field `(name, value)` that the guard refuses.
fn refused_field(name: &ast::Expr, value: &ast::Expr, media: &[String]) -> Option<String> {
    let (ast::ExprKind::String { value: n, .. }, ast::ExprKind::String { value: v, .. }) = (&name.kind, &value.kind) else {
        return None;
    };
    (n.eq_ignore_ascii_case("content-type") && names_media(v, media)).then(|| v.clone())
}

impl Checker {
    pub(crate) fn check_content_type_literals(&mut self, program: &ast::Program) {
        // A call that is a pipe's right side is judged with the pipe, where
        // its first argument is known.
        let mut piped: std::collections::HashSet<ast::ExprId> = std::collections::HashSet::new();
        let mut collect = |e: &ast::Expr| {
            if let ast::ExprKind::Pipe { right, .. } = &e.kind {
                piped.insert(right.id);
            }
        };
        for_each_body(program, &mut collect);
        let mut found: Vec<(String, String, Option<ast::Span>)> = Vec::new();
        let mut on = |e: &ast::Expr| {
            let (call, first) = match &e.kind {
                ast::ExprKind::Pipe { left, right } => (&**right, Some(&**left)),
                ast::ExprKind::Call { .. } if !piped.contains(&e.id) => (e, None),
                _ => return,
            };
            let ast::ExprKind::Call { callee, args, named_args, .. } = &call.kind else { return };
            let Some((module, func)) = self.stdlib_callee(callee) else { return };
            let Some(ast::Decl::Fn { params, .. }) = stdlib_fn_decl(&module, &func) else { return };
            // The argument bound to each parameter, by position.
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
            let Some(guard) = content_type_guard_of(&module, &func).or_else(|| self.dispatched_guard(&module, &func, &bound)) else {
                return;
            };
            let refused = match guard.fields.as_slice() {
                [name, value] => match (arg_named(*name), arg_named(*value)) {
                    (Some(n), Some(v)) => refused_field(n, v, &guard.media),
                    _ => None,
                },
                [pairs] => match arg_named(*pairs).map(|a| &a.kind) {
                    Some(ast::ExprKind::List { elements }) => elements.iter().find_map(|p| match &p.kind {
                        ast::ExprKind::Tuple { elements: kv } if kv.len() == 2 => refused_field(&kv[0], &kv[1], &guard.media),
                        _ => None,
                    }),
                    _ => None,
                },
                _ => None,
            };
            if let Some(v) = refused {
                found.push((format!("{module}.{func}"), v, call.span.or(e.span)));
            }
        };
        for_each_body(program, &mut on);
        for (callee, value, span) in found {
            let mut d = Diagnostic::error(
                format!("`{callee}` serves `Content-Type: {value}`, an HTML media type, with a String body; HTML is served only from SafeHtml"),
                "build the page as SafeHtml with the html builders (html.escape for text, html.attr / html.url_attr for attributes, html.script_json for data in <script>) and answer with http.html(status, page); trusted markup goes through html.raw(s)".to_string(),
                format!("the Content-Type written by {callee}"),
            )
            .with_code("E115");
            d.file = self.source_file.clone();
            if let Some(s) = span {
                d.line = Some(s.line);
                d.col = Some(s.col);
                d.end_col = Some(s.end_col);
            }
            self.diagnostics.push(d);
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
                let q = self.env.import_table.resolve_direct(name.as_str())?;
                let (m, f) = q.rsplit_once('.')?;
                Some((m.to_string(), f.to_string()))
            }
            _ => None,
        }
    }

    /// The guard of the per-type requirement a generic writer dispatches to:
    /// its body calls `p.__self_<verb>(…)` on a parameter `p`, and the
    /// argument passed for `p` has a named type whose requirement fn
    /// (`requirement_fn_name`) declares a guard.
    fn dispatched_guard(&self, module: &str, func: &str, bound: &[Option<&ast::Expr>]) -> Option<ContentTypeGuard> {
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
        content_type_guard_of(module, &impl_fn)
    }
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
