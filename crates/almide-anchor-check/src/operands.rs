//! The guard's OPERANDS (#4005, the residue #4011 (d) left trusted): that a
//! located bounds check compares THIS access's index against THIS list's
//! count, read off the shipped bytes.
//!
//! lib.rs holds each `c` to an unsigned compare opening an aborting `if`, and
//! each `a` / `w` to a load / store. Which values the compare tests was the
//! emitter's word. Here every operand is rebuilt as an expression over the
//! function's locals, from the decoder's operand provenance and each local's
//! reaching definition (a write that dominates the read, with no other write
//! of the local on any path between), and the check is held to the access:
//!
//! LIST (`i64.ge_u`): the left operand is the index `I (+ c)`; the right one
//! is the count `len(B) / S` (`i32.div_u` then extend, or extend then
//! `i64.div_u`; through a hoisted local it is the same expression). The
//! access's address is `B' + wrap(I') * S'` at offset `o` (or `P + ...` with
//! `P = B' + o'`, the payload pointer), and it holds that `S' = S`, the access
//! is at most `S` bytes wide, `o + o' = PAYLOAD + k * S` with `k >= 0`, and
//! `I' + k` is `I + c` — the same leaf values.
//!
//! BYTES (`i32.eqz`): the operand is `I >= 0 && I <= len(B) - w` (signed),
//! the access `B' + wrap(I')` at offset `PAYLOAD + k`, `k + width <= w`,
//! `I'` is `I`.
//!
//! In both, the check dominates the access, and `B'` is `B`: the same local
//! read with no write of it in between, where a write the recorded
//! copy-on-write JUDGE made of the local's own block counts as no change (a
//! judge returns a block of its argument's count), and from the first read
//! to the access no call other than a judge is handed the block and no store
//! writes its count word.
//!
//! Dominance is structural: the path from a read to a later one may leave
//! a `block` or `loop` by falling out of it (unless a branch before the read
//! lands at that block's end), never an `if`.
//!
//! Two leaf reads are the same value when they read the same local (or
//! global), the earlier dominates the later, and nothing writes it between —
//! where "between" includes the whole of every loop around the later read
//! that does not hold the earlier one. Code inside an aborting `if` (no
//! `else`, ending in `unreachable`) never falls through and is not
//! "between".
//!
//! TRUSTED: that a recorded judge site calls a copy-on-write judge; that a
//! call changes a block's count only when it is handed the block (a block
//! another local aliases is shared, so no routine shrinks it in place); the
//! block layout (`LEN` at 4, payload at 12, almide-layout).

use std::collections::HashSet;

use wasmparser::Operator as Op;

use crate::decode::{branch_target, is_load, store_width, Body};

/// almide-layout `LEN.offset` and `PAYLOAD` — the block ABI the shipped
/// bytes and the runtime share.
const LEN_OFF: u64 = 4;
const PAYLOAD: i64 = 12;
pub(crate) const FUEL: u32 = 64;

/// A value as an expression over the function's reads.
#[derive(Clone, Debug)]
pub(crate) enum T {
    Const(i64),
    /// A read of a local whose definition is not one dominating write:
    /// `(local, the read's instruction)`.
    Local(u32, usize),
    Global(u32, usize),
    Op(&'static str, Vec<T>),
    Load {
        name: &'static str,
        off: u64,
        addr: Box<T>,
        site: usize,
    },
    Call {
        site: usize,
        args: Vec<T>,
    },
    /// A read of a local through its one dominating write: `(local, the
    /// read, the written value)` — the value, remembering which variable it
    /// came through (two reads of one variable compare by the variable).
    Def(u32, usize, Box<T>),
    /// The value of an instruction this model does not open (`select`, a
    /// remainder, a block result): equal only to itself.
    Opaque(usize),
    Unknown,
}

/// `t` without the variables it came through.
fn pl(t: &T) -> &T {
    match t {
        T::Def(_, _, v) => pl(v),
        _ => t,
    }
}

/// `t`'s children when it is the operator `n`.
fn op<'t>(t: &'t T, n: &str) -> Option<&'t [T]> {
    match pl(t) {
        T::Op(m, xs) if *m == n => Some(xs),
        _ => None,
    }
}

fn cst(t: &T) -> Option<i64> {
    match pl(t) {
        T::Const(c) => Some(*c),
        _ => None,
    }
}

