//! #4277: a recorded release is its local's LAST read in the shipped bytes.
//!
//! The matching (matching.rs) holds each recorded `a` / `d` to one RC call on
//! the value it names, and refuses a call or an event left over — but it
//! matches without regard to WHERE in the body the call sits, so a release
//! moved before the variable's last use (a use after free the recording does
//! not show: the event and the call are both still there, one to one) reads
//! as matched. This pass closes that for the variable itself: from every live
//! release call of a certified frame whose argument is read straight out of a
//! local `L` (`local.get L` / `local.tee L` pushes it), no path through the
//! body's control flow reaches a `local.get L` before a `local.set L` /
//! `local.tee L` rewrites it. The walk follows the wasm structured control:
//! an `if` to both arms, a branch to its target (a `loop`'s head, any other
//! construct's end), a `br_if` to its target and on; `return`, a return call,
//! `unreachable` and `throw` end a path. A read on a path that then aborts is
//! still a read of the released block (under `http.serve` an abort ends only
//! the request), so it is refused like any other.
//!
//! TRUSTED (what this pass does not see): a read of the released block
//! through ANOTHER local or a container slot that holds it without a credit
//! of its own (an alias the emitter made; the matching names it, this walk
//! follows only `L`), and a judge call's release (the judge returns the block
//! it kept or the copy that replaced it, and the frame rebinds `L` to it).

use wasmparser::Operator as Op;

use crate::decode::Body;

/// The control shape the walk needs: per construct opener its `end`, per
/// `if` its `else`, per `else` its `if`, and per instruction the innermost
/// open construct.
pub(super) struct Cfg {
    close: Vec<usize>,
    else_of: Vec<Option<usize>>,
    if_of: Vec<usize>,
    parent: Vec<Option<usize>>,
    /// The locals some `if` / `br_if` tests directly (`local.get X`, or
    /// `local.get X; i32.eqz`): the only ones the walk tracks a value of.
    tested: std::collections::HashSet<u32>,
}

