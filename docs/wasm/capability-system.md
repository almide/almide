# Capability System

Compile-time enforcement of least privilege for WASM agent containers.

## 13 Categories

| # | Category | Stdlib functions | WASI imports |
|---|----------|-----------------|-------------|
| 0 | `FS.read` | fs.read_text, fs.exists, fs.stat, fs.list_dir | path_open(r), fd_read |
| 1 | `FS.write` | fs.write_text, fs.remove, fs.mkdir_p, fs.rename | path_open(w), fd_write(fd>2) |
| 2 | `Net.fetch` | http.get, http.post, http.put, http.delete | host fetch |
| 3 | `Net.listen` | http.serve | host listen |
| 4 | `Env.read` | env.get, env.args, env.cwd, env.os | environ_get, args_get |
| 5 | `Env.write` | env.set | environ_set |
| 6 | `Proc` | process.exec, process.exit | host proc |
| 7 | `Time` | datetime.now, env.millis | clock_time_get |
| 8 | `Rand` | random.int, random.float | random_get |
| 9 | `Fan` | fan { }, fan.map | internal |
| 10 | `IO.stdin` | io.read_line | fd_read(fd=0) |
| 11 | `IO.stdout` | println, io.print | fd_write(fd=1) |
| 12 | `IO.stderr` | eprintln | fd_write(fd=2) |

Internal representation: `CapabilitySet = u16` (13 bits).

Shorthand: `IO` = FS.read + FS.write + IO.stdin + IO.stdout + IO.stderr, `Net` = Net.fetch + Net.listen, `All` = all 13.

## Declaration

```toml
# almide.toml
[permissions]
allow = ["FS.read", "IO.stdout"]
```

No `[permissions]` section = all capabilities allowed (backward compatible).

## Enforcement

### Layer 1: Compiler (compile error)

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

`allow = ["FS.read", "IO.stdout"]` → WASM binary only imports:
- `fd_read` (for FS.read)
- `fd_write` (for IO.stdout, fd=1 only)
- `fd_close`, `path_open`, `path_filestat_get` (for FS.read support)

Missing imports: `path_create_directory`, `path_rename`, `path_unlink_file`, `environ_get`, `random_get`, `clock_time_get`, etc. — **physically absent from the binary**. Even if the compiler has a bug, the WASM runtime will reject calls to nonexistent imports.

### Layer 3: WASI Runtime (--dir scoping)

```bash
wasmtime run --dir /workspace agent.wasm
```

Even with `FS.read` allowed, the agent can only read `/workspace`. WASI pre-opened directories are the final wall.

## Per-Dependency Restriction (future)

```toml
[dependencies.json_parser]
version = "1.0"
allow = []              # pure only

[dependencies.http_client]
version = "2.0"
allow = ["Net.fetch"]
deny = ["Proc"]
```

Compile error if dependency's transitive call graph exceeds granted capabilities.

## Profiles (convenience)

Common permission sets for typical agent patterns:

```toml
[permissions]
profile = "readonly-agent"   # FS.read + IO.stdin + IO.stdout
```

| Profile | Capabilities |
|---------|-------------|
| `readonly-agent` | FS.read, IO.stdin, IO.stdout |
| `readwrite-agent` | FS.read, FS.write, IO.stdin, IO.stdout |
| `network-agent` | Net.fetch, IO.stdin, IO.stdout |
| `full-agent` | All |
| `pure` | (none) |
