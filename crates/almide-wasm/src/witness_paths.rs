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
//! This decomposition is sound because each line is one holder's account of
//! one block. The block's count is the sum of its holders' counts, and every
//! line is checked never to release what it does not hold and to end at 0.
//! The recorder declines what the decomposition cannot carry:
//! - a var bound outside a loop and rebound inside it (`loop-carried-assign`);
//! - an exit from inside a loop body (`loop-exit`);
//! - more than two distinct paths (`branch-paths:N`).

use std::collections::{BTreeMap, BTreeSet};

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
    Loop(Vec<Node>),
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
                seq.push(Node::Loop(body));
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
}

/// The cap on enumerated paths per object: beyond it the frame declines.
const PATH_CAP: usize = 64;

/// Where a walk runs: the frame, or one loop iteration.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Frame,
    Iteration,
}

/// The loops a walk passed, with the object's state at each entry.
type LoopEntries<'t> = Vec<(&'t [Node], Path)>;

fn step(ev: &Ev, o: u32, p: &mut Path) {
    match ev {
        Ev::Birth(b) if *b == o => p.born = true,
        Ev::Op(b, c) if *b == o && p.born => p.events.push(*c),
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
        Ev::LOp(l, c) if p.holders.contains_key(l) => p.events.push(*c),
        Ev::DecOld(l) => {
            if p.holders.get(l).is_some_and(|h| h.owner && h.fresh) {
                p.events.push('d');
            }
            p.holders.remove(l);
        }
        Ev::Exit => p.ended = true,
        _ => {}
    }
}

/// The end of one iteration (natural, `break` or `continue`): each owner
/// local bound in it keeps its block until its next rebind or the epilogue
/// releases it — that release, recorded here.
fn end_iteration(p: &mut Path) {
    for h in p.holders.values() {
        if h.owner && h.fresh {
            p.events.push('d');
        }
    }
    p.ended = true;
}

/// Extend each live path through `seq`, for object `o`.
fn walk<'t>(
    seq: &'t [Node],
    o: u32,
    scope: Scope,
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
                    Scope::Iteration => end_iteration(&mut p),
                    Scope::Frame => return Err("loop-jump-outside-loop".into()),
                },
                Node::Ev(Ev::Exit) if scope == Scope::Iteration => return Err("loop-exit".into()),
                Node::Ev(ev) => step(ev, o, &mut p),
                Node::Branch(arms) => {
                    for arm in arms {
                        next.extend(walk(arm, o, scope, vec![p.clone()], loops)?);
                    }
                    continue;
                }
                // A loop is its own activation: skipped here, walked later
                // from the state it was entered with.
                Node::Loop(body) => loops.push((body.as_slice(), p.clone())),
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

/// Two or fewer distinct paths as one certificate line.
fn line(paths: &[Path]) -> Result<String, String> {
    let set: BTreeSet<&str> = paths.iter().map(|p| p.events.as_str()).collect();
    let v: Vec<&str> = set.into_iter().collect();
    match v.as_slice() {
        [one] => Ok((*one).to_string()),
        [a, b] => Ok(format!("{{{a}|{b}}}")),
        more => Err(format!("branch-paths:{}", more.len())),
    }
}

/// One object's lines: the frame line, then one line per loop activation
/// that touches it.
fn render_object(tree: &[Node], o: u32, out: &mut String) -> Result<(), String> {
    let mut loops: LoopEntries = Vec::new();
    let frame = walk(tree, o, Scope::Frame, vec![Path::default()], &mut loops)?;
    out.push_str(&line(&frame)?);
    out.push('\n');
    // Each loop reached, walked from each entry state; loops nested in a
    // body are reached by that body's walk (appended as it runs).
    let mut k = 0;
    let mut by_loop: Vec<(&[Node], Vec<Path>)> = Vec::new();
    while k < loops.len() {
        let (body, entry) = loops[k].clone();
        k += 1;
        let start = Path {
            events: String::new(),
            born: entry.born,
            holders: entry.holders.iter().map(|(&l, h)| (l, Holder { fresh: false, ..*h })).collect(),
            ended: false,
        };
        let mut iter = walk(body, o, Scope::Iteration, vec![start], &mut loops)?;
        iter.iter_mut().filter(|p| !p.ended).for_each(end_iteration);
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
            out.push_str(&line(&iter)?);
            out.push('\n');
        }
    }
    Ok(())
}

