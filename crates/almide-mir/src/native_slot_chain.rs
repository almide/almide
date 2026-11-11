//! Consecutive writes of one module-level buffer keep it in a local.
//!
//! Each `bytes.put_*(g, …)` on a module-level `var g` lowers to a take of
//! the slot, the write, and the store back. A function that writes twenty
//! fields in a row (an immediate-mode renderer's `push`: twenty
//! `bytes.put_f32_le` per quad) then went through the slot's thread-local
//! cell forty times per quad, where a local buffer is written in place —
//! several times a local write per element, and more on a machine whose
//! thread-local access is slow.
//!
//! When a store of the slot is followed by a take of the same slot with
//! nothing between them but integer constants, the two cancel: the stored
//! value is the taken one.
//!
//! ```text
//! ALMD_HSLOT_0.with(|s| *s.borrow_mut() = v40);
//! let mut v44: i64 = 8192i64;
//! let mut v45: Bytes = ALMD_HSLOT_0.with(|s| std::mem::take(&mut *s.borrow_mut()));
//!   ⇒
//! let mut v44: i64 = 8192i64;
//! let mut v45: Bytes = v40;
//! ```
//!
//! Nothing runs between the two, so nothing can read the slot there; the
//! run's first take and last store are kept, so the slot holds the same
//! value before the run and after it. A slot shared with a snapshot is the
//! same: the first take copies out of the shared block as before and the
//! last store gives the slot a block of its own.

use crate::render_native_loops::{is_note, let_parts};

/// `ALMD_HSLOT_<k>.with(|s| <body>);` → (pad, k, body).
fn slot_call(line: &str) -> Option<(&str, &str, &str)> {
    let at = line.find("ALMD_HSLOT_")?;
    let (k, rest) = line[at + 11..].split_once(".with(|s| ")?;
    (!k.is_empty() && k.bytes().all(|b| b.is_ascii_digit())).then_some((&line[..at], k, rest.strip_suffix(");")?))
}

/// The stored variable of a store of slot `k`, and whether the slot is held
/// in an `Rc` (the two spellings of `render_native_heap_slot`).
fn store_of(line: &str) -> Option<(&str, &str, &str, bool)> {
    let (pad, k, body) = slot_call(line)?;
    if !pad.chars().all(|c| c == ' ') {
        return None;
    }
    [true, false].into_iter().find_map(|snap| {
        let v = if snap { body.strip_prefix("{ let mut c = s.borrow_mut(); match std::rc::Rc::get_mut(&mut c) { Some(v) => *v = ")?.split_once(", None")?.0 } else { body.strip_prefix("*s.borrow_mut() = ")? };
        (crate::render_native_loops::is_var(v) && body == crate::render_native_addr::store_text(snap, v)).then_some((pad, k, v, snap))
    })
}

/// Cancel each store of a slot against the take of it that follows.
pub(crate) fn chain(out: &mut String) {
    if !out.contains("ALMD_HSLOT_") {
        return;
    }
    let mut lines: Vec<String> = out.lines().map(str::to_string).collect();
    let mut i = 0;
    let mut changed = false;
    while i < lines.len() {
        let next = store_of(&lines[i]).and_then(|(pad, k, v, snap)| {
            let between = |l: &String| is_note(l) || let_parts(l).is_some_and(|(_, ty, rhs)| ty == "i64" && rhs.strip_suffix("i64").is_some_and(|n| n.trim_start_matches('-').bytes().all(|b| b.is_ascii_digit())));
            let j = (i + 1..lines.len()).find(|&j| !between(&lines[j]))?;
            let (d, ty, rhs) = let_parts(&lines[j])?;
            let take = format!("ALMD_HSLOT_{k}.with(|s| {})", crate::render_native_addr::take_text(snap));
            (rhs == take && lines[j].starts_with(pad) && !lines[j][pad.len()..].starts_with(' ')).then(|| (j, format!("{pad}let mut {d}: {ty} = {v};")))
        });
        match next {
            Some((j, moved)) => {
                lines[j] = moved;
                lines.remove(i);
                changed = true;
            }
            None => i += 1,
        }
    }
    if changed {
        *out = lines.join("\n");
        out.push('\n');
    }
}
