//! Writable-borrow parameters on the native render (#4491).
//!
//! A `mut b: Bytes` parameter reaches the MIR as the C-132 write-back: the
//! body threads the buffer through the floor's writers (`c = bytes.
//! append_u16_le(b, x)`) and returns the last one, and every caller rebinds
//! its own buffer to the result (`r = rec(b, x); Drop b; b = r`). Borrowed
//! by the v1 convention, the first writer edits a copy of the caller's
//! buffer — a copy the standard codegen never makes, so that call walled.
//!
//! Here such a parameter becomes a WRITABLE BORROW, v0's own spelling: the
//! callee takes `&mut Vec<u8>` and edits it in place (the writers' threaded
//! results are the parameter itself, and the write-back return goes), and
//! every caller lends its own buffer at the call (`rec(&mut b, x)`) with the
//! rebind gone: after the call its binding is the same object, edited.
//!
//! The certificate states both sides (`certificate_native_calls`,
//! proofs/NativeCalls.v's writable-borrow rule): the parameter's mode is 2,
//! its line starts at 0 as a borrow's (it is never released, returned or
//! stored); each call writes its heap arguments' probes as `B` and names
//! their lines in the site's row, so the checker holds the lent buffer to
//! the caller's own unshared object, not passed twice at the call.
//!
//! In scope, by one shape: a function whose callers are all direct named call
//! sites (as for the move modes: not `main`, not taken as a value, not a
//! render-linked runtime name), with ONE heap parameter whose every use is a
//! top-level floor writer (`FloorParam::BytesMut`, called for its result) or
//! a floor read (`FloorParam::Bytes`) of the current buffer, whose last
//! writer's result is the function's return (a `-> Unit` source function),
//! and whose every call site lends a buffer of the caller's own (not a
//! parameter, not passed twice) and rebinds it to the result right after the
//! call. Anything else keeps the borrow convention. All or nothing per
//! callee, and per program when the ownership mirror refuses the result.

use crate::{CallArg, MirFunction, Op, ValueId};
use std::collections::{BTreeMap, BTreeSet};

fn values(op: &Op) -> Vec<ValueId> {
    let mut out = Vec::new();
    crate::mir_ops::op_values(op, &mut out);
    out
}

/// Nesting depth before each op (an `Else` sits at its `IfThen`'s depth).
fn depths(func: &MirFunction) -> Vec<i32> {
    let mut d = 0i32;
    func.ops
        .iter()
        .map(|op| {
            if matches!(op, Op::EndIf { .. } | Op::LoopEnd) {
                d -= 1;
            }
            let here = if matches!(op, Op::Else { .. }) { d - 1 } else { d };
            if matches!(op, Op::IfThen { .. } | Op::LoopStart) {
                d += 1;
            }
            here
        })
        .collect()
}

fn floor_param(name: &str, k: usize) -> Option<crate::native_floor::FloorParam> {
    crate::native_floor::declared(name).and_then(|f| f.params.get(k).cloned())
}

/// The position of `v` among `args`, when it is there exactly once.
fn only_position(args: &[CallArg], v: ValueId) -> Option<usize> {
    let at: Vec<usize> = args.iter().enumerate().filter(|(_, a)| matches!(a, CallArg::Handle(x) if *x == v)).map(|(k, _)| k).collect();
    match at.as_slice() {
        [k] => Some(*k),
        _ => None,
    }
}

/// A floor writer of `cur` called for its result: `dst = writer(…, cur, …)`.
fn is_writer(op: &Op, cur: ValueId) -> Option<ValueId> {
    let Op::CallFn { dst: Some(d), name, args, .. } = op else { return None };
    let k = only_position(args, cur)?;
    let floor = crate::native_floor::declared(name)?;
    (floor_param(name, k) == Some(crate::native_floor::FloorParam::BytesMut)
        && floor.ret == crate::native_floor::FloorRet::Unit)
        .then_some(*d)
}

/// A floor read of `cur` (a `Bytes` param that borrows it).
fn is_read(op: &Op, cur: ValueId) -> bool {
    let Op::CallFn { dst, name, args, .. } = op else { return false };
    only_position(args, cur).is_some_and(|k| floor_param(name, k) == Some(crate::native_floor::FloorParam::Bytes))
        && dst.is_none_or(|d| d != cur)
}

