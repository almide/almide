//! Check the exit-status domain 0..=255 when the argument is a literal (C-351).
//! The WASI preview-1 build's narrower 126..=255 wall is C-350's runtime line,
//! not a check-time error: the checker does not know the build's target.
use super::{Checker, err, int_literal_chain};
use crate::ast::{Expr, ExprKind};
use almide_base::intern::sym;

impl Checker {
    pub(super) fn reject_exit_literal(&mut self, callee: &Expr, argument: &Expr) {
        if !self.is_stdlib_exit(callee) {
            return;
        }
        let Some((_, raw, negated)) = int_literal_chain(argument) else {
            return;
        };
        let clean = raw.replace('_', "");
        let (radix, digits) = crate::literals::radix_and_digits(&clean);
        let Ok(magnitude) = u128::from_str_radix(digits, radix) else {
            // E024 owns integer magnitudes too large to represent.
            return;
        };
        if magnitude <= 255 && (!negated || magnitude == 0) {
            return;
        }
        let shown = if negated { format!("-{raw}") } else { raw };
        let mut diagnostic = err(
            format!("exit code {shown} is outside the exit-status range 0..=255"),
            "Use 0 for success or a code from 1 through 255 for failure (a WASI preview-1 build delivers only 0..=125)",
            "process.exit argument",
        )
        .with_code("E084");
        if let Some(span) = argument.span {
            diagnostic.file = self.source_file.clone();
            diagnostic.line = Some(span.line);
            diagnostic.col = Some(span.col);
            diagnostic.end_col = Some(span.end_col);
        }
        self.emit(diagnostic);
    }

    fn is_stdlib_exit(&self, callee: &Expr) -> bool {
        let table = &self.env.import_table;
        if !table.stdlib.contains(&sym("process")) {
            return false;
        }
        match &callee.kind {
            ExprKind::Paren { expr } => self.is_stdlib_exit(expr),
            ExprKind::Ident { name, .. } => {
                self.env.lookup_var(name).is_none()
                    && table.resolve_direct(name).as_deref() == Some("process.exit")
            }
            // A local binding of the module's own name wins, as it does for a
            // bare `exit` above. The checker's member resolution still reaches
            // past it (#2345), so this guard is deliberately ahead of it: a
            // literal we decline to judge falls back to C-350's runtime check,
            // whereas a wrong E084 would reject a program that is fine.
            ExprKind::Member { object, field, .. } if field.as_str() == "exit" => {
                matches!(&object.kind, ExprKind::Ident { name, .. }
                    if self.env.lookup_var(name).is_none()
                        && table.resolve(name).is_some_and(|module| module.as_str() == "process"))
            }
            _ => false,
        }
    }
}
