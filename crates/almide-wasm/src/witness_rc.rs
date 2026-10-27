//! Where the ownership witness's RC events are in the bytes (#4277): the
//! producer half of the byte check that each recorded `a` (rc_inc) and `d`
//! (release) of a certified frame is a call in the shipped body, on the
//! value the event names, and that the body holds no other.
//!
//! The recorder logs its events per object and renders them per path, away
//! from the instruction (a frame's exit releases are emitted, then recorded).
//! So this sink does not anchor an event to an offset: it records, per frame
//! body, the LIVE `a` / `d` events the recorder logged — each with the local
//! it names, or `-` for a temporary — and, per pass, the function indices
//! the emitter used as releases (every index `dec_fn_of` handed out: the one
//! place a release routine is chosen). `almide-anchor-check` then matches the
//! events against the calls of the shipped body (anchor-check `rc.rs`).
//!
//! Measurement-gated while #4278 is open: the anchor file carries the `rc`
//! lines only under `ALMIDE_RC_ANCHORS=1`, since releases before an abort and
//! RC inside runtime helpers are not recorded events by design.

use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;

#[derive(Default)]
struct State {
    pass: usize,
    /// Per open body serial, its events so far.
    open: HashMap<u64, String>,
    /// `(pass, serial, frame name, events)` of every finished frame.
    frames: Vec<(usize, u64, String, String)>,
    /// Per pass, the release routines handed out.
    releases: HashMap<usize, BTreeSet<u32>>,
}

fn state() -> &'static Mutex<Option<State>> {
    use std::sync::OnceLock;
    static S: OnceLock<Mutex<Option<State>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}

fn with(f: impl FnOnce(&mut State)) {
    if let Some(s) = state().lock().expect("rc sink").as_mut() {
        f(s);
    }
}

pub(crate) fn start() {
    *state().lock().expect("rc sink") = Some(State::default());
}

pub(crate) fn begin_pass(pass: usize) {
    with(|s| s.pass = pass);
}

/// The recorder logged a live `a` / `d` on `local` (`None`: a temporary).
pub(crate) fn event(kind: char, local: Option<u32>) {
    let Some(serial) = crate::witness::anchor::current() else { return };
    with(|s| {
        let e = s.open.entry(serial).or_default();
        e.push(kind);
        match local {
            Some(l) => e.push_str(&l.to_string()),
            None => e.push('-'),
        }
        e.push(' ');
    });
}

/// The frame whose body is `serial` finished as `name`.
pub(crate) fn frame(serial: u64, name: &str) {
    with(|s| {
        let ev = s.open.remove(&serial).unwrap_or_default();
        s.frames.push((s.pass, serial, name.to_string(), ev.trim_end().to_string()));
    });
}

/// `f` was handed out as a release routine.
pub(crate) fn release_fn(f: u32) {
    with(|s| {
        s.releases.entry(s.pass).or_default().insert(f);
    });
}

/// The shipped pass's frames `(serial, name, events)` and release routines.
pub fn take(pass: usize) -> (Vec<(u64, String, String)>, BTreeSet<u32>) {
    let Some(mut s) = state().lock().expect("rc sink").take() else { return Default::default() };
    let frames = s.frames.into_iter().filter(|f| f.0 == pass).map(|(_, k, n, e)| (k, n, e)).collect();
    (frames, s.releases.remove(&pass).unwrap_or_default())
}
