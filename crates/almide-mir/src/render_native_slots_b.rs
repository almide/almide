// ── tail of render_native_slots.rs, include!-spliced back at module level ──
//
// A pure code move: this file continues its parent verbatim. The split exists
// only so the parent stays under the 800-line ceiling the codopsy gate holds
// this crate to; there is no boundary of meaning here, and `include!` at module
// level is the one splice Rust allows (an impl-item position rejects it).

/// One op of a block's straight-line build run after its alloc.
enum BuildStep<'o> {
    /// Not a store into the block: whether the run goes on past it.
    Go(bool),
    /// A store into the block (its args): the caller judges it.
    Store(&'o [ValueId]),
}

/// What `later` is to the build run of block `b`: control flow ends it, the
/// block's own address arithmetic continues it, a store into it is the
/// caller's to judge, and any other op continues it unless it reads the block.
fn build_step<'o>(later: &'o Op, b: &ValueId, base_of: &BTreeMap<ValueId, (ValueId, Option<i64>)>) -> BuildStep<'o> {
    let rooted = |v: &ValueId, b: ValueId| base_of.get(v).is_some_and(|(r, _)| *r == b);
    match later {
        Op::IfThen { .. } | Op::Else { .. } | Op::EndIf { .. } | Op::LoopStart | Op::LoopEnd | Op::LoopBreakUnless { .. } | Op::Return { .. } => BuildStep::Go(false),
        Op::Prim { kind: PrimKind::Handle, args, .. } if args.first() == Some(b) => BuildStep::Go(true),
        Op::IntBinOp { op: crate::IntOp::Add, a, b: y, .. } if rooted(a, *b) || rooted(y, *b) => BuildStep::Go(true),
        Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } if args.first().is_some_and(|a| rooted(a, *b)) => {
            BuildStep::Store(args.as_slice())
        }
        other => {
            let mut reads = Vec::new();
            crate::mir_ops::op_reads(other, &mut reads);
            BuildStep::Go(!reads.iter().any(|v| v == b || rooted(v, *b)))
        }
    }
}

