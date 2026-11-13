//! A Result carrier released right after the `if` that dispatches on its tag
//! is released at the end of each arm of that `if` instead, where the tag is
//! known.
//!
//! After the join, Rust's drop of the carrier reads the tag again: one more
//! branch and a conditional free in the loop body that holds the dispatch. In a
//! loop whose body chooses between `!` arms on loop-invariant conditions, that
//! branch put the body past LLVM's unswitching budget and the loop lost the
//! closed form the standard codegen's `match` kept (`if a % 3 == 1 then f(b)!
//! else if a % 3 == 2 then 0 else g(b)!` summed over `b`: 0.045 s against
//! 0.001 s). Inside an arm the release of the Ok side folds to nothing.
//!
//! Only the release moves: the carrier is read by nothing but its tag and
//! payload views inside the dispatch, and by nothing after it, so the arm's
//! end is past its last read on that path. The render confirms the carrier is
//! still a named owned local held only by reference before naming it.
//!
//! The tag is a `ResTag`, or the carrier's tag word read through its address
//! (`Load{4}` at `Handle(c) + RESULT_TAG_OFFSET`, or at its length word for a
//! scalar-Ok Result; rendered `c.is_err()`). The
//! release it replaces is the carrier's `Drop` at the dispatch's block level,
//! right after the `if` or further on with no read of the carrier between
//! (a second call and its own dispatch in the same loop body: T0ok's
//! consumers held their dead Result to the round's end, 1.26x against the
//! release in each arm).

use crate::{IntOp, MirFunction, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};

/// The `Else` / `EndIf` op indices at which a carrier is released, keyed to
/// the carrier.
pub(crate) fn arm_releases(func: &MirFunction, flag_exits: &BTreeSet<usize>) -> BTreeMap<usize, ValueId> {
    let ops = &func.ops;
    let mut out = BTreeMap::new();
    for (k, op) in ops.iter().enumerate() {
        let Some((t, c)) = tag_of(ops, op) else { continue };
        if let Some((i1, i2)) = dispatch(ops, k, t, c, flag_exits) {
            out.insert(i1, c);
            out.insert(i2, c);
        }
    }
    out
}

/// The (tag, carrier) of a tag read: `ResTag(c)`, or the tag word loaded at
/// `Handle(c) + RESULT_TAG_OFFSET` (or the length word).
fn tag_of(ops: &[Op], op: &Op) -> Option<(ValueId, ValueId)> {
    match op {
        Op::Prim { kind: PrimKind::ResTag, dst: Some(t), args } => match args.as_slice() {
            [c] => Some((*t, *c)),
            _ => None,
        },
        Op::Prim { kind: PrimKind::Load { width: 4 }, dst: Some(t), args } => {
            let [a] = args.as_slice() else { return None };
            let Some(Op::IntBinOp { op: IntOp::Add, a: h, b: off, .. }) = def(ops, *a) else { return None };
            // The tag word, or the length word a scalar-Ok Result is tagged by.
            let tag_words = [crate::render_native_addr::carrier::RESULT_TAG_OFFSET, i64::from(crate::lower::layout::BLOCK_LEN_OFFSET)];
            if !matches!(def(ops, *off), Some(Op::ConstInt { value, .. }) if tag_words.contains(value)) {
                return None;
            }
            match def(ops, *h)? {
                Op::Prim { kind: PrimKind::Handle, args, .. } => match args.as_slice() {
                    [c] => Some((*t, *c)),
                    _ => None,
                },
                _ => None,
            }
        }
        _ => None,
    }
}

fn def(ops: &[Op], v: ValueId) -> Option<&Op> {
    ops.iter().find(|o| crate::mir_ops::defined_value(o) == Some(v))
}

/// The values that are carrier `c` or read it in place: `c`, its handles, the
/// addresses offset from them, and the payload views loaded through those.
fn views_of(ops: &[Op], c: ValueId) -> (BTreeSet<ValueId>, BTreeSet<ValueId>) {
    let mut addrs: BTreeSet<ValueId> = BTreeSet::new();
    let mut loaded = BTreeSet::new();
    for op in ops {
        match op {
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if args.as_slice() == [c] => {
                addrs.insert(*d);
            }
            Op::IntBinOp { dst, op: IntOp::Add, a, .. } if addrs.contains(a) => {
                addrs.insert(*dst);
            }
            Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args } if args.iter().any(|a| addrs.contains(a)) => {
                loaded.insert(*d);
            }
            _ => {}
        }
    }
    (addrs, loaded)
}

/// The `Else` and `EndIf` of the `if` on tag `t` of carrier `c` (read at `k`)
/// when that `if` is the carrier's last use and its arms only view it.
fn dispatch(ops: &[Op], k: usize, t: ValueId, c: ValueId, flag_exits: &BTreeSet<usize>) -> Option<(usize, usize)> {
    let i0 = (k + 1..ops.len()).find(|&i| matches!(ops[i], Op::IfThen { .. }))?;
    if !matches!(ops[i0], Op::IfThen { cond, dst: None } if cond == t) {
        return None;
    }
    let (i1, i2) = arms(ops, i0, flag_exits)?;
    let (addrs, loaded) = views_of(ops, c);
    let carrier = Carrier { c, addrs: &addrs, loaded: &loaded };
    let release = release_at(ops, i2, &carrier)?;
    only_views_before(ops, (k, i2, release), &carrier).then_some((i1, i2))
}

/// The carrier `c` and its views (addresses off it, handles loaded through them).
struct Carrier<'a> {
    c: ValueId,
    addrs: &'a BTreeSet<ValueId>,
    loaded: &'a BTreeSet<ValueId>,
}

