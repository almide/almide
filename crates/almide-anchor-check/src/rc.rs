//! #4277: the ownership witness's RC events against the shipped calls.
//!
//! For each certified frame the anchor file names (`rc <position>|<events>`),
//! every live call in that body to the rc_inc routine or to a release
//! routine (`rcfns <inc> <release> ...`) must be one recorded event of its
//! kind, and every recorded event one such call: an event that names a local
//! matches only a call whose argument was read through that local
//! (operands.rs `chain`); a temporary's event (`-`) matches any call of its
//! kind. A call in dead code (after an unconditional branch in its block) is
//! not live. A call on a path that aborts (in an aborting arm, or a release
//! whose block then reaches `unreachable` through only calls, constants and
//! local reads) is matched like any other; left over, it is counted
//! (`pre_abort`), not refused — the process ends there, and the witness does
//! not record every such release (#4278).
//!
//! An owner rebind's release of the old block (`o`, the recorder's DecOld)
//! and a variable's release (`d` on a local) is a release call, or happens
//! inside the copy-on-write judge the anchor file's `judges` line locates
//! (the judge releases its argument when it copies): it matches either, read
//! through its local, a release call first; a DecOld on a
//! local that held nothing on its path has no instruction, so an unmatched
//! `o` is counted (`rebind_unmatched`), not refused, and a judge call no `o`
//! takes is counted (`judges_unmatched`) — both #4278's to settle.
//!
//! An RC call left over inside a stdlib arm's window (`rcarms`) is the arm's
//! own (an inlined `list.get` takes its payload's +1, which the witness
//! accounts as the arm's handed-over credit): counted (`arm_internal`), not
//! refused — the arm-certificate residue of #4278.
//!
//! Likewise an RC call left over inside a taken compound routine's window
//! (`rcroutines`: an in-place append, string building, a map edit — every
//! route the emitter takes through `Emitter::routine`) is that routine's own,
//! counted (`routine_internal`); and one whose argument is a value loaded out
//! of a heap block's slot is the container's element credit, counted
//! (`element`). Each count is printed per build, apart from the others.
//!
//! An event on an OUTER holder names it by the recorder's pseudo local (a
//! global `0xC000_0000 | g`, a C-319 cell `0xA000_0000 | l`): it matches a
//! call whose argument was read through that global, or loaded out of the
//! block cell local `l` holds.
//!
//! A variable's recorded release with no release call of its own matches a
//! call inside a stdlib arm's or a compound routine's window that takes the
//! variable as an argument — the arm or routine consumes it — counted
//! (`consumed`).
//!
//! TRUSTED (#4278, until its arm, routine and container certificates exist):
//! that `rcfns` names every rc_inc and release routine; that an arm's own RC
//! keeps the balance its ownership declaration states; that a compound
//! routine's own RC keeps the balance of the operation the witness records;
//! and that an element's RC keeps its container's credit invariant.

use std::collections::HashSet;

use wasmparser::Operator as Op;

use crate::decode::Body;
use crate::operands::{chain, Flow, Src, FUEL};

/// What the RC check saw.
#[derive(Default, Debug)]
pub struct RcReport {
    pub frames: usize,
    pub events: usize,
    pub calls: usize,
    pub pre_abort: usize,
    pub rebind_unmatched: usize,
    pub judges_unmatched: usize,
    pub arm_internal: usize,
    pub routine_internal: usize,
    pub element: usize,
    pub consumed: usize,
}

/// Instructions no path reaches: after `br` / `br_table` / `return` /
/// `unreachable` / a return call, up to the end of that block.
fn dead(b: &Body<'_>) -> Vec<bool> {
    let mut out = vec![false; b.ops.len()];
    // Per open block, whether its rest is dead.
    let mut stack: Vec<bool> = vec![false];
    for (k, op) in b.ops.iter().enumerate() {
        let d = *stack.last().unwrap_or(&false);
        out[k] = d && !matches!(op, Op::End | Op::Else);
        match op {
            Op::Block { .. } | Op::Loop { .. } | Op::If { .. } | Op::TryTable { .. } => stack.push(d),
            Op::Else => {
                stack.pop();
                let outer = *stack.last().unwrap_or(&false);
                stack.push(outer);
            }
            Op::End => {
                stack.pop();
            }
            Op::Br { .. }
            | Op::BrTable { .. }
            | Op::Return
            | Op::Unreachable
            | Op::ReturnCall { .. }
            | Op::ReturnCallIndirect { .. } => {
                if let Some(t) = stack.last_mut() {
                    *t = true;
                }
            }
            _ => {}
        }
    }
    out
}

