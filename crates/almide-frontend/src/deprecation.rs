//! `@deprecated` — the migration channel.
//!
//! When the language moves, a generator that learned the old spelling keeps
//! writing it. A diagnostic that only says "no such function" costs a whole
//! retry to guess the fix from; one that names the replacement is a repair
//! instruction the model can apply in a single edit. That difference is the
//! mission metric, so the deprecation carries the replacement and the
//! compiler classifies the edit rather than making the reader do it.
//!
//! ```almide
//! @deprecated(since = 3, use = "string.trim_start")
//! pub fn trim_left(s: String) -> String = string.trim_start(s)
//! ```
//!
//! `since` is the DIALECT EPOCH the deprecation landed in
//! (`proofs/dialect-epochs.toml`), not a release — the same currency a file's
//! `@dialect(N)` stamp records, so a stale file and the reason it is stale are
//! denominated the same way.

use almide_lang::ast;

/// A parsed `@deprecated` attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deprecation {
    /// Dialect epoch this deprecation landed in.
    pub since: u32,
    /// The replacement's fully-qualified name, when there is one. `None` is a
    /// removal with no successor, which is legal and must say why in `note`.
    pub use_instead: Option<String>,
    /// Free-text reason, required when there is no replacement.
    pub note: Option<String>,
}

/// Why the attribute could not be used. Reported as an error at the
/// DECLARATION: a deprecation the compiler cannot read is worse than none,
/// because the author believes callers are being told something.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeprecationError {
    MissingSince,
    NonIntegerSince,
    NeitherUseNorNote,
    UnknownArg(String),
}

impl DeprecationError {
    pub fn message(&self) -> String {
        match self {
            Self::MissingSince => "`@deprecated` needs `since = N`, the dialect epoch it landed in".into(),
            Self::NonIntegerSince => "`@deprecated`'s `since` must be an integer dialect epoch".into(),
            Self::NeitherUseNorNote => {
                "`@deprecated` needs either `use = \"replacement\"` or `note = \"why\"`".into()
            }
            Self::UnknownArg(k) => format!("`@deprecated` has no argument `{k}`"),
        }
    }

    pub fn hint(&self) -> String {
        match self {
            Self::MissingSince | Self::NonIntegerSince => {
                "Write the epoch from proofs/dialect-epochs.toml, e.g. `@deprecated(since = 3, use = \"string.trim_start\")`. \
                 The epoch is what lets a caller's `@dialect(N)` stamp be compared against this deprecation."
                    .into()
            }
            Self::NeitherUseNorNote => {
                "A deprecation with no replacement and no reason tells a caller nothing it can act on. \
                 Name the successor with `use = \"...\"`, or say why it is going away with `note = \"...\"`."
                    .into()
            }
            Self::UnknownArg(_) => "`@deprecated` takes `since`, `use` and `note`.".into(),
        }
    }
}

/// Read a `@deprecated` attribute off a declaration's attribute list.
/// `Ok(None)` means the declaration is simply not deprecated.
pub fn parse(attrs: &[ast::Attribute]) -> Result<Option<Deprecation>, DeprecationError> {
    let Some(attr) = attrs.iter().find(|a| a.name.as_str() == "deprecated") else {
        return Ok(None);
    };
    let mut since = None;
    let mut use_instead = None;
    let mut note = None;
    for arg in &attr.args {
        match arg.name.as_ref().map(|n| n.as_str()) {
            Some("since") => match &arg.value {
                ast::AttrValue::Int { value } if *value >= 0 => since = Some(*value as u32),
                _ => return Err(DeprecationError::NonIntegerSince),
            },
            Some("use") => match &arg.value {
                ast::AttrValue::String { value } => use_instead = Some(value.clone()),
                _ => return Err(DeprecationError::UnknownArg("use".into())),
            },
            Some("note") => match &arg.value {
                ast::AttrValue::String { value } => note = Some(value.clone()),
                _ => return Err(DeprecationError::UnknownArg("note".into())),
            },
            Some(other) => return Err(DeprecationError::UnknownArg(other.to_string())),
            None => return Err(DeprecationError::UnknownArg("<positional>".into())),
        }
    }
    let Some(since) = since else { return Err(DeprecationError::MissingSince) };
    if use_instead.is_none() && note.is_none() {
        return Err(DeprecationError::NeitherUseNorNote);
    }
    Ok(Some(Deprecation { since, use_instead, note }))
}

/// How much work the caller's edit is. DERIVED by comparing the two
/// signatures rather than declared, so it cannot rot the way lean4's
/// hand-written `+typeChanged` marker can: a replacement whose type drifts
/// later re-classifies itself on the next compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    /// Same signature — swapping the name is the whole edit, and `almide fix`
    /// can do it.
    Rename,
    /// The signature differs; the arguments need looking at.
    SignatureChanged,
    /// The replacement is not a function this compiler knows (a different
    /// module's, or simply misspelled in the attribute).
    ReplacementUnknown,
}

impl EditKind {
    pub fn describe(self) -> &'static str {
        match self {
            Self::Rename => "same signature — the name is the whole edit",
            Self::SignatureChanged => "the signature differs — check the arguments",
            Self::ReplacementUnknown => "the replacement is not visible from here",
        }
    }

    /// Only a same-signature rename is safe to hand to `almide fix`.
    pub fn is_mechanical(self) -> bool {
        matches!(self, Self::Rename)
    }
}