/// The heap slot reads of an owned block that can MOVE the value out instead
/// of copying it: outside every loop, and after the read the block is only
/// dropped, addressed, or read at its other slots. Nothing reads the slot
/// again, so taking it leaves every later observation as the copy would.
/// The second set holds the loads after which nothing reads the block at all
/// (no other slot either): a carrier whose payload moves out reads as empty
/// after it, so only those may move a carrier's payload.
pub(super) fn movable_loads(
    func: &MirFunction,
    base_of: &BTreeMap<ValueId, (ValueId, Option<i64>)>,
) -> (BTreeSet<ValueId>, BTreeSet<ValueId>, BTreeSet<ValueId>) {
    let rooted = |v: &ValueId, b: ValueId| base_of.get(v).is_some_and(|(r, _)| *r == b);
    let mut out = BTreeSet::new();
    let mut last = BTreeSet::new();
    // The movable reads that are the first read of their slot since the
    // block was last bound: no other read of that slot of the same block
    // value precedes them, so none can be rendered later than them (an
    // interpolation chain reads its pieces at its last step).
    let mut first = BTreeSet::new();
    let mut since_bind: BTreeSet<(ValueId, usize)> = BTreeSet::new();
    let consts: BTreeMap<ValueId, i64> = func
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::ConstInt { dst, value } => Some((*dst, *value)),
            _ => None,
        })
        .collect();
    // The open loops' starts, and where each value is defined.
    let mut loops: Vec<usize> = Vec::new();
    let mut def_at: BTreeMap<ValueId, usize> = BTreeMap::new();
    // The earlier reads of each (block, slot): a borrowed one still read
    // after a later read would see the slot that read moved out.
    let mut seen: BTreeMap<(ValueId, usize), Vec<ValueId>> = BTreeMap::new();
    for (i, op) in func.ops.iter().enumerate() {
        match op {
            Op::LoopStart => loops.push(i),
            Op::LoopEnd => {
                loops.pop();
            }
            _ => {}
        }
        if let Some(v) = crate::mir_ops::defined_value(op) {
            def_at.entry(v).or_insert(i);
        }
        if let Op::SetLocal { local, .. } = op {
            since_bind.retain(|(b, _)| b != local);
        }
        let Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args } = op else { continue };
        let Some(&(b, Some(off))) = args.first().and_then(|a| base_of.get(a)) else { continue };
        let Some(k) = slot_of(Some(off)) else { continue };
        let earlier = seen.entry((b, k)).or_default().clone();
        seen.get_mut(&(b, k)).expect("just inserted").push(*d);
        let first_read = since_bind.insert((b, k));
        // Inside a loop the block must be born in the same iteration (defined
        // after the innermost loop's start, never rebound): a block from
        // before the loop would be read again by the next iteration.
        // Or the block is a loop slot every path from the read to the back
        // edge rebinds (`b = { ...b, f: g(b.f) }`): the next iteration reads
        // the new block, never this one's slot (#4006, no-perf-loss).
        // The scan then stops at that rebind: what follows reads the new block.
        let mut rebound = false;
        let mut end = func.ops.len();
        if let Some(&ls) = loops.last() {
            let fresh = def_at.get(&b).is_some_and(|d| *d > ls)
                && !func.ops.iter().any(|o| matches!(o, Op::SetLocal { local, .. } if *local == b));
            if !fresh {
                let Some(at) = rebound_in_block(func, i, b) else { continue };
                rebound = true;
                end = at;
            }
        }
        // A scan that runs to the end of the fn: the fn's result is read whole
        // after its last op (a `mut` param's write-back included), so no slot
        // of it may be taken.
        if !rebound && func.ret == Some(b) {
            continue;
        }
        let mut movable = true;
        let mut other_slot = false;
        for later in &func.ops[i + 1..end] {
            let fine = match later {
                Op::Drop { v } | Op::DropListStr { v } | Op::DropVariant { v, .. } | Op::DropWrapperRec { v, .. } => *v == b,
                // The ownership marks render nothing; an op that hands the
                // block on reads it itself.
                Op::Consume { .. } | Op::Borrow { .. } | Op::MakeUnique { .. } => true,
                // A scalar read of another slot, by its constant index.
                Op::ListGetScalar { list, idx, .. } => {
                    *list == b && consts.get(idx).is_some_and(|x| usize::try_from(*x).is_ok_and(|x| x != k))
                }
                Op::Prim { kind: PrimKind::Handle, args, .. } => args.first() == Some(&b),
                Op::IntBinOp { op: crate::IntOp::Add, a, b: y, .. } => rooted(a, b) || rooted(y, b),
                Op::Prim { kind: PrimKind::Load { .. } | PrimKind::LoadHandle, args, .. } => args.first().is_some_and(|a| {
                    base_of.get(a).is_some_and(|(r, o)| *r == b && o.is_some() && slot_of(*o).is_some_and(|s| s != k))
                }),
                _ => false,
            };
            if fine {
                other_slot |= matches!(later, Op::Prim { kind: PrimKind::Load { .. } | PrimKind::LoadHandle, .. });
                continue;
            }
            let mut reads = Vec::new();
            crate::mir_ops::op_reads(later, &mut reads);
            let earlier_live = !crate::render_native_move::bookkeeping(later) && reads.iter().any(|v| earlier.contains(v));
            if earlier_live || reads.iter().any(|v| *v == b || rooted(v, b)) {
                movable = false;
                break;
            }
        }
        if movable {
            out.insert(*d);
            if !other_slot && !rebound {
                last.insert(*d);
            }
            if first_read {
                first.insert(*d);
            }
        }
    }
    (out, last, first)
}

/// The rebind of `b` from another value that follows the op at `at` in its
/// own block (not inside a nested `if` or loop, before the block ends), with
/// no exit from the loop or the fn on the way: every path from `at` passes
/// it before anything after it runs.
fn rebound_in_block(func: &MirFunction, at: usize, b: ValueId) -> Option<usize> {
    let mut depth = 0usize;
    for (k, op) in func.ops.iter().enumerate().skip(at + 1) {
        match op {
            Op::IfThen { .. } | Op::LoopStart => depth += 1,
            Op::EndIf { .. } | Op::LoopEnd | Op::Else { .. } if depth == 0 => return None,
            Op::EndIf { .. } | Op::LoopEnd => depth -= 1,
            Op::Return { .. } | Op::LoopBreakUnless { .. } => return None,
            Op::SetLocal { local, src } if depth == 0 && *local == b => return (*src != b).then_some(k),
            _ => {}
        }
    }
    None
}

/// The field count of a generated plain struct (not a list, an enum or a carrier).
pub(super) fn struct_width(i: u16) -> Option<usize> {
    if rec::list_elem(i).is_some() || rec::enum_cases(i).is_some() || carrier::is_carrier(i) {
        return None;
    }
    let n = (0..).take_while(|k| rec::rec_field(i, *k).is_some()).count();
    (n > 0).then_some(n)
}

