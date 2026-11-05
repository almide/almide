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

/// The merges a moved slot receives (#4006 class P): a region's merge dst
/// that feeds a slot a call-site move hands over, and, inward, every arm
/// value of such a region that is itself a nested region's merge — the
/// `match` arms' chain of merges into `out`. Merge dst → slot.
fn merge_slot_chain(
    func: &MirFunction,
    feeder_to_slot: &BTreeMap<ValueId, ValueId>,
    moved_slots: &BTreeSet<ValueId>,
) -> BTreeMap<ValueId, ValueId> {
    let mut arm_vals: BTreeMap<ValueId, Vec<ValueId>> = BTreeMap::new();
    let mut merges: BTreeSet<ValueId> = BTreeSet::new();
    let mut stack: Vec<Option<ValueId>> = Vec::new();
    for op in &func.ops {
        match op {
            Op::IfThen { dst, .. } => {
                merges.extend(*dst);
                stack.push(*dst);
            }
            Op::Else { val } | Op::EndIf { val } => {
                if let (Some(Some(d)), Some(v)) = (stack.last(), val) {
                    arm_vals.entry(*d).or_default().push(*v);
                }
                if matches!(op, Op::EndIf { .. }) {
                    stack.pop();
                }
            }
            _ => {}
        }
    }
    let mut chain: BTreeMap<ValueId, ValueId> = feeder_to_slot
        .iter()
        .filter(|(d, s)| merges.contains(d) && moved_slots.contains(s))
        .map(|(d, s)| (*d, *s))
        .collect();
    let mut work: Vec<ValueId> = chain.keys().copied().collect();
    while let Some(d) = work.pop() {
        let slot = chain[&d];
        for v in arm_vals.get(&d).into_iter().flatten() {
            if merges.contains(v) && !chain.contains_key(v) {
                chain.insert(*v, slot);
                work.push(*v);
            }
        }
    }
    chain
}

/// The value a DROP-family op releases (every recursive drop variant is the
/// same single `d` on its object), or `None` for any other op.
fn drop_family_value(op: &Op) -> Option<ValueId> {
    match op {
        Op::Drop { v }
        | Op::DropListStr { v }
        | Op::DropValue { v }
        | Op::DropListValue { v }
        | Op::DropListStrValue { v }
        | Op::DropListStrStr { v }
        | Op::DropListIntStr { v }
        | Op::DropListStrInt { v }
        | Op::DropResultListValue { v }
        | Op::DropResultValue { v }
        | Op::DropResultStrInt { v }
        | Op::DropResultValueInt { v }
        | Op::DropResultListValueInt { v }
        | Op::DropResultListStrInt { v }
        | Op::DropResultListStr { v }
        | Op::DropListListStr { v }
        | Op::DropVariant { v, .. }
        | Op::DropWrapperRec { v, .. } => Some(*v),
        // The region window's close (#4006 C2): the arena rewind releases the
        // window's mark — the one `d` its `RegionSave` opened.
        Op::Prim { kind: PrimKind::RegionRestore, args, .. } => args.first().copied(),
        _ => None,
    }
}

/// The dst of an ALLOCATOR-CLASS prim — one that returns a fresh OWNED heap
/// value the caller must balance, exactly like `Alloc`. Without the `i` these
/// emit, the heap result would be an unbacked object the cert never opens and
/// the verify_ownership/cert agreement breaks for the owning stdlib body.
/// Every arg of every prim here is BORROWED (no cert event). The roster and
/// what each allocates:
/// `args_get_list` → `List[String]` (argv[1..]); `env_get` → `Option[String]`
/// (a 0/1-slot block owning the value String when some); `read_text_file` →
/// `Result[String, String]` (cap-as-tag, owns one payload String); `read_dir`
/// → `Result[List[String], String]` (balanced by the recursive
/// `DropResultListStr`); `write_text_file` / `make_dir` / `remove_all` →
/// `Result[Unit, String]` (Ok carries NO payload, Err owns one message
/// String); `read_line` / `read_n_bytes` → a fresh canonical `String` (flat
/// `Drop` — a String owns no nested handles); `region_save` → the region
/// window's arena mark (#4006 C2), released once by its `RegionRestore`.
fn alloc_class_prim_dst(op: &Op) -> Option<ValueId> {
    match op {
        // A native Result carrier birth (#4349): the mirror's alloc-class rule.
        Op::Prim { kind, dst: Some(d), .. } if kind.births_native_carrier() => Some(*d),
        Op::Prim {
            kind:
                PrimKind::ArgsGetList
                | PrimKind::EnvGet
                | PrimKind::ReadTextFile
                | PrimKind::ReadBytesFile
                | PrimKind::ReadDir
                | PrimKind::WriteTextFile
                | PrimKind::MakeDir
                | PrimKind::RemoveAll
                | PrimKind::Rename
                | PrimKind::ReadLine
                | PrimKind::ReadNBytes
                | PrimKind::RegionSave,
            dst: Some(d),
            ..
        } => Some(*d),
        _ => None,
    }
}

/// The dst of a call op returning a HEAP result (any call form), or `None`.
fn heap_call_dst(op: &Op) -> Option<ValueId> {
    match op {
        Op::Call { dst: Some(d), result: Some(r), .. }
        | Op::CallFn { dst: Some(d), result: Some(r), .. }
        | Op::CallImport { dst: Some(d), result: Some(r), .. }
        | Op::CallIndirect { dst: Some(d), result: Some(r), .. }
            if r.is_heap() =>
        {
            Some(*d)
        }
        _ => None,
    }
}
