# Cross-Platform Support Specification

Almide is a write-once language. Platform differences are handled by the compiler and runtime, never by the user.

## Principles

1. **Source semantics are platform-independent.** The same `.almd` file produces the same behavior on macOS, Linux, and Windows.
2. **No platform conditionals in user code.** There is no `cfg`, no `#ifdef` required for basic operations. `env.os()` is available for edge cases (e.g., platform-specific shell commands) but should rarely be needed.
3. **The runtime absorbs OS differences.** Stdlib functions like `random.int()`, `fs.read_text()`, `env.temp_dir()` work identically everywhere.

## Source File Handling

### Line ending normalization

The lexer strips all `\r` characters from source input before tokenizing.

```
Input:  "let x = 1\r\nlet y = 2\r\n"
Lexed:  "let x = 1\nlet y = 2\n"
```

This means:
- Heredoc literals produce `\n` regardless of the source file's line endings
- String literals never contain accidental `\r`
- Token positions (line/col) are consistent across platforms

**Rationale:** Every modern language (Go, Rust, Python, Swift) normalizes at the lexer level. Git's `autocrlf` setting on Windows would otherwise silently change program behavior.

### File I/O is not normalized

`fs.read_text()` and `fs.write()` pass content as-is. If a file on disk contains `\r\n`, the program receives `\r\n`. This is intentional — file I/O is the user's domain.

`fs.read_lines()` strips trailing `\r` from each line, matching Python's universal newline behavior.

## Random Number Generation

`random.int()`, `random.float()`, `random.choice()`, and `random.shuffle()` use OS-native cryptographic random sources:

| Platform | API | Header/Library |
|----------|-----|----------------|
| Unix (macOS, Linux) | `/dev/urandom` | None (filesystem) |
| Windows | `BCryptGenRandom` | `bcrypt.dll` (system) |

The dispatch is compile-time via Rust's `#[cfg(unix)]` / `#[cfg(windows)]` in the generated runtime. No external crates are required — this is critical because `almide run` uses bare `rustc` without Cargo.

### Why not `getrandom` crate?

Almide's `run` command compiles with `rustc` directly (no Cargo, no `Cargo.toml`). External crate dependencies would break the single-file workflow. The BCrypt API is stable, documented, and available on all supported Windows versions (Vista+).

## Build Output

### Automatic `.exe` extension on Windows

`almide build app.almd -o myapp` produces:
- macOS/Linux: `myapp`
- Windows: `myapp.exe`

If the user explicitly writes `-o myapp.exe`, it is respected as-is.

The temporary `.rs` file strips `.exe` (and `.wasm`) from the filename to avoid invalid Rust crate names.

## Temporary Directory

`env.temp_dir()` returns the platform-appropriate temp directory:

| Platform | Typical value |
|----------|---------------|
| macOS | `/var/folders/.../T/` |
| Linux | `/tmp` |
| Windows | `C:\Users\<user>\AppData\Local\Temp` |

Stdlib tests and exercises use `env.temp_dir()` instead of hardcoding `/tmp`.

## CLI Behavior

### `almide run` argument passing

All arguments after the source file are passed to the program:

```bash
almide run app.almd check config.toml    # "check" and "config.toml" go to the program
```

No `--` separator is required. Hyphenated arguments like `-v` are also forwarded.

### Error exit behavior

When `effect fn main` returns `err(...)`, the program exits with code 1 silently. The error value is not printed to stderr — CLI tools are expected to print their own error messages via `println` before returning.

## Path Separators

All path-returning functions normalize to forward slashes (`/`) on all platforms:
- `fs.walk()` — recursive directory listing, paths use `/`
- `env.temp_dir()` — returns `/`-normalized temp directory path
- `fs.list_dir()` — returns filenames only (no separator issue)
- `fs.stat()` — operates on user-provided paths

This matches the convention of Deno and Go's `filepath.ToSlash`. Users should never see `\` in path outputs.

## Platform Detection

`env.os()` returns a normalized platform name:

| Platform | Return value |
|----------|-------------|
| macOS | `"macos"` |
| Linux | `"linux"` |
| Windows | `"windows"` |
| Other | `"unknown"` |

Available in all targets (Rust, TS/Deno, JS/Node). Not an `effect` function — no I/O involved.

## CI

Windows CI runs on `windows-latest` (GitHub Actions) and covers:
- `cargo build --release` — compiler builds on MSVC toolchain
- `cargo test` — Rust unit tests pass
- `almide test` — all 43 `.almd` test files pass
- Smoke tests: `almide run`, `almide build`, fs operations