/// The struct blocks the render builds whole, `Struct { f0, … }`, instead of
/// a default value whose fields are then assigned: each field is stored
/// exactly once at a constant slot, in the alloc's own straight-line run
/// (no `if` or loop marker before the last store), and nothing reads the
/// block before then but its own address arithmetic.
/// List blocks filled densely by one loop: allocated with length `n`, then a
/// loop whose counter starts at 0, steps by 1 and exits only on `c < n` failing
/// stores slot `c` exactly once per iteration (at the loop's own level, not
/// under an `if`) and reads nothing else of the block. Every slot is written in
/// order, so the block is reserved and pushed (`list.map`'s defunctionalized
/// fill), as the standard codegen collects it.
pub(super) fn dense_loop_fills(func: &MirFunction, consts: &BTreeMap<ValueId, i64>) -> BTreeSet<ValueId> {
    let ops = &func.ops;
    let mut out = BTreeSet::new();
    // Every `t = a < b` of the fn, by op index: (t, a, b).
    let lts: BTreeMap<usize, (ValueId, ValueId, ValueId)> = ops
        .iter()
        .enumerate()
        .filter_map(|(k, o)| match o {
            Op::IntBinOp { dst, op: crate::IntOp::Lt, a, b } => Some((k, (*dst, *a, *b))),
            _ => None,
        })
        .collect();
    for (i, op) in ops.iter().enumerate() {
        if let Some(b) = dense_fill_at(ops, i, op, consts, &lts) {
            out.insert(b);
        }
    }
    out
}

/// The block `op` (at `i`) allocates, when one loop right after fills it densely.
fn dense_fill_at(ops: &[Op], i: usize, op: &Op, consts: &BTreeMap<ValueId, i64>, lts: &BTreeMap<usize, (ValueId, ValueId, ValueId)>) -> Option<ValueId> {
    let Op::Alloc { dst: b, init: Init::DynList { len } | Init::DynListStr { len }, .. } = op else { return None };
    let (h, j) = handle_up_to_loop(ops, i, *b)?;
    let (c, end) = counted_loop(ops, j, *len, consts, lts)?;
    fills_each_slot_once(&ops[j + 3..end], *b, h, c, consts).then_some(*b)
}

/// Up to the loop after the allocation at `i`: only block `b`'s handle is
/// taken. The handle and the `LoopStart`'s index.
fn handle_up_to_loop(ops: &[Op], i: usize, b: ValueId) -> Option<(ValueId, usize)> {
    let mut j = i + 1;
    let mut handle: Option<ValueId> = None;
    while j < ops.len() && !matches!(ops[j], Op::LoopStart) {
        let mut reads = Vec::new();
        crate::mir_ops::op_reads(&ops[j], &mut reads);
        match &ops[j] {
            Op::Prim { kind: PrimKind::Handle, dst: Some(h), args } if args.as_slice() == [b] => handle = Some(*h),
            Op::IfThen { .. } | Op::Else { .. } | Op::EndIf { .. } | Op::LoopEnd | Op::LoopBreakUnless { .. } | Op::Return { .. } => return None,
            _ if reads.contains(&b) || handle.is_some_and(|h| reads.contains(&h)) => return None,
            _ => {}
        }
        j += 1;
    }
    matches!(ops.get(j), Some(Op::LoopStart)).then_some(())?;
    Some((handle?, j))
}

/// The loop at `j` counts `c` from 0 to `len` by one: `c < len`, break
/// unless, body, `c = c + 1`, LoopEnd. The counter and the `LoopEnd`'s index.
fn counted_loop(ops: &[Op], j: usize, len: ValueId, consts: &BTreeMap<ValueId, i64>, lts: &BTreeMap<usize, (ValueId, ValueId, ValueId)>) -> Option<(ValueId, usize)> {
    let &(t, c, n) = lts.get(&(j + 1))?;
    (n == len).then_some(())?;
    matches!(ops.get(j + 2), Some(Op::LoopBreakUnless { cond }) if *cond == t).then_some(())?;
    ops[..j].iter().any(|o| matches!(o, Op::ConstInt { dst, value: 0 } if *dst == c)).then_some(())?;
    let end = loop_end(ops, j)?;
    let body = &ops[j + 3..end];
    // The step: `c' = c + 1; c = c'` right before LoopEnd.
        let step = body.len() >= 2
            && matches!(body[body.len() - 1], Op::SetLocal { local, src } if local == c && matches!(body[body.len() - 2], Op::IntBinOp { dst, op: crate::IntOp::Add, a, b: one } if dst == src && a == c && consts.get(&one) == Some(&1)));
    step.then_some((c, end))
}

