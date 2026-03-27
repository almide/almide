<!-- description: Implement remaining filesystem operations for the WASM target -->
# WASM Remaining FS Operations

**Priority:** Medium — read_text/write/exists are implemented. Remaining operations added as needed
**Prerequisites:** WASI path_open, fd_read, fd_write, fd_close, fd_filestat_get, path_filestat_get registered

---

## Implemented

- [x] `fs.read_text(path)` — path_open → fd_filestat_get → fd_read → String construction
- [x] `fs.write(path, content)` — path_open(O_CREAT|O_TRUNC) → fd_write
- [x] `fs.exists(path)` — path_filestat_get → errno check
- [x] wasmtime `--dir=/` (root preopened) + WASI absolute path strip
- [x] top-level let dynamic initialization (`compile_init_globals`)
- [x] mutable collection operations: `list.push`, `list.pop`, `list.clear`, `map.insert`, `map.delete`, `map.clear`

## Not Yet Implemented (by priority)

### High
- [ ] `fs.list_dir(path)` — requires parsing fd_readdir, analyzing dirent structs
- [ ] `fs.mkdir_p(path)` — path_create_directory + recursive creation via path splitting
- [ ] `fs.remove(path)` — path_unlink_file

### Medium
- [ ] `fs.read_lines(path)` — composable via read_text + split("\n") (no compiler work needed, can be written in Almide)
- [ ] `fs.append(path, content)` — just change path_open oflags to O_APPEND
- [ ] `fs.rename(src, dst)` — path_rename WASI call
- [ ] `fs.copy(src, dst)` — composable via read_text + write

### Low
- [ ] `fs.read_bytes(path)` — similar to read_text (builds List[Int] instead of String)
- [ ] `fs.write_bytes(path, bytes)` — similar to write
- [ ] `fs.is_dir?(path)` / `fs.is_file?(path)` — flag analysis from path_filestat_get
- [ ] `fs.stat(path)` — convert fd_filestat_get result to Record

## Technical Notes

- Bump allocator has no deallocation — memory usage grows with heavy file operations
- fd_readdir dirent struct parsing is the most complex part (variable-length records)
- read_lines / copy can be composed in Almide code, so compiler-side implementation may not be necessary
