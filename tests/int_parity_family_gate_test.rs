//! #1923 family gate: the parity predicate PAIR (`is_even` / `is_odd`)
//! exists on scalar `int` and NOWHERE else. The sized-int modules
//! (int8..uint64) are conversion + bounds surfaces by ruling; a sized
//! value reaches the pair through `int.from_int8(x)`. Growing a parity
//! predicate on a sized module, or losing one half of the pair on `int`,
//! fails here — the family rule stays machine-enforced (CLAUDE.md:
//! extended by matrix, never point-wise).

fn stdlib(name: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib").join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

#[test]
fn int_parity_pair_is_complete_on_int_and_absent_on_the_sized_modules() {
    let int = stdlib("int.almd");
    for f in ["fn is_even(n: Int) -> Bool", "fn is_odd(n: Int) -> Bool"] {
        assert!(int.contains(f), "parity pair cell missing on int: {f}");
    }
    let mut sized_predicates = Vec::new();
    for m in ["int8", "int16", "int32", "int64", "uint8", "uint16", "uint32", "uint64"] {
        for file in [format!("{m}.almd"), format!("{m}_convert.almd")] {
            let src = stdlib(&file);
            for line in src.lines() {
                if line.starts_with("fn is_") {
                    sized_predicates.push(format!("{file}: {line}"));
                }
            }
        }
    }
    assert!(
        sized_predicates.is_empty(),
        "a predicate grew on a sized-int module — the parity family rule says the \
         sized modules carry NO arithmetic predicate (update the rule in \
         stdlib/int.almd and this gate in the same PR):\n{}",
        sized_predicates.join("\n")
    );
}
