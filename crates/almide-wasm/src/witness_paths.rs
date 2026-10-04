//! Branch and loop frames in the structural witness (#2756 / #2757, #1696
//! step 4).
//!
//! The recorder (witness.rs) logs every RC event in EMISSION order, together
//! with the control structure the emitter walked:
//! - `Open` / `Arm` / `Close` for an `if` / `match` / short-circuit site;
//! - `LoopOpen` / `LoopClose` for a `for` / `while` body;
//! - `Jump` for a `break` / `continue`;
//! - `Exit` where a path leaves the frame (a `return_call`).
//!
//! Emission order is program order within an arm or body, so the log is the
//! frame's control-flow tree with the events on it.
//!
//! Events name either an OBJECT directly (`Op`, for a temporary) or a LOCAL
//! (`LOp`). A local event is resolved per path to the object the local holds
//! on that path (`Bind` / `Alias` set it), so an `if` that rebinds a var in
//! one arm is followed exactly after the join. There is no phi to guess.
//!
//! Each object gets a FRAME line and, for every loop whose body touches it,
//! one ACTIVATION line per loop.
//!
//! - **The frame line** is the object's path set through the frame, with loop
//!   bodies skipped. One distinct path renders flat. Two render as the
//!   whole-line branch `{p|q}`: the checker runs each arm from rc 0 and both
//!   must end at 0, so every path must balance on its own. An `Exit` ends a
//!   path. A release through a local that holds nothing on this path (a
//!   local bound in the other arm) is a `$dec_flat` of NULL: a no-op,
//!   skipped.
//! - **An activation line** is the path set of ONE iteration of a loop body,
//!   from rc 0. The loop is a holder of its own: every credit it takes on an
//!   object must be given back (or moved on) within the iteration. An object
//!   born in the body lives one iteration. The owner local that holds it
//!   keeps it until the next rebind (the Bind route's dec-old) or the
//!   epilogue releases it, so that release is recorded at the iteration's
//!   end, and the dec-old of a rebind is recorded only when it releases a
//!   block bound earlier in the SAME iteration (the unrolled lane).
//!
//! A var bound outside a loop and rebound inside it is LOOP-CARRIED
//! (#2755, [`carry`]): the loop holds its block between iterations, each
//! iteration receives one and hands one on.
//!
//! This decomposition is sound because each line is one holder's account of
//! one block. The block's count is the sum of its holders' counts, and every
//! line is checked never to release what it does not hold and to end at 0.
//! The recorder declines what the decomposition cannot carry:
//! - an exit from inside a loop body (`loop-exit`);
//! - more than two distinct paths (`branch-paths:N`).

use std::collections::{BTreeMap, BTreeSet};

use super::lines::{flat_exits, hoist, net, record};

/// The loop-carried locals (#2755), split for the file budget.
#[path = "witness_carry.rs"]
mod carry;
use carry::carry;

/// The `Carry` depth of an OUTER holder the whole frame borrows (#2755 /
/// #2758, witness_carry.rs `frame_carry`).
pub(crate) const FRAME_HELD: u32 = u32::MAX;

/// One logged event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ev {
    Birth(u32),
    Op(u32, char),
    /// `local` now holds `obj` (`owner`: its release is the local's).
    Bind { local: u32, obj: u32, owner: bool },
    /// `local` now holds whatever `src` holds on this path.
    Alias { local: u32, src: u32, owner: bool },
    /// An RC op on the block `local` holds on this path.
    LOp(u32, char),
    /// An owner rebind's dec-old of `local`.
    DecOld(u32),
    Open,
    Arm,
    Close,
    LoopOpen,
    LoopClose,
    Jump,
    Exit,
    /// #2755: the process aborts (exit_plan.rs `Continuation::Abort`).
    Abort,
    /// #2755: `local`, bound at loop depth `depth`, was rebound inside a
    /// deeper loop — every loop between is LOOP-CARRIED for it ([`carry`]).
    Carry { local: u32, depth: u32 },
}