pub fn classify(
    old: Option<&almide_lang::types::FnSig>,
    new: Option<&almide_lang::types::FnSig>,
) -> EditKind {
    match (old, new) {
        (Some(o), Some(n)) => {
            let same = o.params.len() == n.params.len()
                && o.params.iter().zip(&n.params).all(|((_, a), (_, b))| a == b)
                && o.ret == n.ret
                && o.is_effect == n.is_effect;
            if same { EditKind::Rename } else { EditKind::SignatureChanged }
        }
        (_, None) => EditKind::ReplacementUnknown,
        (None, Some(_)) => EditKind::SignatureChanged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use almide_base::intern::sym;
    use almide_lang::types::{FnSig, Ty};

    fn attr(args: Vec<(Option<&str>, ast::AttrValue)>) -> Vec<ast::Attribute> {
        vec![ast::Attribute {
            name: sym("deprecated"),
            args: args
                .into_iter()
                .map(|(n, value)| ast::AttrArg { name: n.map(sym), value })
                .collect(),
            span: None,
        }]
    }

    fn s(v: &str) -> ast::AttrValue {
        ast::AttrValue::String { value: v.to_string() }
    }
    fn i(v: i64) -> ast::AttrValue {
        ast::AttrValue::Int { value: v }
    }

    #[test]
    fn a_well_formed_attribute_parses() {
        let got = parse(&attr(vec![(Some("since"), i(3)), (Some("use"), s("string.trim_start"))]));
        assert_eq!(
            got,
            Ok(Some(Deprecation {
                since: 3,
                use_instead: Some("string.trim_start".into()),
                note: None
            }))
        );
    }

    #[test]
    fn a_declaration_without_the_attribute_is_not_deprecated() {
        assert_eq!(parse(&[]), Ok(None));
    }

    /// Every rejection the type promises, asserted. A deprecation the
    /// compiler silently ignores is worse than none: the author believes
    /// callers are being warned.
    #[test]
    fn every_malformed_shape_is_rejected() {
        assert_eq!(
            parse(&attr(vec![(Some("use"), s("x"))])),
            Err(DeprecationError::MissingSince)
        );
        assert_eq!(
            parse(&attr(vec![(Some("since"), s("3")), (Some("use"), s("x"))])),
            Err(DeprecationError::NonIntegerSince)
        );
        assert_eq!(
            parse(&attr(vec![(Some("since"), i(3))])),
            Err(DeprecationError::NeitherUseNorNote)
        );
        assert_eq!(
            parse(&attr(vec![(Some("since"), i(3)), (Some("replacement"), s("x"))])),
            Err(DeprecationError::UnknownArg("replacement".into()))
        );
        assert_eq!(
            parse(&attr(vec![(None, s("x"))])),
            Err(DeprecationError::UnknownArg("<positional>".into()))
        );
    }

    #[test]
    fn a_note_alone_is_enough_for_a_removal_with_no_successor() {
        let got = parse(&attr(vec![(Some("since"), i(3)), (Some("note"), s("unsound"))]));
        assert!(matches!(got, Ok(Some(Deprecation { use_instead: None, .. }))));
    }

    fn sig(params: Vec<Ty>, ret: Ty, is_effect: bool) -> FnSig {
        FnSig {
            params: params.into_iter().enumerate().map(|(i, t)| (sym(&format!("p{i}")), t)).collect(),
            ret,
            is_effect,
            generics: vec![],
            structural_bounds: Default::default(),
            protocol_bounds: Default::default(),
            mut_params: vec![],
        }
    }

    #[test]
    fn an_identical_signature_is_a_mechanical_rename() {
        let a = sig(vec![Ty::String], Ty::String, false);
        let b = sig(vec![Ty::String], Ty::String, false);
        assert_eq!(classify(Some(&a), Some(&b)), EditKind::Rename);
        assert!(classify(Some(&a), Some(&b)).is_mechanical());
    }

    /// Parameter names differ but types do not — still mechanical. Naming a
    /// parameter differently changes nothing at a call site.
    #[test]
    fn parameter_names_do_not_make_an_edit_non_mechanical() {
        let mut b = sig(vec![Ty::String], Ty::String, false);
        b.params[0].0 = sym("completely_different");
        assert_eq!(classify(Some(&sig(vec![Ty::String], Ty::String, false)), Some(&b)), EditKind::Rename);
    }

    #[test]
    fn a_differing_signature_is_never_mechanical() {
        let a = sig(vec![Ty::String], Ty::String, false);
        for b in [
            sig(vec![Ty::String, Ty::Int], Ty::String, false),
            sig(vec![Ty::Int], Ty::String, false),
            sig(vec![Ty::String], Ty::Int, false),
            sig(vec![Ty::String], Ty::String, true),
        ] {
            assert_eq!(classify(Some(&a), Some(&b)), EditKind::SignatureChanged);
            assert!(!classify(Some(&a), Some(&b)).is_mechanical());
        }
    }

    #[test]
    fn an_unknown_replacement_is_never_mechanical() {
        let a = sig(vec![Ty::String], Ty::String, false);
        assert_eq!(classify(Some(&a), None), EditKind::ReplacementUnknown);
        assert!(!classify(Some(&a), None).is_mechanical());
    }
}
