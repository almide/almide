//! The self-append rewrite: `x = x + [e]` (also what `list.push(x, e)`
//! desugars to on the v1 leg) lowers today as
//!
//! ```text
//! ListLit  t ← [e]                 ; a 1-element temp list
//! CallFn   d ← __list_concat(x, t) ; FULL COPY of x, every append
//! Drop     x
//! SetLocal x ← d
//! Drop     t
//! ```
//!
//! — an O(len) copy per append, O(n²) for the canonical accumulator loop
//! (spectralnorm's row build: ~4.8 GB of memcpy per run; fannkuch's
//! permutation seeds; every real-world `list.push` loop). This pass rewrites
//! the exact window to
//!
//! ```text
//! CallFn   d ← __list_append1(x, e)
//! Drop     x
//! SetLocal x ← d
//! ```
//!
//! where the runtime `$list_append1` (render_wasm_p3.rs) BORROWS `x` and:
//! - if `rc(x) == 1 && len < cap`: stores `e` in place, bumps `len`,
//!   `rc_inc`s and returns `x` itself (the caller's `Drop x` rebalances to
//!   rc 1 — the sole surviving handle is the rebound slot);
//! - else: allocates `cap = 2·len + headroom` (amortized doubling), copies,
//!   appends, returns the fresh block (the caller's `Drop x` releases the
//!   old reference exactly as the concat shape did).
//!
//! Value semantics are preserved DYNAMICALLY: in-place mutation happens only
//! when `rc == 1` at entry, i.e. the about-to-be-dropped caller handle is
//! the only owner — no other observer can see the mutation. No static alias
//! proof is needed, so the rewrite fires on every self-append.
//!
//! The CERTIFICATE stream is untouched by construction: the slot's
//! loop-carried `(id)` shape (feeder `i` from the heap-returning call + the
//! old reference's `d`) is exactly what remains; the temp `t`'s balanced
//! `i…d` pair simply no longer exists. `Drop x` stays REAL (`rc_dec`), so
//! the release trace still matches one `rc_dec` per witness drop.

use crate::{CallArg, MirFunction, Op, ValueId};
use std::collections::BTreeMap;

/// `Some((e, d, x))` when `ops[i..i+5]` is exactly the self-append concat
/// window described in the module doc. `occ` counts every ValueId mention
/// (defs + reads) across the whole function: the temp list and the concat
/// result must not be referenced outside the window.
fn match_window(
    ops: &[Op],
    i: usize,
    occ: &BTreeMap<ValueId, usize>,
) -> Option<(ValueId, ValueId, ValueId)> {
    let Op::ListLit { dst: t, elems } = &ops[i] else { return None };
    let [e] = elems.as_slice() else { return None };
    let Op::CallFn { dst: Some(d), name, args, .. } = &ops[i + 1] else { return None };
    if name != "__list_concat" {
        return None;
    }
    let [CallArg::Handle(x), CallArg::Handle(t2)] = args.as_slice() else { return None };
    if t2 != t {
        return None;
    }
    let Op::Drop { v: x2 } = &ops[i + 2] else { return None };
    let Op::SetLocal { local: x3, src: d2 } = &ops[i + 3] else { return None };
    let Op::Drop { v: t3 } = &ops[i + 4] else { return None };
    if x2 != x || x3 != x || d2 != d || t3 != t {
        return None;
    }
    // Window-local only: t = ListLit def + concat arg + drop (3 mentions);
    // d = call dst + SetLocal src (2). Any extra reference means the shape
    // is not the pure self-append and the copying concat must stay.
    if occ.get(t).copied() != Some(3) || occ.get(d).copied() != Some(2) {
        return None;
    }
    Some((*e, *d, *x))
}

/// Rewrite every self-append concat window in `functions` to the amortized
/// O(1) `__list_append1` form. Scalar-element lists only by construction:
/// the window is keyed to `__list_concat` (the byte-copy flavor) — the
/// rc-incrementing `__list_concat_rc` element families keep their shape.
pub fn rewrite_self_append(functions: &mut [MirFunction]) {
    let mut vals: Vec<ValueId> = Vec::new();
    for f in functions.iter_mut() {
        if !f.ops.iter().any(|op| matches!(op,
            Op::CallFn { name, .. } if name == "__list_concat"))
        {
            continue;
        }
        let mut occ: BTreeMap<ValueId, usize> = BTreeMap::new();
        for op in &f.ops {
            vals.clear();
            crate::render_wasm::op_values(op, &mut vals);
            for v in &vals {
                *occ.entry(*v).or_insert(0) += 1;
            }
        }
        let mut i = 0;
        let mut out: Vec<Op> = Vec::with_capacity(f.ops.len());
        while i < f.ops.len() {
            if i + 5 <= f.ops.len() {
                if let Some((e, d, x)) = match_window(&f.ops, i, &occ) {
                    out.push(Op::CallFn {
                        dst: Some(d),
                        name: "__list_append1".to_string(),
                        args: vec![CallArg::Handle(x), CallArg::Scalar(e)],
                        result: Some(crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }),
                    });
                    out.push(Op::Drop { v: x });
                    out.push(Op::SetLocal { local: x, src: d });
                    i += 5;
                    continue;
                }
            }
            out.push(f.ops[i].clone());
            i += 1;
        }
        f.ops = out;
    }
}
