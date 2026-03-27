<!-- description: Unify Rust and TS runtime file layout and management -->
<!-- done: 2026-03-17 -->
# Runtime Layout Unification

## Problem

Rust and TypeScript runtimes are managed in separate locations with separate formats.

```
Current:
  runtime/rust/src/*.rs        Rust runtime (plain .rs, embedded via include_str!)
  src/emit_ts_runtime/*.rs     TS runtime (TS code held as Rust const string literals)
```

- TS side has TS code embedded as string literals inside `.rs` files
- IDE syntax highlighting and completion do not work
- Cannot write tests (since it is a string)
- Asymmetric structure between Rust/TS

## Goal

```
runtime/
├── rust/src/*.rs       Rust runtime (no changes)
└── ts/
    ├── string.ts       TS runtime (plain .ts)
    ├── list.ts
    ├── map.ts
    ├── int.ts
    ├── float.ts
    ├── math.ts
    ├── json.ts
    ├── result.ts
    ├── io.ts
    ├── net.ts
    └── ...
```

- Both targets unified under `runtime/{lang}/`
- TS runtime becomes plain `.ts` files, enabling IDE support and unit testing
- Compiler reads via `include_str!("../../runtime/ts/string.ts")`

## Migration Steps

1. Extract each `const MOD_*_TS: &str = r#"..."#` from `src/emit_ts_runtime/core.rs` into `runtime/ts/*.ts`
2. Same for `src/emit_ts_runtime/collections.rs`
3. Same for `src/emit_ts_runtime/data.rs`
4. Same for `src/emit_ts_runtime/io.rs`
5. Same for `src/emit_ts_runtime/net.rs`
6. Modify `src/emit_ts_runtime/mod.rs`: read via `include_str!("../../runtime/ts/*.ts")`
7. Delete Rust const definitions from `src/emit_ts_runtime/*.rs`
8. Keep only `mod.rs` (runtime assembly logic) under `src/emit_ts_runtime/`
9. Verify TS/JS tests pass in Cross-Target CI

## Notes

- Rust side already has the correct structure in `runtime/rust/src/`. No changes needed
- When extracting to TS files, just unescape `r#"..."#` (the code itself is the same)
- Enables writing unit tests for `runtime/ts/` with Deno (future)
