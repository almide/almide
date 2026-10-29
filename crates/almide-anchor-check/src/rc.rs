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
//! A variable's recorded release (or +1) with no RC call of its own matches
//! a call inside a stdlib arm's or a compound routine's window that takes
//! the variable as an argument — the arm or routine consumes it, or takes
//! the +1 its argument guard prepays — counted (`consumed`).
//!
//! The windows and the element class are confined (sec-rows-2-3-4, the
//! drills of proofs/fixtures/rc-witness/reinject/): a leftover call counts
//! as an arm's, a routine's or an element's only if its argument names no
//! variable the frame's events name; an element release only if a later
//! store writes the same slot; and a release or +1 counts as `consumed` only
//! by a call inside an arm or routine window whose callee's own body takes
//! ownership of that parameter on EVERY path to its exits (a callee that
//! takes it on some paths only is refused — the every-path rule, adopted
//! 2026-10-28). When a runtime routine fails that rule the routine is the
//! bug and is fixed, never the rule: `$list_push` and `$bytes_push` released
//! their outgrown block only when uniquely owned, and now release it on
//! every grow path.
//!
//! The four counted classes (arm-internal, routine-internal, element,
//! consumed) are debts to #4278's per-helper certificates — a declared
//! per-path effect for each helper, checked against its body. They may only
//! shrink; a new trusted class is an owner decision, not a checker edit.
//!
//! TRUSTED (#4278, until its arm, routine and container certificates exist):
//! that `rcfns` names every rc_inc and release routine; that an arm's own RC
//! keeps the balance its ownership declaration states; that a compound
//! routine's own RC keeps the balance of the operation the witness records;
//! and that an element's RC keeps its container's credit invariant.

use std::collections::HashSet;

use wasmparser::Operator as Op;

use crate::decode::{store_width, Body};
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

