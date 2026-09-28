//! WHERE the structural leg declined (#2807).
//!
//! An emit wall is a reason string (`ty-mismatch:Option(ETy(0))-vs-Scalar(Str)`)
//! that doubles as a census key, so it carries no location. In a 77-file
//! package that string alone cannot be acted on. This side channel records the
//! function whose body did not lower and the source line of the innermost
//! expression that refused, without changing the reason the ledgers key on.
//!
//! The innermost span is kept by clearing on every successful `lower` and
//! setting only when unset on the way out of a failing one: a decline that a
//! caller recovered from is erased by the next expression that lowers, so the
//! span that survives belongs to the error that actually propagated.

use almide_base::span::Span;
use std::cell::{Cell, RefCell};

thread_local! {
    static PENDING: Cell<Option<Span>> = const { Cell::new(None) };
    static SITE: RefCell<Option<DeclineSite>> = const { RefCell::new(None) };
}

/// The function a structural-leg wall came from, and the line in its source
/// file of the expression that refused (when the IR node carried a span).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclineSite {
    /// The qualified name (`cli.dispatch_ok`), or `<lambda> in <owner>`.
    pub function: String,
    /// The module the function is declared in (`None` = the entry file).
    pub module: Option<String>,
    pub line: Option<usize>,
}

impl std::fmt::Display for DeclineSite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "fn `{}`", self.function)?;
        match (&self.module, self.line) {
            (Some(m), Some(l)) => write!(f, " (module `{m}`, line {l})"),
            (Some(m), None) => write!(f, " (module `{m}`)"),
            (None, Some(l)) => write!(f, " (entry file, line {l})"),
            (None, None) => Ok(()),
        }
    }
}

/// Record the outcome of lowering one expression.
pub(crate) fn note<T, E>(r: &Result<T, E>, span: Option<Span>) {
    PENDING.with(|p| match r {
        Ok(_) => p.set(None),
        Err(_) => {
            if p.get().is_none() {
                p.set(span);
            }
        }
    });
}

/// A function body starts: no span from an earlier body may leak into it.
pub(crate) fn reset_pending() {
    PENDING.with(|p| p.set(None));
}

/// The span of the innermost expression whose decline propagated out.
pub(crate) fn take_pending() -> Option<Span> {
    PENDING.with(|p| p.take())
}

/// The emit pass chose which reachable body walls the program: remember it.
pub(crate) fn set(site: Option<DeclineSite>) {
    SITE.with(|s| *s.borrow_mut() = site);
}

/// Where the last structural-leg wall on this thread came from. `None` when
/// the last emit succeeded, or when the wall was not a function body's.
pub fn last() -> Option<DeclineSite> {
    SITE.with(|s| s.borrow().clone())
}
