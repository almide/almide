//! Byte-exact parity with the incumbent (almide@a877d2138).
//!
//! `golden/diag-golden.txt` was produced by compiling the SAME battery body
//! (`golden/battery_body.rs`) against the incumbent's almide-base +
//! src/diagnostic_render.rs and capturing its output — see PORTLOG.md unit 1
//! for the generator provenance. This test replays the body against the
//! ported crate; any byte of divergence fails.

mod env {
    pub use almide_diag::diagnostic::*;
    pub use almide_diag::render::{display, display_with_source, to_json};
    pub use almide_diag::span::Span;
}

mod battery {
    use super::env::*;
    include!("golden/battery_body.rs");
}

#[test]
fn matches_incumbent_golden_byte_for_byte() {
    let got = battery::golden_output();
    let want = include_str!("golden/diag-golden.txt");
    assert_eq!(got, want, "diagnostic rendering diverged from the incumbent golden (almide@a877d2138)");
}