/// The loop `body` stores block `b`'s slot `c` exactly once, at its top
/// level, and reads neither the block nor an address into it otherwise.
fn fills_each_slot_once(body: &[Op], b: ValueId, h: ValueId, c: ValueId, consts: &BTreeMap<ValueId, i64>) -> bool {
    // Addresses into the block: h + const, + c * 8.
    let mut scaled: BTreeSet<ValueId> = BTreeSet::new();
    let mut base12: BTreeSet<ValueId> = BTreeSet::new();
    let mut slot_c: BTreeSet<ValueId> = BTreeSet::new();
    let mut stores = 0usize;
    let mut d = 0i32;
    for o in body {
        match o {
            Op::IfThen { .. } | Op::LoopStart => d += 1,
            Op::EndIf { .. } | Op::LoopEnd => d -= 1,
            _ => {}
        }
        let fine = match o {
            Op::LoopBreakUnless { .. } | Op::Return { .. } => false,
            Op::SetLocal { local, .. } if *local == c => matches!(body.last(), Some(l) if std::ptr::eq(l, o)),
            Op::IntBinOp { dst, op: crate::IntOp::Mul, a, b: w } if *a == c && consts.get(w) == Some(&8) => {
                scaled.insert(*dst);
                true
            },
            Op::IntBinOp { dst, op: crate::IntOp::Add, a, b: k } if *a == h && consts.get(k) == Some(&12) => {
                base12.insert(*dst);
                true
            },
            Op::IntBinOp { dst, op: crate::IntOp::Add, a, b: x } if base12.contains(a) && scaled.contains(x) => {
                slot_c.insert(*dst);
                true
            },
            Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } if args.first().is_some_and(|a| slot_c.contains(a)) => {
                stores += 1;
                d == 0
            }
            other => {
                let mut reads = Vec::new();
                crate::mir_ops::op_reads(other, &mut reads);
                !(reads.contains(&b) || reads.iter().any(|v| *v == h || base12.contains(v) || slot_c.contains(v)))
            }
        };
        if !fine {
            return false;
        }
    }
    stores == 1
}

/// List blocks of a constant length whose every slot is stored once, in slot
/// order, right after the allocation (only address arithmetic between, no
/// read of the block, no control flow): reserved and pushed, rather than
/// filled with defaults first and each default replaced. Slots past the
/// header are the elements, so the k-th store is the k-th push.
pub(super) fn filled_in_order(
    func: &MirFunction,
    base_of: &BTreeMap<ValueId, (ValueId, Option<i64>)>,
    consts: &BTreeMap<ValueId, i64>,
) -> BTreeSet<ValueId> {
    let rooted = |v: &ValueId, b: ValueId| base_of.get(v).is_some_and(|(r, _)| *r == b);
    let mut out = BTreeSet::new();
    for (i, op) in func.ops.iter().enumerate() {
        let Op::Alloc { dst: b, init: Init::DynList { len } | Init::DynListStr { len }, .. } = op else { continue };
        let Some(n) = consts.get(len).and_then(|n| usize::try_from(*n).ok()).filter(|n| *n > 0) else { continue };
        let mut next = 0usize;
        for later in &func.ops[i + 1..] {
            let ok = match build_step(later, b, base_of) {
                BuildStep::Go(ok) => ok,
                BuildStep::Store(args) => {
                    let k = base_of.get(&args[0]).and_then(|(_, o)| slot_of(*o));
                    if k == Some(next) && !rooted(&args[1], *b) {
                        next += 1;
                        true
                    } else {
                        false
                    }
                }
            };
            if !ok {
                break;
            }
            if next == n {
                out.insert(*b);
                break;
            }
        }
    }
    out
}

