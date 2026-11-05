//! The row-4 census (#4005): every heap access the shipped bytes contain,
//! and which of them a witness event locates.
//!
//! A located access is one the bounds, init or fields witness names (its
//! anchor is the instruction; an `a` anchored at the release call after
//! its load counts that load) — the accesses the proven checkers and this
//! crate's byte checks hold. Every other load, store, `memory.copy` and
//! `memory.fill` is outside every certificate: the inventory row 4's
//! trusted list owes. A census, not a verdict: it rejects nothing.
//!
//! Each access is classed by where it is — a certified FRAME (a function
//! the anchor file's `rc` line names: user code and linked self-host
//! bodies) or a RUNTIME routine (every other defined function: the fixed
//! helpers, the per-program helpers, drop / copy / equality / display
//! bodies) — and by its shape: a HEADER word (memarg offset below the
//! payload and the address a bare `local.get`: a block's rc, len or cap
//! word), BULK (`memory.copy` / `memory.fill`), or a payload or raw
//! access.

use std::collections::HashSet;

use wasmparser::Operator as Op;

use crate::decode::{is_load, store_width, Body};
use crate::{parse_anchors_held, parse_at, At};

/// One class's counts.
#[derive(Default, Debug, Clone, Copy)]
pub struct Tally {
    pub accesses: usize,
    pub located: usize,
    pub header: usize,
    pub bulk: usize,
}

/// The census of one artifact: certified frames, runtime routines.
#[derive(Default, Debug, Clone, Copy)]
pub struct Census {
    pub frames: Tally,
    pub routines: Tally,
}

fn offset(op: &Op<'_>) -> Option<u64> {
    if let Some((_, off)) = store_width(op) {
        return Some(off);
    }
    if !is_load(op) {
        return None;
    }
    Some(match op {
        Op::I32Load { memarg }
        | Op::I64Load { memarg }
        | Op::F32Load { memarg }
        | Op::F64Load { memarg }
        | Op::I32Load8S { memarg }
        | Op::I32Load8U { memarg }
        | Op::I32Load16S { memarg }
        | Op::I32Load16U { memarg }
        | Op::I64Load8S { memarg }
        | Op::I64Load8U { memarg }
        | Op::I64Load16S { memarg }
        | Op::I64Load16U { memarg }
        | Op::I64Load32S { memarg }
        | Op::I64Load32U { memarg }
        | Op::V128Load { memarg } => memarg.offset,
        _ => return None,
    })
}

/// Every `func:op` token of the anchor file's event lines.
fn located(anchors: &str) -> Result<HashSet<At>, String> {
    let (_, init, bounds, _, _, _) = parse_anchors_held(anchors)?;
    let fields = crate::fields_anchors(anchors);
    let mut out = HashSet::new();
    for t in [init.as_str(), bounds.as_str(), fields.as_str()]
        .into_iter()
        .flat_map(|l| l.split(|c: char| c.is_whitespace() || c == ';' || c == '|'))
        .filter(|t| !t.is_empty())
    {
        if let Ok(Some(at)) = parse_at(t) {
            out.insert(at);
        }
    }
    Ok(out)
}

/// The census of `bodies` against `anchors`.
pub fn census(bodies: &[Body<'_>], anchors: &str) -> Result<Census, String> {
    let at = located(anchors)?;
    let (_, _, _, _, _, rc) = parse_anchors_held(anchors)?;
    let frames: HashSet<usize> = rc.map(|(_, _, f, _, _, _)| f.into_iter().map(|(p, _)| p).collect()).unwrap_or_default();
    let mut c = Census::default();
    for (func, b) in bodies.iter().enumerate() {
        let t = if frames.contains(&func) { &mut c.frames } else { &mut c.routines };
        for (k, op) in b.ops.iter().enumerate() {
            let bulk = matches!(op, Op::MemoryCopy { .. } | Op::MemoryFill { .. });
            let Some(off) = offset(op).or(bulk.then_some(0)) else { continue };
            t.accesses += 1;
            let here = at.contains(&At { func, op: k });
            let before_call = b.ops.get(k + 1).is_some_and(|o| matches!(o, Op::Call { .. })) && at.contains(&At { func, op: k + 1 });
            if here || before_call {
                t.located += 1;
            } else if bulk {
                t.bulk += 1;
            } else if off < 12 && b.args[k].first().copied().flatten().is_some_and(|p| matches!(b.ops[p], Op::LocalGet { .. })) {
                t.header += 1;
            }
        }
    }
    Ok(c)
}
