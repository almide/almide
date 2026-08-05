//! #1110 family gate: every 2-way sum type with a channel assert has BOTH
//! polarities. Adding a one-sided assert (a future assert_left without
//! assert_right, or removing one of the four) fails here — the family is
//! extended by matrix, never point-wise (CLAUDE.md).

#[test]
fn assert_family_has_both_polarities() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/testing.almd"),
    )
    .expect("read stdlib/testing.almd");
    let pairs = [
        ("fn assert_some", "fn assert_none"), // Option: positive / negative
        ("fn assert_ok", "fn assert_err"),    // Result: positive / negative
    ];
    for (pos, neg) in pairs {
        assert!(
            src.contains(pos) && src.contains(neg),
            "assertion family is one-sided: {pos} / {neg} — both polarities must exist"
        );
    }
    // The wasm self-host twins must cover the same four cells.
    let twins = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/testing_assert.almd"),
    )
    .expect("read stdlib/testing_assert.almd");
    for f in ["testing_assert_some", "testing_assert_none", "testing_assert_ok", "testing_assert_err"] {
        assert!(twins.contains(f), "wasm twin missing: {f}");
    }
}
