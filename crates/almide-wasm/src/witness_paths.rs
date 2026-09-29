//! Branch frames in the structural witness (#2756, #1696 step 4).
//!
//! The recorder (witness.rs) logs every RC event in EMISSION order together
//! with the branch structure the emitter walked: `Open` at an `if` / `match`,
//! `Arm` at each arm boundary, `Close` at the join, `Exit` where an arm leaves
//! the frame (a `return_call`). Emission order is program order within an
//! arm, so the log is the frame's control-flow tree with the events on it.
//!
//! One certificate line per object is rendered from that tree:
//!
//! - **The object's path set.** Every path through the frame is enumerated,
//!   keeping only this object's events. An event on a path where the object
//!   was never born is a release of the NULL slot the local still holds (the
//!   epilogue releases every owner local, and a local bound in the other arm
//!   is zero here), which `$dec_flat` no-ops on, so it is skipped. An `Exit`
//!   ends a path: nothing after it runs on that path. One distinct path
//!   renders flat. Two render as the whole-line branch `{p|q}`. The checker
//!   runs each arm from rc 0, requires the two to agree, and requires the
//!   line to end at 0, so every path must balance on its own. That is exactly
//!   the claim, and it needs no exit marker.
//! - **Per branch site**, when the object has more than two distinct paths
//!   (it lives across several sequential branches) and was born outside all
//!   of them: its top-level sequence is rendered item by item, and each site
//!   becomes `{a|b}` (law 6: every arm consumes the same), or the v5
//!   `{a x|b}` when one arm exits (the exiting arm must reach 0 on its own).
//!   Nested branches inside a site are flattened into the site's arm paths.
//!
//! Anything the grammar cannot carry, such as a site with three distinct
//! arm paths, withdraws the certificate with a counted reason. It never
//! approximates.

use std::collections::BTreeSet;

/// One logged event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ev {
    Birth(u32),
    Op(u32, char),
    Open,
    Arm,
    Close,
    Exit,
}

/// An open branch site while the emitter is inside it.
#[derive(Default)]
struct Site {
    /// The `Open` was logged (the site is not in dead code).
    logged: bool,
    /// An arm has begun.
    started: bool,
    arms: u32,
    exited: u32,
    /// The current arm has left the frame: what follows in it is dead.
    cur_exited: bool,
}

/// The branch state of one frame's recording.
#[derive(Default)]
pub(crate) struct Branches {
    stack: Vec<Site>,
    /// The whole frame has left (a top-level exit, or every arm of a
    /// top-level site exited): nothing emitted after runs.
    frame_dead: bool,
}

impl Branches {
    /// Is the code being emitted right now unreachable?
    pub(crate) fn dead(&self) -> bool {
        self.frame_dead || self.stack.iter().any(|s| s.cur_exited)
    }

    pub(crate) fn open(&mut self, log: &mut Vec<Ev>) {
        let logged = !self.dead();
        if logged {
            log.push(Ev::Open);
        }
        self.stack.push(Site { logged, ..Site::default() });
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

    /// The join. A site whose EVERY arm exited leaves the enclosing arm
    /// (or the frame) too.
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
    Birth(u32),
    Op(u32, char),
    Exit,
    Branch(Vec<Vec<Node>>),
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
            Ev::Arm | Ev::Close => return Some(seq),
            Ev::Birth(o) => seq.push(Node::Birth(*o)),
            Ev::Op(o, c) => seq.push(Node::Op(*o, *c)),
            Ev::Exit => seq.push(Node::Exit),
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
        }
        *pos += 1;
    }
    Some(seq)
}

/// A path through the frame, as one object sees it. Two paths in the same
/// state are interchangeable from here on, so each step keeps one of each
/// (an object untouched by ten sequential branches is one path, not 1024).
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Path {
    events: String,
    exists: bool,
    exited: bool,
}

/// The cap on enumerated paths per object: beyond it the frame declines.
const PATH_CAP: usize = 64;

