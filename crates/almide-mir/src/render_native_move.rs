//! Last-use moves on the native render (#4006, no-perf-loss rule).
//!
//! The MIR copies a heap value with `Dup` and rebinds a slot with `SetLocal`;
//! the native render realizes both as `.clone()`, which keeps every alias its
//! own value. When the source is never read again that copy is pure cost — a
//! swap-carried list (`cur = merged` at a loop's end) copied its whole buffer
//! twice per round. Here such a copy becomes `std::mem::take(&mut src)`: the
//! destination gets the buffer and the source is left empty, which nothing
//! observes, because:
//!
//! * no op after the copy reads the source (a drop is not a read: drops erase
//!   to scope end on this render), nor is it the function's result;
//! * the source is defined in the same loop as the copy, so a later iteration
//!   re-defines it before any read — a value from outside the loop is never
//!   emptied for the next round;
//! * no borrow join (`crate::borrow_join`) holds a reference to it;
//! * its type has a `Default` to leave behind.
//!
//! Any doubt keeps the clone.

use crate::render_native::NTy;
use crate::{MirFunction, Op, ValueId};

/// Whether `op` only does ownership bookkeeping on this render (no code reads
/// its operand).
pub(crate) fn bookkeeping(op: &Op) -> bool {
    matches!(
        op,
        Op::Drop { .. }
            | Op::DropListStr { .. }
            // The typed releases erase to scope end too (render_native's drop arms).
            | Op::DropValue { .. }
            | Op::DropListValue { .. }
            | Op::DropListStrValue { .. }
            | Op::DropResultValue { .. }
            | Op::DropResultListValue { .. }
            | Op::DropResultValueInt { .. }
            | Op::DropResultListValueInt { .. }
            | Op::DropListStrStr { .. }
            | Op::DropListIntStr { .. }
            | Op::DropListStrInt { .. }
            | Op::DropResultStrInt { .. }
            | Op::DropResultListStrInt { .. }
            | Op::DropResultListStr { .. }
            | Op::DropListListStr { .. }
            | Op::DropVariant { .. }
            | Op::DropWrapperRec { .. }
            | Op::Consume { .. }
            | Op::Borrow { .. }
            | Op::MakeUnique { .. }
    )
}

/// The negative control of scripts/check-native-v1-perf.sh: with
/// `ALMIDE_NATIVE_DRILL_NO_MOVES=1` the render keeps every copy (and the
/// one-element-list push), so the gate must read the cost.
pub(crate) fn drill_no_moves() -> bool {
    almide_base::env::flag("ALMIDE_NATIVE_DRILL_NO_MOVES")
}

/// The innermost open `LoopStart` index at each op (`None` = no loop).
fn loop_of(func: &MirFunction) -> Vec<Option<usize>> {
    let mut stack: Vec<usize> = Vec::new();
    func.ops
        .iter()
        .enumerate()
        .map(|(i, op)| {
            let here = stack.last().copied();
            match op {
                Op::LoopStart => stack.push(i),
                Op::LoopEnd => {
                    stack.pop();
                }
                _ => {}
            }
            here
        })
        .collect()
}

/// Whether the copy of `src` at `ops[at]` may move it out instead.
pub(crate) fn last_use(
    func: &MirFunction,
    at: usize,
    src: ValueId,
    t: NTy,
    borrow_arms: &std::collections::BTreeSet<ValueId>,
) -> bool {
    if drill_no_moves() || !matches!(t, NTy::Str | NTy::Vec | NTy::VecStr | NTy::Rec(_) | NTy::Res | NTy::ResStr) {
        return false;
    }
    // A kind with no `Default` (a Result) moves only where its binding moves
    // as itself ([`take_expr`]): nothing is left behind to fill.
    if !crate::render_native_rec::has_default(t) && !moves_as_itself(src) {
        return false;
    }
    // Or no path after the copy reads it again (`mir_liveness`).
    dying_after(func, at, src, borrow_arms) || crate::mir_liveness::dead_after(func, at, src, borrow_arms)
}

