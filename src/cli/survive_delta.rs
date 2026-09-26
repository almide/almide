//! The before/after comparison of `almide survive` (#2147): which diagnostic,
//! test and contract outcome is the SAME one on both sides of an edit, and
//! which of the four buckets it falls in.
//!
//! ## Diagnostic identity (the crux)
//!
//! A diagnostic has no stable id; its position is the only thing that says
//! WHICH `E001` it is, and an edit moves positions — insert one line at the
//! top and every diagnostic below it is renumbered without anything having
//! changed. So identity is decided on the edited file's LINE MAP, not on raw
//! line numbers:
//!
//! 1. A line diff (LCS over lines) between the before- and after-text maps
//!    every UNCHANGED before-line to its after-line; an edited line maps to
//!    nothing. Files other than the edited one map every line to itself.
//! 2. **position** — a before- and an after-diagnostic are one diagnostic when
//!    they come from the same checked entry, name the same file, and have the
//!    same level, code and message, and the before-position mapped through
//!    the line map equals the after-position (same column: an unchanged line
//!    did not move horizontally).
//! 3. **position_renumbered** — as 2, but the messages are equal only once
//!    every digit run is erased: a message that quotes a line number ("first
//!    defined at line 12") is renumbered by the same insertion that moved it.
//! 4. **content** — for a diagnostic that sits on an EDITED line on either
//!    side (its position has no image, or its after-position has no
//!    pre-image), same entry, file, level, code and message suffice. An edit
//!    that rewrites a line and leaves its error in place did not fix it.
//!
//! Everything left unpaired is a before-only diagnostic (`newly_fixed`) or an
//! after-only one (`newly_broken`). A diagnostic is itself the bad state, so
//! the `removed` bucket is always empty on the check leg.
//!
//! ## Buckets (tests and contracts)
//!
//! Keyed by identity (file + test name; fixture + contract id), with a status
//! per side that is either good (a test passes, a contract holds) or not:
//!
//! | before | after | bucket |
//! |---|---|---|
//! | present | absent | `removed` |
//! | absent or not good | good | `newly_fixed` |
//! | absent or good | not good | `newly_broken` |
//! | good | good | `unchanged` |
//! | not good | not good | `unchanged` |

use serde_json::{json, Value};

/// Unchanged before-lines → their after-lines, for one edited file.
pub struct LineMap {
    /// Index `i` = before line `i + 1`; `Some(j)` = after line `j + 1`.
    fwd: Vec<Option<usize>>,
    /// Index `j` = after line `j + 1`; whether it is an unchanged line.
    after_kept: Vec<bool>,
}

/// Above this many cells the middle of a rewrite is treated as wholly
/// replaced (no anchors inside it) instead of paying the quadratic table.
const LCS_CELL_BUDGET: usize = 4_000_000;

impl LineMap {
    pub fn new(before: &str, after: &str) -> Self {
        let a: Vec<&str> = before.lines().collect();
        let b: Vec<&str> = after.lines().collect();
        let mut fwd = vec![None; a.len()];
        let mut after_kept = vec![false; b.len()];
        let mut pre = 0;
        while pre < a.len() && pre < b.len() && a[pre] == b[pre] {
            fwd[pre] = Some(pre);
            after_kept[pre] = true;
            pre += 1;
        }
        let mut suf = 0;
        while suf < a.len() - pre && suf < b.len() - pre && a[a.len() - 1 - suf] == b[b.len() - 1 - suf] {
            fwd[a.len() - 1 - suf] = Some(b.len() - 1 - suf);
            after_kept[b.len() - 1 - suf] = true;
            suf += 1;
        }
        let (am, bm) = (&a[pre..a.len() - suf], &b[pre..b.len() - suf]);
        if !am.is_empty() && !bm.is_empty() && (am.len() + 1) * (bm.len() + 1) <= LCS_CELL_BUDGET {
            for (i, j) in lcs_pairs(am, bm) {
                fwd[pre + i] = Some(pre + j);
                after_kept[pre + j] = true;
            }
        }
        LineMap { fwd, after_kept }
    }

    /// The after-line of before-line `line` (1-based), when it is unchanged.
    /// Line 0 (a diagnostic with no position) maps to itself.
    pub fn map(&self, line: usize) -> Option<usize> {
        if line == 0 {
            return Some(0);
        }
        self.fwd.get(line - 1).copied().flatten().map(|j| j + 1)
    }

    /// Is after-line `line` (1-based) an unchanged line?
    pub fn after_kept(&self, line: usize) -> bool {
        line == 0 || self.after_kept.get(line - 1).copied().unwrap_or(false)
    }
}

