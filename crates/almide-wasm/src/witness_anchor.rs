//! Where in the BYTES a recorded event is (#4011 (d)): the anchor half of
//! the translation check that the bounds and init witnesses' events are the
//! instructions the shipped module contains.
//!
//! The sinks (witness_bounds.rs, witness_init.rs) record each event next to
//! the instruction it describes. Beside the event they now keep its ANCHOR:
//! the function being emitted (a serial number, `begin_fn` at its
//! `Function::new`) and the byte length of its body right after the
//! instruction — the end offset of the instruction the event describes.
//! `seal` stamps a finished body with a digest of its bytes; the bundle
//! producer (witness_bundle.rs `anchor_text`) finds that body in the emitted
//! module by the digest and turns `serial@offset` into `position:ordinal` —
//! the defined function's position and the instruction's index in it, which
//! `to_wasi` keeps (it re-encodes one operator for one operator).
//!
//! An anchor is a LOCATOR, not evidence: `almide-anchor-check` decodes the
//! shipped bytes itself and refuses an event whose located instruction is not
//! the one the event names, so a wrong locator can only reject.
//!
//! A body that never seals (a refused frame, a dead registry body the pass
//! replaced with a stub) leaves its events unlocated (`-`); the checker counts
//! them and they are not evidence about the artifact.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
struct State {
    serial: u64,
    /// The bodies being emitted, innermost last (a helper body may be
    /// emitted while another is open).
    open: Vec<u64>,
    sealed: HashMap<u64, u64>,
}

fn state() -> &'static Mutex<Option<State>> {
    use std::sync::OnceLock;
    static S: OnceLock<Mutex<Option<State>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}

pub(crate) fn start() {
    *state().lock().expect("anchor sink") = Some(State::default());
}

/// A function body begins: events from here on are anchored in it, until
/// it seals. Returns its serial (0 when no sweep collects).
pub(crate) fn begin_fn() -> u64 {
    match state().lock().expect("anchor sink").as_mut() {
        Some(s) => {
            s.serial += 1;
            s.open.push(s.serial);
            s.serial
        }
        None => 0,
    }
}

/// The anchor of the instruction just emitted into `f`: `serial@offset`, or
/// `-` when no body is open.
pub(crate) fn at(f: &wasm_encoder::Function) -> String {
    match state()
        .lock()
        .expect("anchor sink")
        .as_ref()
        .and_then(|s| s.open.last().copied())
    {
        Some(k) => format!("{k}@{}", f.byte_len()),
        None => "-".to_string(),
    }
}

/// The digest of a body's bytes (without the size prefix).
pub fn body_digest(raw: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    raw.hash(&mut h);
    h.finish()
}

/// Body `k` is finished: `f` is its final form. A body opened after it and
/// never sealed (a refused frame) is closed with it, unlocated.
pub(crate) fn seal(k: u64, f: &wasm_encoder::Function) {
    let mut g = state().lock().expect("anchor sink");
    let Some(s) = g.as_mut() else { return };
    let Some(at) = s.open.iter().rposition(|&x| x == k) else {
        return;
    };
    s.open.truncate(at);
    let mut v = Vec::new();
    wasm_encoder::Encode::encode(f, &mut v);
    // Strip the LEB128 size prefix `encode` writes.
    let skip = v.iter().position(|b| b & 0x80 == 0).map_or(0, |i| i + 1);
    s.sealed.insert(k, body_digest(&v[skip..]));
}

/// The sealed bodies, `serial -> digest`.
pub fn take() -> HashMap<u64, u64> {
    state()
        .lock()
        .expect("anchor sink")
        .take()
        .map(|s| s.sealed)
        .unwrap_or_default()
}
