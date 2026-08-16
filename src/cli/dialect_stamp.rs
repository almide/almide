//! `almide check --stamp`: advance a file's `@dialect(N)` stamp, forward only,
//! and only after the file has checked clean.
//!
//! The stamp means "verified against dialect N", so the only honest place to
//! write it is a successful verification — not the formatter, which never
//! type-checks anything. Callers wire this into `cmd_check` AFTER the
//! error-or-exit gate, so reaching it is itself the proof.
//!
//! The edit is textual and surgical: replace the epoch in an existing stamp
//! line, or prepend one. Reformatting the file would make `--stamp`
//! indistinguishable from `fmt`, and a flag that quietly rewrites unrelated
//! lines is a flag nobody dares put in a loop.

/// What `--stamp` did, for the caller to report.
#[derive(Debug, PartialEq, Eq)]
pub enum StampOutcome {
    /// Already at the current dialect — nothing written.
    AlreadyCurrent,
    /// Stamp is ahead of this compiler. Never lowered: the file was verified
    /// somewhere this binary cannot reproduce, and silently walking it
    /// backwards would erase that fact. (E051 has already refused the check,
    /// so in practice this is unreachable from `cmd_check`; it is here so the
    /// function is total and the rule is written down rather than implied.)
    Ahead { epoch: u32 },
    /// The new source text to write.
    Write { from: Option<u32>, to: u32, source: String },
}

/// A file's stamp line is the first non-blank, non-comment line when it is an
/// `@dialect(...)`. Returns (line index, epoch).
fn find_stamp_line(source: &str) -> Option<(usize, u32)> {
    for (i, line) in source.lines().enumerate() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") {
            continue;
        }
        let rest = t.strip_prefix("@dialect(")?;
        let digits = rest.strip_suffix(')')?;
        return digits.trim().parse::<u32>().ok().map(|n| (i, n));
    }
    None
}

pub fn plan(source: &str, current: u32) -> StampOutcome {
    match find_stamp_line(source) {
        Some((_, n)) if n == current => StampOutcome::AlreadyCurrent,
        Some((_, n)) if n > current => StampOutcome::Ahead { epoch: n },
        Some((idx, n)) => {
            let mut lines: Vec<String> = source.lines().map(str::to_string).collect();
            lines[idx] = format!("@dialect({current})");
            let mut out = lines.join("\n");
            if source.ends_with('\n') {
                out.push('\n');
            }
            StampOutcome::Write { from: Some(n), to: current, source: out }
        }
        None => StampOutcome::Write {
            from: None,
            to: current,
            source: format!("@dialect({current})\n\n{source}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "fn main() -> Unit = println(\"hi\")\n";

    #[test]
    fn an_unstamped_file_gains_a_stamp_above_everything() {
        let StampOutcome::Write { from, to, source } = plan(SRC, 3) else {
            panic!("an unstamped file must be stamped")
        };
        assert_eq!((from, to), (None, 3));
        assert!(source.starts_with("@dialect(3)\n\nfn main()"), "got: {source:?}");
    }

    #[test]
    fn an_older_stamp_advances_in_place_without_touching_other_lines() {
        let src = format!("@dialect(1)\n\n{SRC}");
        let StampOutcome::Write { from, to, source } = plan(&src, 3) else {
            panic!("an older stamp must advance")
        };
        assert_eq!((from, to), (Some(1), 3));
        assert_eq!(source, format!("@dialect(3)\n\n{SRC}"));
    }

    #[test]
    fn a_current_stamp_writes_nothing() {
        assert_eq!(plan(&format!("@dialect(3)\n\n{SRC}"), 3), StampOutcome::AlreadyCurrent);
    }

    /// Forward only. A stamp from a newer compiler records a verification this
    /// binary cannot perform; lowering it would forge evidence.
    #[test]
    fn a_newer_stamp_is_never_walked_backwards() {
        assert_eq!(
            plan(&format!("@dialect(9)\n\n{SRC}"), 3),
            StampOutcome::Ahead { epoch: 9 }
        );
    }

    #[test]
    fn leading_comments_do_not_hide_the_stamp() {
        let src = format!("// header\n\n@dialect(1)\n\n{SRC}");
        let StampOutcome::Write { source, .. } = plan(&src, 3) else {
            panic!("a stamp below comments must still be found")
        };
        assert_eq!(source, format!("// header\n\n@dialect(3)\n\n{SRC}"));
    }

    #[test]
    fn stamping_is_idempotent() {
        let StampOutcome::Write { source, .. } = plan(SRC, 3) else { panic!() };
        assert_eq!(plan(&source, 3), StampOutcome::AlreadyCurrent);
    }
}
