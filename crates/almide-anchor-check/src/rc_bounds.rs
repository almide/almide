//! #4631 (b1-inc): the ownership witness's rc_inc letters held to the
//! frame's recorded RC calls by count bounds.
//!
//! The ownership witness the checkers judge is the recorder's RENDERING of
//! its event log: one line per object (and per loop activation), each a set
//! of path classes `{p|q}`. The bytes are tied to a different record, the
//! anchor file's `rc` line (rc.rs matches each event to a shipped call).
//! This check ties the two for the rc_inc family: the witness's `a` / `s`
//! letters against the rc line's `a` / `s` events.
//!
//! Over every path a frame can take, the witness claims (summing each
//! object's lines) between `w.min` and `w.max` rc_incs, and the rc line's
//! events (choosing an arm at every recorded site) between `r.min` and
//! `r.max`. A real path's count lies in both, so the intervals must meet:
//! a witness claiming an rc_inc the shipped calls do not hold (the
//! use-after-free direction: the checker would believe a count the bytes
//! never take), or dropping one in a frame with one path, is refused. A
//! witness line with a loop item claims no upper bound.
//!
//! The release family is NOT checked here: the rc line is a call log and
//! the witness a per-holder net account — a rebind's release of a local
//! that held nothing, a borrowed parameter's moved-in credit, a slot
//! consume inside an arm do not map one to one (measured: 7324 of 25141
//! corpus frames would be refused). That correspondence is the rendering
//! #4632 re-derives. TRUSTED: release accounting and the partition of ops
//! into path classes and objects (#4632).

use std::collections::HashMap;

/// A count interval; `max: None` is unbounded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub min: u64,
    pub max: Option<u64>,
}

impl Span {
    const ZERO: Span = Span { min: 0, max: Some(0) };

    fn add(self, o: Span) -> Span {
        Span { min: self.min + o.min, max: self.max.zip(o.max).map(|(a, b)| a + b) }
    }

    fn either(self, o: Span) -> Span {
        Span { min: self.min.min(o.min), max: self.max.zip(o.max).map(|(a, b)| a.max(b)) }
    }

    fn meets(self, o: Span) -> bool {
        self.max.is_none_or(|m| m >= o.min) && o.max.is_none_or(|m| m >= self.min)
    }
}

/// The two RC-call letter families (only `Inc` is enforced; `Rel` is
/// measured by the tests, see the module note).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Inc,
    Rel,
}

fn witness_kind(c: u8) -> Option<Kind> {
    match c {
        b'a' | b'A' | b's' | b'S' => Some(Kind::Inc),
        b'd' | b'D' => Some(Kind::Rel),
        _ => None,
    }
}

fn rc_kind(t: &str) -> Option<Kind> {
    match t.as_bytes().first()? {
        b'a' | b's' => Some(Kind::Inc),
        b'd' | b'o' | b'r' => Some(Kind::Rel),
        _ => None,
    }
}

/// One witness line's span of `kind`: right to left, an op adds, a branch
/// `{a|b}` takes either arm (an arm marked `x` / `t` ends the line there), a
/// loop `( … )` or `[ … | … ]` runs any number of times.
pub fn line_span(line: &[u8], kind: Kind) -> Span {
    let count = |s: &[u8]| s.iter().filter(|&&c| witness_kind(c) == Some(kind)).count() as u64;
    let ends = |s: &[u8]| s.iter().any(|&c| matches!(c, b'x' | b'X' | b't' | b'T'));
    // Items, left to right.
    let mut items: Vec<Item> = Vec::new();
    let mut i = 0;
    while i < line.len() {
        let c = line[i];
        let close = match c {
            b'{' => Some(b'}'),
            b'(' => Some(b')'),
            b'[' => Some(b']'),
            _ => None,
        };
        match close {
            Some(cl) => {
                let j = line[i + 1..].iter().position(|&b| b == cl).map_or(line.len(), |k| i + 1 + k);
                let body = &line[i + 1..j];
                items.push(if c == b'{' {
                    let bar = body.iter().position(|&x| x == b'|').unwrap_or(body.len());
                    let (then_arm, rest) = body.split_at(bar);
                    let else_arm = rest.get(1..).unwrap_or(&[]);
                    Item::Branch((count(then_arm), ends(then_arm)), (count(else_arm), ends(else_arm)))
                } else {
                    Item::Loop(count(body))
                });
                i = j + 1;
            }
            None => {
                if witness_kind(c) == Some(kind) {
                    items.push(Item::Op);
                }
                i += 1;
            }
        }
    }
    items.iter().rev().fold(Span::ZERO, |rest, it| match *it {
        Item::Op => rest.add(Span { min: 1, max: Some(1) }),
        Item::Loop(0) => rest,
        Item::Loop(_) => Span { min: rest.min, max: None },
        Item::Branch(a, b) => {
            let arm = |(n, end): (u64, bool)| if end { Span { min: n, max: Some(n) } } else { rest.add(Span { min: n, max: Some(n) }) };
            arm(a).either(arm(b))
        }
    })
}

