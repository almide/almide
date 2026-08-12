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
//! where the runtime `__list_append1` (SELF-HOSTED in stdlib/list_concat.almd
//! — §4.1 forbids growing the hand-written WAT floor) BORROWS `x` and:
//! - if `rc(x) == 1 && len < cap`: stores `e` in place, bumps `len`, and
//!   returns `x` itself — its own value-semantics return Dups (rc → 2), and
//!   the caller's `Drop x` rebalances to rc 1 on the rebound slot;
//! - else: allocates `cap = 2·len + 8` (amortized doubling), copies,
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

/// The window HEAD is exactly these four ops, in order: `ListLit t`,
/// `d ← __list_concat(x, t)`, `Drop x`, `SetLocal x ← d`. The temp's trailing
/// `Drop t` is searched separately (it may trail by straight-line ops).
const WINDOW_HEAD: usize = 4;

/// How many straight-line ops past a window a trailing `Drop` may lag (drops
/// batch after e.g. the loop counter increment — see `match_window`'s doc).
/// Shared by the scalar-list window and the string-chain window.
const DROP_SCAN: usize = 8;

/// `Some((e, d, x, drop_at))` when `ops[i..i+WINDOW_HEAD]` is exactly the
/// self-append head described in the module doc and `ops[drop_at]` is the
/// temp's trailing `Drop` in the same straight-line region. `occ` counts
/// every ValueId mention (defs + reads) across the whole function: the temp
/// list and the concat result must not be referenced outside the window.
fn match_window(
    ops: &[Op],
    i: usize,
    occ: &BTreeMap<ValueId, usize>,
) -> Option<(ValueId, ValueId, ValueId, usize)> {
    let Op::ListLit { dst: t, elems } = &ops[i] else {
        return None;
    };
    let [e] = elems.as_slice() else { return None };
    let Op::CallFn {
        dst: Some(d),
        name,
        args,
        ..
    } = &ops[i + 1]
    else {
        return None;
    };
    if name != "__list_concat" {
        return None;
    }
    let [CallArg::Handle(x), CallArg::Handle(t2)] = args.as_slice() else {
        return None;
    };
    if t2 != t {
        return None;
    }
    let Op::Drop { v: x2 } = &ops[i + 2] else {
        return None;
    };
    let Op::SetLocal { local: x3, src: d2 } = &ops[i + 3] else {
        return None;
    };
    if x2 != x || x3 != x || d2 != d {
        return None;
    }
    // The temp's Drop may TRAIL the rebind by unrelated straight-line ops —
    // `list.push(xs, j); j = j + 1` batches the temp's drop AFTER the
    // increment, so the exact-5-op window missed every push that is not the
    // loop body's last statement (the loop_buffer_churn Hang class: each such
    // push stayed a full-copy concat, O(n²)). `occ(t) == 3` below proves
    // nothing in between references the temp, so DELETING the trailing drop
    // is position-independent; the scan stops at any control marker (the
    // drop must live in the same straight-line region as the window).
    let mut drop_at = None;
    for (k, op) in ops.iter().enumerate().skip(i + WINDOW_HEAD).take(DROP_SCAN) {
        match op {
            Op::Drop { v } if v == t => {
                drop_at = Some(k);
                break;
            }
            Op::IfThen { .. }
            | Op::Else { .. }
            | Op::EndIf { .. }
            | Op::LoopStart
            | Op::LoopBreakUnless { .. }
            | Op::LoopEnd => break,
            _ => {}
        }
    }
    let drop_at = drop_at?;
    // Window-local only: t = ListLit def + concat arg + drop (3 mentions);
    // d = call dst + SetLocal src (2). Any extra reference means the shape
    // is not the pure self-append and the copying concat must stay.
    if occ.get(t).copied() != Some(3) || occ.get(d).copied() != Some(2) {
        return None;
    }
    Some((*e, *d, *x, drop_at))
}