#[derive(Default)]
struct Site {
    /// The opening event was logged (the site is not in dead code).
    logged: bool,
    started: bool,
    arms: u32,
    exited: u32,
    /// Branch: the current arm has left. Loop: the rest of the body is dead
    /// (a `break` / `continue` / exit).
    cur_exited: bool,
}

/// The control state of one frame's recording.
#[derive(Default)]
pub(crate) struct Branches {
    stack: Vec<Site>,
    /// The whole frame has left: nothing emitted after runs.
    frame_dead: bool,
}

impl Branches {
    /// Is the code being emitted right now unreachable?
    pub(crate) fn dead(&self) -> bool {
        self.frame_dead || self.stack.iter().any(|s| s.cur_exited)
    }

    fn push(&mut self, log: &mut Vec<Ev>, ev: Ev) {
        let logged = !self.dead();
        if logged {
            log.push(ev);
        }
        self.stack.push(Site { logged, ..Site::default() });
    }

    pub(crate) fn open(&mut self, log: &mut Vec<Ev>) {
        self.push(log, Ev::Open);
    }

    /// The next arm begins (the first call after `open` starts arm one).
    pub(crate) fn arm(&mut self, log: &mut Vec<Ev>) {
        let Some(s) = self.stack.last_mut() else { return };
        if s.started {
            s.exited += u32::from(s.cur_exited);
            s.cur_exited = false;
            s.arms += 1;
            if s.logged {
                log.push(Ev::Arm);
            }
        } else {
            s.started = true;
            s.arms = 1;
        }
    }

    /// The join. A site whose EVERY arm left leaves the enclosing arm, loop
    /// body or frame too.
    pub(crate) fn close(&mut self, log: &mut Vec<Ev>) {
        let Some(mut s) = self.stack.pop() else { return };
        s.exited += u32::from(s.cur_exited);
        if s.logged {
            log.push(Ev::Close);
        }
        if s.started && s.exited == s.arms {
            self.left();
        }
    }

    pub(crate) fn loop_open(&mut self, log: &mut Vec<Ev>) {
        self.push(log, Ev::LoopOpen);
    }

    /// The loop is left: code after it is live again (a dead body is one
    /// path of the loop, not the frame's).
    pub(crate) fn loop_close(&mut self, log: &mut Vec<Ev>) {
        let Some(s) = self.stack.pop() else { return };
        if s.logged {
            log.push(Ev::LoopClose);
        }
    }

    /// A `break` / `continue`: the iteration ends here.
    pub(crate) fn jump(&mut self, log: &mut Vec<Ev>) {
        if !self.dead() {
            log.push(Ev::Jump);
        }
        self.left();
    }

    /// #2755: the process aborts here — the path ends, and its outstanding
    /// credits are discharged by the abort terminal.
    pub(crate) fn abort(&mut self, log: &mut Vec<Ev>) {
        if !self.dead() {
            log.push(Ev::Abort);
        }
        self.left();
    }

    /// A frame-ending edge was emitted here (its releases already logged).
    pub(crate) fn exit(&mut self, log: &mut Vec<Ev>) {
        if !self.dead() {
            log.push(Ev::Exit);
        }
        self.left();
    }

    fn left(&mut self) {
        match self.stack.last_mut() {
            Some(s) => s.cur_exited = true,
            None => self.frame_dead = true,
        }
    }

    /// Every site the emitter opened was closed (a hook imbalance is a
    /// recorder bug, loudly).
    pub(crate) fn settled(&self) -> bool {
        self.stack.is_empty()
    }
}

/// The log as a tree.
#[derive(Clone, Debug)]
enum Node {
    Ev(Ev),
    Branch(Vec<Vec<Node>>),
    /// A loop body and the locals it carries ([`carry`]).
    Loop(Vec<Node>, Vec<u32>),
}

