//! Elide dead [`Op::MakeUnique`] guards (issue #824).
//!
//! `lower_place_mutation` inserts `Op::MakeUnique { v }` before EVERY in-place
//! indexed write to a local heap value, unconditionally — it has no alias
//! analysis, so it pays a runtime `rc > 1` check (and the `rc_dec` +
//! `list_copy` it can trigger) even for a value that is provably never shared.
//! This is measurable in hot loops (e.g. `perm1[j] = perm1[j + 1]` in
//! fannkuchredux): every iteration re-checks a refcount that can only ever be 1.
//!
//! This module does NOT touch `Op::MakeUnique`'s semantics or its rendering
//! (`render_wasm_p2.rs` / the native `.clone()`-on-alias path) — both stay
//! exactly as proven (the rendering refines `CowSafety.v`). It only decides,
//! per function and PURELY from that function's own already-lowered `ops`,
//! whether a given OCCURRENCE is dead (rc can never exceed 1 at that program
//! point) and removes it. On any doubt the analysis keeps the guard — this is
//! only ever a subtraction of provably-redundant checks, never a new code path.
//!
//! # Why per-function, and why this is sound
//!
//! `lower_place_mutation` already refuses to target a borrowed PARAM (see its
//! `param_values` check) — every `Op::MakeUnique { v }` in a function's `ops`
//! therefore targets a value this function itself produced (a fresh
//! `Op::Alloc`/`Op::ListLit`, or a heap-typed call result — `CallFn`/`Call`/
//! `CallImport`/`CallIndirect` with `result: Some(heap)` are a fresh OWNED `+1`
//! at the call site, per their own doc comments, exactly like `Alloc`). And
//! module-level MUTABLE globals are scalar-only 8-byte slots (`MG_SLOT_BASE`)
//! — a heap object can never be shared through one. So `v`'s entire refcount
//! lifecycle, from the moment this function comes to own it, is visible in
//! this function's OWN flat `ops` list; nothing another function does can
//! raise `v`'s rc without this function's ops recording an event that hands
//! out a new alias (see [`step`] for the exhaustive per-op classification).
//!
//! # Why this needs a fixpoint, not one linear scan
//!
//! A naive "is `v` EVER an escaping use, anywhere in the function" set is
//! sound but too coarse: a `var xs = []; for … { list.push(xs, …) }` builds
//! `xs` through a REBIND chain — each `list.push` desugars to `xs = xs + [v]`,
//! which lowers to `CallFn("__list_concat", args: [Handle(old_xs), …])`
//! followed by `Op::SetLocal { local: xs, src: concat_result }` — the SAME
//! `ValueId` (`xs`'s stable local slot) is reused for the REST of the
//! function, including any later in-place mutation loop. The `Handle(old_xs)`
//! escape is real, but it only implicates the OLD incarnation of that slot;
//! `SetLocal` rebinds the slot to a value (`concat_result`) that was never
//! itself escaped. Treating the slot as escaped for its ENTIRE lifetime (as a
//! single flat scan does) would keep every later `MakeUnique` on it too,
//! missing exactly the fannkuchredux `perm1`/`fac_digits`/`perm`/`count` case.
//!
//! So [`step`] treats `Op::SetLocal { local, src }` as a RESET: `local`'s
//! membership in the escaped set becomes whatever `src`'s CURRENT membership
//! is (add OR remove) — a rebind makes `local` denote a fresh identity, and
//! its pre-rebind escape history belongs to a now-dead value.
//!
//! A reset makes the analysis POSITION-SENSITIVE, and a loop body's ops are
//! stored ONCE in the flat list even though they run N times — a naive single
//! forward pass could see a `MakeUnique` BEFORE (textually) an escape that,
//! on iteration 2+, actually already happened (via the iteration-1 tail
//! wrapping around to iteration 2's head). The fix is a small LEAST FIXPOINT
//! computation: `step` is MONOTONE in the escaped set (every arm either adds
//! fixed elements, or — for `SetLocal` — copies `src`'s current membership,
//! which preserves `⊆` by a straightforward induction), so the sequence
//! `E₀ = ∅, Eₙ₊₁ = pass(Eₙ)` (`pass` = one linear scan applying `step` to
//! every op in order) is non-decreasing and MUST stabilize within
//! `|distinct ValueIds|` iterations (standard finite-lattice dataflow
//! convergence — the same argument reaching-definitions/may-alias analyses
//! use). The converged `E*` (`pass(E*) == E*`) is self-consistent across a
//! loop's back-edge: seeding a FINAL decision pass with `E*` correctly models
//! "this is the steady state any iteration after the first observes".
//! [`compute_fixpoint`] runs that; [`elide_unaliased_make_unique`] then does
//! the seeded decision pass and drops every `MakeUnique` found safe.
//!
//! [`step`] is an EXHAUSTIVE match over [`Op`] (no wildcard arm) so a future
//! op variant fails to compile here until it is explicitly judged safe or
//! escaping, rather than silently defaulting to "safe".

