// Guard facts for payload reads, including conjunctions used by Option equality.
type GuardFacts = BTreeMap<ValueId, bool>;
type GuardSides = (GuardFacts, GuardFacts);
type GuardArm = (bool, GuardSides);

/// Sink a hoisted payload read into the single branch dominating all its
/// uses. A non-Default payload (for example a cancellation handle) has no
/// value on the absent side. Reading it only in its proven tag arm avoids
/// manufacturing one. This runs before ownership checking and certificates.
/// Multiple disjoint use scopes, mutated carriers, and unguarded uses keep
/// the original read and the renderer's refusal.
pub(crate) fn sink_guarded_payload_reads(func: &mut MirFunction) {
    let sides = hoisted_sides(func);
    if sides.is_empty() { return; }
    let mut stack = Vec::new();
    let mut paths = Vec::with_capacity(func.ops.len());
    let mut uses: BTreeMap<ValueId, Vec<usize>> = BTreeMap::new();
    for (at, op) in func.ops.iter().enumerate() {
        paths.push(stack.clone());
        let mut read = Vec::new();
        crate::mir_ops::op_reads(op, &mut read);
        for v in read { uses.entry(v).or_default().push(at); }
        match op {
            Op::IfThen { .. } => stack.push(at),
            Op::Else { .. } => { if let Some(last) = stack.last_mut() { *last = at; } }
            Op::EndIf { .. } => { stack.pop(); }
            _ => {}
        }
    }
    let mut moved = std::collections::BTreeSet::new();
    let mut after: BTreeMap<usize, Vec<Op>> = BTreeMap::new();
    for (at, op) in func.ops.iter().enumerate() {
        let Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), .. } = op else { continue };
        if !sides.contains_key(d) || func.ret == Some(*d) { continue; }
        let Some(reads) = uses.get(d).filter(|u| !u.is_empty()) else { continue };
        let mut common = paths[reads[0]].clone();
        for u in reads.iter().skip(1) {
            common.truncate(common.iter().zip(&paths[*u]).take_while(|(a, b)| a == b).count());
        }
        if !common.starts_with(&paths[at]) || common.len() <= paths[at].len() { continue; }
        let enter = *common.last().expect("strictly deeper common branch");
        if enter <= at || reads.iter().any(|u| *u <= enter) { continue; }
        if func.ops[at + 1..enter].iter().any(|op| matches!(op, Op::LoopStart | Op::LoopEnd)) { continue; }
        // This scope dominates every use. The normal carrier renderer still
        // requires a matching tag fact at the new read location.
        after.entry(enter).or_default().push(op.clone());
        moved.insert(at);
    }
    if moved.is_empty() { return; }
    let mut out = Vec::with_capacity(func.ops.len());
    for (at, op) in std::mem::take(&mut func.ops).into_iter().enumerate() {
        if !moved.contains(&at) { out.push(op); }
        if let Some(reads) = after.remove(&at) { out.extend(reads); }
    }
    func.ops = out;
}

fn predicate_facts(v: &ValueId, tags: &BTreeMap<ValueId, (ValueId, bool)>, groups: &BTreeMap<ValueId, GuardSides>) -> GuardSides {
    groups.get(v).cloned().or_else(|| tags.get(v).map(|(r, s)|
        ([( *r, *s )].into_iter().collect(), [( *r, !*s )].into_iter().collect())))
        .unwrap_or_default()
}

fn combined_facts(a: &GuardFacts, b: &GuardFacts, union: bool) -> GuardFacts {
    if !union { return a.iter().filter(|(k, v)| b.get(k) == Some(v)).map(|(k, v)| (*k, *v)).collect(); }
    let mut out = a.clone();
    for (root, side) in b {
        if out.get(root).is_some_and(|s| s != side) { return GuardFacts::new(); }
        out.insert(*root, *side);
    }
    out
}

/// The side each hoisted payload read `op` reads is used under (`None` once
/// two uses disagree).
fn note_hoisted_uses(
    op: &Op,
    reads: &BTreeMap<ValueId, ValueId>,
    arms: &[GuardArm],
    sides: &mut BTreeMap<ValueId, Option<bool>>,
) {
    let mut used = Vec::new();
    crate::mir_ops::op_reads(op, &mut used);
    for v in used {
        let Some(root) = reads.get(&v) else { continue };
        let side = arms.iter().rev().find_map(|(then, (yes, no))| if *then { yes.get(root) } else { no.get(root) }).copied();
        let e = sides.entry(v).or_insert(side);
        if *e != side {
            *e = None;
        }
    }
}

