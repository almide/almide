//! The MIR loop fusion uses the same callback analysis and sequencing rule
//! as the nanopass emitter. Unknown callees never establish purity.
use almide_codegen::pass_stream_fusion::Purity;
use almide_ir::{IrExpr, IrProgram, fusion::{Stage, stages_mergeable}};
use std::cell::RefCell;

thread_local! {
    static FACTS: RefCell<Option<Purity>> = const { RefCell::new(None) };
}
pub(crate) struct Scope(Option<Purity>);
impl Scope {
    pub(crate) fn enter(ir: &IrProgram) -> Self {
        Self(FACTS.with(|s| s.replace(Some(Purity::of(ir)))))
    }
}
impl Drop for Scope {
    fn drop(&mut self) { FACTS.with(|s| s.replace(self.0.take())); }
}

pub(crate) fn mergeable(inner: &IrExpr, outer: &IrExpr, init: Option<&IrExpr>, short: bool) -> bool {
    FACTS.with(|s| {
        let facts = s.borrow();
        let Some(facts) = facts.as_ref() else { return false; };
        stages_mergeable(&[
            Stage { pure: facts.pure(inner), total: facts.total(inner), short_circuits: false },
            Stage {
                pure: facts.pure(outer) && init.is_none_or(|e| facts.pure(e)),
                total: facts.total(outer) && init.is_none_or(|e| facts.total(e)),
                short_circuits: short,
            },
        ])
    })
}
