//! Unit tests for Core-IR → MIR lowering (extracted from lower.rs).
#![allow(clippy::all)]

    use super::*;
    use almide_ir::*;
    use crate::{verify_ownership, ViolationKind};
    use almide_lang::types::constructor::TypeConstructorId;
    include!("tests_part1.rs");
    include!("tests_part1_b.rs");
    include!("tests_part2.rs");
    include!("tests_part2_b.rs");
