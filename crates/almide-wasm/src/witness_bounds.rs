//! The structural leg's BOUNDS witness (#4005) — what makes #3345's elided
//! checks something the proven checker re-derives instead of an analysis it
//! trusts.
//!
//! bounds_facts.rs skips the check of `xs[i + c]` when an earlier check of
//! the same key decided it in the same straight-line stretch. This sink
//! records each stretch as a program of the certificate language
//! `BoundsCert.check_bounds_cert` judges (proofs/BoundsCert.v,
//! `almide-verify bounds`), stretches joined by `;`:
//!
//! - `c N` — a check of slot N was emitted (and passed, or the run aborted);
//! - `a N` / `w N` — slot N's element was read / written;
//! - `s N` — slot N's index or list changed (a `let` or assign of a variable
//!   the key names).
//!
//! A slot is one key `(xs, i, c)`, numbered from 0 per stretch. A stretch
//! opens where the emitter starts a fact set (a loop body, and after every
//! statement that empties it) and closes where the set is dropped, so no
//! knowledge crosses a `;` — exactly the emitter's rule, which the checker
//! now enforces. An `s` goes to EVERY open stretch, so a stretch suspended by
//! a nested loop body still sees the body's assignments.
//!
//! TRUSTED, as for the ownership witness: that these events are the
//! accesses the bytes contain — an `a` / `w` at every access whose check was
//! elided, an `s` at every change of a variable a key names — and that the
//! hoisted count is the list's count (len_hoist.rs). Checks of keys that are
//! not fact candidates (no hoisted count, or a non-affine index) are emitted
//! immediately before their access and are not recorded.

use std::sync::Mutex;

/// A fact key: list variable, index variable, constant offset.
pub(crate) type Key = (u32, u32, i64);

#[derive(Default)]
struct Stretch {
    slots: Vec<Key>,
    text: String,
}

#[derive(Default)]
struct State {
    pass: usize,
    open: Vec<Stretch>,
    stretches: Vec<String>,
    done: Vec<(usize, String)>,
}

fn state() -> &'static Mutex<Option<State>> {
    use std::sync::OnceLock;
    static S: OnceLock<Mutex<Option<State>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}

fn with(f: impl FnOnce(&mut State)) {
    if let Some(s) = state().lock().expect("bounds sink").as_mut() {
        f(s);
    }
}

pub(crate) fn start() {
    *state().lock().expect("bounds sink") = Some(State::default());
}

/// A new emission pass begins: the previous one's witness is complete.
pub(crate) fn begin_pass(pass: usize) {
    with(|s| {
        s.flush();
        s.pass = pass;
    });
}

impl State {
    fn close_one(&mut self) {
        if let Some(st) = self.open.pop()
            && !st.text.is_empty()
        {
            self.stretches.push(st.text.trim_end().to_string());
        }
    }

    fn flush(&mut self) {
        while !self.open.is_empty() {
            self.close_one();
        }
        let stretches = std::mem::take(&mut self.stretches);
        if !stretches.is_empty() {
            self.done.push((self.pass, stretches.join(";")));
        }
    }

    fn access(&mut self, k: Key, checked: bool, write: bool) {
        let Some(st) = self.open.last_mut() else { return };
        let n = match st.slots.iter().position(|&x| x == k) {
            Some(n) => n,
            None => {
                st.slots.push(k);
                st.slots.len() - 1
            }
        };
        if checked {
            st.text.push_str(&format!("c{n} "));
        }
        st.text.push_str(&format!("{}{n} ", if write { 'w' } else { 'a' }));
    }

    fn kill(&mut self, v: u32) {
        for st in &mut self.open {
            for (n, &(xs, i, _)) in st.slots.iter().enumerate() {
                if xs == v || i == v {
                    st.text.push_str(&format!("s{n} "));
                }
            }
        }
    }
}

/// The emitter starts a fact set: a new stretch, knowing nothing.
pub(crate) fn open() {
    with(|s| s.open.push(Stretch::default()));
}

/// The emitter drops its innermost fact set.
pub(crate) fn close() {
    with(State::close_one);
}

/// An access to key `k` in the innermost stretch: `checked` when its check
/// was emitted here, `write` for a store.
pub(crate) fn access(k: Key, checked: bool, write: bool) {
    with(|s| s.access(k, checked, write));
}

/// Variable `v` changed: every slot of every open stretch that names it.
pub(crate) fn kill(v: u32) {
    with(|s| s.kill(v));
}

/// Every pass's bounds witness, in emission order.
pub fn take() -> Vec<(usize, String)> {
    let mut g = state().lock().expect("bounds sink");
    let Some(mut s) = g.take() else { return Vec::new() };
    s.flush();
    s.done
}

#[cfg(test)]
mod tests {
    use super::*;

    // A local state: the global sink is shared with every other test.
    fn run(f: impl FnOnce(&mut State)) -> String {
        let mut s = State::default();
        f(&mut s);
        s.flush();
        s.done.into_iter().map(|(_, w)| w).collect::<Vec<_>>().join("|")
    }

    #[test]
    fn the_flip_records_two_checks_and_four_accesses() {
        let w = run(|s| {
            s.open.push(Stretch::default());
            s.access((1, 2, 0), true, false);
            s.access((1, 3, 0), true, false);
            s.access((1, 2, 0), false, true);
            s.access((1, 3, 0), false, true);
            s.close_one();
        });
        assert_eq!(w, "c0 a0 c1 a1 w0 w1");
    }

    #[test]
    fn an_assignment_kills_every_slot_naming_the_variable_in_every_open_stretch() {
        let w = run(|s| {
            s.open.push(Stretch::default());
            s.access((1, 2, 0), true, false);
            s.open.push(Stretch::default());
            s.access((5, 2, 1), true, false);
            s.kill(2);
            s.close_one();
            s.close_one();
        });
        assert_eq!(w, "c0 a0 s0;c0 a0 s0");
    }

    #[test]
    fn an_empty_stretch_leaves_nothing() {
        assert_eq!(
            run(|s| {
                s.open.push(Stretch::default());
                s.close_one();
            }),
            ""
        );
    }
}
