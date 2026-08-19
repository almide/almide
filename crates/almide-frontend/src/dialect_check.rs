//! E051: a `@dialect(N)` stamp naming a dialect this compiler does not speak.
//!
//! The stamp records which language dialect a file was last verified against
//! (`proofs/dialect-epochs.toml`). Three of the four standings are silent:
//! no stamp, a current stamp, and a STALE stamp all compile normally — a file
//! written for an older dialect keeps working until it actually uses something
//! that moved, and warning on every older file would make the stamp a tax
//! instead of an instrument.
//!
//! The fourth is an error. A stamp AHEAD of this compiler says the file was
//! verified against a language this binary has never seen, so compiling it
//! successfully here would be luck, not evidence — exactly the case where a
//! generator's output looks fine and is not.

use almide_base::diagnostic::Diagnostic;
use almide_lang::ast;
use almide_lang::dialect::{standing, DialectStanding, CURRENT_DIALECT};

pub fn check_dialect_stamp(program: &ast::Program, diagnostics: &mut Vec<Diagnostic>) {
    check_dialect_stamp_in(None, program, diagnostics)
}

/// As [`check_dialect_stamp`], attributing the diagnostic to `module` when the
/// program being checked is an imported module rather than the file the user
/// named. Without the attribution the caret would be drawn against the main
/// file's source, pointing at whatever happens to be on that line.
pub fn check_dialect_stamp_in(
    module: Option<&str>,
    program: &ast::Program,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(stamp) = program.dialect else { return };
    let DialectStanding::Ahead { epoch } = standing(Some(stamp.epoch)) else { return };

    let subject = match module {
        Some(m) => format!("module '{m}' is"),
        None => "this file is".to_string(),
    };
    let mut diag = Diagnostic::error(
        format!(
            "{} stamped for dialect {} but this compiler speaks dialect {}",
            subject, epoch, CURRENT_DIALECT
        ),
        format!(
            "Upgrade the compiler, or lower the stamp to {} if the file really does target this dialect. \
             `@dialect(N)` records the dialect a file was VERIFIED against — a stamp ahead of the compiler \
             means nothing here has checked it.",
            CURRENT_DIALECT
        ),
        format!("@dialect({})", epoch),
    )
    .with_code("E051");
    if let Some(m) = module {
        diag.file = Some(m.to_string());
    } else if let Some(s) = stamp.span {
        // Span only for the file the user named: a module's span would be
        // drawn against the main file's source text.
        diag.line = Some(s.line);
        diag.col = Some(s.col);
    }
    diagnostics.push(diag);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program_with(stamp: Option<u32>) -> ast::Program {
        let mut p = almide_lang::parse_cached("fn main() -> Unit = println(\"x\")")
            .expect("probe parses")
            .clone();
        p.dialect = stamp.map(|epoch| ast::DialectStamp { epoch, span: None });
        p
    }

    fn diags_for(stamp: Option<u32>) -> Vec<Diagnostic> {
        let mut d = Vec::new();
        check_dialect_stamp(&program_with(stamp), &mut d);
        d
    }

    #[test]
    fn a_future_stamp_is_a_named_error() {
        let d = diags_for(Some(CURRENT_DIALECT + 1));
        assert_eq!(d.len(), 1, "a stamp ahead of the compiler must be rejected");
        assert_eq!(d[0].code.as_deref(), Some("E051"));
    }

    /// The three silent standings, asserted as silent. Without these the
    /// check could tighten into a nag on every older file and no test would
    /// notice.
    #[test]
    fn unstamped_current_and_stale_are_all_silent() {
        assert!(diags_for(None).is_empty(), "unstamped files must stay silent");
        assert!(diags_for(Some(CURRENT_DIALECT)).is_empty(), "a current stamp must be silent");
        for n in 1..CURRENT_DIALECT {
            assert!(diags_for(Some(n)).is_empty(), "stale stamp {n} must be silent");
        }
    }
}
