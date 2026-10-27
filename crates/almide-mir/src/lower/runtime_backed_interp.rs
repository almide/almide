/// RUNTIME-BACKED INTERPOLATION (a pre-erasure program pass, run first inside
/// [`erase_transparent_newtypes`], so the pipeline AND classify both see it —
/// desugar-before-both: the caps `mir == ir` count reads the rewritten tree on
/// both sides).
///
/// A runtime-backed value (`HttpRequest`, `HttpResponse`, `HttpCall`
/// — `stdlib_info::RUNTIME_BACKED_TYPES`) displays as its DECLARED repr
/// (`<module>.__<type>_repr`, #4204), bare or nested in a container. Newtype
/// erasure then rewrites the self-hosted ones to their carrier (`List[String]`),
/// after which the interpolation desugar can only see the carrier and would show
/// it (every header value). So, while the nominal type is still visible:
///
///   - a bare runtime-backed part `${req}` becomes `${http.__http_request_repr(req)}`;
///   - a LITERAL tuple / list / `some(..)` / named-record part that holds one
///     (`${(req, path, 1)}`, `${[c, c]}`, `${Wrap { req: r, .. }}`) is FLATTENED
///     into the text parts of its display — `"(" ++ repr(req) ++ ", " ++ repr(path)
///     ++ ", " ++ "1" ++ ")"` — so no heap aggregate has to be materialized just to
///     be printed. Each element expression appears exactly once, in source order
///     (the literal's own evaluation order); a String element shows quoted
///     (`string.quote`), an Int/Bool element as its digits / `true`/`false`;
///   - any other part whose type mentions a runtime-backed type (a `Var` of
///     `List[HttpRequest]`, a user variant whose payload holds one (#4230), an
///     element this pass has no nested display for) routes
///     to the unlinked `compound.to_string` — the established honest wall, never
///     the carrier's bytes.
///
/// Parts whose type mentions no runtime-backed type are left untouched.
pub fn rewrite_runtime_backed_interps(program: &mut almide_ir::IrProgram) {
    use almide_ir::visit_mut::IrMutVisitor;
    let mut records: HashMap<String, Vec<(almide_lang::intern::Sym, Ty)>> = HashMap::new();
    let mut variants: HashMap<String, Vec<Ty>> = HashMap::new();
    let decls = program.type_decls.iter().chain(program.modules.iter().flat_map(|m| m.type_decls.iter()));
    for td in decls {
        // Generic declarations are kept: a field of a fixed runtime-backed type
        // (`Got(T, HttpRequest)`) mentions one whatever the arguments are, and a
        // field of a type parameter mentions nothing by itself (the arguments are
        // asked separately). Only the literal flattening needs no arguments.
        match &td.kind {
            almide_ir::IrTypeDeclKind::Record { fields } => {
                records.insert(
                    td.name.as_str().to_string(),
                    fields.iter().map(|f| (f.name, f.ty.clone())).collect(),
                );
            }
            // A variant's payloads, every case's in one list (#4230): the
            // question asked of a declaration is only "does any field mention a
            // runtime-backed type", so the case a field belongs to is irrelevant.
            almide_ir::IrTypeDeclKind::Variant { cases, .. } => {
                let payloads = cases.iter().flat_map(|c| match &c.kind {
                    almide_ir::IrVariantKind::Unit => Vec::new(),
                    almide_ir::IrVariantKind::Tuple { fields } => fields.clone(),
                    almide_ir::IrVariantKind::Record { fields } => fields.iter().map(|f| f.ty.clone()).collect(),
                });
                variants.insert(td.name.as_str().to_string(), payloads.collect());
            }
            almide_ir::IrTypeDeclKind::Alias { .. } => {}
        }
    }
    let mut v = RbInterp { records: &records, variants: &variants };
    for f in program.functions.iter_mut() {
        v.visit_expr_mut(&mut f.body);
    }
    for m in program.modules.iter_mut() {
        for f in m.functions.iter_mut() {
            v.visit_expr_mut(&mut f.body);
        }
        for tl in m.top_lets.iter_mut() {
            v.visit_expr_mut(&mut tl.value);
        }
    }
    for tl in program.top_lets.iter_mut() {
        v.visit_expr_mut(&mut tl.value);
    }
}

struct RbInterp<'a> {
    /// Record declarations: name → declaration-ordered fields.
    records: &'a HashMap<String, Vec<(almide_lang::intern::Sym, Ty)>>,
    /// Variant declarations: name → the payload types of all cases.
    variants: &'a HashMap<String, Vec<Ty>>,
}

impl almide_ir::visit_mut::IrMutVisitor for RbInterp<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        almide_ir::visit_mut::walk_expr_mut(self, e);
        let IrExprKind::StringInterp { parts } = &mut e.kind else { return };
        if !parts.iter().any(|p| matches!(p, IrStringPart::Expr { expr } if self.mentions_rb(&expr.ty, &mut Vec::new()))) {
            return;
        }
        let old = std::mem::take(parts);
        for p in old {
            match p {
                IrStringPart::Expr { expr } if self.mentions_rb(&expr.ty, &mut Vec::new()) => {
                    match self.top_part(&expr) {
                        Some(mut flat) => parts.append(&mut flat),
                        None => parts.push(IrStringPart::Expr {
                            expr: to_string_call("compound", "to_string", expr),
                        }),
                    }
                }
                other => parts.push(other),
            }
        }
    }
}