/// The HEAP-ELEMENT self-append window (#939). The lowering of `list.push(x, e)`
/// over owned-handle slots (`List[Value]`, `List[String]`, `List[(k,v)]`) does
/// not produce the scalar window's `ListLit` — the 1-element temp is an
/// `Alloc{DynListStr}` filled by stores, the callee is `__list_concat_rc`, and
/// the drops are the recursive list-drop family — so `match_window` never fired
/// and every heap-element push stayed a full-copy concat: O(n²), which is what
/// took json.parse's array loop from 5 ms to minutes on a 103 KiB document.
///
/// The rewrite here is NAME-ONLY: `__list_concat_rc(x, t)` becomes
/// `__list_append1_rc(x, t)` and every surrounding op stays — the temp is still
/// built (the element lives in it) and still dropped (its recursive walk
/// rc_decs the element once, balancing the append's acquire — the exact balance
/// the concat left). What licenses it: the very next two ops drop `x` and
/// rebind `x` to the result, so the caller relinquishes its handle either way,
/// and the in-place arm fires only at `rc == 1` where no other owner can
/// observe the mutation (the same dynamic argument as `__list_append1`).
fn match_rc_window(
    ops: &[Op],
    i: usize,
    occ: &BTreeMap<ValueId, usize>,
    def_at: &BTreeMap<ValueId, usize>,
) -> bool {
    let Op::CallFn {
        dst: Some(d),
        name,
        args,
        ..
    } = &ops[i]
    else {
        return false;
    };
    if name != "__list_concat_rc" {
        return false;
    }
    let [CallArg::Handle(x), CallArg::Handle(t)] = args.as_slice() else {
        return false;
    };
    // The next two ops: x's drop (any recursive list-drop flavor — the element
    // family decides which) and the rebind of x to the result.
    let dropped = match &ops[i + 1] {
        Op::Drop { v }
        | Op::DropListStr { v }
        | Op::DropListValue { v }
        | Op::DropListStrValue { v }
        | Op::DropListStrStr { v } => v,
        _ => return false,
    };
    let Op::SetLocal { local: x3, src: d2 } = &ops[i + 2] else {
        return false;
    };
    if dropped != x || x3 != x || d2 != d {
        return false;
    }
    // `t` must be the 1-ELEMENT temp: its defining Alloc's len feeds from a
    // ConstInt 1. Anything else is a general `x = x + ys` concat, where an
    // append-one callee would be simply wrong.
    let Some(&ti) = def_at.get(t) else {
        return false;
    };
    let Op::Alloc {
        init: crate::Init::DynListStr { len },
        ..
    } = &ops[ti]
    else {
        return false;
    };
    let Some(&li) = def_at.get(len) else {
        return false;
    };
    if !matches!(&ops[li], Op::ConstInt { value: 1, .. }) {
        return false;
    }
    // Result used only by the rebind; the temp only by its build + this call +
    // its trailing drop (Alloc def + Handle prim arg + call arg + drop = 4).
    occ.get(d).copied() == Some(2) && occ.get(t).copied() == Some(4)
}

/// The STRING self-append window (#910): `x = x + s` lowers as
/// `Alloc t ← Str(..)` (or any suffix build), `d ← __str_concat(x, t)`,
/// `Drop x`, `SetLocal x ← d` — and unlike the list windows there is no
/// element-count condition: ANY suffix appends. The rewrite is name-only,
/// exactly the rc window's shape: the temp stays (its bytes are the suffix)
/// and the callee becomes `__str_append1`, whose rc == 1 + byte-cap-headroom
/// fast path appends in place. Without this the canonical string accumulator
/// whole-copied per append: a 100k-iteration `acc = acc + "x"` loop peaked at
/// 1.27 GB and 400k died — while the LIST twin (closed by the same machinery)
/// ran flat at 20 MB.
fn match_str_window(ops: &[Op], i: usize, occ: &BTreeMap<ValueId, usize>) -> bool {
    let Op::CallFn {
        dst: Some(d),
        name,
        args,
        ..
    } = &ops[i]
    else {
        return false;
    };
    if name != "__str_concat" {
        return false;
    }
    let [CallArg::Handle(x), CallArg::Handle(_t)] = args.as_slice() else {
        return false;
    };
    let Op::Drop { v: x2 } = &ops[i + 1] else {
        return false;
    };
    let Op::SetLocal { local: x3, src: d2 } = &ops[i + 2] else {
        return false;
    };
    if x2 != x || x3 != x || d2 != d {
        return false;
    }
    // The result is used only by the rebind. (The suffix temp needs no occ
    // constraint: __str_append1 only READS it, exactly as __str_concat did,
    // and its ownership events — build + arg + trailing drop — are untouched.)
    occ.get(d).copied() == Some(2)
}