/// The variables `t` was read through, outermost first, each with its read.
pub(crate) fn chain(t: &T) -> Vec<(Src, usize)> {
    let mut v = Vec::new();
    let mut c = t;
    loop {
        match c {
            T::Def(l, u, x) => {
                v.push((Src::Local(*l), *u));
                c = x;
            }
            T::Local(l, u) => {
                v.push((Src::Local(*l), *u));
                break;
            }
            T::Global(g, u) => {
                v.push((Src::Global(*g), *u));
                break;
            }
            _ => break,
        }
    }
    v
}

/// The instruction that evaluated an unopened value, a load or a call.
fn site_id(t: &T) -> Option<usize> {
    match pl(t) {
        T::Load { site, .. } | T::Call { site, .. } | T::Opaque(site) => Some(*site),
        _ => None,
    }
}

fn pure(op: &Op<'_>) -> Option<&'static str> {
    Some(match op {
        Op::I32Add => "i32.add",
        Op::I32Sub => "i32.sub",
        Op::I32Mul => "i32.mul",
        Op::I32DivU => "i32.div_u",
        Op::I32And => "i32.and",
        Op::I32Eqz => "i32.eqz",
        Op::I32WrapI64 => "i32.wrap_i64",
        Op::I64ExtendI32U => "i64.extend_i32_u",
        Op::I64ExtendI32S => "i64.extend_i32_s",
        Op::I64Add => "i64.add",
        Op::I64Sub => "i64.sub",
        Op::I64Mul => "i64.mul",
        Op::I64DivU => "i64.div_u",
        Op::I64GeS => "i64.ge_s",
        Op::I64LeS => "i64.le_s",
        Op::I64GeU => "i64.ge_u",
        _ => return None,
    })
}

fn load_name(op: &Op<'_>) -> (&'static str, u64, u64) {
    match op {
        Op::I32Load { memarg } => ("i32.load", memarg.offset, 4),
        Op::I64Load { memarg } => ("i64.load", memarg.offset, 8),
        Op::F32Load { memarg } => ("f32.load", memarg.offset, 4),
        Op::F64Load { memarg } => ("f64.load", memarg.offset, 8),
        Op::I32Load8S { memarg } | Op::I32Load8U { memarg } => ("i32.load8", memarg.offset, 1),
        Op::I64Load8S { memarg } | Op::I64Load8U { memarg } => ("i64.load8", memarg.offset, 1),
        Op::I32Load16S { memarg } | Op::I32Load16U { memarg } => ("i32.load16", memarg.offset, 2),
        Op::I64Load16S { memarg } | Op::I64Load16U { memarg } => ("i64.load16", memarg.offset, 2),
        Op::I64Load32S { memarg } | Op::I64Load32U { memarg } => ("i64.load32", memarg.offset, 4),
        Op::V128Load { memarg } => ("v128.load", memarg.offset, 16),
        _ => ("?", 0, u64::MAX),
    }
}

/// What a leaf reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Src {
    Local(u32),
    Global(u32),
}

/// One function's control shape and value expressions.
pub(crate) struct Flow<'b, 'a> {
    b: &'b Body<'a>,
    /// For a `block` / `loop` / `if`: the index of its `end`.
    close: Vec<usize>,
    /// The innermost construct holding each instruction (its opener).
    parent: Vec<Option<usize>>,
    /// Inside an aborting `if` (no `else`, ends in `unreachable`).
    abort_arm: Vec<bool>,
    /// The recorded copy-on-write judge call sites of this function.
    judges: HashSet<usize>,
}

impl<'b, 'a> Flow<'b, 'a> {
    pub(crate) fn new(b: &'b Body<'a>, judges: HashSet<usize>) -> Self {
        let n = b.ops.len();
        let (mut close, mut parent, mut abort_arm) = (vec![usize::MAX; n], vec![None; n], vec![false; n]);
        let mut stack: Vec<usize> = Vec::new();
        let mut has_else = vec![false; n];
        for (i, op) in b.ops.iter().enumerate() {
            parent[i] = stack.last().copied();
            match op {
                Op::Block { .. } | Op::Loop { .. } | Op::If { .. } | Op::TryTable { .. } => stack.push(i),
                Op::Else => {
                    if let Some(&s) = stack.last() {
                        has_else[s] = true;
                    }
                }
                Op::End => {
                    if let Some(s) = stack.pop() {
                        close[s] = i;
                    }
                }
                _ => {}
            }
        }
        for s in 0..n {
            if matches!(b.ops[s], Op::If { .. })
                && !has_else[s]
                && close[s] != usize::MAX
                && close[s] > s + 1
                && matches!(b.ops[close[s] - 1], Op::Unreachable)
            {
                abort_arm[s + 1..=close[s]].iter_mut().for_each(|x| *x = true);
            }
        }
        Flow { b, close, parent, abort_arm, judges }
    }

