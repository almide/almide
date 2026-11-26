//! The matching of a frame's recorded events to its RC calls (rc.rs).
//!
//! Events to calls: a MAXIMUM matching, each call taking at most one
//! event, so the result never depends on the order the recorder logged
//! events in (an alias's reads name several variables; a first-fit
//! pass let one variable's event take the call another's needed).
//! Compatible pairs:
//! - a named `a` / `d` takes a call of its kind read through its local;
//!   a `d` may instead take a copy-on-write judge call on its local (the
//!   judge released the variable's old block when it copied it);
//! - an unnamed `a-` / `d-` takes any call of its kind;
//! - a rebind `o` takes a release or a judge call on its local;
//! - a move `m` (the variable's credit handed into an arm or a
//!   routine) takes a release on its local INSIDE that window — the
//!   arm's or routine's release of the credit it was given, one per
//!   path (below). A move the arm keeps (stores or returns) takes
//!   nothing.
//!
//! The rebinds are matched first, the named and unnamed events next,
//! the moves last (augmenting paths never unmatch an earlier event).
//! A rebind may go without an instruction, so it must not be left the
//! call a release elsewhere was dropped from: matched after the
//! releases, it would let a release take the rebind's call and go
//! unmatched itself, hiding the dropped release (the
//! rc-release-dropped drill). A move takes only what the frame's own
//! events left inside its window — its credit may as well leave
//! through the callee (a tail call) with no release here;
//! each event tries the calls outside every arm and routine window
//! before those inside one.
//! - a named `a` / `d` may instead be an arm's or a routine's
//!   consumption of the variable: a call INSIDE an arm or routine
//!   window (no RC call itself) takes the variable's value at that
//!   call as argument `j`, and the callee's checked signature
//!   declares parameter `j` consumed (#4278 B1). Each such argument
//!   is one node of the matching, so it accounts for ONE event.

use super::{is_move, Frame, RcReport, EXPLAIN};
use crate::operands::{pl, T, FUEL};

/// How deep a select tree [`Frame::select_leaves`] follows nested `if`s.
const SELECT_DEPTH: u32 = 8;

/// The matching's order and state: the events in matching order, the
/// nodes (the RC calls, then the consuming arguments) in preference order,
/// which calls a move could take, and per node the event holding it.
struct Matching {
    order: Vec<usize>,
    by_pref: Vec<usize>,
    move_fits: Vec<bool>,
    owner: Vec<Option<usize>>,
}

impl Frame<'_, '_> {
    /// Match the frame's events to its calls; refuse an event or a call
    /// left over.
    pub(super) fn verdict(&self, rep: &mut RcReport) -> Result<(), String> {
        let mut m = self.preference();
        self.match_events(&mut m);
        self.absorb(&mut m);
        let shared = self.loop_backs(&mut m.owner);
        // Who holds a node can change along an augmenting path; read the
        // final matching.
        let mut got = vec![false; self.ev.evs.len()];
        for o in m.owner.iter().flatten() {
            got[*o] = true;
        }
        for e in shared {
            got[e] = true;
        }
        let mut co: Vec<Vec<usize>> = vec![Vec::new(); self.nc];
        self.share(&mut m.owner, &m.by_pref, &mut got, &mut co);
        self.pair_arms(&mut m.owner, &mut got, &mut co);
        self.report(&m.owner, &got, rep)
    }

    /// More generally: a call that has a reading of its own when left over
    /// would be tried after the calls that have none; none has one now. An
    /// arm's or a routine's window gives no such reading (#4278 B1): every
    /// RC call either emits is one of its logged events; nor does a call
    /// before an abort (sec-b2): it is matched like any other.
    fn fallback(&self, i: usize) -> bool {
        i >= self.live
    }

