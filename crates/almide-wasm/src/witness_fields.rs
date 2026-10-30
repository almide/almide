//! The structural leg's FIELDS witness (#4013, #4279) — record, tuple and
//! variant construction sites and field accesses, judged by
//! `FieldCert.check_fields_cert` (proofs/FieldCert.v, `almide-verify
//! fields`). `;`-separated items:
//!
//!   `L R c C n N z S o O w W ...`   the layout of kind (R, C)
//!   `k R c C s I ...`               a construction site, then its stores
//!   `a R c C f I`                   a load or store of field I
//!   `t R`                           a read of a variant's tag word
//!
//! R names the type: `3 * index` for a record, `3 * index + 1` for a
//! variant, `3 * index + 2` for a tuple (types_table.rs indices); C is 0 for
//! a record or tuple and the case's tag for a variant; N is the case count;
//! S the payload bytes and (O, W) each field's payload offset and width.
//! A variant case's field 0 is its tag word (0, 4). Every kind an item names
//! is laid out once per pass, from the type table — the one source the
//! emitted offsets come from.
//!
//! Each event is recorded where its instruction is emitted — the site at
//! its block's `local.set` / `local.tee`, a store right after it, an access
//! right after its load or store — and keeps its ANCHOR (witness_anchor.rs),
//! which almide-anchor-check holds to the shipped bytes: the allocation's
//! size, each store's width and offset against the layout, each access's,
//! and a variant case field's read against the tag test that guards it.
//!
//! COVERAGE: record, tuple and variant literals and constructors, member and
//! positional reads, the slot a dying read takes, pattern tests and binds,
//! and spread overwrites. The runtime's own routines over these blocks (a
//! typed drop, copy, equality, comparison or display body) record nothing
//! yet (#4278).

use std::collections::BTreeMap;
use std::sync::Mutex;

/// A site's place in the pass's item list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Site(usize);

/// A kind: (representation, case).
pub(crate) type Kind = (u32, u32);

/// A kind's layout: the case count, the payload bytes, and each field's
/// payload offset and width.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Layout {
    pub(crate) n: u32,
    pub(crate) z: u32,
    pub(crate) fields: Vec<(u32, u32)>,
}

#[derive(Default)]
struct State {
    pass: usize,
    layouts: BTreeMap<Kind, Layout>,
    /// One kind laid out two ways in a pass: the witness is made
    /// unparseable, so it is rejected rather than read with either.
    clash: bool,
    items: Vec<String>,
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
    state().lock().expect("fields sink").as_mut().map(f)
}

pub(crate) fn start() {
    *state().lock().expect("fields sink") = Some(State::default());
}

/// Whether a sweep collects (the emitter skips building layouts otherwise).
pub(crate) fn armed() -> bool {
    state().lock().expect("fields sink").is_some()
}

impl State {
    fn flush(&mut self) {
        let layouts = std::mem::take(&mut self.layouts);
        let items = std::mem::take(&mut self.items);
        let anchors = std::mem::take(&mut self.anchors);
        let clash = std::mem::take(&mut self.clash);
        if items.is_empty() && layouts.is_empty() && !clash {
            return;
        }
        let mut text: Vec<String> = layouts.iter().map(|(k, l)| layout_text(*k, l)).collect();
        let mut anch: Vec<String> = vec!["-".to_string(); text.len()];
        text.extend(items.iter().map(|s| s.trim_end().to_string()));
        anch.extend(anchors.iter().map(|s| s.trim_end().to_string()));
        if clash {
            text.push("x".to_string());
            anch.push("-".to_string());
        }
        self.done.push((self.pass, text.join(";")));
        self.done_anchors.push((self.pass, anch.join(";")));
    }

    fn declare(&mut self, k: Kind, l: &Layout) {
        match self.layouts.get(&k) {
            Some(old) if old != l => self.clash = true,
            Some(_) => {}
            None => {
                self.layouts.insert(k, l.clone());
            }
        }
    }
}

fn layout_text((r, c): Kind, l: &Layout) -> String {
    let mut s = format!("L{r} c{c} n{} z{}", l.n, l.z);
    for (o, w) in &l.fields {
        s.push_str(&format!(" o{o} w{w}"));
    }
    s
}