    /// Inside an aborting `if` (module header).
    pub(crate) fn in_abort_arm(&self, k: usize) -> bool {
        self.abort_arm[k]
    }

    /// Every path from the entry to `q` passes `p` (`p < q`, structured
    /// control: nothing in `(p, q]` leaves the construct `p` sits in).
    fn dominates(&self, p: usize, q: usize) -> bool {
        if p > q {
            return false;
        }
        // The constructs `p` sits in that `q` does not: the path leaves each
        // through its `end`. Leaving an `if` is refused (the other arm, or no
        // arm, reaches its end without `p`); a `block` or `loop` is left by
        // falling through, so `p` is on every path out unless a branch
        // BEFORE `p` inside it lands at the end of an exited `block`.
        let chain = |mut c: Option<usize>| {
            let mut v = Vec::new();
            while let Some(s) = c {
                v.push(s);
                c = self.parent[s];
            }
            v
        };
        let (cp, cq) = (chain(self.parent[p]), chain(self.parent[q]));
        let exited: Vec<usize> = cp.iter().copied().filter(|s| !cq.contains(s)).collect();
        if exited.iter().any(|&s| !matches!(self.b.ops[s], Op::Block { .. } | Op::Loop { .. })) {
            return false;
        }
        // Inner depth of each exited `block` (a branch to a `loop` goes to
        // its start, from which the path passes `p` again).
        let block_depths: Vec<u32> = exited
            .iter()
            .filter(|&&s| matches!(self.b.ops[s], Op::Block { .. }))
            .map(|&s| self.b.depth[s] + 1)
            .collect();
        if let Some(&outer) = exited.last() {
            for j in outer + 1..p {
                if let Some(t) = branch_target(&self.b.ops[j], self.b.depth[j]) {
                    let all: Vec<u32> = match &self.b.ops[j] {
                        Op::BrTable { targets } => targets
                            .targets()
                            .filter_map(Result::ok)
                            .chain(std::iter::once(targets.default()))
                            .map(|r| self.b.depth[j].saturating_sub(r))
                            .collect(),
                        _ => vec![t],
                    };
                    if all.iter().any(|t| block_depths.contains(t)) {
                        return false;
                    }
                }
            }
        }
        // Within the construct both share, `p` precedes `q` at its own level
        // or inside constructs that end before `q`: no `else` switches arms
        // at that level.
        let shared = cq.iter().copied().find(|s| cp.contains(s));
        let d_shared = shared.map_or(0, |s| self.b.depth[s] + 1);
        !(p + 1..=q).any(|k| self.b.depth[k] == d_shared && matches!(self.b.ops[k], Op::Else))
    }

    /// The instructions that may run after `from` and before `to` reaches
    /// again: `(from, to)`, plus the whole of every loop around `to` that
    /// does not hold `from`, never an aborting arm.
    fn window(&self, from: usize, to: usize) -> Vec<usize> {
        let mut v: Vec<usize> = (from + 1..to).collect();
        let mut c = self.parent[to];
        while let Some(s) = c {
            let holds_from = s < from && from <= self.close[s];
            if holds_from {
                break;
            }
            if matches!(self.b.ops[s], Op::Loop { .. }) {
                v.extend(s + 1..self.close[s].min(self.b.ops.len()));
            }
            c = self.parent[s];
        }
        v.sort_unstable();
        v.dedup();
        v.retain(|&k| !self.abort_arm[k]);
        v
    }

    fn writes(&self, k: usize, src: Src) -> bool {
        match (src, &self.b.ops[k]) {
            (Src::Local(l), Op::LocalSet { local_index } | Op::LocalTee { local_index }) => *local_index == l,
            (Src::Global(g), Op::GlobalSet { global_index }) => *global_index == g,
            _ => false,
        }
    }

    /// The write of local `l` every path to `p` last passes, when there is
    /// exactly one such instruction.
    fn reaching_def(&self, l: u32, p: usize) -> Option<usize> {
        let k = (0..p).rev().find(|&k| !self.abort_arm[k] && self.writes(k, Src::Local(l)))?;
        if !self.dominates(k, p) {
            return None;
        }
        if self.window(k, p).into_iter().any(|j| self.writes(j, Src::Local(l))) {
            return None;
        }
        Some(k)
    }

