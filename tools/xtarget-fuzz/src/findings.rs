//! Writing findings to disk.
//!
//! Each finding lands in its own subdirectory under `findings/`, named
//! by its dedup key, containing:
//!   - `repro.almd`      — the minimized reproducer
//!   - `original.almd`   — the un-minimized program (for context)
//!   - `meta.txt`        — seed, index, rung, kind, summary
//!   - `native.out` / `wasm.out` — captured stdout/stderr/exit of both
//!
//! Findings are deduplicated by `(kind, summary)` so a campaign that
//! re-discovers the same divergence thousands of times writes it once.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::generator::Origin;
use crate::oracle::{Finding, FindingKind, RunEvidence};

/// Sink that owns the findings directory and the dedup set. Shared
/// across worker threads behind a mutex (findings are rare, so the lock
/// is uncontended in practice).
pub struct FindingSink {
    dir: PathBuf,
    /// The campaign's `--family`. Recorded in every `meta.txt`, because
    /// `(seed, index)` only reproduces a program under the same family —
    /// a replay without it regenerates a DIFFERENT program and silently
    /// reports "does not reproduce".
    family: String,
    seen: Mutex<HashSet<String>>,
    /// Count of *unique* findings written (after dedup).
    written: Mutex<usize>,
    /// The perf-class subset of `written` (#1235): unique `Slow` findings.
    /// The campaign's exit code fails on `written - slow` — correctness
    /// classes only.
    slow: Mutex<usize>,
}

impl FindingSink {
    pub fn new(dir: PathBuf, family: &str) -> std::io::Result<Self> {
        std::fs::create_dir_all(&dir)?;
        Ok(FindingSink {
            dir,
            family: family.to_string(),
            seen: Mutex::new(HashSet::new()),
            written: Mutex::new(0),
            slow: Mutex::new(0),
        })
    }

    /// Number of unique findings written so far.
    pub fn count(&self) -> usize {
        *self.written.lock().unwrap()
    }

    /// Number of unique perf-class (`Slow`) findings written so far.
    pub fn slow_count(&self) -> usize {
        *self.slow.lock().unwrap()
    }

    /// Has a finding with this dedup key been recorded already? Lets the
    /// worker skip minimizing a re-discovery of a constant-summary kind
    /// (LeakAtExit), whose key does not change under minimization.
    pub fn is_known(&self, finding: &Finding) -> bool {
        self.seen.lock().unwrap().contains(&dedup_key(finding))
    }

    /// Record a finding. Returns `true` if it was new (written), `false`
    /// if it deduplicated against a prior one.
    pub fn record(
        &self,
        seed: u64,
        index: u64,
        origin: &Origin,
        original: &str,
        minimized: &str,
        finding: &Finding,
    ) -> bool {
        let key = dedup_key(finding);
        {
            let mut seen = self.seen.lock().unwrap();
            if !seen.insert(key.clone()) {
                return false;
            }
        }

        let sub = self.dir.join(sanitize(&key));
        if std::fs::create_dir_all(&sub).is_err() {
            return false;
        }

        let _ = std::fs::write(sub.join("repro.almd"), minimized);
        let _ = std::fs::write(sub.join("original.almd"), original);
        let _ = std::fs::write(
            sub.join("meta.txt"),
            render_meta(seed, index, &self.family, origin, finding),
        );
        if let Some(ev) = &finding.native {
            let _ = std::fs::write(sub.join("native.out"), render_evidence(ev));
        }
        if let Some(ev) = &finding.wasm {
            let _ = std::fs::write(sub.join("wasm.out"), render_evidence(ev));
        }

        *self.written.lock().unwrap() += 1;
        if finding.kind == FindingKind::Slow {
            *self.slow.lock().unwrap() += 1;
        }
        true
    }
}

/// Dedup key: kind + a normalized summary. The summary already encodes
/// the differing line, which is specific enough to separate distinct
/// bugs while collapsing re-discoveries.
fn dedup_key(f: &Finding) -> String {
    format!("{:?}::{}", f.kind, f.summary)
}

/// Turn a dedup key into a filesystem-safe directory name.
fn sanitize(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for ch in key.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    // Cap length so pathological summaries do not overflow path limits.
    out.truncate(MAX_DIR_NAME_LEN);
    out
}

