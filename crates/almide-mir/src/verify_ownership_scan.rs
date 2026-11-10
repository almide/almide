pub fn verify_ownership(func: &MirFunction) -> Result<(), Vec<Violation>> {
    // Handle ≠ object. Each known heap HANDLE (ValueId) maps to its OBJECT (the
    // `Alloc`'d representative ValueId); the refcount is per OBJECT. A handle is
    // also tracked LIVE/dead, so a use of a handle after its own drop/consume is
    // caught even when the object lives on through a sibling handle.
    let mut object_of: BTreeMap<ValueId, ValueId> = BTreeMap::new();
    let rc: BTreeMap<ValueId, i64> = BTreeMap::new(); // keyed by object — OUR (callee's) owned refs
    let mut dead: BTreeMap<ValueId, bool> = BTreeMap::new(); // keyed by handle
    let violations: Vec<Violation> = Vec::new();

    let mut borrowed: BTreeSet<ValueId> = BTreeSet::new();
    init_borrowed_params(func, &mut object_of, &mut dead, &mut borrowed);
    // A MOVE param (#4006 class P) enters holding the reference its caller
    // handed over: owned at 1, not borrowed.
    let mut rc = rc;
    for p in &func.owned_params {
        if borrowed.remove(p) {
            rc.insert(*p, 1);
        }
    }

    // BRANCH JOIN (mirrors the proven checker's `CBranch` rule): each arm of an
    // `IfThen`/`Else`/`EndIf` runs from the SAME entry state, and the arms must
    // AGREE on every object's leaving count (the net may be nonzero — a
    // heap-result branch nets +1 through either arm). Folding the arms FLAT
    // (the old model) counted BOTH arms' events, silently accepting cross-arm
    // compensation — a `Consume` in one arm "balancing" the other arm's missing
    // release, i.e. a path-dependent leak/double-free.

    // Decomposed (#781, cog 140): the per-op transition lives in
    // `OwnershipScan::step`; the maps moved into the scan struct verbatim.
    let mut scan = OwnershipScan {
        object_of,
        rc,
        dead,
        borrowed,
        branches: Vec::new(),
        violations,
        diverged: false,
        addr_of: BTreeMap::new(),
        child_parent: BTreeMap::new(),
        out_of_path: BTreeSet::new(),
        rebound: BTreeSet::new(),
        rebound_frames: Vec::new(),
        carriers: BTreeSet::new(),
        consts: BTreeMap::new(),
        slot_roots: BTreeMap::new(),
        carried: BTreeMap::new(),
        owning_merges: crate::certificate::merge_dsts_holding_a_reference(func),
        slot_objects: 0,
        cow_dups: crate::certificate::cow_dup_dsts(func),
    };
    for (i, op) in func.ops.iter().enumerate() {
        scan.step(i, op);
        // The proven CBranchAbort rule checks every operation up to process
        // termination, then continues only the other arm. It imposes no
        // return-time release obligation on a process that has ended.
        if crate::mir_wellformed::aborts_before_arm_end(&func.ops, i) {
            scan.diverged = true;
        }
    }
    let OwnershipScan { object_of, mut rc, mut dead, borrowed, mut violations, diverged, .. } =
        scan;

    // A TOP-LEVEL `Op::Return` already took the whole boundary obligation at
    // its own op index (and mir_wellformed forbids ops after it), so the tail
    // is unreachable — running the boundary phases again would double-release
    // the returned value and re-report the same leaks.
    if !diverged {
        check_return_release(func, &object_of, &mut rc, &mut dead, &borrowed, &mut violations);
        check_leaks(func, &rc, &mut violations);
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

