//! Streaming-lines family gate: the streaming walkers over a file's lines are
//! EXACTLY {fold_lines, for_each_line, fold_lines_range, fold_lines_chunked}.
//! Growing a lines walker outside the family (map_lines, filter_lines, a
//! range cell for for_each, …) — or dropping a cell — fails here, so the
//! completeness rule and its intentional omissions stay machine-enforced
//! (CLAUDE.md: extended by matrix, never point-wise). Omissions: transforming
//! pipelines belong on fold_lines, an eager List belongs to read_lines, chunk
//! workers accumulate (fold is their shape, so for_each has no range/chunked
//! cell).
//!
//! SECOND AXIS (#1144, ADR-0006): each cell either HAS a fallible callback
//! form or is a named, reasoned omission. The rule is "a fallible form exists
//! iff the walk has a defined first err" — true for the two sequential,
//! callback-driven cells, false for the partitioned ones (which chunk errs
//! first is a thread-schedule observable, so an erring chunk body handles its
//! own error). The matrix below is the executable statement of that rule:
//! adding a cell forces a decision in BOTH columns.

/// (cell signature, fallible carrier — `None` = a reasoned omission).
const STREAMING_MATRIX: &[(&str, Option<&str>)] = &[
    (
        "fn fold_lines[A](path: String, init: A, f: (A, String) -> A) -> Result[A, String]",
        Some("fn __fallible_fold_lines[A, E](path: String, init: A, f: (A, String) -> Result[A, E]) -> Result[A, E]"),
    ),
    (
        "fn for_each_line(path: String, f: (String) -> Unit) -> Result[Unit, String]",
        Some("fn __fallible_for_each_line[E](path: String, f: (String) -> Result[Unit, E]) -> Result[Unit, E]"),
    ),
    (
        "fn fold_lines_range[A](path: String, start: Int, end: Int, init: A, f: (A, String) -> A) -> Result[A, String]",
        None,
    ),
    (
        "fn fold_lines_chunked[A](path: String, workers: Int, init: A, f: (A, String) -> A) -> Result[List[A], String]",
        None,
    ),
];

fn fs_almd() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib/fs.almd"),
    )
    .expect("read stdlib/fs.almd")
}

#[test]
fn fs_streaming_family_is_exactly_the_four_walkers() {
    let src = fs_almd();
    for (cell, _) in STREAMING_MATRIX {
        assert!(src.contains(cell), "streaming family cell missing or reshaped: {cell}");
    }
    // Every effect-fn walker over lines is one of the four cells or one of
    // their fallible carriers (read_lines and its _if_exists twin are the
    // EAGER readers, a different family).
    let walkers = src
        .lines()
        .filter(|l| l.starts_with("effect fn ") && l.contains("line") && !l.contains("read_lines"))
        .count();
    let carriers = STREAMING_MATRIX.iter().filter(|(_, c)| c.is_some()).count();
    assert_eq!(
        walkers,
        4 + carriers,
        "the streaming-lines family changed size ({walkers} != {}) — update the \
         family rule (stdlib/fs.almd comment + this gate + C-220) in the same PR",
        4 + carriers
    );
}

/// The ADR-0006 column: a cell's fallible carrier exists iff the matrix says
/// it should, and an OMITTED cell must have no carrier hiding in the module.
#[test]
fn every_streaming_cell_decides_its_fallible_form() {
    let src = fs_almd();
    for (cell, carrier) in STREAMING_MATRIX {
        // The cell name, e.g. "fold_lines_range".
        let name = cell["fn ".len()..]
            .split(|c| c == '[' || c == '(')
            .next()
            .expect("cell name");
        match carrier {
            Some(sig) => assert!(
                src.contains(sig),
                "{name} declares a fallible callback form but the carrier is missing or \
                 reshaped: {sig}\nthe checker's normalize_fallible_hof_callback rewrites \
                 `fs.{name}(.., (..) => f(..)!)` to it; deleting it breaks the \
                 polymorphic form at a distance"
            ),
            None => assert!(
                !src.contains(&format!("__fallible_{name}")),
                "{name} is a REASONED omission from the ADR-0006 column (a partitioned \
                 walk has no defined first err) but a `__fallible_{name}` carrier \
                 exists — decide the rule in the gate, the stdlib comment and C-220 \
                 together, never point-wise"
            ),
        }
    }
}

/// The frontend's dispatch table and this matrix must name the SAME cells —
/// a carrier nothing rewrites to is dead code, and a rewrite with no carrier
/// is a wall at a distance.
#[test]
fn the_frontend_dispatch_table_matches_the_matrix() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("crates/almide-frontend/src/check/infer_calls_closures.rs"),
    )
    .expect("read infer_calls_closures.rs");
    let table = src
        .lines()
        .find(|l| l.contains("const FALLIBLE_HOF_FS"))
        .and_then(|_| {
            src.split("const FALLIBLE_HOF_FS")
                .nth(1)
                .and_then(|s| s.split(';').next())
                .map(|s| s.to_string())
        })
        .expect("FALLIBLE_HOF_FS table");
    for (cell, carrier) in STREAMING_MATRIX {
        let name = cell["fn ".len()..]
            .split(|c| c == '[' || c == '(')
            .next()
            .expect("cell name");
        let listed = table.contains(&format!("\"{name}\""));
        assert_eq!(
            listed,
            carrier.is_some(),
            "FALLIBLE_HOF_FS and the streaming matrix disagree about {name} \
             (dispatch lists it: {listed}, matrix has a carrier: {})",
            carrier.is_some()
        );
    }
}
