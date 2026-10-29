//! The structural leg's INIT witness (#4013) — construction sites, judged by
//! `InitCert.check_init_cert` (proofs/InitCert.v, `almide-verify init`).
//!
//! One SITE per allocation the emitter fills itself before the block can be
//! reached by anything else:
//!
//!   `h H w W n N r R s i1 s i2 ...`
//!
//! a block of an H-byte header (`$alloc` writes rc / len / cap) and N slots
//! of W bytes, representation R, then the slots stored, in emission order.
//! A block whose slot count is a run-time value is a LOOP FILL:
//!
//!   `h H w W v K r R f L t S`
//!
//! the count in wasm local K at the `call $alloc`, then the countdown loop
//! on local L whose cursor advances S bytes per stored slot. The checker
//! accepts it when K = L and S = W — the loop counts down the count the
//! block was sized from, one slot per step (InitCert.v `fill_window`).
//! Sites are joined by `;`. The checker accepts a site when every slot below
//! N is stored and none at or past it; InitCert.v proves that the window of
//! an accepted site leaves its block initialized over its whole extent, with
//! the layout the bounds certificate assumes (`certified_sited_memory_safe`).
//!
//! The two halves are recorded where their instructions are emitted: the
//! site where the `call $alloc` is, each `s i` right after its slot store.
//! An emitter that loses a store therefore leaves a slot unrecorded, which
//! the checker rejects (scripts/witness-reinject.sh init re-injects such a
//! bug and requires the rejection).
//!
//! COVERAGE: list literals (`[a, b, c]`, emitter.rs) and range lists
//! (`a..b`, ranges.rs — a loop fill). Every other allocation
//! site — records, variants, strings, the runtime's routines — records
//! nothing yet, so INITIALIZATION is certified for list-literal blocks only;
//! the corpus count of recorded sites is reported by proofs/structural-wall.sh.
//!
//! Each event also keeps its ANCHOR (witness_anchor.rs): where in the body
//! its instruction ends — the site at its `local.set` of the block, a fill
//! at the `local.get` of the block after the loop, a store at the store.
//! `almide-anchor-check` decodes the shipped bytes and checks every located
//! event against the instructions there (#4011 (d)): the allocation's size,
//! the store's width, offset and address local, the fill loop's shape.
//!
//! TRUSTED: that the representation R is the block's (no instruction
//! carries it), and that `$alloc` writes the header.

use std::sync::Mutex;

/// A site's place in the pass's site list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Site(usize);

#[derive(Default)]
struct State {
    pass: usize,
    sites: Vec<String>,
    /// Each site's anchors, in event order (the site, then each fill or
    /// store), parallel to `sites`.
    anchors: Vec<String>,
    done: Vec<(usize, String)>,
    done_anchors: Vec<(usize, String)>,
}

fn state() -> &'static Mutex<Option<State>> {
    use std::sync::OnceLock;
    static S: OnceLock<Mutex<Option<State>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}

fn with<T>(f: impl FnOnce(&mut State) -> T) -> Option<T> {
    state().lock().expect("init sink").as_mut().map(f)
}

pub(crate) fn start() {
    *state().lock().expect("init sink") = Some(State::default());
}

impl State {
    fn flush(&mut self) {
        let sites = std::mem::take(&mut self.sites);
        let anchors = std::mem::take(&mut self.anchors);
        if !sites.is_empty() {
            self.done.push((self.pass, sites.iter().map(|s| s.trim_end()).collect::<Vec<_>>().join(";")));
            self.done_anchors.push((self.pass, anchors.iter().map(|s| s.trim_end()).collect::<Vec<_>>().join(";")));
        }
    }
}

/// A new emission pass begins: the previous one's witness is complete.
pub(crate) fn begin_pass(pass: usize) {
    with(|s| {
        s.flush();
        s.pass = pass;
    });
}

/// The `call $alloc` of a block of `count` slots of `width` bytes behind an
/// `hdr`-byte header, representation `repr`, was just emitted.
pub(crate) fn site(hdr: u32, width: u32, count: usize, repr: u32, at: &wasm_encoder::Function) -> Option<Site> {
    let a = crate::witness::anchor::at(at);
    with(|s| {
        s.sites.push(format!("h{hdr} w{width} n{count} r{repr} "));
        s.anchors.push(format!("{a} "));
        Site(s.sites.len() - 1)
    })
}

/// The `call $alloc` of a block of a run-time count of `width`-byte slots,
/// the count in wasm local `count_local`, was just emitted.
pub(crate) fn fill_site(hdr: u32, width: u32, count_local: u32, repr: u32, at: &wasm_encoder::Function) -> Option<Site> {
    let a = crate::witness::anchor::at(at);
    with(|s| {
        s.sites.push(format!("h{hdr} w{width} v{count_local} r{repr} "));
        s.anchors.push(format!("{a} "));
        Site(s.sites.len() - 1)
    })
}

/// `site`'s countdown fill loop was just emitted: it counts down local
/// `counter` and advances its cursor `step` bytes per stored slot.
pub(crate) fn fill(site: Option<Site>, counter: u32, step: u32, at: &wasm_encoder::Function) {
    if let Some(Site(k)) = site {
        let a = crate::witness::anchor::at(at);
        with(|s| {
            if let Some(t) = s.sites.get_mut(k) {
                t.push_str(&format!("f{counter} t{step} "));
            }
            if let Some(t) = s.anchors.get_mut(k) {
                t.push_str(&format!("{a} "));
            }
        });
    }
}

/// Slot `i` of `site` was just stored.
pub(crate) fn store(site: Option<Site>, i: usize, at: &wasm_encoder::Function) {
    if let Some(Site(k)) = site {
        let a = crate::witness::anchor::at(at);
        with(|s| {
            if let Some(t) = s.sites.get_mut(k) {
                t.push_str(&format!("s{i} "));
            }
            if let Some(t) = s.anchors.get_mut(k) {
                t.push_str(&format!("{a} "));
            }
        });
    }
}

/// Every pass's init witness, in emission order.
pub fn take() -> Vec<(usize, String)> {
    let mut g = state().lock().expect("init sink");
    let Some(mut s) = g.take() else {
        return Vec::new();
    };
    s.flush();
    *taken_anchors().lock().expect("init anchors") = std::mem::take(&mut s.done_anchors);
    s.done
}

fn taken_anchors() -> &'static Mutex<Vec<(usize, String)>> {
    use std::sync::OnceLock;
    static A: OnceLock<Mutex<Vec<(usize, String)>>> = OnceLock::new();
    A.get_or_init(|| Mutex::new(Vec::new()))
}

/// The anchors of the witness the last [`take`] returned, per pass: per
/// site (`;`-joined) the anchor of each event, `serial@offset` or `-`.
pub fn take_anchors() -> Vec<(usize, String)> {
    std::mem::take(&mut *taken_anchors().lock().expect("init anchors"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_site_lists_its_header_slots_and_stores_and_sites_join_with_semicolons() {
        let mut s = State::default();
        s.sites.push("h12 w8 n2 r3 ".into());
        s.sites[0].push_str("s0 s1 ");
        s.sites.push("h12 w4 n0 r5 ".into());
        s.flush();
        assert_eq!(s.done, vec![(0, "h12 w8 n2 r3 s0 s1;h12 w4 n0 r5".to_string())]);
    }
}