/// `List[String]` blocks that are a constant: a constant length, every slot
/// stored once in order right after the allocation (`filled_in_order`) with a
/// string literal, and afterwards only read — handed to a floor call's
/// borrowed list param, or released. Such a block is the same value on every
/// run of its function (a top-level `let` table the lowering re-materializes
/// at each use), so the render builds it once, as v0 keeps a top-level `let`.
/// Returns each block's literals in slot order.
pub(super) fn const_str_lists(
    func: &MirFunction,
    base_of: &BTreeMap<ValueId, (ValueId, Option<i64>)>,
    consts: &BTreeMap<ValueId, i64>,
) -> BTreeMap<ValueId, Vec<String>> {
    let filled = filled_in_order(func, base_of, consts);
    let rooted = |v: &ValueId, b: ValueId| base_of.get(v).is_some_and(|(r, _)| *r == b);
    let lits: BTreeMap<ValueId, &String> = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Alloc { dst, init: Init::Str(s), .. } => Some((*dst, s)),
            _ => None,
        })
        .collect();
    let handle_of: BTreeMap<ValueId, ValueId> = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if args.len() == 1 => Some((*d, args[0])),
            _ => None,
        })
        .collect();
    let mut out = BTreeMap::new();
    for (i, op) in func.ops.iter().enumerate() {
        let Op::Alloc { dst: b, init: Init::DynListStr { len }, .. } = op else { continue };
        let Some(n) = consts.get(len).and_then(|n| usize::try_from(*n).ok()).filter(|_| filled.contains(b)) else { continue };
        let mut elems: Vec<String> = Vec::new();
        let mut ok = true;
        for later in &func.ops[i + 1..] {
            let mut reads = Vec::new();
            crate::mir_ops::op_reads(later, &mut reads);
            let touches = reads.iter().any(|v| v == b || rooted(v, *b));
            let fine = match later {
                Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } if args.first().is_some_and(|a| rooted(a, *b)) => {
                    match handle_of.get(&args[1]).and_then(|s| lits.get(s)) {
                        Some(s) if elems.len() < n => {
                            elems.push((*s).clone());
                            true
                        }
                        _ => false,
                    }
                }
                Op::Prim { kind: PrimKind::Handle, args, .. } if args.first() == Some(b) => true,
                Op::IntBinOp { op: crate::IntOp::Add, .. } if touches => true,
                Op::DropListStr { v } if v == b => true,
                Op::CallFn { name, args, .. } if touches => args.iter().enumerate().all(|(k, a)| match a {
                    crate::CallArg::Handle(v) if v == b => crate::native_floor::borrows_list(name, k),
                    crate::CallArg::Handle(v) | crate::CallArg::Scalar(v) => !rooted(v, *b),
                    _ => true,
                }),
                _ => !touches,
            };
            if !fine {
                ok = false;
                break;
            }
        }
        if ok && elems.len() == n {
            out.insert(*b, elems);
        }
    }
    out
}

pub(super) fn built_whole(
    func: &MirFunction,
    alloc_rec: &BTreeMap<ValueId, u16>,
    base_of: &BTreeMap<ValueId, (ValueId, Option<i64>)>,
) -> BTreeSet<ValueId> {
    let rooted = |v: &ValueId, b: ValueId| base_of.get(v).is_some_and(|(r, _)| *r == b);
    let mut out = BTreeSet::new();
    for (i, op) in func.ops.iter().enumerate() {
        let Op::Alloc { dst: b, init: Init::DynList { .. }, .. } = op else { continue };
        let Some(n) = alloc_rec.get(b).and_then(|r| struct_width(*r)) else { continue };
        let mut stored = vec![false; n];
        let mut whole = false;
        for later in &func.ops[i + 1..] {
            let ok = match build_step(later, b, base_of) {
                BuildStep::Go(ok) => ok,
                BuildStep::Store(args) => {
                    match base_of.get(&args[0]).and_then(|(_, o)| slot_of(*o)).filter(|k| *k < n) {
                        Some(k) if !stored[k] && !rooted(&args[1], *b) => {
                            stored[k] = true;
                            true
                        }
                        _ => false,
                    }
                }
            };
            if !ok {
                break;
            }
            if stored.iter().all(|s| *s) {
                whole = true;
                break;
            }
        }
        if whole {
            out.insert(*b);
        }
    }
    out
}

/// Enum blocks built whole: the tag stored first as a constant naming one of
/// the enum's declared cases, then each payload slot of that case stored
/// exactly once, with nothing reading the block in between. The render then
/// builds the case in one expression (`T::C{t}(f1, Box::new(f2), ..)`) where
/// it would otherwise construct the case with default payloads and replace
/// each one (a boxed default allocated and freed per heap slot). The value
/// is the tag and the case's payload width.
pub(super) fn enum_built_whole(
    func: &MirFunction,
    alloc_rec: &BTreeMap<ValueId, u16>,
    base_of: &BTreeMap<ValueId, (ValueId, Option<i64>)>,
    consts: &BTreeMap<ValueId, i64>,
) -> BTreeMap<ValueId, (u32, usize)> {
    let rooted = |v: &ValueId, b: ValueId| base_of.get(v).is_some_and(|(r, _)| *r == b);
    let mut out = BTreeMap::new();
    for (i, op) in func.ops.iter().enumerate() {
        let Op::Alloc { dst: b, init: Init::DynList { .. }, .. } = op else { continue };
        let Some(cases) = alloc_rec.get(b).and_then(|r| rec::enum_cases(*r)) else { continue };
        let mut case: Option<(u32, Vec<bool>)> = None;
        let mut whole = false;
        for later in &func.ops[i + 1..] {
            let ok = match build_step(later, b, base_of) {
                BuildStep::Go(ok) => ok,
                BuildStep::Store(args) => {
                    let k = base_of.get(&args[0]).and_then(|(_, o)| slot_of(*o));
                    enum_store(&mut case, k, consts.get(&args[1]).copied(), &cases, rooted(&args[1], *b))
                }
            };
            if !ok {
                break;
            }
            if case.as_ref().is_some_and(|(_, s)| s.iter().all(|x| *x)) {
                whole = true;
                break;
            }
        }
        if let (true, Some((t, s))) = (whole, case) {
            out.insert(*b, (t, s.len()));
        }
    }
    out
}

