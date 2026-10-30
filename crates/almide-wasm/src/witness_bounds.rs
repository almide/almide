//! The structural leg's BOUNDS witness (#4005) — what makes every index
//! operator access's bounds check something the proven checker re-derives
//! instead of an analysis it trusts.
//!
//! bounds_facts.rs skips the check of `xs[i + c]` when an earlier check of
//! the same key decided it in the same straight-line stretch. This sink
//! records each stretch as a program of the certificate language
//! `BoundsCert.check_bounds_cert` judges (proofs/BoundsCert.v,
//! `almide-verify bounds`), stretches joined by `;`:
//!
//! - `c N` — a check of slot N was emitted (and passed, or the run aborted);
//! - `a N` / `w N` — slot N's element was read / written;
//! - `s N` — slot N's index or list changed (a `let` or assign of a variable
//!   the key names).
//!
//! A slot is one key `(xs, i, c)`, numbered from 0 per stretch. A stretch
//! opens where the emitter starts a fact set (a loop body, and after every
//! statement that empties it) and closes where the set is dropped, so no
//! knowledge crosses a `;` — exactly the emitter's rule, which the checker
//! now enforces. An `s` goes to EVERY open stretch, so a stretch suspended by
//! a nested loop body still sees the body's assignments.
//!
//! EVERY ACCESS IS RECORDED: every `xs[i]` read and `xs[i] = v` store of a
//! list, and every `b[i]` read and store of a Bytes value. An access that is
//! not a fact candidate (no open fact set, no hoisted count, a non-affine
//! index, a cell or a global) gets a FRESH slot in the pass's LOOSE stretch,
//! so it is accepted only if its own check precedes it. The two halves of an
//! access are recorded where their instructions are emitted, not where the
//! elision is decided: `c` by the routine that emits the compare
//! (`emit_read_bounds_check`, the store route's compare, the Bytes room
//! test), `a` / `w` right after the load or store. An emitter that drops a
//! check therefore leaves a bare `a` / `w`, which the checker rejects
//! (scripts/witness-reinject.sh bounds re-injects such bugs and requires the
//! rejection).
//!
//! Each `c` / `a` / `w` also keeps its ANCHOR (witness_anchor.rs): where in
//! the body its instruction ends. `almide-anchor-check` decodes the shipped
//! bytes and checks each located event there (#4011 (d)): a `c` is an
//! unsigned compare (or a Bytes room test) opening an `if` that aborts, an
//! `a` a load, a `w` a store, and a slot's events lie in byte order.
//!
//! The compare's OPERANDS are checked on the bytes too: almide-anchor-check
//! (operands.rs) rebuilds the compare's index and count and the access's
//! address from the shipped instructions and holds them to each other, so a
//! wrong index, a wrong list, a wrong stride or a variable changed between
//! the check and the access rejects — the hoisted count (len_hoist.rs) and
//! the payload pointer (payload_ptr.rs) included. For that it needs to know
//! which calls are copy-on-write JUDGES (a judge returns a block of its
//! argument's count): every judge call on a list or Bytes local, global or
//! cell records its anchor with [`judge`].
//!
//! A `for x in xs` loop over a list (#4280) reads its element guarded by the
//! loop's EXIT test, `cur >=u count; br_if` to the block around the loop,
//! not by an aborting check. Each such loop records a stretch of its own,
//! `e0 a0`: `e` anchored at the `br_if`, `a` at the element's load.
//! `PExit` (proofs/SpatialCert.v) is that test — failing it ends the
//! stretch normally — and almide-anchor-check holds the `br_if`'s compare
//! to the load as it holds a `c`'s: the counter against `len(xs) / S`
//! captured at entry, the load at `xs + cur * S`, neither changed between.
//! The loop holds its subject across the body (+1 on a borrowed list,
//! #4351); that `rc_inc` call is recorded as a HOLD ([`hold`]), and the
//! anchor file's `holds` line lets almide-anchor-check accept a call handed
//! the list inside the loop (the copy-on-write contract: a shared block is
//! never written or resized in place).
//! Element accesses that go through neither (stdlib `list.get`, the
//! runtime's own routines) are not recorded.

use std::sync::Mutex;

/// A fact key: list variable, index variable, constant offset.
pub(crate) type Key = (u32, u32, i64);

