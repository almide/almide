//! Per-parameter move modes on the native render (#4006, class P).
//!
//! The v1 calling convention borrows every heap parameter: the caller keeps
//! its reference, and a body that returns or stores a parameter first copies
//! it (`Dup`, a `.clone()` on this render). `keep(xs) = xs` therefore copied
//! the whole list on every call where the standard codegen moves it.
//!
//! Here a heap parameter becomes a MOVE parameter when the only thing its body
//! does with it is take its own reference, once, at the top level of the body
//! (`Dup { dst, src: p }`, no other op naming `p`). The `Dup` goes and the
//! parameter IS `dst`: it enters holding the reference the caller hands over.
//!
//! Every call to such a function hands that reference over at the call: the
//! argument's `Consume` right after the call (the call-site move, recorded in
//! [`MirFunction::call_moves`]). The argument is the caller's own value when
//! its next occurrence after the call is its release in the same block
//! (that release goes: the callee takes it over); otherwise the caller copies
//! it first (`Dup` before the call, the copy moved) — the copy the callee
//! used to make, now made at the one site that needs it.
//!
//! The certificate states both sides: the callee's parameter line starts at
//! the handed-over reference, the caller's line moves the argument out at the
//! call (`M`), and `proofs/NativeCalls.v` binds every moved argument of every
//! site to that event (`certificate_native_calls`). A function or a site the
//! ownership mirror refuses after the rewrite keeps the borrow convention.
//!
//! Out of scope, by one rule — a function whose callers are not all its
//! direct named call sites keeps borrowing: `main`, a function taken as a
//! value (`FuncRef`, so a closure call may reach it), and the render-linked
//! runtime names (dotted or `__`-prefixed).

use crate::{MirFunction, Op, ValueId};
use std::collections::{BTreeMap, BTreeSet};

fn values(op: &Op) -> Vec<ValueId> {
    let mut out = Vec::new();
    crate::mir_ops::op_values(op, &mut out);
    out
}

pub(crate) fn is_release(op: &Op, v: ValueId) -> bool {
    crate::render_native_move::bookkeeping(op)
        && !matches!(op, Op::Consume { .. } | Op::Borrow { .. } | Op::MakeUnique { .. })
        && values(op) == [v]
}

/// Nesting depth before each op (an `Else` sits at its `IfThen`'s depth).
fn depths(func: &MirFunction) -> Vec<i32> {
    let mut d = 0i32;
    func.ops
        .iter()
        .map(|op| {
            match op {
                Op::EndIf { .. } | Op::LoopEnd => d -= 1,
                _ => {}
            }
            let here = if matches!(op, Op::Else { .. }) { d - 1 } else { d };
            if matches!(op, Op::IfThen { .. } | Op::LoopStart) {
                d += 1;
            }
            here
        })
        .collect()
}

/// The parameters of `func` that can be moved in: (param index, the `Dup` op
/// index), where `takes(k)` holds: every call site hands a heap handle at that
/// position. The copy may be rebound (a `mut` param's write-back, `out =
/// emit(out)`): taken over, the param IS that slot, and a copy that feeds it
/// after a call moved it out is certified as the share-out of the copy's
/// source (`certificate_b.rs` `dup_step`), never as an alias on the slot's
/// own line.
fn movable_params(func: &MirFunction, takes: &dyn Fn(usize) -> bool) -> Vec<(usize, usize)> {
    let depth = depths(func);
    let mut out = Vec::new();
    for (k, p) in func.params.iter().enumerate() {
        if !p.repr.is_heap() {
            continue;
        }
        let named: Vec<usize> =
            func.ops.iter().enumerate().filter(|(_, op)| values(op).contains(&p.value)).map(|(i, _)| i).collect();
        let [at] = named.as_slice() else { continue };
        let Op::Dup { src, .. } = func.ops[*at] else { continue };
        if depth[*at] == 0 && src == p.value && takes(k) {
            out.push((k, *at));
        }
    }
    out
}