/// One-element string lists that exist only to be appended (#4006, the heap
/// twin of `render_native_append::list_push`): `acc + [x]` lowers to a
/// `DynListStr` block of length 1, its slot 0 stored once, then the
/// self-append rebind `d = acc + t; drop acc; acc = d`. The render keeps the
/// element in a local and pushes it (`acc.push(e)`), where it would allocate
/// a one-element Vec per append and free it after. Nothing else may read the
/// block: between the alloc and the append only its own address arithmetic
/// and the one store name it, and after the append only its drop.
pub(super) fn single_elem_lists(
    func: &MirFunction,
    base_of: &BTreeMap<ValueId, (ValueId, Option<i64>)>,
    consts: &BTreeMap<ValueId, i64>,
) -> BTreeSet<ValueId> {
    single_blocks(func, base_of, consts, false)
}

/// The same one-element, append-only shape over any list block, `DynList`
/// included: the candidates a generated list (a list of records, lists or
/// variants) takes the push for (`AddrState::single_rec`).
pub(super) fn single_elem_blocks(
    func: &MirFunction,
    base_of: &BTreeMap<ValueId, (ValueId, Option<i64>)>,
    consts: &BTreeMap<ValueId, i64>,
) -> BTreeSet<ValueId> {
    single_blocks(func, base_of, consts, true)
}

fn single_blocks(
    func: &MirFunction,
    base_of: &BTreeMap<ValueId, (ValueId, Option<i64>)>,
    consts: &BTreeMap<ValueId, i64>,
    any_list: bool,
) -> BTreeSet<ValueId> {
    let rooted = |v: &ValueId, b: ValueId| base_of.get(v).is_some_and(|(r, _)| *r == b);
    let mut out = BTreeSet::new();
    for (i, op) in func.ops.iter().enumerate() {
        let (b, len) = match op {
            Op::Alloc { dst, init: Init::DynListStr { len }, .. } => (dst, len),
            Op::Alloc { dst, init: Init::DynList { len }, .. } if any_list => (dst, len),
            _ => continue,
        };
        if consts.get(len) != Some(&1) {
            continue;
        }
        let mut stored = false;
        // (the op that takes the element, the first op after it)
        let mut window = None;
        for (j, later) in func.ops.iter().enumerate().skip(i + 1) {
            if let Some(w) = append_window_at(func, j, later, b, base_of) {
                window = stored.then_some(w);
                break;
            }
            let ok = match build_step(later, b, base_of) {
                BuildStep::Go(ok) => ok,
                BuildStep::Store(args) => {
                    let slot0 = base_of.get(&args[0]).and_then(|(_, o)| slot_of(*o)) == Some(0);
                    let fine = slot0 && !stored && !rooted(&args[1], *b) && args[1] != *b;
                    stored = true;
                    fine
                }
            };
            if !ok {
                break;
            }
        }
        let Some((_, after)) = window else { continue };
        // After the window, only bookkeeping names the block.
        let clean_after = func.ops[after..].iter().all(|later| bookkeeping_only(later, b, base_of));
        if clean_after && func.ret != Some(*b) {
            out.insert(*b);
        }
    }
    out
}

