//! Curated cross-language misspelling map.
//!
//! Greenfield evolution E1, adopted from Roc's `common_misspellings.zig`
//! (roc@707a8082, src/reporting/common_misspellings.zig:10-87) after the
//! 9-compiler diagnostics survey: edit distance can never map `switch` to
//! `match` or `&&` to `and` — only a hand-written catalogue can, and "an LLM
//! writes another language's spelling" is Almide's core failure mode.
//!
//! Every entry is grounded in the incumbent's normative surface
//! (docs/CHEATSHEET.md "Common mistakes" / "Common mistakes from other
//! languages" and llms.txt fast facts at almide@a877d2138) — nothing here is
//! invented. Consumers: the tokenizer (unit 2) for token entries and the
//! resolver (unit 4) for identifier entries. `machine_fix` marks the entries
//! that are pure re-spellings (one reading, same meaning) and may therefore
//! be attached with `with_machine_fix` under the #1312 discipline; everything
//! else is hint-only.

/// Where the wrong spelling appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An operator or punctuation token (`&&`, `::`, `<T>`).
    Token,
    /// A keyword-position word (`def`, `switch`, `async`).
    Keyword,
    /// A type-position name (`int`, `Vec`, `HashMap`).
    Type,
    /// A call-position name (`foldLeft`, `length`).
    Function,
}

#[derive(Debug, Clone, Copy)]
pub struct Misspelling {
    pub wrong: &'static str,
    pub kind: Kind,
    /// The Almide spelling, or the construct to reach for. Only a drop-in
    /// replacement when `machine_fix` is true.
    pub right: &'static str,
    /// One-sentence hint, phrased for the `hint:` row.
    pub hint: &'static str,
    /// True only when `right` is a pure re-spelling of `wrong` — same value,
    /// same reading — and may be applied unattended.
    pub machine_fix: bool,
}

macro_rules! m {
    ($wrong:literal, $kind:ident, $right:literal, $hint:literal, $mf:literal) => {
        Misspelling { wrong: $wrong, kind: Kind::$kind, right: $right, hint: $hint, machine_fix: $mf }
    };
}

