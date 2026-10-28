//! main's top-let prelude: storing each initializer into its global, the
//! witness gate over the prelude, and the live-heap measurement's release —
//! split from func.rs for the file budget.

use crate::emitter::Emitter;
use crate::*;

use std::collections::HashSet;

use almide_ir::VarId;
use wasm_encoder::ValType;

use super::Ctx;

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

/// The same measurement, after `main`'s exit plan: the runtime's own blocks
/// that are live by design, each kept for the program's life once made —
/// the keyed-lookup side table (map_index.rs, global `G_MAPIDX`, allocated
/// at the first indexed lookup) and the line-buffer arena (runtime_line.rs,
/// a build that outgrew the room relocates it to a heap block, reached as
/// `G_LINE_START + G_LINE_DELTA`). Freed LAST, once every map / set the
/// frame and the top-lets held (and the index block the table names for
/// each) is gone.
pub(super) fn release_runtime_blocks_for_measurement(em: &mut Emitter<'_>, in_main: bool) {
    if !in_main || !crate::alloc_count::releases_top_lets() {
        return;
    }
    let mut i = em.f.instructions();
    i.global_get(crate::G_MAPIDX).if_(wasm_encoder::BlockType::Empty);
    i.global_get(crate::G_MAPIDX).call(crate::F_FREE);
    i.i32_const(0).global_set(crate::G_MAPIDX);
    i.end();
    i.global_get(crate::G_LINE_DELTA).if_(wasm_encoder::BlockType::Empty);
    i.global_get(crate::G_LINE_START)
        .global_get(crate::G_LINE_DELTA)
        .i32_add()
        .i32_const(almide_layout::PAYLOAD as i32)
        .i32_sub()
        .call(crate::F_FREE);
    i.end();
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

/// #1627: does this module-space initializer carry inner binds? Those
/// binds index the MODULE's VarTable, so the value cannot lower inline in
/// the calling frame — it needs its own.
pub(super) fn initializer_needs_own_frame(il: &crate::InitLet, ctx: &Ctx) -> Result<bool, EmitError> {
    if il.space == 0 {
        return Ok(false);
    }
    let mut probe: Vec<(VarId, SliceTy)> = Vec::new();
    let mut probe_seen: HashSet<VarId> = HashSet::new();
    collect_binds(&il.tl.value, &mut probe, &mut probe_seen, ctx.types)?;
    Ok(!probe.is_empty())
}

/// #1627: register the binds-carrying module initializer as a synthetic
/// zero-param entry on the lifted pipeline — lowered in its own frame
/// with the module's space and name — and call_indirect it exactly as a
/// closure would (env 0: nothing to capture; no hop charge: native
/// charges nothing for reaching a top-let's value).
pub(super) fn emit_modinit_call(
    em: &mut crate::emitter::Emitter<'_>,
    il: &crate::InitLet,
    declared: SliceTy,
    site_name: String,
) {
    let j = em.work.register_lambda(crate::LiftedLambda {
        site_name: Some(site_name),
        params: Vec::new(),
        ret: Some(declared),
        effect_raw: None,
        body: il.tl.value.clone(),
        captures: Vec::new(),
        var_space: il.space,
        cur_module: il.module.clone(),
        charge_hop: false,
    });
    let slot = em.work.slot(crate::TableEntry::Lambda(j));
    let ti = em.work.itype(vec![ValType::I32], Some(declared.val_type()));
    em.f.instructions().i32_const(0); // env: unused
    em.f.instructions().i32_const(slot as i32);
    em.f.instructions().call_indirect(0, ti);
}

/// The name a wall in top-let `il`'s initializer is reported under
/// (decline_site.rs): module-qualified, as program functions are.
pub(super) fn top_let_site_name(il: &crate::InitLet, ctx: &Ctx) -> String {
    let name = (ctx.var_name)(il.space, il.tl.var).unwrap_or_else(|| "<top-let>".to_string());
    match &il.module {
        Some(m) => format!("{m}.{name}"),
        None => name,
    }
}
