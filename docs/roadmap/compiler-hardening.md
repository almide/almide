# Compiler Hardening

Eliminate all panics and unhandled edge cases. Other languages never crash on invalid input — Almide shouldn't either.

### Panic elimination ✅

All `unwrap()`, `panic!()` calls in compiler source eliminated. Generated code uses `expect()` with descriptive messages.

- [x] Parser: `panic!("Parser: no tokens available")` → static EOF token fallback
- [x] Emitter: `.unwrap()` on character case conversion → `.unwrap_or(c)` (emit_ts/mod.rs)
- [x] Emitter: `final_expr.unwrap()` in do-block → `.expect("guarded by is_some()")` (emit_rust/blocks.rs)
- [x] Checker: `path.last().unwrap()` in import resolution → `.map().unwrap_or()` (check/mod.rs)
- [x] CLI: `unwrap()` on file I/O in init/build commands → proper `if let Err` with exit(1) (cli.rs)
- [x] Codegen: `/dev/urandom` direct read with `unwrap()` → `.map_err()?` propagation (random module)
- [x] Codegen: `UNIX_EPOCH` duration `.unwrap()` → `.unwrap_or_default()` (time/env modules)
- [x] Project: `.unwrap()` on split results → `.expect()` with reason (project.rs)
- [x] Generated code: thread spawn/join `.unwrap()` → `.expect()` with message (emit_rust/program.rs)

### Codegen `todo!()` fallbacks ✅

All 16 module fallbacks replaced with compile-time ICE (Internal Compiler Error) that exits with code 70 instead of silently generating broken Rust code.

- [x] Audit all `format!("/* {}.{} */ todo!()", ...)` patterns in emit_rust/calls.rs — 16 modules
- [x] Replace with `eprintln!("internal error: ...")` + `exit(70)` — catches mismatches immediately
- [x] Verified: all stdlib signatures in `lookup_sig()` have corresponding emitter implementations (no gap)

### Error message improvements

- [x] Import resolution failures: include file path tried and hint for typos (already excellent)
- [x] Effect fn called outside effect context: suggest adding `effect` keyword (already excellent)
- [x] Interpolated string validation at checker stage — parse and type-check `${expr}` in checker, report syntax errors early
- [x] Parser error hints: type name casing, function name casing, parameter name hints, pattern syntax guide

---