impl Cfg {
    pub(super) fn new(b: &Body<'_>) -> Self {
        let n = b.ops.len();
        let (mut close, mut else_of, mut if_of, mut parent) = (vec![usize::MAX; n], vec![None; n], vec![usize::MAX; n], vec![None; n]);
        let mut stack: Vec<usize> = Vec::new();
        for (i, op) in b.ops.iter().enumerate() {
            parent[i] = stack.last().copied();
            match op {
                Op::Block { .. } | Op::Loop { .. } | Op::If { .. } | Op::TryTable { .. } => stack.push(i),
                Op::Else => {
                    if let Some(&s) = stack.last() {
                        else_of[s] = Some(i);
                        if_of[i] = s;
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
        let tested = (0..n)
            .filter(|&j| matches!(b.ops[j], Op::If { .. } | Op::BrIf { .. }))
            .filter_map(|j| {
                let mut p = b.args.get(j)?.last().copied().flatten()?;
                if matches!(b.ops[p], Op::I32Eqz) {
                    p = b.args.get(p)?.first().copied().flatten()?;
                }
                match b.ops[p] {
                    Op::LocalGet { local_index } => Some(local_index),
                    _ => None,
                }
            })
            .collect();
        Cfg { close, else_of, if_of, parent, tested }
    }

    /// Where a branch of relative depth `d` at `i` continues: a `loop`'s
    /// first instruction, another construct's `end`; `None` leaves the
    /// function.
    fn target(&self, b: &Body<'_>, i: usize, d: u32) -> Option<usize> {
        let mut o = self.parent[i]?;
        for _ in 0..d {
            o = self.parent[o]?;
        }
        match b.ops[o] {
            Op::Loop { .. } => Some(o + 1),
            _ => Some(self.close[o]).filter(|&c| c != usize::MAX),
        }
    }

    /// The instructions control may reach right after `i`.
    fn succ(&self, b: &Body<'_>, i: usize, out: &mut Vec<usize>) {
        let n = b.ops.len();
        let next = |out: &mut Vec<usize>| {
            if i + 1 < n {
                out.push(i + 1);
            }
        };
        match &b.ops[i] {
            Op::If { .. } => {
                next(out);
                match self.else_of[i] {
                    Some(e) => out.push(e + 1),
                    None if self.close[i] != usize::MAX => out.push(self.close[i]),
                    None => {}
                }
            }
            Op::Else => {
                if let Some(&c) = self.if_of.get(i).and_then(|&s| self.close.get(s)) {
                    if c != usize::MAX {
                        out.push(c);
                    }
                }
            }
            Op::Br { relative_depth } => out.extend(self.target(b, i, *relative_depth)),
            Op::BrIf { relative_depth } | Op::BrOnNull { relative_depth } | Op::BrOnNonNull { relative_depth } => {
                out.extend(self.target(b, i, *relative_depth));
                next(out);
            }
            Op::BrTable { targets } => {
                for t in targets.targets().flatten().chain(std::iter::once(targets.default())) {
                    out.extend(self.target(b, i, t));
                }
            }
            Op::Return
            | Op::Unreachable
            | Op::ReturnCall { .. }
            | Op::ReturnCallIndirect { .. }
            | Op::ReturnCallRef { .. }
            | Op::Throw { .. }
            | Op::ThrowRef
            | Op::Rethrow { .. } => {}
            _ => next(out),
        }
    }
}

/// The local a release call at `k` reads its argument straight out of.
pub(super) fn released_local(b: &Body<'_>, k: usize) -> Option<u32> {
    let p = b.args.get(k)?.first().copied().flatten()?;
    match b.ops[p] {
        Op::LocalGet { local_index } | Op::LocalTee { local_index } => Some(local_index),
        _ => None,
    }
}

/// What a path knows of a local's value: nonzero (`true`) or zero.
type Known = Vec<(u32, bool)>;

/// The value instruction `p` pushes, when it is known zero or nonzero: a
/// constant, or a fresh block out of `$alloc`.
fn known_value(b: &Body<'_>, alloc: Option<u32>, p: usize) -> Option<bool> {
    match b.ops[p] {
        Op::I32Const { value } => Some(value != 0),
        Op::Call { function_index } if Some(function_index) == alloc => Some(true),
        _ => None,
    }
}

/// A condition's truth on this path, when a local the path knows decides
/// it: `local.get X`, or `local.get X; i32.eqz`.
fn decided(b: &Body<'_>, known: &Known, j: usize) -> Option<bool> {
    // The condition is the operand popped last.
    let p = b.args.get(j)?.last().copied().flatten()?;
    let (p, negate) = match b.ops[p] {
        Op::I32Eqz => (b.args.get(p)?.first().copied().flatten()?, true),
        _ => (p, false),
    };
    let Op::LocalGet { local_index } = b.ops[p] else { return None };
    known.iter().find(|k| k.0 == local_index).map(|k| k.1 != negate)
}

/// The first `local.get l` some path from the call at `k` reaches before a
/// write of `l`, if any. The walk is path-sensitive in one respect only: a
/// local the path set to a constant or to a fresh `$alloc` block is known
/// zero or nonzero, and an `if` / `br_if` testing it takes only the arm the
/// value selects (an error carrier built after a release, then tested).
pub(super) fn read_after(b: &Body<'_>, cfg: &Cfg, alloc: Option<u32>, k: usize, l: u32) -> Option<usize> {
    let mut seen: std::collections::HashSet<(usize, Known)> = std::collections::HashSet::new();
    let mut work: Vec<(usize, Known)> = Vec::new();
    let mut succ = Vec::new();
    cfg.succ(b, k, &mut succ);
    work.extend(succ.drain(..).map(|j| (j, Known::new())));
    while let Some((j, mut known)) = work.pop() {
        if !seen.insert((j, known.clone())) {
            continue;
        }
        match b.ops[j] {
            Op::LocalGet { local_index } if local_index == l => return Some(j),
            Op::LocalSet { local_index } | Op::LocalTee { local_index } if local_index == l => continue,
            Op::LocalSet { local_index } | Op::LocalTee { local_index } => {
                known.retain(|x| x.0 != local_index);
                let v = b.args.get(j).and_then(|a| a.first().copied().flatten()).and_then(|p| known_value(b, alloc, p));
                if let (Some(v), true) = (v, cfg.tested.contains(&local_index)) {
                    known.push((local_index, v));
                    known.sort_unstable();
                }
            }
            _ => {}
        }
        match (&b.ops[j], decided(b, &known, j)) {
            // An `if` the path decides: only the arm its value selects.
            (Op::If { .. }, Some(true)) => succ.push(j + 1),
            (Op::If { .. }, Some(false)) => match cfg.else_of[j] {
                Some(e) => succ.push(e + 1),
                None => succ.extend((cfg.close[j] != usize::MAX).then_some(cfg.close[j])),
            },
            // A `br_if` the path decides: taken, or not.
            (Op::BrIf { relative_depth }, Some(true)) => succ.extend(cfg.target(b, j, *relative_depth)),
            (Op::BrIf { .. }, Some(false)) => succ.extend((j + 1 < b.ops.len()).then_some(j + 1)),
            _ => cfg.succ(b, j, &mut succ),
        }
        work.extend(succ.drain(..).map(|s| (s, known.clone())));
    }
    None
}
