// E111 — an inferred type over the `type_size` budget (ADR-0030 D4, #4114).
//
// Two places build a type that can outgrow its source: an expression's type
// as the checker infers it (`(t0, t0)` copies `t0`'s type twice), and the
// resolution of an inference variable, which copies a shared subtree into
// every place it is referenced (`dup(dup(dup(x)))` through a generic). Both
// count nodes against `almide_base::budget`'s `type_size`; the first is
// judged here at the expression, the second is recorded by the resolver
// (`types::take_type_budget_hit`) and reported at the constraint or, after
// solving, at the program. The over-budget type becomes `Unknown`, so every
// later phase sees a bounded type and the run ends in this diagnostic.

use almide_base::budget::{self, BudgetId};

use super::types::{take_type_budget_hit, type_size_capped};
use super::{err, Checker};
use crate::types::Ty;

impl Checker {
    /// `ty` when it fits the budget; otherwise E111 at `span` and `Unknown`.
    pub(crate) fn within_type_budget(&mut self, ty: Ty, span: Option<crate::ast::Span>) -> Ty {
        let cap = budget::limit(BudgetId::TypeSize);
        let n = type_size_capped(&ty, cap);
        budget::add_work(BudgetId::TypeSize, n);
        if n <= cap {
            budget::observe(BudgetId::TypeSize, n);
            return ty;
        }
        self.report_type_budget_at(span, Some(n));
        Ty::Unknown
    }

    /// Report a resolution that stopped at the budget since the last look.
    pub(crate) fn report_type_budget_hit(&mut self, span: Option<crate::ast::Span>) {
        if take_type_budget_hit() {
            self.report_type_budget_at(span, None);
        }
    }

    /// E111, once per site. `counted` is how far the count got before it
    /// stopped (it stops just past the budget, so it is a lower bound).
    pub(crate) fn report_type_budget_at(&mut self, span: Option<crate::ast::Span>, counted: Option<u64>) {
        let key = span.map(|s| (s.line, s.col)).unwrap_or((0, 0));
        if !self.type_budget_sites.insert(key) {
            return;
        }
        let cap = budget::limit(BudgetId::TypeSize);
        let figure = counted.unwrap_or(cap + 1);
        let mut diag = err(
            format!("type too large ({figure}+ nodes, max {cap})"),
            "A value's type grows with every copy of a type inside it — `(t, t)` doubles `t`, and so does a \
             generic that returns its argument twice. Name the intermediate type, or hold the data in a record \
             or a list whose type does not grow with each step. The budget is the compiler's, not the program's: \
             the source cannot raise it (ADR-0030); `--budget type_size=N` on the command line can.",
            "inferred type",
        )
        .with_code("E111");
        diag.file = self.source_file.clone();
        if let Some(s) = span {
            diag.line = Some(s.line);
            diag.col = Some(s.col);
        }
        self.diagnostics.push(diag);
    }
}