/// The CHAINED string self-append window (#1229): `x = x + s1 + s2 (+ …)` —
/// the TCO'd multi-concat accumulator (`build(n, pos + 1, acc + c0 + c1)`,
/// base64's `enc(.., acc + c0 + c1 + c2 + c3)`) lowers as a LEFT-SPINE chain
/// of `__str_concat` calls through fresh intermediates:
///
/// ```text
/// d1 ← __str_concat(x, t1)
/// d2 ← __str_concat(d1, t2)      ; …up to dk
/// Drop x
/// SetLocal x ← dk
/// …straight-line ops…
/// Drop d1 … Drop d(k-1)          ; the intermediates' trailing drops
/// ```
///
/// `match_str_window` never fires on it (the drop/rebind target `x` is not the
/// LAST call's left operand), so every iteration whole-copied the accumulator
/// once per `+`: O(n²) bytes for the loop — the 0.57.0 release-gate fuzz
/// "hang" (run 31486558420: `build(65535, 0, "")` took 21.7 s on wasm, native
/// instant; the exact-size free-list never reuses a monotonically growing
/// block, so every step also bump-allocated fresh).
///
/// The rewrite renames every call in the chain to `__str_append1` and moves
/// each consumed owner's `Drop` to DIRECTLY AFTER the call that consumed it:
///
/// ```text
/// d1 ← __str_append1(x, t1)      ; rc(x)==1 → in-place, ret Dups (rc 2)
/// Drop x                          ; rc 2 → 1: d1 is now the sole owner
/// d2 ← __str_append1(d1, t2)     ; rc(d1)==1 → in-place again
/// Drop d1                         ; …
/// SetLocal x ← dk                 ; the loop-carried slot, rc 1
/// ```
///
/// Moving the drops is what makes the chain amortized: without it the first
/// append's value-semantics return leaves rc == 2, forcing every later link
/// down the whole-copy slow path. Each moved `Drop` still sits AFTER its
/// value's last use (the very call that consumed it), so def-before-use and
/// the one-`rc_dec`-per-drop release accounting are unchanged — the op
/// multiset is identical, only the order shifts. In-place mutation stays
/// unobservable for the same dynamic reason as the single window: it fires
/// only at `rc == 1`, when the about-to-be-dropped handle is the sole owner.
/// Suffixes must be DISTINCT ValueIds from `x` and the intermediates
/// (`match_str_chain_window` rejects `acc + acc + c`): an in-place first
/// append would mutate the block a later suffix read still expects original
/// bytes from. Aliases via `Dup` (a distinct ValueId over the same block) are
/// safe dynamically — the extra owner holds rc > 1, which forces the copying
/// slow path.
struct StrChain {
    /// `(dst, left, suffix)` of each `__str_concat` in chain order. The first
    /// link's `left` is the loop-carried slot `x` (the `Drop`/`SetLocal`
    /// target — the emitter clones the window's own rebind op, so it is not
    /// carried separately).
    calls: Vec<(ValueId, ValueId, ValueId)>,
    /// Op index of each intermediate's trailing `Drop`, chain order (all past
    /// the rebind).
    inter_drop_at: Vec<usize>,
}

/// `Some((dst, left, suffix))` when `op` is `dst ← __str_concat(left, suffix)`.
fn as_str_concat(op: &Op) -> Option<(ValueId, ValueId, ValueId)> {
    let Op::CallFn { dst: Some(d), name, args, .. } = op else {
        return None;
    };
    if name != "__str_concat" {
        return None;
    }
    let [CallArg::Handle(l), CallArg::Handle(t)] = args.as_slice() else {
        return None;
    };
    Some((*d, *l, *t))
}