fn parse(log: &[Ev]) -> Option<Vec<Node>> {
    let mut pos = 0;
    let seq = parse_seq(log, &mut pos)?;
    (pos == log.len()).then_some(seq)
}

fn parse_seq(log: &[Ev], pos: &mut usize) -> Option<Vec<Node>> {
    let mut seq = Vec::new();
    while let Some(ev) = log.get(*pos) {
        match ev {
            Ev::Arm | Ev::Close | Ev::LoopClose => return Some(seq),
            Ev::Open => {
                *pos += 1;
                let mut arms = vec![parse_seq(log, pos)?];
                loop {
                    match log.get(*pos)? {
                        Ev::Arm => {
                            *pos += 1;
                            arms.push(parse_seq(log, pos)?);
                        }
                        Ev::Close => break,
                        _ => return None,
                    }
                }
                seq.push(Node::Branch(arms));
            }
            Ev::LoopOpen => {
                *pos += 1;
                let body = parse_seq(log, pos)?;
                if log.get(*pos)? != &Ev::LoopClose {
                    return None;
                }
                seq.push(Node::Loop(body, Vec::new()));
            }
            other => seq.push(Node::Ev(other.clone())),
        }
        *pos += 1;
    }
    Some(seq)
}

/// A holder of the object: an owner local (its release is the local's)
/// or a view; `fresh` = bound in THIS walk (this frame, or this iteration
/// of the loop whose activation is being walked).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Holder {
    owner: bool,
    fresh: bool,
}

/// A path through the frame or one iteration, as one object sees it. Two
/// paths in the same state are interchangeable from here on, so each step
/// keeps one of each.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Path {
    events: String,
    born: bool,
    holders: BTreeMap<u32, Holder>,
    ended: bool,
    /// #2755: the path ended in a process ABORT: whatever it still holds is
    /// discharged by the checker's abort terminal (`t`, format v6).
    aborted: bool,
    first: Option<char>, // the first event recorded (#3259, witness_lines.rs `record`)
}

/// The cap on enumerated paths per object: beyond it the frame declines.
const PATH_CAP: usize = 64;

/// Where a walk runs: the frame, or one loop iteration.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope<'t> {
    /// The frame, with the outer holders it borrows (`frame_carry`).
    Frame(&'t [u32]),
    /// One iteration of a loop body, with the locals that loop carries.
    Iteration(&'t [u32]),
}

/// The loops a walk passed, with the locals each carries and the object's
/// state at each entry.
type LoopEntries<'t> = Vec<(&'t [Node], &'t [u32], Path)>;

fn step(ev: &Ev, o: u32, p: &mut Path) {
    match ev {
        Ev::Birth(b) if *b == o => p.born = true,
        Ev::Op(b, c) if *b == o && p.born => record(&mut p.events, &mut p.first, *c),
        Ev::Bind { local, obj, owner } => {
            if *obj == o {
                p.holders.insert(*local, Holder { owner: *owner, fresh: true });
            } else {
                p.holders.remove(local);
            }
        }
        Ev::Alias { local, src, owner } => {
            if p.holders.contains_key(src) {
                p.holders.insert(*local, Holder { owner: *owner, fresh: true });
            } else {
                p.holders.remove(local);
            }
        }
        // A release through a local that holds nothing on this path is a
        // release of NULL (a no-op) — skipped with every other op.
        Ev::LOp(l, c) if p.holders.contains_key(l) => record(&mut p.events, &mut p.first, *c),
        Ev::DecOld(l) => {
            if p.holders.get(l).is_some_and(|h| h.owner && h.fresh) {
                record(&mut p.events, &mut p.first, 'd');
            }
            p.holders.remove(l);
        }
        Ev::Exit => p.ended = true,
        Ev::Abort => {
            p.ended = true;
            p.aborted = true;
        }
        _ => {}
    }
}

/// The end of one iteration (natural, `break` or `continue`): each owner
/// local bound in it keeps its block until its next rebind or the epilogue
/// releases it — that release, recorded here. A LOOP-CARRIED local
/// ([`carry`]) instead hands its block on to the next iteration, or out of
/// the loop (`m`).
fn end_iteration(p: &mut Path, carried: &[u32]) {
    for (l, h) in &p.holders {
        if h.owner && h.fresh {
            p.events.push(if carried.contains(l) { 'm' } else { 'd' });
        }
    }
    p.ended = true;
}

/// A path leaves the frame: each outer holder it borrowed gets its block
/// back (`m`, witness_carry.rs `frame_carry`).
fn hand_back(p: &mut Path, held: &[u32]) {
    for (l, h) in &p.holders {
        if h.owner && held.contains(l) {
            p.events.push('m');
        }
    }
}

/// How a walk renders a branch with an exiting arm.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Exits {
    /// An exit ends its path; the line is the set of whole paths.
    Paths,
    /// #2758: an exiting arm is folded into the surviving path as the v5
    /// branch-return item `{<arm>x|}` — checked from the count at the
    /// branch to exactly 0, the line continuing from the survivor. A frame
    /// with several `!` sites is then one path, not one per site.
    Fold,
}

