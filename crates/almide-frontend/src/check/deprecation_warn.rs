//! E052: calling a `@deprecated` function.
//!
//! The warning's job is to be a REPAIR INSTRUCTION, not a notice. It names
//! the replacement, states whether the edit is mechanical (derived by
//! comparing the two signatures, so the classification cannot rot), and when
//! it is mechanical attaches a machine-applicable fix so `almide fix` can do
//! the migration without a model in the loop at all.

use almide_base::diagnostic::Diagnostic;
use almide_base::intern::sym;
use almide_lang::ast;
use almide_lang::ast::ExprKind;

use crate::deprecation::classify;

impl super::Checker {
    /// The `functions` key a callee resolves to, mirroring `lookup_call_sig`.
    /// `None` for shapes that do not name a top-level function.
    pub(crate) fn callee_key(&self, callee: &ast::Expr) -> Option<String> {
        match &callee.kind {
            ExprKind::Ident { name, .. } => Some(name.to_string()),
            ExprKind::Member { object, field, .. } => {
                let ExprKind::Ident { name: module, .. } = &object.kind else { return None };
                let canonical = self
                    .env
                    .import_table
                    .resolve(module.as_str())
                    .map(|s| s.as_str().to_string())
                    .unwrap_or_else(|| module.to_string());
                Some(format!("{canonical}.{field}"))
            }
            _ => None,
        }
    }

    pub(crate) fn warn_if_deprecated(&mut self, callee: &ast::Expr) {
        let Some(key) = self.callee_key(callee) else { return };
        self.warn_deprecated_key(&key, callee.span, None);
    }

    /// The deprecation marker `key` resolves to: a registered one, else — for
    /// a stdlib fn the call resolves through the bundled-signature fallback,
    /// exactly as `lookup_call_sig` does — the marker on the bundled source.
    fn deprecation_of(&self, key: &str) -> Option<crate::deprecation::Deprecation> {
        if let Some(dep) = self.env.deprecations.get(&sym(key)) {
            return Some(dep.clone());
        }
        if self.env.functions.contains_key(&sym(key)) {
            return None;
        }
        let (module, f) = key.split_once('.')?;
        crate::stdlib::is_stdlib_module(module).then_some(())?;
        crate::bundled_sigs::lookup_deprecation(module, f)
    }

    /// E052 for the UFCS form `x.method(…)` that resolved to the stdlib fn
    /// `module.method` — the third spelling of a call, after the direct call
    /// and the pipe stage (#3085). `span` is the method NAME's span, so the
    /// machine fix renames the method in place (`xs.length()` → `xs.len()`),
    /// which is only the whole edit when the replacement lives in the same
    /// module.
    pub(crate) fn warn_if_deprecated_method(&mut self, module: &str, method: &str, span: Option<ast::Span>) {
        self.warn_deprecated_key(&format!("{module}.{method}"), span, Some(module));
    }

    fn warn_deprecated_key(&mut self, key: &str, span: Option<ast::Span>, method_of: Option<&str>) {
        let key = key.to_string();
        let Some(dep) = self.deprecation_of(&key) else { return };

        let old_sig = self.env.functions.get(&sym(&key)).cloned().or_else(|| {
            let (module, f) = key.split_once('.')?;
            crate::stdlib::lookup_sig(module, f)
        });
        let new_sig = dep.use_instead.as_ref().and_then(|r| {
            self.env.functions.get(&sym(r)).cloned().or_else(|| {
                let (module, f) = r.split_once('.')?;
                crate::stdlib::lookup_sig(module, f)
            })
        });

        let (message, hint, edit) = match &dep.use_instead {
            // An OPERATOR replacement (`use = "??"`, ADR-0005 D4): the edit is
            // a call-shape rewrite, not a rename, so no span fix-it — the AST
            // family of `almide fix` performs it (both the direct call and
            // the pipe stage). The hint spells the target shape.
            Some(op) if !op.contains('.') && !op.chars().all(|c| c.is_alphanumeric() || c == '_') => (
                format!("`{key}` is deprecated since dialect {}", dep.since),
                format!(
                    "Use the `{op}` operator — `value {op} default` — the default is evaluated only on the fallback path; {}.{}",
                    // The AST rewrite covers the direct call and the pipe
                    // stage; the method form's receiver type is not known to
                    // it, so that one edit is the reader's.
                    match method_of {
                        None => "`almide fix` rewrites the call".to_string(),
                        Some(_) => format!("write `x {op} default` for `x.{}(default)`", key.rsplit('.').next().unwrap_or_default()),
                    },
                    match &dep.note {
                        Some(n) => format!(" Note: {n}"),
                        None => String::new(),
                    }
                ),
                None,
            ),
            Some(replacement) => {
                let edit = classify(old_sig.as_ref(), new_sig.as_ref());
                (
                    format!("`{key}` is deprecated since dialect {}", dep.since),
                    format!(
                        "Use `{replacement}` — {}.{}",
                        edit.describe(),
                        match &dep.note {
                            Some(n) => format!(" Note: {n}"),
                            None => String::new(),
                        }
                    ),
                    Some((replacement.clone(), edit)),
                )
            }
            None => (
                format!("`{key}` is deprecated since dialect {}", dep.since),
                format!(
                    "{} There is no drop-in replacement.",
                    dep.note.clone().unwrap_or_default()
                ),
                None,
            ),
        };

        let mut diag = Diagnostic::warning(message, hint, key.clone()).with_code("E052");
        if let Some(s) = &span {
            diag.file = self.source_file.clone();
            diag.line = Some(s.line);
            diag.col = Some(s.col);
            if s.end_col > s.col {
                diag.end_col = Some(s.end_col);
            }
            // MACHINE-APPLICABLE only where swapping the name IS the whole
            // edit — `almide fix` applies these unattended, so offering one
            // for a signature change would hand it a miscompile. A signature
            // change still shows the replacement in the hint; a human or a
            // model does that edit. The method form renames only the method, so the fix is the
            // replacement's bare name — and exists only when the replacement
            // is in the receiver's module (else `x.new()` would not resolve).
            let spelled = match method_of {
                None => edit.as_ref().map(|(r, _)| r.clone()),
                Some(m) => edit.as_ref().and_then(|(r, _)| r.strip_prefix(m)?.strip_prefix('.')).map(str::to_string),
            };
            if let (Some((_, edit)), Some(text)) = (&edit, spelled) {
                if edit.is_mechanical() && s.end_col > s.col {
                    diag = diag.with_machine_fix(s.line, s.col, s.end_col, text);
                }
            }
        }
        if edit.as_ref().is_some_and(|(_, e)| !e.is_mechanical()) {
            if let Some((replacement, _)) = &edit {
                diag = diag.with_try(replacement.clone());
            }
        }
        self.diagnostics.push(diag);
    }
}

#[cfg(test)]
mod tests {
    use crate::deprecation::EditKind;

    /// The fix-it rule, stated as a test so a later edit cannot quietly start
    /// offering `almide fix` a rewrite that changes behavior.
    #[test]
    fn only_a_mechanical_rename_offers_a_fix_it() {
        assert!(EditKind::Rename.is_mechanical());
        assert!(!EditKind::SignatureChanged.is_mechanical());
        assert!(!EditKind::ReplacementUnknown.is_mechanical());
    }
}