enum Item {
    Op,
    Loop(u64),
    Branch((u64, bool), (u64, bool)),
}

/// The witness of one frame: the sum of its lines' spans.
pub fn witness_span(witness: &str, kind: Kind) -> Span {
    witness.split('\n').fold(Span::ZERO, |s, l| s.add(line_span(l.as_bytes(), kind)))
}

/// The rc line's events of one frame: a site `( … | … )` takes one arm (a
/// site with one arm may be skipped).
pub fn rc_span(events: &str, kind: Kind) -> Span {
    // A stack of open sites: per site, its finished arms and the arm in progress.
    let mut stack: Vec<(Vec<Span>, Span)> = vec![(Vec::new(), Span::ZERO)];
    for t in events.split_whitespace() {
        if t.starts_with('(') {
            stack.push((Vec::new(), Span::ZERO));
        } else if t.starts_with('|') {
            if stack.len() > 1 {
                let top = stack.last_mut().expect("site");
                top.0.push(top.1);
                top.1 = Span::ZERO;
            }
        } else if t == ")" {
            if stack.len() > 1 {
                close_site(&mut stack);
            }
        } else if rc_kind(t) == Some(kind) {
            let top = stack.last_mut().expect("frame");
            top.1 = top.1.add(Span { min: 1, max: Some(1) });
        }
    }
    while stack.len() > 1 {
        close_site(&mut stack);
    }
    stack.pop().map_or(Span::ZERO, |(_, s)| s)
}

fn close_site(stack: &mut Vec<(Vec<Span>, Span)>) {
    let (mut arms, cur) = stack.pop().expect("site");
    arms.push(cur);
    if arms.len() == 1 {
        arms.push(Span::ZERO);
    }
    let site = arms.iter().copied().reduce(Span::either).unwrap_or(Span::ZERO);
    let top = stack.last_mut().expect("frame");
    top.1 = top.1.add(site);
}

/// What the bounds check judged.
#[derive(Debug, Default)]
pub struct RcBoundsReport {
    /// Frames with both a witness and an rc line.
    pub frames: usize,
    /// Frames whose spans do not meet: `(frame, kind, witness, rc)`.
    pub refused: Vec<(String, Kind, Span, Span)>,
}

/// Every named rc frame (`rcnames`) against its ownership witness.
pub fn check_rc_bounds(bundle: &[u8], anchors: &str) -> Result<RcBoundsReport, String> {
    let line = |key: &str| anchors.lines().find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix(' '))).unwrap_or("");
    let names: HashMap<usize, &str> =
        line("rcnames").split_whitespace().filter_map(|t| t.split_once(':')).filter_map(|(p, n)| Some((p.parse().ok()?, n))).collect();
    let witnesses: HashMap<String, String> = crate::bundle_records(bundle, "ownership")?
        .into_iter()
        .map(|(f, w)| (f.chars().map(|c| if c.is_whitespace() { '_' } else { c }).collect(), w))
        .collect();
    let mut rep = RcBoundsReport::default();
    for g in line("rc").split(';') {
        let Some((p, events)) = g.trim().split_once('|') else { continue };
        let Some(name) = p.parse::<usize>().ok().and_then(|p| names.get(&p)) else { continue };
        let Some(w) = witnesses.get(*name) else { continue };
        rep.frames += 1;
        let (ws, rs) = (witness_span(w, Kind::Inc), rc_span(events, Kind::Inc));
        if !ws.meets(rs) {
            rep.refused.push((name.to_string(), Kind::Inc, ws, rs));
        }
    }
    Ok(rep)
}

#[cfg(test)]
#[path = "rc_bounds_tests.rs"]
mod tests;