/// Match the chained window at `ops[i..]` (k >= 2 calls; k == 1 is
/// `match_str_window`'s). See [`StrChain`] for the shape and the safety
/// argument each check enforces.
fn match_str_chain_window(
    ops: &[Op],
    i: usize,
    occ: &BTreeMap<ValueId, usize>,
) -> Option<StrChain> {
    let (d1, x, t1) = as_str_concat(&ops[i])?;
    let mut calls = vec![(d1, x, t1)];
    while let Some((d, l, t)) = ops.get(i + calls.len()).and_then(as_str_concat) {
        if l != calls.last().expect("chain is non-empty").0 {
            break;
        }
        calls.push((d, l, t));
    }
    let k = calls.len();
    if k < 2 {
        return None;
    }
    let Some(Op::Drop { v: x2 }) = ops.get(i + k) else {
        return None;
    };
    let Some(Op::SetLocal { local: x3, src: dk }) = ops.get(i + k + 1) else {
        return None;
    };
    if *x2 != x || *x3 != x || *dk != calls[k - 1].0 {
        return None;
    }
    // Mention counts: the final result is used only by the rebind (dst +
    // SetLocal src = 2); each intermediate only by its def, the next link's
    // left arg, and its trailing drop (3). Any extra reference (including an
    // intermediate reused as a later SUFFIX) means the shape is not the pure
    // chain and the copying concat must stay.
    if occ.get(&calls[k - 1].0).copied() != Some(2) {
        return None;
    }
    if calls[..k - 1].iter().any(|(d, _, _)| occ.get(d).copied() != Some(3)) {
        return None;
    }
    // Alias guard: a suffix that IS `x` (or an intermediate) would read the
    // accumulator's block AFTER an in-place append mutated it. Distinct
    // ValueIds only — Dup-aliases are dynamically safe (rc > 1 → slow path).
    if calls.iter().any(|(_, _, t)| *t == x || calls.iter().any(|(d, _, _)| d == t)) {
        return None;
    }
    if calls.iter().any(|(d, _, _)| *d == x) {
        return None;
    }
    // Each intermediate's trailing Drop must live in the same straight-line
    // region after the rebind (same scan discipline as `match_window`).
    let mut inter_drop_at = Vec::with_capacity(k - 1);
    'inters: for (d, _, _) in &calls[..k - 1] {
        for (m, op) in ops.iter().enumerate().skip(i + k + 2).take(DROP_SCAN) {
            match op {
                Op::Drop { v } if v == d => {
                    inter_drop_at.push(m);
                    continue 'inters;
                }
                Op::IfThen { .. }
                | Op::Else { .. }
                | Op::EndIf { .. }
                | Op::LoopStart
                | Op::LoopBreakUnless { .. }
                | Op::LoopEnd => return None,
                _ => {}
            }
        }
        return None;
    }
    Some(StrChain { calls, inter_drop_at })
}

/// Rewrite every self-append concat window in `functions` to the amortized
/// O(1) append form: the scalar `__list_concat` window is REPLACED (temp
/// eliminated — see `match_window`) and the heap-element `__list_concat_rc`
/// window is RENAMED in place (temp kept — see `match_rc_window`).
pub fn rewrite_self_append(functions: &mut [MirFunction]) {
    for f in functions.iter_mut() {
        if !has_concat_call(&f.ops) {
            continue;
        }
        let (occ, def_at) = value_census(&f.ops);
        f.ops = rewrite_ops(&f.ops, &occ, &def_at);
    }
}

/// Does this function call one of the concat runtime fns this pass rewrites?
fn has_concat_call(ops: &[Op]) -> bool {
    ops.iter().any(|op| {
        matches!(op,
            Op::CallFn { name, .. }
                if name == "__list_concat" || name == "__list_concat_rc" || name == "__str_concat")
    })
}

/// Per-value occurrence counts (the single-use test every window relies on) and
/// the op index each value is DEFINED at.
fn value_census(ops: &[Op]) -> (BTreeMap<ValueId, usize>, BTreeMap<ValueId, usize>) {
    let mut occ: BTreeMap<ValueId, usize> = BTreeMap::new();
    let mut def_at: BTreeMap<ValueId, usize> = BTreeMap::new();
    let mut vals: Vec<ValueId> = Vec::new();
    for (k, op) in ops.iter().enumerate() {
        vals.clear();
        crate::render_wasm::op_values(op, &mut vals);
        for v in &vals {
            *occ.entry(*v).or_insert(0) += 1;
        }
        if let Op::Alloc { dst, .. } | Op::ConstInt { dst, .. } = op {
            def_at.insert(*dst, k);
        }
    }
    (occ, def_at)
}

