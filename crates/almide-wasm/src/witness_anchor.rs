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
    /// A sealed body rewritten after sealing (the self-tail-call loop
    /// conversion, tco.rs): its recorded end offsets, mapped to the end
    /// offsets of the instructions they became.
    remaps: HashMap<u64, HashMap<usize, usize>>,
    /// The emission pass the bodies being sealed belong to.
    pass: usize,
    /// Per sealed body of a source function, its pass and the function's
    /// witness name: the bundle's `coverage` record (witness_bundle.rs).
    names: HashMap<u64, (usize, String)>,
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

/// An emission pass begins (witness_frames.rs `mark_pass`).
pub(crate) fn begin_pass(pass: usize) {
    if let Some(s) = state().lock().expect("anchor sink").as_mut() {
        s.pass = pass;
    }
}

/// Body `k`, sealed, is source function `name` of the current pass.
pub(crate) fn name(k: u64, name: &str) {
    if let Some(s) = state().lock().expect("anchor sink").as_mut() {
        let pass = s.pass;
        s.names.insert(k, (pass, name.to_string()));
    }
}

/// The anchor of the instruction just emitted into `f`: `serial@offset`, or
/// `-` when no body is open.
pub(crate) fn at(f: &wasm_encoder::Function) -> String {
    at_back(f, 0)
}

/// As [`at`], for the instruction that ended `back` bytes before the body's
/// current end.
pub(crate) fn at_back(f: &wasm_encoder::Function, back: usize) -> String {
    match state()
        .lock()
        .expect("anchor sink")
        .as_ref()
        .and_then(|s| s.open.last().copied())
    {
        Some(k) => format!("{k}@{}", f.byte_len() - back),
        None => "-".to_string(),
    }
}

/// The serial of the body being emitted, if a sweep collects (#4277).
pub(crate) fn current() -> Option<u64> {
    state().lock().expect("anchor sink").as_ref().and_then(|s| s.open.last().copied())
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

fn encoded(f: &wasm_encoder::Function) -> Vec<u8> {
    let mut v = Vec::new();
    wasm_encoder::Encode::encode(f, &mut v);
    let skip = v.iter().position(|b| b & 0x80 == 0).map_or(0, |i| i + 1);
    v.split_off(skip)
}

/// The offset of an encoded body's first instruction (after its locals).
fn first_offset(raw: &[u8]) -> Option<usize> {
    let body = wasmparser::FunctionBody::new(wasmparser::BinaryReader::new(raw, 0));
    Some(body.get_operators_reader().ok()?.original_position() as usize)
}

/// The end offset of each instruction of an encoded body (locals skipped).
fn ends(raw: &[u8]) -> Option<Vec<usize>> {
    let body = wasmparser::FunctionBody::new(wasmparser::BinaryReader::new(raw, 0));
    let mut r = body.get_operators_reader().ok()?;
    let mut out = Vec::new();
    while !r.eof() {
        r.read().ok()?;
        out.push(r.original_position() as usize);
    }
    Some(out)
}

/// A sealed body `old` was rewritten into `new` (tco.rs `loop_convert`;
/// `origin[n]` is the instruction of `old` that `new`'s instruction `n` came
/// from). Every body sealed as `old` now names `new`, and each recorded end
/// offset maps to the end of the LAST instruction its instruction became —
/// a rewritten self tail call's `br`, any other instruction itself.
pub(crate) fn retarget(old: &wasm_encoder::Function, new: &wasm_encoder::Function, origin: &[u32]) {
    let mut g = state().lock().expect("anchor sink");
    let Some(s) = g.as_mut() else { return };
    let (ro, rn) = (encoded(old), encoded(new));
    let d = body_digest(&ro);
    let serials: Vec<u64> = s.sealed.iter().filter(|&(_, &v)| v == d).map(|(&k, _)| k).collect();
    if serials.is_empty() {
        return;
    }
    let (Some(eo), Some(en)) = (ends(&ro), ends(&rn)) else { return };
    let mut map = HashMap::new();
    // The body's first offset (after its locals) is no instruction's end,
    // yet a window opened before the first instruction starts there (a
    // `match list.get(..)` scrutinee's arm at the top of the body): it maps
    // to the rewritten body's own start, the `loop` the rewrite put first.
    if let (Some(so), Some(sn)) = (first_offset(&ro), first_offset(&rn)) {
        map.insert(so, sn);
    }
    for (n, &o) in origin.iter().enumerate() {
        if let (Some(&from), Some(&to)) = (eo.get(o as usize), en.get(n)) {
            map.insert(from, to);
        }
    }
    let dn = body_digest(&rn);
    for k in serials {
        s.sealed.insert(k, dn);
        s.remaps.insert(k, map.clone());
    }
}

/// What the sink recorded: the sealed bodies (`serial -> digest`), the
/// remapped offsets of the bodies rewritten after sealing, and the source
/// function each named body is (`serial -> (pass, name)`).
#[derive(Default)]
pub(crate) struct Sealed {
    pub(crate) sealed: HashMap<u64, u64>,
    pub(crate) remaps: HashMap<u64, HashMap<usize, usize>>,
    pub(crate) names: HashMap<u64, (usize, String)>,
}

/// Drain the sink.
pub(crate) fn take_all() -> Sealed {
    state()
        .lock()
        .expect("anchor sink")
        .take()
        .map(|s| Sealed { sealed: s.sealed, remaps: s.remaps, names: s.names })
        .unwrap_or_default()
}

/// The sealed bodies, `serial -> digest`.
pub fn take() -> HashMap<u64, u64> {
    take_all().sealed
}
