<!-- description: Backward compatibility policy, edition system, and API freeze -->
<!-- done: 2026-03-17 -->
# Stability Contract [DONE — 1.0 Phase II]

> Go 1 compatibility promise: "every Go program that compiles today compiles forever."
> Rust editions: syntax evolution without breaking existing code.
> Python 2→3: silent semantic changes nearly killed the language.

## Implemented

- [x] Added `edition = "2026"` field to `almide.toml`
- [x] `almide init` generates edition
- [x] Breaking change policy document: `docs/BREAKING_CHANGE_POLICY.md`
- [x] Core type API freeze audit: `docs/FROZEN_API.md` (string 41, int 19, float 16, list 54, map 16, result 9)
- [x] Rejected Patterns list: `docs/REJECTED_PATTERNS.md` (20+ items)
- [x] Hidden operations documented: `docs/HIDDEN_OPERATIONS.md` (clone, auto-?, Result erasure, runtime, fan)