/// Maximum length of a generated finding-directory name.
const MAX_DIR_NAME_LEN: usize = 120;

fn render_meta(seed: u64, index: u64, family: &str, origin: &Origin, f: &Finding) -> String {
    let origin_line = match origin {
        Origin::Synthesis => "synthesis".to_string(),
        Origin::Mutation { corpus_file } => format!("mutation of {corpus_file}"),
        Origin::Identity { blocks } => {
            format!("identity family, {blocks} blocks (oracle: by construction, #1332)")
        }
        Origin::Composition { probes } => {
            format!("composition family, {probes} probes (oracle: by construction)")
        }
    };
    // An identity repro carries its own oracle, so `ladder repro.almd` is
    // the more robust replay: it does not depend on the corpus (or this
    // generator) being byte-identical to the campaign's.
    let ladder_hint = match origin {
        Origin::Identity { .. } | Origin::Composition { .. } => {
            "\nreproduce2  = xtarget-fuzz ladder <this dir>/repro.almd"
        }
        _ => "",
    };
    format!(
        "seed        = {seed}\n\
         index       = {index}\n\
         family      = {family}\n\
         origin      = {origin_line}\n\
         rung        = {:?}\n\
         kind        = {:?}\n\
         summary     = {}\n\
         reproduce   = xtarget-fuzz replay --seed {seed} --index {index} --family {family}{ladder_hint}\n",
        f.rung,
        f.kind,
        capped_summary(&f.summary)
    )
}

fn render_evidence(ev: &RunEvidence) -> String {
    format!(
        "exit_code = {:?}\ntimed_out = {}\nduration  = {:.1}s\n\n--- stdout ---\n{}\n--- stderr ---\n{}\n",
        ev.exit_code,
        ev.timed_out,
        ev.duration_secs,
        head_tail(&ev.stdout, EVIDENCE_STREAM_CAP),
        head_tail(&ev.stderr, EVIDENCE_STREAM_CAP)
    )
}

// ---------------------------------------------------------------------------
// Byte caps on what a finding writes (#3207).
//
// A program that prints a 2^31-1-wide `pad_start` produced a 2 GiB stdout
// line. It reached `meta.txt` twice over (the `summary` line embedded the
// whole line) and `native.out` once, and the nightly verdict job that turns
// `meta.txt` into the findings issue died with SIGSEGV pulling it into a shell
// variable — so no issue was filed for a real correctness finding. Every byte
// a finding writes is now bounded, and every cut says how much was there.
// ---------------------------------------------------------------------------

/// Bytes of each side (native / wasm, expected / got) a finding summary
/// quotes before it cuts to a "... (N bytes total)" marker.
pub const SUMMARY_SIDE_CAP: usize = 2 * 1024;

/// Backstop on the whole `summary` line in `meta.txt`, for summaries no
/// per-side excerpt bounded.
pub const SUMMARY_LINE_CAP: usize = 8 * 1024;

/// Bytes of each stream (stdout, stderr) `native.out` / `wasm.out` keep: half
/// from the head, half from the tail, with the elided count and true size.
pub const EVIDENCE_STREAM_CAP: usize = 1024 * 1024;

