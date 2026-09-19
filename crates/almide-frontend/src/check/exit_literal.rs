//! Check the portable exit-code domain when the argument is a literal.
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
        if magnitude <= 125 && (!negated || magnitude == 0) {
            return;
        }
        let shown = if negated { format!("-{raw}") } else { raw };
        let mut diagnostic = err(
            format!("exit code {shown} is outside the portable range 0..=125"),
            "Use 0 for success or a code from 1 through 125 for failure",
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
            ExprKind::Member { object, field, .. } if field.as_str() == "exit" => {
                matches!(&object.kind, ExprKind::Ident { name, .. }
                    if table.resolve(name).is_some_and(|module| module.as_str() == "process"))
            }
            _ => false,
        }
    }
}