/// Extend each live path through `seq`, for object `o`.
fn walk(seq: &[Node], o: u32, paths: Vec<Path>) -> Result<Vec<Path>, String> {
    let mut paths = paths;
    for node in seq {
        let mut next = Vec::with_capacity(paths.len());
        for mut p in paths {
            if p.exited {
                next.push(p);
                continue;
            }
            match node {
                Node::Birth(b) if *b == o => p.exists = true,
                // An event on the null slot of an unborn object is a no-op.
                Node::Op(b, c) if *b == o && p.exists => p.events.push(*c),
                Node::Exit => p.exited = true,
                Node::Branch(arms) => {
                    for arm in arms {
                        next.extend(walk(arm, o, vec![p.clone()])?);
                    }
                    continue;
                }
                _ => {}
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

fn distinct(paths: &[Path]) -> BTreeSet<(String, bool)> {
    paths.iter().map(|p| (p.events.clone(), p.exited)).collect()
}

/// One object's line.
fn render_object(tree: &[Node], o: u32) -> Result<String, String> {
    let paths = walk(tree, o, vec![Path::default()])?;
    let whole: BTreeSet<String> = paths.iter().map(|p| p.events.clone()).collect();
    let v: Vec<&String> = whole.iter().collect();
    match v.as_slice() {
        [one] => return Ok((*one).clone()),
        [a, b] => return Ok(format!("{{{a}|{b}}}")),
        _ => {}
    }
    render_by_site(tree, o)
}

/// The per-site rendering of an object born at the top level.
fn render_by_site(tree: &[Node], o: u32) -> Result<String, String> {
    let Some(born) = tree.iter().position(|n| matches!(n, Node::Birth(b) if *b == o)) else {
        return Err("branch-paths:born-in-arm".into());
    };
    let mut line = String::new();
    for node in &tree[born + 1..] {
        match node {
            Node::Op(b, c) if *b == o => line.push(*c),
            Node::Exit => break,
            Node::Branch(arms) => {
                let mut site = Vec::new();
                for arm in arms {
                    site.extend(walk(arm, o, vec![Path { exists: true, ..Path::default() }])?);
                }
                let d: Vec<(String, bool)> = distinct(&site).into_iter().collect();
                let all_exit = d.iter().all(|(_, x)| *x);
                match d.as_slice() {
                    [(a, _)] if all_exit => {
                        line.push_str(a);
                        break;
                    }
                    [(a, _)] => line.push_str(a),
                    [(a, _), (b, _)] if all_exit => {
                        line.push_str(&format!("{{{a}|{b}}}"));
                        break;
                    }
                    [(a, false), (b, false)] => line.push_str(&format!("{{{a}|{b}}}")),
                    [(a, xa), (b, _)] => {
                        let (ex, sv) = if *xa { (a, b) } else { (b, a) };
                        line.push_str(&format!("{{{ex}x|{sv}}}"));
                    }
                    _ => return Err(format!("branch-paths:{}", d.len())),
                }
            }
            _ => {}
        }
    }
    Ok(line)
}

/// Render every object's line, in object order; `Err(reason)` withdraws
/// the certificate.
pub(crate) fn render(log: &[Ev], objects: u32) -> Result<String, String> {
    let Some(tree) = parse(log) else {
        return Err("branch-log:unbalanced".into());
    };
    let mut s = String::new();
    for o in 0..objects {
        s.push_str(&render_object(&tree, o)?);
        s.push('\n');
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

    #[test]
    fn a_straight_line_renders_flat() {
        assert_eq!(cert(&[Birth(0), Op(0, 'i'), Op(0, 'd')], 1), "id\n");
    }

    #[test]
    fn a_param_shared_in_one_arm_renders_as_a_whole_line_branch() {
        // i ; if { a m } else { } ; d
        let log = [Birth(0), Op(0, 'i'), Open, Op(0, 'a'), Op(0, 'm'), Arm, Close, Op(0, 'd')];
        assert_eq!(cert(&log, 1), "{iamd|id}\n");
    }

    #[test]
    fn a_local_bound_in_one_arm_is_released_at_the_epilogue_on_that_path_only() {
        // if { let t = …(born) } else { } ; epilogue dec t (null on else)
        let log = [Open, Birth(0), Op(0, 'i'), Arm, Close, Op(0, 'd')];
        assert_eq!(cert(&log, 1), "{|id}\n");
    }

    #[test]
    fn an_exit_ends_its_path() {
        // i ; if { d exit } else { } ; d
        let log = [Birth(0), Op(0, 'i'), Open, Op(0, 'd'), Exit, Arm, Close, Op(0, 'd')];
        assert_eq!(cert(&log, 1), "id\n");
        // A missing release on the exiting arm is two distinct paths, one
        // of which leaks (the checker rejects `{i|id}`: the arms disagree).
        let leak = [Birth(0), Op(0, 'i'), Open, Exit, Arm, Close, Op(0, 'd')];
        assert_eq!(cert(&leak, 1), "{i|id}\n");
    }

    #[test]
    fn three_sequential_sites_render_per_site() {
        let log = [
            Birth(0), Op(0, 'i'),
            Open, Op(0, 'a'), Op(0, 'm'), Arm, Close,
            Open, Op(0, 'a'), Op(0, 'm'), Arm, Close,
            Op(0, 'd'),
        ];
        assert_eq!(cert(&log, 1), "i{|am}{|am}d\n");
    }

    #[test]
    fn a_site_whose_every_arm_exits_ends_the_line() {
        let log = [
            Birth(0), Op(0, 'i'),
            Open, Op(0, 'a'), Op(0, 'm'), Arm, Close,
            Open, Op(0, 'd'), Exit, Arm, Op(0, 'a'), Op(0, 'm'), Op(0, 'd'), Exit, Close,
        ];
        assert_eq!(cert(&log, 1), "i{|am}{amd|d}\n");
    }

    #[test]
    fn an_unbalanced_log_withdraws() {
        assert!(render(&[Open, Birth(0)], 1).is_err());
    }

    #[test]
    fn the_branch_state_marks_dead_code_after_an_exit() {
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
        assert!(b.settled());
    }

}