/// A payload read the match lowering hoists above its tag test (both arms'
/// binders are read before the `if`) → the side its uses select: `true` =
/// the then-arm (tag 1, Err). Read off where the value is used: every use
/// inside one arm of an `if` that tests the same block's tag word.
pub(super) fn hoisted_sides(func: &MirFunction) -> BTreeMap<ValueId, bool> {
    let mut consts: BTreeMap<ValueId, i64> = BTreeMap::new();
    let mut at: BTreeMap<ValueId, (ValueId, i64)> = BTreeMap::new();
    let mut tags: BTreeMap<ValueId, (ValueId, bool)> = BTreeMap::new();
    let mut reads: BTreeMap<ValueId, ValueId> = BTreeMap::new();
    let mut sides: BTreeMap<ValueId, Option<bool>> = BTreeMap::new();
    let mut arms: Vec<GuardArm> = Vec::new();
    let mut groups: BTreeMap<ValueId, GuardSides> = BTreeMap::new();
    let mutable: std::collections::BTreeSet<_> = func.ops.iter().filter_map(|op|
        if let Op::SetLocal { local, .. } = op { Some(*local) } else { None }).collect();
    for op in &func.ops {
        // Uses first: a read used under a tag test of its own block.
        note_hoisted_uses(op, &reads, &arms, &mut sides);
        let changed = match op {
            Op::SetLocal { local, .. } => Some(*local),
            Op::Prim { kind: PrimKind::Store { .. }, args, .. } => args.first().and_then(|a| at.get(a)).map(|(root, _)| *root),
            _ => None,
        };
        if let Some(root) = changed {
            for (read, base) in &reads { if *base == root { sides.insert(*read, None); } }
            tags.retain(|key, (base, _)| *key != root && *base != root);
            for (_, (yes, no)) in &mut arms { yes.remove(&root); no.remove(&root); }
            for (yes, no) in groups.values_mut() { yes.remove(&root); no.remove(&root); }
        }
        note_hoist_facts(op, &mutable, &mut consts, &mut at, &mut tags, &mut reads, &mut arms, &mut groups);
    }
    sides.into_iter().filter_map(|(v, s)| s.map(|s| (v, s))).collect()
}

/// [`hoisted_sides`]' facts from one op: the constants, the addresses off a
/// block's handle, the tag words and payload reads loaded through them, and
/// the open tag tests.
fn note_hoist_facts(
    op: &Op,
    mutable: &std::collections::BTreeSet<ValueId>,
    consts: &mut BTreeMap<ValueId, i64>,
    at: &mut BTreeMap<ValueId, (ValueId, i64)>,
    tags: &mut BTreeMap<ValueId, (ValueId, bool)>,
    reads: &mut BTreeMap<ValueId, ValueId>,
    arms: &mut Vec<GuardArm>,
    groups: &mut BTreeMap<ValueId, GuardSides>,
) {
        match op {
            Op::ConstInt { dst, value } => {
                if !mutable.contains(dst) { consts.insert(*dst, *value); }
            }
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if args.len() == 1 => {
                at.insert(*d, (args[0], 0));
            }
            Op::IntBinOp { dst, op: crate::IntOp::Add, a, b } => {
                if let (Some(&(r, o)), Some(c)) = (at.get(a), consts.get(b)) {
                    at.insert(*dst, (r, o + c));
                }
            }
            Op::Prim { kind: PrimKind::Load { width: 4 }, dst: Some(d), args } if args.len() == 1 => {
                // The tag word of either Result layout (an Option's length
                // word lands here too; only a Result's payload read asks).
                if let Some(&(r, o)) = at.get(&args[0]) {
                    if (o == RESULT_TAG_OFFSET || o == i64::from(BLOCK_LEN_OFFSET))
                        && !mutable.contains(d) { tags.insert(*d, (r, true)); }
                }
            }
            Op::Prim { kind: PrimKind::LoadHandle | PrimKind::Load { width: 8 }, dst: Some(d), args } if args.len() == 1 => {
                if let Some(&(r, o)) = at.get(&args[0]) {
                    if o == i64::from(BLOCK_HEADER) {
                        reads.insert(*d, r);
                    }
                }
            }
            Op::IntBinOp { dst, op: cmp @ (crate::IntOp::And | crate::IntOp::Or), a, b } if !mutable.contains(dst) => {
                let (ay, an) = predicate_facts(a, tags, groups);
                let (by, bn) = predicate_facts(b, tags, groups);
                let and = *cmp == crate::IntOp::And;
                groups.insert(*dst, (combined_facts(&ay, &by, and), combined_facts(&an, &bn, !and)));
            }
            Op::IntBinOp { dst, op: cmp, a, b } => {
                if !mutable.contains(dst) {
                    if let Some(tag) = tag_comparison(*cmp, *a, *b, consts, tags) { tags.insert(*dst, tag); }
                }
            }
            Op::IfThen { cond, .. } => arms.push((true, predicate_facts(cond, tags, groups))),
            Op::Else { .. } => {
                if let Some(a) = arms.last_mut() {
                    a.0 = !a.0;
                }
            }
            Op::EndIf { .. } => {
                arms.pop();
            }
            _ => {}
        }
}