    pub(crate) fn arg(&self, p: usize, k: usize, fuel: u32) -> T {
        match self.b.args.get(p).and_then(|a| a.get(k)).copied().flatten() {
            Some(q) => self.term(q, fuel),
            None => T::Unknown,
        }
    }

    /// The value instruction `p` pushes.
    pub(crate) fn term(&self, p: usize, fuel: u32) -> T {
        if fuel == 0 {
            return T::Unknown;
        }
        let f = fuel - 1;
        let op = &self.b.ops[p];
        match op {
            Op::I32Const { value } => T::Const(i64::from(*value)),
            Op::I64Const { value } => T::Const(*value),
            Op::LocalGet { local_index } => match self.reaching_def(*local_index, p) {
                Some(q) => T::Def(*local_index, p, Box::new(self.arg(q, 0, f))),
                None => T::Local(*local_index, p),
            },
            Op::LocalTee { .. } => self.arg(p, 0, f),
            Op::GlobalGet { global_index } => T::Global(*global_index, p),
            Op::Call { .. } => T::Call {
                site: p,
                args: (0..self.b.args[p].len()).map(|k| self.arg(p, k, f)).collect(),
            },
            o if is_load(o) => {
                let (name, off, _) = load_name(o);
                T::Load { name, off, addr: Box::new(self.arg(p, 0, f)), site: p }
            }
            o => match pure(o) {
                Some(name) => T::Op(name, (0..self.b.args[p].len()).map(|k| self.arg(p, k, f)).collect()),
                None => T::Opaque(p),
            },
        }
    }

    /// No write of `src` between reads `u` and `v`, the earlier dominating.
    fn unchanged(&self, src: Src, u: usize, v: usize) -> bool {
        if u == v {
            return true;
        }
        let (a, b) = (u.min(v), u.max(v));
        self.dominates(a, b) && !self.window(a, b).into_iter().any(|k| self.writes(k, src))
    }

    /// `x` and `y` are the same value: read through one variable with no
    /// write of it between the reads, or the same operator over the same
    /// values, or one evaluation of one unopened instruction.
    pub(crate) fn same(&self, x: &T, y: &T) -> bool {
        let (cx, cy) = (chain(x), chain(y));
        if cx.iter().any(|&(s1, u)| cy.iter().any(|&(s2, v)| s1 == s2 && self.unchanged(s1, u, v))) {
            return true;
        }
        match (pl(x), pl(y)) {
            (T::Const(a), T::Const(b)) => a == b,
            (T::Op(n, xs), T::Op(m, ys)) => {
                n == m && xs.len() == ys.len() && xs.iter().zip(ys).all(|(a, b)| self.same(a, b))
            }
            (a, b) => matches!((site_id(a), site_id(b)), (Some(s), Some(t)) if s == t),
        }
    }

