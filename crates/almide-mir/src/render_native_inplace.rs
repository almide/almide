//! A runtime call whose result goes straight into a slot the function
//! already holds, made in that slot (#4661).
//!
//! EDITED. `counts[k] = (map.get(counts, k) ?? 0) + 1` reaches this render
//! as `counts = map.upsert(counts, k, 1, f)`: the call takes the map by value
//! and returns it, and the next op rebinds the slot to the result. Rendered
//! literally the map's header moves into the call and back out on every
//! update — 1.5x the standard codegen's in-place `upsert_with` on a counting
//! loop, all of it those two copies. Where the runtime carries an in-place
//! twin of the body the call renders through
//! ([`crate::native_floor::in_place_twin`]: the first param `&mut`, nothing
//! returned), the site calls the twin on the slot. In the MIR:
//!
//! ```text
//! d = call(s, ..)      // s a rebound slot, handed over (moved or taken)
//! Drop s               // releases only
//! SetLocal s <- d      // d read nowhere else
//! ```
//!
//! REFILLED. `bytes.clear(buf)` reaches it as `buf = bytes.zeros(0)`: a
//! buffer built afresh, the old one released. A line buffer cleared per line
//! was freed and regrown per line (1.13x on a line-writing loop). Where the
//! runtime carries a refilling twin of the building body
//! ([`crate::native_floor::refill_twin`]: a `&mut` of the result type ahead
//! of the body's params, leaving there what the body returns), the site
//! refills the slot. In the MIR the same three ops, the call not naming `s`
//! and the release required: it is what says the slot still holds its old
//! value (handed on earlier, the value has no release left and the slot is
//! empty). The slot is then read at the rebind, which the move analysis is
//! told ([`refills`]): a slot refilled later is never left moved-out.
//!
//! Either way the rendered text must be the one statement `let mut d: T =
//! body(..);` whose other arguments do not name `s` (the twin holds `s`
//! exclusively for the call), and the rebind then renders nothing. Any other
//! shape keeps the call as it is. The MIR and its certificate are unchanged:
//! the slot holds one owned value before and after, which is what the three
//! ops together say.

use crate::render_native::{var, NTy};
use crate::{CallArg, MirFunction, Op, ValueId};
use std::collections::{BTreeMap, BTreeSet};

/// How a site's call reaches its slot.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    /// The call takes the slot's value and returns it.
    Edited,
    /// The call builds a value afresh.
    Refilled,
}

/// A candidate site: its kind, the slot, the result, the rebind's op index.
type Site = (Kind, ValueId, ValueId, usize);

thread_local! {
    /// The candidate sites of the function being rendered, by the call's op index.
    static SITES: std::cell::RefCell<BTreeMap<usize, Site>> = const { std::cell::RefCell::new(BTreeMap::new()) };
    /// The rebinds whose call was rendered in the slot: they render nothing.
    static DONE: std::cell::RefCell<BTreeSet<usize>> = const { std::cell::RefCell::new(BTreeSet::new()) };
}

/// Find the candidate sites of `func`.
pub(crate) fn begin(func: &MirFunction) {
    let sites = if crate::render_native_move::drill_no_moves() { BTreeMap::new() } else { sites(func) };
    SITES.with(|s| *s.borrow_mut() = sites);
    DONE.with(|d| d.borrow_mut().clear());
}

/// The site whose rebind is `ops[j]`: the call's op index and the site.
fn site_at(func: &MirFunction, j: usize) -> Option<(usize, Site)> {
    let Op::SetLocal { local: s, src: d } = func.ops.get(j)? else { return None };
    // Only releases of the slot stand between the call and the rebind.
    let i = (0..j).rev().find(|&k| !crate::native_move_params::is_release(&func.ops[k], *s))?;
    let Op::CallFn { dst: Some(made), args, .. } = &func.ops[i] else { return None };
    let mut vs = Vec::new();
    crate::mir_ops::op_values(&func.ops[i], &mut vs);
    let kind = match (vs.iter().filter(|v| *v == s).count(), args.first()) {
        // The slot's old value is released here, so the slot still holds
        // it: with no release it was handed on before, and the slot is empty.
        (0, _) if i + 1 < j => Kind::Refilled,
        (0, _) => return None,
        (1, Some(CallArg::Handle(first))) if first == s => Kind::Edited,
        _ => return None,
    };
    // The result is read by the rebind alone.
    let reads = func.ops.iter().enumerate().filter(|(k, o)| {
        vs.clear();
        crate::mir_ops::op_values(o, &mut vs);
        *k != i && vs.contains(d)
    });
    (made == d && s != d && func.ret != Some(*d) && reads.count() == 1).then_some((i, (kind, *s, *d, j)))
}

fn sites(func: &MirFunction) -> BTreeMap<usize, Site> {
    (0..func.ops.len()).filter_map(|j| site_at(func, j)).collect()
}

/// The runtime body a call to `name` renders through, on either route.
fn symbol(name: &str) -> Option<&'static str> {
    crate::native_floor::declared(name)
        .map(|f| f.symbol.as_str())
        .or_else(|| crate::native_hof::of(name).map(|h| h.symbol.as_str()))
}

/// The rebind `ops[j]` may be rendered as a refill of its slot, which reads
/// the slot: it is a refilled site whose call has a refilling twin.
pub(crate) fn refills(func: &MirFunction, j: usize) -> bool {
    let Some((i, (Kind::Refilled, ..))) = site_at(func, j) else { return false };
    let Op::CallFn { name, .. } = &func.ops[i] else { return false };
    symbol(name).is_some_and(|s| crate::native_floor::refill_twin(s).is_some())
}