/// Scan the op list left to right, replacing each recognized concat window with
/// its append form and copying everything else through.
fn rewrite_ops(
    ops: &[Op],
    occ: &BTreeMap<ValueId, usize>,
    def_at: &BTreeMap<ValueId, usize>,
) -> Vec<Op> {
    let mut out: Vec<Op> = Vec::with_capacity(ops.len());
    let mut i = 0;
    while i < ops.len() {
        if let Some(next) = push_list_append_window(ops, i, occ, &mut out) {
            i = next;
            continue;
        }
        if let Some(next) = push_str_chain_window(ops, i, occ, &mut out) {
            i = next;
            continue;
        }
        if let Some(next) = push_renamed_window(ops, i, occ, def_at, &mut out) {
            i = next;
            continue;
        }
        out.push(ops[i].clone());
        i += 1;
    }
    out
}

/// The chained string window: every link renamed to `__str_append1` with the
/// consumed owner's `Drop` moved to directly after the call that consumed it
/// (see [`StrChain`] for why the move is what unlocks the rc == 1 fast path),
/// then the rebind, then the straight-line ops up to the last intermediate
/// drop verbatim — minus those drops, which were emitted early. Same op
/// multiset, new order. Returns the next index when it fired.
fn push_str_chain_window(
    ops: &[Op],
    i: usize,
    occ: &BTreeMap<ValueId, usize>,
    out: &mut Vec<Op>,
) -> Option<usize> {
    let ch = match_str_chain_window(ops, i, occ)?;
    let k = ch.calls.len();
    for (j, (d, l, t)) in ch.calls.iter().enumerate() {
        let Op::CallFn { result, .. } = &ops[i + j] else {
            unreachable!("a matched chain link is always a CallFn")
        };
        out.push(Op::CallFn {
            dst: Some(*d),
            name: "__str_append1".to_string(),
            args: vec![CallArg::Handle(*l), CallArg::Handle(*t)],
            result: result.clone(),
        });
        // The consumed owner's drop: `Drop x` (the window's own, op i+k) for
        // the first link, the intermediate's trailing drop for the rest.
        out.push(Op::Drop { v: *l });
    }
    out.push(ops[i + k + 1].clone()); // SetLocal x ← dk
    let last = *ch.inter_drop_at.iter().max().expect("k >= 2 has intermediates");
    for m in i + k + 2..=last {
        if ch.inter_drop_at.contains(&m) {
            continue;
        }
        out.push(ops[m].clone());
    }
    Some(last + 1)
}

/// The 4-op list-concat window: emit `__list_append1`, free the old list, rebind
/// it to the result, then keep the straight-line ops up to the temp's trailing
/// drop verbatim (only the drop vanishes — the temp no longer exists). Returns
/// the next index when it fired.
fn push_list_append_window(
    ops: &[Op],
    i: usize,
    occ: &BTreeMap<ValueId, usize>,
    out: &mut Vec<Op>,
) -> Option<usize> {
    // Minimum window = the 4-op head plus the trailing drop.
    if i + WINDOW_HEAD + 1 > ops.len() {
        return None;
    }
    let (e, d, x, drop_at) = match_window(ops, i, occ)?;
    out.push(Op::CallFn {
        dst: Some(d),
        name: "__list_append1".to_string(),
        args: vec![CallArg::Handle(x), CallArg::Scalar(e)],
        result: Some(crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }),
    });
    out.push(Op::Drop { v: x });
    out.push(Op::SetLocal { local: x, src: d });
    for k in i + WINDOW_HEAD..drop_at {
        out.push(ops[k].clone());
    }
    Some(drop_at + 1)
}

/// The string and heap-element windows: both keep every op and only RENAME the
/// callee in place (see `match_str_window` / `match_rc_window`).
fn push_renamed_window(
    ops: &[Op],
    i: usize,
    occ: &BTreeMap<ValueId, usize>,
    def_at: &BTreeMap<ValueId, usize>,
    out: &mut Vec<Op>,
) -> Option<usize> {
    if i + 3 > ops.len() {
        return None;
    }
    let renamed = if match_str_window(ops, i, occ) {
        "__str_append1"
    } else if match_rc_window(ops, i, occ, def_at) {
        "__list_append1_rc"
    } else {
        return None;
    };
    let Op::CallFn { dst, args, result, .. } = ops[i].clone() else {
        unreachable!("a matched window head is always a CallFn")
    };
    out.push(Op::CallFn { dst, name: renamed.to_string(), args, result });
    Some(i + 1)
}