#[derive(Default)]
struct Stretch {
    slots: Vec<Key>,
    text: String,
    /// One anchor per token of `text`.
    anchors: String,
}

#[derive(Default)]
struct State {
    pass: usize,
    open: Vec<Stretch>,
    /// The pass's non-candidate accesses, one fresh slot each.
    loose: Stretch,
    stretches: Vec<String>,
    stretch_anchors: Vec<String>,
    done: Vec<(usize, String)>,
    done_anchors: Vec<(usize, String)>,
    /// The pass's copy-on-write judge calls, one anchor each.
    judges: String,
    done_judges: Vec<(usize, String)>,
    /// The pass's loop HOLDS (#4280): the rc_inc a list `for` takes on its
    /// subject before the loop, one anchor each.
    holds: String,
    done_holds: Vec<(usize, String)>,
    /// A site outlived its stretch: the witness is made unparseable, so it
    /// is rejected rather than read short.
    broken: bool,
}

/// Where one access's events go: an open stretch (by depth) or the loose
/// stretch (`None`), and the slot there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Site {
    depth: Option<usize>,
    slot: usize,
}

fn state() -> &'static Mutex<Option<State>> {
    use std::sync::OnceLock;
    static S: OnceLock<Mutex<Option<State>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}

fn with(f: impl FnOnce(&mut State)) {
    if let Some(s) = state().lock().expect("bounds sink").as_mut() {
        f(s);
    }
}

pub(crate) fn start() {
    *state().lock().expect("bounds sink") = Some(State::default());
}

/// A new emission pass begins: the previous one's witness is complete.
pub(crate) fn begin_pass(pass: usize) {
    with(|s| {
        s.flush();
        s.pass = pass;
    });
}

impl State {
    fn close_one(&mut self) {
        if let Some(st) = self.open.pop()
            && !st.text.is_empty()
        {
            self.stretches.push(st.text.trim_end().to_string());
            self.stretch_anchors.push(st.anchors.trim_end().to_string());
        }
    }

    fn flush(&mut self) {
        while !self.open.is_empty() {
            self.close_one();
        }
        let loose = std::mem::take(&mut self.loose);
        if !loose.text.is_empty() {
            self.stretches.push(loose.text.trim_end().to_string());
            self.stretch_anchors.push(loose.anchors.trim_end().to_string());
        }
        if std::mem::take(&mut self.broken) {
            self.stretches.push("x".to_string());
            self.stretch_anchors.push("-".to_string());
        }
        let judges = std::mem::take(&mut self.judges);
        if !judges.is_empty() {
            self.done_judges.push((self.pass, judges.trim_end().to_string()));
        }
        let holds = std::mem::take(&mut self.holds);
        if !holds.is_empty() {
            self.done_holds.push((self.pass, holds.trim_end().to_string()));
        }
        let stretches = std::mem::take(&mut self.stretches);
        let anchors = std::mem::take(&mut self.stretch_anchors);
        if !stretches.is_empty() {
            self.done.push((self.pass, stretches.join(";")));
            self.done_anchors.push((self.pass, anchors.join(";")));
        }
    }

    fn site(&mut self, k: Option<Key>) -> Site {
        match (k, self.open.len()) {
            (Some(k), n) if n > 0 => {
                let st = &mut self.open[n - 1];
                let slot = st.slots.iter().position(|&x| x == k).unwrap_or_else(|| {
                    st.slots.push(k);
                    st.slots.len() - 1
                });
                Site { depth: Some(n - 1), slot }
            }
            _ => {
                // A fresh slot: no key, so no kill names it and no earlier
                // check can decide it.
                self.loose.slots.push((u32::MAX, u32::MAX, 0));
                Site { depth: None, slot: self.loose.slots.len() - 1 }
            }
        }
    }

    fn text_of(&mut self, site: Site) -> Option<&mut Stretch> {
        match site.depth {
            None => Some(&mut self.loose),
            Some(d) if d < self.open.len() => Some(&mut self.open[d]),
            Some(_) => {
                self.broken = true;
                None
            }
        }
    }

    #[cfg(test)]
    fn push(&mut self, site: Site, tag: char) {
        self.push_at(site, tag, "-");
    }