/// The release at `k` is followed, in its block, only by calls, constants,
/// local reads and drops until `unreachable`.
fn before_abort(b: &Body<'_>, k: usize) -> bool {
    for op in &b.ops[k + 1..] {
        match op {
            Op::Unreachable => return true,
            Op::Call { .. } | Op::I32Const { .. } | Op::I64Const { .. } | Op::LocalGet { .. } | Op::Drop => {}
            _ => return false,
        }
    }
    false
}

/// The recorder's pseudo locals for an OUTER holder (almide-wasm
/// `witness_holder`): a global, and a C-319 cell (the frame local holding
/// the cell).
const GLOBAL_HOLDER: u32 = 0xC000_0000;
const CELL_HOLDER: u32 = 0xA000_0000;

/// The names an RC call's argument answers to, in the recorder's terms: each
/// local it was read through, each global (as its holder pseudo local), and,
/// for a value loaded out of a block, the cell locals that block was read
/// through (the cell's occupant).
fn names(arg: &crate::operands::T) -> Vec<u32> {
    let mut v: Vec<u32> = chain(arg)
        .into_iter()
        .map(|(s, _)| match s {
            Src::Local(l) => l,
            Src::Global(g) => GLOBAL_HOLDER | g,
        })
        .collect();
    if let Some(addr) = crate::operands::loaded_from(arg) {
        v.extend(chain(addr).into_iter().filter_map(|(s, _)| match s {
            Src::Local(l) => Some(CELL_HOLDER | l),
            Src::Global(_) => None,
        }));
    }
    v
}