impl RbInterp<'_> {
    /// Does `ty` mention a runtime-backed type anywhere (through containers,
    /// declared record fields and variant payloads, #4230)? `seen` holds the record declarations already on
    /// the path: a recursive record (`kids: List[Node]`) adds nothing on re-entry,
    /// so it mentions one only if some non-recursive field does.
    fn mentions_rb(&self, ty: &Ty, seen: &mut Vec<String>) -> bool {
        match ty {
            Ty::Named(name, args) => {
                if rb_repr(ty).is_some() || args.iter().any(|a| self.mentions_rb(a, seen)) {
                    return true;
                }
                let key = name.as_str().to_string();
                if seen.contains(&key) {
                    return false;
                }
                let fields: Vec<&Ty> = match (self.records.get(&key), self.variants.get(&key)) {
                    (Some(fs), _) => fs.iter().map(|(_, t)| t).collect(),
                    (None, Some(ps)) => ps.iter().collect(),
                    (None, None) => return false,
                };
                seen.push(key);
                let hit = fields.into_iter().any(|t| self.mentions_rb(t, seen));
                seen.pop();
                hit
            }
            // A structural variant type carries its cases' payloads inline.
            Ty::Variant { cases, .. } => cases.iter().any(|c| match &c.payload {
                almide_lang::types::VariantPayload::Unit => false,
                almide_lang::types::VariantPayload::Tuple(ts) => ts.iter().any(|t| self.mentions_rb(t, seen)),
                almide_lang::types::VariantPayload::Record(fs) => fs.iter().any(|(_, t)| self.mentions_rb(t, seen)),
            }),
            Ty::Applied(_, args) | Ty::Tuple(args) => args.iter().any(|a| self.mentions_rb(a, seen)),
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                fields.iter().any(|(_, t)| self.mentions_rb(t, seen))
            }
            _ => false,
        }
    }

    /// The text parts of a TOP-LEVEL interpolation part that mentions a
    /// runtime-backed type, or `None` (the caller walls it).
    fn top_part(&self, e: &IrExpr) -> Option<Vec<IrStringPart>> {
        if let Some((module, func)) = rb_repr(&e.ty) {
            return Some(vec![IrStringPart::Expr { expr: to_string_call(&module, &func, e.clone()) }]);
        }
        self.literal_container(e)
    }

    /// The display of an ELEMENT nested in a container: a runtime-backed value
    /// shows its repr (the same text as bare), a String shows quoted, an Int /
    /// Bool shows as at the top level, a nested literal container recurses.
    /// `None` for anything else.
    fn nested(&self, e: &IrExpr) -> Option<Vec<IrStringPart>> {
        if let Some((module, func)) = rb_repr(&e.ty) {
            return Some(vec![IrStringPart::Expr { expr: to_string_call(&module, &func, e.clone()) }]);
        }
        match &e.ty {
            Ty::String => Some(vec![IrStringPart::Expr { expr: to_string_call("string", "quote", e.clone()) }]),
            Ty::Int | Ty::Bool => Some(vec![IrStringPart::Expr { expr: e.clone() }]),
            _ => self.literal_container(e),
        }
    }

    /// A tuple / list / `some(..)` / named-record LITERAL, flattened.
    fn literal_container(&self, e: &IrExpr) -> Option<Vec<IrStringPart>> {
        let lit = |s: &str| IrStringPart::Lit { value: s.to_string() };
        let seq = |open: &str, elems: &[&IrExpr], close: &str| -> Option<Vec<IrStringPart>> {
            let mut out = vec![lit(open)];
            for (i, el) in elems.iter().enumerate() {
                if i > 0 {
                    out.push(lit(", "));
                }
                out.extend(self.nested(el)?);
            }
            out.push(lit(close));
            Some(out)
        };
        match &e.kind {
            IrExprKind::Tuple { elements } => seq("(", &elements.iter().collect::<Vec<_>>(), ")"),
            IrExprKind::List { elements } => seq("[", &elements.iter().collect::<Vec<_>>(), "]"),
            IrExprKind::OptionSome { expr } => seq("some(", &[&**expr], ")"),
            IrExprKind::Record { fields, .. } => {
                let Ty::Named(name, args) = &e.ty else { return None };
                if !args.is_empty() {
                    return None;
                }
                let decl = self.records.get(name.as_str())?;
                // Only the DECLARATION-ordered literal: then the display order is
                // the evaluation order, and no field is defaulted or missing.
                if decl.len() != fields.len() || decl.iter().zip(fields.iter()).any(|((d, _), (f, _))| d != f) {
                    return None;
                }
                let shown = almide_ir::declared_type_name(name.as_str());
                let mut out = vec![lit(&format!("{shown} {{ "))];
                for (i, (fname, fe)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(lit(", "));
                    }
                    out.push(lit(&format!("{}: ", fname.as_str())));
                    out.extend(self.nested(fe)?);
                }
                out.push(lit(" }"));
                Some(out)
            }
            _ => None,
        }
    }
}

/// `(module, func)` of the declared repr when `ty` IS a runtime-backed type
/// (`stdlib_info::runtime_backed_repr_surface`, derived from its row).
fn rb_repr(ty: &Ty) -> Option<(String, String)> {
    let Ty::Named(name, args) = ty else { return None };
    if !args.is_empty() {
        return None;
    }
    let surface = almide_lang::stdlib_info::runtime_backed_repr_surface(name.as_str())?;
    let (m, f) = surface.split_once('.')?;
    Some((m.to_string(), f.to_string()))
}