/// Whether the value `src` is never read again after `ops[at]` (whatever its
/// type): [`last_use`]'s rule without the moved value's own `Default`.
/// `op` releases exactly `v` (a drop, not a hand-over or a borrow mark).
fn released(op: &Op, v: ValueId) -> bool {
    if matches!(op, Op::Consume { .. } | Op::Borrow { .. } | Op::MakeUnique { .. }) {
        return false;
    }
    let mut vs = Vec::new();
    crate::mir_ops::op_values(op, &mut vs);
    vs == [v]
}

pub(crate) fn dying_after(
    func: &MirFunction,
    at: usize,
    src: ValueId,
    borrow_arms: &std::collections::BTreeSet<ValueId>,
) -> bool {
    dying_after_skipping(func, at, src, borrow_arms, &Default::default())
}

/// [`dying_after`] for a record a field is moved out of at `at`: a later read
/// of one of its scalar slots (`ListGetScalar`, or the address `prim.handle`
/// takes of it and nothing reads) does not see the field, which the move
/// leaves empty, so it does not keep the record alive.
pub(crate) fn carrier_dies_after(
    func: &MirFunction,
    at: usize,
    src: ValueId,
    borrow_arms: &std::collections::BTreeSet<ValueId>,
) -> bool {
    let unread = |d: ValueId| {
        !func.ops.iter().any(|op| {
            let mut reads = Vec::new();
            crate::mir_ops::op_reads(op, &mut reads);
            reads.contains(&d)
        })
    };
    let scalar_reads: std::collections::BTreeSet<usize> = func.ops[at + 1..]
        .iter()
        .enumerate()
        .filter(|(_, op)| match op {
            Op::ListGetScalar { list, .. } => *list == src,
            Op::Prim { kind: crate::PrimKind::Handle, dst: Some(d), args } => args.as_slice() == [src] && unread(*d),
            _ => false,
        })
        .map(|(j, _)| at + 1 + j)
        .collect();
    dying_after_skipping(func, at, src, borrow_arms, &scalar_reads)
}

fn dying_after_skipping(
    func: &MirFunction,
    at: usize,
    src: ValueId,
    borrow_arms: &std::collections::BTreeSet<ValueId>,
    skip: &std::collections::BTreeSet<usize>,
) -> bool {
    if drill_no_moves() {
        return false;
    }
    // A borrowed param is the caller's; a MOVE param (#4006 class P) is ours.
    let owned = func.owned_params.contains(&src);
    if func.params.iter().any(|p| p.value == src) && !owned {
        return false;
    }
    // A move param the body rebinds (a `mut` param's write-back) is a slot
    // like any other local: it dies at its release.
    let owned_param = owned && !func.ops.iter().any(|op| matches!(op, Op::SetLocal { local, .. } if *local == src));
    let joined = func
        .ops
        .iter()
        .any(|op| matches!(op, Op::Dup { dst, src: s } if *s == src && borrow_arms.contains(dst)));
    if joined {
        return false;
    }
    if let Some(verdict) = scan_after_copy(func, at, src, owned_param, skip) {
        return verdict;
    }
    // The returned value is read at the end.
    if func.ret == Some(src) {
        return false;
    }
    let loops = loop_of(func);
    let defs: Vec<usize> = func
        .ops
        .iter()
        .enumerate()
        .filter(|(_, op)| {
            crate::mir_ops::defined_value(op) == Some(src) || matches!(op, Op::SetLocal { local, .. } if *local == src)
        })
        .map(|(i, _)| i)
        .collect();
    // Every definition precedes the copy, inside its loop (or a loop nested in
    // it), and one is in that loop itself: each round re-defines the value.
    let within = |d: usize| {
        let mut l = loops[d];
        loop {
            if l == loops[at] {
                return true;
            }
            match l {
                Some(start) => l = loops[start],
                None => return false,
            }
        }
    };
    // A move param is defined at entry, outside every loop: it may move only
    // where no loop could read it again.
    if owned_param {
        return loops[at].is_none() && defs.is_empty();
    }
    defs.iter().any(|&d| d < at && loops[d] == loops[at]) && defs.iter().all(|&d| d < at && within(d))
}

