<!-- description: Write-once cross-platform support with transparent OS differences -->
# Cross-Platform Support

Almide is a write-once language — platform differences are the compiler's problem, never the user's.

## Principle

> If `random.int(1, 10)` works on macOS, it works on Windows. Period.
> If a heredoc produces `"line one\nline two"` on Linux, it produces the same on Windows. Period.

No `cfg` flags, no platform modules, no conditional imports. The compiler and runtime absorb all OS differences transparently.

## Implemented

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

**Fix**: `env.temp_dir()` returns normalized forward-slash paths on all platforms.

**Location**: `stdlib/defs/env.toml`, `src/emit_rust/platform_runtime.txt`

### `env.os()` ✅

**Problem**: No way to detect platform at runtime for platform-specific test commands.

**Fix**: `env.os()` returns `"macos"`, `"windows"`, `"linux"`, or `"unknown"`. Pure function (no `effect` needed in Rust). TS/JS: `Deno.build.os` / `require("os").platform()` normalized.

**Location**: `stdlib/defs/env.toml`, `src/emit_rust/platform_runtime.txt`, `src/emit_ts_runtime.rs`

### List literal ownership ✅

**Problem**: `vec![f1, f2]` moves variables, preventing reuse after the list literal. Common pattern in real code.

**Fix**: Emit `.clone()` for `Ident` expressions inside list literals.

**Location**: `src/emit_rust/expressions.rs`

### Path separator normalization ✅

**Decision**: Normalize to `/` everywhere — matching Deno and Go's `filepath.ToSlash`.

**Fix**: `fs.walk()` uses `.replace('\\', "/")` on paths. `env.temp_dir()` also normalizes. `fs.list_dir()` returns filenames only (no separator issue). `fs.stat()` operates on user-provided paths (no output normalization needed).

**Location**: `src/emit_rust/platform_runtime.txt`

### `fs.read_lines` universal newlines ✅

**Decision**: `fs.read_text` returns as-is (user's domain). `fs.read_lines` strips `\r` (like Python's universal newline mode).

**Fix**: Rust: `.trim_end_matches('\r')`. TS/JS: `.replace(/\r$/, "")`.

**Location**: `src/emit_rust/platform_runtime.txt`, `src/emit_ts_runtime.rs`

### Cross-platform test commands ✅

**Problem**: `process.exec_status("false", [])` fails on Windows (no `false` command).

**Fix**: Test uses `env.os()` to dispatch: `"windows"` → `cmd /c exit 1`, others → `false`.

**Location**: `stdlib/fs_process_test.almd`

### Windows CI ✅

Windows is in the full CI matrix (`ci.yml`), triggered on PRs to main. Covers: build, `cargo test`, `almide test` (all test files), smoke tests.

**Location**: `.github/workflows/ci.yml`

## Design decisions

| Decision | Rationale |
|---|---|
| Strip `\r` in lexer, not in file reader | Source semantics must be platform-independent. File I/O is the user's domain. |
| `BCryptGenRandom` over `RtlGenRandom` | BCrypt is the documented, stable Windows API. RtlGenRandom is undocumented (though widely used). |
| No `getrandom` crate dependency | Almide uses bare `rustc` (no Cargo) for `almide run`. External crates require Cargo, breaking the single-file workflow. |
| Clone in list literals (not move analysis) | Move analysis is Phase 1a of codegen-optimization. Until then, clone is correct and safe. |
| Normalize paths to `/` | Cross-platform consistency. Every modern tool does this (Deno, Go, Python pathlib). Users should never see `\`. |
| `fs.read_text` as-is, `fs.read_lines` strips `\r` | Reading raw bytes is the user's choice. Line-oriented API should behave like Python's universal newlines. |
| `env.os()` returns normalized names | `"macos"` not `"darwin"`, `"windows"` not `"win32"`. Human-readable, grep-friendly. |
