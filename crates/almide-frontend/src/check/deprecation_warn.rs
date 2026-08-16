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

use crate::deprecation::{classify, EditKind};

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
        let Some(dep) = self.env.deprecations.get(&sym(&key)).cloned() else { return };

        let old_sig = self.env.functions.get(&sym(&key)).cloned();
        let new_sig = dep.use_instead.as_ref().and_then(|r| {
            self.env.functions.get(&sym(r)).cloned().or_else(|| {
                let (module, f) = r.split_once('.')?;
                crate::stdlib::lookup_sig(module, f)
            })
        });

        let (message, hint, edit) = match &dep.use_instead {
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
        if let Some(s) = &callee.span {
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
            // model does that edit.
            if let Some((replacement, edit)) = &edit {
                if edit.is_mechanical() && s.end_col > s.col {
                    diag = diag.with_machine_fix(s.line, s.col, s.end_col, replacement.clone());
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
