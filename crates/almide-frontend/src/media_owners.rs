//! Media-type ownership (ADR-0032 D4, D7): which media types a `Body` type
//! owns, and which are script-capable. Everything here is read off
//! declarations, never off a list in this file:
//!
//! - `@media_owns(patterns…)` on a type's media fn says the type owns those
//!   patterns, so `http.typed` cannot label bytes as one of them;
//! - `@script_capable(patterns…)` in `stdlib/html.almd` is the one
//!   declaration of the types a browser runs script in. Owning one of those
//!   outside the stdlib needs the application's grant (`[trust] media`).
//!
//! Patterns follow RFC 6839: `*` as the type is any type, and a subtype
//! `*<suffix>` is any longer subtype ending in `<suffix>` (`*/*+xml`).
//! Shared by the checker (E117, E118) and the driver's grant survey.

use crate::ast;

/// The attribute that declares a type's owned media patterns.
pub const MEDIA_OWNS_ATTR: &str = "media_owns";
/// The attribute that declares the script-capable media patterns.
pub const SCRIPT_CAPABLE_ATTR: &str = "script_capable";
/// The protocol whose types are response bodies, and its requirement that
/// states a body type's media type, by the name a type outside the stdlib
/// writes (`fn T.media_type(self)`, ADR-0032 Q24).
pub const BODY_PROTOCOL: &str = "Body";
pub const MEDIA_REQUIREMENT: &str = "media_type";
/// The module holding the `@script_capable` declaration.
const SCRIPT_CAPABLE_MODULE: &str = "html";

/// One `@media_owns` pattern: the pattern (lowercased), the fn declaring it,
/// and the line it is declared on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owned {
    pub pattern: String,
    pub func: String,
    pub line: usize,
}

/// Every `@media_owns` pattern declared in `program`, in declaration order.
pub fn owns_in(program: &ast::Program) -> Vec<Owned> {
    attr_strings(program, MEDIA_OWNS_ATTR)
}

fn attr_strings(program: &ast::Program, attr: &str) -> Vec<Owned> {
    let mut out = Vec::new();
    for d in &program.decls {
        let ast::Decl::Fn { name, attrs, span, .. } = d else { continue };
        for a in attrs.iter().filter(|a| a.name.as_str() == attr) {
            for arg in &a.args {
                if let ast::AttrValue::String { value } = &arg.value {
                    out.push(Owned { pattern: value.to_ascii_lowercase(), func: name.to_string(), line: span.map_or(0, |s| s.line) });
                }
            }
        }
    }
    out
}

/// The stdlib's owners: every `@media_owns` pattern across the bundled
/// modules, with its owner as `module.fn`.
pub fn stdlib_owners() -> &'static [(String, String)] {
    static TABLE: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut out = Vec::new();
        for &module in almide_lang::stdlib_info::BUNDLED_MODULES {
            let Some(program) = almide_lang::stdlib_info::bundled_source(module).and_then(almide_lang::parse_cached) else { continue };
            out.extend(owns_in(program).into_iter().map(|o| (o.pattern, format!("{module}.{}", o.func))));
        }
        out
    })
}

/// The script-capable patterns, from `stdlib/html.almd`'s `@script_capable`.
pub fn script_capable() -> &'static [String] {
    static TABLE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        almide_lang::stdlib_info::bundled_source(SCRIPT_CAPABLE_MODULE)
            .and_then(almide_lang::parse_cached)
            .map(|p| attr_strings(p, SCRIPT_CAPABLE_ATTR).into_iter().map(|o| o.pattern).collect())
            .unwrap_or_default()
    })
}

/// Can a browser run script in some media type `pattern` matches?
pub fn is_script_capable(pattern: &str) -> bool {
    script_capable().iter().any(|a| overlap(pattern, a))
}

/// A media type (its essence, lowercased) against one pattern.
pub fn matches(t: &str, pattern: &str) -> bool {
    let (Some((tt, ts)), Some((pt, ps))) = (t.split_once('/'), pattern.split_once('/')) else { return false };
    let subtype = match ps.strip_prefix('*') {
        Some(suffix) => ts.len() > suffix.len() && ts.ends_with(suffix),
        None => ps == ts,
    };
    (pt == "*" || pt == tt) && subtype
}

/// Can one media type match both patterns? Two owners whose patterns
/// overlap would both own that type (ADR-0032 Q22).
pub fn overlap(a: &str, b: &str) -> bool {
    let (Some((at, asub)), Some((bt, bsub))) = (a.split_once('/'), b.split_once('/')) else { return false };
    let types = at == "*" || bt == "*" || at == bt;
    let subtypes = match (asub.strip_prefix('*'), bsub.strip_prefix('*')) {
        (None, None) => asub == bsub,
        (Some(s), None) => bsub.len() > s.len() && bsub.ends_with(s),
        (None, Some(s)) => asub.len() > s.len() && asub.ends_with(s),
        // Two suffixes share a longer subtype exactly when one ends with the other.
        (Some(s), Some(t)) => s.ends_with(t) || t.ends_with(s),
    };
    types && subtypes
}

/// An RFC 9110 `token` character.
fn is_tchar(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c)
}

/// Is `pattern` a well-formed owned pattern: `type/subtype`, `*` as the
/// type, or `*<suffix>` as the subtype?
pub fn well_formed_pattern(pattern: &str) -> bool {
    let Some((t, s)) = pattern.split_once('/') else { return false };
    let token = |x: &str| !x.is_empty() && x.chars().all(|c| is_tchar(c) && c != '*');
    (t == "*" || token(t)) && (s == "*" || token(s.strip_prefix('*').unwrap_or(s)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_follows_the_patterns() {
        assert!(overlap("image/svg+xml", "*/*+xml"));
        assert!(overlap("*/*+xml", "image/svg+xml"));
        assert!(overlap("*/*+xml", "*/*ml"));
        assert!(!overlap("*/*+xml", "*/*+json"));
        assert!(!overlap("text/html", "*/*+xml"));
        assert!(overlap("text/html", "text/html"));
        assert!(!overlap("text/csv", "text/html"));
        assert!(overlap("*/xml", "application/xml"));
        assert!(!overlap("*/xml", "*/*+xml"));
    }

    #[test]
    fn the_script_capable_set_is_read_from_html() {
        for t in ["text/html", "application/xhtml+xml", "image/svg+xml", "application/xml", "application/atom+xml"] {
            assert!(is_script_capable(t), "{t}");
        }
        assert!(!is_script_capable("text/csv"));
        assert!(!is_script_capable("application/json"));
    }
}