/// The forward scan of [`dying_after`] over the ops after the copy at
/// `ops[at]`: `Some(true)` = released first, `Some(false)` = read or rebound
/// first, `None` = neither before the end of the function.
fn scan_after_copy(func: &MirFunction, at: usize, src: ValueId, owned_param: bool, skip: &std::collections::BTreeSet<usize>) -> Option<bool> {
    // `src` is released, unconditionally and before anything reads it again,
    // in the copy's own block: the value dies there, whatever later rebinds
    // the slot (a loop slot's write-back `Drop v; SetLocal v = ..`). A copy
    // followed by that release is a move.
    let mut depth = 0i32;
    let mut straight = true;
    let mut reads = Vec::new();
    for (j, op) in func.ops.iter().enumerate().skip(at + 1) {
        if skip.contains(&j) {
            continue;
        }
        if straight {
            match op {
                Op::IfThen { .. } | Op::LoopStart => depth += 1,
                Op::EndIf { .. } | Op::LoopEnd => {
                    depth -= 1;
                    straight = depth >= 0;
                }
                Op::Else { .. } | Op::Return { .. } | Op::LoopBreakUnless { .. } if depth == 0 => straight = false,
                _ => {}
            }
        }
        if bookkeeping(op) {
            if straight && depth == 0 && !owned_param && released(op, src) {
                return Some(true);
            }
            continue;
        }
        reads.clear();
        crate::mir_ops::op_reads(op, &mut reads);
        if reads.contains(&src) {
            return Some(false);
        }
        if matches!(op, Op::SetLocal { local, .. } if *local == src) {
            return Some(false);
        }
    }
    None
}