/// Render every object's lines, in object order; `Err(reason)` withdraws
/// the certificate.
pub(crate) fn render(log: &[Ev], objects: u32) -> Result<String, String> {
    let Some(tree) = parse(log) else {
        return Err("branch-log:unbalanced".into());
    };
    let mut s = String::new();
    for o in 0..objects {
        render_object(&tree, o, &mut s)?;
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Ev::*;

    fn cert(log: &[Ev], n: u32) -> String {
        render(log, n).expect("renders")
    }

    fn bind(local: u32, obj: u32) -> Ev {
        Bind { local, obj, owner: true }
    }

    #[test]
    fn a_straight_line_renders_flat() {
        assert_eq!(cert(&[Birth(0), Op(0, 'i'), bind(3, 0), LOp(3, 'd')], 1), "id\n");
    }

    #[test]
    fn a_param_shared_in_one_arm_renders_as_a_whole_line_branch() {
        // i ; if { a m } else { } ; d
        let log = [Birth(0), Op(0, 'i'), bind(3, 0), Open, LOp(3, 'a'), LOp(3, 'm'), Arm, Close, LOp(3, 'd')];
        assert_eq!(cert(&log, 1), "{iamd|id}\n");
    }

    #[test]
    fn a_local_bound_in_one_arm_is_released_at_the_epilogue_on_that_path_only() {
        let log = [Open, Birth(0), Op(0, 'i'), bind(4, 0), Arm, Close, LOp(4, 'd')];
        assert_eq!(cert(&log, 1), "{|id}\n");
    }

    #[test]
    fn a_var_reassigned_in_one_arm_is_followed_across_the_join() {
        // var x = a; if { x = b (release a) } else { } ; epilogue releases x
        let log = [
            Birth(0), Op(0, 'i'), bind(3, 0),
            Open, LOp(3, 'd'), Birth(1), Op(1, 'i'), bind(3, 1), Arm, Close,
            LOp(3, 'd'),
        ];
        assert_eq!(cert(&log, 2), "id\n{|id}\n");
    }

    #[test]
    fn an_exit_ends_its_path() {
        let log = [Birth(0), Op(0, 'i'), bind(3, 0), Open, LOp(3, 'd'), Exit, Arm, Close, LOp(3, 'd')];
        assert_eq!(cert(&log, 1), "id\n");
        // A missing release on the exiting arm: two paths, one leaking.
        let leak = [Birth(0), Op(0, 'i'), bind(3, 0), Open, Exit, Arm, Close, LOp(3, 'd')];
        assert_eq!(cert(&leak, 1), "{i|id}\n");
    }

    #[test]
    fn a_loop_body_is_an_activation_and_its_locals_live_one_iteration() {
        // xs (param, owned); for .. { let t = f(); g(xs) } ; epilogue d xs, d t
        let log = [
            Birth(0), Op(0, 'i'), bind(3, 0),
            LoopOpen,
            DecOld(4), Birth(1), Op(1, 'i'), bind(4, 1),
            LOp(3, 'a'), LOp(3, 'm'),
            LoopClose,
            LOp(3, 'd'), LOp(4, 'd'),
        ];
        // xs: frame line `id`, activation line `am`; t: its iteration `id`.
        assert_eq!(cert(&log, 2), "id\nam\n\nid\n");
    }

    #[test]
    fn a_rebind_in_the_same_iteration_releases_the_earlier_block() {
        // The unrolled lane: two copies of `let t = f()` in one iteration.
        let log = [
            LoopOpen,
            DecOld(4), Birth(0), Op(0, 'i'), bind(4, 0),
            DecOld(4), Birth(1), Op(1, 'i'), bind(4, 1),
            LoopClose,
            LOp(4, 'd'),
        ];
        assert_eq!(cert(&log, 2), "\nid\n\nid\n");
    }

    #[test]
    fn a_break_ends_the_iteration() {
        let log = [LoopOpen, DecOld(4), Birth(0), Op(0, 'i'), bind(4, 0), Open, Jump, Arm, Close, LoopClose];
        assert_eq!(cert(&log, 1), "\nid\n");
    }

    #[test]
    fn an_exit_inside_a_loop_body_withdraws() {
        let log = [Birth(0), Op(0, 'i'), bind(3, 0), LoopOpen, LOp(3, 'a'), Exit, LoopClose, LOp(3, 'd')];
        assert_eq!(render(&log, 1), Err("loop-exit".into()));
    }

    #[test]
    fn an_unbalanced_log_withdraws() {
        assert!(render(&[Open, Birth(0)], 1).is_err());
        assert!(render(&[LoopOpen, Birth(0)], 1).is_err());
    }

    #[test]
    fn the_control_state_marks_dead_code_after_an_exit_and_a_jump() {
        let mut b = Branches::default();
        let mut log = Vec::new();
        b.open(&mut log);
        b.arm(&mut log);
        b.exit(&mut log);
        assert!(b.dead());
        b.arm(&mut log);
        assert!(!b.dead());
        b.exit(&mut log);
        b.close(&mut log);
        assert!(b.dead(), "every arm exited: the frame is dead after the join");
        let mut b = Branches::default();
        b.loop_open(&mut log);
        b.jump(&mut log);
        assert!(b.dead());
        b.loop_close(&mut log);
        assert!(!b.dead(), "the code after a loop is live");
        assert!(b.settled());
    }
}
