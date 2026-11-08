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
fn consumed_params(func: &MirFunction, takes: &dyn Fn(usize) -> bool, owned: &Modes<'_>) -> Vec<(usize, usize)> {
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
                    .is_some_and(|k| crate::native_floor::takes_by_value(name, k)
                        // Or a program function whose own param there is a
                        // move param: the body hands the param on (`poke(mut
                        // b) = bytes.put_u8(b, 0, v)` over a linked body).
                        || owned.moves(&func.ops[last], k)))
                && values(&func.ops[last]).iter().filter(|x| **x == p.value).count() == 1)
            // Or a closure call whose every possible callee takes it over there.
            || matches!(&func.ops[last], Op::CallIndirect { args, .. }
                if args.iter().position(|a| matches!(a, crate::CallArg::Handle(v) if *v == p.value))
                    .is_some_and(|k| owned.moves(&func.ops[last], k))
                && values(&func.ops[last]).iter().filter(|x| **x == p.value).count() == 1);
        let plain = named.iter().all(|&i| !matches!(func.ops[i], Op::Dup { .. } | Op::SetLocal { .. }));
        let early_exit = func.ops[..last].iter().any(|op| matches!(op, Op::Return { .. }));
        if at_top && grows && plain && !early_exit && func.ret != Some(p.value) {
            out.push((k, last));
            continue;
        }
        // Or given away on each path once — copied (`if c then … else xs`'s
        // write-back copy) or handed to a by-value position (`if c then
        // grow(xs, x)`): each of those is its last use there.
        let gives = |i: usize| match &func.ops[i] {
            Op::Dup { src, .. } => *src == p.value,
            Op::CallFn { name, args, .. } => {
                values(&func.ops[i]).iter().filter(|x| **x == p.value).count() == 1
                    && args.iter().position(|a| matches!(a, crate::CallArg::Handle(v) if *v == p.value)).is_some_and(|k| {
                        crate::native_floor::takes_by_value(name, k)
                            || owned.moves(&func.ops[i], k)
                            // The list or string a concatenation grows in place.
                            || (k == 0 && (crate::native_list_ops::is_list_concat(name) || name == "__str_concat"))
                    })
            }
            Op::CallIndirect { args, .. } => {
                values(&func.ops[i]).iter().filter(|x| **x == p.value).count() == 1
                    && args
                        .iter()
                        .position(|a| matches!(a, crate::CallArg::Handle(v) if *v == p.value))
                        .is_some_and(|k| owned.moves(&func.ops[i], k))
            }
            _ => false,
        };
        // Reads before them stay reads of the param it still holds.
        let given: Vec<usize> = named.iter().copied().filter(|&i| gives(i) && !crate::mir_liveness::live_after(func, i, p.value)).collect();
        if !given.is_empty() && func.ret != Some(p.value) && crate::mir_liveness::last_use_on_every_path(func, p.value, &given) {
            out.extend(given.iter().map(|&i| (k, i)));
            continue;
        }
        // Or given away on some arms and never read again on the others
        // (`some(..) => runs + [..], none => [(c, 1)]`): taken over, it is
        // released where it dies unread, at the start of each such arm (the
        // release lands after the op before that start).
        if !given.is_empty() && func.ret != Some(p.value) {
            let dead = crate::mir_liveness::dead_arm_starts(func, p.value);
            let mut ends = given.clone();
            ends.extend(dead.iter().copied());
            if !dead.is_empty() && crate::mir_liveness::last_use_on_every_path(func, p.value, &ends) {
                out.extend(given.iter().map(|&i| (k, i)));
                out.extend(dead.iter().map(|&s| (k, s - 1)));
                continue;
            }
        }
        // Or a field of it is given away (`{ ...b, m: map.set(b.m, k, v) }`):
        // taken over, the record dies after its last read, so the field it
        // hands to a by-value position at its own last use moves out of it
        // instead of being copied. A caller that reads its argument again
        // copies the record for the call, as the standard codegen's by-value
        // convention does; one whose release follows the call hands it over.
        let exits_before = |end: usize| func.ops[..end].iter().any(|op| matches!(op, Op::Return { .. }));
        if at_top && plain && func.ret != Some(p.value) {
            if let Some(end) = field_given(func, p.value, owned).filter(|&end| depth[end] == 0 && !exits_before(end)) {
                out.push((k, end));
            }
        }
    }
    out
}

