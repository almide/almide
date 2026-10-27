//! #1923 family gate: the parity predicate PAIR (`is_even` / `is_odd`)
//! exists on scalar `int` and, since R-sized-10 made it an Integer
//! requirement, on every sized-int module (int8..uint64) too — both halves on
//! each, never one. Losing a half anywhere, or a sized module carrying any
//! other `is_*` predicate, fails here — the family rule stays
//! machine-enforced (CLAUDE.md: extended by matrix, never point-wise).

fn stdlib(name: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib").join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

#[test]
fn int_parity_pair_is_complete_on_int_and_on_every_sized_module() {
    let int = stdlib("int.almd");
    for f in ["fn is_even(x: Int) -> Bool", "fn is_odd(x: Int) -> Bool"] {
        assert!(int.contains(f), "parity pair cell missing on int: {f}");
    }
    let types = [
        ("int8", "Int8"), ("int16", "Int16"), ("int32", "Int32"), ("int64", "Int64"),
        ("uint8", "UInt8"), ("uint16", "UInt16"), ("uint32", "UInt32"), ("uint64", "UInt64"),
    ];
    let mut problems = Vec::new();
    for (m, t) in types {
        let module = stdlib(&format!("{m}.almd"));
        for f in [format!("fn is_even(x: {t}) -> Bool"), format!("fn is_odd(x: {t}) -> Bool")] {
            if !module.contains(&f) {
                problems.push(format!("{m}.almd: parity pair cell missing: {f}"));
            }
        }
        for file in [format!("{m}.almd"), format!("{m}_convert.almd")] {
            for line in stdlib(&file).lines() {
                if line.starts_with("fn is_") && !line.starts_with("fn is_even(") && !line.starts_with("fn is_odd(") {
                    problems.push(format!("{file}: a predicate outside the parity pair: {line}"));
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "the parity family rule says every sized-int module carries exactly the pair \
         (update the rule in stdlib/int.almd and this gate in the same PR):\n{}",
        problems.join("\n")
    );
}
