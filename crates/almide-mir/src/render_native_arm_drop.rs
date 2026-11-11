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

use crate::{MirFunction, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};

/// The `Else` / `EndIf` op indices at which a carrier is released, keyed to
/// the carrier.
pub(crate) fn arm_releases(func: &MirFunction, flag_exits: &BTreeSet<usize>) -> BTreeMap<usize, ValueId> {
    let ops = &func.ops;
    let mut out = BTreeMap::new();
    for (k, op) in ops.iter().enumerate() {
        let Op::Prim { kind: PrimKind::ResTag, dst: Some(t), args } = op else { continue };
        let [c] = args.as_slice() else { continue };
        if let Some((i1, i2)) = dispatch(ops, k, *t, *c, flag_exits) {
            out.insert(i1, *c);
            out.insert(i2, *c);
        }
    }
    out
}

/// The `Else` and `EndIf` of the `if` on tag `t` of carrier `c` (read at `k`)
/// when that `if` is the carrier's last use and its arms only view it.
fn dispatch(ops: &[Op], k: usize, t: ValueId, c: ValueId, flag_exits: &BTreeSet<usize>) -> Option<(usize, usize)> {
    let i0 = (k + 1..ops.len()).find(|&i| matches!(ops[i], Op::IfThen { .. }))?;
    if !matches!(ops[i0], Op::IfThen { cond, dst: None } if cond == t) {
        return None;
    }
    let (mut depth, mut loops) = (0i32, 0i32);
    let (mut i1, mut i2) = (None, None);
    for (i, op) in ops.iter().enumerate().skip(i0 + 1) {
        match op {
            Op::IfThen { .. } => depth += 1,
            Op::Else { val } if depth == 0 => {
                if val.is_some() {
                    return None;
                }
                i1 = Some(i);
            }
            Op::EndIf { val } if depth == 0 => {
                if val.is_some() {
                    return None;
                }
                i2 = Some(i);
                break;
            }
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
    let (i1, i2) = (i1?, i2?);
    let released = match ops.get(i2 + 1) {
        Some(Op::Drop { v } | Op::DropListStr { v }) => *v == c,
        _ => false,
    };
    if !released {
        return None;
    }
    let mut reads = Vec::new();
    for (i, op) in ops.iter().enumerate().skip(k + 1) {
        if i == i2 + 1 {
            continue;
        }
        reads.clear();
        crate::mir_ops::op_reads(op, &mut reads);
        if !reads.contains(&c) {
            continue;
        }
        let views = matches!(op, Op::Prim { kind: PrimKind::ResTag | PrimKind::ResOkScalar | PrimKind::ResErrStr, .. });
        if i > i2 || !views {
            return None;
        }
    }
    Some((i1, i2))
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
