//! #4277: the ownership witness's RC events against the shipped calls.
//!
//! For each certified frame the anchor file names (`rc <position>|<events>`),
//! every live call in that body to the rc_inc routine or to a release
//! routine (`rcfns <inc> <release> ...`) must be one recorded event of its
//! kind, and every recorded event one such call: an event that names a local
//! matches only a call whose argument was read through that local
//! (operands.rs `chain`); a temporary's event (`-`) matches any call of its
//! kind. A call in dead code (after an unconditional branch in its block) or
//! in an aborting arm is not live. A release whose block then aborts (only
//! calls, constants and local reads between it and `unreachable`) is counted
//! apart: the witness does not record it by design (#4278).
//!
//! TRUSTED: that `rcfns` names every rc_inc and release routine.

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

/// Check the `rc` lines (`frames` as `(position, events)`) over `bodies`.
pub fn check_rc(bodies: &[Body<'_>], inc: Option<u32>, releases: &HashSet<u32>, frames: &[(usize, String)], rep: &mut RcReport) -> Result<(), String> {
    for (pos, events) in frames {
        let b = bodies.get(*pos).ok_or_else(|| format!("rc: no defined function at position {pos}"))?;
        let fl = Flow::new(b, HashSet::new());
        let dead = dead(b);
        // The live RC calls: (instruction, kind, the locals its argument was read through).
        let mut calls: Vec<(usize, char, Vec<u32>)> = Vec::new();
        for (k, op) in b.ops.iter().enumerate() {
            let Op::Call { function_index } = op else { continue };
            let kind = if Some(*function_index) == inc {
                'a'
            } else if releases.contains(function_index) {
                'd'
            } else {
                continue;
            };
            if dead[k] || fl.in_abort_arm(k) {
                continue;
            }
            if kind == 'd' && before_abort(b, k) {
                rep.pre_abort += 1;
                continue;
            }
            let locals = chain(&fl.arg(k, 0, FUEL))
                .into_iter()
                .filter_map(|(s, _)| match s {
                    Src::Local(l) => Some(l),
                    Src::Global(_) => None,
                })
                .collect();
            calls.push((k, kind, locals));
        }
        rep.calls += calls.len();
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
        // Named events first: each takes a call of its kind read through its local.
        for &(kind, local) in evs.iter().filter(|e| e.1.is_some()) {
            let l = local.unwrap_or(u32::MAX);
            match (0..calls.len()).find(|&i| !used[i] && calls[i].1 == kind && calls[i].2.contains(&l)) {
                Some(i) => used[i] = true,
                None => unmatched.push(format!("{kind}{l}")),
            }
        }
        for &(kind, _) in evs.iter().filter(|e| e.1.is_none()) {
            match (0..calls.len()).find(|&i| !used[i] && calls[i].1 == kind) {
                Some(i) => used[i] = true,
                None => unmatched.push(format!("{kind}-")),
            }
        }
        let extra: Vec<String> = (0..calls.len())
            .filter(|&i| !used[i])
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
