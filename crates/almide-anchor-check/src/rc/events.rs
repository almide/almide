//! A frame's `rc` line: its recorded events and the recorder's branch sites.

use std::collections::HashMap;

/// An event: its kind and the local it names (`-`: none).
type Event = (char, Option<u32>);

/// An instruction window `<from>-<to>`.
type Window = (usize, usize);

/// A frame's recorded events, read off its `rc` line.
pub(super) struct Events {
    /// Each event.
    pub(super) evs: Vec<Event>,
    /// Each event, and for a move `m<local>@<from>-<to>` the window of
    /// the arm or routine the credit moved into.
    pub(super) evwin: Vec<Option<Window>>,
    /// Each event's depth in the recorder's branch sites (`(` … `)`): at
    /// depth 0 it happens once on every path through the frame.
    pub(super) evdepth: Vec<usize>,
    /// Each event's path through the recorder's branch sites: per open
    /// site, its id and the arm the event is in (`|` starts the next arm).
    pub(super) evpath: Vec<Vec<(usize, usize)>>,
    /// A site the recorder anchored (`(@<if>-<else>`, an `if` lowered with
    /// its arms in source order): the shipped `if` and `else` it is.
    pub(super) site_at: HashMap<usize, (usize, usize)>,
}

/// A branch-site token's anchor (`(@<if>-<else>` / `|@<if>-<else>`).
fn anchor(t: &str) -> Option<(usize, usize)> {
    t.get(2..)?.split_once('-').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))
}

/// One event token: its kind and local, and its window.
fn event(t: &str) -> (Event, Option<Window>) {
    let mut c = t.chars();
    let kind = c.next().unwrap_or('?');
    let rest: String = c.collect();
    let (local, win) = match rest.split_once('@') {
        Some((l, w)) => (l, w.split_once('-').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))),
        None => (rest.as_str(), None),
    };
    ((kind, local.parse().ok()), win)
}

/// The events of an `rc` line.
pub(super) fn parse(events: &str) -> Events {
    let (mut evdepth, mut evpath) = (Vec::new(), Vec::new());
    let mut open_sites: Vec<(usize, usize)> = Vec::new();
    let mut next_site = 0usize;
    let mut site_at: HashMap<usize, (usize, usize)> = HashMap::new();
    let mut tokens: Vec<&str> = Vec::new();
    for t in events.split_whitespace() {
        match t {
            t if t.starts_with('(') => {
                next_site += 1;
                open_sites.push((next_site, 0));
                if let Some(at) = anchor(t) {
                    site_at.insert(next_site, at);
                }
            }
            t if t.starts_with('|') => {
                let Some(s) = open_sites.last_mut() else { continue };
                s.1 += 1;
                // An arm boundary anchored elsewhere than its site:
                // the site is not held to the shipped arms.
                if anchor(t) != site_at.get(&s.0).copied() {
                    site_at.remove(&s.0);
                }
            }
            ")" => {
                open_sites.pop();
            }
            _ => {
                evdepth.push(open_sites.len());
                evpath.push(open_sites.clone());
                tokens.push(t);
            }
        }
    }
    let (evs, evwin) = tokens.into_iter().map(event).unzip();
    Events { evs, evwin, evdepth, evpath, site_at }
}
