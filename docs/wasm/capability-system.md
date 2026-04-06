# Capability System

Compile-time enforcement of least privilege for WASM agent containers.

Almide's capability system is a three-layer defense that guarantees an agent binary cannot perform operations it was not explicitly granted. Layer 1 (the compiler) rejects unauthorized calls before any binary is produced. Layer 2 (WASM import pruning) physically removes unauthorized host functions from the binary. Layer 3 (WASI runtime) scopes filesystem access to pre-opened directories. No single layer can be circumvented without the other two also failing.

---

## 13 Capability Categories

| Bit | Capability | Summary |
|-----|------------|---------|
| 0 | `FS.read` | Read files and query filesystem metadata |
| 1 | `FS.write` | Create, modify, and delete files and directories |
| 2 | `Net.fetch` | Outbound HTTP requests |
| 3 | `Net.listen` | Bind and accept inbound connections |
| 4 | `Env.read` | Read environment variables, args, cwd, OS info |
| 5 | `Env.write` | Set environment variables |
| 6 | `Proc` | Execute child processes, exit |
| 7 | `Time` | Read system clocks |
| 8 | `Rand` | Access cryptographic/pseudo-random bytes |
| 9 | `Fan` | Structured concurrency (fan blocks, fan.map, fan.race) |
| 10 | `IO.stdin` | Read from standard input |
| 11 | `IO.stdout` | Write to standard output |
| 12 | `IO.stderr` | Write to standard error |

Internal representation: `CapabilitySet = u16` (13 bits, bits 13-15 reserved for future use).

Shorthand groups: `IO` = FS.read + FS.write + IO.stdin + IO.stdout + IO.stderr, `Net` = Net.fetch + Net.listen, `All` = all 13.

---

## Category Details

### Bit 0: `FS.read` -- Read Filesystem

**Stdlib functions requiring this capability:**

| Function | Signature | Description |
|----------|-----------|-------------|
| `fs.read_text(path)` | `(String) -> Result[String, String]` | Read file as UTF-8 string |
| `fs.read_bytes(path)` | `(String) -> Result[List[Int], String]` | Read file as byte list |
| `fs.read_lines(path)` | `(String) -> Result[List[String], String]` | Read file as list of lines |
| `fs.exists(path)` | `(String) -> Bool` | Check if path exists |
| `fs.is_dir(path)` | `(String) -> Bool` | Check if path is directory |
| `fs.is_file(path)` | `(String) -> Bool` | Check if path is regular file |
| `fs.is_symlink(path)` | `(String) -> Bool` | Check if path is symbolic link |
| `fs.stat(path)` | `(String) -> Result[{size, is_dir, is_file, modified}, String]` | File metadata |
| `fs.file_size(path)` | `(String) -> Result[Int, String]` | File size in bytes |
| `fs.modified_at(path)` | `(String) -> Result[Int, String]` | Modification timestamp |
| `fs.list_dir(path)` | `(String) -> Result[List[String], String]` | List directory entries |
| `fs.walk(dir)` | `(String) -> Result[List[String], String]` | Recursive file listing |
| `fs.glob(pattern)` | `(String) -> Result[List[String], String]` | Glob pattern matching |
| `fs.temp_dir()` | `() -> String` | System temp directory path |

**WASI imports:**

| Import | Signature | Role |
|--------|-----------|------|
| `path_open` | `(fd, dirflags, path_ptr, path_len, oflags, fs_rights_base, fs_rights_inheriting, fdflags, fd_out) -> errno` | Open file for reading (rights: `fd_read`) |
| `fd_read` | `(fd, iovs_ptr, iovs_len, nread_ptr) -> errno` | Read bytes from file descriptor |
| `fd_close` | `(fd) -> errno` | Close opened file descriptor |
| `fd_seek` | `(fd, offset, whence, newoffset_ptr) -> errno` | Seek within file (for stat/size) |
| `fd_filestat_get` | `(fd, buf_ptr) -> errno` | Get metadata from open FD |
| `path_filestat_get` | `(fd, flags, path_ptr, path_len, buf_ptr) -> errno` | Get metadata by path |
| `fd_readdir` | `(fd, buf_ptr, buf_len, cookie, bufused_ptr) -> errno` | Read directory entries |

**Real-world example:** A code review agent reads source files, parses them, and produces a report. It needs `FS.read` to access the repository but has no reason to modify anything.

**Without this capability:** The agent cannot read any file from the filesystem. It can only operate on data passed to it via stdin or hardcoded into its binary. Any call to `fs.read_text`, `fs.exists`, etc. is a compile error.

---

### Bit 1: `FS.write` -- Write Filesystem

**Stdlib functions requiring this capability:**

| Function | Signature | Description |
|----------|-----------|-------------|
| `fs.write(path, content)` | `(String, String) -> Result[Unit, String]` | Write string to file |
| `fs.write_bytes(path, bytes)` | `(String, List[Int]) -> Result[Unit, String]` | Write bytes to file |
| `fs.append(path, content)` | `(String, String) -> Result[Unit, String]` | Append string to file |
| `fs.mkdir_p(path)` | `(String) -> Result[Unit, String]` | Create directory tree |
| `fs.remove(path)` | `(String) -> Result[Unit, String]` | Delete a file |
| `fs.remove_all(path)` | `(String) -> Result[Unit, String]` | Recursively delete directory |
| `fs.rename(src, dst)` | `(String, String) -> Result[Unit, String]` | Rename/move file |
| `fs.copy(src, dst)` | `(String, String) -> Result[Unit, String]` | Copy file |
| `fs.create_temp_file(prefix)` | `(String) -> Result[String, String]` | Create temp file |
| `fs.create_temp_dir(prefix)` | `(String) -> Result[String, String]` | Create temp directory |