    fn preference(&self) -> Matching {
        let (evs, nc) = (&self.ev.evs, self.nc);
        let order: Vec<usize> = (0..evs.len())
            .filter(|&e| evs[e].0 == 'o')
            .chain((0..evs.len()).filter(|&e| evs[e].0 != 'o' && evs[e].0 != 'D' && !is_move(evs[e].0)))
            .chain((0..evs.len()).filter(|&e| is_move(evs[e].0)))
            .collect();
        // A call inside an arm or routine window is tried after those
        // outside (a windowed temporary's event can take only a call inside
        // its window), and a consuming argument last.
        // A call a move (`m` / `h`) could take is tried after the others —
        // before any other preference (a hold's name makes its release read
        // as a frame credit, which would otherwise sort it first): a move is
        // matched last and never displaces an event, so an unnamed event that
        // took such a call first would leave the move's release over and an
        // instruction the window's own events account unmatched.
        let move_fits: Vec<bool> = (0..nc).map(|i| (0..evs.len()).any(|e| is_move(evs[e].0) && self.fits(e, i))).collect();
        let mut by_pref: Vec<usize> = (0..nc).collect();
        by_pref.sort_by_key(|&i| (i >= self.live, move_fits[i], self.fallback(i), self.in_window(self.calls[i].0), self.calls[i].0));
        by_pref.extend(nc..nc + self.takers.len());
        let owner = vec![None; nc + self.takers.len()];
        Matching { order, by_pref, move_fits, owner }
    }

    fn match_events(&self, m: &mut Matching) {
        let (evs, nc) = (&self.ev.evs, self.nc);
        let fits = |e: usize, i: usize| self.fits(e, i);
        // First the RC calls alone, so as many of them as possible are
        // accounted; then each event still unmatched may reach a consuming
        // argument, re-routing others along the way (an augmenting path never
        // frees a node, so no call matched in the first pass is left over).
        let calls_only: Vec<usize> = m.by_pref.iter().copied().filter(|&i| i < nc).collect();
        for &e in &m.order {
            let mut seen = vec![false; m.owner.len()];
            augment(e, &calls_only, &fits, &mut seen, &mut m.owner);
        }
        // A move is not re-routed here: it may take nothing (its arm or
        // routine keeps the credit), so it never displaces a release from its
        // own call onto a consuming argument — that would trade an instruction
        // the frame's events account for a signed consumption.
        for &e in &m.order {
            if m.owner.contains(&Some(e)) || is_move(evs[e].0) {
                continue;
            }
            let mut seen = vec![false; m.owner.len()];
            augment(e, &m.by_pref, &fits, &mut seen, &mut m.owner);
        }
        // Then from the calls' side: a call with no reading of its own still
        // unmatched takes an event along an alternating path that ends at an
        // unmatched event, or frees a call that has a reading of its own — so
        // the leftovers are, as far as any matching allows, the calls that
        // have one (never fewer events matched).
        let fallback_call = |i: usize| i >= nc || self.fallback(i);
        for c in 0..nc {
            if m.owner[c].is_none() && !self.fallback(c) {
                let mut seen = vec![false; evs.len()];
                rehome(c, evs.len(), &fits, &fallback_call, &mut seen, &mut m.owner);
            }
        }
    }

    fn absorb(&self, m: &mut Matching) {
        let (evs, nc) = (&self.ev.evs, self.nc);
        let fits = |e: usize, i: usize| self.fits(e, i);
        let (move_fits, by_pref, owner) = (&m.move_fits, &m.by_pref, &mut m.owner);
        let or_move = |i: usize| i >= nc || self.fallback(i) || move_fits[i];
        for round in 0..2 {
            self.absorb_moves(owner, by_pref);
            self.absorb_once(owner, by_pref);
            if round == 1 {
                break;
            }
            // A call still left over after the absorptions (refused as it
            // stands) may end its path at a call a move could take instead
            // (its release inside the window the credit moved into): the
            // absorptions run again, and a call no move takes is still judged
            // as left over.
            for c in 0..nc {
                if owner[c].is_none() && !self.fallback(c) {
                    let mut seen = vec![false; evs.len()];
                    rehome(c, evs.len(), &fits, &or_move, &mut seen, owner);
                }
            }
        }
    }

