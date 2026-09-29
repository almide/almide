//! The structural leg's CALL-MODE witness (#2758, #1696 step 4) — the
//! program-level companion of the per-frame ownership certificates.
//!
//! Per-frame certificates compose only if every call site hands each heap
//! argument over the way its callee's frame assumed: a site that moved a
//! credit (`rc_arg_guard`, `im` / `am`) into a param the callee only borrows
//! leaks it, and one that lent a value to a param the callee releases frees
//! it twice — while both frames balance on their own. The witness is the
//! `<signatures>|<sites>` stream `CallModes.check_modes_cert` judges
//! (proofs/CallModes.v, `almide-verify` `call-modes`): 0 = borrow, 1 = move.
//!
//! - A SIGNATURE is recorded where a table fn's frame is lowered (func.rs):
//!   the convention its exit plan releases by (`param_owned`), one mode per
//!   droppable param.
//! - A SITE is recorded where a Named or linked call lowers its arguments
//!   (calls.rs): the mode each droppable argument was actually handed over
//!   with — the `rc_arg_guard` route is a move, the lend / park route a
//!   borrow, a loop-back or accumulator window that spends the argument a
//!   move. Nested calls in argument position record their own sites (a stack).
//!
//! Both sides are keyed by the callee's wasm function index. A callee with
//! no recorded signature (a refused body shipped as a stub) gets the
//! out-of-range index, which the checker rejects — never a silent skip.
//! Closure calls are out of scope: every lambda frame is callee-owned and
//! every `call_indirect` site hands over owned (calls.rs), by one rule.

use std::collections::BTreeMap;
use std::sync::Mutex;

#[derive(Default)]
struct Pass {
    sigs: BTreeMap<u32, Vec<u8>>,
    sites: Vec<(u32, Vec<u8>)>,
    open: Vec<Vec<u8>>,
}

#[derive(Default)]
struct State {
    pass: usize,
    cur: Pass,
    done: Vec<(usize, String)>,
}

fn state() -> &'static Mutex<Option<State>> {
    use std::sync::OnceLock;
    static S: OnceLock<Mutex<Option<State>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}

fn with(f: impl FnOnce(&mut State)) {
    if let Some(s) = state().lock().expect("modes sink").as_mut() {
        f(s);
    }
}

pub(crate) fn start() {
    *state().lock().expect("modes sink") = Some(State::default());
}

/// A new emission pass begins: the previous one's witness is complete.
pub(crate) fn begin_pass(pass: usize) {
    with(|s| {
        flush(s);
        s.pass = pass;
    });
}

fn flush(s: &mut State) {
    let p = std::mem::take(&mut s.cur);
    if !p.sigs.is_empty() || !p.sites.is_empty() {
        s.done.push((s.pass, render(&p)));
    }
}

fn render(p: &Pass) -> String {
    let index: BTreeMap<u32, usize> = p.sigs.keys().enumerate().map(|(i, &k)| (k, i)).collect();
    let modes = |m: &[u8]| m.iter().map(u8::to_string).collect::<Vec<_>>().join(" ");
    let sigs: Vec<String> = p.sigs.values().map(|m| modes(m)).collect();
    let sites: Vec<String> = p
        .sites
        .iter()
        .map(|(callee, m)| {
            let i = index.get(callee).copied().unwrap_or(p.sigs.len());
            if m.is_empty() { i.to_string() } else { format!("{i} {}", modes(m)) }
        })
        .collect();
    format!("{}|{}", sigs.join(";"), sites.join(";"))
}

/// A table fn's frame convention. Two lowerings of one index in a pass
/// that disagree record an ill-formed mode (2): the checker rejects it.
pub(crate) fn signature(index: u32, modes: Vec<u8>) {
    with(|s| {
        let e = s.cur.sigs.entry(index).or_insert_with(|| modes.clone());
        if *e != modes {
            e.push(2);
        }
    });
}

/// A call site starts lowering its arguments; returns the stack depth.
pub(crate) fn site_begin() -> usize {
    let mut d = 0;
    with(|s| {
        d = s.cur.open.len();
        s.cur.open.push(Vec::new());
    });
    d
}

/// One droppable argument of the innermost open site.
pub(crate) fn site_arg(moved: bool) {
    with(|s| {
        if let Some(top) = s.cur.open.last_mut() {
            top.push(u8::from(moved));
        }
    });
}

/// The site at `depth` closes on `callee` (an early error path's deeper
/// sites are dropped with it).
pub(crate) fn site_end(depth: usize, callee: u32) {
    with(|s| {
        if s.cur.open.len() > depth {
            s.cur.open.truncate(depth + 1);
            let m = s.cur.open.pop().unwrap_or_default();
            s.cur.sites.push((callee, m));
        }
    });
}

/// Every pass's call-mode witness, in emission order.
pub fn take() -> Vec<(usize, String)> {
    let mut g = state().lock().expect("modes sink");
    let Some(mut s) = g.take() else { return Vec::new() };
    flush(&mut s);
    s.done
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pass(sigs: &[(u32, &[u8])], sites: &[(u32, &[u8])]) -> String {
        let mut p = Pass::default();
        p.sigs.extend(sigs.iter().map(|&(k, m)| (k, m.to_vec())));
        p.sites.extend(sites.iter().map(|&(k, m)| (k, m.to_vec())));
        render(&p)
    }

    #[test]
    fn signatures_are_indexed_in_wasm_index_order_and_sites_name_them() {
        // fn 7 (owned, borrowed) and fn 3 (no heap param): 3 renders first.
        assert_eq!(pass(&[(7, &[1, 0]), (3, &[])], &[(7, &[1, 0]), (3, &[])]), ";1 0|1 1 0;0");
    }

    #[test]
    fn a_callee_without_a_signature_is_out_of_range() {
        // A stub body's index: the checker rejects the site, never skips it.
        assert_eq!(pass(&[(1, &[1])], &[(9, &[1])]), "1|1 1");
    }
}
