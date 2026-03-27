<!-- description: Move uuid, crypto, toml, compress, term out of stdlib to packages -->
<!-- done: 2026-03-20 -->
# Stdlib Scope Reduction — Complete

**Priority:** Before 1.0 — decide what to move out before freezing
**Research:** [stdlib-module-matrix.md](../../research/stdlib-module-matrix.md)

## Removal Candidates (stdlib -> first-party package)

Based on comparison with 1.0 stdlibs of other languages.

| Module | Current | Rationale | Action |
|--------|---------|-----------|--------|
| **uuid** | TOML 6 functions | Gleam/Elm/Rust/Kotlin/MoonBit/Elixir **all outside stdlib** | Remove. `crypto.random_hex` can substitute |
| **crypto** | TOML 4 functions | Rust/Kotlin/MoonBit/Elixir all outside stdlib. Only Go includes it | Remove. Too thin, freeze risk |
| **toml** | .almd 14 functions | **All languages have it outside stdlib** | Move to first-party package |
| **compress** | .almd 4 functions | All except Go have it outside stdlib. 4 functions is half-baked | Move to first-party package |
| **term** | .almd 21 functions | **All languages have it outside stdlib**. Does not work on TS target | Move to first-party package |

## Decision Criteria

1. **Included in other languages' 1.0 stdlib?** — If more than half exclude it, keep it outside stdlib
2. **Works multi-target?** — Must be meaningful for both Rust + TS
3. **Freeze risk** — Freezing an immature API leads to Go's log problem
4. **Alternatives** — Can other modules in stdlib substitute?

## Completed

- [x] uuid removed — TOML definitions, runtime (Rust/TS/JS) all removed
- [x] crypto removed — TOML definitions, runtime (Rust/TS/JS) all removed
- [x] toml, compress, term removed from bundled stdlib
- [x] Excluded from STDLIB_MODULES, PRELUDE_MODULES (uuid/crypto were not included)
- [x] FROZEN_API.md updated
- [x] SPEC.md updated (uuid/crypto/toml/compress/term removed from module list)
- [x] STDLIB-SPEC.md updated (crypto/uuid sections deleted, module index updated)