/// Whether `src`, read at `ops[at]`, is rebound (`SetLocal`) later in the
/// block `at` sits in (not inside a nested `if` or loop) before any further
/// read: the rebind dominates everything after, the loop's back edge
/// included, so the read is the old value's last (`m = map.set(m, k, v)`).
fn rebound_before_read(func: &MirFunction, at: usize, src: ValueId) -> bool {
    let mut depth = 0usize;
    let mut reads = Vec::new();
    for op in &func.ops[at + 1..] {
        match op {
            Op::IfThen { .. } | Op::LoopStart => depth += 1,
            Op::EndIf { .. } | Op::LoopEnd | Op::Else { .. } if depth == 0 => return false,
            Op::EndIf { .. } | Op::LoopEnd => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            if let Op::SetLocal { local, src: s } = op {
                if *local == src {
                    return *s != src;
                }
            }
        }
        if bookkeeping(op) {
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

/// Whether a call's arg `src` at `ops[at]` may be moved into a by-value
/// runtime param: the last-use rule above, or a read the value's own rebind
/// follows before any other read. Same eligibility (owned kind with a
/// `Default`, not a param, not the result, no borrow join).
pub(crate) fn moves_at_call(
    func: &MirFunction,
    at: usize,
    src: ValueId,
    t: NTy,
    borrow_arms: &std::collections::BTreeSet<ValueId>,
) -> bool {
    if last_use(func, at, src, t, borrow_arms) {
        return true;
    }
    if drill_no_moves() || !matches!(t, NTy::Str | NTy::Vec | NTy::VecStr | NTy::Rec(_)) || !crate::render_native_rec::has_default(t) {
        return false;
    }
    // (The function's result is fine here: what it returns is the rebound
    // value, read after the rebind.)
    if func.params.iter().any(|p| p.value == src) && !func.owned_params.contains(&src) {
        return false;
    }
    let joined = func
        .ops
        .iter()
        .any(|op| matches!(op, Op::Dup { dst, src: s } if *s == src && borrow_arms.contains(dst)));
    !joined && rebound_before_read(func, at, src)
}

thread_local! {
    static MOVING: std::cell::RefCell<std::collections::BTreeSet<ValueId>> =
        const { std::cell::RefCell::new(std::collections::BTreeSet::new()) };
}

thread_local! {
    static STATIC_STRS: std::cell::RefCell<std::collections::BTreeMap<ValueId, String>> =
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// The string literals of the function being rendered: values an
/// `Alloc { init: Str }` defines, rendered as immortal static blocks, whose
/// buffer is never unique (an edit copies it first).
pub(crate) fn set_static_strs(func: &MirFunction) {
    let lits = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Alloc { dst, init: crate::Init::Str(s), .. } => Some((*dst, s.clone())),
            _ => None,
        })
        .collect();
    STATIC_STRS.with(|m| *m.borrow_mut() = lits);
}

pub(crate) fn is_static_str(v: ValueId) -> bool {
    STATIC_STRS.with(|m| m.borrow().contains_key(&v))
}

/// A string literal read as a `&str` operand (#4006, no-perf-loss): the
/// literal itself, as the standard codegen passes it, instead of the static
/// block's view (a load of its header, its length and its bytes' address).
///
/// Only for a binding the literal is all it ever holds: a slot that starts
/// at a literal and is rebound (`acc = acc + s`) holds other strings later.
pub(crate) fn static_str_operand(v: ValueId) -> Option<String> {
    if REBOUND.with(|m| m.borrow().contains(&v)) {
        return None;
    }
    STATIC_STRS.with(|m| m.borrow().get(&v).map(|s| format!("\"{}\"", almide_base::rust_lit::escape_rust_str(s))))
}

thread_local! {
    static STACK_LITS: std::cell::RefCell<std::collections::BTreeSet<ValueId>> =
        const { std::cell::RefCell::new(std::collections::BTreeSet::new()) };
}

/// Scalar list literals that live on the stack (#4006, no-perf-loss): a
/// `ListLit` read only by borrowing calls to the program's own functions,
/// as a closure call's environment, and by scalar slot reads, dropped and
/// never returned, copied or stored — a closure block or a small tuple passed
/// down. The render spells it `[i64; N]` where a `Vec` allocated and freed one
/// block per evaluation (a closure value built per recursive call read 65x
/// the standard codegen's instructions). Each of those uses borrows it as
/// `&[i64]`, which an array coerces to.
///
/// A constant int list (`Alloc { init: IntList }`: a folded literal or a
/// top-level list `let` read inside a function) under the same rule is a stack
/// array too: a lookup table indexed in a loop allocated, filled and freed its
/// `vec![…]` on every iteration, where the standard codegen reads its lazy
/// static. At most [`STACK_TABLE_MAX`] elements, so a frame stays small.
pub(crate) fn set_stack_lits(func: &MirFunction, user_fns: &std::collections::BTreeSet<&str>) {
    let mut lits: std::collections::BTreeSet<ValueId> = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::ListLit { dst, .. } => Some(*dst),
            Op::Alloc { dst, init: crate::Init::IntList(vs), .. }
                if !vs.is_empty() && vs.len() <= STACK_TABLE_MAX =>
            {
                Some(*dst)
            }
            _ => None,
        })
        .collect();
    lits.retain(|l| func.ret != Some(*l));
    // A literal handed to a move param (#4006 class P) leaves as the callee's
    // owned list, never as a borrowed array.
    for &i in &func.call_moves {
        if let Some(Op::Consume { v }) = func.ops.get(i) {
            lits.remove(v);
        }
    }
    let mut reads = Vec::new();
    for op in &func.ops {
        if bookkeeping(op) {
            continue;
        }
        let allowed: Vec<ValueId> = match op {
            Op::ListLit { dst, .. } | Op::Alloc { dst, init: crate::Init::IntList(_), .. } => vec![*dst],
            Op::CallFn { name, args, .. } if user_fns.contains(name.as_str()) => args
                .iter()
                .filter_map(|a| match a {
                    crate::CallArg::Handle(v) => Some(*v),
                    _ => None,
                })
                .collect(),
            // A list `+`'s right operand is only read as a slice.
            Op::CallFn { name, args, .. } if crate::native_list_ops::is_list_concat(name) => match args.as_slice() {
                [crate::CallArg::Handle(a), crate::CallArg::Handle(b)] if a != b => vec![*b],
                _ => vec![],
            },
            Op::CallIndirect { args, .. } => match args.first() {
                Some(crate::CallArg::Handle(env)) => vec![*env],
                _ => vec![],
            },
            Op::ListGetScalar { list, .. } => vec![*list],
            _ => vec![],
        };
        reads.clear();
        crate::mir_ops::op_values(op, &mut reads);
        for v in &reads {
            if !allowed.contains(v) {
                lits.remove(v);
            }
        }
        // A value used twice by one op (a list as its own index) is not a borrow.
        if let Op::ListGetScalar { list, idx, .. } = op {
            if list == idx {
                lits.remove(list);
            }
        }
    }
    let single = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::ListLit { dst, elems } if elems.len() == 1 && lits.contains(dst) => Some(*dst),
            _ => None,
        })
        .collect();
    STACK_SINGLES.with(|m| *m.borrow_mut() = single);
    STACK_LITS.with(|m| *m.borrow_mut() = lits);
}