/// The parameters of `func` its body consumes (P step 2): every op naming the
/// param sits at the top level of the body, and the last one is a
/// concatenation with the param on its left — the list or string the body
/// grows (`list.push(ys, x)` on a `mut` param lowers to `ys + [x]`). Taken
/// over, the param is released right after that concatenation, so the
/// render grows the caller's own buffer in place (`last_use`) instead of
/// copying it into a new one. A list of heap elements counts too (the `_rc`
/// spelling of `+`, #4503): the loop-converted self tail call over a
/// `List[List[Int]]` carry starts its slot as `acc + []`, which copied every
/// element list per call where the standard codegen moves the argument in.
/// Returns (param index, the concatenation's op index).
fn consumed_params(func: &MirFunction, takes: &dyn Fn(usize) -> bool) -> Vec<(usize, usize)> {
    let depth = depths(func);
    let mut out = Vec::new();
    for (k, p) in func.params.iter().enumerate() {
        if !p.repr.is_heap() || !takes(k) {
            continue;
        }
        let named: Vec<usize> =
            func.ops.iter().enumerate().filter(|(_, op)| values(op).contains(&p.value)).map(|(i, _)| i).collect();
        let Some(&last) = named.last() else { continue };
        let at_top = named.iter().all(|&i| depth[i] == 0);
        // Or a floor call that takes it by value there (`map.set(m, k, v)`
        // on a `mut` param): the body's last use gives it away.
        let grows = matches!(&func.ops[last], Op::CallFn { name, args, .. }
            if (((crate::native_list_ops::is_list_concat(name) || name == "__str_concat")
                && matches!(args.first(), Some(crate::CallArg::Handle(v)) if *v == p.value))
                || args.iter().position(|a| matches!(a, crate::CallArg::Handle(v) if *v == p.value))
                    .is_some_and(|k| crate::native_floor::takes_by_value(name, k)))
                && values(&func.ops[last]).iter().filter(|x| **x == p.value).count() == 1);
        let plain = named.iter().all(|&i| !matches!(func.ops[i], Op::Dup { .. } | Op::SetLocal { .. }));
        let early_exit = func.ops[..last].iter().any(|op| matches!(op, Op::Return { .. }));
        if at_top && grows && plain && !early_exit && func.ret != Some(p.value) {
            out.push((k, last));
        }
    }
    out
}

/// Make the movable parameters of `func` move parameters, if the ownership
/// mirror accepts the result.
fn own_params(func: &MirFunction, takes: &dyn Fn(usize) -> bool) -> Option<MirFunction> {
    let movable = movable_params(func, takes);
    let consumed: Vec<(usize, usize)> = consumed_params(func, takes)
        .into_iter()
        .filter(|(k, _)| !movable.iter().any(|m| m.0 == *k))
        .collect();
    if movable.is_empty() && consumed.is_empty() {
        return None;
    }
    let mut f = func.clone();
    // The consumed params' releases, right after each last use, inserted
    // latest first so the earlier indices stay put.
    let mut releases: Vec<(usize, crate::ValueId)> =
        consumed.iter().map(|&(k, at)| (at, func.params[k].value)).collect();
    releases.sort_unstable_by_key(|r| std::cmp::Reverse(r.0));
    for &(at, p) in &releases {
        f.ops.insert(at + 1, Op::Drop { v: p });
        f.owned_params.insert(p);
    }
    // A movable param's Dup index shifts by the releases inserted before it.
    let movable: Vec<(usize, usize)> = movable
        .into_iter()
        .map(|(k, at)| (k, at + releases.iter().filter(|(r, _)| *r < at).count()))
        .collect();
    for &(k, at) in &movable {
        let Op::Dup { dst, .. } = f.ops[at] else { unreachable!("a movable param's Dup") };
        f.params[k].value = dst;
        f.owned_params.insert(dst);
    }
    let mut at: Vec<usize> = movable.iter().map(|m| m.1).collect();
    at.sort_unstable();
    for i in at.into_iter().rev() {
        f.ops.remove(i);
    }
    crate::verify_ownership(&f).is_ok().then_some(f)
}

/// Whether the caller can move `v` into the call at `call` without a copy:
/// its next occurrence is its release in the same block, and nothing derived
/// from it is read before that.
fn release_after(func: &MirFunction, call: usize, v: ValueId, depth: &[i32]) -> Option<usize> {
    let ops = &func.ops;
    if values(&ops[call]).iter().filter(|x| **x == v).count() != 1 {
        return None;
    }
    // Values read off `v` before the call (an address, a loaded child).
    let mut derived: BTreeSet<ValueId> = [v].into_iter().collect();
    for op in &ops[..call] {
        if crate::render_native_move::bookkeeping(op) {
            continue;
        }
        if let Some(d) = crate::mir_ops::defined_value(op) {
            let vs = values(op);
            if vs.iter().any(|x| *x != d && derived.contains(x)) {
                derived.insert(d);
            }
        }
    }
    derived.remove(&v);
    let base = depth[call];
    for j in call + 1..ops.len() {
        if depth[j] < base || (depth[j] == base && matches!(ops[j], Op::Else { .. } | Op::EndIf { .. } | Op::LoopEnd)) {
            return None;
        }
        if matches!(ops[j], Op::Return { .. } | Op::LoopBreakUnless { .. }) {
            return None;
        }
        let vs = values(&ops[j]);
        if vs.iter().any(|x| derived.contains(x)) {
            return None;
        }
        if vs.contains(&v) {
            return (depth[j] == base && is_release(&ops[j], v)).then_some(j);
        }
    }
    None
}