use crate::{CallArg, MirFunction, Op, ValueId};
use std::collections::HashSet;

/// First-party, self-hosted runtime constructors whose result is HAND-VERIFIED
/// (by reading `stdlib/list_concat.almd`) to be a genuinely fresh allocation —
/// `let out = prim.alloc_list(…)` followed by a raw element-by-element copy —
/// never a `Dup`/alias of either argument. This is the ONLY way a `CallFn`'s
/// heap `dst` is trusted as "born unaliased" rather than conservatively
/// escaping (see `step`'s `Op::CallFn` arm). Extending this list requires
/// re-reading the callee's OWN source and re-confirming the same property —
/// do not add a name here on the strength of its doc comment alone.
// `__list_append1` (render_wasm_p3.rs, the self-append runtime): its result is
// either (a) a freshly allocated grown copy, or (b) its own first argument,
// returned ONLY when `rc == 1` at entry — i.e. the caller's about-to-be-dropped
// handle was the object's sole owner, so after the rewrite window's `Drop x` +
// `SetLocal x ← d` no OTHER live handle can alias the result. Case (b) is the
// reason this entry is safe even though the fn can return an argument: the
// runtime rc check proves at execution time what the other entries prove
// statically (see concat_to_append.rs for the window that emits the call).
const TRUSTED_FRESH_ALLOCATORS: &[&str] =
    &["__list_concat", "__list_concat_rc", "__list_append1"];

