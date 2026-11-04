//! The native render's BOUNDS witness (#4006, security row 4).
//!
//! Every element access the native render writes into the Rust source is
//! recorded where its text is written, in the certificate language of
//! `proofs/BoundsCert.v` (`check_bounds_cert`, `almide-verify bounds`), the
//! language the structural wasm leg's bounds witness uses:
//!
//! - `c N` — the render wrote the check of slot N (`if … >= ….len() {
//!   Error: index out of bounds; exit 1 }`, or a checking shim's call);
//! - `a N` / `w N` — the render wrote a read / a store of slot N's element;
//! - `s N` — slot N's list or index changed (a `SetLocal` of either);
//! - `L{…}` / `B{…|…}` — the function's loops and branches.
//!
//! A slot is one (list, index) pair: the list value and the index value plus
//! a constant slot offset. The structure and the `s` events come from the
//! function's MIR ops; the `c` / `a` / `w` events from the render, tagged with
//! the op being rendered, so a render path that writes an access with no
//! check before it leaves a bare `a` / `w`, which the checker rejects
//! (`scripts/witness-reinject.sh native-bounds` re-injects such bugs).
//!
//! The checker proves every run of an accepted witness SPATIAL
//! (`proofs/SpatialCert.v`, `accepts_spatial`): each recorded access is
//! dominated by a check of its own slot. Rust's own index check stays in
//! the code, but nothing rests on it without a record.

use crate::{MirFunction, Op, ValueId};
use std::cell::RefCell;

/// A slot: the list, the index value (none for a constant slot) and the
/// constant slot offset.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Slot {
    pub base: ValueId,
    pub idx: Option<ValueId>,
    pub k: i64,
}

#[derive(Default)]
struct Rec {
    op: usize,
    /// The loop exit tests that cover a slot: (exit op, slot, last op covered).
    exits: Vec<(usize, Slot, usize)>,
    slots: Vec<Slot>,
    /// (op index, event letter, slot number), in recording order.
    events: Vec<(usize, char, usize)>,
    done: Vec<(String, String)>,
}

thread_local! {
    static REC: RefCell<Rec> = RefCell::new(Rec::default());
}

/// A new function: no slots, no events, its loop exit tests read off the MIR.
pub(crate) fn begin(func: &MirFunction) {
    let exits = exit_tests(func);
    REC.with(|r| {
        let mut r = r.borrow_mut();
        r.op = 0;
        r.slots.clear();
        r.events.clear();
        r.exits = exits;
    });
}

/// Whether the access of `s` the render writes now is covered by its loop's
/// exit test (`e`): no explicit check is needed there.
pub(crate) fn exit_covered(s: Slot) -> bool {
    REC.with(|r| {
        let r = r.borrow();
        r.exits.iter().any(|(t, x, last)| *x == s && *t < r.op && r.op <= *last)
    })
}

/// The loop exit tests `i < len(xs)` that cover the element `xs[i]` (`PExit`,
/// #4280): a `LoopBreakUnless` directly in its loop's body over `i < n`,
/// where `n` is the length word of a list `xs` no `SetLocal` rebinds, and `i`
/// a counter that starts at 0 and only ever steps by 1 — never negative, so
/// the signed test the render writes is the unsigned one `PExit` reads. It
/// covers `xs[i]` from the test to the first op that rebinds `i` (or the
/// loop's end), and `ys[i]` of every list `ys` allocated with that same
/// length value `n` and never rebound. Returns (test op, slot, last op
/// covered).
fn exit_tests(func: &MirFunction) -> Vec<(usize, Slot, usize)> {
    let ops = &func.ops;
    let facts = ExitFacts::of(ops);
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut loops: Vec<usize> = Vec::new();
    for (t, op) in ops.iter().enumerate() {
        match op {
            Op::LoopStart => {
                depth += 1;
                loops.push(t);
            }
            Op::IfThen { .. } => depth += 1,
            Op::EndIf { .. } => depth -= 1,
            Op::LoopEnd => {
                depth -= 1;
                loops.pop();
            }
            Op::LoopBreakUnless { cond } => {
                let Some(&start) = loops.last() else { continue };
                facts.exit_test_at(ops, (start, t), cond, &mut out);
            }
            _ => {}
        }
    }
    let _ = depth;
    out
}