**WASI imports:**

| Import | Signature | Role |
|--------|-----------|------|
| `path_open` | (same as FS.read) | Open file for writing (rights: `fd_write`) |
| `fd_write` | `(fd, iovs_ptr, iovs_len, nwritten_ptr) -> errno` | Write bytes (fd > 2, i.e. not stdout/stderr) |
| `fd_close` | (same as FS.read) | Close written file descriptor |
| `path_create_directory` | `(fd, path_ptr, path_len) -> errno` | Create directory |
| `path_rename` | `(old_fd, old_path_ptr, old_path_len, new_fd, new_path_ptr, new_path_len) -> errno` | Rename file |
| `path_unlink_file` | `(fd, path_ptr, path_len) -> errno` | Delete file |
| `path_remove_directory` | `(fd, path_ptr, path_len) -> errno` | Delete directory |

**Real-world example:** A code generation agent writes generated source files, creates directory structures, and cleans up temporary build artifacts.

**Without this capability:** The agent can read existing files but cannot create, modify, or delete anything. A refactoring agent that can only suggest changes (via stdout) but never write them directly -- useful for dry-run/preview modes.

---

### Bit 2: `Net.fetch` -- Outbound HTTP

**Stdlib functions requiring this capability:**

| Function | Signature | Description |
|----------|-----------|-------------|
| `http.get(url)` | `(String) -> Result[String, String]` | HTTP GET |
| `http.post(url, body)` | `(String, String) -> Result[String, String]` | HTTP POST |
| `http.put(url, body)` | `(String, String) -> Result[String, String]` | HTTP PUT |
| `http.patch(url, body)` | `(String, String) -> Result[String, String]` | HTTP PATCH |
| `http.delete(url)` | `(String) -> Result[String, String]` | HTTP DELETE |
| `http.request(method, url, body, headers)` | `(String, String, String, Map[String, String]) -> Result[String, String]` | Custom HTTP request |

Note: Pure response builder functions (`http.response`, `http.json`, `http.redirect`, `http.with_headers`) and request accessor functions (`http.req_method`, `http.req_path`, `http.req_body`, `http.req_header`, `http.query_params`) do not require `Net.fetch` because they operate on in-memory data structures, not the network.

**WASI imports:**

| Import | Source | Role |
|--------|--------|------|
| `almide_host_fetch` | `almide_host` module | Delegate HTTP request to host runtime |

Networking is not part of WASI snapshot preview 1. Almide uses a host-provided `almide_host_fetch` import that the embedding runtime (wasmtime, browser, etc.) must supply. This means the host runtime is the final gatekeeper: even if the binary contains the import, the host can refuse to provide it.

**Real-world example:** An API integration agent that fetches data from external services, posts webhooks, or calls LLM APIs.

**Without this capability:** The agent is completely air-gapped from the network. It cannot make any outbound HTTP requests. Useful for agents that should process only local data.

---

### Bit 3: `Net.listen` -- Inbound Connections

**Stdlib functions requiring this capability:**

| Function | Signature | Description |
|----------|-----------|-------------|
| `http.serve(port, handler)` | `(Int, (Request) -> Response) -> Unit` | Start HTTP server |

**WASI imports:**

| Import | Source | Role |
|--------|--------|------|
| `almide_host_listen` | `almide_host` module | Bind port and accept connections |

Like `Net.fetch`, server binding is a host-provided capability outside WASI preview 1.

**Real-world example:** An agent that exposes an HTTP API for tool-use integration -- other agents or systems call it to trigger actions.

**Without this capability:** The agent cannot bind any port or accept incoming connections. It can still make outbound requests (with `Net.fetch`), but nothing can call into it over the network.

---

### Bit 4: `Env.read` -- Read Environment

**Stdlib functions requiring this capability:**

| Function | Signature | Description |
|----------|-----------|-------------|
| `env.get(name)` | `(String) -> Option[String]` | Read environment variable |
| `env.args()` | `() -> List[String]` | Command-line arguments |
| `env.cwd()` | `() -> Result[String, String]` | Current working directory |
| `env.os()` | `() -> String` | Operating system name |
| `env.temp_dir()` | `() -> String` | System temp directory |
| `process.args()` | `() -> List[String]` | Command-line arguments (process module) |

**WASI imports:**

| Import | Signature | Role |
|--------|-----------|------|
| `environ_get` | `(environ_ptr, environ_buf_ptr) -> errno` | Read all environment variables |
| `environ_sizes_get` | `(count_ptr, buf_size_ptr) -> errno` | Get env var count and buffer size |
| `args_get` | `(argv_ptr, argv_buf_ptr) -> errno` | Read command-line arguments |
| `args_sizes_get` | `(argc_ptr, argv_buf_size_ptr) -> errno` | Get argument count and buffer size |