/// The matched index pairs of a longest common subsequence of lines.
fn lcs_pairs(a: &[&str], b: &[&str]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let w = m + 1;
    let mut t = vec![0u32; (n + 1) * w];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            t[i * w + j] = if a[i] == b[j] {
                t[(i + 1) * w + j + 1] + 1
            } else {
                t[(i + 1) * w + j].max(t[i * w + j + 1])
            };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if t[(i + 1) * w + j] >= t[i * w + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// One diagnostic as `almide check --json` emitted it, plus where it came
/// from. `json` is carried to the output untouched, so any field the check
/// JSON grows (the repair schema, #2149) reaches the caller with no change here.
pub struct Diag {
    /// The entry whose `check` run printed it.
    pub checked: String,
    /// Its `file`, or the checked entry when the field is empty.
    pub file: String,
    /// Whether `file` is the edited file (its positions move with the edit).
    pub in_edited: bool,
    pub json: Value,
}

impl Diag {
    fn s(&self, k: &str) -> &str {
        self.json.get(k).and_then(|v| v.as_str()).unwrap_or("")
    }
    fn n(&self, k: &str) -> usize {
        self.json.get(k).and_then(|v| v.as_u64()).unwrap_or(0) as usize
    }
    fn same_kind(&self, o: &Diag) -> bool {
        self.checked == o.checked && self.file == o.file && self.s("level") == o.s("level") && self.s("code") == o.s("code")
    }
    fn pos(&self) -> (usize, usize) {
        (self.n("line"), self.n("col"))
    }
}

fn erase_digits(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_run = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            if !in_run {
                out.push('#');
            }
            in_run = true;
        } else {
            out.push(c);
            in_run = false;
        }
    }
    out
}

/// The four buckets of one leg.
#[derive(Default)]
pub struct Delta {
    pub unchanged: Vec<Value>,
    pub newly_broken: Vec<Value>,
    pub newly_fixed: Vec<Value>,
    pub removed: Vec<Value>,
}

impl Delta {
    pub fn to_json(&self) -> Value {
        json!({
            "unchanged": self.unchanged,
            "newly_broken": self.newly_broken,
            "newly_fixed": self.newly_fixed,
            "removed": self.removed,
        })
    }

    pub fn counts(&self) -> Value {
        json!({
            "unchanged": self.unchanged.len(),
            "newly_broken": self.newly_broken.len(),
            "newly_fixed": self.newly_fixed.len(),
            "removed": self.removed.len(),
        })
    }
}

/// Pair before- and after-diagnostics by the identity rule in the module doc.
pub fn diagnostic_delta(before: &[Diag], after: &[Diag], map: &LineMap) -> Delta {
    let mut a_taken = vec![false; after.len()];
    let mut b_pair: Vec<Option<(usize, &'static str)>> = vec![None; before.len()];

    let mapped = |b: &Diag| -> Option<(usize, usize)> {
        let (l, c) = b.pos();
        if b.in_edited { map.map(l).map(|l2| (l2, c)) } else { Some((l, c)) }
    };
    let on_edited_line = |b: &Diag, a: &Diag| -> bool {
        b.in_edited && (map.map(b.pos().0).is_none() || !map.after_kept(a.pos().0))
    };

    // Passes 1 and 2: anchored by the mapped position.
    for (pass, renumbered) in [("position", false), ("position_renumbered", true)] {
        for (bi, b) in before.iter().enumerate() {
            if b_pair[bi].is_some() {
                continue;
            }
            let Some(at) = mapped(b) else { continue };
            let found = after.iter().enumerate().position(|(ai, a)| {
                !a_taken[ai]
                    && b.same_kind(a)
                    && a.pos() == at
                    && if renumbered {
                        erase_digits(b.s("message")) == erase_digits(a.s("message"))
                    } else {
                        b.s("message") == a.s("message")
                    }
            });
            if let Some(ai) = found {
                a_taken[ai] = true;
                b_pair[bi] = Some((ai, pass));
            }
        }
    }
    // Pass 3: same content, when either side sits on an edited line.
    for (bi, b) in before.iter().enumerate() {
        if b_pair[bi].is_some() {
            continue;
        }
        let found = after.iter().enumerate().position(|(ai, a)| {
            !a_taken[ai] && b.same_kind(a) && b.s("message") == a.s("message") && on_edited_line(b, a)
        });
        if let Some(ai) = found {
            a_taken[ai] = true;
            b_pair[bi] = Some((ai, "content"));
        }
    }

    let mut d = Delta::default();
    for (bi, b) in before.iter().enumerate() {
        match b_pair[bi] {
            Some((ai, how)) => d.unchanged.push(json!({
                "checked": b.checked,
                "matched_by": how,
                "before": b.json,
                "after": after[ai].json,
            })),
            None => d.newly_fixed.push(json!({ "checked": b.checked, "before": b.json, "after": Value::Null })),
        }
    }
    for (ai, a) in after.iter().enumerate() {
        if !a_taken[ai] {
            d.newly_broken.push(json!({ "checked": a.checked, "before": Value::Null, "after": a.json }));
        }
    }
    d
}

/// Which bucket a keyed outcome falls in (see the table in the module doc).
pub fn bucket(before: Option<&str>, after: Option<&str>, good: fn(&str) -> bool) -> &'static str {
    match (before, after) {
        (Some(_), None) => "removed",
        (b, Some(a)) => {
            let was_good = b.is_some_and(good);
            match (was_good, good(a)) {
                (false, true) => "newly_fixed",
                (true, false) => "newly_broken",
                (false, false) if b.is_none() => "newly_broken",
                _ => "unchanged",
            }
        }
        (None, None) => "unchanged",
    }
}

