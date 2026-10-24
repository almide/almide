//! The native regex runtime, compiled into the checker so that `almide check`
//! refuses a literal pattern with exactly the reason `regex.new` would abort
//! with at run time (E102, R-regex-02). It is the shipped file — not a copy —
//! so the two can never disagree; the self-hosted twin is held to the same
//! answers by tests/regex_pike_differential_test.rs.
#![allow(dead_code, unused, clippy::all, clippy::pedantic)]

pub(crate) type AlmideString = String;

pub(crate) trait IntoAlmide<T> {
    fn into_almide(self) -> T;
}

impl<T> IntoAlmide<T> for T {
    fn into_almide(self) -> T {
        self
    }
}

fn almide_abort(msg: impl std::fmt::Display) -> ! {
    panic!("{msg}")
}

include!("../../../../runtime/rs/src/regex.rs");

/// Why the pattern is refused, or None.
pub(crate) fn check(pat: &str) -> Option<String> {
    rx_check(pat)
}