/// Extend each live path through `seq`, for object `o`.
fn walk<'t>(
    seq: &'t [Node],
    o: u32,
    scope: Scope<'t>,
    exits: Exits,
    paths: Vec<Path>,
    loops: &mut LoopEntries<'t>,
) -> Result<Vec<Path>, String> {
    let mut paths = paths;
    for node in seq {
        let mut next = Vec::with_capacity(paths.len());
        for mut p in paths {
            if p.ended {
                next.push(p);
                continue;
            }
            match node {
                Node::Ev(Ev::Jump) => match scope {
                    Scope::Iteration(carried) => end_iteration(&mut p, carried),
                    Scope::Frame(_) => return Err("loop-jump-outside-loop".into()),
                },
                Node::Ev(Ev::Exit) => match scope {
                    Scope::Frame(held) => {
                        hand_back(&mut p, held);
                        step(&Ev::Exit, o, &mut p);
                    }
                    Scope::Iteration(_) => return Err("loop-exit".into()),
                },
                Node::Ev(ev) => step(ev, o, &mut p),
                Node::Branch(arms) if exits == Exits::Fold => {
                    next.extend(fold_branch(arms, o, scope, p, loops)?);
                    continue;
                }
                Node::Branch(arms) => {
                    for arm in arms {
                        next.extend(walk(arm, o, scope, exits, vec![p.clone()], loops)?);
                    }
                    continue;
                }
                // A loop is its own activation: skipped here, walked later
                // from the state it was entered with.
                Node::Loop(body, carried) => loops.push((body.as_slice(), carried.as_slice(), p.clone())),
            }
            next.push(p);
        }
        next.sort();
        next.dedup();
        if next.len() > PATH_CAP {
            return Err("branch-paths:cap".into());
        }
        paths = next;
    }
    Ok(paths)
}