/// Check the `rc` lines (`frames` as `(position, events)`) over `bodies`.
pub fn check_rc(
    bodies: &[Body<'_>],
    inc: Option<u32>,
    releases: &HashSet<u32>,
    judges: &[crate::At],
    frames: &[(usize, String)],
    windows: &[Vec<(usize, usize, usize)>; 2],
    rep: &mut RcReport,
) -> Result<(), String> {
    for (pos, events) in frames {
        let judged: HashSet<usize> = judges.iter().filter(|j| j.func == *pos).map(|j| j.op).collect();
        let b = bodies.get(*pos).ok_or_else(|| format!("rc: no defined function at position {pos}"))?;
        let fl = Flow::new(b, HashSet::new());
        let dead = dead(b);
        // The live RC calls: (instruction, kind, the locals its argument was
        // read through). Those on a path that aborts take part in matching;
        // left over, they are counted, not refused (#4278).
        let mut calls: Vec<(usize, char, Vec<u32>)> = Vec::new();
        // Calls whose argument is a value loaded out of a heap block's slot.
        let mut elements: HashSet<usize> = HashSet::new();
        let mut aborting: HashSet<usize> = HashSet::new();
        for (k, op) in b.ops.iter().enumerate() {
            let Op::Call { function_index } = op else { continue };
            let kind = if Some(*function_index) == inc {
                'a'
            } else if releases.contains(function_index) {
                'd'
            } else if judged.contains(&k) {
                'j'
            } else {
                continue;
            };
            if dead[k] {
                continue;
            }
            if fl.in_abort_arm(k) || (kind == 'd' && before_abort(b, k)) {
                aborting.insert(k);
            }
            let arg = fl.arg(k, 0, FUEL);
            if crate::operands::is_loaded(&arg) {
                elements.insert(k);
            }
            calls.push((k, kind, names(&arg)));
        }
        rep.calls += calls.iter().filter(|c| c.1 != 'j').count();
        let evs: Vec<(char, Option<u32>)> = events
            .split_whitespace()
            .map(|t| {
                let mut c = t.chars();
                let kind = c.next().unwrap_or('?');
                let rest: String = c.collect();
                (kind, rest.parse().ok())
            })
            .collect();
        rep.events += evs.len();
        let mut used = vec![false; calls.len()];
        let mut unmatched: Vec<String> = Vec::new();
        // Named events no RC call took: a release may be the consumption of
        // the variable by a stdlib arm or a compound routine (below).
        let mut pending: Vec<(char, u32)> = Vec::new();
        // Rebind releases first: a release or a judge call on their local.
        for &(_, local) in evs.iter().filter(|e| e.0 == 'o') {
            let l = local.unwrap_or(u32::MAX);
            match (0..calls.len()).find(|&i| !used[i] && matches!(calls[i].1, 'd' | 'j') && calls[i].2.contains(&l)) {
                Some(i) => used[i] = true,
                None => rep.rebind_unmatched += 1,
            }
        }
        // Named events next: each takes a call of its kind read through its local.
        for &(kind, local) in evs.iter().filter(|e| e.1.is_some() && e.0 != 'o') {
            let l = local.unwrap_or(u32::MAX);
            let on = |i: usize, k: char| !used[i] && calls[i].1 == k && calls[i].2.contains(&l);
            // A release a copy-on-write judge performs on the variable's old
            // block (it copied a shared one) is recorded as that variable's `d`.
            let found = (0..calls.len()).find(|&i| on(i, kind)).or_else(|| {
                (kind == 'd').then(|| (0..calls.len()).find(|&i| on(i, 'j'))).flatten()
            });
            match found {
                Some(i) => used[i] = true,
                None => pending.push((kind, l)),
            }
        }
        for &(kind, _) in evs.iter().filter(|e| e.1.is_none()) {
            match (0..calls.len()).find(|&i| !used[i] && calls[i].1 == kind) {
                Some(i) => used[i] = true,
                None => unmatched.push(format!("{kind}-")),
            }
        }
        rep.judges_unmatched += (0..calls.len()).filter(|&i| !used[i] && calls[i].1 == 'j').count();
        let inside = |w: &[(usize, usize, usize)], k: usize| w.iter().any(|&(p, a, b)| p == *pos && a <= k && k <= b);
        // A variable's release that is an arm's or a routine's consumption of
        // it: a call inside its window takes the variable as an argument (the
        // arm's ownership declaration says it consumes; #4278 trusts that).
        for (kind, l) in pending {
            let consumed = kind == 'd'
                && b.ops.iter().enumerate().any(|(k, op)| {
                    matches!(op, Op::Call { .. })
                        && !dead[k]
                        && (inside(&windows[0], k) || inside(&windows[1], k))
                        && (0..b.args[k].len()).any(|j| names(&fl.arg(k, j, FUEL)).contains(&l))
                });
            if consumed {
                rep.consumed += 1;
            } else {
                unmatched.push(format!("{kind}{l}"));
            }
        }
        for i in 0..calls.len() {
            if used[i] || calls[i].1 == 'j' {
                continue;
            }
            let k = calls[i].0;
            let slot = if inside(&windows[0], k) {
                &mut rep.arm_internal
            } else if inside(&windows[1], k) {
                &mut rep.routine_internal
            } else if elements.contains(&k) {
                &mut rep.element
            } else if aborting.contains(&k) {
                &mut rep.pre_abort
            } else {
                continue;
            };
            *slot += 1;
            used[i] = true;
        }
        let extra: Vec<String> = (0..calls.len())
            .filter(|&i| !used[i] && calls[i].1 != 'j')
            .map(|i| format!("{}@{} on {:?}", calls[i].1, calls[i].0, calls[i].2))
            .collect();
        if !unmatched.is_empty() || !extra.is_empty() {
            return Err(format!(
                "rc: function {pos}: recorded events with no call [{}]; calls with no recorded event [{}]",
                unmatched.join(" "),
                extra.join(", ")
            ));
        }
        rep.frames += 1;
    }
    Ok(())
}