/// Loop exits written as a flag (#4006): the tail-recursion loop the lowering
/// emits is `loop { if done != 0 { break; } if base { done = 1 } else { step } }`
/// (`done` a local, its test the loop's first ops). The `SetLocal` that raises
/// the flag is the last thing that arm does before the loop comes round and
/// breaks, so the render can break right there — the value of every local is
/// the same — and the loop reads as the counted loop it is (LLVM folds a
/// counted sum where it keeps a flag loop running). Returns the op indices of
/// those `SetLocal`s.
///
/// Taken when the loop opens with exactly `c = 0; t = (flag == c); break
/// unless t` over a flag nothing else in the loop reads, the `SetLocal`
/// writes a nonzero constant into the flag, and from it to the loop's end the
/// ops are only bookkeeping, the closing of statement `if`s, and other arms
/// (skipped whole).
pub(crate) fn flag_exits(func: &MirFunction) -> BTreeSet<usize> {
    let ops = &func.ops;
    let mut consts: BTreeMap<ValueId, i64> = BTreeMap::new();
    for op in ops {
        if let Op::ConstInt { dst, value } = op {
            consts.insert(*dst, *value);
        }
    }
    let mut out = BTreeSet::new();
    for (l, op) in ops.iter().enumerate() {
        if !matches!(op, Op::LoopStart) {
            continue;
        }
        let Some((flag, t, c)) = flag_header(ops, l) else { continue };
        // The loop's extent.
        let Some(end) = loop_end(ops, l) else { continue };
        // Nothing in the body reads the flag but its own test; the header's
        // values are read nowhere else.
        if !flag_unread(ops, (l, end), flag, (t, c)) {
            continue;
        }
        for i in l + 4..end {
            let Op::SetLocal { local, src } = &ops[i] else { continue };
            if *local != flag || consts.get(src).is_none_or(|v| *v == 0) {
                continue;
            }
            if exits_to_loop_end(ops, i + 1, end) {
                out.insert(i);
            }
        }
    }
    out
}

/// From `from` to the loop end at `end`, control reaches the end doing
/// nothing: bookkeeping, the close of a statement `if` it is in, and the
/// other arm of one (skipped whole, it does not run).
fn exits_to_loop_end(ops: &[Op], from: usize, end: usize) -> bool {
    let mut k = from;
    while k < end {
        match &ops[k] {
            o if crate::render_native_move::bookkeeping(o) => k += 1,
            Op::EndIf { val: None } => k += 1,
            Op::Else { val: None } => {
                // Skip the else arm to its matching EndIf.
                let mut depth = 0usize;
                k += 1;
                loop {
                    if k >= end {
                        return false;
                    }
                    match &ops[k] {
                        Op::IfThen { .. } => depth += 1,
                        Op::EndIf { .. } if depth == 0 => break,
                        Op::EndIf { .. } => depth -= 1,
                        _ => {}
                    }
                    k += 1;
                }
                // ops[k] is that EndIf.
                if !matches!(ops[k], Op::EndIf { val: None }) {
                    return false;
                }
                k += 1;
            }
            _ => return false,
        }
    }
    true
}

/// A store into an enum block in its built-whole run (the slot `k` it names,
/// the constant it stores if any, whether the stored value is the block's own
/// address): the tag first, naming a declared case, then each payload slot of
/// that case once.
fn enum_store(
    case: &mut Option<(u32, Vec<bool>)>,
    k: Option<usize>,
    stored_const: Option<i64>,
    cases: &[(u32, Vec<NTy>)],
    self_rooted: bool,
) -> bool {
    match (k, case.as_mut()) {
        // The tag, first: a constant that names a declared case.
        (Some(0), None) => match stored_const.and_then(|t| u32::try_from(t).ok()) {
            Some(t) => match cases.iter().find(|(c, _)| *c == t) {
                Some((_, fs)) => {
                    *case = Some((t, vec![false; fs.len()]));
                    true
                }
                None => false,
            },
            None => false,
        },
        (Some(k), Some((_, stored))) if k >= 1 && k <= stored.len() && !stored[k - 1] && !self_rooted => {
            stored[k - 1] = true;
            true
        }
        _ => false,
    }
}

/// The append that takes one-element list `b`'s element at op `j`, as (the op
/// that takes it, the first op after it).
fn append_window_at(
    func: &MirFunction,
    j: usize,
    later: &Op,
    b: &ValueId,
    base_of: &BTreeMap<ValueId, (ValueId, Option<i64>)>,
) -> Option<(usize, usize)> {
    let rooted = |v: &ValueId, b: ValueId| base_of.get(v).is_some_and(|(r, _)| *r == b);
    if let Some((_, x, _)) = crate::render_native_append::self_append_window(func, j) {
        if x == *b {
            return Some((j, j + 3));
        }
    }
    // The right side of a list `+`: the element is pushed onto the copy.
    if let Op::CallFn { name, args, .. } = later {
        if crate::native_list_ops::is_list_concat(name)
            && matches!(args.as_slice(), [crate::CallArg::Handle(a), crate::CallArg::Handle(r)] if r == b && a != b && !rooted(a, *b))
        {
            return Some((j, j + 1));
        }
    }
    None
}

