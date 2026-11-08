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
//! local reads) is matched like any other, and left over it is REFUSED like
//! any other: under `http.serve` an abort ends only the request, not the
//! instance, so a release the emitter performs before an abort is a recorded
//! event, never a trusted class (sec-b2, 2026-10-30). What an aborting path
//! leaves unreleased is the abort terminal's: no later request sees it
//! (the embedded lane copies its snapshot back, a trapped p3 instance is never
//! re-entered; tests/http_serve_cross_test.rs `aborting_requests_leave_no_
//! block_live…`, tests/http_serve_export_test.rs `aborting_requests_leave_
//! nothing…`).
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
//! A stdlib arm's window (`rcarms`) orders the matching (a call inside it is
//! tried after those outside) but accounts nothing: an RC call left over
//! inside it is refused like any other (#4278 B1 retired the `arm_internal`
//! class — each RC call an arm makes is one of its logged events: a
//! temporary it allocated and releases, a share it stores or gives back, a
//! retained argument it gives back, a slot consume).
//!
//! A taken compound routine's window (`rcroutines`: an in-place append,
//! string building, a map edit — every route the emitter takes through
//! `Emitter::routine`) orders the matching (a call inside it is tried after
//! those outside) but accounts nothing: an RC call left over inside it is
//! refused like any other (#4278 B1 retired the `routine_internal` class —
//! every such call is a logged event of the routine, a slot consume, or a
//! consumption its callee's checked signature declares).
//!
//! A release of a value loaded out of a heap block's slot that a later store
//! overwrites is the slot's consume (#4278 B1): the recorder logs it as an
//! `r` event in the release's own window (almide-wasm `slot_release`), and
//! it matches only a release inside that window on a loaded value whose slot
//! a later store writes. The `element` class that used to count such calls
//! is retired: an unlogged one is refused like any other leftover call.
//!
//! An event on an OUTER holder names it by the recorder's pseudo local (a
//! global `0xC000_0000 | g`, a C-319 cell `0xA000_0000 | l`): it matches a
//! call whose argument was read through that global, or loaded out of the
//! block cell local `l` holds.
//!
//! A routine's HOLD move (`h<local>@<from>-<to>`: a credit moved into a
//! compound routine through one of the emitter's pooled hold locals) is
//! matched like a move, but its local is named only inside its window: the
//! slot holds other values elsewhere, so outside the window a call on it is
//! not the frame's credit for that reason alone (a value read through a frame
//! variable still is — the rc-hold-credit-escapes drill).
//!
//! A variable's recorded release (or +1) with no RC call of its own matches
//! a call inside a stdlib arm's or a compound routine's window that takes
//! the variable as an argument, when the callee's SIGNATURE declares that
//! parameter consumed (#4278 B1, `signed`): a runtime helper's builder
//! declares, per parameter, borrow or consume on every path, and logs its
//! body's events (almide-wasm `helper_sig.rs`). The declaration is the
//! bundle's `rc-sig` record — the proven checker, the Rocq kernel and
//! almide-verify hold it to the helper's lines (proofs/HelperSig.v,
//! `check_sig_cert_sound`), and this check holds those events to the
//! helper's shipped body like any frame's (`rchelpers` places each). A
//! callee with no signature consumes nothing: the release stays unmatched.
//! The consumption is no longer read off the callee's bytes (the every-path
//! reading of 2026-10-28, which a body could satisfy by a path the reading
//! misjudged): a signature that lies about its body is refused by the
//! checker of that body, not trusted here.
//!
//! The windows are confined (sec-rows-2-3-4, the drills of
//! proofs/fixtures/rc-witness/reinject/): a leftover call counts as an arm's
//! or a routine's only if its argument names no variable the frame's events
//! name.
//!
//! An arm that answers with its ARGUMENT's block as its owned result on
//! some path (`if c { rc_inc(p); p } else { fresh }`) states it per site: the
//! recorder logs the share-out `s<local>` (#4278's per-arm certificate, the
//! `s` byte the proven checker folds), and it matches only the `rc_inc` in
//! that arm's own window read through the argument — a named event, never a
//! counted class.
//!
//! No counted window class is left (#4278 B1): `consumed` was retired by the
//! helper signatures, `element` by the slot consume, `routine-internal` and
//! `arm-internal` by the routines' and arms' own logged events. A new
//! trusted class is an owner decision, not a checker edit.
//!
//! A matched release is its local's last read (after.rs): from every live
//! release call whose argument is read straight out of a local, no path
//! reaches a read of that local before a write of it — a release moved before
//! the variable's last use is refused although its event still matches it.
//!
//! TRUSTED (#4278): that `rcfns` names every rc_inc and release routine.
//! (An arm's and a compound routine's RC are no longer trusted: each of
//! their calls is one of their logged events.)


