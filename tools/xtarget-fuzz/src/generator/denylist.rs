//! The single source of truth for the effectful / nondeterministic
//! surface the fuzzer must never let into a differential comparison.
//!
//! Two generation paths consume this list:
//!
//!   - **Synthesis** ([`catalogue`](super::catalogue)) excludes effectful
//!     modules by *not bundling* them, and additionally drops the
//!     nondeterministic functions named here so the type-directed
//!     generator never emits them.
//!   - **Mutation** ([`mutate`](super::mutate)) screens every corpus file
//!     against [`NamedDenylist::source_references_denied_surface`] before
//!     admitting it, so a runnable corpus program that calls `fs.*`,
//!     `process.*`, `http.*`, a clock, RNG, or `fan.timeout` can never be
//!     mutated into a spurious "divergence" (the two targets legitimately
//!     differ run-to-run, or one cannot perform the effect at all).
//!
//! Keeping both paths derived from this one table is the point: when the
//! effectful surface grows, only this file changes, and the two paths
//! stay consistent by construction.

/// The denied surface, named so every entry documents *why* it is denied.
pub struct NamedDenylist;

/// Module name-prefixes whose functions are **effectful or
/// nondeterministic as a whole**: their result depends on the host
/// environment (filesystem, network, clock, process table, OS RNG) or
/// they perform side effects one target may not be able to replay. A
/// generated program touching any of these cannot be byte-compared
/// across targets, so the whole module is denied.
///
/// This mirrors the synthesis path's choice to omit these modules from
/// `BUNDLED_MODULES` — the two are the same decision, expressed once.
const DENIED_MODULES: &[&str] = &[
    "fs",       // filesystem reads/writes — host state
    "process",  // subprocess exec, exit codes — host state
    "http",     // network I/O — host + remote state
    "net",      // raw sockets — host + remote state
    "io",       // stdin and other stream I/O — host state
    "env",      // environment variables — host state
    "clock",    // wall-clock time — nondeterministic
    "datetime", // current time / now() — nondeterministic
    "random",   // RNG — nondeterministic by definition
];

/// Specific `(module, function)` pairs that are nondeterministic even
/// though their *module* is otherwise in-universe and deterministic.
/// The synthesis catalogue's denylist is derived from exactly this table
/// (see [`NamedDenylist::nondeterministic_functions`]), so the two paths
/// never drift.
///
/// `fan` is a language-level concurrency construct (`fan { … }`,
/// `fan.map`, `fan.race`, `fan.any`, `fan.timeout`) rather than a bundled
/// stdlib module; its scheduling-order-dependent combinators are
/// nondeterministic and are denied here for the mutation path.
const NONDETERMINISTIC_FUNCTIONS: &[(&str, &str)] = &[
    ("list", "shuffle"), // randomized permutation — not input-determined
    ("list", "sample"),  // randomized subset — not input-determined
    ("fan", "race"),     // first-to-finish — scheduling-order-dependent
    ("fan", "any"),      // first-success — scheduling-order-dependent
    ("fan", "timeout"),  // wall-clock deadline — nondeterministic
];

/// Bare tokens that name a nondeterministic operation regardless of the
/// receiver, used as a belt-and-braces source screen for the mutation
/// path (catches `xs.shuffle()` UFCS-style or any future re-export). Kept
/// in sync with [`NONDETERMINISTIC_FUNCTIONS`]'s function names.
const NONDETERMINISTIC_TOKENS: &[&str] = &["shuffle", "sample"];

impl NamedDenylist {
    /// `true` if `(module, func)` is a denied nondeterministic function.
    pub fn is_denied_function(module: &str, func: &str) -> bool {
        NONDETERMINISTIC_FUNCTIONS
            .iter()
            .any(|(m, f)| *m == module && *f == func)
    }

    /// Screen a corpus file's **source text** for any reference to the
    /// denied surface. Returns the first matched surface name (for a
    /// triage log) or `None` if the file is clean.
    ///
    /// The match is on call-prefix form `module.` so an effectful module
    /// used as a *receiver* (`fs.read_text(…)`, `http.serve(…)`,
    /// `fan.timeout(…)`) is caught, while an incidental substring in a
    /// comment or string literal that is not a call prefix is not. The
    /// bare nondeterministic tokens are matched on a word boundary to
    /// catch method-call / re-export forms.
    pub fn source_references_denied_surface(src: &str) -> Option<&'static str> {
        for &m in DENIED_MODULES {
            if contains_call_prefix(src, m) {
                return Some(m);
            }
        }
        // Fan: deny only the nondeterministic combinators (`fan.race(`,
        // `fan.any(`, `fan.timeout(`), NOT `fan.map` / `fan.settle` / a
        // bare `fan { }` block — those are deterministic and a valid
        // mutation seed.
        for &(m, f) in NONDETERMINISTIC_FUNCTIONS {
            if m == "fan" && contains_token_call(src, &format!("{m}.{f}")) {
                return Some("fan");
            }
        }
        for &tok in NONDETERMINISTIC_TOKENS {
            if contains_word(src, tok) {
                return Some(tok);
            }
        }
        None
    }
}

