//! A read whose own rebind follows before any other read: the read is the
//! old value's last, so it may move into a by-value param.
use crate::{MirFunction, Op, ValueId};

/// Whether `src`, read at `ops[at]`, is rebound (`SetLocal`) later in the
/// block `at` sits in (not inside a nested `if` or loop) before any further
/// read: the rebind dominates everything after, the loop's back edge
/// included, so the read is the old value's last (`m = map.set(m, k, v)`).
///
/// `inout`: `src` is an in-out param (`crate::native_inout`), the caller's
/// place behind `&mut`. No exit may lie between the read and the rebind
/// either — an exit hands the place back (a `Consume`, which the read scan
/// skips as bookkeeping) — so the place taken empty (`std::mem::take`) is
/// seen on no path: `p = op(p, ..)` feeds the op's by-value form (its
/// `@reuse` owned twin, an in-place concat or push) and `*p = ..` writes the
/// result back.
pub(super) fn rebound_before_read(func: &MirFunction, at: usize, src: ValueId, inout: bool) -> bool {
    let mut depth = 0usize;
    let mut reads = Vec::new();
    for op in &func.ops[at + 1..] {
        match op {
            Op::IfThen { .. } | Op::LoopStart => depth += 1,
            Op::EndIf { .. } | Op::LoopEnd | Op::Else { .. } if depth == 0 => return false,
            Op::EndIf { .. } | Op::LoopEnd => depth -= 1,
            Op::Return { .. } if inout => return false,
            Op::Consume { v } | Op::Borrow { v } if inout && *v == src => return false,
            _ => {}
        }
        if depth == 0 {
            if let Op::SetLocal { local, src: s } = op {
                if *local == src {
                    return *s != src;
                }
            }
        }
        if super::bookkeeping(op) {
            continue;
        }
        reads.clear();
        crate::mir_ops::op_reads(op, &mut reads);
        if reads.contains(&src) {
            return false;
        }
    }
    false
}
