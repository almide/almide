# almide-base

Foundation crate. Every other crate depends on this.

## Provides

- **`Sym`** — `Copy` handle into a global thread-safe string interner (`lasso::ThreadedRodeo`). All identifiers, type names, field names throughout the compiler are `Sym`.
- **`Span`** — Source location (line, col, end_col).
- **`Diagnostic`** — Compiler error/warning with context, hints, and Levenshtein-based "did you mean?" suggestions.

## Rules

- **Never compare identifiers by string.** Use `Sym` equality (`==`). Equality is O(1) (compares interned IDs). `Sym::Ord` (`<`, `>`) intentionally compares by string content for deterministic ordering — the walker sorts record fields alphabetically for stable Rust struct layout.
- **`sym(s)` to intern, `.as_str()` to read.** The `resolve()` function returns `&'static str` — safe because the interner never deallocates.
- **Diagnostics must be actionable.** Every error should include a hint or suggestion when possible. Use `Diagnostic::suggest(name, candidates)` for fuzzy matching.
- **Do not add runtime-heavy dependencies here.** This crate must compile fast — it's in every crate's dependency chain.