/// One forward step of the "which values are possibly aliased right now"
/// dataflow fact, applied in place to `escaped`. See the module doc for the
/// `SetLocal` reset and the fixpoint argument this relies on.
fn step(op: &Op, escaped: &mut HashSet<ValueId>) {
    match op {
        // The canonical alias-creating op: at this op, the object has (at
        // least) TWO live owners — `src`'s existing reference AND `dst`'s new
        // one. Taint BOTH: trusting `dst` would require proving `src`'s own
        // reference is dropped before any later use of `dst`, which this pass
        // does not model (e.g. `Dup { dst: v_speeds, src: borrowed }` off a
        // module-global `PrimKind::LoadHandle` load — `borrowed` is used once
        // and never again, but the TRUE other owner is the global SLOT
        // itself, invisible to a same-function scan; C-033's
        // `module_var_alias_cow` fixture pins this: `v_speeds` must stay
        // COW-guarded even though nothing in ITS OWN function re-touches it).
        Op::Dup { dst, src } => {
            escaped.insert(*src);
            escaped.insert(*dst);
        }
        // A heap `Handle` arg is a BORROW at this call site, but the callee is
        // opaque — it may `Dup` it and leak the alias past the call's return
        // (e.g. `fn keep(x) = dup(x)` returned to a DIFFERENT local than `v`).
        // And a heap-returning call's `dst` is a FRESH owned value ONLY as far
        // as the callee's own certificate accounting goes — that says nothing
        // about whether the callee internally `Dup`'d one of ITS OWN (opaque)
        // borrowed args and returned that (the SAME `keep`-shape risk), which
        // WOULD make `dst` a genuine alias of an arg this same op just marked
        // escaping. So `dst` escapes too, UNLESS the callee is a `CallFn` in
        // `TRUSTED_FRESH_ALLOCATORS` — hand-verified (by reading their
        // `stdlib/*.almd` source) to `prim.alloc_list` + raw-copy their
        // result, never `dup`-and-return an argument. `Call`/`CallImport`/
        // `CallIndirect` have no such whitelist (a closure callee especially
        // is UNANALYZABLE — see `Op::CallIndirect`'s own doc comment).
        Op::CallFn { dst, name, args, .. } => {
            for a in args {
                if let CallArg::Handle(id) = a {
                    escaped.insert(*id);
                }
            }
            if let Some(d) = dst {
                if !TRUSTED_FRESH_ALLOCATORS.contains(&name.as_str()) {
                    escaped.insert(*d);
                }
            }
        }
        Op::Call { dst, args, .. }
        | Op::CallImport { dst, args, .. }
        | Op::CallIndirect { dst, args, .. } => {
            for a in args {
                if let CallArg::Handle(id) = a {
                    escaped.insert(*id);
                }
            }
            if let Some(d) = dst {
                escaped.insert(*d);
            }
        }
        // Scalar-element list literals only (see the op's own doc comment) —
        // a heap-typed value cannot legally appear in `elems`, but one that
        // does is, by construction, moved INTO a fresh container: treat it as
        // escaping defensively (costs nothing — it is never a heap `v`).
        Op::ListLit { elems, .. } => {
            escaped.extend(elems.iter().copied());
        }
        // A value flowing out of an `if` arm as the branch-merge result. The
        // merge/ownership discipline here is NOT re-derived by this pass —
        // conservatively treat it as escaping rather than assume move-semantics.
        Op::Else { val: Some(v) } | Op::EndIf { val: Some(v) } => {
            escaped.insert(*v);
        }
        // A rebind: `local` now denotes `src`'s identity going forward, so its
        // membership is RESET (not unioned) to `src`'s CURRENT membership —
        // see the module doc's "why a fixpoint" section.
        Op::SetLocal { local, src } => {
            if escaped.contains(src) {
                escaped.insert(*local);
            } else {
                escaped.remove(local);
            }
        }
        // Defines only (no use of an existing value's identity).
        Op::Alloc { .. } | Op::Const { .. } | Op::ConstInt { .. } | Op::FuncRef { .. } => {}
        // Releases (−1) and moves (out, no alias created) — never a share.
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
        | Op::Consume { .. } => {}
        // `PrimKind::LoadHandle` reads "the heap handle CURRENTLY stored at
        // this address" — a record field, a heap-typed list/Option/Result
        // slot, anything reached via `Handle`+`IntBinOp`+`LoadHandle`. This is
        // how field/element access is lowered (no dedicated "read a heap
        // field" op), and it is NOT the same value that any Dup targeted: a
        // `Dup`'d handle gets `Store`d into the container, then a LATER
        // `LoadHandle` from that same address materializes a BRAND NEW
        // `ValueId` with no recorded relationship to the Dup that put it
        // there. So the store→load round-trip silently erases this pass's
        // SSA-level tracking — `dst` must be tainted, unconditionally, same
        // as an opaque call's result (this pass cannot see whether the slot
        // it reads was ever the target of a Dup, in THIS function or another
        // one — record fields are exactly module-global slots' problem again,
        // just via a container instead of `MG_SLOT_BASE`). Caught by
        // `spec/lang/rccow_value_semantics_test.almd` ("record copy shields
        // the original's bytes field": `var p2 = p1` Dup+field-copies `buf`
        // into a fresh record, and reading `p2.buf` back via `LoadHandle`
        // before `bytes.set_at` was wrongly treated as never-escaped).
        Op::Prim { kind: crate::PrimKind::LoadHandle, dst: Some(d), .. } => {
            escaped.insert(*d);
        }
        // Explicitly refcount-neutral reads (own doc comments: "no refcount
        // change" / "ownership-NEUTRAL" / "BORROWS its inputs").
        Op::Borrow { .. }
        | Op::MakeUnique { .. }
        | Op::Pure { .. }
        | Op::ListGetScalar { .. }
        | Op::ListSetScalar { .. }
        | Op::Prim { .. } => {}
        // Scalar-only.
        Op::IntBinOp { .. }
        | Op::IfThen { .. }
        | Op::Else { val: None }
        | Op::EndIf { val: None }
        | Op::LoopStart
        | Op::LoopBreakUnless { .. }
        | Op::LoopEnd => {}
    }
}