/// The largest char boundary of `s` at or below `at`.
fn floor_boundary(s: &str, at: usize) -> usize {
    if at >= s.len() {
        return s.len();
    }
    let mut i = at;
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// The smallest char boundary of `s` at or above `at`.
fn ceil_boundary(s: &str, at: usize) -> usize {
    if at >= s.len() {
        return s.len();
    }
    let mut i = at;
    while !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// `s` Debug-quoted, or — past `cap` bytes — its first `cap` bytes quoted
/// followed by `... (N bytes total)`.
pub fn quoted_excerpt(s: &str, cap: usize) -> String {
    if s.len() <= cap {
        return format!("{s:?}");
    }
    let head = &s[..floor_boundary(s, cap)];
    format!("{head:?}... ({} bytes total)", s.len())
}

/// The `summary` field as `meta.txt` writes it: whole when it fits in
/// [`SUMMARY_LINE_CAP`], else the head with the true size. Newlines are
/// escaped so the field stays one `key = value` line.
fn capped_summary(summary: &str) -> String {
    let head = &summary[..floor_boundary(summary, SUMMARY_LINE_CAP)];
    let mut out = head.replace('\n', "\\n");
    if head.len() < summary.len() {
        out.push_str(&format!("... ({} bytes total)", summary.len()));
    }
    out
}

/// `s` whole when it fits in `cap` bytes, else its first and last `cap / 2`
/// bytes around a marker naming how many were elided and the true size.
pub fn head_tail(s: &str, cap: usize) -> String {
    if s.len() <= cap {
        return s.to_string();
    }
    let head = &s[..floor_boundary(s, cap / 2)];
    let tail = &s[ceil_boundary(s, s.len() - cap / 2)..];
    let elided = s.len() - head.len() - tail.len();
    format!(
        "{head}\n... [{elided} bytes elided; {} bytes total] ...\n{tail}",
        s.len()
    )
}

#[cfg(test)]
mod cap_tests {
    use super::*;

    #[test]
    fn a_short_side_is_quoted_whole() {
        assert_eq!(quoted_excerpt("r1 = ok(2)", SUMMARY_SIDE_CAP), "\"r1 = ok(2)\"");
    }

    #[test]
    fn a_long_side_is_cut_at_the_cap_and_names_its_size() {
        let line = " ".repeat(5_000_000);
        let q = quoted_excerpt(&line, SUMMARY_SIDE_CAP);
        assert!(q.ends_with("... (5000000 bytes total)"), "{}", &q[q.len() - 40..]);
        assert!(q.len() <= SUMMARY_SIDE_CAP + 64, "len {}", q.len());
    }

    #[test]
    fn a_cut_never_splits_a_char() {
        let line = "日".repeat(1000); // 3 bytes each, 3000 bytes
        let q = quoted_excerpt(&line, 1000); // 1000 is not a boundary
        assert!(q.starts_with('"'));
        assert!(q.ends_with("... (3000 bytes total)"));
        let h = head_tail(&line, 1001);
        assert!(h.contains("bytes elided; 3000 bytes total"));
    }

    #[test]
    fn the_meta_summary_is_bounded_and_one_line() {
        let s = format!("stdout differs: native={}", "x\n".repeat(100_000));
        let c = capped_summary(&s);
        assert!(!c.contains('\n'));
        assert!(c.len() <= 2 * SUMMARY_LINE_CAP + 64, "len {}", c.len());
        assert!(c.ends_with(&format!("... ({} bytes total)", s.len())));
        assert_eq!(capped_summary("wasm run hung"), "wasm run hung");
    }

    #[test]
    fn evidence_keeps_head_and_tail_and_the_true_size() {
        let out = format!("HEAD{}TAIL", "-".repeat(3 * EVIDENCE_STREAM_CAP));
        let c = head_tail(&out, EVIDENCE_STREAM_CAP);
        assert!(c.starts_with("HEAD"));
        assert!(c.ends_with("TAIL"));
        assert!(c.contains(&format!("{} bytes total", out.len())));
        assert!(c.len() <= EVIDENCE_STREAM_CAP + 80, "len {}", c.len());
        assert_eq!(head_tail("small", EVIDENCE_STREAM_CAP), "small");
    }

    /// The #3207 shape end to end: an `OutputDivergence` whose differing
    /// line is the whole (huge) output renders a bounded `meta.txt`.
    #[test]
    fn a_huge_divergence_renders_a_bounded_meta() {
        use crate::oracle::{Finding, FindingKind, Rung};
        let line = " ".repeat(4_000_000);
        let f = Finding {
            rung: Rung::Run,
            kind: FindingKind::OutputDivergence,
            summary: format!(
                "stdout differs: native={} wasm={}",
                quoted_excerpt(&line, SUMMARY_SIDE_CAP),
                quoted_excerpt("x", SUMMARY_SIDE_CAP)
            ),
            native: None,
            wasm: None,
        };
        let meta = render_meta(1, 2, "all", &Origin::Synthesis, &f);
        assert!(meta.len() < 3 * SUMMARY_SIDE_CAP, "meta is {} bytes", meta.len());
        assert!(meta.contains("(4000000 bytes total)"));
    }
}
