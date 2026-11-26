//! C-462: proofs/incomplete-verification.toml lists the corpus programs whose
//! wasm certificate is still INCOMPLETE, the only ones the repository's
//! harnesses run with `--allow-incomplete-verification`. The list is
//! shrink-only against its `# ceiling` header line: more rows than the
//! ceiling is a program listed instead of certified; fewer is a ceiling left
//! high after a fix. Each row is one well-formed `entry`, unique, naming a
//! file of this repository.

use std::collections::HashSet;
use std::path::Path;

#[test]
fn the_incomplete_verification_ledger_is_shrink_only_and_well_formed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(root.join("proofs/incomplete-verification.toml")).expect("the ledger");
    let ceiling: usize = text
        .lines()
        .find_map(|l| l.strip_prefix("# ceiling = \"")?.strip_suffix('"')?.parse().ok())
        .expect("a `# ceiling = \"N\"` header line");
    let rows = text.lines().filter(|l| *l == "[[program]]").count();
    let entries: Vec<(&str, &str)> = text
        .lines()
        .filter_map(|l| l.strip_prefix("entry = \"")?.strip_suffix('"')?.split_once(' '))
        .collect();
    assert_eq!(rows, entries.len(), "every [[program]] row carries exactly one `entry = \"<route> <path>\"`");
    let mut seen = HashSet::new();
    for (route, path) in &entries {
        assert!(matches!(*route, "test" | "run" | "build" | "bench"), "{route} {path}: unknown route");
        assert!(seen.insert((*route, *path)), "{route} {path}: listed twice");
        assert!(root.join(path).is_file(), "{route} {path}: no such file");
    }
    assert!(entries.len() <= ceiling, "{} rows exceed the ceiling {ceiling}: certify the program in the producer instead of listing it", entries.len());
    assert!(entries.len() >= ceiling, "{} rows under the ceiling {ceiling}: lower the ceiling to {} in this change", entries.len(), entries.len());
}