    fn push_at(&mut self, site: Site, tag: char, at: &str) {
        if let Some(t) = self.text_of(site) {
            t.text.push_str(&format!("{tag}{} ", site.slot));
            t.anchors.push_str(&format!("{at} "));
        }
    }

    fn kill(&mut self, v: u32) {
        for st in &mut self.open {
            for (n, &(xs, i, _)) in st.slots.iter().enumerate() {
                if xs == v || i == v {
                    st.text.push_str(&format!("s{n} "));
                    st.anchors.push_str("- ");
                }
            }
        }
    }
}

/// The emitter starts a fact set: a new stretch, knowing nothing.
pub(crate) fn open() {
    with(|s| s.open.push(Stretch::default()));
}

/// The emitter drops its innermost fact set.
pub(crate) fn close() {
    with(State::close_one);
}

/// The slot an access to key `k` (or, `None`, a non-candidate access) is
/// recorded under; `None` when no sweep collects.
pub(crate) fn site(k: Option<Key>) -> Option<Site> {
    let mut g = state().lock().expect("bounds sink");
    g.as_mut().map(|s| s.site(k))
}

/// The compare guarding `site`'s access was just emitted.
pub(crate) fn check(site: Option<Site>, at: &wasm_encoder::Function) {
    if let Some(site) = site {
        let a = crate::witness::anchor::at(at);
        with(|s| s.push_at(site, 'c', &a));
    }
}

/// `site`'s element was just loaded (`write` false) or stored.
pub(crate) fn access(site: Option<Site>, write: bool, at: &wasm_encoder::Function) {
    if let Some(site) = site {
        let a = crate::witness::anchor::at(at);
        with(|s| s.push_at(site, if write { 'w' } else { 'a' }, &a));
    }
}

/// A copy-on-write judge call (one argument, the block; it returns a block
/// of the same count) was just emitted.
pub(crate) fn judge(at: &wasm_encoder::Function) {
    let a = crate::witness::anchor::at(at);
    with(|s| {
        s.judges.push_str(&a);
        s.judges.push(' ');
    });
}

/// A list `for` loop's header (#4280): a stretch of its own with one fresh
/// slot, for the exit test and the element read it guards. The caller
/// records [`exit`] and [`access`] on the returned site, then [`close`]s.
pub(crate) fn loop_header() -> Option<Site> {
    let mut g = state().lock().expect("bounds sink");
    g.as_mut().map(|s| {
        s.open.push(Stretch::default());
        let n = s.open.len();
        // No variable is u32::MAX, so no kill names the slot.
        s.open[n - 1].slots.push((u32::MAX, u32::MAX, 0));
        Site { depth: Some(n - 1), slot: 0 }
    })
}

/// The loop exit test guarding `site`'s access (its `br_if`) was just
/// emitted.
pub(crate) fn exit(site: Option<Site>, at: &wasm_encoder::Function) {
    if let Some(site) = site {
        let a = crate::witness::anchor::at(at);
        with(|s| s.push_at(site, 'e', &a));
    }
}

/// A list `for` loop's +1 on its subject (the `call` of rc_inc) was just
/// emitted: while the loop runs the block has a second holder, so no
/// routine writes or resizes it in place (the copy-on-write contract).
pub(crate) fn hold(at: &wasm_encoder::Function) {
    let a = crate::witness::anchor::at(at);
    with(|s| {
        s.holds.push_str(&a);
        s.holds.push(' ');
    });
}

/// Variable `v` changed: every slot of every open stretch that names it.
pub(crate) fn kill(v: u32) {
    with(|s| s.kill(v));
}

/// Every pass's bounds witness, in emission order.
pub fn take() -> Vec<(usize, String)> {
    let mut g = state().lock().expect("bounds sink");
    let Some(mut s) = g.take() else { return Vec::new() };
    s.flush();
    *taken_anchors().lock().expect("bounds anchors") = std::mem::take(&mut s.done_anchors);
    *taken_judges().lock().expect("bounds judges") = std::mem::take(&mut s.done_judges);
    *taken_holds().lock().expect("bounds holds") = std::mem::take(&mut s.done_holds);
    s.done
}

fn taken_judges() -> &'static Mutex<Vec<(usize, String)>> {
    use std::sync::OnceLock;
    static J: OnceLock<Mutex<Vec<(usize, String)>>> = OnceLock::new();
    J.get_or_init(|| Mutex::new(Vec::new()))
}

