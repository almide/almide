<!-- description: Production readiness checklist (11 stdlib runtimes, CI, packaging) -->
<!-- done: 2026-03-18 -->
# Production Readiness Requirements

## 1. 11 Stdlib Runtimes Not Implemented

**Status:** The following modules do not exist in `runtime/rust/src/`

| Module | TOML Definitions | Difficulty | Dependencies |
|--------|-----------------|------------|--------------|
| io | 3 | Low | stdin/stdout |
| log | 8 | Low | eprintln |
| random | 4 | Low | rand crate or std |
| uuid | 6 | Low | format |
| json | 36 | Medium | serde_json or hand-written parser (existing in value.rs) |
| regex | 8 | Medium | regex crate |
| datetime | 21 | Medium | chrono crate or std::time |
| fs | 24 | Medium | std::fs |
| http | 26 | Hard | reqwest or ureq |
| crypto | 4 | Hard | sha2/hmac crate |

**Fix approach:** All TOML definitions exist. Create `runtime/rust/src/<module>.rs` and add to include_str in `src/emit_rust/lower_rust.rs`
**Estimate:** io/log/random/uuid = 1 day, json/regex/datetime/fs = 3 days, http/crypto = 2 days

## 2. let-polymorphism Rust Codegen

**Status:** Checker passes `let f = (x) => x; f(1); f("hello")`. Rust closures are monomorphic so this causes compile error
**Fix approach:** Convert polymorphic let bindings to Rust generic functions

```rust
// Almide: let f = (x) => x
// Current: let f = move |x| x;  (monomorphic)
// Goal: fn f<T>(x: T) -> T { x }  (generic)
```

**Condition:** Only when let binding type has unresolved TypeVar. Monomorphic bindings remain as closures
**Estimate:** 2-3 days

## 3. WASM target

**Status:** CLI has `--target wasm` but not verified to work
**Fix approach:** Generate .wasm with `almide build app.almd --target wasm`. wasm-pack or rustc --target wasm32
**Estimate:** 1-2 days (just pass to rustc if Rust codegen is correct)
