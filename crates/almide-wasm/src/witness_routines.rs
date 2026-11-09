//! #4005 A4: the runtime routines a module ships — the fixed-slot helpers,
//! the per-program helpers and the closure shims — each by its emitted
//! function index, with a label and, where the routine's builder states
//! them, its heap parameters.
//!
//! The anchor file's `routines` line carries them by shipped position:
//!
//! ```text
//! routines <position>:<heap params>:<label> ...
//! ```
//!
//! `<heap params>` has the `selfhost` line's syntax (`-`, or `p` / `p=s`
//! comma-separated: wasm parameter `p` holds a block's base, its len word a
//! count at stride `s`). The linear certificate's translation binds each to
//! a block (`h p r`), exactly as a self-host body's heap-typed parameter —
//! the same trusted declaration, made where the routine's body is built. The
//! label is for the inventory only (almide-anchor-check's census by
//! function, the row-4 ledger); nothing checks it.

use std::sync::Mutex;

#[derive(Default)]
struct State {
    pass: usize,
    /// `(pass, emitted index, heap params, label)`.
    labels: Vec<(usize, u32, String, String)>,
}

fn state() -> &'static Mutex<Option<State>> {
    use std::sync::OnceLock;
    static S: OnceLock<Mutex<Option<State>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}

fn with(f: impl FnOnce(&mut State)) {
    if let Some(s) = state().lock().expect("routines sink").as_mut() {
        f(s);
    }
}

pub(crate) fn start() {
    *state().lock().expect("routines sink") = Some(State::default());
}

pub(crate) fn begin_pass(pass: usize) {
    with(|s| s.pass = pass);
}

/// One label token: no whitespace, no `:`.
fn token(s: &str) -> String {
    s.chars().map(|c| if c.is_whitespace() || c == ':' { '_' } else { c }).collect()
}

/// The routine at emitted index `f` ships as `label`, with heap parameters
/// `heap` (`&[]`: none declared).
pub(crate) fn label(f: u32, label: &str, heap: &[(u32, Option<u32>)]) {
    let heap = if heap.is_empty() {
        "-".to_string()
    } else {
        heap.iter()
            .map(|(p, s)| match s {
                Some(s) => format!("{p}={s}"),
                None => p.to_string(),
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    with(|s| {
        let pass = s.pass;
        s.labels.push((pass, f, heap, token(label)));
    });
}

/// The shipped pass's routines: `(emitted index, heap params, label)`, one
/// per index (a module assembled twice in a pass keeps the last).
pub fn take(pass: usize) -> Vec<(u32, String, String)> {
    let mut g = state().lock().expect("routines sink");
    let Some(s) = g.as_mut() else { return Vec::new() };
    let mut by_index = std::collections::BTreeMap::new();
    for (p, f, h, l) in std::mem::take(&mut s.labels) {
        if p == pass {
            by_index.insert(f, (h, l));
        }
    }
    by_index.into_iter().map(|(f, (h, l))| (f, h, l)).collect()
}
