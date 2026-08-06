//! #1106 family gate: the `_if_exists` variants exist for EXACTLY the fs
//! content readers. Adding a content reader without its `_if_exists` cell —
//! or growing an `_if_exists` outside the family — fails here, so the
//! family's completeness rule (and its intentional omissions) stays
//! machine-enforced (CLAUDE.md: extended by matrix, never point-wise).

#[test]
fn fs_if_exists_family_is_exactly_the_content_readers() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/fs.almd"),
    )
    .expect("read stdlib/fs.almd");
    // Every content reader has its cell…
    for f in [
        "fn read_text_if_exists",
        "fn read_bytes_if_exists",
        "fn read_lines_if_exists",
        "fn read_bytes_raw_if_exists",
    ] {
        assert!(src.contains(f), "content-reader family cell missing: {f}");
    }
    // …and nothing else grows one. Intentional omissions: metadata queries
    // (stat/file_size/modified_at — exists/is_dir predicates are first-class),
    // directory ops (list_dir/walk/glob), writes (they create), and remove
    // (absence tolerance there is rm -f, a different semantics).
    let count = src.matches("_if_exists(").count();
    assert_eq!(
        count, 4,
        "the _if_exists family changed size ({count} != 4) — update the family \
         rule (stdlib/fs.almd comment + this gate) in the same PR"
    );
}
