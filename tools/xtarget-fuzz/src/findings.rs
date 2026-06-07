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
use crate::oracle::{Finding, RunEvidence};

/// Sink that owns the findings directory and the dedup set. Shared
/// across worker threads behind a mutex (findings are rare, so the lock
/// is uncontended in practice).
pub struct FindingSink {
    dir: PathBuf,
    seen: Mutex<HashSet<String>>,
    /// Count of *unique* findings written (after dedup).
    written: Mutex<usize>,
}

impl FindingSink {
    pub fn new(dir: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&dir)?;
        Ok(FindingSink {
            dir,
            seen: Mutex::new(HashSet::new()),
            written: Mutex::new(0),
        })
    }

    /// Number of unique findings written so far.
    pub fn count(&self) -> usize {
        *self.written.lock().unwrap()
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
        let _ = std::fs::write(sub.join("meta.txt"), render_meta(seed, index, origin, finding));
        if let Some(ev) = &finding.native {
            let _ = std::fs::write(sub.join("native.out"), render_evidence(ev));
        }
        if let Some(ev) = &finding.wasm {
            let _ = std::fs::write(sub.join("wasm.out"), render_evidence(ev));
        }

        *self.written.lock().unwrap() += 1;
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

fn render_meta(seed: u64, index: u64, origin: &Origin, f: &Finding) -> String {
    let origin_line = match origin {
        Origin::Synthesis => "synthesis".to_string(),
        Origin::Mutation { corpus_file } => format!("mutation of {corpus_file}"),
    };
    format!(
        "seed        = {seed}\n\
         index       = {index}\n\
         origin      = {origin_line}\n\
         rung        = {:?}\n\
         kind        = {:?}\n\
         summary     = {}\n\
         reproduce   = xtarget-fuzz replay --seed {seed} --index {index}\n",
        f.rung, f.kind, f.summary
    )
}

fn render_evidence(ev: &RunEvidence) -> String {
    format!(
        "exit_code = {:?}\ntimed_out = {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}\n",
        ev.exit_code, ev.timed_out, ev.stdout, ev.stderr
    )
}
