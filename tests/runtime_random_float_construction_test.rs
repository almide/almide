// THE 53-BIT FLOAT CONSTRUCTION, TESTED ON THE SHIPPED SOURCE (#2495 / #2507)
// ===========================================================================
//
// `runtime/rs` is not a workspace member and a `#[cfg(test)]` block written
// there runs nowhere (runtime/rs/README.md; scripts/check-runtime-test-placement.sh
// refuses one). The behavioural oracle for the runtime is `spec/`, which
// exercises it the way programs do.
//
// `random.float` is the one cell `spec/` cannot reach completely: the mapping
// under test, `almide_float_from_u64`, is private and its only public door,
// `random.float()`, draws a u64 nobody chooses. `spec/stdlib/random_test.almd`
// asserts what IS reachable — a sampled floor over 2000 draws, and the
// construction's top value — but not the individual u64 inputs where the old
// quotient rounded to 1.0.
//
// So this test compiles the SHIPPED source text itself. `runtime/rs/src/random.rs`
// is self-contained (`std::cell::Cell` and nothing else), which is what makes
// the include legal here and not a general answer: most of runtime/rs resolves
// only under the embedder's flat concatenation plus a synthesised prelude, so
// including another module would not compile. Do not read this file as a
// licence to test the rest of runtime/rs this way.
//
// Testing the file rather than a copy is the point: a copy would pass while the
// shipped constant drifted.

#[allow(dead_code)]
mod runtime_random {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/runtime/rs/src/random.rs"
    ));

    // `almide_float_from_u64` is private in the shipped source and stays that
    // way — the emitted crate is one flat module, so a visibility change there
    // is a change to what ships. Inside this module the private item is in
    // scope, so the accessor lives here instead.
    pub fn float_from_u64(r: u64) -> f64 {
        almide_float_from_u64(r)
    }
}

/// The ~1024 largest u64 values are where `r as f64 / u64::MAX as f64` returned
/// exactly 1.0: both numerator and denominator round to 2^64 as a double
/// (#2495). The 53-bit construction takes the top 53 bits and scales by 2^-53,
/// so every u64 maps into [0, 1) by construction.
#[test]
fn every_boundary_draw_lands_below_one() {
    for r in [
        u64::MAX,
        u64::MAX - 1,
        u64::MAX - 1023,
        u64::MAX - 1024,
        u64::MAX - 2047,
        1u64 << 63,
        (1u64 << 63) - 1,
        1u64 << 53,
        (1u64 << 11) - 1,
    ] {
        let f = runtime_random::float_from_u64(r);
        assert!(
            (0.0..1.0).contains(&f),
            "almide_float_from_u64({r}) = {f}, outside [0, 1)"
        );
    }
}

/// The two ends are exact, not merely in range: 0 maps to 0.0, and the top of
/// the u64 range maps to (2^53 - 1) / 2^53 — the largest double below 1.0 with
/// 53 bits of mantissa, which is `1.0 - f64::EPSILON / 2.0`.
#[test]
fn the_ends_of_the_u64_range_are_exact() {
    assert_eq!(runtime_random::float_from_u64(0), 0.0);
    assert_eq!(
        runtime_random::float_from_u64(u64::MAX),
        1.0 - f64::EPSILON / 2.0
    );
    // Anything below 2^11 has no top-53 bits at all and is 0.0 exactly.
    assert_eq!(runtime_random::float_from_u64((1u64 << 11) - 1), 0.0);
    assert_eq!(
        runtime_random::float_from_u64(1u64 << 11),
        1.0 / 9007199254740992.0
    );
}

/// The public draw stays in range over a large sample. The generator is seeded
/// from the clock, so this asserts the invariant, not a sequence.
#[test]
fn the_public_draw_stays_in_range() {
    for _ in 0..100_000 {
        let f = runtime_random::almide_rt_random_float();
        assert!((0.0..1.0).contains(&f), "random.float() returned {f}");
    }
}
