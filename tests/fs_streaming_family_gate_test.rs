//! Streaming-lines family gate: the streaming walkers over a file's lines are
//! EXACTLY {fold_lines, for_each_line, fold_lines_range, fold_lines_chunked}.
//! Growing a lines walker outside the family (map_lines, filter_lines, a
//! range cell for for_each, …) — or dropping a cell — fails here, so the
//! completeness rule and its intentional omissions stay machine-enforced
//! (CLAUDE.md: extended by matrix, never point-wise). Omissions: transforming
//! pipelines belong on fold_lines, an eager List belongs to read_lines, chunk
//! workers accumulate (fold is their shape, so for_each has no range/chunked
//! cell), and the ADR-0006 fallible callback forms are the tracked follow-up
//! cell.

#[test]
fn fs_streaming_family_is_exactly_the_four_walkers() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/fs.almd"),
    )
    .expect("read stdlib/fs.almd");
    for f in [
        "fn fold_lines[A](path: String, init: A, f: fn(A, String) -> A) -> Result[A, String]",
        "fn for_each_line(path: String, f: fn(String) -> Unit) -> Result[Unit, String]",
        "fn fold_lines_range[A](path: String, start: Int, end: Int, init: A, f: fn(A, String) -> A) -> Result[A, String]",
        "fn fold_lines_chunked[A](path: String, workers: Int, init: A, f: fn(A, String) -> A) -> Result[List[A], String]",
    ] {
        assert!(src.contains(f), "streaming family cell missing or reshaped: {f}");
    }
    // Every effect-fn walker over lines is one of the four (read_lines and
    // its _if_exists twin are the EAGER readers, a different family).
    let walkers = src
        .lines()
        .filter(|l| {
            l.starts_with("effect fn ") && l.contains("line") && !l.contains("read_lines")
        })
        .count();
    assert_eq!(
        walkers, 4,
        "the streaming-lines family changed size ({walkers} != 4) — update the \
         family rule (stdlib/fs.almd comment + this gate + C-220) in the same PR"
    );
}