**Real-world example:** An agent that reads `API_KEY` from the environment to authenticate with external services, or reads `--verbose` from command-line arguments to adjust its output level.

**Without this capability:** The agent cannot read any environment variables or command-line arguments. It must receive all configuration through stdin or files. This prevents credential leakage -- an untrusted agent cannot read `AWS_SECRET_ACCESS_KEY` or `GITHUB_TOKEN` from the environment.

---

### Bit 5: `Env.write` -- Write Environment

**Stdlib functions requiring this capability:**

| Function | Signature | Description |
|----------|-----------|-------------|
| `env.set(name, value)` | `(String, String) -> Unit` | Set environment variable |

**WASI imports:**

| Import | Source | Role |
|--------|--------|------|
| `environ_set` | Extension (not in WASI preview 1) | Set environment variable |

Setting environment variables is not part of WASI preview 1. The host must provide this as an extension.

**Real-world example:** An agent that sets `PATH` or `LD_LIBRARY_PATH` before spawning child processes.

**Without this capability:** The agent cannot modify the environment. It operates in a read-only environment context. Combined with denying `Env.read`, the agent is completely isolated from environment state.

---

### Bit 6: `Proc` -- Process Execution

**Stdlib functions requiring this capability:**

| Function | Signature | Description |
|----------|-----------|-------------|
| `process.exec(cmd, args)` | `(String, List[String]) -> Result[String, String]` | Execute command, return stdout |
| `process.exec_in(dir, cmd, args)` | `(String, String, List[String]) -> Result[String, String]` | Execute in working directory |
| `process.exec_with_stdin(cmd, args, input)` | `(String, String, List[String], String) -> Result[String, String]` | Execute with stdin pipe |
| `process.exec_status(cmd, args)` | `(String, List[String]) -> Result[{code, stdout, stderr}, String]` | Execute, return full result |
| `process.exit(code)` | `(Int) -> Never` | Exit process |
| `process.stdin_lines()` | `() -> Result[List[String], String]` | Read all stdin lines |

**WASI imports:**

| Import | Source | Role |
|--------|--------|------|
| `almide_host_proc` | `almide_host` module | Spawn child process |
| `proc_exit` | `wasi_snapshot_preview1` | Terminate with exit code |

Process execution is the most dangerous capability. An agent with `Proc` can execute arbitrary shell commands, which can bypass all other capability restrictions at the OS level.

**Real-world example:** A build agent that runs `cargo build`, `npm install`, or `make` as part of a CI pipeline.

**Without this capability:** The agent cannot spawn any child processes. It cannot run shell commands, compilers, or any external tools. This is the single most important capability to deny for untrusted agents.

---

### Bit 7: `Time` -- System Clocks

**Stdlib functions requiring this capability:**

| Function | Signature | Description |
|----------|-----------|-------------|
| `datetime.now()` | `() -> Int` | Current Unix timestamp (seconds) |
| `env.millis()` | `() -> Int` | Milliseconds since epoch |
| `env.unix_timestamp()` | `() -> Int` | Unix timestamp (seconds) |
| `env.sleep_ms(ms)` | `(Int) -> Unit` | Sleep for milliseconds |

Note: Pure datetime functions (`datetime.format`, `datetime.year`, `datetime.add_days`, etc.) do NOT require `Time` because they operate on integer timestamps, not the system clock.

**WASI imports:**

| Import | Signature | Role |
|--------|-----------|------|
| `clock_time_get` | `(clock_id, precision, time_ptr) -> errno` | Read system or monotonic clock |

**Real-world example:** A monitoring agent that timestamps log entries, measures operation durations, or implements retry backoff with `env.sleep_ms`.

**Without this capability:** The agent has no concept of "now." It cannot timestamp events, measure durations, or implement timeouts. Every operation is timeless. This prevents timing side-channel attacks -- an agent cannot measure how long operations take to infer information about the system.

---

### Bit 8: `Rand` -- Random Number Generation

**Stdlib functions requiring this capability:**

| Function | Signature | Description |
|----------|-----------|-------------|
| `random.int(min, max)` | `(Int, Int) -> Int` | Random integer in range |
| `random.float()` | `() -> Float` | Random float in [0.0, 1.0) |
| `random.choice(xs)` | `(List[T]) -> Option[T]` | Random element from list |
| `random.shuffle(xs)` | `(List[T]) -> List[T]` | Shuffled copy of list |

**WASI imports:**

| Import | Signature | Role |
|--------|-----------|------|
| `random_get` | `(buf_ptr, buf_len) -> errno` | Fill buffer with random bytes |

**Real-world example:** An agent that generates unique IDs, randomizes test data, or implements probabilistic algorithms.

**Without this capability:** The agent is fully deterministic. Given the same inputs, it always produces the same outputs. This is valuable for reproducible builds, deterministic testing, and auditable agent behavior.

---

### Bit 9: `Fan` -- Structured Concurrency

**Stdlib functions / syntax requiring this capability:**

