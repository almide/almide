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
//! COVERAGE: list literals (`[a, b, c]`, emitter.rs). Every other allocation
//! site — records, variants, strings, the runtime's routines — records
//! nothing yet, so INITIALIZATION is certified for list-literal blocks only;
//! the corpus count of recorded sites is reported by proofs/structural-wall.sh.
//!
//! TRUSTED, as for the other witnesses: that the recorded events are the
//! instructions the bytes contain, and that the block stays in the
//! emitter's private local until its last store (no element expression can
//! name the block being built).

use std::sync::Mutex;

/// A site's place in the pass's site list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Site(usize);

#[derive(Default)]
struct State {
    pass: usize,
    sites: Vec<String>,
    done: Vec<(usize, String)>,
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
        if !sites.is_empty() {
            self.done.push((self.pass, sites.iter().map(|s| s.trim_end()).collect::<Vec<_>>().join(";")));
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
pub(crate) fn site(hdr: u32, width: u32, count: usize, repr: u32) -> Option<Site> {
    with(|s| {
        s.sites.push(format!("h{hdr} w{width} n{count} r{repr} "));
        Site(s.sites.len() - 1)
    })
}

/// Slot `i` of `site` was just stored.
pub(crate) fn store(site: Option<Site>, i: usize) {
    if let Some(Site(k)) = site {
        with(|s| {
            if let Some(t) = s.sites.get_mut(k) {
                t.push_str(&format!("s{i} "));
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
    s.done
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
