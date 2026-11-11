//! A consuming runtime call whose result goes straight back into the slot
//! its argument came from, edited in place (#4661).
//!
//! `counts[k] = (map.get(counts, k) ?? 0) + 1` reaches this render as
//! `counts = map.upsert(counts, k, 1, f)`: the call takes the map by value
//! and returns it, and the next op rebinds the slot to the result. Rendered
//! literally the map's header moves into the call and back out on every
//! update — 1.5x the standard codegen's in-place `upsert_with` on a counting
//! loop, all of it those two copies.
//!
//! Where the runtime carries an in-place twin of the body the call renders
//! through ([`crate::native_floor::in_place_twin`]: the first param `&mut`,
//! nothing returned), the site calls the twin on the slot and the rebind
//! renders nothing. The site is, in the MIR:
//!
//! ```text
//! d = call(s, ..)      // s a rebound slot, handed over (moved or taken)
//! Drop s               // releases only
//! SetLocal s <- d      // d read nowhere else
//! ```
//!
//! and in the rendered text one statement `let mut d: T = body(<s moved>,
//! rest);` whose `rest` does not name `s` (the twin holds `s` exclusively
//! for the call). Any other shape keeps the consuming form. The MIR and its
//! certificate are unchanged: the slot holds one owned map before and after,
//! which is what the two ops together say.

use crate::render_native::var;
use crate::{CallArg, MirFunction, Op, ValueId};
use std::collections::{BTreeMap, BTreeSet};

thread_local! {
    /// The candidate sites of the function being rendered: the call's op
    /// index to (the slot, the result, the rebind's op index).
    static SITES: std::cell::RefCell<BTreeMap<usize, (ValueId, ValueId, usize)>> =
        const { std::cell::RefCell::new(BTreeMap::new()) };
    /// The rebinds whose call was rendered in place: they render nothing.
    static DONE: std::cell::RefCell<BTreeSet<usize>> = const { std::cell::RefCell::new(BTreeSet::new()) };
}

/// Find the candidate sites of `func`.
pub(crate) fn begin(func: &MirFunction) {
    let sites = if crate::render_native_move::drill_no_moves() { BTreeMap::new() } else { sites(func) };
    SITES.with(|s| *s.borrow_mut() = sites);
    DONE.with(|d| d.borrow_mut().clear());
}

fn sites(func: &MirFunction) -> BTreeMap<usize, (ValueId, ValueId, usize)> {
    let mut out = BTreeMap::new();
    let mut vs = Vec::new();
    for (i, op) in func.ops.iter().enumerate() {
        let Op::CallFn { dst: Some(d), args, .. } = op else { continue };
        let Some(CallArg::Handle(s)) = args.first() else { continue };
        // The slot once among the arguments, then only its releases up to the rebind.
        vs.clear();
        crate::mir_ops::op_values(op, &mut vs);
        if s == d || vs.iter().filter(|v| *v == s).count() != 1 {
            continue;
        }
        let Some(j) = (i + 1..func.ops.len()).find(|&j| !crate::native_move_params::is_release(&func.ops[j], *s)) else { continue };
        if !matches!(&func.ops[j], Op::SetLocal { local, src } if local == s && src == d) {
            continue;
        }
        // The result is read by the rebind alone.
        let reads = func.ops.iter().enumerate().filter(|(k, o)| {
            vs.clear();
            crate::mir_ops::op_values(o, &mut vs);
            *k != i && vs.contains(d)
        });
        if reads.count() == 1 && func.ret != Some(*d) {
            out.insert(i, (*s, *d, j));
        }
    }
    out
}

/// `ops[i]` is a rebind whose call was rendered in place.
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

/// Rewrite the text `ops[i]` rendered (`out[start..]`) to its in-place form
/// when `i` is a candidate site and the text is the one statement described
/// in the module doc. The twin's items join `used_shims`.
pub(crate) fn rewrite(i: usize, out: &mut String, start: usize, used_shims: &mut Vec<&'static str>) {
    let Some((s, d, rebind)) = SITES.with(|m| m.borrow().get(&i).copied()) else { return };
    let Some(text) = out.get(start..) else { return };
    let (slot, head) = (var(s), format!("let mut {}: ", var(d)));
    // One statement, on the last line; whatever precedes it stays.
    let body = text.trim_end_matches('\n');
    let line_at = body.rfind('\n').map_or(0, |n| n + 1);
    let line = &body[line_at..];
    let indent = &line[..line.len() - line.trim_start().len()];
    let Some(stmt) = line.trim_start().strip_prefix(&head).and_then(|l| l.strip_suffix(");")) else { return };
    let Some((_, call)) = stmt.split_once(" = ") else { return };
    let Some((symbol, rest)) = call.split_once('(') else { return };
    let moved = [format!("std::mem::take(&mut {slot}), "), format!("{slot}, ")];
    let Some(rest) = moved.iter().find_map(|m| rest.strip_prefix(m.as_str())) else { return };
    // The slot is the twin's alone for the call.
    if names(rest, &slot) {
        return;
    }
    // The body the call renders through: the runtime fn, or its static twin.
    let base = symbol.strip_suffix("_static").unwrap_or(symbol);
    let Some((twin, items)) = crate::native_floor::in_place_twin(base) else { return };
    let edited = format!("{}{indent}{twin}(&mut {slot}, {rest});\n", &body[..line_at]);
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
}