/// The last op that reads `p` or anything loaded from it, when `p` is only
/// read (its fields, its scalar slots) and a field loaded from it is handed
/// to a by-value position at that field's last use.
fn field_given(func: &MirFunction, p: ValueId, owned: &Modes<'_>) -> Option<usize> {
    let mut derived: BTreeSet<ValueId> = [p].into_iter().collect();
    let mut children: BTreeSet<ValueId> = BTreeSet::new();
    let mut end = None;
    let mut given = false;
    for (i, op) in func.ops.iter().enumerate() {
        let vs = values(op);
        if !vs.iter().any(|v| derived.contains(v)) {
            continue;
        }
        end = Some(i);
        match op {
            Op::Prim { kind: crate::PrimKind::Handle, dst: Some(d), args } if args.len() == 1 => {
                derived.insert(*d);
            }
            Op::IntBinOp { dst, op: crate::IntOp::Add, .. } => {
                derived.insert(*dst);
            }
            Op::Prim { kind: crate::PrimKind::LoadHandle, dst: Some(c), .. } => {
                derived.insert(*c);
                children.insert(*c);
            }
            Op::Prim { kind: crate::PrimKind::Load { .. }, .. } | Op::ListGetScalar { .. } => {}
            // A field copied at its own last use (a spread's other fields):
            // out of a record that dies, the copy is the field itself.
            Op::Dup { src, .. } if children.contains(src) => {
                given |= !crate::mir_liveness::live_after(func, i, *src);
            }
            Op::CallFn { name, args, .. } => {
                // A loaded field reaches a call once: given at a by-value
                // position at its own last use, else lent to a read.
                let at = args.iter().position(|a| matches!(a, crate::CallArg::Handle(v) if children.contains(v)))?;
                let crate::CallArg::Handle(c) = args[at] else { return None };
                if vs.iter().filter(|v| derived.contains(v)).count() != 1 {
                    return None;
                }
                let by_value = crate::native_floor::takes_by_value(name, at)
                    || owned.moves(op, at)
                    // The list or string a concatenation grows in place.
                    || (at == 0 && (crate::native_list_ops::is_list_concat(name) || name == "__str_concat"));
                given |= by_value && !crate::mir_liveness::live_after(func, i, c);
            }
            _ => return None,
        }
    }
    given.then_some(end).flatten()
}

