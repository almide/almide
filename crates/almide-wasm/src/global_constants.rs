//! Literal immutable globals are hints for guarded integer division, not
//! substitutions. Exported functions may run before main initializes the
//! globals; the emitter must compare the actual value before taking a
//! constant-divisor fast path. Zero and every other value retain the old
//! runtime guards and operation, including before initialization.
use almide_ir::top_let_storage::{GVar, build_global_tables_spaced};
use almide_ir::{IrExprKind, IrProgram};
use almide_types::types::Ty;
use std::collections::HashMap;

pub(crate) fn literal_divisors(ir: &IrProgram) -> HashMap<GVar, i64> {
    let mut values = HashMap::new();
    for (space, lets) in std::iter::once((0, &ir.top_lets)).chain(
        ir.modules
            .iter()
            .enumerate()
            .map(|(i, m)| (i as u32 + 1, &m.top_lets)),
    ) {
        for tl in lets {
            if !tl.mutable
                && matches!(tl.ty, Ty::Int)
                && let IrExprKind::LitInt { value } = tl.value.kind
                && value >= 2
                && (value as u64).is_power_of_two()
            {
                values.insert((space, tl.var), value);
            }
        }
    }
    if values.is_empty() {
        return values;
    }
    let (_, aliases, _) = build_global_tables_spaced(ir);
    for (use_site, decl) in aliases {
        if let Some(value) = values.get(&decl).copied() {
            values.insert(use_site, value);
        }
    }
    values
}
