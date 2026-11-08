//! A closure block nothing reads (#4006, no-perf-loss). A let-bound lambda is
//! built as a closure block at its binding, and every direct call of it is
//! inlined (`try_inline_direct_lambda_call`), so a lambda that is only called
//! directly leaves a block that is allocated, has its slots stored — its
//! heap captures copied in — and is then only released. The native render
//! built all of it on every execution: a fn holding `(i) => list.get(xs, i)`
//! copied `xs` per call, where the standard codegen borrows it.
//!
//! The block, the copies made only to be stored in it, their addresses and
//! their releases are removed together, before the render and before the
//! ownership certificate is computed from the same MIR: a block that is
//! never read has no observable effect, and a copy that only it owned is
//! never made, so nothing it would have released is left behind.
use crate::{IntOp, MirFunction, Op, PrimKind, ValueId};
use std::collections::BTreeSet;

/// Remove every unread closure block of `f` (see the module doc).
pub(crate) fn elide(f: &mut MirFunction) {
    while let Some(dead) = (0..f.ops.len()).find_map(|i| unread_block(f, i)) {
        // Op indices that side tables carry move with the ops left.
        let kept: Vec<usize> = (0..f.ops.len()).filter(|i| !dead.contains(i)).collect();
        f.call_moves = f.call_moves.iter().filter_map(|m| kept.iter().position(|k| k == m)).collect();
        let mut k = 0;
        f.ops.retain(|_| {
            let keep = !dead.contains(&k);
            k += 1;
            keep
        });
    }
}

/// A closure block's declared type, as the lowering spells it
/// (`render_native_rec::block_key` of a `Ty::Fn`).
fn is_closure_block(f: &MirFunction, v: ValueId) -> bool {
    f.aggregate_blocks.get(&v).is_some_and(|k| k.starts_with("Fn {"))
}

/// The ops of the closure block `ops[i]` allocates when nothing reads it.
fn unread_block(f: &MirFunction, i: usize) -> Option<BTreeSet<usize>> {
    let root = match &f.ops[i] {
        Op::Alloc { dst, .. } | Op::ListLit { dst, .. } => *dst,
        _ => return None,
    };
    if !is_closure_block(f, root) || f.ret == Some(root) {
        return None;
    }
    let mut material: BTreeSet<ValueId> = BTreeSet::from([root]);
    let mut ops: BTreeSet<usize> = BTreeSet::from([i]);
    let mut vals = Vec::new();
    loop {
        let before = (material.len(), ops.len());
        for (j, op) in f.ops.iter().enumerate() {
            if ops.contains(&j) {
                continue;
            }
            vals.clear();
            crate::mir_ops::op_values(op, &mut vals);
            if !vals.iter().any(|v| material.contains(v)) {
                continue;
            }
            if !absorb(f, op, &mut material)? {
                return None;
            }
            ops.insert(j);
        }
        if (material.len(), ops.len()) == before {
            break;
        }
    }
    Some(ops)
}

/// Take `op`, which touches the block's material, into it: `Some(true)` when
/// it only builds, addresses, fills or releases that material; `None` when it
/// reads it (the block is live).
fn absorb(f: &MirFunction, op: &Op, material: &mut BTreeSet<ValueId>) -> Option<bool> {
    match op {
        // The block's (or a captured copy's) address.
        Op::Prim { kind: PrimKind::Handle, dst: Some(h), args } if args.len() == 1 && material.contains(&args[0]) => {
            material.insert(*h);
        }
        // An address moved by a constant within the block.
        Op::IntBinOp { dst, op: IntOp::Add, a, b } if material.contains(a) != material.contains(b) => {
            material.insert(*dst);
        }
        // A store into the block: a scalar word, or the address of a capture
        // this block alone owns.
        Op::Prim { kind: PrimKind::Store { .. }, dst: None, args }
            if args.len() == 2 && material.contains(&args[0]) && !material.contains(&args[1]) =>
        {
            if let Some(c) = handle_of(f, args[1]) {
                if !fresh_copy(f, c) {
                    return None;
                }
                material.insert(c);
                material.insert(args[1]);
            } else if is_address(f, args[1]) {
                return None;
            }
        }
        Op::Prim { kind: PrimKind::Store { .. }, dst: None, args } if args.len() == 2 && material.contains(&args[0]) && material.contains(&args[1]) => {}
        // A captured copy's own definition.
        Op::Dup { dst, src } if material.contains(dst) && !material.contains(src) => {}
        op if is_release(op) => {}
        _ => return None,
    }
    Some(true)
}

/// The value whose address `v` is (`Prim{Handle}`), if `v` is one.
fn handle_of(f: &MirFunction, v: ValueId) -> Option<ValueId> {
    f.ops.iter().find_map(|op| match op {
        Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if *d == v && args.len() == 1 => Some(args[0]),
        _ => None,
    })
}

/// Whether `v` is an address computed from another (`IntBinOp::Add`).
fn is_address(f: &MirFunction, v: ValueId) -> bool {
    f.ops.iter().any(|op| matches!(op, Op::IntBinOp { dst, op: IntOp::Add, a, b } if *dst == v && (handle_of(f, *a).is_some() || handle_of(f, *b).is_some())))
}

/// Whether `c` is a copy made here (`Dup`), defined once: dropping it with
/// the block leaves the value it copies as it was.
fn fresh_copy(f: &MirFunction, c: ValueId) -> bool {
    let defs = f.ops.iter().filter(|op| match op {
        Op::Dup { dst, .. } | Op::Alloc { dst, .. } | Op::ListLit { dst, .. } => *dst == c,
        Op::SetLocal { local, .. } => *local == c,
        _ => false,
    });
    let mut defs = defs.peekable();
    matches!(defs.next(), Some(Op::Dup { .. })) && defs.next().is_none() && !f.params.iter().any(|p| p.value == c)
}

fn is_release(op: &Op) -> bool {
    matches!(
        op,
        Op::Drop { .. }
            | Op::DropListStr { .. }
            | Op::DropValue { .. }
            | Op::DropListValue { .. }
            | Op::DropListStrValue { .. }
            | Op::DropListStrStr { .. }
            | Op::DropListIntStr { .. }
            | Op::DropListStrInt { .. }
            | Op::DropResultListValue { .. }
            | Op::DropResultValue { .. }
            | Op::DropResultStrInt { .. }
            | Op::DropResultValueInt { .. }
            | Op::DropResultListValueInt { .. }
            | Op::DropResultListStrInt { .. }
            | Op::DropResultListStr { .. }
            | Op::DropListListStr { .. }
            | Op::DropVariant { .. }
            | Op::DropWrapperRec { .. }
            | Op::Consume { .. }
    )
}
