//! The region window's arena mark on the certified native render (#4006 C2).
//!
//! `almide_codegen::pass_region_window_native` rewrites a discovered
//! `consume(produce(scalars))` site onto `__rgn_` twins and a window fn marked
//! `REGION_WINDOW_ATTR`. Here that fn's MIR is bracketed: `RegionSave` at
//! entry opens the arena mark, `RegionRestore` after its last op rewinds the
//! arena to it. The mark is an object of the ownership certificate — the save
//! is its `i`, the restore its `d` — so the kernel-proven balance certifies
//! that every window is rewound exactly once. The twin nodes the pair builds
//! keep their own per-node stream; their `Drop` renders as nothing (a twin is
//! `Copy`) and the rewind reclaims them, which is sound because no handle
//! outlives the window (the region-pure verdict, named interim trust in
//! docs/contracts/proven-vs-trusted.md until C1 retires it).

use std::collections::BTreeMap;

use crate::lower::LowerError;
use crate::{MirFunction, Op, PrimKind, ValueId};

/// The window fns `ir` carries: name → the site's span (the window fn's body
/// is the site's call, rewritten).
pub(crate) fn window_fns(ir: &almide_ir::IrProgram) -> BTreeMap<String, Option<almide_ir::Span>> {
    ir.functions.iter().filter(|f| f.is_region_window()).map(|f| (f.name.as_str().to_string(), f.body.span)).collect()
}

/// Bracket every window fn of `functions` with the arena mark's save and
/// restore. A window fn is straight-line (one call expression over scalar
/// params), so its last op is its end; an early exit would skip the restore
/// and walls instead.
pub(crate) fn bracket_windows(
    functions: &mut [MirFunction],
    windows: &BTreeMap<String, Option<almide_ir::Span>>,
) -> Result<(), LowerError> {
    for f in functions.iter_mut() {
        let Some(span) = windows.get(&f.name) else { continue };
        if f.ops.iter().any(|op| matches!(op, Op::Return { .. } | Op::LoopStart | Op::IfThen { .. })) {
            return Err(LowerError::at(*span, format!("native: region window `{}` is not straight-line MIR", f.name)));
        }
        let mark = fresh_value(f);
        f.ops.insert(0, Op::Prim { kind: PrimKind::RegionSave, dst: Some(mark), args: Vec::new() });
        // Op indices moved by one.
        f.call_moves = f.call_moves.iter().map(|i| i + 1).collect();
        f.call_mut_borrows = f.call_mut_borrows.iter().map(|&(i, k)| (i + 1, k)).collect();
        f.ops.push(Op::Prim { kind: PrimKind::RegionRestore, dst: None, args: vec![mark] });
    }
    Ok(())
}

/// A value id no param or op of `f` uses.
fn fresh_value(f: &MirFunction) -> ValueId {
    let w = crate::certificate::name_witness(f);
    let top = w.defined.iter().chain(&w.used).map(|v| v.0).max().map_or(0, |m| m + 1);
    ValueId(top)
}