/// One linear pass over `ops`, applying [`step`] in order.
fn pass(ops: &[Op], escaped: &mut HashSet<ValueId>) {
    for op in ops {
        step(op, escaped);
    }
}

/// The least fixpoint of `pass` starting from `∅` — see the module doc.
/// Bounded defensively: `step` can only ever add a `ValueId` that already
/// appears somewhere in `ops`, so the set cannot grow past that count: if
/// convergence takes longer than that many rounds, something violates the
/// monotonicity this relies on — bail to the empty (fully conservative, keep
/// every `MakeUnique`) set rather than loop or guess.
fn compute_fixpoint(ops: &[Op]) -> HashSet<ValueId> {
    let cap = ops.len().saturating_add(1);
    let mut escaped: HashSet<ValueId> = HashSet::new();
    for _ in 0..cap {
        let mut next = escaped.clone();
        pass(ops, &mut next);
        if next == escaped {
            return escaped;
        }
        escaped = next;
    }
    HashSet::new()
}

/// Remove every `Op::MakeUnique { v }` in `functions` whose `v` is provably
/// never aliased at that point (per the fixpoint computed by
/// [`compute_fixpoint`]) — the guard can only ever no-op there.
pub fn elide_unaliased_make_unique(functions: &mut [MirFunction]) {
    for func in functions {
        let mut state = compute_fixpoint(&func.ops);
        let keep: Vec<bool> = func
            .ops
            .iter()
            .map(|op| {
                let safe_to_elide = matches!(op, Op::MakeUnique { v } if !state.contains(v));
                step(op, &mut state);
                !safe_to_elide
            })
            .collect();
        let mut it = keep.into_iter();
        func.ops.retain(|_| it.next().expect("keep has exactly func.ops.len() entries: it was built via .map() over the same ops"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Init, Repr, PLACEHOLDER_LAYOUT};

    fn v(n: u32) -> ValueId {
        ValueId(n)
    }

    fn base_func(ops: Vec<Op>) -> MirFunction {
        MirFunction { name: "f".to_string(), params: vec![], ops, ..Default::default() }
    }

    fn alloc_list(dst: ValueId) -> Op {
        Op::Alloc { dst, repr: Repr::Ptr { layout: PLACEHOLDER_LAYOUT }, init: Init::Opaque }
    }

    fn make_unique_count(f: &MirFunction) -> usize {
        f.ops.iter().filter(|op| matches!(op, Op::MakeUnique { .. })).count()
    }

    #[test]
    fn elides_makeunique_on_a_local_that_never_escapes() {
        let mut fs = vec![base_func(vec![
            alloc_list(v(1)),
            Op::LoopStart,
            Op::MakeUnique { v: v(1) },
            Op::ListSetScalar { list: v(1), idx: v(2), val: v(3) },
            Op::LoopEnd,
            Op::Drop { v: v(1) },
        ])];
        elide_unaliased_make_unique(&mut fs);
        assert_eq!(make_unique_count(&fs[0]), 0);
    }

    #[test]
    fn keeps_makeunique_when_the_value_is_dup_aliased() {
        let mut fs = vec![base_func(vec![
            alloc_list(v(1)),
            Op::Dup { dst: v(4), src: v(1) },
            Op::MakeUnique { v: v(1) },
            Op::ListSetScalar { list: v(1), idx: v(2), val: v(3) },
            Op::Drop { v: v(1) },
            Op::Drop { v: v(4) },
        ])];
        elide_unaliased_make_unique(&mut fs);
        assert_eq!(make_unique_count(&fs[0]), 1);
    }

    #[test]
    fn keeps_makeunique_when_the_value_is_passed_to_a_call() {
        let mut fs = vec![base_func(vec![
            alloc_list(v(1)),
            Op::CallFn {
                dst: None,
                name: "some_helper".to_string(),
                args: vec![CallArg::Handle(v(1))],
                result: None,
            },
            Op::MakeUnique { v: v(1) },
            Op::ListSetScalar { list: v(1), idx: v(2), val: v(3) },
            Op::Drop { v: v(1) },
        ])];
        elide_unaliased_make_unique(&mut fs);
        assert_eq!(make_unique_count(&fs[0]), 1);
    }

    #[test]
    fn keeps_makeunique_when_the_value_flows_out_of_a_branch_merge() {
        let mut fs = vec![base_func(vec![
            alloc_list(v(1)),
            Op::IfThen { cond: v(2), dst: Some(v(5)) },
            Op::Else { val: Some(v(1)) },
            Op::EndIf { val: Some(v(1)) },
            Op::MakeUnique { v: v(1) },
            Op::ListSetScalar { list: v(1), idx: v(2), val: v(3) },
        ])];
        elide_unaliased_make_unique(&mut fs);
        assert_eq!(make_unique_count(&fs[0]), 1);
    }

    /// The fannkuchredux `perm1` shape: `var xs = []; for … { xs = xs + [i] }`
    /// (a `CallFn("__list_concat", [Handle(old), …])` + `SetLocal{xs, result}`
    /// rebind chain, per-iteration-flat), THEN a later loop mutates `xs` in
    /// place with no further escape. The rebind must be recognized as
    /// starting a FRESH, never-escaped identity for `xs`.
    #[test]
    fn elides_makeunique_after_a_build_loop_that_rebinds_via_setlocal() {
        let mut fs = vec![base_func(vec![
            alloc_list(v(1)), // xs = []
            Op::LoopStart,
            Op::CallFn {
                dst: Some(v(9)),
                name: "__list_concat".to_string(),
                args: vec![CallArg::Handle(v(1)), CallArg::Handle(v(8))],
                result: Some(Repr::Ptr { layout: PLACEHOLDER_LAYOUT }),
            },
            Op::Drop { v: v(1) },
            Op::SetLocal { local: v(1), src: v(9) }, // xs := concat result
            Op::LoopEnd,
            // A later, unrelated mutation loop over the SAME slot v(1).
            Op::LoopStart,
            Op::MakeUnique { v: v(1) },
            Op::ListSetScalar { list: v(1), idx: v(2), val: v(3) },
            Op::LoopEnd,
        ])];
        elide_unaliased_make_unique(&mut fs);
        assert_eq!(make_unique_count(&fs[0]), 0);
    }

    /// Same shape, but the rebound value ITSELF later escapes (e.g. is passed
    /// to a call) before the mutation loop — the later `MakeUnique` must stay.
    #[test]
    fn keeps_makeunique_when_the_rebound_value_itself_later_escapes() {
        let mut fs = vec![base_func(vec![
            alloc_list(v(1)),
            Op::LoopStart,
            Op::CallFn {
                dst: Some(v(9)),
                name: "__list_concat".to_string(),
                args: vec![CallArg::Handle(v(1)), CallArg::Handle(v(8))],
                result: Some(Repr::Ptr { layout: PLACEHOLDER_LAYOUT }),
            },
            Op::Drop { v: v(1) },
            Op::SetLocal { local: v(1), src: v(9) },
            Op::LoopEnd,
            // v(1) (post-rebind) escapes here too.
            Op::Dup { dst: v(20), src: v(1) },
            Op::MakeUnique { v: v(1) },
            Op::ListSetScalar { list: v(1), idx: v(2), val: v(3) },
        ])];
        elide_unaliased_make_unique(&mut fs);
        assert_eq!(make_unique_count(&fs[0]), 1);
    }

    /// The C-033 `module_var_alias_cow` regression this pass shipped with:
    /// `Dup { dst: v_speeds, src: borrowed }` off a module-global handle load,
    /// where `borrowed` itself is never touched again in THIS function. Naive
    /// "only `src` escapes" would elide `v_speeds`'s later `MakeUnique`,
    /// corrupting a live alias held by another function's `let snap = speeds`
    /// (native: "9 2.5", buggy wasm: "9 9"). `dst` must be tainted too.
    #[test]
    fn keeps_makeunique_on_a_dup_dst_even_when_the_src_is_never_reused() {
        let mut fs = vec![base_func(vec![
            Op::Prim {
                kind: crate::PrimKind::LoadHandle,
                dst: Some(v(1)),
                args: vec![v(0)],
            },
            Op::Dup { dst: v(2), src: v(1) }, // v(1) never referenced again
            Op::MakeUnique { v: v(2) },
            Op::ListSetScalar { list: v(2), idx: v(3), val: v(4) },
        ])];
        elide_unaliased_make_unique(&mut fs);
        assert_eq!(make_unique_count(&fs[0]), 1);
    }

    /// A heap-returning call to a NON-whitelisted (opaque) function must taint
    /// its `dst` too — the callee could be `fn keep(x) = dup(x)`, returning an
    /// alias of one of its own (already-escaping) arguments.
    #[test]
    fn keeps_makeunique_on_the_result_of_an_untrusted_heap_returning_call() {
        let mut fs = vec![base_func(vec![
            alloc_list(v(1)),
            Op::CallFn {
                dst: Some(v(9)),
                name: "some_user_fn".to_string(),
                args: vec![CallArg::Handle(v(1))],
                result: Some(Repr::Ptr { layout: PLACEHOLDER_LAYOUT }),
            },
            Op::MakeUnique { v: v(9) },
            Op::ListSetScalar { list: v(9), idx: v(2), val: v(3) },
        ])];
        elide_unaliased_make_unique(&mut fs);
        assert_eq!(make_unique_count(&fs[0]), 1);
    }

    /// The `rccow_value_semantics_test.almd` regression this pass shipped
    /// with ("record copy shields the original's bytes field"): `var p2 = p1`
    /// Dups the source record, field-copies (Dup + Store) each field into a
    /// FRESH record, then later reads a field back via `LoadHandle` before
    /// mutating it — the `LoadHandle` result is a brand-new `ValueId` with no
    /// recorded link to the `Dup` that populated the slot, so it must be
    /// tainted unconditionally, not treated as a fresh, never-escaped value.
    #[test]
    fn keeps_makeunique_on_a_field_value_read_back_via_loadhandle() {
        let mut fs = vec![base_func(vec![
            alloc_list(v(1)), // the original record's buf field (e.g. Bytes)
            Op::Dup { dst: v(2), src: v(1) }, // field-copy Dup into the new record
            Op::Prim {
                kind: crate::PrimKind::Store { width: 8 },
                dst: None,
                args: vec![v(3) /* new record slot addr */, v(2)],
            },
            Op::Consume { v: v(2) },
            // later: read the field back out of the new record
            Op::Prim {
                kind: crate::PrimKind::LoadHandle,
                dst: Some(v(10)),
                args: vec![v(3)],
            },
            Op::MakeUnique { v: v(10) },
            Op::ListSetScalar { list: v(10), idx: v(4), val: v(5) },
        ])];
        elide_unaliased_make_unique(&mut fs);
        assert_eq!(make_unique_count(&fs[0]), 1);
    }
}