    /// The LOOP-BACK rule (decision row anchor.check). A self tail call the
    /// loop conversion rewrote (almide-wasm tco.rs) ends the activation at
    /// its `br`, carrying an owner local into the next iteration with the
    /// block it holds: the recorder ends the credit there (`D<local>`), and
    /// the bytes release that block where the local is next released as a
    /// rebind's old occupant or by an early exit — a release the recorder
    /// logs as that site's `o<local>` (the dec-old), in the activation that
    /// holds nothing yet. So a `D` takes a live release on its local that
    /// an `o` on that local holds; the displaced `o` is then a rebind with no
    /// instruction of its own (counted). It never takes a call no event
    /// holds, nor one a release (`d`), a share or a move holds: the calls the
    /// frame accounts are the same with or without it, so it cannot hide a
    /// release the bytes dropped or one they added; a `D` left with no such
    /// call is refused. A loop-back ends its activation's path, so the `D`s
    /// of one local at different loop-backs lie on paths no run passes two
    /// of — a `D` on a path EXCLUSIVE of every `D` already holding a call
    /// (two arms of one recorder site) shares that call, as the moves of
    /// one credit do (`absorb_moves`); one that is not takes a call of its
    /// own. Returns the `D`s that share.
    fn loop_backs(&self, owner: &mut [Option<usize>]) -> Vec<usize> {
        let evs = &self.ev.evs;
        let exclusive = |a: usize, b: usize| {
            self.ev.evpath[a].iter().any(|(s, x)| self.ev.evpath[b].iter().any(|(t, y)| s == t && x != y))
        };
        // Per call a D holds, the D events that share it.
        let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
        let mut shared = Vec::new();
        for (e, &(kind, local)) in evs.iter().enumerate() {
            let ('D', Some(l)) = (kind, local) else { continue };
            if let Some(g) = groups
                .iter_mut()
                .find(|(i, members)| evs[members[0]].1 == Some(l) && owner[*i] == Some(members[0]) && members.iter().all(|&m| exclusive(m, e)))
            {
                g.1.push(e);
                shared.push(e);
                continue;
            }
            let rebind_held = |i: usize| {
                matches!(self.calls[i].1, 'd' | 'j')
                    && super::through(&self.calls[i].2, l)
                    && owner[i].is_some_and(|o| evs[o] == ('o', Some(l)))
            };
            if let Some(i) = (0..self.live).find(|&i| rebind_held(i)) {
                owner[i] = Some(e);
                groups.push((i, vec![e]));
            }
        }
        shared
    }

    /// A move's credit is released by the arm on some of its paths: one
    /// release per path. Releases of it that no run passes through two of
    /// (the two arms of one `if`) are all that one move's.
    fn absorb_moves(&self, owner: &mut [Option<usize>], by_pref: &[usize]) {
        let (evs, nc) = (&self.ev.evs, self.nc);
        for e in (0..evs.len()).filter(|&e| is_move(evs[e].0)) {
            let mut held: Vec<usize> = (0..nc).filter(|&i| owner[i] == Some(e)).collect();
            for &i in by_pref.iter().filter(|&&i| i < nc) {
                if owner[i].is_none() && self.fits(e, i) && held.iter().all(|&h| self.fl.exclusive(self.calls[h].0, self.calls[i].0)) {
                    owner[i] = Some(e);
                    held.push(i);
                }
            }
        }
    }