/// One branch in [`Exits::Fold`] mode: each arm is walked from the entry
/// state with an empty event suffix; the arms that exit become `{<arm>x|}`
/// items prefixed to every surviving path. A branch whose every arm exits
/// leaves the enclosing arm too: its paths stay ended, for the enclosing
/// branch (or the frame) to take.
fn fold_branch<'t>(
    arms: &'t [Vec<Node>],
    o: u32,
    scope: Scope<'t>,
    p: Path,
    loops: &mut LoopEntries<'t>,
) -> Result<Vec<Path>, String> {
    // (events, aborted): an aborting arm folds as `{<arm>t|}` — checked
    // fault-free from the count at the branch, its remainder discharged.
    let mut exited: BTreeSet<(String, bool)> = BTreeSet::new();
    let mut ended: Vec<Path> = Vec::new();
    let mut survivors: Vec<Path> = Vec::new();
    for arm in arms {
        let start = Path { events: String::new(), ..p.clone() };
        for r in walk(arm, o, scope, Exits::Fold, vec![start], loops)? {
            if !r.ended {
                survivors.push(r);
                continue;
            }
            // An arm item is flat (v5). An exiting arm that itself folded a
            // nested exit (`a{bx|}c`, then its own exit) is several exit
            // paths from this branch's entry — `ab` and `ac` — each its own
            // flat item, checked from the same count.
            if r.born || !r.events.is_empty() {
                for (flat, aborted) in flat_exits(&r.events, r.aborted) {
                    exited.insert((flat, aborted));
                }
            }
            ended.push(r);
        }
    }
    if survivors.is_empty() {
        return Ok(ended.into_iter().map(|r| Path { events: format!("{}{}", p.events, r.events), ..r }).collect());
    }
    let items: String = exited
        .iter()
        .map(|(e, aborted)| format!("{{{e}{}|}}", if *aborted { 't' } else { 'x' }))
        .collect();
    Ok(survivors
        .into_iter()
        .map(|r| Path { events: format!("{}{items}{}", p.events, r.events), ..r })
        .collect())
}

/// A path as the line shows it. #2755: a path that ABORTED still holding
/// credits ends in the abort terminal `t`; one that aborted balanced is
/// shown as the ordinary path it is (checked to 0 like any other).
fn shown(p: &Path) -> String {
    if p.aborted && net(&p.events) != 0 {
        format!("{}t", p.events)
    } else {
        p.events.clone()
    }
}

/// Two or fewer distinct paths as one certificate line.
///
/// #2755: an ABORTING path whose events are a prefix of a non-aborting path
/// of the same line needs no arm of its own: the checker runs that longer
/// path fault-free, and a fault in the prefix would be a fault in it (`exec`
/// is a left fold; `check_line_prefix_safe`), so the aborting run is safe up
/// to the abort — all an abort owes; what it still holds is discharged. Only
/// an aborting path no other path covers is shown, with its terminal (`t`).
///
/// A path that carries folded exit items has them hoisted to the line start
/// ([`hoist`]), so the whole-line branch between the flat remainders stays
/// flat (v5 arms do not nest).
fn line(paths: &[Path], exits: Exits) -> Result<String, String> {
    let returning: Vec<String> = paths.iter().filter(|p| !p.aborted || net(&p.events) == 0).map(shown).collect();
    let covered = |p: &Path| p.aborted && net(&p.events) != 0 && returning.iter().any(|q| q.starts_with(&p.events));
    let set: BTreeSet<String> = paths.iter().filter(|p| !covered(p)).map(shown).collect();
    // One returning path carries its folded items in place, as before.
    if let [one] = set.iter().collect::<Vec<_>>().as_slice()
        && !one.ends_with('t')
    {
        return Ok((*one).clone());
    }
    let mut hoisted: BTreeSet<String> = BTreeSet::new();
    let mut flats: BTreeSet<String> = BTreeSet::new();
    for s in &set {
        let (h, f) = hoist(s);
        hoisted.extend(h);
        flats.insert(f);
    }
    let head: String = hoisted.into_iter().collect();
    let v: Vec<&str> = flats.iter().map(String::as_str).collect();
    let body = match v.as_slice() {
        // A lone aborting path is the abort arm of a branch whose other arm
        // (a path that never runs) claims nothing: `{<p>t|}`.
        [one] if one.ends_with('t') => format!("{{{one}|}}"),
        [one] => (*one).to_string(),
        // One terminal arm per branch: two paths, at most one aborting.
        [a, b] if !(a.ends_with('t') && b.ends_with('t')) => format!("{{{a}|{b}}}"),
        // Any other set of whole paths: every path but one is its own
        // terminal item from the line's start (`{<p>x|}` — a returning path,
        // checked from 0 to exactly 0; `{<p>t|}` — an aborting one, checked
        // fault-free), each with an empty survivor that moves no count; the
        // last returning path (or none) is the line's tail, checked to 0.
        // The whole-path form keeps its two-arm shape; a frame whose paths
        // it cannot carry is rendered again in the folded form first.
        more if exits == Exits::Paths => return Err(format!("branch-paths:{}", more.len())),
        more => {
            let (aborting, returning): (Vec<&str>, Vec<&str>) = more.iter().partition(|p| p.ends_with('t'));
            let (tail, rest) = returning.split_last().map_or(("", &[][..]), |(t, r)| (*t, r));
            let items: String = aborting
                .iter()
                .map(|p| format!("{{{p}|}}"))
                .chain(rest.iter().map(|p| format!("{{{p}x|}}")))
                .collect();
            format!("{items}{tail}")
        }
    };
    Ok(format!("{head}{body}"))
}