/// A new emission pass begins: the previous one's witness is complete.
pub(crate) fn begin_pass(pass: usize) {
    with(|s| {
        s.flush();
        s.pass = pass;
    });
}

/// Lay out kind `k` in this pass.
pub(crate) fn declare(k: Kind, l: &Layout) {
    with(|s| s.declare(k, l));
}

/// The block of a construction site of kind `k` was just set (or teed) into
/// its local, right after its allocation.
pub(crate) fn site(k: Kind, at: &wasm_encoder::Function) -> Option<Site> {
    let a = crate::witness::anchor::at(at);
    with(|s| {
        s.items.push(format!("k{} c{} ", k.0, k.1));
        s.anchors.push(format!("{a} "));
        Site(s.items.len() - 1)
    })
}

/// Field `i` of `site` was just stored.
pub(crate) fn store(site: Option<Site>, i: usize, at: &wasm_encoder::Function) {
    if let Some(Site(k)) = site {
        let a = crate::witness::anchor::at(at);
        with(|s| {
            if let Some(t) = s.items.get_mut(k) {
                t.push_str(&format!("s{i} "));
            }
            if let Some(t) = s.anchors.get_mut(k) {
                t.push_str(&format!("{a} "));
            }
        });
    }
}

/// Field `i` of a block of kind `k` was just loaded or stored.
pub(crate) fn access(k: Kind, i: usize, at: &wasm_encoder::Function) {
    let a = crate::witness::anchor::at(at);
    with(|s| {
        s.items.push(format!("a{} c{} f{i}", k.0, k.1));
        s.anchors.push(a);
    });
}

/// The tag word of a block of variant representation `r` was just loaded.
pub(crate) fn tag(r: u32, at: &wasm_encoder::Function) {
    let a = crate::witness::anchor::at(at);
    with(|s| {
        s.items.push(format!("t{r}"));
        s.anchors.push(a);
    });
}

/// Every pass's fields witness, in emission order.
pub fn take() -> Vec<(usize, String)> {
    let mut g = state().lock().expect("fields sink");
    let Some(mut s) = g.take() else {
        return Vec::new();
    };
    s.flush();
    *taken_anchors().lock().expect("fields anchors") = std::mem::take(&mut s.done_anchors);
    s.done
}

fn taken_anchors() -> &'static Mutex<Vec<(usize, String)>> {
    use std::sync::OnceLock;
    static A: OnceLock<Mutex<Vec<(usize, String)>>> = OnceLock::new();
    A.get_or_init(|| Mutex::new(Vec::new()))
}

/// The anchors of the witness the last [`take`] returned, per pass: per item
/// (`;`-joined) one anchor per event, `serial@offset` or `-`.
pub fn take_anchors() -> Vec<(usize, String)> {
    std::mem::take(&mut *taken_anchors().lock().expect("fields anchors"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_lead_and_items_follow_in_emission_order() {
        let mut s = State::default();
        let l = Layout { n: 1, z: 16, fields: vec![(0, 8), (8, 4)] };
        s.declare((3, 0), &l);
        s.items.push("k3 c0 s0 s1 ".into());
        s.anchors.push("1@4 1@9 1@14 ".into());
        s.items.push("a3 c0 f1".into());
        s.anchors.push("1@20".into());
        s.flush();
        assert_eq!(s.done, vec![(0, "L3 c0 n1 z16 o0 w8 o8 w4;k3 c0 s0 s1;a3 c0 f1".to_string())]);
        assert_eq!(s.done_anchors, vec![(0, "-;1@4 1@9 1@14;1@20".to_string())]);
    }

    #[test]
    fn a_kind_laid_out_two_ways_breaks_the_witness() {
        let mut s = State::default();
        s.declare((3, 0), &Layout { n: 1, z: 8, fields: vec![(0, 8)] });
        s.declare((3, 0), &Layout { n: 1, z: 16, fields: vec![(0, 8)] });
        s.flush();
        assert!(s.done[0].1.ends_with(";x"));
    }
}