    /// An `a` / `d` the recorder logged outside every branch site happens
    /// once on each path: the same value's release (or +1) emitted once in
    /// each arm of an `if` is that one event's, never twice on one path.
    fn absorb_once(&self, owner: &mut [Option<usize>], by_pref: &[usize]) {
        let (evs, nc, calls) = (&self.ev.evs, self.nc, &self.calls);
        for e in (0..evs.len()).filter(|&e| matches!(evs[e].0, 'a' | 'd') && self.ev.evdepth[e] == 0) {
            let mut held: Vec<usize> = (0..nc).filter(|&i| owner[i] == Some(e)).collect();
            let Some(&first) = held.first() else { continue };
            for &i in by_pref.iter().filter(|&&i| i < nc) {
                if owner[i].is_none()
                    && calls[i].1 == calls[first].1
                    && !calls[i].2.is_empty()
                    && calls[i].2 == calls[first].2
                    && held.iter().all(|&h| self.fl.exclusive(calls[h].0, calls[i].0))
                {
                    owner[i] = Some(e);
                    held.push(i);
                }
            }
        }
    }

    /// Events the recorder logged in different arms of one branch site
    /// never happen on one path.
    fn exclusive_events(&self, x: usize, y: usize) -> bool {
        for (p, q) in self.ev.evpath[x].iter().zip(self.ev.evpath[y].iter()) {
            if p.0 != q.0 {
                return false;
            }
            if p.1 != q.1 {
                return true;
            }
        }
        false
    }

    /// Events logged in different arms of one branch site share one call
    /// past the join (the `+1` on the value the arms chose): each such
    /// event's, never two on a path.
    fn share(&self, owner: &mut [Option<usize>], by_pref: &[usize], got: &mut [bool], co: &mut [Vec<usize>]) {
        let (evs, nc) = (&self.ev.evs, self.nc);
        let fits = |e: usize, i: usize| self.fits(e, i);
        let twin = |x: usize, y: usize| self.exclusive_events(x, y);
        let movable = |e: usize| matches!(evs[e].0, 'a' | 'd' | 'o');
        let prefs: Vec<usize> = by_pref.iter().copied().filter(|&c| c < nc).collect();
        for (e, g) in got.iter_mut().enumerate() {
            if *g || !movable(e) {
                continue;
            }
            let mut seen = vec![false; nc];
            if share_path(e, &prefs, (&fits, &twin, &movable), &mut seen, owner, co) {
                *g = true;
            }
        }
    }

    /// A `+1` / release past the join of an anchored `if` on the value its
    /// arms chose: one event in each arm, each naming its own arm's value
    /// (arm i of the recorder's site is the shipped arm i — the anchor
    /// holds it, never trusted), logged at the site's own level (on every
    /// path through that arm). An arm whose value is itself an anchored `if`
    /// (a NESTED select, `if a then (if b then x else y) else z`) takes, in
    /// place of its one event, one per arm of a recorder site anchored to
    /// that inner `if` and opened directly inside it — recursively, so the
    /// call is one event's on every leaf path of the select tree, each event
    /// naming its own leaf's shipped value.
    ///
    /// A call such a pair could take may already be held by a temporary's
    /// unnamed event (`a-` / `d-`), which fits any call of its kind in its
    /// window: when that event also fits a call still left over, it moves
    /// there and the arm pair takes this call — each call is still one
    /// event's (or the pair's) and each event one call's (#4788: the temporary
    /// took the `+1` past a select's join, leaving its own call over).
    fn pair_arms(&self, owner: &mut [Option<usize>], got: &mut [bool], co: &mut [Vec<usize>]) {
        for c in 0..self.live {
            if !co[c].is_empty() || !matches!(self.calls[c].1, 'a' | 'd') {
                continue;
            }
            if let Some(e) = owner[c] {
                if self.ev.evs[e].1.is_some() || self.arm_pair(c, got).is_none() {
                    continue;
                }
                let Some(free) = (0..self.live).find(|&x| x != c && owner[x].is_none() && co[x].is_empty() && self.fits(e, x)) else { continue };
                owner[free] = Some(e);
                owner[c] = None;
            }
            let Some(leaves) = self.arm_pair(c, got).or_else(|| self.free_leaves(c, owner, got, co)) else { continue };
            let Some((&e0, rest)) = leaves.split_first() else { continue };
            owner[c] = Some(e0);
            co[c].extend_from_slice(rest);
            for &e in &leaves {
                got[e] = true;
            }
        }
    }