impl Carrier<'_> {
    /// Whether `op` reads the carrier or a view of it (`reads` left holding
    /// what `op` reads).
    fn read_by(&self, op: &Op, reads: &mut Vec<ValueId>) -> bool {
        reads.clear();
        crate::mir_ops::op_reads(op, reads);
        reads.iter().any(|r| *r == self.c || self.addrs.contains(r) || self.loaded.contains(r))
    }
}

/// [`dispatch`]'s two arms of the `if` at `i0`: its `Else` and `EndIf`, where
/// both are valueless and no arm leaves the dispatch or may end without
/// reaching its own end.
fn arms(ops: &[Op], i0: usize, flag_exits: &BTreeSet<usize>) -> Option<(usize, usize)> {
    let (mut depth, mut loops) = (0i32, 0i32);
    let mut i1 = None;
    for (i, op) in ops.iter().enumerate().skip(i0 + 1) {
        match op {
            Op::IfThen { .. } => depth += 1,
            Op::Else { val } if depth == 0 => {
                if val.is_some() {
                    return None;
                }
                i1 = Some(i);
            }
            Op::EndIf { val } if depth == 0 => return if val.is_some() { None } else { Some((i1?, i)) },
            Op::EndIf { .. } => depth -= 1,
            Op::LoopStart => loops += 1,
            Op::LoopEnd => loops -= 1,
            // An arm that leaves the dispatch, or may end without reaching
            // its own end, keeps the release where it was.
            Op::LoopBreakUnless { .. } if loops == 0 => return None,
            Op::Return { .. } => return None,
            Op::Prim { kind: PrimKind::Die | PrimKind::ProcExit, .. } => return None,
            _ => {}
        }
        if flag_exits.contains(&i) {
            return None;
        }
    }
    None
}

/// The carrier's release at the dispatch's block level: the first op past
/// the `if` (ending at `i2`) that reads the carrier (or a view of it),
/// reached with every `if` and loop opened since closed again.
fn release_at(ops: &[Op], i2: usize, carrier: &Carrier) -> Option<usize> {
    let (mut depth, mut loops) = (0i32, 0i32);
    let mut reads = Vec::new();
    for (i, op) in ops.iter().enumerate().skip(i2 + 1) {
        if carrier.read_by(op, &mut reads) {
            let dropped = matches!(op, Op::Drop { v } | Op::DropListStr { v } if *v == carrier.c);
            return (dropped && depth == 0 && loops == 0).then_some(i);
        }
        match op {
            Op::IfThen { .. } => depth += 1,
            Op::Else { .. } | Op::EndIf { .. } if depth == 0 => return None,
            Op::EndIf { .. } => depth -= 1,
            Op::LoopStart => loops += 1,
            Op::LoopEnd if loops == 0 => return None,
            Op::LoopEnd => loops -= 1,
            _ => {}
        }
    }
    None
}

/// Whether every read of the carrier from `k` on, other than its release,
/// sits before the dispatch's end `i2` and only views it (its tag, its
/// scalar Ok, its err string, its handle, an address or load off a view).
fn only_views_before(ops: &[Op], (k, i2, release): (usize, usize, usize), carrier: &Carrier) -> bool {
    let c = carrier.c;
    let mut reads = Vec::new();
    for (i, op) in ops.iter().enumerate().skip(k + 1) {
        if i == release || !carrier.read_by(op, &mut reads) {
            continue;
        }
        let views = matches!(op, Op::Prim { kind: PrimKind::ResTag | PrimKind::ResOkScalar | PrimKind::ResErrStr, .. })
            || matches!(op, Op::Prim { kind: PrimKind::Handle, args, .. } if args.as_slice() == [c])
            || (!reads.contains(&c)
                && !reads.iter().any(|r| carrier.loaded.contains(r))
                && matches!(
                    op,
                    Op::IntBinOp { op: IntOp::Add, .. } | Op::Prim { kind: PrimKind::Load { .. } | PrimKind::LoadHandle, .. }
                ));
        if i > i2 || !views {
            return false;
        }
    }
    true
}

/// The release line for carrier `name` at an arm's end, when the text so far
/// binds it as an owned local and only ever borrows it or reads its tag.
pub(crate) fn release_line(text: &str, name: &str) -> Option<String> {
    let b = text.as_bytes();
    let mut bound = false;
    let mut from = 0;
    while let Some(k) = text[from..].find(name) {
        let at = from + k;
        let end = at + name.len();
        from = end;
        let word = (at == 0 || !(b[at - 1].is_ascii_alphanumeric() || b[at - 1] == b'_'))
            && (end == b.len() || !(b[end].is_ascii_alphanumeric() || b[end] == b'_'));
        if !word {
            continue;
        }
        let before = &text[..at];
        let after = &text[end..];
        // A comment naming it (`// v12: the address of v11`) is no use.
        let line = &before[before.rfind('\n').map_or(0, |n| n + 1)..];
        if line.trim_start().starts_with("//") {
            continue;
        }
        if before.ends_with("let mut ") && after.starts_with(':') {
            bound = true;
        } else if before.ends_with("drop(") && after.starts_with(");") {
            // The other arm's release.
        } else if !(before.ends_with('&') || before.ends_with("&*") || before.ends_with("&mut ") || after.starts_with(".is_err()")) {
            return None;
        }
    }
    bound.then(|| format!("drop({name});"))
}

#[cfg(test)]
#[path = "render_native_arm_drop_test.rs"]
mod tests;
