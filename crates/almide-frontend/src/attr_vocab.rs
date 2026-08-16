//! E053: an attribute the compiler does not read.
//!
//! Attributes used to be silently dropped. `@deprecatd(since = 3, ...)` — one
//! transposed letter — parsed, checked, compiled, and told nobody, while the
//! author believed callers were being warned. For a language whose writers are
//! largely models, a construct that accepts anything and reports nothing is
//! the worst possible surface: there is no signal to learn from.
//!
//! This is a WARNING, not an error, on purpose. Rejecting outright would break
//! every file carrying an attribute from some older tool, and the language now
//! has the machinery to do that properly instead: promote it to an error at a
//! future dialect epoch, with `proofs/dialect-epochs.toml` recording the break
//! and E052-style migration telling each caller what to write.

use almide_base::diagnostic::Diagnostic;
use almide_lang::ast;

/// Every attribute name the compiler reads somewhere. Adding an attribute
/// means adding it here — the list is what makes "unknown" mean anything.
///
/// Ordered by area so a reader can tell what each group is for; the lookup
/// is a linear scan over a handful of entries and never shows up in a
/// profile.
pub const KNOWN_ATTRS: &[&str] = &[
    // Foreign bodies and symbols
    "extern",
    "export",
    "intrinsic",
    "wasm_intrinsic",
    "inline_rust",
    // Ownership / calling convention
    "mutating",
    "consume",
    "borrow_ref",
    "pure",
    // Scheduling and placement
    "schedule",
    "location",
    // Compiler-internal marks
    "derived",
    "main",
    // The stability surface
    "deprecated",
    "dialect",
];

/// Attributes the grammar accepts and NO code reads.
///
/// Kept out of `KNOWN_ATTRS` so the distinction stays visible, and kept out
/// of the warning so the existing corpus is not noisy — but written down,
/// because "accepted and inert" is exactly the state this module exists to
/// make impossible to hold accidentally.
///
/// `@rewrite` (7 uses in `stdlib/matrix.almd`) reads as a declaration of a
/// fusion rule; the rules are in fact implemented imperatively in
/// `MatrixFusionPass`, and nothing parses the attribute. Found by this
/// check's first run over the corpus (almide#1450). Either the pass grows a
/// reader or the attributes come out; until one happens, this row is the
/// honest record.
pub const DECLARED_BUT_UNREAD: &[&str] = &["rewrite"];

/// Closest known attribute by edit distance, for the did-you-mean. `None`
/// when nothing is close enough to be worth suggesting — a wrong suggestion
/// is worse than none, because it reads as authoritative.
fn nearest(name: &str) -> Option<&'static str> {
    let mut best: Option<(usize, &'static str)> = None;
    for cand in KNOWN_ATTRS {
        let d = edit_distance(name, cand);
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, cand));
        }
    }
    // A third of the name may differ; beyond that the "suggestion" is noise.
    best.filter(|(d, _)| *d * 3 <= name.len().max(3)).map(|(_, c)| c)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let sub = prev[j - 1] + usize::from(a[i - 1] != b[j - 1]);
            cur[j] = sub.min(prev[j] + 1).min(cur[j - 1] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

pub fn check_attrs(attrs: &[ast::Attribute], diagnostics: &mut Vec<Diagnostic>) {
    for attr in attrs {
        let name = attr.name.as_str();
        if KNOWN_ATTRS.contains(&name) || DECLARED_BUT_UNREAD.contains(&name) {
            continue;
        }
        let hint = match nearest(name) {
            Some(c) => format!(
                "Did you mean `@{c}`? An attribute the compiler does not read is dropped, \
                 so nothing it was meant to do happens."
            ),
            None => format!(
                "Almide reads these attributes: {}. An attribute outside that list is dropped, \
                 so nothing it was meant to do happens.",
                KNOWN_ATTRS.join(", ")
            ),
        };
        let mut diag =
            Diagnostic::warning(format!("unknown attribute `@{name}`"), hint, format!("@{name}"))
                .with_code("E053");
        if let Some(s) = &attr.span {
            diag.line = Some(s.line);
            diag.col = Some(s.col);
        }
        diagnostics.push(diag);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use almide_base::intern::sym;

    fn attrs(names: &[&str]) -> Vec<ast::Attribute> {
        names
            .iter()
            .map(|n| ast::Attribute { name: sym(n), args: vec![], span: None })
            .collect()
    }

    fn diags(names: &[&str]) -> Vec<Diagnostic> {
        let mut d = Vec::new();
        check_attrs(&attrs(names), &mut d);
        d
    }

    #[test]
    fn every_known_attribute_is_silent() {
        assert!(diags(KNOWN_ATTRS).is_empty(), "a known attribute must never warn");
    }

    /// The two lists must stay disjoint: an attribute cannot be both read and
    /// unread, and a name drifting into both would make the second list stop
    /// meaning anything.
    #[test]
    fn the_read_and_unread_vocabularies_are_disjoint() {
        for a in DECLARED_BUT_UNREAD {
            assert!(!KNOWN_ATTRS.contains(a), "`{a}` is in both vocabularies");
        }
        assert!(diags(DECLARED_BUT_UNREAD).is_empty(), "an accepted-but-inert attribute must not warn");
    }

    #[test]
    fn an_unknown_attribute_warns() {
        let d = diags(&["totally_made_up_thing"]);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].code.as_deref(), Some("E053"));
    }

    /// The case this exists for: one transposed letter in a deprecation,
    /// which used to compile silently while telling no caller anything.
    #[test]
    fn a_near_miss_suggests_the_real_one() {
        let d = diags(&["deprecatd"]);
        assert_eq!(d.len(), 1);
        assert!(d[0].hint.contains("`@deprecated`"), "hint was: {}", d[0].hint);
    }

    /// A wrong suggestion reads as authoritative, so distant names get the
    /// vocabulary instead of a guess.
    fn suggests_nothing(name: &str) -> bool {
        !diags(&[name])[0].hint.contains("Did you mean")
    }

    #[test]
    fn a_distant_name_gets_the_vocabulary_not_a_guess() {
        assert!(suggests_nothing("qqqqqqqqqqqq"));
        assert!(diags(&["qqqqqqqqqqqq"])[0].hint.contains("deprecated"));
    }

    #[test]
    fn each_unknown_attribute_is_reported_once() {
        assert_eq!(diags(&["one_bogus", "another_bogus", "deprecated"]).len(), 2);
    }
}