    /// The leaves of call `c`'s select tree when some already hold another
    /// call each: every such call is handed to an event still unmatched that
    /// fits it (one each, none of the leaves), and the leaves are freed for
    /// `c` — an augmenting step, so no event or call ends up matched twice
    /// and none that was matched is left over. `None` (nothing changed) when
    /// a held call has no such event, or a leaf holds a shared or second call.
    fn free_leaves(&self, c: usize, owner: &mut [Option<usize>], got: &mut [bool], co: &[Vec<usize>]) -> Option<Vec<usize>> {
        let leaves = self.arm_pair(c, &vec![false; got.len()])?;
        let mut plan: Vec<(usize, usize, usize)> = Vec::new();
        for &x in leaves.iter().filter(|&&x| got[x]) {
            let held: Vec<usize> = (0..owner.len()).filter(|&k| owner[k] == Some(x)).collect();
            let [k] = held.as_slice() else { return None };
            if *k >= self.nc || !co[*k].is_empty() || co.iter().any(|v| v.contains(&x)) {
                return None;
            }
            let e = (0..got.len()).find(|&e| !got[e] && !leaves.contains(&e) && !plan.iter().any(|p| p.2 == e) && self.fits(e, *k))?;
            plan.push((x, *k, e));
        }
        for (x, k, e) in plan {
            owner[k] = Some(e);
            got[e] = true;
            got[x] = false;
        }
        self.arm_pair(c, got)
    }

    /// The unmatched events, one per leaf path of the anchored select tree,
    /// that call `c` past its outer `if` is.
    fn arm_pair(&self, c: usize, got: &[bool]) -> Option<Vec<usize>> {
        let arg = self.fl.arg(self.calls[c].0, 0, FUEL);
        let T::Opaque(p) = pl(&arg) else { return None };
        self.select_leaves(*p, &[], self.calls[c].1, got, SELECT_DEPTH)
    }

    /// The events of the select whose `if` closes at `end`, reached through
    /// the recorder arms `outer` (outermost first; empty at the top): for
    /// each arm, the event naming its shipped value in the recorder arm
    /// `outer ++ [(site, arm)]` (the event's own innermost arms), or that
    /// arm's own nested select's events. Every recorder site anchored to
    /// this `if` (the arms' own site and a select site recorded over it
    /// later) is a candidate; the first that pairs every leaf is taken.
    fn select_leaves(&self, end: usize, outer: &[(usize, usize)], kind: char, got: &[bool], depth: u32) -> Option<Vec<usize>> {
        let depth = depth.checked_sub(1)?;
        let evs = &self.ev.evs;
        let (s, e, vals) = self.fl.if_arms(end, FUEL)?;
        let mut sites: Vec<usize> = self.ev.site_at.iter().filter(|(_, &at)| at == (s, e)).map(|(&k, _)| k).collect();
        sites.sort_unstable();
        sites.iter().find_map(|&site| {
            let mut leaves: Vec<usize> = Vec::new();
            for (arm, val) in vals.iter().enumerate() {
                let path: Vec<(usize, usize)> = outer.iter().copied().chain([(site, arm)]).collect();
                let names = self.names_of(val);
                if !names.is_empty() {
                    let x = (0..evs.len()).find(|&x| {
                        !got[x]
                            && !leaves.contains(&x)
                            && evs[x].0 == kind
                            && self.ev.evpath[x].ends_with(&path)
                            && evs[x].1.is_some_and(|l| names.contains(&l))
                    })?;
                    leaves.push(x);
                } else {
                    let T::Opaque(q) = pl(val) else { return None };
                    let inner = self.select_leaves(*q, &path, kind, got, depth)?;
                    if inner.iter().any(|x| leaves.contains(x)) {
                        return None;
                    }
                    leaves.extend(inner);
                }
            }
            Some(leaves)
        })
    }