/// The writable chain of param `k` of `f`: the op indices of its writers,
/// its reads, and the releases of its threaded results.
struct Chain {
    param: ValueId,
    links: BTreeSet<ValueId>,
    writers: Vec<usize>,
    reads: Vec<usize>,
    remove: BTreeSet<usize>,
}

fn chain(f: &MirFunction, k: usize) -> Option<Chain> {
    let p = f.params.get(k)?;
    if !p.repr.is_heap() || f.owned_params.contains(&p.value) {
        return None;
    }
    let depth = depths(f);
    let (mut cur, mut links) = (p.value, BTreeSet::from([p.value]));
    let (mut writers, mut reads, mut remove) = (Vec::new(), Vec::new(), BTreeSet::new());
    for (i, op) in f.ops.iter().enumerate() {
        if matches!(op, Op::Return { .. }) {
            return None;
        }
        if !values(op).iter().any(|v| links.contains(v)) {
            continue;
        }
        if depth[i] != 0 {
            return None;
        }
        if let Some(d) = is_writer(op, cur) {
            writers.push(i);
            links.insert(d);
            cur = d;
        } else if is_read(op, cur) {
            reads.push(i);
        } else if matches!(op, Op::Drop { v } if *v != cur && *v != p.value && links.contains(v)) {
            remove.insert(i);
        } else {
            return None;
        }
    }
    (cur != p.value && f.ret == Some(cur)).then_some(Chain { param: p.value, links, writers, reads, remove })
}

/// The one writable parameter of `f`, if it has exactly one.
fn writable_param(f: &MirFunction) -> Option<(usize, Chain)> {
    let found: Vec<(usize, Chain)> = (0..f.params.len()).filter_map(|k| chain(f, k).map(|c| (k, c))).collect();
    let [_] = found.as_slice() else { return None };
    found.into_iter().next()
}

/// The ops of a call site's write-back (`r = f(b); [t = Dup r;] Drop b;
/// b = r|t; … Drop r`) that go when the call lends `b`, or `None` when the
/// site has another shape.
fn write_back(g: &MirFunction, call: usize, r: ValueId, b: ValueId, depth: &[i32]) -> Option<BTreeSet<usize>> {
    let base = depth[call];
    let mut w = BTreeSet::from([r]);
    let (mut gone, mut dropped, mut rebound) = (BTreeSet::new(), false, false);
    for (j, op) in g.ops.iter().enumerate().skip(call + 1) {
        let vs = values(op);
        if !rebound && (depth[j] < base || (depth[j] == base && matches!(op, Op::Else { .. } | Op::EndIf { .. } | Op::LoopEnd))) {
            return None;
        }
        let names_w = vs.iter().any(|v| w.contains(v));
        if !names_w && (rebound || !vs.contains(&b)) {
            continue;
        }
        match op {
            Op::Dup { dst, src } if !rebound && w.contains(src) => {
                w.insert(*dst);
            }
            Op::Drop { v } if !rebound && !dropped && *v == b => dropped = true,
            Op::SetLocal { local, src } if !rebound && dropped && *local == b && w.contains(src) && depth[j] == base => {
                rebound = true;
            }
            Op::Drop { v } if rebound && w.contains(v) => {}
            _ => return None,
        }
        gone.insert(j);
    }
    rebound.then_some(gone)
}

/// One call site of a writable callee, rewritten: the call's index, the ops
/// that go.
struct Site {
    call: usize,
    remove: BTreeSet<usize>,
}

/// Every call site of `callee` in `g`, or `None` when one cannot lend.
fn sites_in(g: &MirFunction, callee: &str, k: usize) -> Option<Vec<Site>> {
    let depth = depths(g);
    let params: BTreeSet<ValueId> = g.params.iter().map(|p| p.value).collect();
    let mut out = Vec::new();
    for (i, op) in g.ops.iter().enumerate() {
        match op {
            Op::CallFn { name, dst, args, .. } if name == callee => {
                let Some(CallArg::Handle(b)) = args.get(k) else { return None };
                let twice = args.iter().filter(|a| matches!(a, CallArg::Handle(x) if x == b)).count() != 1;
                let (Some(r), false, false) = (dst, twice, params.contains(b)) else { return None };
                out.push(Site { call: i, remove: write_back(g, i, *r, *b, &depth)? });
            }
            _ => {}
        }
    }
    Some(out)
}

