//! The witness gate's rules for the values a route BUILDS from its operands
//! (#2755 / #2758), split from witness_gate.rs for the file budget.

use almide_ir::{IrExpr, IrExprKind};

use super::{value_subset, Why};

/// #2755: the values a route BUILDS from its operands. `[]` of a map is a
/// fresh empty block; `["k": v, …]` lowers as `map.from_list` over a fresh
/// pairs list (emitter_values.rs) — a borrowed temporary of the arm (`id`)
/// whose tuple slots are `witness_store`s; a range is a fresh Int list over
/// its bounds (ranges.rs), whose overflow abort is a recorded terminal; `r?`
/// converts its carrier (data.rs `witness_to_option`).
///
/// #2758: `{ ...b, f: v }` (data.rs `lower_spread_record`) is a fresh block
/// like a record literal, copied from a BOUND base — a Var (its read is the
/// Var route's `b` probe) or a slot view of one: the copy helper takes a
/// credit on every handle slot and the overwritten slot's is released right
/// back, the helper's interior bookkeeping exactly as for a field
/// assignment's copy (list_mut.rs `field_assign_with`); each field store is
/// `witness_store` after the share guard. A PRODUCED base is an owned
/// temporary the copy reads and the site releases right after it (#3373,
/// `id`); a borrowed base that is not a view of a bound one declines at
/// emission (`SpreadRecord-base:borrowed`).
pub(super) fn built_value_subset(e: &IrExpr) -> Option<Why> {
    match &e.kind {
        IrExprKind::MapLiteral { entries } => entries
            .iter()
            .find_map(|(k, v)| value_subset(k).or_else(|| value_subset(v)).map(|w| w.inside("map-entry"))),
        IrExprKind::Range { start, end, .. } => {
            value_subset(start).or_else(|| value_subset(end)).map(|w| w.inside("range-bound"))
        }
        IrExprKind::ToOption { expr } => value_subset(expr).map(|w| w.inside("to-option")),
        IrExprKind::SpreadRecord { base, fields } => value_subset(base)
            .map(|w| w.inside("spread-base"))
            .or_else(|| fields.iter().find_map(|(_, x)| value_subset(x).map(|w| w.inside("field")))),
        _ => None,
    }
}