/// One object's lines: the frame line, then one line per loop activation
/// that touches it.
fn render_object(tree: &[Node], o: u32, exits: Exits, held: &[u32], out: &mut String) -> Result<(), String> {
    let mut loops: LoopEntries = Vec::new();
    let mut frame = walk(tree, o, Scope::Frame(held), exits, vec![Path::default()], &mut loops)?;
    frame.iter_mut().filter(|p| !p.ended).for_each(|p| hand_back(p, held));
    out.push_str(&line(&frame, exits)?);
    out.push('\n');
    // Each loop reached, walked from each entry state; loops nested in a
    // body are reached by that body's walk (appended as it runs).
    let mut k = 0;
    let mut by_loop: Vec<(&[Node], Vec<Path>)> = Vec::new();
    while k < loops.len() {
        let (body, carried, entry) = loops[k].clone();
        k += 1;
        let start = Path {
            events: String::new(),
            born: entry.born,
            holders: entry.holders.iter().map(|(&l, h)| (l, Holder { fresh: false, ..*h })).collect(),
            ended: false,
            aborted: false,
            first: None,
        };
        let mut iter = walk(body, o, Scope::Iteration(carried), exits, vec![start], &mut loops)?;
        iter.iter_mut().filter(|p| !p.ended).for_each(|p| end_iteration(p, carried));
        match by_loop.iter_mut().find(|(b, _)| std::ptr::eq(*b, body)) {
            Some((_, ps)) => ps.extend(iter),
            None => by_loop.push((body, iter)),
        }
        if loops.len() > PATH_CAP {
            return Err("branch-paths:cap".into());
        }
    }
    for (_, iter) in by_loop {
        if iter.iter().any(|p| !p.events.is_empty()) {
            out.push_str(&line(&iter, exits)?);
            out.push('\n');
        }
    }
    Ok(())
}

/// Render every object's lines, in object order; `Err(reason)` withdraws
/// the certificate.
pub(crate) fn render(log: &[Ev], objects: u32) -> Result<String, String> {
    let Some(mut tree) = parse(log) else {
        return Err("branch-log:unbalanced".into());
    };
    let mut objects = objects;
    carry(&mut tree, 0, &mut objects);
    let held = carry::frame_carry(&mut tree, &mut objects);
    let mut s = String::new();
    for o in 0..objects {
        // The whole-path form first (every certificate before #2758 keeps
        // its bytes); an object whose paths it cannot carry — a frame with
        // several exits — is rendered again with its exits folded.
        let mut line = String::new();
        if let Err(e) = render_object(&tree, o, Exits::Paths, &held, &mut line) {
            if !e.starts_with("branch-paths") {
                return Err(e);
            }
            line.clear();
            render_object(&tree, o, Exits::Fold, &held, &mut line)?;
        }
        s.push_str(&line);
    }
    Ok(s)
}

#[cfg(test)]
#[path = "witness_paths_tests.rs"]
mod tests;