/// `true` if `src` contains `<module>.` used as a real call/access
/// prefix: the module name (clean left boundary) immediately followed by
/// a dot and then an identifier-start byte — i.e. a member access like
/// `fs.read_text`, not `prefs.` (embedded) nor a prose sentence ending
/// in the word (`"... the Nth in a process."`, where the dot is followed
/// by a space/newline). Requiring the dot to lead into an identifier
/// keeps an incidental comment mention from costing a good mutation seed.
fn contains_call_prefix(src: &str, module: &str) -> bool {
    let needle = format!("{module}.");
    let bytes = src.as_bytes();
    let mut from = 0;
    while let Some(rel) = src[from..].find(&needle) {
        let at = from + rel;
        let after = at + needle.len();
        // Left boundary: not part of a longer identifier (`prefs.`).
        let ok_left = at == 0 || !is_ident_byte(bytes[at - 1]);
        // Right of the dot: an identifier-start byte ⇒ a member access.
        let ok_right = bytes.get(after).is_some_and(|&b| is_ident_start_byte(b));
        if ok_left && ok_right {
            return true;
        }
        from = at + 1;
    }
    false
}

/// `true` if `src` contains the exact `<prefix>(` call form with a clean
/// left boundary (used for `fan.race(`, `fan.timeout(`, …).
fn contains_token_call(src: &str, prefix: &str) -> bool {
    let needle = format!("{prefix}(");
    let bytes = src.as_bytes();
    let mut from = 0;
    while let Some(rel) = src[from..].find(&needle) {
        let at = from + rel;
        let ok_left = at == 0 || !is_ident_byte(bytes[at - 1]);
        if ok_left {
            return true;
        }
        from = at + 1;
    }
    false
}

/// `true` if `tok` appears in `src` as a whole identifier word
/// (boundaries on both sides are non-identifier bytes), so `shuffle`
/// matches `xs.shuffle()` and `list.shuffle(` but not `reshuffled`.
fn contains_word(src: &str, tok: &str) -> bool {
    let bytes = src.as_bytes();
    let mut from = 0;
    while let Some(rel) = src[from..].find(tok) {
        let at = from + rel;
        let end = at + tok.len();
        let ok_left = at == 0 || !is_ident_byte(bytes[at - 1]);
        let ok_right = end >= bytes.len() || !is_ident_byte(bytes[end]);
        if ok_left && ok_right {
            return true;
        }
        from = at + 1;
    }
    false
}

/// `true` if `b` can appear inside an Almide identifier.
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// `true` if `b` can *start* an Almide identifier (a member name after a
/// `.`). Excludes digits so `1.5` is not read as a `1.` member access.
fn is_ident_start_byte(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denies_effectful_call_prefixes() {
        assert!(NamedDenylist::source_references_denied_surface("fs.read_text(p)").is_some());
        assert!(NamedDenylist::source_references_denied_surface("http.serve(3000, h)").is_some());
        assert!(NamedDenylist::source_references_denied_surface("process.exec(c)").is_some());
        assert!(NamedDenylist::source_references_denied_surface("net.connect(a)").is_some());
        assert!(NamedDenylist::source_references_denied_surface("env.get(\"X\")").is_some());
        assert!(NamedDenylist::source_references_denied_surface("datetime.now()").is_some());
        assert!(NamedDenylist::source_references_denied_surface("random.int(0, 9)").is_some());
    }

    #[test]
    fn denies_nondeterministic_functions_and_tokens() {
        assert!(NamedDenylist::source_references_denied_surface("list.shuffle(xs)").is_some());
        assert!(NamedDenylist::source_references_denied_surface("xs.sample(3)").is_some());
        assert!(NamedDenylist::source_references_denied_surface("fan.timeout(100, t)").is_some());
        assert!(NamedDenylist::source_references_denied_surface("fan.race(a, b)").is_some());
    }

    #[test]
    fn allows_clean_and_deterministic_surface() {
        // No denied surface: pure string/list/float work.
        assert!(NamedDenylist::source_references_denied_surface(
            "let r = string.to_upper(\"hi\")\nlist.map(xs, f)"
        )
        .is_none());
        // Deterministic fan forms stay allowed (not race/any/timeout).
        assert!(NamedDenylist::source_references_denied_surface("fan.map(xs, f)").is_none());
        assert!(NamedDenylist::source_references_denied_surface("fan { a(); b() }").is_none());
        // Substrings inside longer identifiers must NOT match.
        assert!(NamedDenylist::source_references_denied_surface("let prefs = 1\nreshuffled").is_none());
        assert!(NamedDenylist::source_references_denied_surface("let fsx = io_free()").is_none());
        // A prose sentence ending in a denied module word (dot followed by
        // whitespace, not an identifier) must NOT match — an incidental
        // comment mention should not cost a good mutation seed.
        assert!(NamedDenylist::source_references_denied_surface(
            "// the Nth in a process. Then continue.\nlet r = list.map(xs, f)"
        )
        .is_none());
    }

    #[test]
    fn function_predicate_agrees_with_table() {
        assert!(NamedDenylist::is_denied_function("list", "shuffle"));
        assert!(!NamedDenylist::is_denied_function("list", "map"));
    }
}
