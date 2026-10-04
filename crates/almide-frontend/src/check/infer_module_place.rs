// Writes into ANOTHER module's top-level binding — `m.x = v`, `m.xs[i] = v`,
// `m.r.f = v` (#3312): the place's root and its mutability. Included by
// `infer_statements.rs`.

impl Checker {
    /// The `top_lets` key of a place rooted at ANOTHER module's top-level
    /// binding: `target` names no local or root binding, resolves as an
    /// imported module, and `first` is one of its top-level `let`/`var`s
    /// (#3312). The write marks the import used, as a read does.
    fn module_place(&mut self, target: &Sym, first: Option<&Sym>) -> Option<Sym> {
        if self.env.lookup_var(target.as_str()).is_some() || self.env.top_lets.contains_key(target) {
            return None;
        }
        let module = self.env.import_table.resolve(target.as_str())?;
        let key = sym(&format!("{}.{}", module, first?));
        self.env.top_lets.contains_key(&key).then(|| {
            self.env.import_table.mark_used(target.as_str());
            key
        })
    }

    /// E009 for a write into another module's top-level `let` (#3312): only
    /// a `var` is writable from outside its module, as from inside it.
    fn check_module_place_mutable(&mut self, module: &Sym, name: &Sym, key: Sym, shape: String) {
        if self.env.mutable_top_lets.contains(&key) {
            return;
        }
        self.emit(super::err(
            format!("cannot mutate immutable binding '{}.{}'", module, name),
            format!("'{}' is a `let` of module '{}'; declare it `var {} = ...` there to write it", name, module, name),
            shape,
        ).with_code("E009").with_try(format!("var {} = ...", name)));
    }
}