/// The judge-call anchors of the passes the last [`take`] flushed.
pub fn take_judges() -> Vec<(usize, String)> {
    std::mem::take(&mut *taken_judges().lock().expect("bounds judges"))
}

fn taken_holds() -> &'static Mutex<Vec<(usize, String)>> {
    use std::sync::OnceLock;
    static H: OnceLock<Mutex<Vec<(usize, String)>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(Vec::new()))
}

/// The loop-hold anchors of the passes the last [`take`] flushed.
pub fn take_holds() -> Vec<(usize, String)> {
    std::mem::take(&mut *taken_holds().lock().expect("bounds holds"))
}

fn taken_anchors() -> &'static Mutex<Vec<(usize, String)>> {
    use std::sync::OnceLock;
    static A: OnceLock<Mutex<Vec<(usize, String)>>> = OnceLock::new();
    A.get_or_init(|| Mutex::new(Vec::new()))
}

/// The anchors of the witness the last [`take`] returned, per pass: per
/// stretch (`;`-joined) one anchor per token, `serial@offset` or `-` (a
/// kill, which no instruction carries).
pub fn take_anchors() -> Vec<(usize, String)> {
    std::mem::take(&mut *taken_anchors().lock().expect("bounds anchors"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // A local state: the global sink is shared with every other test.
    fn run(f: impl FnOnce(&mut State)) -> String {
        let mut s = State::default();
        f(&mut s);
        s.flush();
        s.done.into_iter().map(|(_, w)| w).collect::<Vec<_>>().join("|")
    }

    #[test]
    fn the_flip_records_two_checks_and_four_accesses() {
        let w = run(|s| {
            s.open.push(Stretch::default());
            let (lo, hi) = (s.site(Some((1, 2, 0))), s.site(Some((1, 3, 0))));
            s.push(lo, 'c');
            s.push(lo, 'a');
            s.push(hi, 'c');
            s.push(hi, 'a');
            s.push(lo, 'w');
            s.push(hi, 'w');
            s.close_one();
        });
        assert_eq!(w, "c0 a0 c1 a1 w0 w1");
    }

    #[test]
    fn an_assignment_kills_every_slot_naming_the_variable_in_every_open_stretch() {
        let w = run(|s| {
            s.open.push(Stretch::default());
            let a = s.site(Some((1, 2, 0)));
            s.push(a, 'c');
            s.push(a, 'a');
            s.open.push(Stretch::default());
            let b = s.site(Some((5, 2, 1)));
            s.push(b, 'c');
            s.push(b, 'a');
            s.kill(2);
            s.close_one();
            s.close_one();
        });
        assert_eq!(w, "c0 a0 s0;c0 a0 s0");
    }

    #[test]
    fn an_empty_stretch_leaves_nothing() {
        assert_eq!(
            run(|s| {
                s.open.push(Stretch::default());
                s.close_one();
            }),
            ""
        );
    }

    #[test]
    fn a_non_candidate_access_gets_a_fresh_loose_slot() {
        let w = run(|s| {
            let a = s.site(None);
            s.push(a, 'c');
            s.push(a, 'a');
            let b = s.site(None);
            s.push(b, 'w');
        });
        // The second access's check is missing: the checker must reject.
        assert_eq!(w, "c0 a0 w1");
    }

    #[test]
    fn a_for_loop_header_is_a_stretch_of_its_own() {
        let w = run(|s| {
            s.open.push(Stretch::default());
            let a = s.site(Some((1, 2, 0)));
            s.push(a, 'c');
            s.open.push(Stretch::default());
            s.open[1].slots.push((u32::MAX, u32::MAX, 0));
            let h = Site { depth: Some(1), slot: 0 };
            s.push(h, 'e');
            s.push(h, 'a');
            s.kill(2);
            s.close_one();
            s.push(a, 'a');
            s.close_one();
        });
        assert_eq!(w, "e0 a0;c0 s0 a0");
    }

    #[test]
    fn a_site_that_outlives_its_stretch_breaks_the_witness() {
        let w = run(|s| {
            s.open.push(Stretch::default());
            let a = s.site(Some((1, 2, 0)));
            s.close_one();
            s.push(a, 'a');
        });
        assert_eq!(w, "x");
    }
}
