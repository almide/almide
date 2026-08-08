//! Streaming-lines family gate: the streaming walkers over a file's lines are
//! EXACTLY {fold_lines, for_each_line}. Growing a lines walker outside the
//! family (map_lines, filter_lines, …) — or dropping a cell — fails here, so
//! the completeness rule and its intentional omissions stay machine-enforced
//! (CLAUDE.md: extended by matrix, never point-wise). Omissions: transforming
//! pipelines belong on fold_lines, an eager List belongs to read_lines, and
//! the ADR-0006 fallible callback forms are the tracked follow-up cell.

#[test]
fn fs_streaming_family_is_exactly_fold_and_for_each() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/fs.almd"),
    )
    .expect("read stdlib/fs.almd");
    for f in [
        "fn fold_lines[A](path: String, init: A, f: fn(A, String) -> A) -> Result[A, String]",
        "fn for_each_line(path: String, f: fn(String) -> Unit) -> Result[Unit, String]",
    ] {
        assert!(src.contains(f), "streaming family cell missing or reshaped: {f}");
    }
    let walkers = src.matches("_lines[").count() + src.matches("_line(").count();
    assert_eq!(
        walkers, 2,
        "the streaming-lines family changed size ({walkers} != 2) — update the \
         family rule (stdlib/fs.almd comment + this gate) in the same PR"
    );
}