fn next_value(func: &MirFunction) -> u32 {
    let mut max = func.params.iter().map(|p| p.value.0).max().unwrap_or(0);
    for op in &func.ops {
        for v in values(op) {
            max = max.max(v.0);
        }
    }
    max + 1
}

/// The `IfThen` and the `Else` (if any) of the region the `EndIf` at `end`
/// closes.
fn region_of(ops: &[Op], end: usize) -> Option<(usize, Option<usize>)> {
    let mut depth = 0i32;
    let mut els = None;
    for k in (0..=end).rev() {
        match ops[k] {
            Op::EndIf { .. } => depth += 1,
            Op::Else { .. } if depth == 1 => els = Some(k),
            Op::IfThen { .. } => {
                depth -= 1;
                if depth == 0 {
                    return Some((k, els));
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether `ops` hands `s` to a move position of a program function.
fn hands_over(ops: &[Op], s: ValueId, owned: &BTreeMap<String, Vec<bool>>) -> bool {
    ops.iter().any(|op| match op {
        Op::CallFn { name, args, .. } => owned.get(name.as_str()).is_some_and(|modes| {
            args.iter()
                .enumerate()
                .any(|(k, a)| matches!(a, crate::CallArg::Handle(v) if *v == s) && modes.get(k).copied().unwrap_or(false))
        }),
        _ => false,
    })
}

/// Sink a join's release into the arms it joins: `if … { … } else { … };
/// Drop s; s = merge` — a `mut` slot's write-back after a `match` whose arms
/// hand `s` to a call (`38 => emit_str(out, "&amp;")`) — becomes the same
/// release at the end of each arm. The release then follows the call in the
/// call's own block, so the call takes the slot over (`release_after`)
/// instead of copying it. The arm's last op writes the slot's current
/// binding either way, so the rewrite releases exactly what the join did.
/// Nested regions sink in turn (a release that lands right after an inner
/// `EndIf` whose merge is the arm's value). Not across an arm that exits,
/// breaks, or yields `s` itself.
fn sink_join_releases(func: &MirFunction, owned: &BTreeMap<String, Vec<bool>>) -> MirFunction {
    let mut f = func.clone();
    'again: loop {
        for j in 1..f.ops.len() {
            let Op::Drop { v: s } = f.ops[j] else { continue };
            if !matches!(f.ops[j - 1], Op::EndIf { .. }) {
                continue;
            }
            let Some((start, Some(els))) = region_of(&f.ops, j - 1) else { continue };
            let Op::IfThen { dst: Some(merge), .. } = f.ops[start] else { continue };
            let joins = match f.ops.get(j + 1) {
                Some(Op::SetLocal { local, src }) => *local == s && *src == merge,
                Some(Op::Else { val } | Op::EndIf { val }) => *val == Some(merge),
                _ => false,
            };
            let region = &f.ops[start..j];
            let exits = region.iter().any(|op| matches!(op, Op::Return { .. } | Op::LoopBreakUnless { .. }));
            let yields_s = [els, j - 1].iter().any(|&k| matches!(f.ops[k], Op::Else { val } | Op::EndIf { val } if val == Some(s)));
            if !joins || exits || yields_s || !hands_over(region, s, owned) {
                continue;
            }
            f.ops.remove(j);
            f.ops.insert(j - 1, Op::Drop { v: s });
            f.ops.insert(els, Op::Drop { v: s });
            continue 'again;
        }
        return f;
    }
}

/// Hand every owned-parameter argument of `func`'s calls over at the call.
fn move_at_sites(func: &MirFunction, owned: &BTreeMap<String, Vec<bool>>) -> MirFunction {
    let mut f = func.clone();
    let mut fresh = next_value(&f);
    let mut i = 0;
    while i < f.ops.len() {
        let Op::CallFn { name, args, .. } = &f.ops[i] else {
            i += 1;
            continue;
        };
        let Some(modes) = owned.get(name.as_str()) else {
            i += 1;
            continue;
        };
        let moved: Vec<(usize, ValueId)> = args
            .iter()
            .enumerate()
            .filter_map(|(k, a)| match a {
                crate::CallArg::Handle(v) if modes.get(k).copied().unwrap_or(false) => Some((k, *v)),
                _ => None,
            })
            .collect();
        let mut call = i;
        let mut consumes = Vec::new();
        for (k, v) in moved {
            let depth = depths(&f);
            match release_after(&f, call, v, &depth) {
                Some(j) => {
                    f.ops.remove(j);
                    consumes.push(v);
                }
                None => {
                    let t = ValueId(fresh);
                    fresh += 1;
                    f.ops.insert(call, Op::Dup { dst: t, src: v });
                    call += 1;
                    if let Op::CallFn { args, .. } = &mut f.ops[call] {
                        args[k] = crate::CallArg::Handle(t);
                    }
                    consumes.push(t);
                }
            }
        }
        for (n, v) in consumes.into_iter().enumerate() {
            f.ops.insert(call + 1 + n, Op::Consume { v });
            f.call_moves.insert(call + 1 + n);
        }
        i = call + 1;
    }
    f
}

/// The pass: move parameters where the body takes its parameter over, and
/// the call-site moves that feed them. All or nothing per program: a caller
/// the ownership mirror refuses after the rewrite leaves the whole program on
/// the borrow convention.
pub(crate) fn rewrite(functions: &mut [MirFunction]) {
    if crate::render_native_move::drill_no_moves() {
        return;
    }
    let taken: BTreeSet<String> = functions
        .iter()
        .flat_map(|f| f.ops.iter())
        .filter_map(|op| match op {
            Op::FuncRef { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    let eligible = |f: &MirFunction| {
        f.name != "main" && !taken.contains(&f.name) && !f.name.contains('.') && !f.name.starts_with("__")
    };
    // (callee, position) pairs some call site fills with something other than
    // a heap handle: that position cannot take a handed-over reference.
    let mut not_handle: BTreeSet<(String, usize)> = BTreeSet::new();
    for op in functions.iter().flat_map(|f| f.ops.iter()) {
        if let Op::CallFn { name, args, .. } = op {
            for (k, a) in args.iter().enumerate() {
                if !matches!(a, crate::CallArg::Handle(_)) {
                    not_handle.insert((name.clone(), k));
                }
            }
        }
    }
    let mut next: Vec<MirFunction> = functions
        .iter()
        .map(|f| {
            let takes = |k: usize| !not_handle.contains(&(f.name.clone(), k));
            if eligible(f) {
                let r = own_params(f, &takes);
                if std::env::var("C1_DBG").is_ok() {
                    eprintln!("MOVEDBG fn {} movable={:?} consumed={:?} accepted={}", f.name, movable_params(f, &takes), consumed_params(f, &takes), r.is_some());
                    if r.is_none() && !movable_params(f, &takes).is_empty() {
                        let mut g = f.clone();
                        for (k, at) in movable_params(f, &takes) { if let Op::Dup { dst, .. } = g.ops[at] { g.params[k].value = dst; g.owned_params.insert(dst); } }
                        eprintln!("MOVEDBG   verify err: {:?}", crate::verify_ownership(&g).err());
                    }
                    if movable_params(f, &takes).is_empty() {
                        for p in &f.params { if p.repr.is_heap() { let named: Vec<String> = f.ops.iter().filter(|op| values(op).contains(&p.value)).map(|op| format!("{op:?}")).collect(); eprintln!("MOVEDBG   param {:?} named by {:?}", p.value, named); } }
                    }
                }
                r.unwrap_or_else(|| f.clone())
            } else { f.clone() }
        })
        .collect();
    let owned: BTreeMap<String, Vec<bool>> = next
        .iter()
        .filter(|f| !f.owned_params.is_empty())
        .map(|f| (f.name.clone(), f.params.iter().map(|p| f.owned_params.contains(&p.value)).collect()))
        .collect();
    if owned.is_empty() {
        return;
    }
    for f in next.iter_mut() {
        // The join releases sunk into the arms first, so an arm's call takes
        // the slot over; the plain body if the mirror refuses that.
        let sunk = move_at_sites(&sink_join_releases(f, &owned), &owned);
        let moved = if crate::verify_ownership(&sunk).is_ok() { sunk } else { move_at_sites(f, &owned) };
        if crate::verify_ownership(&moved).is_err() {
            if std::env::var("C1_DBG").is_ok() { eprintln!("MOVEDBG BAIL at {}: {:?}", f.name, crate::verify_ownership(&moved).err()); }
            return;
        }
        *f = moved;
    }
    functions.clone_from_slice(&next);
}