impl Delta {
    pub fn push(&mut self, bucket: &str, entry: Value) {
        match bucket {
            "removed" => self.removed.push(entry),
            "newly_fixed" => self.newly_fixed.push(entry),
            "newly_broken" => self.newly_broken.push(entry),
            _ => self.unchanged.push(entry),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diag(line: usize, col: usize, code: &str, msg: &str) -> Diag {
        Diag {
            checked: "m.almd".into(),
            file: "m.almd".into(),
            in_edited: true,
            json: json!({"level":"error","code":code,"message":msg,"file":"m.almd","line":line,"col":col}),
        }
    }

    #[test]
    fn the_line_map_follows_an_insertion_and_marks_the_edited_lines() {
        let m = LineMap::new("a\nb\nc\n", "x\na\nb2\nc\n");
        assert_eq!(m.map(1), Some(2)); // a moved down one
        assert_eq!(m.map(2), None); // b was rewritten
        assert_eq!(m.map(3), Some(4)); // c moved down one
        assert!(!m.after_kept(1) && !m.after_kept(3) && m.after_kept(4));
    }

    #[test]
    fn a_diagnostic_below_an_insertion_is_the_same_diagnostic() {
        let map = LineMap::new("a\nb\n", "new\na\nb\n");
        let d = diagnostic_delta(&[diag(2, 5, "E001", "undefined x")], &[diag(3, 5, "E001", "undefined x")], &map);
        assert_eq!((d.unchanged.len(), d.newly_broken.len(), d.newly_fixed.len()), (1, 0, 0));
        assert_eq!(d.unchanged[0]["matched_by"], "position");
    }

    #[test]
    fn the_content_pass_pairs_only_across_an_edited_line() {
        // Line 1 (carrying the error) was rewritten and the same error now
        // shows on line 3: the before-side sits on an edited line, so the
        // content pass pairs them — the edit moved the error, it did not fix it.
        let map = LineMap::new("bad\nok\nok2\n", "good\nok\nok2\n");
        let d = diagnostic_delta(&[diag(1, 1, "E001", "m")], &[diag(3, 1, "E001", "m")], &map);
        assert_eq!(d.unchanged.len(), 1);
        assert_eq!(d.unchanged[0]["matched_by"], "content");
        // Both lines unchanged: a diagnostic that vanished from line 2 and one
        // that appeared on line 3 are two different diagnostics.
        let map = LineMap::new("x\nok\nok2\n", "y\nok\nok2\n");
        let d = diagnostic_delta(&[diag(2, 1, "E001", "m")], &[diag(3, 1, "E001", "m")], &map);
        assert_eq!((d.unchanged.len(), d.newly_broken.len(), d.newly_fixed.len()), (0, 1, 1));
    }

    #[test]
    fn a_message_quoting_a_line_number_is_renumbered_not_replaced() {
        let map = LineMap::new("a\nb\n", "new\na\nb\n");
        let d = diagnostic_delta(
            &[diag(2, 1, "E010", "duplicate, first defined at line 1")],
            &[diag(3, 1, "E010", "duplicate, first defined at line 2")],
            &map,
        );
        assert_eq!(d.unchanged.len(), 1);
        assert_eq!(d.unchanged[0]["matched_by"], "position_renumbered");
    }

    #[test]
    fn a_changed_message_on_a_kept_line_is_a_fix_and_a_break() {
        let map = LineMap::new("a\n", "a\n");
        let d = diagnostic_delta(&[diag(1, 1, "E003", "expected Int")], &[diag(1, 1, "E003", "expected String")], &map);
        assert_eq!((d.newly_broken.len(), d.newly_fixed.len()), (1, 1));
    }

    #[test]
    fn the_bucket_table() {
        let good = |s: &str| s == "pass";
        assert_eq!(bucket(Some("pass"), Some("pass"), good), "unchanged");
        assert_eq!(bucket(Some("fail"), Some("fail"), good), "unchanged");
        assert_eq!(bucket(Some("pass"), Some("fail"), good), "newly_broken");
        assert_eq!(bucket(Some("fail"), Some("pass"), good), "newly_fixed");
        assert_eq!(bucket(None, Some("pass"), good), "newly_fixed");
        assert_eq!(bucket(None, Some("fail"), good), "newly_broken");
        assert_eq!(bucket(Some("pass"), None, good), "removed");
        assert_eq!(bucket(Some("fail"), None, good), "removed");
    }
}
