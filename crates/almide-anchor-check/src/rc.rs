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

use std::collections::{HashMap, HashSet};

use wasmparser::Operator as Op;

use crate::decode::{store_width, Body};
use crate::operands::{chain, Flow, Src, FUEL};

/// `--explain`: a refusal lists every RC call of the frame and the event
/// (or counted class) that took it. Diagnosis output only; the verdict is
/// the same.
pub static EXPLAIN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
    // Per open construct: whether the code now reached in it is dead, the
    // then-arm's verdict once an `else` begins, whether a branch targets its
    // end, and whether it is a loop (a branch to a loop re-enters it).
    struct Frame {
        dead: bool,
        then_dead: Option<bool>,
        targeted: bool,
        is_loop: bool,
        is_if: bool,
    }
    let mut out = vec![false; b.ops.len()];
    let mut stack: Vec<Frame> = vec![Frame { dead: false, then_dead: None, targeted: false, is_loop: false, is_if: false }];
    let target = |stack: &mut Vec<Frame>, depth: u32| {
        let n = stack.len();
        if let Some(f) = (depth as usize).checked_add(1).and_then(|d| n.checked_sub(d)).and_then(|i| stack.get_mut(i)) {
            f.targeted = true;
        }
    };
    for (k, op) in b.ops.iter().enumerate() {
        let d = stack.last().is_some_and(|f| f.dead);
        out[k] = d && !matches!(op, Op::End | Op::Else);
        match op {
            Op::Block { .. } | Op::Loop { .. } | Op::If { .. } | Op::TryTable { .. } => stack.push(Frame {
                dead: d,
                then_dead: None,
                targeted: false,
                is_loop: matches!(op, Op::Loop { .. }),
                is_if: matches!(op, Op::If { .. }),
            }),
            Op::Else => {
                let outer = stack.len() >= 2 && stack[stack.len() - 2].dead;
                if let Some(f) = stack.last_mut() {
                    f.then_dead = Some(f.dead);
                    f.dead = outer;
                }
            }
            Op::End => {
                let Some(f) = stack.pop() else { continue };
                // Past the end: reached when the body falls through (both arms
                // of an `if` with an `else`; an `if` without one always can),
                // or a branch targets the end (never a loop's: that re-enters).
                let falls_through = match (f.is_if, f.then_dead) {
                    (true, Some(t)) => !(t && f.dead),
                    (true, None) => true,
                    _ => !f.dead,
                };
                let reached = falls_through || (f.targeted && !f.is_loop);
                if let Some(p) = stack.last_mut() {
                    if !reached {
                        p.dead = true;
                    }
                }
            }
            Op::Br { relative_depth } => {
                if !d {
                    target(&mut stack, *relative_depth);
                }
                if let Some(t) = stack.last_mut() {
                    t.dead = true;
                }
            }
            Op::BrIf { relative_depth } => {
                if !d {
                    target(&mut stack, *relative_depth);
                }
            }
            Op::BrTable { targets } => {
                if !d {
                    for t in targets.targets().flatten().chain(std::iter::once(targets.default())) {
                        target(&mut stack, t);
                    }
                }
                if let Some(t) = stack.last_mut() {
                    t.dead = true;
                }
            }
            Op::Return | Op::Unreachable | Op::ReturnCall { .. } | Op::ReturnCallIndirect { .. } => {
                if let Some(t) = stack.last_mut() {
                    t.dead = true;
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
/// Callee `f` hands its parameter `j` back as its result on some path.
fn returns_param(bodies: &[Body<'_>], imports: u32, f: u32, j: u32) -> bool {
    let Some(b) = f.checked_sub(imports).and_then(|p| bodies.get(p as usize)) else { return false };
    let fl = Flow::new(b, HashSet::new());
    let dead = dead(b);
    let last = b.ops.len().saturating_sub(1);
    b.ops.iter().enumerate().any(|(k, op)| {
        !dead[k]
            && (matches!(op, Op::Return) || (matches!(op, Op::End) && k == last))
            && (0..b.args[k].len()).any(|a| names(&fl.arg(k, a, FUEL)).contains(&j))
    })
}

/// [`names`], and for a call's result, the names of each argument the callee
/// hands back as its result on some path: the result may be that block (a
/// view the callee returns as is), so its RC is that variable's.
fn names_through(bodies: &[Body<'_>], imports: u32, b: &Body<'_>, judged: &HashSet<usize>, arg: &crate::operands::T, depth: u32) -> Vec<u32> {
    let mut v = names(arg);
    if depth == 0 {
        return v;
    }
    let crate::operands::T::Call { site, args } = crate::operands::pl(arg) else { return v };
    if let Some(Op::Call { function_index: g }) = b.ops.get(*site) {
        for (j, a) in args.iter().enumerate() {
            // A recorded copy-on-write judge returns the variable's own block
            // (the one it judged, or the copy that replaced it): its result
            // is the judged argument's credit (the judge site's TRUSTED reading).
            if returns_param(bodies, imports, *g, j as u32) || (j == 0 && judged.contains(site)) {
                for n in names_through(bodies, imports, b, judged, a, depth - 1) {
                    if !v.contains(&n) {
                        v.push(n);
                    }
                }
            }
        }
    }
    v
}

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
        let mut dead_calls: Vec<(usize, char, Vec<u32>)> = Vec::new();
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
                // An unreachable RC call: never run, so never refused; an
                // event the recorder logged on a path that cannot reach it
                // (an arm that aborts) may still name it, last of all.
                dead_calls.push((k, kind, names(&fl.arg(k, 0, FUEL))));
                continue;
            }
            if fl.in_abort_arm(k) || (kind == 'd' && before_abort(b, k)) {
                aborting.insert(k);
            }
            let arg = fl.arg(k, 0, FUEL);
            if crate::operands::is_loaded(&arg) {
                elements.insert(k);
            }
            let named = names_through(bodies, imports, b, &judged, &arg, 2);
            calls.push((k, kind, named));
        }
        rep.calls += calls.iter().filter(|c| c.1 != 'j').count();
        // The unreachable calls follow the live ones.
        let live = calls.len();
        calls.extend(dead_calls);
        // Each event, and for a move `m<local>@<from>-<to>` the window of
        // the arm or routine the credit moved into.
        let mut evwin: Vec<Option<(usize, usize)>> = Vec::new();
        // Each event's depth in the recorder's branch sites (`(` … `)`): at
        // depth 0 it happens once on every path through the frame.
        let mut evdepth: Vec<usize> = Vec::new();
        // Each event's path through the recorder's branch sites: per open
        // site, its id and the arm the event is in (`|` starts the next arm).
        let mut evpath: Vec<Vec<(usize, usize)>> = Vec::new();
        let mut open_sites: Vec<(usize, usize)> = Vec::new();
        let mut next_site = 0usize;
        // A site the recorder anchored (`(@<if>-<else>`, an `if` lowered with
        // its arms in source order): the shipped `if` and `else` it is.
        let mut site_at: HashMap<usize, (usize, usize)> = HashMap::new();
        let anchor = |t: &str| t.get(2..)?.split_once('-').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)));
        let evs: Vec<(char, Option<u32>)> = events
            .split_whitespace()
            .filter(|t| match *t {
                t if t.starts_with('(') => {
                    next_site += 1;
                    open_sites.push((next_site, 0));
                    if let Some(at) = anchor(t) {
                        site_at.insert(next_site, at);
                    }
                    false
                }
                t if t.starts_with('|') => {
                    if let Some(s) = open_sites.last_mut() {
                        s.1 += 1;
                        // An arm boundary anchored elsewhere than its site:
                        // the site is not held to the shipped arms.
                        if anchor(t) != site_at.get(&s.0).copied() {
                            site_at.remove(&s.0);
                        }
                    }
                    false
                }
                ")" => {
                    open_sites.pop();
                    false
                }
                _ => {
                    evdepth.push(open_sites.len());
                    evpath.push(open_sites.clone());
                    true
                }
            })
            .collect::<Vec<_>>()
            .into_iter()
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
        //   arm's or routine's release of the credit it was given, one per
        //   path (below). A move the arm keeps (stores or returns) takes
        //   nothing.
        // The rebinds are matched first, the named and unnamed events next,
        // the moves last (augmenting paths never unmatch an earlier event).
        // A rebind may go without an instruction, so it must not be left the
        // call a release elsewhere was dropped from: matched after the
        // releases, it would let a release take the rebind's call and go
        // unmatched itself, hiding the dropped release (the
        // rc-release-dropped drill). A move takes only what the frame's own
        // events left inside its window — its credit may as well leave
        // through the callee (a tail call) with no release here;
        // each event tries the calls outside every arm and routine window
        // before those inside one.
        // - a named `a` / `d` may instead be an arm's or a routine's
        //   consumption of the variable: a call INSIDE an arm or routine
        //   window (no RC call itself) takes the variable's value at that
        //   call as argument `j`, and the callee's own body takes ownership
        //   of parameter `j` on every path (releases, stores, returns or
        //   passes it on). Each such argument is one node of the matching,
        //   so it accounts for ONE event.
        let nc = calls.len();
        let inside = |w: &[(usize, usize, usize)], k: usize| w.iter().any(|&(p, a, b)| p == *pos && a <= k && k <= b);
        let rc_call: HashSet<usize> = calls.iter().map(|c| c.0).collect();
        let mut takers: Vec<(usize, usize, u32)> = Vec::new();
        for (k, op) in b.ops.iter().enumerate() {
            let Op::Call { function_index: g } = op else { continue };
            if dead[k] || rc_call.contains(&k) || !(inside(&windows[0], k) || inside(&windows[1], k)) {
                continue;
            }
            for j in 0..b.args[k].len() {
                takers.push((k, j, *g));
            }
        }
        let verdicts: std::cell::RefCell<std::collections::HashMap<(u32, usize), (bool, bool)>> = Default::default();
        let verdict = |g: u32, j: usize| -> (bool, bool) {
            *verdicts.borrow_mut().entry((g, j)).or_insert_with(|| takes(bodies, imports, releases, g, j as u32, 4))
        };
        // Per variable a named event carries, which consuming arguments hold
        // its value at their call (computed once per variable: a term per
        // argument, compared against the variable's value there).
        let args: Vec<crate::operands::T> = takers.iter().map(|&(k, j, _)| fl.arg(k, j, FUEL)).collect();
        let holding: std::cell::RefCell<std::collections::HashMap<u32, Vec<bool>>> = Default::default();
        let holds = |l: u32, t: usize| -> bool {
            holding.borrow_mut()
                .entry(l)
                .or_insert_with(|| {
                    (0..takers.len())
                        .map(|t| {
                            names_through(bodies, imports, b, &judged, &args[t], 2).contains(&l)
                                || fl.same(&args[t], &fl.local_value(l, takers[t].0, FUEL))
                        })
                        .collect()
                })[t]
        };
        let temp_arg: Vec<bool> = (0..takers.len())
            .map(|t| !names_through(bodies, imports, b, &judged, &args[t], 2).iter().any(|n| witnessed.contains(n)))
            .collect();
        let fits = |e: usize, i: usize| -> bool {
            if i >= nc {
                let t = i - nc;
                return match evs[e] {
                    ('a' | 'd', Some(l)) => holds(l, t) && verdict(takers[t].2, takers[t].1).1,
                    // A temporary's event logged inside an arm or routine
                    // window: that window's call taking a temporary (no frame
                    // variable) on every path.
                    ('a' | 'd', None) => {
                        evwin[e].is_some_and(|(a, z)| a <= takers[t].0 && takers[t].0 <= z)
                            && temp_arg[t]
                            && verdict(takers[t].2, takers[t].1).1
                    }
                    _ => false,
                };
            }
            let (k, names_of) = (calls[i].1, &calls[i].2);
            match evs[e] {
                ('o', Some(l)) => matches!(k, 'd' | 'j') && names_of.contains(&l),
                // The arm or routine the credit moved into, or one enclosing
                // it (a callback's move released by the fold that called it).
                ('m', Some(l)) => {
                    k == 'd'
                        && names_of.contains(&l)
                        && evwin[e].is_some_and(|(a, z)| {
                            windows.iter().flatten().any(|&(p, x, y)| p == *pos && x <= a && z <= y && x <= calls[i].0 && calls[i].0 <= y)
                                || (a <= calls[i].0 && calls[i].0 <= z)
                        })
                }
                ('m', None) => false,
                // A C-319 cell variable's events name its local; a call on
                // the block the cell holds reads through it (`CELL_HOLDER | l`).
                // Both are that variable's credit, matched one to one.
                (kind, Some(l)) => {
                    (k == kind || (kind == 'd' && k == 'j'))
                        && (names_of.contains(&l) || (l < GLOBAL_HOLDER.min(CELL_HOLDER) && names_of.contains(&(CELL_HOLDER | l))))
                }
                // A temporary's event logged inside an arm or routine window
                // records an instruction emitted inside it: it takes a call
                // in that window, never a release elsewhere it does not own.
                (kind, None) => k == kind && evwin[e].is_none_or(|(a, z)| a <= calls[i].0 && calls[i].0 <= z),
            }
        };
        let order: Vec<usize> = (0..evs.len())
            .filter(|&e| evs[e].0 == 'o')
            .chain((0..evs.len()).filter(|&e| !matches!(evs[e].0, 'o' | 'm')))
            .chain((0..evs.len()).filter(|&e| evs[e].0 == 'm'))
            .collect();
        // A call inside an arm or routine window is tried after those
        // outside (left over, it is that arm's or routine's own; one outside
        // has no such reading), and a consuming argument last.
        let in_window = |k: usize| windows.iter().flatten().any(|&(p, a, b)| p == *pos && a <= k && k <= b);
        // More generally: a call that has a reading of its own when left over
        // (an arm's or routine's, a replaced element's, one before an abort)
        // is tried after the calls that have none.
        let fallback = |i: usize| {
            if i >= live {
                return true;
            }
            let k = calls[i].0;
            let frame_owned = calls[i].2.iter().any(|n| witnessed.contains(n));
            (!frame_owned && (in_window(k) || (elements.contains(&k) && (calls[i].1 == 'a' || replaces(&fl, b, k)))))
                || aborting.contains(&k)
        };
        let mut by_pref: Vec<usize> = (0..nc).collect();
        by_pref.sort_by_key(|&i| (i >= live, fallback(i), in_window(calls[i].0), calls[i].0));
        by_pref.extend(nc..nc + takers.len());
        let mut owner: Vec<Option<usize>> = vec![None; nc + takers.len()];
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
        // First the RC calls alone, so as many of them as possible are
        // accounted; then each event still unmatched may reach a consuming
        // argument, re-routing others along the way (an augmenting path never
        // frees a node, so no call matched in the first pass is left over).
        let calls_only: Vec<usize> = by_pref.iter().copied().filter(|&i| i < nc).collect();
        for &e in &order {
            let mut seen = vec![false; owner.len()];
            augment(e, &calls_only, &fits, &mut seen, &mut owner);
        }
        for &e in &order {
            if owner.contains(&Some(e)) {
                continue;
            }
            let mut seen = vec![false; owner.len()];
            augment(e, &by_pref, &fits, &mut seen, &mut owner);
        }
        // Then from the calls' side: a call with no reading of its own still
        // unmatched takes an event along an alternating path that ends at an
        // unmatched event, or frees a call that has a reading of its own — so
        // the leftovers are, as far as any matching allows, the calls that
        // have one (never fewer events matched).
        fn rehome(
            c: usize,
            n_events: usize,
            fits: &dyn Fn(usize, usize) -> bool,
            fallback: &dyn Fn(usize) -> bool,
            seen: &mut [bool],
            owner: &mut [Option<usize>],
        ) -> bool {
            for e in 0..n_events {
                if seen[e] || !fits(e, c) {
                    continue;
                }
                seen[e] = true;
                match owner.iter().position(|&o| o == Some(e)) {
                    None => {
                        owner[c] = Some(e);
                        return true;
                    }
                    Some(c2) if fallback(c2) || rehome(c2, n_events, fits, fallback, seen, owner) => {
                        if owner[c2] == Some(e) {
                            owner[c2] = None;
                        }
                        owner[c] = Some(e);
                        return true;
                    }
                    Some(_) => {}
                }
            }
            false
        }
        let fallback_call = |i: usize| i >= nc || fallback(i);
        for c in 0..nc {
            if owner[c].is_none() && !fallback(c) {
                let mut seen = vec![false; evs.len()];
                rehome(c, evs.len(), &fits, &fallback_call, &mut seen, &mut owner);
            }
        }
        let move_fits: Vec<bool> = (0..nc).map(|i| (0..evs.len()).any(|e| evs[e].0 == 'm' && fits(e, i))).collect();
        let or_move = |i: usize| fallback_call(i) || move_fits[i];
        for round in 0..2 {
            // A move's credit is released by the arm on some of its paths: one
            // release per path. Releases of it that no run passes through two of
            // (the two arms of one `if`) are all that one move's.
            for e in (0..evs.len()).filter(|&e| evs[e].0 == 'm') {
                let mut held: Vec<usize> = (0..nc).filter(|&i| owner[i] == Some(e)).collect();
                for &i in by_pref.iter().filter(|&&i| i < nc) {
                    if owner[i].is_none() && fits(e, i) && held.iter().all(|&h| fl.exclusive(calls[h].0, calls[i].0)) {
                        owner[i] = Some(e);
                        held.push(i);
                    }
                }
            }
            // An `a` / `d` the recorder logged outside every branch site happens
            // once on each path: the same value's release (or +1) emitted once in
            // each arm of an `if` is that one event's, never twice on one path.
            for e in (0..evs.len()).filter(|&e| matches!(evs[e].0, 'a' | 'd') && evdepth[e] == 0) {
                let mut held: Vec<usize> = (0..nc).filter(|&i| owner[i] == Some(e)).collect();
                let Some(&first) = held.first() else { continue };
                for &i in by_pref.iter().filter(|&&i| i < nc) {
                    if owner[i].is_none()
                        && calls[i].1 == calls[first].1
                        && !calls[i].2.is_empty()
                        && calls[i].2 == calls[first].2
                        && held.iter().all(|&h| fl.exclusive(calls[h].0, calls[i].0))
                    {
                        owner[i] = Some(e);
                        held.push(i);
                    }
                }
            }
            if round == 1 {
                break;
            }
            // A call still left over after the absorptions (refused as it
            // stands) may end its path at a call a move could take instead
            // (its release inside the window the credit moved into): the
            // absorptions run again, and a call no move takes is still judged
            // as left over.
            for c in 0..nc {
                if owner[c].is_none() && !fallback(c) {
                    let mut seen = vec![false; evs.len()];
                    rehome(c, evs.len(), &fits, &or_move, &mut seen, &mut owner);
                }
            }
        }
        // Who holds a node can change along an augmenting path; read the
        // final matching.
        let mut got = vec![false; evs.len()];
        for o in owner.iter().flatten() {
            got[*o] = true;
        }
        // Events the recorder logged in different arms of one branch site
        // never happen on one path: one call past the join (the `+1` on the
        // value the arms chose) is each such event's, never two on a path.
        let exclusive_events = |x: usize, y: usize| {
            for (p, q) in evpath[x].iter().zip(evpath[y].iter()) {
                if p.0 != q.0 {
                    return false;
                }
                if p.1 != q.1 {
                    return true;
                }
            }
            false
        };
        let mut co: Vec<Vec<usize>> = vec![Vec::new(); nc];
        let movable = |e: usize| matches!(evs[e].0, 'a' | 'd' | 'o');
        // An alternating path ending in a share: `e` takes a call, its holder
        // (holding nothing else) takes another, … until one shares a call
        // with an event in another arm of a common site.
        type Preds<'p> = (&'p dyn Fn(usize, usize) -> bool, &'p dyn Fn(usize, usize) -> bool, &'p dyn Fn(usize) -> bool);
        fn share_path(
            e: usize,
            prefs: &[usize],
            (fits, twin, movable): Preds<'_>,
            seen: &mut [bool],
            owner: &mut [Option<usize>],
            co: &mut [Vec<usize>],
        ) -> bool {
            for &c in prefs {
                if seen[c] || !fits(e, c) {
                    continue;
                }
                seen[c] = true;
                if owner[c].is_some_and(|o| twin(e, o)) && co[c].iter().all(|&o| twin(e, o)) {
                    co[c].push(e);
                    return true;
                }
                let Some(o) = owner[c] else { continue };
                if !co[c].is_empty() || !movable(o) || owner.iter().enumerate().any(|(x, h)| x != c && *h == Some(o)) {
                    continue;
                }
                if share_path(o, prefs, (fits, twin, movable), seen, owner, co) {
                    owner[c] = Some(e);
                    return true;
                }
            }
            false
        }
        let prefs: Vec<usize> = by_pref.iter().copied().filter(|&c| c < nc).collect();
        for e in 0..evs.len() {
            if got[e] || !movable(e) {
                continue;
            }
            let mut seen = vec![false; nc];
            if share_path(e, &prefs, (&fits, &exclusive_events, &movable), &mut seen, &mut owner, &mut co) {
                got[e] = true;
            }
        }
        // A `+1` / release past the join of an anchored `if` on the value its
        // arms chose: one event in each arm, each naming its own arm's value
        // (arm i of the recorder's site is the shipped arm i — the anchor
        // holds it, never trusted), logged at the site's own level (on every
        // path through that arm).
        for c in 0..live {
            if owner[c].is_some() || !co[c].is_empty() || !matches!(calls[c].1, 'a' | 'd') {
                continue;
            }
            let arg = fl.arg(calls[c].0, 0, FUEL);
            let crate::operands::T::Opaque(p) = crate::operands::pl(&arg) else { continue };
            let Some((s, e, vals)) = fl.if_arms(*p, FUEL) else { continue };
            let arm_names: Vec<Vec<u32>> = vals.iter().map(|v| names_through(bodies, imports, b, &judged, v, 2)).collect();
            // Every recorder site anchored to this `if` (the arms' own site
            // and a select site recorded over it later) is a candidate.
            let pick = |site: usize, arm: usize| {
                (0..evs.len()).find(|&x| {
                    !got[x]
                        && evs[x].0 == calls[c].1
                        && evpath[x].last() == Some(&(site, arm))
                        && evs[x].1.is_some_and(|l| arm_names[arm].contains(&l))
                })
            };
            let mut sites: Vec<usize> = site_at.iter().filter(|(_, &at)| at == (s, e)).map(|(&k, _)| k).collect();
            sites.sort_unstable();
            if let Some((e0, e1)) = sites.iter().find_map(|&site| Some((pick(site, 0)?, pick(site, 1)?))) {
                owner[c] = Some(e0);
                co[c].push(e1);
                got[e0] = true;
                got[e1] = true;
            }
        }
        for i in 0..nc {
            used[i] = owner[i].is_some();
        }
        rep.consumed += owner[nc..].iter().filter(|o| o.is_some()).count();
        for (e, &(kind, local)) in evs.iter().enumerate() {
            if got[e] {
                continue;
            }
            match (kind, local) {
                ('o', _) => rep.rebind_unmatched += 1,
                ('m', _) => {}
                // The every-path rule (adopted 2026-10-28): a callee that
                // takes the parameter on some paths but not all is refused.
                (k @ ('a' | 'd'), Some(l)) if (0..takers.len()).any(|t| holds(l, t) && verdict(takers[t].2, takers[t].1) == (true, false)) => {
                    unmatched.push(format!("{k}{l} (its consuming callee takes it on some paths only)"))
                }
                (k, Some(l)) => unmatched.push(format!("{k}{l}")),
                (k, None) => unmatched.push(format!("{k}-")),
            }
        }
        rep.judges_unmatched += (0..live).filter(|&i| !used[i] && calls[i].1 == 'j').count();
        for i in 0..calls.len() {
            if used[i] || calls[i].1 == 'j' || i >= live {
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
        let extra: Vec<String> = (0..live)
            .filter(|&i| !used[i] && calls[i].1 != 'j')
            .map(|i| format!("{}@{} on {:?}", calls[i].1, calls[i].0, calls[i].2))
            .collect();
        if !unmatched.is_empty() || !extra.is_empty() {
            let mut msg = format!(
                "rc: function {pos}: recorded events with no call [{}]; calls with no recorded event [{}]",
                unmatched.join(" "),
                extra.join(", ")
            );
            if EXPLAIN.load(std::sync::atomic::Ordering::Relaxed) {
                // Diagnosis only (`--explain`): every RC call and what took it.
                let ev_txt = |e: usize| format!("{}{}", evs[e].0, evs[e].1.map_or("-".to_string(), |l| l.to_string()));
                for i in 0..nc {
                    let by = match owner[i] {
                        Some(e) => ev_txt(e),
                        None if used[i] => "counted".to_string(),
                        None => "LEFT".to_string(),
                    };
                    msg.push_str(&format!("\n  {}@{} {:?} <- {}", calls[i].1, calls[i].0, calls[i].2, by));
                }
            }
            return Err(msg);
        }
        rep.frames += 1;
    }
    Ok(())
}
