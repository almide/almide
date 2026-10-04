// Continuation of `impl Checker` — the module qualifier of a TYPE spelling
// (#3336). Spliced into check/mod.rs via `include!`, same module scope.
//
// `fn root() -> v.View` with no module `v` used to be accepted: the resolver
// strips the qualifier when the qualified key misses and answers the bare
// name, so the program bound to whatever same-named type was in scope. In an
// expression `v.f()` is E003 with the import fix-it; a type position now says
// the same thing, and a module that IS in scope but declares no such type is
// named too. Both are E029 — the unknown-type family.

/// Why a qualified type spelling resolves to nothing.
enum QualifiedTypeMiss {
    /// The qualifier names no module this file can see.
    UnknownModule,
    /// The qualifier is a module in scope that declares no such type.
    NoSuchType,
}

impl Checker {
    /// E029 for every qualified type spelling (`m.T` in an annotation, a
    /// field, a payload, an alias target or a type argument) whose qualifier
    /// is not a module in scope, or whose module declares no `T`.
    pub(crate) fn validate_qualified_type_heads(&mut self, program: &mut ast::Program) {
        let spelled = import_spellings(program).qualified_types;
        if !spelled.iter().any(|n| self.qualified_type_miss(*n).is_some()) {
            return;
        }
        let mut shell = program.clone();
        shell.decls.clear();
        let saved = self.current_span;
        let mut reported: std::collections::HashSet<Sym> = std::collections::HashSet::new();
        for decl in &program.decls {
            let span = match decl {
                ast::Decl::Fn { span, .. } | ast::Decl::Type { span, .. } | ast::Decl::Protocol { span, .. }
                | ast::Decl::TopLet { span, .. } | ast::Decl::Test { span, .. }
                | ast::Decl::TestWhereDef { span, .. } => *span,
                ast::Decl::Module { .. } | ast::Decl::Import { .. } => continue,
            };
            shell.decls = vec![decl.clone()];
            let mut here: Vec<Sym> = import_spellings(&mut shell).qualified_types.into_iter().collect();
            here.sort_by(|a, b| a.as_str().cmp(b.as_str()));
            for name in here {
                let Some(miss) = self.qualified_type_miss(name) else { continue };
                if reported.insert(name) {
                    self.qualified_type_misses.insert(name);
                    if let Some((_, bare)) = name.as_str().rsplit_once('.') {
                        self.qualified_type_misses.insert(sym(bare));
                    }
                    let diag = self.qualified_type_diag(name, miss, span);
                    self.diagnostics.push(diag);
                }
            }
        }
        self.current_span = saved;
    }

    /// `None` when the qualified spelling `name` (`head.T`) names a type.
    fn qualified_type_miss(&self, name: Sym) -> Option<QualifiedTypeMiss> {
        let (head, bare) = name.as_str().rsplit_once('.')?;
        let Some(module) = self.type_qualifier_module(head) else {
            return Some(QualifiedTypeMiss::UnknownModule);
        };
        let types = &self.env.types;
        let cur = self.current_module_prefix.as_deref();
        let declared = types.contains_key(&name)
            || types.contains_key(&sym(&format!("{}.{}", module, bare)))
            || crate::canonicalize::resolve::canonical_user_type_sym(name.as_str(), types, cur).is_some();
        let stdlib_head = crate::stdlib::is_stdlib_module(head) || almide_lang::stdlib_info::is_bundled_module(head);
        let stdlib_type = stdlib_head
            && (types.contains_key(&sym(bare))
                || crate::canonicalize::resolve::builtin_type_head(bare, TypeSpelling::Bare).is_some()
                || almide_lang::stdlib_info::stdlib_owned_type_owner(bare).is_some()
                || almide_lang::stdlib_info::runtime_backed_type_owner(bare).is_some()
                || crate::bundled_sigs::bundled_type_owner(bare).is_some());
        (!declared && !stdlib_type).then_some(QualifiedTypeMiss::NoSuchType)
    }

    /// The canonical module a type qualifier `head` names in this file: an
    /// import (alias or canonical), an auto-imported stdlib module, the file's
    /// own module, or a sibling submodule of the file's package.
    fn type_qualifier_module(&self, head: &str) -> Option<String> {
        if let Some(m) = self.env.import_table.resolve(head) {
            return Some(m.to_string());
        }
        let stdlib = crate::stdlib::is_stdlib_module(head) || almide_lang::stdlib_info::is_bundled_module(head);
        if stdlib && !crate::stdlib::is_import_suggestable(head) {
            return Some(head.to_string());
        }
        let cur = self.current_module_prefix.as_deref()?;
        if cur == head || cur.ends_with(&format!(".{}", head)) {
            return Some(cur.to_string());
        }
        let pkg = cur.split('.').next()?;
        let sibling = format!("{}.{}", pkg, head);
        self.env.user_modules.contains(&sym(&sibling)).then_some(sibling)
    }

    /// A type the module `head` declares within edit distance of `bare`.
    fn nearest_module_type(&self, head: &str, bare: &str) -> Option<String> {
        let module = self.type_qualifier_module(head)?;
        let prefix = format!("{}.", module);
        let candidates: Vec<&str> = self.env.types.keys()
            .filter_map(|k| k.as_str().strip_prefix(prefix.as_str()))
            .filter(|b| !b.contains('.'))
            .collect();
        almide_base::diagnostic::suggest(bare, candidates.into_iter()).map(|s| s.to_string())
    }

    /// The E029 for one qualified spelling, located at it.
    fn qualified_type_diag(&mut self, name: Sym, miss: QualifiedTypeMiss, span: Option<crate::ast::Span>) -> Diagnostic {
        let (head, bare) = name.as_str().rsplit_once('.').unwrap_or(("", name.as_str()));
        let import_fix = matches!(miss, QualifiedTypeMiss::UnknownModule) && crate::stdlib::is_import_suggestable(head);
        let (message, hint) = match miss {
            QualifiedTypeMiss::UnknownModule if import_fix => (
                format!("unknown module '{}' in type '{}'", head, name),
                format!(
                    "Add `import {}` (stdlib: {})\nOr run `almide fmt` to auto-add missing imports",
                    head, crate::stdlib::module_description(head)
                ),
            ),
            QualifiedTypeMiss::UnknownModule => (
                format!("unknown module '{}' in type '{}'", head, name),
                format!(
                    "No module '{}' is imported in this file. Import the module that declares `{}` \
                     (`import self.{}` for a module of this package), or drop the qualifier for a type \
                     this file declares",
                    head, bare, head
                ),
            ),
            QualifiedTypeMiss::NoSuchType => (
                format!("unknown type '{}': module '{}' declares no such type", bare, head),
                match self.nearest_module_type(head, bare) {
                    Some(t) => format!("did you mean `{}.{}`? Otherwise declare `type {}` in module '{}'", head, t, bare, head),
                    None => format!("Check the type name, or declare `type {}` in module '{}'", bare, head),
                },
            ),
        };
        self.current_span = span;
        let mut diag = err(message, hint, format!("type {}", name)).with_code("E029");
        if let Some(sp) = span {
            diag.file = self.source_file.clone();
            diag.line = Some(sp.line);
            diag.col = Some(sp.col);
            if let Some((line, col)) = self.locate_type_name(sp.line, sp.col, name.as_str()) {
                diag.line = Some(line);
                diag.col = Some(col);
                diag.end_col = Some(col + name.as_str().chars().count());
            }
        }
        if import_fix {
            // SUGGESTION: a placement heuristic, as the expression form's.
            diag = diag.with_suggested_fix(1, 1, 1, format!("import {}\n", head));
        }
        diag
    }
}