thread_local! {
    static STACK_SINGLES: std::cell::RefCell<std::collections::BTreeSet<ValueId>> =
        const { std::cell::RefCell::new(std::collections::BTreeSet::new()) };
}

/// A stack literal of one element (`[x]`): a list `+` appends it as a push.
pub(crate) fn is_stack_single(v: ValueId) -> bool {
    STACK_SINGLES.with(|m| m.borrow().contains(&v))
}

/// The largest constant int list [`set_stack_lits`] puts on the stack.
pub(crate) const STACK_TABLE_MAX: usize = 256;

pub(crate) fn is_stack_lit(v: ValueId) -> bool {
    STACK_LITS.with(|m| m.borrow().contains(&v))
}

/// Record which of `ops[at]`'s call args may move (read by the floor render).
///
/// An arg the same call reads more than once (`map.merge(m, m)`, `xs + xs`)
/// moves at none of its positions (#4517): whichever position took it over,
/// another position still reads the binding in that same call, so its last
/// use is the call as a whole, not one argument of it. It is borrowed (or
/// copied) at every position.
pub(crate) fn set_moving_args(
    func: &MirFunction,
    at: usize,
    tys: &std::collections::BTreeMap<ValueId, NTy>,
    borrow_arms: &std::collections::BTreeSet<ValueId>,
) {
    let moving: std::collections::BTreeSet<ValueId> = match &func.ops[at] {
        Op::CallFn { args, .. } => {
            let handles: Vec<ValueId> = args
                .iter()
                .filter_map(|a| match a {
                    crate::CallArg::Handle(v) => Some(*v),
                    _ => None,
                })
                .collect();
            handles
                .iter()
                .copied()
                .filter(|v| handles.iter().filter(|x| *x == v).count() == 1)
                .filter(|v| tys.get(v).is_some_and(|t| moves_at_call(func, at, *v, *t, borrow_arms)))
                .collect()
        }
        _ => Default::default(),
    };
    // A rebound arg whose own rebind comes next, with only its release
    // between (the loop write-back `Drop v; SetLocal v = ..`): its binding
    // is re-initialized before any read, so it moves as itself.
    let mut reinit: std::collections::BTreeSet<ValueId> = moving
        .iter()
        .copied()
        .filter(|v| {
            func.ops[at + 1..]
                .iter()
                .find(|o| !bookkeeping(o))
                .is_some_and(|o| matches!(o, Op::SetLocal { local, .. } if local == v))
        })
        .collect();
    // Any rebound value the op reads that every later path rebinds before
    // naming it again (`mir_liveness`).
    reinit.extend(crate::mir_liveness::reinit_reads(func, at));
    REINIT.with(|m| *m.borrow_mut() = reinit);
    // A borrowed payload or field read handed to the call whose carrier dies
    // here, as do the carrier's other borrowed reads of it: the call can take
    // the payload itself, moved out of the carrier (#4006, no-perf-loss).
    let taken: std::collections::BTreeMap<ValueId, String> = match &func.ops[at] {
        Op::CallFn { args, .. } if !drill_no_moves() => args
            .iter()
            .filter_map(|a| match a {
                crate::CallArg::Handle(v) => Some(*v),
                _ => None,
            })
            .filter(|v| !moving.contains(v) && args.iter().filter(|a| matches!(a, crate::CallArg::Handle(x) if x == v)).count() == 1)
            .filter_map(|v| {
                if !matches!(tys.get(&v), Some(NTy::StrRef | NTy::VecRef | NTy::VecStrRef | NTy::RecRef(_))) {
                    return None;
                }
                let (base, take, reads) = match crate::render_native_addr::payload_take_of(v) {
                    Some((base, take)) => {
                        let reads = crate::render_native_addr::reads_of_take(&take);
                        (base, take, reads)
                    }
                    None => {
                        let (base, place) = crate::render_native_addr::field_of(v)?;
                        let reads = crate::render_native_addr::reads_of_place(&place);
                        (base, format!("std::mem::take(&mut {place})"), reads)
                    }
                };
                let ok = matches!(tys.get(&base), Some(NTy::Rec(_)))
                    && carrier_dies_after(func, at, base, borrow_arms)
                    && reads.iter().all(|r| dying_after(func, at, *r, borrow_arms));
                ok.then_some((v, take))
            })
            .collect(),
        _ => Default::default(),
    };
    TAKEN_ARGS.with(|m| *m.borrow_mut() = taken);
    MOVING.with(|m| *m.borrow_mut() = moving);
}

