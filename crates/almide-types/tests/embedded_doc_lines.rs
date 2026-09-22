//! The embedded stdlib copy vs the file on disk (#878 / #2436).
//!
//! `build.rs` blanks every whole-line `//` comment and KEEPS every whole-line
//! `///` doc comment, preserving the line count. This pins all three
//! properties against the real stdlib rather than a sample string, so a
//! regression in `blank_comment_lines` (or a copy that drifts from its awk
//! mirror in `scripts/check-embedded-size.sh`) fails here and not in a docs
//! page three steps downstream.

use std::path::PathBuf;

fn stdlib_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../stdlib")
}

fn is_doc(line: &str) -> bool {
    line.trim_start().starts_with("///")
}

fn is_note(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("//") && !t.starts_with("///")
}

#[test]
fn embedded_copy_keeps_doc_lines_blanks_notes_and_preserves_line_count() {
    let mut checked = 0usize;
    let mut doc_lines_seen = 0usize;
    let mut entries: Vec<PathBuf> = std::fs::read_dir(stdlib_dir())
        .expect("stdlib dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "almd").unwrap_or(false))
        .collect();
    entries.sort();
    for path in entries {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let Some(embedded) = almide_types::embedded::source_of(&stem) else {
            panic!("{stem}: no embedded source generated");
        };
        let on_disk = std::fs::read_to_string(&path).unwrap();
        let disk: Vec<&str> = on_disk.split('\n').collect();
        let emb: Vec<&str> = embedded.split('\n').collect();
        assert_eq!(disk.len(), emb.len(), "{stem}: line count differs between disk and embedded");
        for (n, (d, e)) in disk.iter().zip(emb.iter()).enumerate() {
            let ln = n + 1;
            if is_doc(d) {
                assert_eq!(d, e, "{stem}:{ln}: a `///` doc line must survive verbatim");
                doc_lines_seen += 1;
            } else if is_note(d) {
                assert_eq!(*e, "", "{stem}:{ln}: a whole-line `//` note must be blanked");
            } else {
                assert_eq!(d, e, "{stem}:{ln}: a code line must survive verbatim");
            }
            assert!(!is_note(e), "{stem}:{ln}: no whole-line `//` note may ship");
        }
        checked += 1;
    }
    assert!(checked >= 250, "only {checked} stdlib sources checked — the scan went blind");
    // The channel has at least one carrier (string.split_once) so the
    // by-name interface route is exercised end-to-end, not only by shape.
    assert!(doc_lines_seen >= 1, "no `///` line in the stdlib: the doc channel has no carrier");
}
