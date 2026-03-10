# Cross-Platform Support [IN PROGRESS]

Almide must be a write-once language — platform differences are the compiler's problem, never the user's.

## Principle

> If `random.int(1, 10)` works on macOS, it works on Windows. Period.
> If a heredoc produces `"line one\nline two"` on Linux, it produces the same on Windows. Period.

No `cfg` flags, no platform modules, no conditional imports. The compiler and runtime absorb all OS differences transparently.

## Done

### CRLF normalization in lexer ✅

**Problem**: Source files checked out with `\r\n` (Windows git default) produced different string literals than `\n` files. Heredoc tests failed on Windows CI because `\r` leaked into string values.

**Fix**: `lexer.rs` strips all `\r` from source input before tokenizing. Every modern language does this (Go, Rust, Python, Swift). Source file line endings never affect program semantics.

**Location**: `src/lexer.rs` — `tokenize()` function

### Cross-platform random ✅

**Problem**: Runtime used `/dev/urandom` directly — does not exist on Windows.

**Fix**: `almide_rt_fill_random_bytes()` helper with `#[cfg]` dispatch:
- Unix: `/dev/urandom`
- Windows: `BCryptGenRandom` (Win API, no external crates)

All random functions (`random.int`, `random.float`, `random.choice`, `random.shuffle`) route through this single helper.

**Location**: `src/emit_rust/platform_runtime.txt`

### `env.temp_dir()` ✅

**Problem**: Tests hardcoded `/tmp` which doesn't exist on Windows.

**Fix**: Added `env.temp_dir()` stdlib function → `std::env::temp_dir()` in Rust codegen.

**Location**: `src/stdlib.rs`, `src/emit_rust/calls.rs`

### List literal ownership ✅

**Problem**: `vec![f1, f2]` moves variables, preventing reuse after the list literal. Common pattern in real code.

**Fix**: Emit `.clone()` for `Ident` expressions inside list literals.

**Location**: `src/emit_rust/expressions.rs`

## Remaining

### `process.exec_status("false", [])` on Windows

The `false` command doesn't exist on Windows. Test files using it will fail. Options:
- [ ] Skip platform-specific tests via `env.os()` guard
- [ ] Add `env.os()` to stdlib (returns `"windows"`, `"macos"`, `"linux"`)
- [ ] Use a cross-platform command in tests (e.g., `process.exec_status("cmd", ["/c", "exit", "1"])`)

### Path separator normalization

`fs.walk()` returns paths with `\` on Windows, `/` on Unix. Should Almide normalize to `/` everywhere? Most modern tools do (Deno, Go's `filepath.ToSlash`).

- [ ] Decide: normalize in runtime vs expose `path.separator`
- [ ] If normalizing: update `fs.walk`, `fs.stat`, `fs.list_dir` to use `/`

### Line ending in `fs.read_text` / `fs.read_lines`

Files written by other programs on Windows may contain `\r\n`. Should `fs.read_text` normalize?

- [ ] `fs.read_text`: return as-is (user decides)
- [ ] `fs.read_lines`: strip `\r` from each line (like Python's universal newline mode)

### CI

- [ ] Windows CI green on `ci/windows-test` branch
- [ ] Add Windows to main CI matrix once stable

## Design decisions

| Decision | Rationale |
|---|---|
| Strip `\r` in lexer, not in file reader | Source semantics must be platform-independent. File I/O is the user's domain. |
| `BCryptGenRandom` over `RtlGenRandom` | BCrypt is the documented, stable Windows API. RtlGenRandom is undocumented (though widely used). |
| No `getrandom` crate dependency | Almide uses bare `rustc` (no Cargo) for `almide run`. External crates require Cargo, breaking the single-file workflow. |
| Clone in list literals (not move analysis) | Move analysis is Phase 1a of codegen-optimization. Until then, clone is correct and safe. |
