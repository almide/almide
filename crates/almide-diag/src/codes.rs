//! Machine-readable diagnostic-code registry with lifecycle metadata.
//!
//! Greenfield evolution E1, adopting two survey findings
//! (../almide-references/RESEARCH-diagnostics.md):
//! - **Lean 4's versioned error-doc metadata** (lean4@bb01f17,
//!   src/Lean/ErrorExplanation.lean:27-31 — `sinceVersion`/`removedVersion?`),
//!   here keyed to Almide's dialect epochs instead of semver.
//! - **MoonBit's recoverability signal** (moonbit@d4ada10,
//!   src/error_code_utils.ml:27-28 — a code band marking non-fatal parse
//!   errors the parser can continue past), here an explicit field instead of
//!   a numeric band.
//!
//! The prose source of truth stays `docs/diagnostics/EXXX.md` (one page per
//! code, ported at unit 0/1); this table is the runtime API, and
//! `tests/codes_docs_sync.rs` holds the two bidirectionally in sync — a code
//! without a page, a page without a row, or a drifted title is a test failure.
//!
//! Lifecycle rule: the 59 ported codes predate both the epoch ledger and the
//! recoverability classification, so their `since_dialect` / `recoverable`
//! are `None` — and that set is FROZEN (see `legacy_none_set_only_shrinks`).
//! Every code added after this table lands must state both.

/// One diagnostic code and its lifecycle metadata.
#[derive(Debug, Clone, Copy)]
pub struct CodeInfo {
    pub code: &'static str,
    /// Title, byte-identical to the `# EXXX — <title>` heading of its doc page.
    pub title: &'static str,
    /// Dialect epoch that introduced this code. `None` = predates the ledger
    /// (frozen legacy set); mandatory for new codes.
    pub since_dialect: Option<u32>,
    /// Dialect epoch that retired it. A retired code keeps its row and its
    /// doc page forever (rustc's never-remove policy).
    pub removed_dialect: Option<u32>,
    /// Whether the emitting phase can recover and continue past this
    /// diagnostic (MoonBit's non-fatal signal). `None` = unclassified legacy;
    /// mandatory for new codes.
    pub recoverable: Option<bool>,
}

macro_rules! c {
    ($code:literal, $title:literal) => {
        CodeInfo { code: $code, title: $title, since_dialect: None, removed_dialect: None, recoverable: None }
    };
}

/// Every diagnostic code, in code order. Generated from the doc pages'
/// heading lines; kept in sync by `tests/codes_docs_sync.rs`.
pub static CODES: &[CodeInfo] = &[
    c!("E001", "Type mismatch"),
    c!("E002", "Undefined function"),
    c!("E003", "Undefined variable"),
    c!("E004", "Wrong number of arguments"),
    c!("E005", "Argument type mismatch (constructor / function call)"),
    c!("E006", "Effect isolation: pure fn calls effect fn"),
    c!("E007", "`fan` block outside effect fn"),
    c!("E008", "`fan` block captures mutable variable"),
    c!("E009", "Reassignment to immutable binding"),
    c!("E010", "Non-exhaustive match"),
    c!("E011", "Mutable var mutated inside closure in pure fn"),
    c!("E012", "Duplicate definition"),
    c!("E013", "Field access on a non-record / missing field"),
    c!("E014", "Unreachable match arm"),
    c!("E015", "Possible stdlib reimplementation"),
    c!("E016", "Function in a Set element or Map key"),
    c!("E017", "Record construction on an enum type name"),
    c!("E018", "Empty collection with an uninferable element type"),
    c!("E019", "Ambiguous constructor"),
    c!("E020", "Conflicting type declarations"),
    c!("E021", "Invalid record construction or pattern"),
    c!("E022", "`!` operator outside an error-propagating context"),
    c!("E023", "derived protocol not satisfied by a field type"),
    c!("E024", "integer literal out of range"),
    c!("E025", "cannot infer a concrete type (unconstrained type slot)"),
    c!("E026", "cannot index a String with `[]`"),
    c!("E027", "fan.timeout was removed"),
    c!("E028", "main() takes no parameters"),
    c!("E029", "unknown type name"),
    c!("E030", "type has no ordering"),
    c!("E031", "retired range spelling"),
    c!("E032", "immutable binding passed to `mut` parameter"),
    c!("E033", "opaque type constructed outside its defining module"),
    c!("E034", "error-channel operator on a non-Option/Result operand"),
    c!("E035", "branching on the text of an error message (warning)"),
    c!("E036", "map_err lambda never uses the error value (warning)"),
    c!("E037", "equality between incompatible types"),
    c!("E038", "`??` separated from its fallback"),
    c!("E041", "implicit propagation was removed"),
    c!("E042", "this statement discards a Result (must-use)"),
    c!("E043", "the try_* spellings, public and internal"),
    c!("E044", "main() returns Unit"),
    c!("E045", "tuple index on a non-tuple, or out of range"),
    c!("E046", "placeholder `_` in a call argument"),
    c!("E047", "invalid escape in a string literal"),
    c!("E048", "variant pattern the subject's type does not have"),
    c!("E049", "`let ... in <expr>` is OCaml/Haskell syntax"),
    c!("E050", "local `fn` collides with a selectively-imported name"),
    c!("E051", "file stamped for a dialect this compiler does not speak"),
    c!("E052", "calling a deprecated function"),
    c!("E053", "unknown attribute"),
    c!("E054", "fmt verification failed"),
    c!("E055", "`?.` on a Result"),
    c!("E056", "`?` on an Option is a no-op (warning)"),
    c!("E057", "fn has no body"),
    c!("E058", "unhashable Map key"),
    c!("E059", "non-iterable for head"),
    c!("E060", "import hygiene (warning)"),
    c!("E420", "Function visibility violation"),
];

/// Number of legacy rows allowed to carry `None` lifecycle fields. This is a
/// shrink-only ratchet: filling in a legacy row lowers it; a NEW code with
/// `None` fields trips `legacy_none_set_only_shrinks` instead of raising it.
pub const LEGACY_NONE_ROWS: usize = 59;

pub fn lookup(code: &str) -> Option<&'static CodeInfo> {
    CODES.iter().find(|c| c.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn codes_are_unique_and_ordered() {
        let mut seen = HashSet::new();
        for w in CODES.windows(2) {
            assert!(w[0].code < w[1].code, "{} out of order", w[1].code);
        }
        for c in CODES {
            assert!(seen.insert(c.code), "duplicate {}", c.code);
        }
    }

    #[test]
    fn legacy_none_set_only_shrinks() {
        let none_rows = CODES.iter()
            .filter(|c| c.since_dialect.is_none() || c.recoverable.is_none())
            .count();
        assert!(
            none_rows <= LEGACY_NONE_ROWS,
            "{} rows lack lifecycle metadata, ceiling is {} — every NEW code must state since_dialect and recoverable",
            none_rows, LEGACY_NONE_ROWS
        );
    }

    #[test]
    fn lookup_finds_known_and_rejects_unknown() {
        assert_eq!(lookup("E010").unwrap().title, "Non-exhaustive match");
        assert!(lookup("E039").is_none(), "E039 is a gap in the incumbent's numbering");
        assert!(lookup("E999").is_none(), "E999 (internal) has no doc page and no row");
    }
}
