//! Literal immutable Int globals as known divisors. Such a global holds
//! exactly two values over a module's life: zero until main's prelude
//! stores it (an exported function may run before that), and its literal
//! ever after. So a division by one tests the global for zero — the
//! pre-initialization abort, unchanged — and otherwise divides by the
//! literal itself, with the literal's strength reductions and without the
//! overflow guard a nonzero non-minus-one divisor makes dead. The literals
//! 0, -1 and MIN stay on the guarded runtime path.
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
                && !matches!(value, 0 | -1 | i64::MIN)
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