| Syntax/Function | Description |
|-----------------|-------------|
| `fan { expr1; expr2 }` | Execute expressions concurrently, return tuple |
| `fan.map(xs, f)` | Apply function to each element concurrently |
| `fan.race(fns)` | Run all, return first result |
| `fan.any(fns)` | Run all, return first success |
| `fan.settle(fns)` | Run all, collect all results |
| `fan.timeout(duration, fn)` | Run with timeout |

**WASI imports:**

None. Fan is implemented internally using `std::thread::scope` (Rust target) or sequential execution (WASM target, since WASM is single-threaded). No WASI imports are needed.

**Real-world example:** A data processing agent that fetches multiple API endpoints concurrently, then combines the results.

**Without this capability:** The agent cannot use `fan` blocks. All operations execute sequentially. The compiler rejects `fan { ... }` syntax at compile time. In the WASM target, fan blocks execute sequentially regardless, but the capability still controls whether the syntax is allowed.

---

### Bit 10: `IO.stdin` -- Standard Input

**Stdlib functions requiring this capability:**

| Function | Signature | Description |
|----------|-----------|-------------|
| `io.read_line()` | `() -> String` | Read one line from stdin |
| `io.read_all()` | `() -> String` | Read all of stdin |
| `process.stdin_lines()` | `() -> Result[List[String], String]` | Read all stdin lines |

**WASI imports:**

| Import | Signature | Role |
|--------|-----------|------|
| `fd_read` | `(fd=0, iovs_ptr, iovs_len, nread_ptr) -> errno` | Read from file descriptor 0 (stdin) |

**Real-world example:** A pipe-friendly agent that reads JSON from stdin, processes it, and writes results to stdout -- composable with Unix pipes.

**Without this capability:** The agent cannot read from stdin. It can only operate on data from files (FS.read) or hardcoded values. Attempting `io.read_line()` is a compile error.

---

### Bit 11: `IO.stdout` -- Standard Output

**Stdlib functions requiring this capability:**

| Function | Signature | Description |
|----------|-----------|-------------|
| `println(value)` | `(T) -> Unit` | Print with newline |
| `io.print(s)` | `(String) -> Unit` | Print without newline |
| `io.write_bytes(data)` | `(List[Int]) -> Unit` | Write raw bytes to stdout |
| `io.write(data)` | `(Bytes) -> Unit` | Write Bytes buffer to stdout |

**WASI imports:**

| Import | Signature | Role |
|--------|-----------|------|
| `fd_write` | `(fd=1, iovs_ptr, iovs_len, nwritten_ptr) -> errno` | Write to file descriptor 1 (stdout) |

**Real-world example:** Nearly every agent needs stdout for outputting results. A pure computation agent that transforms data and prints the result.

**Without this capability:** The agent is silent. It cannot print anything. It can still write to files (with `FS.write`) but has no console output. The `pure` profile omits this capability intentionally.

---

### Bit 12: `IO.stderr` -- Standard Error

**Stdlib functions requiring this capability:**

| Function | Signature | Description |
|----------|-----------|-------------|
| `eprintln(value)` | `(T) -> Unit` | Print to stderr with newline |

**WASI imports:**

| Import | Signature | Role |
|--------|-----------|------|
| `fd_write` | `(fd=2, iovs_ptr, iovs_len, nwritten_ptr) -> errno` | Write to file descriptor 2 (stderr) |

Note: `fd_write` serves both `IO.stdout` and `IO.stderr`, differentiated by the file descriptor argument. When the binary has `IO.stderr` but not `IO.stdout`, the `fd_write` import is present but the runtime constrains which FDs are writable.

**Real-world example:** An agent that separates structured output (stdout) from diagnostic/debug messages (stderr), following Unix conventions.

**Without this capability:** The agent cannot write to stderr. Useful when stderr is reserved for host-level diagnostics and the agent should not pollute it.

---

## Declaration

```toml
# almide.toml
[permissions]
allow = ["FS.read", "IO.stdout"]
```

No `[permissions]` section = all capabilities allowed (backward compatible with pre-capability code).

Explicit empty: `allow = []` = the `pure` profile. No I/O of any kind.

---

## Implementation Details

### CapabilitySet as u16 Bitset

Each capability maps to a single bit in a `u16` value:

```
Bit:  15 14 13 12 11 10  9  8  7  6  5  4  3  2  1  0
       _  _  _  IO IO IO Fan Rnd Tim Proc Env Env Net Net FS  FS
                 .se.so.si          .w  .r  .li .fe .w  .r
                 rr ut di          ri  ea  st  tc
                 r  t  n           te  d   en  h
```

Operations on CapabilitySet are pure bitwise:

```
grant(cap)              set |= (1 << cap)
revoke(cap)             set &= !(1 << cap)
has(cap)                set & (1 << cap) != 0
union(a, b)             a | b
intersection(a, b)      a & b
is_subset(required, granted)   required & granted == required
missing(required, granted)     required & !granted
```

The `pure` profile is `0x0000`. The `All` profile is `0x1FFF` (bits 0-12 set). Shorthand `IO` is `0x1C03` (bits 0, 1, 10, 11, 12). Shorthand `Net` is `0x000C` (bits 2, 3).

### Checking Algorithm

The capability check runs after the `EffectInferencePass` in the compiler pipeline. The algorithm has three steps:

**Step 1: Collect direct capabilities per function.**

Walk each function body. For every stdlib call, map the callee module to its required capability:

```
module_to_capabilities("fs", "read_text")  -> {FS.read}
module_to_capabilities("fs", "write")      -> {FS.write}
module_to_capabilities("http", "get")      -> {Net.fetch}
module_to_capabilities("http", "serve")    -> {Net.listen}
module_to_capabilities("env", "get")       -> {Env.read}
module_to_capabilities("env", "set")       -> {Env.write}
module_to_capabilities("process", "exec")  -> {Proc}
module_to_capabilities("datetime", "now")  -> {Time}
module_to_capabilities("random", "int")    -> {Rand}
```

The mapping is per-function, not per-module: `fs.read_text` requires `FS.read` while `fs.write` requires `FS.write`, even though both are in the `fs` module.

**Step 2: Transitive closure via fixpoint iteration.**

Build a call graph from the IR. Then iterate:

```
repeat until no changes:
    for each function F:
        for each function G that F calls:
            F.transitive_caps |= G.transitive_caps
```

This converges in at most N iterations (where N is the longest call chain). In practice, 2-3 iterations suffice. The implementation caps at 20 iterations as a safety bound.

**Step 3: Compare against manifest.**

```
for each function F in the program:
    let required = F.transitive_caps
    let granted = manifest.permissions.allow
    let missing = required & !granted
    if missing != 0:
        emit error E010 for each bit set in missing
```

### Transitive Checking

If function `A` calls function `B`, and `B` calls `fs.write`, then `A` transitively requires `FS.write`. The user does not need to annotate `A` -- the compiler infers it.

```almide
// This function directly requires FS.write
effect fn save(path: String, data: String) -> Result[Unit, String] =
  fs.write(path, data)

// This function transitively requires FS.write (through save)
effect fn process_and_save(input: String) -> Result[Unit, String] = {
  let result = transform(input)
  save("output.txt", result)
}

// almide.toml: allow = ["IO.stdout"]
// Error on process_and_save, not just save
```

The error message traces the full chain: `process_and_save -> save -> fs.write`.

### Lambda and Closure Handling

Lambdas are analyzed by walking their body, identical to named functions. The inferred capabilities of a lambda propagate to the enclosing function:

```almide
effect fn main() -> Unit = {
  let files = ["a.txt", "b.txt"]
  // This lambda requires FS.read, so main requires FS.read
  let contents = files |> list.map((f) => fs.read_text(f)!)
  println(contents)
}
```

The lambda `(f) => fs.read_text(f)!` requires `FS.read`. Since it is defined inside `main`, `main`'s transitive capabilities include `FS.read`.

### Higher-Order Function Propagation (Phase 4, Future)

Currently, if a function accepts a callback parameter, the compiler does not track what capabilities that callback might require. A function like `list.map` is not marked as requiring `FS.read` just because one call site passes a lambda that reads files.

Phase 4 will add capability-polymorphic function signatures:

```almide
// Future syntax (not yet implemented)
fn apply[C: Capabilities](f: (String) -> String with C, x: String) -> String with C = f(x)
```

Until Phase 4, the capability check is sound but conservative: capabilities are inferred from the concrete lambda body at each call site, not from the higher-order function's signature.

---

## Enforcement

### Layer 1: Compiler (compile error)

The compiler analyzes the full call graph and rejects the binary before it is produced. The error is always E010.

```
error[E010]: capability violation: fs.write_text requires FS.write
  --> agent.almd:5:3
  |
5 |   fs.write_text(path, content)
  |   ^^^^^^^^^^^^^
  = note: [permissions] allows: FS.read, IO.stdout
  = hint: add "FS.write" to [permissions].allow in almide.toml
```

The binary is never produced.

### Layer 2: WASM Binary (import pruning)

`allow = ["FS.read", "IO.stdout"]` produces a WASM binary that only imports:

- `fd_read` (for FS.read)
- `fd_write` (for IO.stdout, fd=1 only)
- `fd_close`, `fd_seek`, `fd_filestat_get` (for FS.read support)
- `path_open`, `path_filestat_get` (for FS.read support)
- `fd_readdir` (for FS.read directory listing)

Missing imports: `path_create_directory`, `path_rename`, `path_unlink_file`, `path_remove_directory`, `environ_get`, `args_get`, `random_get`, `clock_time_get`, `proc_exit`, `almide_host_fetch`, `almide_host_listen`, `almide_host_proc` -- **physically absent from the binary**. Even if the compiler has a bug, the WASM runtime will reject calls to nonexistent imports.

### Layer 3: WASI Runtime (--dir scoping)

```bash
wasmtime run --dir /workspace agent.wasm
```

Even with `FS.read` allowed, the agent can only read `/workspace`. WASI pre-opened directories are the final wall.

---

## Error Messages

### Error 1: Direct stdlib call violation

```
error[E010]: capability violation: fs.write requires FS.write
  --> agent.almd:12:3
   |
12 |   fs.write("output.txt", result)
   |   ^^^^^^^^
   = note: [permissions] allows: FS.read, IO.stdout
   = hint: add "FS.write" to [permissions].allow in almide.toml
```

### Error 2: Transitive violation through user function

