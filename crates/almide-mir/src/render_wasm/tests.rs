//! Tests for `render_wasm` — the wasm renderer + the self-hosted-stdlib e2e suite.
//! Split out of `render_wasm.rs` to keep that file under the line-count limit; this
//! is a submodule of `render_wasm`, so `use super::*` still resolves to the renderer.

    use super::*;
    use crate::{verify_ownership, MirParam, MirProgram, ScalarWidth, PLACEHOLDER_LAYOUT};
    use std::process::Command;

    include!("tests_part1.rs");
    include!("tests_part1_b.rs");
    include!("tests_part2.rs");
    include!("tests_part3.rs");
    include!("tests_part4.rs");
    include!("tests_part5.rs");