/// Remove the ops at `gone` from `f`, keeping its op-index side tables on
/// the ops they name.
fn remove_ops(f: &mut MirFunction, gone: &BTreeSet<usize>) {
    let shift = |i: usize| i - gone.range(..i).count();
    f.call_moves = f.call_moves.iter().filter(|i| !gone.contains(i)).map(|&i| shift(i)).collect();
    f.call_mut_borrows = f.call_mut_borrows.iter().filter(|(i, _)| !gone.contains(i)).map(|&(i, k)| (shift(i), k)).collect();
    let mut at = 0;
    f.ops.retain(|_| {
        let keep = !gone.contains(&at);
        at += 1;
        keep
    });
}

/// The callee's body on its writable parameter: every writer edits the
/// parameter itself and binds nothing; the threaded results' releases and
/// the write-back return go.
fn rewrite_callee(f: &mut MirFunction, c: &Chain) {
    for &i in c.writers.iter().chain(&c.reads) {
        let writer = c.writers.contains(&i);
        if let Op::CallFn { dst, args, result, .. } = &mut f.ops[i] {
            for a in args.iter_mut() {
                if matches!(a, CallArg::Handle(v) if c.links.contains(v)) {
                    *a = CallArg::Handle(c.param);
                }
            }
            if writer {
                *dst = None;
                *result = None;
            }
        }
    }
    remove_ops(f, &c.remove);
    f.ret = None;
    f.mut_borrow_params.insert(c.param);
}

/// A caller's site: the call lends its buffer and binds nothing; the
/// write-back goes.
fn rewrite_site(g: &mut MirFunction, s: &Site, k: usize) {
    if let Op::CallFn { dst, result, .. } = &mut g.ops[s.call] {
        *dst = None;
        *result = None;
    }
    let call = s.call - s.remove.range(..s.call).count();
    remove_ops(g, &s.remove);
    g.call_mut_borrows.insert((call, k));
}

/// The pass. Returns the names of the functions whose return it took away
/// (the write-back), so the caller drops their declared heap result.
pub(crate) fn rewrite(functions: &mut [MirFunction]) -> Vec<String> {
    let taken: BTreeSet<String> = functions
        .iter()
        .flat_map(|f| f.ops.iter())
        .filter_map(|op| match op {
            Op::FuncRef { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    let eligible = |f: &MirFunction| f.name != "main" && !taken.contains(&f.name) && !f.name.contains('.') && !f.name.starts_with("__");
    let mut plan: BTreeMap<String, (usize, ValueId)> = BTreeMap::new();
    for f in functions.iter().filter(|f| eligible(f)) {
        let Some((k, c)) = writable_param(f) else { continue };
        if functions.iter().all(|g| sites_in(g, &f.name, k).is_some()) {
            plan.insert(f.name.clone(), (k, c.param));
        }
    }
    if plan.is_empty() {
        return Vec::new();
    }
    let before = functions.to_vec();
    if !apply(functions, &plan) || functions.iter().any(|f| crate::verify_ownership(f).is_err()) {
        functions.clone_from_slice(&before);
        return Vec::new();
    }
    plan.into_keys().collect()
}

/// Rewrite every planned callee's sites, one callee at a time on the bodies
/// as they stand, then the callees. False when a site or a chain is gone.
fn apply(functions: &mut [MirFunction], plan: &BTreeMap<String, (usize, ValueId)>) -> bool {
    for (callee, &(k, _)) in plan {
        for g in functions.iter_mut() {
            let Some(sites) = sites_in(g, callee, k) else { return false };
            // Latest first, so the earlier indices stay put.
            for s in sites.iter().rev() {
                rewrite_site(g, s, k);
            }
        }
    }
    for f in functions.iter_mut() {
        let Some(&(k, param)) = plan.get(&f.name) else { continue };
        match chain(f, k) {
            Some(c) if c.param == param => rewrite_callee(f, &c),
            _ => return false,
        }
    }
    true
}
