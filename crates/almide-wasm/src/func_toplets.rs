//! main's top-let prelude: storing each initializer into its global, the
//! witness gate over the prelude, and the live-heap measurement's release —
//! split from func.rs for the file budget.

use crate::emitter::Emitter;
use crate::*;

use super::{initializer_needs_own_frame, Ctx};

/// The live-heap measurement (`alloc_count`, armed builds only): drop every
/// droppable top-let global at the end of `main`, so a value that is live by
/// design until exit is not counted as a leak. A no-op for a shipped module.
pub(super) fn release_top_lets_for_measurement(em: &mut Emitter<'_>, in_main: bool, top_lets: &[crate::InitLet], ctx: &Ctx) {
    if !in_main || !crate::alloc_count::releases_top_lets() {
        return;
    }
    for il in top_lets {
        if let Some(&(gidx, declared)) = ctx.globals.get(&(il.space, il.tl.var))
            && em.rc_droppable(declared)
        {
            let dec = em.dec_fn_of(declared);
            em.f.instructions().global_get(gidx).call(dec);
        }
    }
}

/// A top-let's lowered value becomes its global's. A List / Map / Set /
/// Bytes global holds its own COPY of the block; a fresh initializer (a
/// literal, a call result) was the copy's source only, and is released
/// after it — it stayed live until exit, one block per such top-let (#2758).
pub(super) fn store_top_let(em: &mut Emitter<'_>, owned: bool, declared: SliceTy, gidx: u32) -> Result<(), EmitError> {
    let copied = matches!(declared, SliceTy::List(_) | SliceTy::Map(..) | SliceTy::Set(_) | SliceTy::Scalar(Scalar::Bytes));
    em.witness_top_let(declared, owned, copied);
    if copied {
        let copy = em.copy_fn_of(declared);
        if owned {
            let h = em.hold_i32()?;
            let dec = em.dec_fn_of(declared);
            em.f.instructions().local_tee(h).call(copy).local_get(h).call(dec);
            em.release_i32();
        } else {
            em.f.instructions().call(copy);
        }
    } else if !owned && em.rc_droppable(declared) {
        // #2992: the global owns its occupant (a reassign releases it), so
        // an initializer that BORROWS — another global, a pool static —
        // takes the credit here, as a Bind would.
        em.rc_inc_top();
    }
    em.f.instructions().global_set(gidx);
    Ok(())
}

/// #2758: the top-let prelude's gate — an initializer with its own frame
/// hands an owned result over; an inline one must be an admissible value.
pub(super) fn top_lets_gate(top_lets: &[crate::InitLet], ctx: &Ctx) -> Option<String> {
    top_lets.iter().find_map(|il| match initializer_needs_own_frame(il, ctx) {
        Ok(true) => None,
        Ok(false) => crate::witness::top_let_subset(&il.tl.value),
        Err(_) => Some("top-let:unlowered".to_string()),
    })
}