use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use wasmparser::Operator as Op;

use crate::decode::{store_width, Body};
use crate::operands::{chain, Flow, Src, T, FUEL};

mod after;
mod events;
mod matching;

use events::Events;

/// `--explain`: a refusal lists every RC call of the frame and the event
/// (or counted class) that took it. Diagnosis output only; the verdict is
/// the same.
pub static EXPLAIN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A move event: a variable's credit (`m`) or a routine hold's (`h`) handed
/// into an arm's or a routine's window.
fn is_move(c: char) -> bool {
    matches!(c, 'm' | 'h')
}

/// The anchor file's `rcfns` line — the rc_inc routine's function index, if
/// the module has one, and every release routine's — the signed helpers, and
/// the copy-on-write judge sites its `judges` line locates.
#[derive(Clone, Copy)]
pub struct RcFns<'r> {
    pub inc: Option<u32>,
    pub releases: &'r HashSet<u32>,
    /// Per signed runtime helper (function index), whether it consumes each
    /// heap parameter (#4278 B1, the checked `rc-sig` records).
    pub sigs: &'r HashMap<u32, Vec<bool>>,
    /// The located copy-on-write judge calls (the anchor file's `judges`).
    pub judges: &'r [crate::At],
}

/// What the RC check saw.
#[derive(Default, Debug)]
pub struct RcReport {
    pub frames: usize,
    pub events: usize,
    pub calls: usize,
    pub rebind_unmatched: usize,
    pub judges_unmatched: usize,
    /// Releases (or +1s) a callee's checked signature consumes (#4278 B1).
    pub signed: usize,
    /// Live release calls on a local held to no later read of it (after.rs).
    pub last_reads: usize,
}

/// One open construct of [`dead`]'s walk: whether the code now reached in
/// it is dead, the then-arm's verdict once an `else` begins, whether a
/// branch targets its end, and whether it is a loop (a branch to a loop
/// re-enters it).
struct Construct {
    dead: bool,
    then_dead: Option<bool>,
    targeted: bool,
    is_loop: bool,
    is_if: bool,
}

/// [`dead`]'s walk: the open constructs, innermost last.
struct DeadWalk {
    stack: Vec<Construct>,
}

impl DeadWalk {
    /// A branch of relative depth `depth` targets its construct's end.
    fn target(&mut self, depth: u32) {
        let n = self.stack.len();
        if let Some(f) = (depth as usize).checked_add(1).and_then(|d| n.checked_sub(d)).and_then(|i| self.stack.get_mut(i)) {
            f.targeted = true;
        }
    }

    /// The rest of the innermost construct is dead.
    fn kill(&mut self) {
        if let Some(t) = self.stack.last_mut() {
            t.dead = true;
        }
    }

    fn else_arm(&mut self) {
        let outer = self.stack.len() >= 2 && self.stack[self.stack.len() - 2].dead;
        if let Some(f) = self.stack.last_mut() {
            f.then_dead = Some(f.dead);
            f.dead = outer;
        }
    }

    fn end(&mut self) {
        let Some(f) = self.stack.pop() else { return };
        // Past the end: reached when the body falls through (both arms
        // of an `if` with an `else`; an `if` without one always can),
        // or a branch targets the end (never a loop's: that re-enters).
        let falls_through = match (f.is_if, f.then_dead) {
            (true, Some(t)) => !(t && f.dead),
            (true, None) => true,
            _ => !f.dead,
        };
        let reached = falls_through || (f.targeted && !f.is_loop);
        if let Some(p) = self.stack.last_mut() {
            if !reached {
                p.dead = true;
            }
        }
    }

