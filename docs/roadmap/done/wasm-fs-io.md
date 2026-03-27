<!-- description: WASI-based filesystem I/O for WASM target -->
<!-- done: 2026-03-25 -->
# WASM Filesystem I/O

**Completed:** 2026-03-25

## Implementation

Implemented file I/O for the WASM target via WASI preview1.

### Added WASI Imports
- `path_open` — open a file
- `fd_read` — read from a file
- `fd_close` — close a file descriptor
- `fd_seek` — seek within a file
- `fd_filestat_get` — get file metadata
- `path_filestat_get` — get metadata from a path

### Implemented stdlib Functions
- `fs.read_text(path)` → `Result[String, String]`
- `fs.write(path, content)` → `Result[Unit, String]`
- `fs.exists(path)` → `Bool`

### Other
- Added `--dir=.` to wasmtime to permit filesystem access
- Aligned bump allocator to 8 bytes (prevents traps on i64 load/store)
- Implemented `env.unix_timestamp()`, `env.millis()` via WASI `clock_time_get`
- Implemented `env.args()` via WASI `args_sizes_get`/`args_get`

## Remaining → [active/wasm-remaining-fs.md](../active/wasm-remaining-fs.md)