/// The catalogue. Exact-match, case-sensitive lookups (a lowercase `int` is a
/// mistake; an uppercase `Int` is correct — casing is signal, not noise).
pub static MISSPELLINGS: &[Misspelling] = &[
    // ── tokens ──────────────────────────────────────────────────────────
    m!("&&", Token, "and", "Almide spells logical AND as `and`, not `&&`.", true),
    m!("||", Token, "or", "Almide spells logical OR as `or`, not `||`.", true),
    m!("===", Token, "==", "Almide has one equality operator: `==`.", true),
    m!("!==", Token, "!=", "Almide has one inequality operator: `!=`.", true),
    m!("::", Token, ".", "Almide uses `.` for module access (`list.len`), and has no cons operator — write `[x] + xs`.", false),
    m!("<T>", Token, "[T]", "Generics use square brackets: `fn foo[T](x: T)`, `List[Int]`.", false),
    // ── keywords ────────────────────────────────────────────────────────
    m!("def", Keyword, "fn", "Functions are declared with `fn`.", true),
    m!("func", Keyword, "fn", "Functions are declared with `fn`.", true),
    m!("function", Keyword, "fn", "Functions are declared with `fn`.", true),
    m!("lambda", Keyword, "(x) => ...", "Anonymous functions are `(x) => expr` — parameters always in parentheses.", false),
    m!("switch", Keyword, "match", "Almide uses `match` with `pattern => expr` arms.", false),
    m!("case", Keyword, "match", "There is no `case`; write `match x { pattern => expr }`.", false),
    m!("elif", Keyword, "else if", "Almide spells it `else if`.", true),
    m!("const", Keyword, "let", "Immutable bindings are `let`; mutable bindings are `var`.", true),
    m!("mut", Keyword, "var", "`let mut` is not Almide — use `var x = ...`. `mut` exists only as a parameter modifier.", false),
    m!("async", Keyword, "fan", "Almide has no async/await; use the deterministic `fan.*` block forms.", false),
    m!("await", Keyword, "fan", "Almide has no async/await; `fan { ... }` joins structurally.", false),
    m!("return", Keyword, "(last expression)", "There is no `return` — a block's last expression is its value; use `guard cond else err(...)` for early exit.", false),
    m!("break", Keyword, "(recursion)", "There is no `break`; use a recursive helper that stops when done.", false),
    m!("continue", Keyword, "(recursion)", "There is no `continue`; use a recursive helper or `list.filter`.", false),
    m!("throw", Keyword, "err(...)", "Errors are values: return `err(e)`; `!` propagates.", false),
    m!("raise", Keyword, "err(...)", "Errors are values: return `err(e)`; `!` propagates.", false),
    m!("try", Keyword, "!", "There is no try/catch — `expr!` propagates, `match` branches on `ok`/`err`.", false),
    m!("catch", Keyword, "match", "There is no try/catch — `match` on the `Result` (`ok(v)` / `err(e)`).", false),
    m!("except", Keyword, "match", "There is no try/except — `match` on the `Result` (`ok(v)` / `err(e)`).", false),
    m!("class", Keyword, "type", "There are no classes; declare data with `type`, behavior with `protocol` + `fn Type.method`.", false),
    m!("struct", Keyword, "type", "Records are declared with `type`, e.g. `type P = { x: Int, y: Int }`.", true),
    m!("enum", Keyword, "type", "Sum types are `type T = | A(...) | B(...)`.", true),
    m!("interface", Keyword, "protocol", "Almide's ad-hoc polymorphism is `protocol`.", true),
    m!("trait", Keyword, "protocol", "Almide is not Rust: use `protocol`, not `trait`.", true),
    m!("impl", Keyword, "fn Type.method", "There is no `impl` block — define convention methods: `fn Type.method(...)`.", false),
    m!("use", Keyword, "import", "Modules are brought in with `import`.", true),
    m!("pub", Keyword, "", "There is no `pub` — module interfaces are governed by the module system, not per-item keywords.", false),
    m!("null", Keyword, "none", "Almide has no null: use `Option[T]` (`some(v)` / `none`), with `??` for fallback.", false),
    m!("nil", Keyword, "none", "Almide has no nil: use `Option[T]` (`some(v)` / `none`).", false),
    m!("undefined", Keyword, "none", "Almide has no undefined: use `Option[T]` (`some(v)` / `none`).", false),
    m!("None", Keyword, "none", "Almide's empty Option is lowercase `none`.", true),
    m!("Some", Keyword, "some", "Almide's Option constructor is lowercase `some(v)`.", true),
    m!("True", Keyword, "true", "Booleans are lowercase: `true`.", true),
    m!("False", Keyword, "false", "Booleans are lowercase: `false`.", true),
    m!("foreach", Keyword, "for", "Iteration is `for x in xs { ... }` — or better, `list.map` / `list.filter`.", false),
    // ── types ───────────────────────────────────────────────────────────
    m!("int", Type, "Int", "Type names are capitalized: `Int`.", true),
    m!("i64", Type, "Int", "The default integer type is `Int` (64-bit).", true),
    m!("float", Type, "Float", "Type names are capitalized: `Float`.", true),
    m!("double", Type, "Float", "Almide's floating type is `Float`.", true),
    m!("f64", Type, "Float", "Almide's floating type is `Float`.", true),
    m!("str", Type, "String", "Almide's string type is `String`.", true),
    m!("string", Type, "String", "Type names are capitalized: `String` (`string` is the module).", true),
    m!("bool", Type, "Bool", "Type names are capitalized: `Bool`.", true),
    m!("boolean", Type, "Bool", "Almide's boolean type is `Bool`.", true),
    m!("char", Type, "String", "Almide has no Char type — single characters are `String`, e.g. \"a\".", true),
    m!("Char", Type, "String", "Almide has no Char type — single characters are `String`, e.g. \"a\".", true),
    m!("void", Type, "Unit", "The no-value type is `Unit`.", true),
    m!("Vec", Type, "List", "Almide's sequence type is `List[T]`.", true),
    m!("Array", Type, "List", "Almide's sequence type is `List[T]`.", true),
    m!("array", Type, "List", "Almide's sequence type is `List[T]`.", true),
    m!("HashMap", Type, "Map", "Almide's key-value type is `Map` (literal: `[\"a\": 1]`, empty: `[:]`).", true),
    m!("Dict", Type, "Map", "Almide's key-value type is `Map`.", true),
    m!("dict", Type, "Map", "Almide's key-value type is `Map`.", true),
    m!("HashSet", Type, "Set", "Almide's set type is `Set`.", true),
    // ── functions ───────────────────────────────────────────────────────
    m!("foldLeft", Function, "list.fold", "Folding is `list.fold(xs, init, (acc, x) => ...)`.", true),
    m!("foldRight", Function, "list.fold", "Folding is `list.fold` (left fold); reverse first if order matters.", false),
    m!("foldl", Function, "list.fold", "Folding is `list.fold(xs, init, (acc, x) => ...)`.", true),
    m!("reduce", Function, "list.fold", "Folding is `list.fold(xs, init, (acc, x) => ...)`.", true),
    m!("print", Function, "println", "Output is `println(s)` — strings only; convert with `int.to_string(x)` first.", true),
    m!("printf", Function, "println", "Use `println(\"${x}\")` — string interpolation replaces format strings.", false),
    m!("console.log", Function, "println", "Output is `println(s)`.", true),
    m!("puts", Function, "println", "Output is `println(s)`.", true),
    m!("length", Function, "len", "It is `len`, no synonyms: `string.len(s)`, `list.len(xs)`.", true),
    m!("substring", Function, "string.slice", "Substrings are `string.slice(s, i, j)`.", true),
    m!("to_lowercase", Function, "string.to_lower", "It is `string.to_lower(s)`, no synonyms.", true),
    m!("to_uppercase", Function, "string.to_upper", "It is `string.to_upper(s)`, no synonyms.", true),
    m!("push", Function, "xs + [item]", "For value building use `xs + [item]`; `list.push` mutates a `var` and returns `Unit`.", false),
    m!("append", Function, "xs + [item]", "List building is `xs + [item]` (or `a + b` for concatenation).", false),
];

