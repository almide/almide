[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> Non-normative, source-pinned human reader copy of repository agent instructions. The authoritative CLAUDE.md remains unchanged. This CLAUDE.reader.md family deliberately does not use an auto-loaded instruction filename. These are historical source instructions, not a new instruction layer or independently verified current implementation/security guarantees; no automatic synchronization. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-base/CLAUDE.md)

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