/// `ops[i]` is a rebind whose call was rendered in the slot.
pub(crate) fn done(i: usize) -> bool {
    DONE.with(|d| d.borrow().contains(&i))
}

/// `name` occurs in `text` as a whole identifier.
fn names(text: &str, name: &str) -> bool {
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    text.match_indices(name).any(|(at, _)| {
        !text[..at].chars().next_back().is_some_and(ident) && !text[at + name.len()..].chars().next().is_some_and(ident)
    })
}

/// Rewrite the text `ops[i]` rendered (`out[start..]`) to its in-slot form
/// when `i` is a candidate site and the text is the one statement described
/// in the module doc. The twin's items join `used_shims`.
pub(crate) fn rewrite(i: usize, tys: &BTreeMap<ValueId, NTy>, out: &mut String, start: usize, used_shims: &mut Vec<&'static str>) {
    let Some((kind, s, d, rebind)) = SITES.with(|m| m.borrow().get(&i).copied()) else { return };
    let Some(text) = out.get(start..) else { return };
    let (slot, head) = (var(s), format!("let mut {}: ", var(d)));
    // One statement, on the last line; whatever precedes it stays.
    let body = text.trim_end_matches('\n');
    let line_at = body.rfind('\n').map_or(0, |n| n + 1);
    let line = &body[line_at..];
    let indent = &line[..line.len() - line.trim_start().len()];
    let Some(stmt) = line.trim_start().strip_prefix(&head).and_then(|l| l.strip_suffix(");")) else { return };
    let Some((_, call)) = stmt.split_once(" = ") else { return };
    let Some((symbol, args)) = call.split_once('(') else { return };
    // The body the call renders through: the runtime fn, or its static twin.
    let base = symbol.strip_suffix("_static").unwrap_or(symbol);
    let (twin, rest) = match kind {
        Kind::Edited => {
            let moved = [format!("std::mem::take(&mut {slot}), "), format!("{slot}, ")];
            let Some(rest) = moved.iter().find_map(|m| args.strip_prefix(m.as_str())) else { return };
            (crate::native_floor::in_place_twin(base), rest)
        }
        // The slot holds a value of the kind the call builds.
        Kind::Refilled if tys.get(&s).is_some() && tys.get(&s) == tys.get(&d) => (crate::native_floor::refill_twin(base), args),
        Kind::Refilled => return,
    };
    // The slot is the twin's alone for the call.
    let Some((twin, items)) = twin.filter(|_| !names(rest, &slot)) else { return };
    let sep = if rest.is_empty() { "" } else { ", " };
    let edited = format!("{}{indent}{twin}(&mut {slot}{sep}{rest});\n", &body[..line_at]);
    out.truncate(start);
    out.push_str(&edited);
    used_shims.extend(items);
    DONE.with(|m| m.borrow_mut().insert(rebind));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identifier_is_matched_whole() {
        assert!(names("f(v1, &v12)", "v1"));
        assert!(!names("f(v12, v21)", "v1"));
        assert!(names("v1", "v1"));
        assert!(!names("__env_v1", "v1"));
    }

    fn v(n: u32) -> ValueId {
        ValueId(n)
    }

    fn call(dst: u32, args: Vec<CallArg>) -> Op {
        Op::CallFn { dst: Some(v(dst)), name: "m.f".into(), args, result: None }
    }

    #[test]
    fn a_site_is_a_call_released_and_rebound_into_one_slot() {
        let rebind = Op::SetLocal { local: v(1), src: v(2) };
        let f = |ops: Vec<Op>| MirFunction { name: "f".into(), ops, ..Default::default() };
        // The call takes the slot first: edited.
        let edited = f(vec![call(2, vec![CallArg::Handle(v(1)), CallArg::Imm(3)]), Op::Drop { v: v(1) }, rebind.clone()]);
        assert_eq!(site_at(&edited, 2), Some((0, (Kind::Edited, v(1), v(2), 2))));
        // The call does not name the slot: refilled.
        let refilled = f(vec![call(2, vec![CallArg::Imm(0)]), Op::Drop { v: v(1) }, rebind.clone()]);
        assert_eq!(site_at(&refilled, 2), Some((0, (Kind::Refilled, v(1), v(2), 2))));
        // The slot is not the first argument, or named twice.
        let second = f(vec![call(2, vec![CallArg::Imm(0), CallArg::Handle(v(1))]), rebind.clone()]);
        assert_eq!(site_at(&second, 1), None);
        let twice = f(vec![call(2, vec![CallArg::Handle(v(1)), CallArg::Handle(v(1))]), rebind.clone()]);
        assert_eq!(site_at(&twice, 1), None);
        // No release: the slot's value was handed on, there is nothing to refill.
        let handed_on = f(vec![call(2, vec![CallArg::Imm(0)]), rebind.clone()]);
        assert_eq!(site_at(&handed_on, 1), None);
        // The result is read again, or another op stands before the rebind.
        let read = f(vec![call(2, vec![CallArg::Imm(0)]), rebind.clone(), Op::Dup { dst: v(4), src: v(2) }]);
        assert_eq!(site_at(&read, 1), None);
        let between = f(vec![call(2, vec![CallArg::Imm(0)]), Op::ConstInt { dst: v(5), value: 1 }, rebind]);
        assert_eq!(site_at(&between, 2), None);
    }
}