/// Exact-match lookup. Case-sensitive on purpose: `Int` is correct while
/// `int` is a mistake, so folding case would eat the signal.
pub fn lookup(wrong: &str) -> Option<&'static Misspelling> {
    MISSPELLINGS.iter().find(|m| m.wrong == wrong)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn no_duplicate_wrong_spellings() {
        let mut seen = HashSet::new();
        for m in MISSPELLINGS {
            assert!(seen.insert(m.wrong), "duplicate entry: {}", m.wrong);
        }
    }

    #[test]
    fn entries_are_well_formed() {
        for m in MISSPELLINGS {
            assert!(!m.wrong.is_empty());
            assert_ne!(m.wrong, m.right, "{} maps to itself", m.wrong);
            assert!(m.hint.ends_with('.'), "hint for {} is not a sentence", m.wrong);
            if m.machine_fix {
                // A machine fix must be a drop-in token, never a placeholder
                // or an empty deletion-by-omission.
                assert!(!m.right.is_empty(), "{}: machine fix with empty right", m.wrong);
                assert!(!m.right.contains("..."), "{}: machine fix with placeholder", m.wrong);
            }
        }
    }

    #[test]
    fn lookup_is_exact_and_case_sensitive() {
        assert_eq!(lookup("&&").unwrap().right, "and");
        assert_eq!(lookup("int").unwrap().right, "Int");
        assert!(lookup("Int").is_none(), "correct spellings must not match");
        assert!(lookup("match").is_none(), "correct spellings must not match");
        assert!(lookup("znork").is_none());
    }

    #[test]
    fn correct_almide_spellings_never_appear_as_wrong() {
        // The catalogue must never flag valid Almide surface.
        for ok in ["fn", "match", "let", "var", "protocol", "import", "Int", "Float",
                   "String", "Bool", "Unit", "List", "Map", "Set", "and", "or", "not",
                   "some", "none", "ok", "err", "guard", "fan", "true", "false", "effect"] {
            assert!(lookup(ok).is_none(), "valid spelling {} is in the catalogue", ok);
        }
    }
}
