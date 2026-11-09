// Included from render_native_slots_b.rs; unchanged exit and fill helpers.
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