```
error[E010]: capability violation: function 'deploy' transitively requires Net.fetch
  --> agent.almd:20:3
   |
20 |   deploy(artifact_url)
   |   ^^^^^^
   |
   = note: call chain: deploy -> upload -> http.post
   = note: [permissions] allows: FS.read, FS.write, IO.stdout
   = hint: add "Net.fetch" to [permissions].allow in almide.toml
```

### Error 3: Dependency violation

```
error[E010]: capability violation: dependency 'analytics' uses Proc
  --> almide.toml:8:1
  |
8 | [dependencies.analytics]
  | ^^^^^^^^^^^^^^^^^^^^^^^^
  |
  = note: analytics@1.2.0 calls process.exec in analytics/src/reporter.almd:45
  = note: [dependencies.analytics].allow does not include Proc
  = hint: add "Proc" to [dependencies.analytics].allow, or use a sandboxed alternative
```

### Error 4: Multiple violations in one file

```
error[E010]: 3 capability violations in agent.almd
  --> agent.almd
  |
  | error 1/3: fs.write("out.txt", data) requires FS.write (line 15)
  | error 2/3: http.get(api_url) requires Net.fetch (line 22)
  | error 3/3: random.int(1, 100) requires Rand (line 31)
  |
  = note: [permissions] allows: FS.read, IO.stdout
  = hint: add "FS.write", "Net.fetch", "Rand" to [permissions].allow in almide.toml
```

### Error 5: Helpful hint suggesting pure alternative

```
error[E010]: capability violation: datetime.now() requires Time
  --> agent.almd:8:15
  |
8 |   let stamp = datetime.now()
  |               ^^^^^^^^^^^^^^
  = note: [permissions] allows: FS.read, IO.stdout
  = hint: if you need a fixed timestamp, pass it as a function parameter instead
          of reading the system clock. This keeps your function pure and testable.
          Example: effect fn main() -> Unit = process(datetime.now())
                   fn process(now: Int) -> String = ...
```

### Error 6: Fan block without Fan capability

```
error[E010]: capability violation: fan block requires Fan
  --> agent.almd:10:3
   |
10 |   let (a, b) = fan {
   |                ^^^
   = note: [permissions] allows: FS.read, Net.fetch, IO.stdout
   = hint: add "Fan" to [permissions].allow, or execute sequentially:
           let a = fetch_a()
           let b = fetch_b()
```

---

## Comparison with Other Systems

### Deno: Runtime Permissions

Deno uses `--allow-read`, `--allow-net`, `--allow-env`, etc. as command-line flags. Permissions are checked at runtime -- the program starts, runs, and crashes (or prompts) when it hits an unauthorized operation.

**Strengths:** Simple mental model. Interactive prompting in development. Granular path/host scoping.

**Weaknesses:** Runtime-only. A deployed script can fail in production when it hits a code path that was never tested during development. No compile-time proof. The check happens at the syscall boundary, so the program must be trusted not to catch and suppress the error.

### Rust: No Capability System

Rust has no I/O capability system. Any function can call `std::fs::write` or `std::net::TcpStream::connect`. The `unsafe` keyword governs memory safety (raw pointers, FFI), not I/O access.

**Strengths:** Zero overhead. No permission ceremony for trusted code.

**Weaknesses:** A single dependency in `Cargo.toml` can read your SSH keys, exfiltrate secrets over the network, or delete files. `cargo-audit` checks for known vulnerabilities but cannot prevent unknown malicious behavior.

### Java: SecurityManager (Deprecated)

Java's `SecurityManager` allowed fine-grained runtime permission checks (`FilePermission`, `SocketPermission`, etc.). It was deprecated in Java 17 and removed in Java 24.

**Strengths:** Very granular. Could restrict individual file paths, network hosts, and system properties.

**Weaknesses:** Enormous API surface. Almost impossible to configure correctly. Performance overhead on every I/O operation. Library authors rarely tested with a SecurityManager active, so enabling it broke most libraries. Effectively unused in practice.

### Austral: Linear Types + Capabilities

Austral (academic language) uses linear types to model capabilities. A `Filesystem` capability token must be passed to any function that performs I/O. The type system ensures the token is used exactly once.

**Strengths:** Compile-time. Formally sound. Capabilities are first-class values.

**Weaknesses:** Ergonomic burden. Every I/O function requires threading capability tokens through the call graph. Academic -- limited ecosystem and tooling.

### WASI: Capability-Based Security

WASI (WebAssembly System Interface) uses pre-opened file descriptors. A WASM module can only access directories that the host explicitly grants via `--dir` flags. Network and process spawning are not in WASI preview 1 -- they require host extensions.

**Strengths:** Runtime enforcement at the VM boundary. Defense in depth. Industry standard.

**Weaknesses:** Runtime-only. Coarse granularity (entire directories, not individual files). No compile-time feedback. A module that imports `fd_write` can write to any pre-opened FD; there is no compile-time distinction between "writes to stdout" and "writes to files."

### Comparison Table