/// What [`exit_tests`] reads off the ops before it looks at the loops.
#[derive(Default)]
struct ExitFacts {
    consts: std::collections::BTreeMap<ValueId, i64>,
    handle: std::collections::BTreeMap<ValueId, ValueId>,
    len_addr: std::collections::BTreeMap<ValueId, ValueId>,
    len_of: std::collections::BTreeMap<ValueId, ValueId>,
    lt: std::collections::BTreeMap<ValueId, (ValueId, ValueId)>,
    add_one: std::collections::BTreeMap<ValueId, ValueId>,
    zero: std::collections::BTreeSet<ValueId>,
    rebound: std::collections::BTreeMap<ValueId, Vec<ValueId>>,
    /// The lists allocated with a given length value (a `map` loop's result).
    sized: std::collections::BTreeMap<ValueId, Vec<ValueId>>,
}

impl ExitFacts {
    fn of(ops: &[Op]) -> ExitFacts {
        let mut f = ExitFacts::default();
        for op in ops {
            f.note_def(op);
        }
        for op in ops {
            f.note_arith(op);
        }
        for op in ops {
            f.note_len(op);
        }
        f
    }

    fn note_def(&mut self, op: &Op) {
        use crate::PrimKind;
        match op {
            Op::ConstInt { dst, value } => {
                self.consts.insert(*dst, *value);
                if *value == 0 {
                    self.zero.insert(*dst);
                }
            }
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if args.len() == 1 => {
                self.handle.insert(*d, args[0]);
            }
            Op::SetLocal { local, src } => self.rebound.entry(*local).or_default().push(*src),
            Op::Alloc { dst, init: crate::Init::DynList { len } | crate::Init::DynListStr { len }, .. } => {
                self.sized.entry(*len).or_default().push(*dst);
            }
            _ => {}
        }
    }

    fn note_arith(&mut self, op: &Op) {
        use crate::IntOp;
        let off = i64::from(crate::lower::layout::BLOCK_LEN_OFFSET);
        match op {
            Op::IntBinOp { dst, op: IntOp::Add, a, b } => {
                for (h, c) in [(a, b), (b, a)] {
                    if let (Some(base), Some(&k)) = (self.handle.get(h), self.consts.get(c)) {
                        if k == off {
                            self.len_addr.insert(*dst, *base);
                        }
                    }
                    if self.consts.get(c) == Some(&1) {
                        self.add_one.insert(*dst, *h);
                    }
                }
            }
            Op::IntBinOp { dst, op: IntOp::Lt, a, b } => {
                self.lt.insert(*dst, (*a, *b));
            }
            _ => {}
        }
    }

    fn note_len(&mut self, op: &Op) {
        use crate::PrimKind;
        if let Op::Prim { kind: PrimKind::Load { width: 4 }, dst: Some(d), args } = op {
            if let Some(base) = args.first().and_then(|a| self.len_addr.get(a)) {
                self.len_of.insert(*d, *base);
            }
        }
    }

    fn counter(&self, i: ValueId) -> bool {
        self.zero.contains(&i) && self.rebound.get(&i).is_some_and(|srcs| srcs.iter().all(|s| self.add_one.get(s) == Some(&i)))
    }

    /// The `LoopBreakUnless { cond }` at op `t` of the loop opened at `start`:
    /// the slots its exit test covers, pushed onto `out`.
    fn exit_test_at(&self, ops: &[Op], (start, t): (usize, usize), cond: &ValueId, out: &mut Vec<(usize, Slot, usize)>) {
        let direct = ops[start + 1..t].iter().fold(0i32, |d, o| match o {
            Op::IfThen { .. } | Op::LoopStart => d + 1,
            Op::EndIf { .. } | Op::LoopEnd => d - 1,
            _ => d,
        }) == 0;
        let Some(&(i, n)) = self.lt.get(cond) else { return };
        let Some(&base) = self.len_of.get(&n) else { return };
        if !direct || !self.counter(i) || self.rebound.contains_key(&base) {
            return;
        }
        let last = covered_until(ops, t, i);
        out.push((t, Slot { base, idx: Some(i), k: 0 }, last));
        // A list allocated with that same length `n` has that count
        // too: the test covers its element `i` as well.
        for d in self.sized.get(&n).into_iter().flatten() {
            if !self.rebound.contains_key(d) {
                out.push((t, Slot { base: *d, idx: Some(i), k: 0 }, last));
            }
        }
    }
}

