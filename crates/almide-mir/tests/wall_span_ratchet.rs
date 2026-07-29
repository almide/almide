//! The spanless-wall RATCHET (#931).
//!
//! `LowerError::at(span, reason)` is the constructor every NEW wall site must
//! use: it carries the source span of the construct that walled, so the CLI
//! renders the wall through the Diagnostic machinery — source line, caret —
//! instead of a bare sentence. The legacy spanless `LowerError::Unsupported(…)`
//! sites are grandfathered at the count below, which may only go DOWN (the
//! same ratchet discipline as the walled-real count and the skip ledger):
//! migrating a site lowers it, adding a spanless site trips this test.
//!
//! Counted TEXTUALLY over crates/almide-mir/src — constructions and the few
//! frozen pattern-matches alike — so any new mention of the spanless form
//! (either kind) shows up and gets reviewed.

const BASELINE: usize = 193;

#[test]
fn spanless_wall_count_only_goes_down() {
    let src_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut count = 0usize;
    let mut stack = vec![src_root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read src dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).expect("read source");
                count += text.matches("LowerError::Unsupported(").count();
            }
        }
    }
    assert!(
        count <= BASELINE,
        "{count} spanless `LowerError::Unsupported(` mentions (baseline {BASELINE}) — \
         new wall sites must use `LowerError::at(<node>.span, reason)` so the wall \
         renders with a source location (#931)"
    );
    // Blindness floor at 90% of the baseline, not 1 (#988): 192 of 193 sites
    // relocating out of the scanned tree is a broken scan reading as a 99%
    // win, not progress. A legitimate migration lands in reviewable
    // increments and lowers BASELINE in the same (ratchet-only) commit.
    assert!(
        count >= BASELINE * 9 / 10,
        "sanity: the scan found only {count} mentions (baseline {BASELINE}) — \
         the source layout moved out from under this ratchet; re-point it and \
         lower BASELINE consciously (#988)"
    );
    if count < BASELINE {
        eprintln!(
            "spanless wall count is {count}, below the {BASELINE} baseline — \
             lower BASELINE in this test to lock in the progress"
        );
    }
}