    /// A judge call's argument, through any number of judges.
    fn strip<'t>(&self, t: &'t T) -> &'t T {
        match pl(t) {
            T::Call { site, args } if args.len() == 1 && self.judges.contains(site) => self.strip(&args[0]),
            _ => t,
        }
    }

    /// Does `t` hold the block `blk` (or an address computed from it)? A
    /// value LOADED through it is something the block holds (an element), not
    /// the block, so a load's address is not searched.
    fn mentions(&self, t: &T, blk: &T) -> bool {
        // A block's identity: the innermost variable it was read through, or
        // the instruction that produced it — not a hold local it passed
        // through, which may since hold something else.
        let root = |v: &T| chain(v).last().map(|&(s1, _)| s1);
        let hit = matches!((root(t), root(blk)), (Some(a), Some(b)) if a == b)
            || matches!((site_id(t), site_id(blk)), (Some(a), Some(b)) if a == b);
        hit || match pl(t) {
            T::Op(_, xs) => xs.iter().any(|x| self.mentions(x, blk)),
            T::Call { args, .. } => args.iter().any(|x| self.mentions(x, blk)),
            _ => false,
        }
    }

    /// `cb` (the block the check counted, its count read at `count_at`) and
    /// `ab` (the block the access addresses) have the same count when the
    /// access at `acc` runs: from the first read of either to the access no
    /// write of the variable but its own judge's; from the count's read to
    /// the access no call but a judge handed the block and no store of its
    /// count word.
    fn same_count(&self, cb: &T, ab: &T, count_at: usize, acc: usize) -> Result<(), String> {
        let (x, y) = (self.strip(cb), self.strip(ab));
        if !self.dominates(count_at, acc) {
            return Err("the count's read does not dominate the access".into());
        }
        // Each variable both were read through is a candidate identity; else
        // one evaluation of one instruction. The first candidate that holds
        // decides.
        let (cx, cy) = (chain(x), chain(y));
        let mut cands: Vec<(Option<Src>, usize)> = cx
            .iter()
            .filter_map(|&(s1, u)| cy.iter().find(|&&(s2, _)| s2 == s1).map(|&(_, v)| (Some(s1), u.min(v))))
            .collect();
        if let (Some(a), Some(b)) = (site_id(x), site_id(y)) {
            if a == b {
                cands.push((None, a));
            }
        }
        let mut last = format!("the check counts {x:?}, the access addresses {y:?}");
        for (src, from) in cands {
            match self.count_kept(x, src, from, count_at, acc) {
                Ok(()) => return Ok(()),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// The block `x` (read through `src`, first at `from`) keeps its count
    /// until the access at `acc` (module header of `same_count`).
    fn count_kept(&self, x: &T, src: Option<Src>, from: usize, count_at: usize, acc: usize) -> Result<(), String> {
        if !self.dominates(from, acc) {
            return Err("the block's first read does not dominate the access".into());
        }
        let counted: HashSet<usize> = self.window(count_at, acc).into_iter().collect();
        for k in self.window(from, acc) {
            let ins = &self.b.ops[k];
            if let Some(src) = src {
                if self.writes(k, src) {
                    // A write of the judge's verdict on the variable's own block.
                    let val = self.arg(k, 0, FUEL);
                    let judged = matches!(pl(&val), T::Call { site, .. } if self.judges.contains(site));
                    let own = chain(self.strip(&val)).iter().any(|&(s1, _)| s1 == src);
                    if !(judged && own) {
                        return Err(format!("instruction {k} ({ins:?}) changes the list between the check and the access"));
                    }
                }
            }
            if !counted.contains(&k) {
                continue;
            }
            if matches!(ins, Op::Call { .. } | Op::CallIndirect { .. }) && !self.judges.contains(&k) {
                let n = self.b.args[k].len();
                if (0..n).any(|j| self.mentions(&self.arg(k, j, FUEL), x)) {
                    return Err(format!("instruction {k} ({ins:?}) is handed the list between the check and the access"));
                }
            }
            if let Some((4, off)) = store_width(ins) {
                // The effective address `base + constant`: the count word is
                // the block's own address plus LEN, whatever the split.
                let addr = self.arg(k, 0, FUEL);
                let (base, eff) = match op(&addr, "i32.add") {
                    Some([b, c]) if cst(c).is_some() => (b, off as i64 + cst(c).unwrap_or(0)),
                    _ => (&addr, off as i64),
                };
                if eff == LEN_OFF as i64 && !matches!(pl(base), T::Op(..)) && self.mentions(base, x) {
                    return Err(format!("instruction {k} writes the list's count between the check and the access"));
                }
            }
        }
        Ok(())
    }
}

fn affine(t: &T) -> (&T, i64) {
    match op(t, "i64.add") {
        Some([a, b]) => match cst(b) {
            Some(c) => (a, c),
            None => (t, 0),
        },
        _ => (t, 0),
    }
}

/// `len(B)`: `(B, the load)`.
fn len(l: &T) -> Option<(&T, usize)> {
    match pl(l) {
        T::Load { name: "i32.load", off: LEN_OFF, addr, site } => Some((&**addr, *site)),
        _ => None,
    }
}

/// `0 <= I && I <= len(B) - w` (signed): `(I, I again, (B, its count's
/// load), w)`.
fn room_of(room: &T) -> Option<(&T, &T, (&T, usize), i64)> {
    let [lo, hi] = op(room, "i32.and")? else { return None };
    let [i1, z] = op(lo, "i64.ge_s")? else { return None };
    let [i2, d] = op(hi, "i64.le_s")? else { return None };
    if cst(z)? != 0 {
        return None;
    }
    let [e, w] = op(d, "i64.sub")? else { return None };
    let [l] = op(e, "i64.extend_i32_u")? else { return None };
    Some((i1, i2, len(l)?, cst(w)?))
}

/// `len(B) / S` as the count operand: `((B, its count's load), S)`.
fn count_of(t: &T) -> Option<((&T, usize), i64)> {
    if let Some([d]) = op(t, "i64.extend_i32_u") {
        let [l, s] = op(d, "i32.div_u")? else { return None };
        return Some((len(l)?, cst(s)?));
    }
    let [e, s] = op(t, "i64.div_u")? else { return None };
    let [l] = op(e, "i64.extend_i32_u")? else { return None };
    Some((len(l)?, cst(s)?))
}

/// The access instruction (`acc`; for an `a` anchored at the release call,
/// the load before it): its address term, memarg offset and width.
fn access_parts(fl: &Flow<'_, '_>, acc: usize) -> Result<(T, u64, u64), String> {
    let op = &fl.b.ops[acc];
    let (off, width) = if let Some((w, off)) = store_width(op) {
        (off, u64::from(w))
    } else if is_load(op) {
        let (_, off, w) = load_name(op);
        (off, w)
    } else {
        return Err(format!("{op:?} is no access"));
    };
    Ok((fl.arg(acc, 0, FUEL), off, width))
}

/// Check that the guard whose compare is at `cmp` covers the access at
/// `acc`, both in `b` (module header).
pub(crate) fn covers(fl: &Flow<'_, '_>, cmp: usize, acc: usize) -> Result<(), String> {
    let acc = if matches!(fl.b.ops[acc], Op::Call { .. }) && acc > 0 { acc - 1 } else { acc };
    if !fl.dominates(cmp, acc) {
        return Err("the check does not dominate the access".into());
    }
    let (addr, off, width) = access_parts(fl, acc)?;
    match fl.b.ops[cmp] {
        Op::I64GeU => {
            let (ix, cnt) = (fl.arg(cmp, 0, FUEL), fl.arg(cmp, 1, FUEL));
            let ((cb, count_at), s) = count_of(&cnt).ok_or_else(|| format!("the compare's right operand is not a list's count: {cnt:?}"))?;
            let Some([base, scaled]) = op(&addr, "i32.add") else {
                return Err(format!("the access's address is not block + scaled index: {addr:?}"));
            };
            let (ai, s2) = match op(scaled, "i32.mul") {
                Some([w, c]) => match (op(w, "i32.wrap_i64"), cst(c)) {
                    (Some([i]), Some(s2)) => (i, s2),
                    _ => return Err(format!("the access's index is not scaled by a constant: {scaled:?}")),
                },
                _ => return Err(format!("the access's index is not scaled: {scaled:?}")),
            };
            let (ab, total) = match op(base, "i32.add") {
                Some([b, c]) if cst(c).is_some() => (b, off as i64 + cst(c).unwrap_or(0)),
                _ => (base, off as i64),
            };
            if s <= 0 || s2 != s {
                return Err(format!("the check counts slots of {s} bytes, the access scales by {s2}"));
            }
            if width as i64 > s {
                return Err(format!("the access is {width} bytes wide, its slots {s}"));
            }
            let k = total - PAYLOAD;
            if k < 0 || k % s != 0 {
                return Err(format!("the access lies at payload offset {k}, not a whole slot"));
            }
            let ((ci, cc), (ai, ac)) = (affine(&ix), affine(ai));
            if !fl.same(ci, ai) || cc != ac + k / s {
                return Err(format!("the check tests index {ix:?}, the access reads slot {ai:?} + {}", k / s));
            }
            fl.same_count(cb, ab, count_at, acc)
        }
        Op::I32Eqz => {
            let room = fl.arg(cmp, 0, FUEL);
            let (i1, i2, (cb, count_at), w) = room_of(&room).ok_or_else(|| format!("the room test is not `0 <= i <= len - w`: {room:?}"))?;
            if !fl.same(i1, i2) {
                return Err("the room test's two bounds test different indices".into());
            }
            let Some([ab, wi]) = op(&addr, "i32.add") else {
                return Err(format!("the byte access is not block + index: {addr:?}"));
            };
            let Some(ai) = op(wi, "i32.wrap_i64") else {
                return Err(format!("the byte access's index is {wi:?}"));
            };
            let k = off as i64 - PAYLOAD;
            if k < 0 || k + width as i64 > w {
                return Err(format!("the byte access covers [{k}, {}), the room test {w} bytes", k + width as i64));
            }
            if ai.len() != 1 || !fl.same(i1, &ai[0]) {
                return Err("the room test and the byte access use different indices".into());
            }
            fl.same_count(cb, ab, count_at, acc)
        }
        ref o => Err(format!("the guard's compare is {o:?}")),
    }
}