/// Make the movable parameters of `func` move parameters, if the ownership
/// mirror accepts the result.
fn own_params(func: &MirFunction, takes: &dyn Fn(usize) -> bool, owned: &Modes<'_>) -> Option<MirFunction> {
    let movable = movable_params(func, takes);
    let consumed: Vec<(usize, usize)> = consumed_params(func, takes, owned)
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
fn hands_over(ops: &[Op], s: ValueId, owned: &Modes<'_>) -> bool {
    ops.iter().any(|op| match op {
        Op::CallFn { args, .. } | Op::CallIndirect { args, .. } => {
            args.iter().enumerate().any(|(k, a)| matches!(a, crate::CallArg::Handle(v) if *v == s) && owned.moves(op, k))
        }
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
fn sink_join_releases(func: &MirFunction, owned: &Modes<'_>) -> MirFunction {
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
fn move_at_sites(func: &MirFunction, owned: &Modes<'_>) -> MirFunction {
    let mut f = func.clone();
    let mut fresh = next_value(&f);
    let mut i = 0;
    while i < f.ops.len() {
        let (Op::CallFn { args, .. } | Op::CallIndirect { args, .. }) = &f.ops[i] else {
            i += 1;
            continue;
        };
        let Some(modes) = owned.of(&f.ops[i]) else {
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
                    if let Op::CallFn { args, .. } | Op::CallIndirect { args, .. } = &mut f.ops[call] {
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

/// The call modes the solver reads at a call site: each program function's
/// heap-param modes by name, and the closure table — every function some
/// `FuncRef` names, with each param's heap-ness — that a `CallIndirect` may
/// reach.
pub(crate) struct Modes<'t> {
    by_name: BTreeMap<String, Vec<bool>>,
    table: &'t [(String, Vec<bool>)],
}

impl Modes<'_> {
    /// The possible callees of a closure call: the table entries whose param
    /// shape (arity, heap-ness per slot) matches its arguments — the same
    /// over-approximation the native-calls witness rows the site against
    /// (`certificate_c::indirect_callee_indices`).
    fn possible<'a>(&'a self, args: &'a [crate::CallArg]) -> impl Iterator<Item = &'a str> + 'a {
        self.table
            .iter()
            .filter(move |(_, shape)| {
                shape.len() == args.len()
                    && shape.iter().zip(args).all(|(h, a)| *h == matches!(a, crate::CallArg::Handle(_)))
            })
            .map(|(n, _)| n.as_str())
    }

    /// The modes `op` hands its arguments over in: a call to a program
    /// function with a move param, or a closure call whose one possible
    /// callee has one. A closure call that may reach several callees lends:
    /// the native-calls witness rows the site once per possible callee and
    /// the checker binds each row's move to its own mark, while the site
    /// makes one move (proofs/NativeCalls.v `marks_ok`).
    fn of(&self, op: &Op) -> Option<Vec<bool>> {
        match op {
            Op::CallFn { name, .. } => self.by_name.get(name.as_str()).cloned(),
            Op::CallIndirect { args, .. } => match self.possible(args).collect::<Vec<_>>().as_slice() {
                [one] => self.by_name.get(*one).cloned(),
                _ => None,
            },
            _ => None,
        }
    }

    /// `op` takes its argument `k` over.
    fn moves(&self, op: &Op, k: usize) -> bool {
        self.of(op).is_some_and(|m| m.get(k).copied().unwrap_or(false))
    }

    /// The closure-table entries with a move param that a closure call of
    /// several possible callees may reach: that site lends ([`Self::of`]),
    /// and the checker refuses a lending row against a move param.
    fn disagreeing(&self, functions: &[MirFunction]) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for op in functions.iter().flat_map(|f| f.ops.iter()) {
            let Op::CallIndirect { args, .. } = op else { continue };
            let callees: Vec<&str> = self.possible(args).collect();
            if callees.len() > 1 {
                out.extend(callees.iter().filter(|c| self.by_name.contains_key(**c)).map(|c| c.to_string()));
            }
        }
        out
    }
}

/// The pass: move parameters where the body takes its parameter over, and
/// the call-site moves that feed them. All or nothing per program: a caller
/// the ownership mirror refuses after the rewrite leaves the whole program on
/// the borrow convention.
///
/// A lifted lambda (a declared closure signature, `crate::lambda_sigs`) is
/// reached only through closure calls: the `CallIndirect` sites whose one
/// possible callee it is, and the runtime callback adapters, which hand over
/// the values the runtime gives them. Its user params take part like any
/// other function's; its env (param 0) is always lent. A lambda that a
/// closure call of several possible callees may reach keeps borrowing, and
/// the modes are solved again.
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
    // The closure table: every taken function in the program, by param shape.
    let table: Vec<(String, Vec<bool>)> = functions
        .iter()
        .filter(|f| taken.contains(&f.name))
        .map(|f| (f.name.clone(), f.params.iter().map(|p| p.repr.is_heap()).collect()))
        .collect();
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
    let mut pinned: BTreeSet<String> = BTreeSet::new();
    loop {
        let closure = |f: &MirFunction| {
            taken.contains(&f.name) && crate::lambda_sigs::get(&f.name).is_some() && !pinned.contains(&f.name)
        };
        let eligible = |f: &MirFunction| {
            f.name != "main"
                && !f.name.contains('.')
                && (closure(f) || (!taken.contains(&f.name) && !f.name.starts_with("__")))
        };
        let takes_at = |f: &MirFunction, k: usize| {
            !not_handle.contains(&(f.name.clone(), k)) && !(closure(f) && k == 0)
        };
        let Some((best, owned)) = solve_modes(functions, &table, &eligible, &takes_at) else {
            // A closure call the mirror refuses to hand over at leaves every
            // closure borrowing, as before closures took part.
            let closures: Vec<String> = functions.iter().filter(|f| closure(f)).map(|f| f.name.clone()).collect();
            if closures.is_empty() {
                return;
            }
            pinned.extend(closures);
            continue;
        };
        // Only eligible closures carry modes, and a pinned one is not
        // eligible: every retry pins at least one more, so this ends.
        let disagree = owned.disagreeing(&best);
        if disagree.is_empty() {
            functions.clone_from_slice(&best);
            return;
        }
        pinned.extend(disagree);
    }
}

/// The ownership mirror finds nothing wrong in `after` that it did not find
/// in `before`, the function as it was lowered. A body the rewrite leaves
/// alone is the lowering's; a linked body whose raw slot loads the mirror
/// does not model (`LoadHandle` of an address it does not track) reads as a
/// use of an unknown value both before and after its call sites hand an
/// argument over, and the moves add no violation of their own.
fn no_new_violations(before: &MirFunction, after: &MirFunction) -> bool {
    let Err(found) = crate::verify_ownership(after) else { return true };
    if before.ops == after.ops && before.owned_params == after.owned_params {
        return true;
    }
    let known: BTreeSet<(ValueId, String)> = match crate::verify_ownership(before) {
        Ok(()) => BTreeSet::new(),
        Err(vs) => vs.iter().map(|v| (v.value, format!("{:?}", v.kind))).collect(),
    };
    found.iter().all(|v| known.contains(&(v.value, format!("{:?}", v.kind))))
}

/// The modes to a fixpoint, and the rewritten functions under them.
/// To a fixpoint: a body that hands its param to a callee's move param takes
/// it over in turn, once that callee's mode is known. From no move params up
/// first; then, for functions that hand a param to each other (`collect(xs) =
/// … step(xs)`, `step(xs) = … collect(xs)`), from every handle position down,
/// kept only if every body it leaves verifies.
fn solve_modes<'t>(
    functions: &[MirFunction],
    table: &'t [(String, Vec<bool>)],
    eligible: &dyn Fn(&MirFunction) -> bool,
    takes_at: &dyn Fn(&MirFunction, usize) -> bool,
) -> Option<(Vec<MirFunction>, Modes<'t>)> {
    let solve = |start: BTreeMap<String, Vec<bool>>, strict: bool| -> Option<(Vec<MirFunction>, Modes<'t>)> {
        let mut owned = Modes { by_name: start, table };
        let mut next: Vec<MirFunction>;
        let mut rounds = 0;
        loop {
            next = functions
                .iter()
                .map(|f| {
                    let takes = |k: usize| takes_at(f, k);
                    if eligible(f) { own_params(f, &takes, &owned).unwrap_or_else(|| f.clone()) } else { f.clone() }
                })
                .collect();
            let modes: BTreeMap<String, Vec<bool>> = next
                .iter()
                .filter(|f| !f.owned_params.is_empty())
                .map(|f| (f.name.clone(), f.params.iter().map(|p| f.owned_params.contains(&p.value)).collect()))
                .collect();
            if modes == owned.by_name {
                break;
            }
            if rounds == 8 {
                if strict {
                    return None;
                }
                break;
            }
            rounds += 1;
            owned.by_name = modes;
        }
        if owned.by_name.is_empty() {
            return Some((functions.to_vec(), owned));
        }
        for (f, before) in next.iter_mut().zip(functions) {
            // The join releases sunk into the arms first, so an arm's call takes
            // the slot over; the plain body if the mirror refuses that.
            let sunk = move_at_sites(&sink_join_releases(f, &owned), &owned);
            let moved = if no_new_violations(before, &sunk) { sunk } else { move_at_sites(f, &owned) };
            if !no_new_violations(before, &moved) {
                return None;
            }
            *f = moved;
        }
        Some((next, owned))
    };
    let up = solve(BTreeMap::new(), false)?;
    let top: BTreeMap<String, Vec<bool>> = functions
        .iter()
        .filter(|f| eligible(f))
        .map(|f| (f.name.clone(), (0..f.params.len()).map(|k| f.params[k].repr.is_heap() && takes_at(f, k)).collect()))
        .collect();
    let owned_count = |fs: &[MirFunction]| fs.iter().map(|f| f.owned_params.len()).sum::<usize>();
    Some(match solve(top, true) {
        Some(down) if owned_count(&down.0) > owned_count(&up.0) => down,
        _ => up,
    })
}