/// `later` is bookkeeping, or reads nothing of block `b`.
fn bookkeeping_only(later: &Op, b: &ValueId, base_of: &BTreeMap<ValueId, (ValueId, Option<i64>)>) -> bool {
    let rooted = |v: &ValueId, b: ValueId| base_of.get(v).is_some_and(|(r, _)| *r == b);
    if crate::render_native_move::bookkeeping(later) {
        return true;
    }
    let mut reads = Vec::new();
    crate::mir_ops::op_reads(later, &mut reads);
    !reads.iter().any(|v| v == b || rooted(v, *b))
}

/// The flag test the loop at `l` opens with, `c = 0; t = (flag == c); break
/// unless t`: (flag, t, c).
fn flag_header(ops: &[Op], l: usize) -> Option<(ValueId, ValueId, ValueId)> {
    let (Some(Op::ConstInt { dst: c, value: 0 }), Some(Op::IntBinOp { dst: t, op: crate::IntOp::Eq, a, b }), Some(Op::LoopBreakUnless { cond })) =
        (ops.get(l + 1), ops.get(l + 2), ops.get(l + 3))
    else {
        return None;
    };
    let flag = match (a == c, b == c) {
        (false, true) => *a,
        (true, false) => *b,
        _ => return None,
    };
    if cond != t {
        return None;
    }
    Some((flag, *t, *c))
}

/// The `LoopEnd` of the loop opened at `l`.
fn loop_end(ops: &[Op], l: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut end = None;
    for (j, o) in ops.iter().enumerate().skip(l + 1) {
        match o {
            Op::LoopStart => depth += 1,
            Op::LoopEnd if depth == 0 => {
                end = Some(j);
                break;
            }
            Op::LoopEnd => depth -= 1,
            _ => {}
        }
    }
    end
}

/// Nothing in the loop `(l, end)` reads `flag` but its own test, and the
/// header's values `(t, c)` are read nowhere else.
fn flag_unread(ops: &[Op], (l, end): (usize, usize), flag: ValueId, (t, c): (ValueId, ValueId)) -> bool {
    let mut reads = Vec::new();
    for (j, o) in ops.iter().enumerate() {
        // The header's own three ops.
        if (l + 1..=l + 3).contains(&j) {
            continue;
        }
        reads.clear();
        crate::mir_ops::op_reads(o, &mut reads);
        // A write of the flag names it as its slot, not as a read.
        if let Op::SetLocal { local, src } = o {
            if *local == flag && *src != flag {
                reads.retain(|v| *v != flag);
            }
        }
        if (j > l + 3 && j < end && reads.contains(&flag)) || reads.contains(&t) || reads.contains(&c) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::slot_kinds;
    use crate::render_native::NTy;

    #[test]
    fn a_block_of_mixed_constant_slots_is_a_struct_of_its_slot_kinds() {
        let stores = [(Some(0), NTy::I64), (Some(1), NTy::Str)];
        assert_eq!(slot_kinds(&stores, Some(2), None), Some(vec![NTy::I64, NTy::Str]));
        assert_eq!(slot_kinds(&stores, Some(2), Some(&vec![1])), Some(vec![NTy::I64, NTy::Str]));
    }

    /// A block the stores do not type whole stays untyped (it walls): two
    /// kinds into one slot, an unstored slot, an indexed store, a store past
    /// the length, an unknown length, a heap-slot mask that disagrees.
    #[test]
    fn a_block_whose_stores_disagree_stays_untyped() {
        assert_eq!(slot_kinds(&[(Some(0), NTy::I64), (Some(0), NTy::Str), (Some(1), NTy::Str)], Some(2), None), None);
        assert_eq!(slot_kinds(&[(Some(1), NTy::Str)], Some(2), None), None);
        assert_eq!(slot_kinds(&[(Some(0), NTy::I64), (None, NTy::Str)], Some(2), None), None);
        assert_eq!(slot_kinds(&[(Some(0), NTy::I64), (Some(2), NTy::Str)], Some(2), None), None);
        assert_eq!(slot_kinds(&[(Some(0), NTy::I64), (Some(1), NTy::Str)], None, None), None);
        assert_eq!(slot_kinds(&[(Some(0), NTy::I64), (Some(1), NTy::Str)], Some(2), Some(&vec![0])), None);
    }
}