/// How function `f` takes ownership of its parameter `j`: `(some, all)` —
/// on some path, and on every path to an exit. A path takes it by releasing
/// it, storing it into a block, returning it, or handing it to a callee that
/// takes it on every path (followed `depth` calls deep). An exit is covered
/// when it returns or tail-passes the parameter, or a taking instruction
/// dominates it. An imported function takes nothing.
fn takes(bodies: &[Body<'_>], imports: u32, releases: &HashSet<u32>, f: u32, j: u32, depth: u32) -> (bool, bool) {
    if depth == 0 {
        return (false, false);
    }
    let Some(b) = f.checked_sub(imports).and_then(|p| bodies.get(p as usize)) else { return (false, false) };
    let fl = Flow::new(b, HashSet::new());
    let dead = dead(b);
    let last = b.ops.len().saturating_sub(1);
    let names_j = |k: usize, a: usize| names(&fl.arg(k, a, FUEL)).contains(&j);
    let mut points: Vec<usize> = Vec::new();
    let mut exits: Vec<(usize, bool)> = Vec::new();
    for (k, op) in b.ops.iter().enumerate() {
        if dead[k] {
            continue;
        }
        match op {
            Op::Call { function_index: g } => {
                if (0..b.args[k].len()).any(|a| {
                    names_j(k, a) && (releases.contains(g) || takes(bodies, imports, releases, *g, a as u32, depth - 1).1)
                }) {
                    points.push(k);
                }
            }
            Op::ReturnCall { function_index: g } => {
                let passes = (0..b.args[k].len()).any(|a| {
                    names_j(k, a) && (releases.contains(g) || takes(bodies, imports, releases, *g, a as u32, depth - 1).1)
                });
                exits.push((k, passes));
            }
            Op::Return => exits.push((k, (0..b.args[k].len()).any(|a| names_j(k, a)))),
            Op::End if k == last => exits.push((k, (0..b.args[k].len()).any(|a| names_j(k, a)))),
            o if store_width(o).is_some() && names_j(k, 1) => points.push(k),
            _ => {}
        }
    }
    let some = !points.is_empty() || exits.iter().any(|e| e.1);
    let all = !exits.is_empty() && exits.iter().all(|&(e, ret)| ret || points.iter().any(|&c| fl.dominates(c, e)));
    (some, all)
}

/// The release at `k` of a value loaded out of a slot replaces it: a later
/// store writes the same slot (the container's element is swapped, its old
/// credit released).
fn replaces(fl: &Flow<'_, '_>, b: &Body<'_>, k: usize) -> bool {
    let arg = fl.arg(k, 0, FUEL);
    let Some((addr, off)) = crate::operands::load_parts(&arg) else { return false };
    (k + 1..b.ops.len()).any(|k2| match store_width(&b.ops[k2]) {
        Some((_, o)) => o == off && fl.same(addr, &fl.arg(k2, 0, FUEL)),
        None => false,
    })
}

/// Check the `rc` lines (`frames` as `(position, events)`) over `bodies`
/// (`imports`: the function imports before them in the index space).
pub fn check_rc(
    bodies: &[Body<'_>],
    imports: u32,
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
        // Each event, and for a move `m<local>@<from>-<to>` the window of
        // the arm or routine the credit moved into.
        let mut evwin: Vec<Option<(usize, usize)>> = Vec::new();
        let evs: Vec<(char, Option<u32>)> = events
            .split_whitespace()
            .map(|t| {
                let mut c = t.chars();
                let kind = c.next().unwrap_or('?');
                let rest: String = c.collect();
                let (local, win) = match rest.split_once('@') {
                    Some((l, w)) => (l, w.split_once('-').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))),
                    None => (rest.as_str(), None),
                };
                evwin.push(win);
                (kind, local.parse().ok())
            })
            .collect();
        rep.events += evs.len();
        let mut used = vec![false; calls.len()];
        let mut unmatched: Vec<String> = Vec::new();
        // Named events no RC call took: a release may be the consumption of
        // the variable by a stdlib arm or a compound routine (below).
        let mut pending: Vec<(char, u32)> = Vec::new();
        // Every variable the frame's events name: an arm's, a routine's or a
        // container's own RC never touches one of these (that would be the
        // frame's credit, which its events must account).
        let witnessed: HashSet<u32> = evs.iter().filter_map(|e| e.1).collect();
        // Events to calls: a MAXIMUM matching, each call taking at most one
        // event, so the result never depends on the order the recorder logged
        // events in (an alias's reads name several variables; a first-fit
        // pass let one variable's event take the call another's needed).
        // Compatible pairs:
        // - a named `a` / `d` takes a call of its kind read through its local;
        //   a `d` may instead take a copy-on-write judge call on its local (the
        //   judge released the variable's old block when it copied it);
        // - an unnamed `a-` / `d-` takes any call of its kind;
        // - a rebind `o` takes a release or a judge call on its local;
        // - a move `m` (the variable's credit handed into an arm or a
        //   routine) takes a release on its local INSIDE that window — the
        //   arm's or routine's release of the credit it was given. A move
        //   the arm keeps (stores or returns) takes nothing.
        // The named and unnamed events are matched first; a rebind takes
        // what is left (augmenting paths never unmatch an earlier event);
        // each event tries the calls outside every arm and routine window
        // before those inside one.
        let fits = |e: usize, i: usize| -> bool {
            let (k, names_of) = (calls[i].1, &calls[i].2);
            match evs[e] {
                ('o', Some(l)) => matches!(k, 'd' | 'j') && names_of.contains(&l),
                ('m', Some(l)) => {
                    k == 'd' && names_of.contains(&l) && evwin[e].is_some_and(|(a, b)| a <= calls[i].0 && calls[i].0 <= b)
                }
                ('m', None) => false,
                (kind, Some(l)) => (k == kind || (kind == 'd' && k == 'j')) && names_of.contains(&l),
                (kind, None) => k == kind,
            }
        };
        let order: Vec<usize> = (0..evs.len())
            .filter(|&e| !matches!(evs[e].0, 'o' | 'm'))
            .chain((0..evs.len()).filter(|&e| matches!(evs[e].0, 'o' | 'm')))
            .collect();
        // A call inside an arm or routine window is tried last: left over,
        // it is that arm's or routine's own; one outside has no such reading.
        let in_window = |k: usize| windows.iter().flatten().any(|&(p, a, b)| p == *pos && a <= k && k <= b);
        let mut by_pref: Vec<usize> = (0..calls.len()).collect();
        by_pref.sort_by_key(|&i| (in_window(calls[i].0), calls[i].0));
        let mut owner: Vec<Option<usize>> = vec![None; calls.len()];
        fn augment(
            e: usize,
            by_pref: &[usize],
            fits: &dyn Fn(usize, usize) -> bool,
            seen: &mut [bool],
            owner: &mut [Option<usize>],
        ) -> bool {
            for &i in by_pref {
                if seen[i] || !fits(e, i) {
                    continue;
                }
                seen[i] = true;
                if owner[i].is_none_or(|o| augment(o, by_pref, fits, seen, owner)) {
                    owner[i] = Some(e);
                    return true;
                }
            }
            false
        }
        for &e in &order {
            let mut seen = vec![false; calls.len()];
            augment(e, &by_pref, &fits, &mut seen, &mut owner);
        }
        // Who holds a call can change along an augmenting path; read the
        // final matching.
        let mut got = vec![false; evs.len()];
        for o in owner.iter().flatten() {
            got[*o] = true;
        }
        for (i, o) in owner.iter().enumerate() {
            used[i] = o.is_some();
        }
        for (e, &(kind, local)) in evs.iter().enumerate() {
            if got[e] {
                continue;
            }
            match (kind, local) {
                ('o', _) => rep.rebind_unmatched += 1,
                ('m', _) => {}
                // Named events no RC call took: a release may be the
                // consumption of the variable by a stdlib arm or a compound
                // routine (below).
                (k, Some(l)) => pending.push((k, l)),
                (k, None) => unmatched.push(format!("{k}-")),
            }
        }
        rep.judges_unmatched += (0..calls.len()).filter(|&i| !used[i] && calls[i].1 == 'j').count();
        let inside = |w: &[(usize, usize, usize)], k: usize| w.iter().any(|&(p, a, b)| p == *pos && a <= k && k <= b);
        // A variable's release (or prepaid +1) that is an arm's or a routine's
        // consumption of it: a call INSIDE an arm or routine window takes the
        // variable's value at that call as argument `j`, and the callee's own
        // body takes ownership of parameter `j` (releases, stores, returns or
        // passes it on). Each such argument accounts for ONE event, and never
        // a call an event above already matched (a judge's release): two
        // consumptions need two taking arguments.
        let matched: HashSet<usize> = (0..calls.len()).filter(|&i| used[i]).map(|i| calls[i].0).collect();
        let mut taken: HashSet<(usize, usize)> = HashSet::new();
        for (kind, l) in pending {
            // The first free taking argument, or else whether any took it on
            // some paths only.
            let mut partial = false;
            let mut chosen = None;
            if matches!(kind, 'd' | 'a') {
                'calls: for (k, op) in b.ops.iter().enumerate() {
                    let Op::Call { function_index: g } = op else { continue };
                    if dead[k] || matched.contains(&k) || !(inside(&windows[0], k) || inside(&windows[1], k)) {
                        continue;
                    }
                    for j in 0..b.args[k].len() {
                        if taken.contains(&(k, j)) {
                            continue;
                        }
                        let arg = fl.arg(k, j, FUEL);
                        if !(names(&arg).contains(&l) || fl.same(&arg, &fl.local_value(l, k, FUEL))) {
                            continue;
                        }
                        let (some, all) = takes(bodies, imports, releases, *g, j as u32, 4);
                        if all {
                            chosen = Some((k, j));
                            break 'calls;
                        }
                        partial |= some;
                    }
                }
            }
            if let Some(kj) = chosen {
                taken.insert(kj);
            }
            match (partial, chosen.is_some()) {
                (_, true) => rep.consumed += 1,
                // The every-path rule (adopted 2026-10-28): a callee that
                // takes the parameter on some paths but not all is refused.
                (true, false) => unmatched.push(format!("{kind}{l} (its consuming callee takes it on some paths only)")),
                _ => unmatched.push(format!("{kind}{l}")),
            }
        }
        for i in 0..calls.len() {
            if used[i] || calls[i].1 == 'j' {
                continue;
            }
            let k = calls[i].0;
            // A frame variable's credit is never an arm's, a routine's or an
            // element's: such a call stays unaccounted.
            let frame_owned = calls[i].2.iter().any(|n| witnessed.contains(n));
            let slot = if !frame_owned && inside(&windows[0], k) {
                &mut rep.arm_internal
            } else if !frame_owned && inside(&windows[1], k) {
                &mut rep.routine_internal
            } else if !frame_owned && elements.contains(&k) && (calls[i].1 == 'a' || replaces(&fl, b, k)) {
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