    /// The counts, and the refusal of an event or a call left over.
    fn report(&self, owner: &[Option<usize>], got: &[bool], rep: &mut RcReport) -> Result<(), String> {
        let (evs, nc, live, calls) = (&self.ev.evs, self.nc, self.live, &self.calls);
        rep.signed += owner[nc..].iter().filter(|o| o.is_some()).count();
        let mut unmatched: Vec<String> = Vec::new();
        for (e, &(kind, local)) in evs.iter().enumerate() {
            if got[e] {
                continue;
            }
            match (kind, local) {
                ('o', _) => rep.rebind_unmatched += 1,
                (c, _) if is_move(c) => {}
                // #1961: a region reclaim takes no call; it is held to its
                // restore (rc.rs `reclaim_holds`), refused otherwise.
                ('w', None) if self.reclaim_holds(e) => {}
                // A window's callee that holds the variable but declares no
                // consuming signature for that parameter (#4278 B1).
                (k @ ('a' | 'd'), Some(l)) if (0..self.takers.len()).any(|t| self.holds(l, t)) => {
                    unmatched.push(format!("{k}{l} (no callee in its window declares that it consumes it)"))
                }
                (k, Some(l)) => unmatched.push(format!("{k}{l}")),
                (k, None) => unmatched.push(format!("{k}-")),
            }
        }
        rep.judges_unmatched += (0..live).filter(|&i| owner[i].is_none() && calls[i].1 == 'j').count();
        let extra: Vec<String> = (0..live)
            .filter(|&i| owner[i].is_none() && calls[i].1 != 'j')
            .map(|i| format!("{}@{} on {:?}", calls[i].1, calls[i].0, calls[i].2))
            .collect();
        if unmatched.is_empty() && extra.is_empty() {
            return Ok(());
        }
        let mut msg = format!(
            "rc: function {}: recorded events with no call [{}]; calls with no recorded event [{}]",
            self.pos,
            unmatched.join(" "),
            extra.join(", ")
        );
        if EXPLAIN.load(std::sync::atomic::Ordering::Relaxed) {
            self.explain(owner, &mut msg);
        }
        Err(msg)
    }

    /// Diagnosis only (`--explain`): every RC call and what took it.
    fn explain(&self, owner: &[Option<usize>], msg: &mut String) {
        let (evs, calls) = (&self.ev.evs, &self.calls);
        let ev_txt = |e: usize| format!("{}{}", evs[e].0, evs[e].1.map_or("-".to_string(), |l| l.to_string()));
        for i in 0..self.nc {
            let by = match owner[i] {
                Some(e) => ev_txt(e),
                None => "LEFT".to_string(),
            };
            msg.push_str(&format!("\n  {}@{} {:?} <- {}", calls[i].1, calls[i].0, calls[i].2, by));
        }
    }
}

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

/// [`share_path`]'s predicates: an event fits a call, two events are in
/// different arms of one site, an event may be moved.
type Preds<'p> = (&'p dyn Fn(usize, usize) -> bool, &'p dyn Fn(usize, usize) -> bool, &'p dyn Fn(usize) -> bool);

/// An alternating path ending in a share: `e` takes a call, its holder
/// (holding nothing else) takes another, … until one shares a call
/// with an event in another arm of a common site.
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
        if owner[c].is_some_and(|o| twin(e, o)) && co[c].iter().all(|&o| twin(e, o)) {
            co[c].push(e);
            return true;
        }
        let Some(o) = owner[c] else { continue };
        if !co[c].is_empty() || !movable(o) || owner.iter().enumerate().any(|(x, h)| x != c && *h == Some(o)) {
            continue;
        }
        // Only a call whose holder is displaced is closed to the rest of
        // this path: one merely tried as a share stays open to a later
        // event of the path (a displaced holder may still share it).
        seen[c] = true;
        if share_path(o, prefs, (fits, twin, movable), seen, owner, co) {
            owner[c] = Some(e);
            return true;
        }
    }
    false
}