    /// One instruction, reached in dead code (`d`) or not.
    fn step(&mut self, op: &Op<'_>, d: bool) {
        match op {
            Op::Block { .. } | Op::Loop { .. } | Op::If { .. } | Op::TryTable { .. } => self.stack.push(Construct {
                dead: d,
                then_dead: None,
                targeted: false,
                is_loop: matches!(op, Op::Loop { .. }),
                is_if: matches!(op, Op::If { .. }),
            }),
            Op::Else => self.else_arm(),
            Op::End => self.end(),
            Op::Br { relative_depth } => {
                if !d {
                    self.target(*relative_depth);
                }
                self.kill();
            }
            Op::BrIf { relative_depth } => {
                if !d {
                    self.target(*relative_depth);
                }
            }
            Op::BrTable { targets } => {
                if !d {
                    for t in targets.targets().flatten().chain(std::iter::once(targets.default())) {
                        self.target(t);
                    }
                }
                self.kill();
            }
            Op::Return | Op::Unreachable | Op::ReturnCall { .. } | Op::ReturnCallIndirect { .. } => self.kill(),
            _ => {}
        }
    }
}

/// Instructions no path reaches: after `br` / `br_table` / `return` /
/// `unreachable` / a return call, up to the end of that block.
fn dead(b: &Body<'_>) -> Vec<bool> {
    let mut out = vec![false; b.ops.len()];
    let mut walk = DeadWalk { stack: vec![Construct { dead: false, then_dead: None, targeted: false, is_loop: false, is_if: false }] };
    for (k, op) in b.ops.iter().enumerate() {
        let d = walk.stack.last().is_some_and(|f| f.dead);
        out[k] = d && !matches!(op, Op::End | Op::Else);
        walk.step(op, d);
    }
    out
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

/// The release at `k` of a value loaded out of a slot replaces it: a later
/// store writes the same slot (the container's element is swapped, its old
/// credit released).
fn replaces(fl: &Flow<'_, '_>, b: &Body<'_>, k: usize) -> bool {
    let arg = fl.arg(k, 0, FUEL);
    let Some((addr, off)) = crate::operands::load_parts(&arg) else { return false };
    let (base, at) = crate::operands::effective(addr, off);
    (k + 1..b.ops.len()).any(|k2| match store_width(&b.ops[k2]) {
        Some((_, o)) => {
            let st = fl.arg(k2, 0, FUEL);
            let (sbase, sat) = crate::operands::effective(&st, o);
            sat == at && fl.same(base, sbase)
        }
        None => false,
    })
}

/// Check the `rc` lines (`frames` as `(position, events)`) over `bodies`
/// (`imports`: the function imports before them in the index space).
pub fn check_rc(
    bodies: &[Body<'_>],
    imports: u32,
    rc_fns: RcFns<'_>,
    frames: &[(usize, String)],
    windows: &[Vec<(usize, usize, usize)>; 2],
    rep: &mut RcReport,
) -> Result<(), String> {
    for (pos, events) in frames {
        let fr = Frame::new(bodies, imports, rc_fns, *pos, events, windows)?;
        rep.calls += fr.calls[..fr.live].iter().filter(|c| c.1 != 'j').count();
        rep.events += fr.ev.evs.len();
        fr.verdict(rep)?;
        fr.last_reads(rep)?;
        rep.frames += 1;
    }
    Ok(())
}

/// A live or dead RC call of a frame: its instruction, its kind (`a` the
/// rc_inc routine, `d` a release, `j` a recorded judge), and the names its
/// argument answers to.
type Call = (usize, char, Vec<u32>);

/// One certified frame, read for the matching of its events to its calls.
struct Frame<'x, 'a> {
    bodies: &'x [Body<'a>],
    imports: u32,
    b: &'x Body<'a>,
    pos: usize,
    judged: HashSet<usize>,
    fl: Flow<'x, 'a>,
    windows: &'x [Vec<(usize, usize, usize)>; 2],
    sigs: &'x HashMap<u32, Vec<bool>>,
    /// The live RC calls: (instruction, kind, the locals its argument was
    /// read through). Those on a path that aborts are matched like any
    /// other (sec-b2: no class of their own); left over, they are refused.
    /// The unreachable calls follow the live ones.
    calls: Vec<Call>,
    live: usize,
    nc: usize,
    /// Calls whose argument is a value loaded out of a heap block's slot.
    elements: HashSet<usize>,
    ev: Events,
    /// The consuming-argument candidates: a call INSIDE an arm or routine
    /// window (no RC call itself), per argument `(call, argument, callee)`.
    takers: Vec<(usize, usize, u32)>,
    /// Each taker's argument value.
    args: Vec<T>,
    /// Per variable a named event carries, which consuming arguments hold
    /// its value at their call (computed once per variable: a term per
    /// argument, compared against the variable's value there).
    holding: RefCell<HashMap<u32, Vec<bool>>>,
    temp_arg: Vec<bool>,
    /// #4278 B1: a release whose argument is read straight out of a hold
    /// a routine's or an arm's `h` move names, inside that move's window,
    /// is that hold's credit: a temporary's unnamed event never takes it.
    /// (The direct read only: a pooled hold also shows up among the
    /// locals an unrelated value may have passed through.)
    hold_claimed: Vec<bool>,
}

impl<'x, 'a> Frame<'x, 'a> {
    fn new(
        bodies: &'x [Body<'a>],
        imports: u32,
        rc_fns: RcFns<'x>,
        pos: usize,
        events: &str,
        windows: &'x [Vec<(usize, usize, usize)>; 2],
    ) -> Result<Self, String> {
        let judged: HashSet<usize> = rc_fns.judges.iter().filter(|j| j.func == pos).map(|j| j.op).collect();
        let b = bodies.get(pos).ok_or_else(|| format!("rc: no defined function at position {pos}"))?;
        let mut fr = Frame {
            bodies,
            imports,
            b,
            pos,
            judged,
            fl: Flow::new(b, HashSet::new()),
            windows,
            sigs: rc_fns.sigs,
            calls: Vec::new(),
            live: 0,
            nc: 0,
            elements: HashSet::new(),
            ev: events::parse(events),
            takers: Vec::new(),
            args: Vec::new(),
            holding: RefCell::default(),
            temp_arg: Vec::new(),
            hold_claimed: Vec::new(),
        };
        let dead = dead(b);
        fr.collect_calls(&dead, rc_fns);
        fr.collect_takers(&dead);
        Ok(fr)
    }

    /// after.rs: every live release call on a local is that local's last
    /// read on every path from it.
    fn last_reads(&self, rep: &mut RcReport) -> Result<(), String> {
        let cfg = after::Cfg::new(self.b);
        for &(k, kind, _) in &self.calls[..self.live] {
            if kind != 'd' {
                continue;
            }
            let Some(l) = after::released_local(self.b, k) else { continue };
            if let Some(j) = after::read_after(self.b, &cfg, k, l) {
                return Err(format!("rc: function {}: local {l} read at @{j} after its release at @{k}", self.pos));
            }
            rep.last_reads += 1;
        }
        Ok(())
    }

    /// [`names_through`] two calls deep, in this frame.
    fn names_of(&self, arg: &T) -> Vec<u32> {
        names_through(self.bodies, self.imports, self.b, &self.judged, arg, 2)
    }

    fn collect_calls(&mut self, dead: &[bool], rc_fns: RcFns<'_>) {
        let RcFns { inc, releases, .. } = rc_fns;
        let b = self.b;
        let mut dead_calls: Vec<Call> = Vec::new();
        for (k, op) in b.ops.iter().enumerate() {
            let Op::Call { function_index } = op else { continue };
            let kind = if Some(*function_index) == inc {
                'a'
            } else if releases.contains(function_index) {
                'd'
            } else if self.judged.contains(&k) {
                'j'
            } else {
                continue;
            };
            if dead[k] {
                // An unreachable RC call: never run, so never refused; an
                // event the recorder logged on a path that cannot reach it
                // (an arm that aborts) may still name it, last of all.
                dead_calls.push((k, kind, names(&self.fl.arg(k, 0, FUEL))));
                continue;
            }
            let arg = self.fl.arg(k, 0, FUEL);
            if crate::operands::is_loaded(&arg) {
                self.elements.insert(k);
            }
            let named = self.names_of(&arg);
            self.calls.push((k, kind, named));
        }
        self.live = self.calls.len();
        self.calls.extend(dead_calls);
        self.nc = self.calls.len();
    }

    /// The consuming-argument candidates, and per candidate and per call
    /// what the hold moves claim.
    fn collect_takers(&mut self, dead: &[bool]) {
        let (b, pos, windows) = (self.b, self.pos, self.windows);
        let (evs, evwin) = (&self.ev.evs, &self.ev.evwin);
        // Every variable the frame's events name: an arm's, a routine's or a
        // container's own RC never touches one of these (that would be the
        // frame's credit, which its events must account).
        // A routine's HOLD move (`h<local>@<from>-<to>`: a credit moved into
        // the routine through a pooled hold local) names that local only
        // inside its window — the slot is reused for other values outside it.
        let witnessed: HashSet<u32> = evs.iter().filter(|e| !matches!(e.0, 'h' | 'r')).filter_map(|e| e.1).collect();
        let held_at: Vec<(u32, usize, usize)> = (0..evs.len())
            .filter(|&e| evs[e].0 == 'h')
            .filter_map(|e| Some((evs[e].1?, evwin[e]?.0, evwin[e]?.1)))
            .collect();
        // Does a value read through `names` at instruction `k` carry a credit
        // the frame's events account?
        let owned_at = |names: &[u32], k: usize| {
            names.iter().any(|n| witnessed.contains(n) || held_at.iter().any(|&(l, a, z)| l == *n && a <= k && k <= z))
        };
        let inside = |w: &[(usize, usize, usize)], k: usize| w.iter().any(|&(p, a, b)| p == pos && a <= k && k <= b);
        let rc_call: HashSet<usize> = self.calls.iter().map(|c| c.0).collect();
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
        let args: Vec<T> = takers.iter().map(|&(k, j, _)| self.fl.arg(k, j, FUEL)).collect();
        let temp_arg: Vec<bool> = (0..takers.len()).map(|t| !owned_at(&self.names_of(&args[t]), takers[t].0)).collect();
        let hold_claimed: Vec<bool> = self
            .calls
            .iter()
            .map(|(k, kind, names)| *kind == 'd' && held_at.iter().any(|&(l, a, z)| a <= *k && *k <= z && names.first() == Some(&l)))
            .collect();
        (self.takers, self.args, self.temp_arg, self.hold_claimed) = (takers, args, temp_arg, hold_claimed);
    }

    /// A callee consumes its parameter `j` exactly when its checked
    /// signature declares it (#4278 B1): the bundle's `rc-sig` record,
    /// which the proven checker holds to the callee's own body. A callee
    /// with no signature consumes nothing here.
    fn consumes(&self, g: u32, j: usize) -> bool {
        self.sigs.get(&g).is_some_and(|m| m.get(j) == Some(&true))
    }

    /// Consuming argument `t` holds variable `l`'s value at its call.
    fn holds(&self, l: u32, t: usize) -> bool {
        self.holding.borrow_mut().entry(l).or_insert_with(|| {
            (0..self.takers.len())
                .map(|t| {
                    self.names_of(&self.args[t]).contains(&l)
                        || self.fl.same(&self.args[t], &self.fl.local_value(l, self.takers[t].0, FUEL))
                })
                .collect()
        })[t]
    }

    /// Instruction `k` lies in event `e`'s own window.
    fn in_own_window(&self, e: usize, k: usize) -> bool {
        self.ev.evwin[e].is_some_and(|(a, z)| a <= k && k <= z)
    }

    /// A call inside an arm or routine window.
    fn in_window(&self, k: usize) -> bool {
        self.windows.iter().flatten().any(|&(p, a, b)| p == self.pos && a <= k && k <= b)
    }

    /// Event `e` may take node `i`: an RC call (`i < nc`) or a consuming
    /// argument (`nc + t`).
    fn fits(&self, e: usize, i: usize) -> bool {
        if i >= self.nc {
            self.fits_taker(e, i - self.nc)
        } else {
            self.fits_call(e, i)
        }
    }

    fn fits_taker(&self, e: usize, t: usize) -> bool {
        let (k, j, g) = self.takers[t];
        match self.ev.evs[e] {
            ('a' | 'd', Some(l)) => self.holds(l, t) && self.consumes(g, j),
            // A temporary's event logged inside an arm or routine
            // window: that window's call taking a temporary (no frame
            // variable) on every path.
            ('a' | 'd', None) => self.in_own_window(e, k) && self.temp_arg[t] && self.consumes(g, j),
            _ => false,
        }
    }

    fn fits_call(&self, e: usize, i: usize) -> bool {
        let (at, k, names_of) = (self.calls[i].0, self.calls[i].1, &self.calls[i].2);
        match self.ev.evs[e] {
            ('o', Some(l)) => matches!(k, 'd' | 'j') && names_of.contains(&l),
            // The arm or routine the credit moved into, or one enclosing
            // it (a callback's move released by the fold that called it).
            // A hold's move (`h`) takes only a release read straight out of
            // its hold: the pooled local carries other values too.
            (c, Some(l)) if is_move(c) => k == 'd' && self.move_takes(e, c, l, i),
            (c, None) if is_move(c) => false,
            // An arm's share-out (#4278, `s<local>@<window>`): the arm
            // answers with its argument's block as its owned result after
            // an `rc_inc` on it. It takes exactly that `rc_inc` — read
            // through the argument's variable (or, for a temporary, any)
            // and INSIDE the arm's own window, never a +1 elsewhere.
            ('s', l) => k == 'a' && self.in_own_window(e, at) && l.is_none_or(|l| through(names_of, l)),
            // #4278 B1: a container slot's replaced occupant (`r-@<window>`):
            // a release inside its window on a value loaded out of a slot
            // that a later store overwrites.
            ('r', _) => k == 'd' && self.in_own_window(e, at) && self.elements.contains(&at) && replaces(&self.fl, self.b, at),
            // A C-319 cell variable's events name its local; a call on
            // the block the cell holds reads through it (`CELL_HOLDER | l`).
            // Both are that variable's credit, matched one to one.
            (kind, Some(l)) => (k == kind || (kind == 'd' && k == 'j')) && through(names_of, l),
            // A temporary's event logged inside an arm or routine window
            // records an instruction emitted inside it: it takes a call
            // in that window, never a release elsewhere it does not own.
            (kind, None) => k == kind && !self.hold_claimed[i] && self.ev.evwin[e].is_none_or(|(a, z)| a <= at && at <= z),
        }
    }

    /// The release call `i` is the credit move `e` (`c<l>@<window>`) handed
    /// into an arm or routine: read through `l` (straight out of it for a
    /// hold), inside the window or one enclosing it.
    fn move_takes(&self, e: usize, c: char, l: u32, i: usize) -> bool {
        let (at, names_of) = (self.calls[i].0, &self.calls[i].2);
        (if c == 'h' { names_of.first() == Some(&l) } else { names_of.contains(&l) })
            && self.ev.evwin[e].is_some_and(|(a, z)| {
                self.windows.iter().flatten().any(|&(p, x, y)| p == self.pos && x <= a && z <= y && x <= at && at <= y)
                    || (a <= at && at <= z)
            })
    }
}

/// A call's argument names `l`: read through it, or (a C-319 cell's local)
/// loaded out of the block the cell holds.
fn through(names_of: &[u32], l: u32) -> bool {
    names_of.contains(&l) || (l < GLOBAL_HOLDER.min(CELL_HOLDER) && names_of.contains(&(CELL_HOLDER | l)))
}
