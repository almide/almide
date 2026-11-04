// ── merge-credit count of certificate_b.rs, include!-spliced back at module level ──
//
// A pure code move: this file continues its parent verbatim (its last item,
// ahead of certificate_b_tail.rs). The split exists only so the parent stays
// under the 800-line ceiling the codopsy gate holds this crate to; there is no
// boundary of meaning here, and `include!` at module level is the one splice
// Rust allows (an impl-item position rejects it).

/// The number of `i` events [`ownership_certificate`] credits to branch-MERGE
/// dsts: the RELEASED merges (the arm's moved-in reference, later Consumed/
/// Dropped/val-flowed/returned) plus the slot-FEEDER merges (`acc = if c then
/// acc + [x] else acc`, whose `i` routes into the loop-carried slot stream).
/// Both are backed by the arm value's real producer — the merge is a reference
/// changing hands (the wasm merge local.set), not a synthetic `+1`. classify's
/// borrow-by-default backing gate uses THIS count so the gate and the emission
/// stay in lockstep by construction (one credit per IfThen op occurrence,
/// mirroring `CertScan::step`'s pre-region emission exactly).
pub fn merge_dst_i_credits(func: &MirFunction) -> usize {
    let (feeder_to_slot, _, _) = loop_carried_slots(func);
    let mut merge_dsts: std::collections::HashSet<crate::ValueId> = std::collections::HashSet::new();
    let mut released: std::collections::HashSet<crate::ValueId> = std::collections::HashSet::new();
    for op in &func.ops {
        match op {
            Op::IfThen { dst: Some(d), .. } => {
                merge_dsts.insert(*d);
            }
            Op::Consume { v }
            | Op::Drop { v }
            | Op::DropListStr { v }
            // A RECORD/VARIANT-typed merge dst releases through the TYPED
            // recursive drop (the #1287 record-merge seeding emits DropVariant,
            // not Drop) — without this arm the dst never enters the released
            // set, its IfThen `i` credit is skipped, and the DropVariant certs
            // as a bare `d` the kernel checker rejects (unowned Dec).
            | Op::DropVariant { v, .. } => {
                if merge_dsts.contains(v) {
                    released.insert(*v);
                }
            }
            Op::Else { val: Some(v) } | Op::EndIf { val: Some(v) } if merge_dsts.contains(v) => {
                released.insert(*v);
            }
            _ => {}
        }
    }
    if let Some(r) = func.ret {
        if merge_dsts.contains(&r) {
            released.insert(r);
        }
    }
    // HEAP merges only — the exact filter `ownership_certificate_released_merge_dsts`
    // applies (a scalar merge carries no reference; crediting it would desync
    // this count from the certificate's `i` events).
    let heap_objs = loop_carried_slots_heap_objs(func);
    released.retain(|d| heap_objs.contains(d));
    func.ops
        .iter()
        .filter(|op| match op {
            Op::IfThen { dst: Some(d), .. } => {
                // Single-condition decisions (MC/DC ledger): || as if/else.
                if feeder_to_slot.contains_key(d) {
                    true
                } else {
                    released.contains(d)
                }
            }
            _ => false,
        })
        .count()
}
