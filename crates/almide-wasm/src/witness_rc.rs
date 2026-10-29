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
    /// Per pass, every stdlib arm's instruction window `(serial, from, to)`
    /// in body byte offsets — where RC the arm does inside itself lies.
    arms: HashMap<usize, Vec<(u64, usize, usize)>>,
    /// Per pass, every taken compound routine's window (in-place append,
    /// string building, map edits — `Emitter::routine`).
    routines: HashMap<usize, Vec<(u64, usize, usize)>>,
    /// The windows open right now (innermost last), by id: an `m` logged
    /// while one is open is a move into that arm or routine.
    open_windows: Vec<u64>,
    next_window: u64,
    /// Per pass, each closed window's id and `(serial, from, to)`.
    window_ids: HashMap<usize, HashMap<u64, (u64, usize, usize)>>,
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

/// The recorder logged a live `a` / `d` / `o` on `local` (`None`: a
/// temporary), or an `m`: the variable's credit moved into the arm or
/// routine whose window is open (`#<window id>`; outside every window an
/// `m` is no RC event and is not logged).
pub(crate) fn event(kind: char, local: Option<u32>) {
    let Some(serial) = crate::witness::anchor::current() else { return };
    with(|s| {
        let window = s.open_windows.last().copied();
        if kind == 'm' && (window.is_none() || local.is_none()) {
            return;
        }
        let e = s.open.entry(serial).or_default();
        e.push(kind);
        match local {
            Some(l) => e.push_str(&l.to_string()),
            None => e.push('-'),
        }
        // A move names its window; so does a temporary's event logged inside
        // one (the arm or routine may release that temporary in its callee).
        if kind == 'm' || (local.is_none() && window.is_some()) {
            e.push_str(&format!("#{}", window.unwrap_or(0)));
        }
        e.push(' ');
    });
}

/// A branch site of the recorder opens (`(`) or joins (`)`): an event
/// between them happens on some of the site's paths, one outside every site
/// on each path through the frame (the RC check reads one release per path
/// only for the latter).
pub(crate) fn mark(c: char) {
    let Some(serial) = crate::witness::anchor::current() else { return };
    with(|s| {
        let e = s.open.entry(serial).or_default();
        e.push(c);
        e.push(' ');
    });
}

/// An arm's or a compound routine's instruction window opens; the id names
/// it to [`arm`] / [`routine`] / [`window_dropped`] when it closes.
pub(crate) fn window_open() -> u64 {
    let mut id = 0;
    with(|s| {
        s.next_window += 1;
        id = s.next_window;
        s.open_windows.push(id);
    });
    id
}

fn close(s: &mut State, id: u64, serial: Option<(u64, usize, usize)>) {
    if let Some(at) = s.open_windows.iter().rposition(|&w| w == id) {
        s.open_windows.truncate(at);
    }
    if let Some(w) = serial {
        s.window_ids.entry(s.pass).or_default().insert(id, w);
    }
}

/// A routine route that was not taken: its window records nothing.
pub(crate) fn window_dropped(id: u64) {
    with(|s| close(s, id, None));
}

/// The frame whose body is `serial` finished as `name`.
pub(crate) fn frame(serial: u64, name: &str) {
    with(|s| {
        let ev = s.open.remove(&serial).unwrap_or_default();
        s.frames.push((s.pass, serial, name.to_string(), ev.trim_end().to_string()));
    });
}

/// A stdlib arm was emitted into the open body between byte offsets
/// `from` and `to`.
pub(crate) fn arm(id: u64, from: usize, to: usize) {
    let Some(serial) = crate::witness::anchor::current() else { return };
    with(|s| {
        s.arms.entry(s.pass).or_default().push((serial, from, to));
        close(s, id, Some((serial, from, to)));
    });
}

/// A compound routine that does RC inside itself was taken, emitted into
/// the open body between byte offsets `from` and `to`.
pub(crate) fn routine(id: u64, from: usize, to: usize) {
    let Some(serial) = crate::witness::anchor::current() else { return };
    with(|s| {
        s.routines.entry(s.pass).or_default().push((serial, from, to));
        close(s, id, Some((serial, from, to)));
    });
}

/// `f` was handed out as a release routine.
pub(crate) fn release_fn(f: u32) {
    with(|s| {
        s.releases.entry(s.pass).or_default().insert(f);
    });
}

/// One pass's RC record: its frames `(serial, name, events)`, its release
/// routines, its arm windows and its compound-routine windows, each
/// `(serial, from, to)`, and the windows by the id an `m` event names.
pub type Taken = (
    Vec<(u64, String, String)>,
    BTreeSet<u32>,
    Vec<(u64, usize, usize)>,
    Vec<(u64, usize, usize)>,
    HashMap<u64, (u64, usize, usize)>,
);

/// The shipped pass's RC record.
pub fn take(pass: usize) -> Taken {
    let Some(mut s) = state().lock().expect("rc sink").take() else { return Default::default() };
    let frames = s.frames.into_iter().filter(|f| f.0 == pass).map(|(_, k, n, e)| (k, n, e)).collect();
    // `$dec_flat` is the runtime's own release, in every module whether or
    // not a frame's `dec_fn_of` handed it out: the runtime routines that
    // take a block (`$list_push`, `$bytes_push`, `$str_append`) release
    // through it, and the checker's every-path rule must see that.
    let mut releases = s.releases.remove(&pass).unwrap_or_default();
    releases.insert(crate::F_DEC_FLAT);
    (
        frames,
        releases,
        s.arms.remove(&pass).unwrap_or_default(),
        s.routines.remove(&pass).unwrap_or_default(),
        s.window_ids.remove(&pass).unwrap_or_default(),
    )
}