/// The loop's end after the exit test at op `t`, and the first rebinding of
/// `i` before it: the last op the test covers.
fn covered_until(ops: &[Op], t: usize, i: ValueId) -> usize {
    let mut d = 0i32;
    let mut last = ops.len() - 1;
    for (j, o) in ops.iter().enumerate().skip(t + 1) {
        match o {
            Op::LoopStart | Op::IfThen { .. } => d += 1,
            Op::EndIf { .. } => d -= 1,
            Op::LoopEnd if d == 0 => {
                last = j;
                break;
            }
            Op::LoopEnd => d -= 1,
            Op::SetLocal { local, .. } if *local == i => {
                last = j;
                break;
            }
            _ => {}
        }
    }
    last
}

/// The op the render is writing now.
pub(crate) fn at(op: usize) {
    REC.with(|r| r.borrow_mut().op = op);
}

/// Slot `s`'s number, numbering it next when it is new.
fn slot_number(slots: &mut Vec<Slot>, s: Slot) -> usize {
    match slots.iter().position(|x| *x == s) {
        Some(n) => n,
        None => {
            slots.push(s);
            slots.len() - 1
        }
    }
}

fn event(c: char, s: Slot) {
    REC.with(|r| {
        let mut r = r.borrow_mut();
        let n = slot_number(&mut r.slots, s);
        let op = r.op;
        r.events.push((op, c, n));
    });
}

/// The render wrote the check of `s`.
pub(crate) fn check(s: Slot) {
    event('c', s);
}

/// The render wrote a read of `s`'s element.
pub(crate) fn access(s: Slot) {
    event('a', s);
}

/// The render wrote a store into `s`'s element.
pub(crate) fn store(s: Slot) {
    event('w', s);
}

/// Close `func`: its witness, the MIR's structure with the recorded events
/// in op order, kept for [`take`].
pub(crate) fn finish(func: &MirFunction) {
    REC.with(|r| finish_rec(&mut r.borrow_mut(), func));
}

fn finish_rec(r: &mut Rec, func: &MirFunction) {
    let mut text = String::new();
    let mut ifs: Vec<bool> = Vec::new();
    let mut next = 0usize;
    let mut events = std::mem::take(&mut r.events);
    let exits = std::mem::take(&mut r.exits);
    for (t, x, _) in exits {
        let n = slot_number(&mut r.slots, x);
        events.push((t, 'e', n));
    }
    // Op order; within one op, the recording order (a check before its access).
    events.sort_by_key(|e| e.0);
    for (i, op) in func.ops.iter().enumerate() {
        open_structure(op, &mut text, &mut ifs);
        while next < events.len() && events[next].0 <= i {
            let (_, c, n) = events[next];
            text.push_str(&format!("{c}{n} "));
            next += 1;
        }
        close_structure(op, &mut text, &mut ifs, &r.slots);
    }
    // Events past the last op (none on a well-formed render) still count.
    for &(_, c, n) in &events[next..] {
        text.push_str(&format!("{c}{n} "));
    }
    let text = text.trim_end().to_string();
    r.done.push((func.name.clone(), text));
}

/// The witness text an op opens before its own events.
fn open_structure(op: &Op, text: &mut String, ifs: &mut Vec<bool>) {
    match op {
        Op::LoopStart => text.push_str("L{ "),
        Op::IfThen { .. } => {
            text.push_str("B{ ");
            ifs.push(false);
        }
        Op::Else { .. } => {
            text.push_str("| ");
            if let Some(e) = ifs.last_mut() {
                *e = true;
            }
        }
        _ => {}
    }
}

/// The witness text an op closes after its own events.
fn close_structure(op: &Op, text: &mut String, ifs: &mut Vec<bool>, slots: &[Slot]) {
    match op {
        Op::LoopEnd => text.push_str("} "),
        Op::EndIf { .. } => {
            if ifs.pop() == Some(false) {
                text.push_str("| ");
            }
            text.push_str("} ");
        }
        Op::SetLocal { local, .. } => {
            for (n, s) in slots.iter().enumerate() {
                if s.base == *local || s.idx == Some(*local) {
                    text.push_str(&format!("s{n} "));
                }
            }
        }
        _ => {}
    }
}

/// The witnesses of every function closed since the last call.
pub(crate) fn take() -> Vec<(String, String)> {
    REC.with(|r| std::mem::take(&mut r.borrow_mut().done))
}

/// Forget every closed witness (a render starts).
pub(crate) fn reset() {
    REC.with(|r| *r.borrow_mut() = Rec::default());
}
