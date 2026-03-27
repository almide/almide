<!-- description: Reorganize tests into spec/ (lang/stdlib/integration) and tests/ -->
<!-- done: 2026-03-12 -->
# Test Directory Structure Redesign

Resolved the problem of test-related files being scattered across root directories (`lang/`, `stdlib/`, `exercises/`, `tests/`).

## Final Structure

```
spec/                  ← Almide language tests (almide test spec/)
├── lang/              Language feature tests (expressions, variables, functions, patterns, types, scope, error handling)
├── stdlib/            Stdlib module tests (string, list, map, int, float, math, json, regex, ...)
└── integration/       Multi-file / system integration tests (generics, modules, extern)

tests/                 ← Rust compiler tests (cargo test, Cargo auto-discovery)
├── lexer_test.rs
├── parser_test.rs
├── checker_test.rs
└── ...

stdlib/                ← Source only (no more mixed-in tests)
├── defs/*.toml
├── args.almd
└── (no tests)
```

## Why `spec/` + `tests/`

- `tests/` follows Cargo convention. Auto-discovery works without needing per-file `[[test]]` entries
- `spec/` is clearly distinct from `tests/`. Avoids the confusion of `test/` and `tests/` side by side
- At the root, "where are the tests?" — Rust: `tests/`, Almide: `spec/`

## Command Mapping

```bash
almide test                    # All .almd tests (recursive search)
almide test spec/lang/         # Language tests
almide test spec/stdlib/       # Stdlib tests
almide test spec/integration/  # Integration tests
cargo test                     # Rust compiler tests
```

## Migration Log

| Step | What | Status |
|------|------|--------|
| 1 | `lang/*_test.almd` → `spec/lang/` | done |
| 2 | `stdlib/*_test.almd` → `spec/stdlib/` | done |
| 3 | `exercises/{generics,mod,extern}-test/` → `spec/integration/` | done |
| 4 | Rust tests: keep in `tests/` (Cargo auto-discovery) | done |
| 5 | Remove `autotests = false` + `[[test]]` from `Cargo.toml` | done |
| 6 | Update test paths in `CLAUDE.md` | done |
| 7 | Delete empty directories (`lang/`, `test/`) | done |