| Property | Almide | Deno | Rust | Java SM | Austral | WASI |
|----------|--------|------|------|---------|---------|------|
| **Enforcement time** | Compile | Runtime | None | Runtime | Compile | Runtime |
| **Binary proof** | Import pruning | N/A | N/A | N/A | N/A | Pre-opened FDs |
| **Granularity** | 13 categories | ~8 flags | N/A | ~30 classes | Per-token | Per-FD |
| **Transitive tracking** | Automatic | No | No | Stack walk | Manual | No |
| **Dependency restriction** | Per-dep allow/deny | No | No | Per-classloader | N/A | N/A |
| **Ergonomic cost** | 2 lines in TOML | CLI flags | Zero | XML policy | Token threading | CLI flags |
| **Escape hatch** | None | `--allow-all` | `unsafe` (unrelated) | Custom SM | N/A | `--dir .` |
| **Production failures** | Impossible (won't compile) | Possible | N/A | Common | Impossible | Possible |
| **Status** | Active | Active | N/A | Removed (Java 24) | Academic | Active |

Almide is unique in combining all three: compile-time rejection, binary-level proof (import pruning), and runtime enforcement (WASI). No other production language offers compile-time capability tracking with transitive inference and zero annotation burden on the programmer.

---

## WASM Import Pruning Details

### Imports Per Capability

The following table shows exactly which WASI imports are included in the binary for each capability:

| Capability | WASI Imports Included |
|------------|----------------------|
| `FS.read` | `path_open`, `fd_read`, `fd_close`, `fd_seek`, `fd_filestat_get`, `path_filestat_get`, `fd_readdir` |
| `FS.write` | `path_open`, `fd_write`(fd>2), `fd_close`, `path_create_directory`, `path_rename`, `path_unlink_file`, `path_remove_directory` |
| `Net.fetch` | `almide_host_fetch` |
| `Net.listen` | `almide_host_listen` |
| `Env.read` | `environ_get`, `environ_sizes_get`, `args_get`, `args_sizes_get` |
| `Env.write` | `almide_host_environ_set` |
| `Proc` | `almide_host_proc`, `proc_exit` |
| `Time` | `clock_time_get` |
| `Rand` | `random_get` |
| `Fan` | (none -- internal implementation) |
| `IO.stdin` | `fd_read`(fd=0) |
| `IO.stdout` | `fd_write`(fd=1) |
| `IO.stderr` | `fd_write`(fd=2) |

Note: Some imports serve multiple capabilities. `fd_write` is needed by `FS.write`, `IO.stdout`, and `IO.stderr`. `fd_read` is needed by `FS.read` and `IO.stdin`. `path_open` is needed by both `FS.read` and `FS.write`. The pruner includes an import if ANY capability that requires it is granted.

### Binary Size Comparison

The following shows the WASM import section for three representative configurations:

**`pure` profile (allow = []):**

```wasm
;; Import section: 0 WASI imports
;; Only internal memory and table exports
;; Binary: ~2 KB overhead (just the core runtime)
```

**`readonly-agent` profile (allow = ["FS.read", "IO.stdin", "IO.stdout"]):**

```wasm
(import "wasi_snapshot_preview1" "fd_write"          (func $fd_write ...))
(import "wasi_snapshot_preview1" "fd_read"            (func $fd_read ...))
(import "wasi_snapshot_preview1" "fd_close"           (func $fd_close ...))
(import "wasi_snapshot_preview1" "fd_seek"            (func $fd_seek ...))
(import "wasi_snapshot_preview1" "fd_filestat_get"    (func $fd_filestat_get ...))
(import "wasi_snapshot_preview1" "path_open"          (func $path_open ...))
(import "wasi_snapshot_preview1" "path_filestat_get"  (func $path_filestat_get ...))
(import "wasi_snapshot_preview1" "fd_readdir"         (func $fd_readdir ...))
;; 8 WASI imports
;; Binary: ~3 KB overhead
```

**`full-agent` profile (allow = All):**

```wasm
(import "wasi_snapshot_preview1" "fd_write"              (func ...))
(import "wasi_snapshot_preview1" "fd_read"                (func ...))
(import "wasi_snapshot_preview1" "fd_close"               (func ...))
(import "wasi_snapshot_preview1" "fd_seek"                (func ...))
(import "wasi_snapshot_preview1" "fd_filestat_get"        (func ...))
(import "wasi_snapshot_preview1" "path_open"              (func ...))
(import "wasi_snapshot_preview1" "path_filestat_get"      (func ...))
(import "wasi_snapshot_preview1" "path_create_directory"  (func ...))
(import "wasi_snapshot_preview1" "path_rename"            (func ...))
(import "wasi_snapshot_preview1" "path_unlink_file"       (func ...))
(import "wasi_snapshot_preview1" "path_remove_directory"  (func ...))
(import "wasi_snapshot_preview1" "fd_readdir"             (func ...))
(import "wasi_snapshot_preview1" "clock_time_get"         (func ...))
(import "wasi_snapshot_preview1" "proc_exit"              (func ...))
(import "wasi_snapshot_preview1" "random_get"             (func ...))
(import "wasi_snapshot_preview1" "environ_get"            (func ...))
(import "wasi_snapshot_preview1" "environ_sizes_get"      (func ...))
(import "wasi_snapshot_preview1" "args_get"               (func ...))
(import "wasi_snapshot_preview1" "args_sizes_get"         (func ...))
(import "almide_host" "fetch"                             (func ...))
(import "almide_host" "listen"                            (func ...))
(import "almide_host" "proc"                              (func ...))
(import "almide_host" "environ_set"                       (func ...))
;; 23 imports (19 WASI + 4 host)
;; Binary: ~5 KB overhead
```

### How Missing Imports Cause Rejection

When wasmtime (or any WASI-compliant runtime) instantiates a WASM module, it resolves every import against the host's provided functions. If the binary imports `path_create_directory` but the host does not provide it, instantiation fails before any code executes:

```
Error: unknown import: `wasi_snapshot_preview1::path_create_directory` has not been defined
```

This is the second layer of defense. Even if the compiler has a bug and emits code that calls a pruned import, the WASM runtime catches it at load time -- before any guest code runs.

The pruner works conservatively: it includes an import if the call graph references ANY function that could use it, even in dead code paths. The WASM-level DCE (dead code elimination) pass runs after pruning to remove unreachable functions, but the import set is determined first.

---

## Per-Dependency Restriction (Future)

```toml
[dependencies.json_parser]
version = "1.0"
allow = []              # pure only -- no I/O, no net, nothing

[dependencies.http_client]
version = "2.0"
allow = ["Net.fetch"]   # can fetch, nothing else
deny = ["Proc"]         # explicitly deny even if parent allows

[dependencies.code_formatter]
version = "0.5"
allow = ["FS.read"]     # read-only access to source files
```

Compile error if a dependency's transitive call graph exceeds its granted capabilities. The check runs the same algorithm as the top-level permission check but scopes the granted set to each dependency's `allow` list.

This creates a trust hierarchy: the top-level project declares its own capabilities, and each dependency is restricted to a subset. A compromised dependency cannot escalate beyond its declared permissions.

---

## Profiles

Common permission sets for typical agent patterns:

```toml
[permissions]
profile = "readonly-agent"
```

### `pure` -- Zero I/O

**Capabilities:** (none)

**Use cases:**
- **Data transformation:** Parse JSON, transform records, compute aggregates -- all inputs and outputs passed through function parameters
- **Deterministic computation:** Math, string processing, list operations
- **Library code:** Utility functions that should never touch the outside world

A `pure` agent is provably side-effect-free. It cannot read files, access the network, print to stdout, or read the clock. All behavior is determined entirely by its inputs. This makes it ideal for sandboxed evaluation of untrusted code.

### `readonly-agent` -- Read + Print

**Capabilities:** `FS.read`, `IO.stdin`, `IO.stdout`

**Use cases:**
- **Code review agent:** Reads source files, analyzes code quality, prints a report to stdout
- **Static analysis:** Reads configuration and source, detects issues, outputs diagnostics
- **Documentation generation:** Reads source files and doc comments, produces formatted documentation
- **Search/grep agent:** Reads files matching patterns, outputs matching lines

A `readonly-agent` can observe but never modify. It reads files and prints results. It cannot write files, access the network, read environment variables, or execute processes. This is the safest profile for agents that need filesystem access.

### `readwrite-agent` -- Full Local I/O

**Capabilities:** `FS.read`, `FS.write`, `IO.stdin`, `IO.stdout`

**Use cases:**
- **Code generation agent:** Reads templates and schemas, writes generated source files
- **Refactoring agent:** Reads source, applies transformations, writes modified files
- **Test writer:** Reads source and existing tests, writes new test files
- **Build tool:** Reads source, writes compiled output, creates directory structures

A `readwrite-agent` has full local filesystem access but zero network access. It cannot phone home, exfiltrate data, or download malicious code. Combined with WASI `--dir` scoping, it can only modify files within the granted directory.

### `network-agent` -- Network + Print

**Capabilities:** `Net.fetch`, `IO.stdin`, `IO.stdout`

**Use cases:**
- **API integration agent:** Fetches data from REST APIs, processes it, outputs results
- **Web scraper:** Fetches HTML pages, extracts data, prints structured output
- **LLM tool agent:** Calls external LLM APIs, processes responses

A `network-agent` can talk to the network but cannot touch the local filesystem. It cannot read or write any files. All data comes from network requests or stdin, and all output goes to stdout. This prevents a compromised API response from causing local damage.

### `full-agent` -- All Capabilities

**Capabilities:** All 13

**Use cases:**
- **CI/CD agent:** Reads source, runs builds (Proc), fetches dependencies (Net.fetch), writes artifacts (FS.write)
- **Development assistant:** Full access for interactive development workflows
- **System administration agent:** Manages files, processes, environment, network

A `full-agent` has no restrictions beyond WASI runtime scoping. Use this only when the agent is fully trusted and needs unrestricted access.

### Outgrowing a Profile

Profiles are convenience aliases. When a profile does not match your needs, switch to explicit `allow`:

```toml
# Started with:
[permissions]
profile = "readonly-agent"   # FS.read, IO.stdin, IO.stdout

# But now we need Time for timestamps in our analysis report.
# Switch to explicit allow:
[permissions]
allow = ["FS.read", "IO.stdin", "IO.stdout", "Time"]
```

You cannot combine `profile` with additional `allow` entries. Choose one or the other. Profiles exist to reduce boilerplate for common patterns, not to be extensible base classes.

If you find yourself frequently needing a profile + 1-2 extras, that is by design: the explicit `allow` list makes every granted capability visible and auditable.