thread_local! {
    static TAKEN_ARGS: std::cell::RefCell<std::collections::BTreeMap<ValueId, String>> =
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// The move-out expression of a borrowed arg of the call being rendered whose
/// carrier dies at the call ([`set_moving_args`]), if any.
pub(crate) fn taken_arg(v: ValueId) -> Option<String> {
    TAKEN_ARGS.with(|m| m.borrow().get(&v).cloned())
}

thread_local! {
    static REINIT: std::cell::RefCell<std::collections::BTreeSet<ValueId>> =
        const { std::cell::RefCell::new(std::collections::BTreeSet::new()) };
}

/// `v` is an arg of the call being rendered that may be moved.
pub(crate) fn moving_arg(v: ValueId) -> bool {
    MOVING.with(|m| m.borrow().contains(&v))
}

thread_local! {
    static REBOUND: std::cell::RefCell<std::collections::BTreeSet<ValueId>> =
        const { std::cell::RefCell::new(std::collections::BTreeSet::new()) };
}

/// The values of the function being rendered that a `SetLocal` rebinds, and
/// its params: a Rust binding that outlives one definition.
pub(crate) fn set_rebound(func: &MirFunction) {
    let mut set: std::collections::BTreeSet<ValueId> = func.params.iter().map(|p| p.value).collect();
    set.extend(func.ops.iter().filter_map(|op| match op {
        Op::SetLocal { local, .. } => Some(*local),
        _ => None,
    }));
    REBOUND.with(|m| *m.borrow_mut() = set);
}

/// The move of a dying value's binding `code` (#4006, no-perf-loss): a value
/// bound once (`let`, never rebound, not a param) moves as itself — the
/// binding is never read again, and Rust's drop flag covers its scope end —
/// where `std::mem::take` would build a default in its place and drop it
/// later. Anything else (a rebound slot, a field place) is taken.
/// Whether `v`'s binding moves as itself in [`take_expr`] (bound once, or
/// re-initialized before any read).
fn moves_as_itself(v: ValueId) -> bool {
    !REBOUND.with(|m| m.borrow().contains(&v)) || REINIT.with(|m| m.borrow().contains(&v))
}

pub(crate) fn take_expr(code: &str) -> String {
    let once = code
        .strip_prefix('v')
        .and_then(|n| n.parse::<u32>().ok())
        .is_some_and(|n| !REBOUND.with(|m| m.borrow().contains(&ValueId(n))) || REINIT.with(|m| m.borrow().contains(&ValueId(n))));
    if once && !drill_no_moves() {
        code.to_string()
    } else {
        format!("std::mem::take(&mut {code})")
    }
}
