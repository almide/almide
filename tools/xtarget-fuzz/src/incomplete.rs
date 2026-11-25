//! C-462: every wasm route refuses an INCOMPLETE certificate unless
//! `--allow-incomplete-verification` is given. A refused leg has no behaviour
//! to compare, so every wasm leg this harness builds or runs passes the flag
//! ([`FLAG`]) and the legs are still compared. What the flag lets through is
//! never hidden: each INCOMPLETE verdict the compiler prints is recorded here
//! ([`take`]), counted per program, and printed in the inspection report and
//! the campaign summary ([`report`]). Its warning block is removed from the
//! captured stderr, so the legs' streams compare as they did before C-462.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

/// The flag every wasm leg passes.
pub const FLAG: &str = "--allow-incomplete-verification";

/// The first line of the warning an INCOMPLETE certificate let through prints
/// (src/cli/verify.rs `incomplete_warning`): `<MARK><file>: passed under
/// <flag> with <n> uncertified function(s):`, then `n` indented lines
/// `UNCERTIFIED  <fn>: <reason>`.
const MARK: &str = "warning[verification-incomplete]: ";

#[derive(Default)]
struct Seen {
    /// Every program (the file the warning names) judged INCOMPLETE.
    programs: BTreeSet<String>,
    /// Per reason class, the programs it was seen in.
    reasons: BTreeMap<String, BTreeSet<String>>,
}

static SEEN: Mutex<Option<Seen>> = Mutex::new(None);

/// Record every INCOMPLETE verdict in `stderr` and remove its warning block.
pub fn take(stderr: &mut Vec<u8>) {
    let text = String::from_utf8_lossy(stderr);
    if !text.contains(MARK) {
        return;
    }
    let mut kept = String::with_capacity(text.len());
    let mut lines = text.split_inclusive('\n');
    let mut seen = SEEN.lock().unwrap_or_else(|p| p.into_inner());
    let seen = seen.get_or_insert_with(Seen::default);
    while let Some(line) = lines.next() {
        let Some(rest) = line.strip_prefix(MARK) else {
            kept.push_str(line);
            continue;
        };
        let (file, n) = header(rest.trim_end());
        seen.programs.insert(file.clone());
        for _ in 0..n {
            let Some(l) = lines.next() else { break };
            let reason = l.trim().rsplit_once(": ").map(|(_, r)| r.to_string()).unwrap_or_else(|| l.trim().to_string());
            seen.reasons.entry(reason).or_default().insert(file.clone());
        }
    }
    *stderr = kept.into_bytes();
}

/// `(file, n)` of a warning's first line after the mark.
fn header(rest: &str) -> (String, usize) {
    let key = ": passed under ";
    let Some(at) = rest.find(key) else { return (rest.to_string(), 0) };
    let n = rest[at + key.len()..]
        .split_whitespace()
        .find_map(|w| w.parse::<usize>().ok())
        .unwrap_or(0);
    (rest[..at].to_string(), n)
}

/// The report's own line, after an inspection's verdict lines (a `# ` line,
/// so a stamp writer reading the `key = "..."` block never takes it in). The
/// counts are this process's legs; a `--focus` campaign the parity criterion
/// runs as a child prints its own in `parity`.
pub fn print_report() {
    println!("# INCOMPLETE (this process's legs; a `--focus` campaign child counts its own in `parity`) — {}", report());
}

/// The one-line account: the count and each reason class with its programs.
pub fn report() -> String {
    let seen = SEEN.lock().unwrap_or_else(|p| p.into_inner());
    let Some(s) = seen.as_ref().filter(|s| !s.programs.is_empty()) else {
        return format!("0 program(s) judged INCOMPLETE (wasm legs ran under {FLAG})");
    };
    let mut classes: Vec<(&String, usize)> = s.reasons.iter().map(|(r, p)| (r, p.len())).collect();
    classes.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let classes: Vec<String> = classes.iter().map(|(r, n)| format!("{r} ×{n}")).collect();
    format!(
        "{} program(s) judged INCOMPLETE, compared under {FLAG}: {}",
        s.programs.len(),
        classes.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_warning_block_is_counted_and_removed_and_the_rest_kept() {
        let mut err = b"before\nwarning[verification-incomplete]: /t/p1.almd: passed under --allow-incomplete-verification with 2 uncertified function(s):\n  UNCERTIFIED  main: bind:view-result\n  UNCERTIFIED  f: callback-value:borrowed-temp\nafter\n".to_vec();
        take(&mut err);
        assert_eq!(String::from_utf8(err).unwrap(), "before\nafter\n");
        let r = report();
        assert!(!r.starts_with("0 "), "{r}");
        assert!(r.contains("bind:view-result"), "{r}");
        assert!(r.contains("callback-value:borrowed-temp"), "{r}");
    }

    #[test]
    fn stderr_without_the_warning_is_untouched() {
        let mut err = b"error: boom\n  indented\n".to_vec();
        take(&mut err);
        assert_eq!(err, b"error: boom\n  indented\n");
    }
}
