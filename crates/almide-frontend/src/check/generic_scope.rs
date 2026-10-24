// Continuation of `impl Checker` — entering and leaving a declaration's
// generic parameters (type vars, const params, structural and protocol
// bounds). Split out of module_inference.rs to keep it under the 800-line
// codopsy max-lines threshold; same module scope via `include!` from
// check/mod.rs (no privacy boundary).

impl Checker {
    /// Push generic type vars, structural bounds, and protocol bounds into the
    /// environment. Returns the SHADOWED `env.types` bindings so
    /// `exit_generics` can restore them — a generic letter shadows a
    /// same-named type for one declaration, it must not destroy it (#1574).
    fn enter_generics(&mut self, generics: &Option<Vec<ast::GenericParam>>) -> Vec<(Sym, Option<Ty>)> {
        let mut shadowed: Vec<(Sym, Option<Ty>)> = Vec::new();
        let gs = match generics { Some(gs) => gs, None => return shadowed };
        for g in gs.iter() {
            self.enter_generic_param(g, &mut shadowed);
        }
        // Applied bounds' type arguments (#1589), resolved once every letter
        // of this declaration is in scope — `[R: Repository[K, V], K, V]`
        // names letters declared after the bound.
        for g in gs.iter() {
            self.enter_generic_bound_args(g);
        }
        shadowed
    }

    /// One generic parameter of [`Self::enter_generics`]: its type binding
    /// (a const param or a type var), its structural bound, its protocol bounds.
    fn enter_generic_param(&mut self, g: &ast::GenericParam, shadowed: &mut Vec<(Sym, Option<Ty>)>) {
        use crate::canonicalize::registration::SCALAR_TYPE_NAMES;
        // A const param is a generic with a single scalar type bound. The
        // bound is carried out of the same expression that tests for it —
        // the previous form tested `bounds` here and re-read `bounds[0]`
        // below, so the test and the read could drift.
        let const_bound = g.bounds.as_ref().and_then(|bs| match bs.as_slice() {
            [only] if SCALAR_TYPE_NAMES.contains(&only.as_str()) => Some(only.clone()),
            _ => None,
        });
        let is_const = const_bound.is_some();
        let bound_ty = match const_bound {
            Some(bound) => {
                let ty = self.resolve_type_expr(&ast::TypeExpr::Simple { name: sym(&bound) });
                Ty::ConstParam { name: sym(&g.name), ty: Box::new(ty) }
            }
            None => Ty::TypeVar(sym(&g.name)),
        };
        let prev = self.env.types.insert(sym(&g.name), bound_ty);
        shadowed.push((sym(&g.name), prev));
        if let Some(bte) = &g.structural_bound {
            let bt = self.resolve_type_expr(bte);
            self.env.structural_bounds.insert(sym(&g.name), match bt { Ty::Record { fields } => Ty::OpenRecord { fields }, o => o });
        }
        let Some(bounds) = &g.bounds else { return };
        if bounds.is_empty() || is_const {
            return;
        }
        // #4097: a bound names a protocol, built in or declared. An unknown
        // name used to pass here and fail only at a call site, as "Int does
        // not implement Nonexistent".
        for (i, b) in bounds.iter().enumerate() {
            if !self.protocol_in_scope(b.as_str()) {
                self.emit_unknown_bound_protocol(g, i, b.as_str());
            }
        }
        self.env.generic_protocol_bounds.insert(sym(&g.name), bounds.iter().map(|b| sym(b)).collect());
    }

    /// The type arguments of `g`'s applied protocol bounds (#1589).
    fn enter_generic_bound_args(&mut self, g: &ast::GenericParam) {
        for (i, b) in g.bounds.iter().flatten().enumerate() {
            let Some(r) = ast::protocol_ref_at(&g.bound_refs, i) else { continue };
            if r.args.is_empty() { continue; }
            let args: Vec<Ty> = r.args.iter().map(|a| self.resolve_type_expr(a)).collect();
            self.env.generic_protocol_bound_args.insert((g.name, *b), args);
        }
    }

    /// The diagnostic for bound `i` of `g`, which names no protocol in scope
    /// (#4097), anchored at the bound's own span when the parser recorded one.
    fn emit_unknown_bound_protocol(&mut self, g: &ast::GenericParam, i: usize, b: &str) {
        let builtins: Vec<&str> = crate::canonicalize::protocols::BUILTIN_PROTOCOLS.iter().map(|p| p.name).collect();
        let mut d = crate::check::err(
            format!("unknown protocol '{}' in the bound of '{}'", b, g.name),
            format!("A bound names a protocol: a built-in one ({}), or one declared with `protocol {} {{ ... }}` here or in an imported module",
                builtins.join(", "), b),
            format!("generic parameter {}", g.name));
        if let Some(sp) = g.bound_spans.get(i) {
            d.file = self.source_file.clone();
            d.line = Some(sp.line);
            d.col = Some(sp.col);
            if sp.end_col > sp.col { d.end_col = Some(sp.end_col); }
        }
        self.emit(d);
    }

    /// Is protocol `name` declared — built in, in this file, or by an import
    /// (registered under its module-qualified name)?
    fn protocol_in_scope(&self, name: &str) -> bool {
        let bare = name.rsplit_once('.').map_or(name, |(_, b)| b);
        let suffix = format!(".{}", bare);
        self.env.protocols.contains_key(&sym(name))
            || self.env.protocols.contains_key(&sym(bare))
            || self.env.protocols.keys().any(|k| k.as_str().ends_with(suffix.as_str()))
    }

    /// Remove generic structural/protocol bounds and RESTORE the `env.types`
    /// bindings the generic letters shadowed (#1574).
    fn exit_generics(&mut self, generics: &Option<Vec<ast::GenericParam>>, shadowed: Vec<(Sym, Option<Ty>)>) {
        if let Some(gs) = generics {
            for g in gs.iter() {
                self.env.types.remove(&sym(&g.name));
                self.env.structural_bounds.remove(&sym(&g.name));
                self.env.generic_protocol_bounds.remove(&sym(&g.name));
                for b in g.bounds.iter().flatten() {
                    self.env.generic_protocol_bound_args.remove(&(g.name, *b));
                }
            }
        }
        for (gn, prev) in shadowed.into_iter().rev() {
            match prev {
                Some(t) => { self.env.types.insert(gn, t); }
                None => { self.env.types.remove(&gn); }
            }
        }
    }
}
