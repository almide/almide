<!-- description: Capability-based effect system for sandboxed AI agent containers -->
# Capability-Based Effect System

Compile-time enforcement of least privilege for AI agent containers. The compiler proves that an agent's code never exceeds its declared capabilities. WASM output contains only the WASI imports the manifest permits.

## The Problem

```almide
// Today: effect fn = can do ANYTHING
effect fn agent(cmd: String) -> String =
  fs.write_text("/etc/passwd", "pwned")  // ← compiles fine
```

`effect fn` is binary: either no I/O or all I/O. An AI-generated agent with `effect fn` has unrestricted access to every WASI capability the runtime grants. The only defense is WASI's coarse-grained `--dir` flags at runtime.

## The Solution

```toml
# almide.toml
[permissions]
allow = ["FS.read", "IO.stdout"]
```

```almide
effect fn agent(cmd: String) -> String = {
  let content = fs.read_text(cmd)   // ✅ FS.read is allowed
  println(content)                   // ✅ IO.stdout is allowed
  fs.write_text(cmd, "x")           // ❌ compile error: FS.write not in permissions
}
```

The compiler rejects the binary. The WASM file is never produced. No runtime check needed.

## Capability Taxonomy (13 categories)

| Category | Stdlib functions | WASI imports |
|----------|-----------------|-------------|
| `FS.read` | fs.read_text, fs.exists, fs.stat, fs.list_dir | path_open(read), fd_read |
| `FS.write` | fs.write_text, fs.remove, fs.mkdir_p, fs.rename | path_open(write), fd_write(fd>2) |
| `Net.fetch` | http.get, http.post, http.put, http.delete | host-provided fetch |
| `Net.listen` | http.serve | host-provided listen |
| `Env.read` | env.get, env.args, env.cwd, env.os | environ_get, args_get |
| `Env.write` | env.set | environ_set |
| `Proc` | process.exec, process.exit | host-provided proc |
| `Time` | datetime.now, env.millis | clock_time_get |
| `Rand` | random.int, random.float | random_get |
| `Fan` | fan { }, fan.map | (internal: threads/async) |
| `IO.stderr` | eprintln | fd_write(fd=2) |
| `IO.stdin` | io.read_line | fd_read(fd=0) |
| `IO.stdout` | println, io.print | fd_write(fd=1) |
| `IO.stderr` | eprintln | fd_write(fd=2) |

Shorthand: `IO` = FS.read + FS.write + IO.stdin + IO.stdout + IO.stderr, `Net` = Net.fetch + Net.listen, `All` = all 13

## Three Layers of Defense

```
Layer 1: Compiler          — code that exceeds manifest → compile error (BLOCKED)
Layer 2: WASM binary       — WASI imports pruned to match manifest (ABSENT)
Layer 3: Runtime (WASI)    — --dir, --env flags as final wall (DENIED)
```

Layer 1 catches bugs. Layer 2 catches compiler bugs. Layer 3 catches everything else. Defense in depth.

## Implementation Phases

### Phase 1: Capability Annotation + Compiler Checking

**Goal**: `[permissions]` in almide.toml → compile error on violation.

1. Define the capability → stdlib function mapping in the compiler
   - Each stdlib module function annotated with its capability category
   - `fs.read_text` → `FS.read`, `fs.write_text` → `FS.write`, etc.
   - Stored as a static table in `almide-frontend`

2. Parse `[permissions]` from almide.toml
   - `allow = ["FS.read", "IO.stdout"]` → `CapabilitySet` (u16 bitset, 13 bits)
   - Default when `[permissions]` absent: all capabilities allowed (backward compat)

3. Capability checking pass in the type checker
   - After type checking, walk the call graph
   - For each stdlib call, look up its required capability
   - If not in `allow` → emit error with actionable hint:
     ```
     error[E010]: capability violation: fs.write_text requires FS.write
       --> agent.almd:5:3
       |
     5 |   fs.write_text(path, content)
       |   ^^^^^^^^^^^^^
       = note: [permissions] allows: FS.read, IO.stdout
       = hint: add "FS.write" to [permissions].allow in almide.toml
     ```
   - Transitive: if `fn foo` calls `fn bar` calls `fs.write`, `foo` requires `FS.write`

4. Effect function inference update
   - `effect fn` without [permissions] → all capabilities (existing behavior)
   - `effect fn` with [permissions] → only declared capabilities
   - Pure `fn` → zero capabilities (unchanged)

**Files to modify**:
- `stdlib/defs/*.toml` — add `capability = "FS.read"` field to each function def
- `crates/almide-frontend/src/check/` — capability checking pass
- `crates/almide-frontend/src/resolve.rs` — parse [permissions] from almide.toml
- `crates/almide-base/src/` — CapabilitySet type (u16 bitset)

**Test**: Write an agent test that compiles with `allow = ["FS.read"]` and fails with `fs.write_text` call.

### Phase 2: WASM Import Pruning + Manifest Output

**Goal**: WASM binary contains only WASI imports needed by the capability set. Outputs a machine-readable manifest.

1. Map capabilities to WASI imports
   - `FS.read` → needs `path_open`, `fd_read`, `fd_close`, `fd_seek`, `path_filestat_get`
   - `FS.write` → needs `path_open` (write), `fd_write`, `path_create_directory`, etc.
   - `IO.stdout` → needs `fd_write` (fd=1 only)
   - If capability not allowed → don't emit the WASI import

2. Update WASM emitter import section
   - Current: always imports all WASI functions
   - New: only import WASI functions required by CapabilitySet
   - Runtime functions that reference pruned imports → replace with `unreachable` trap

3. Emit `manifest.json` alongside `.wasm`
   ```json
   {
     "name": "file-reader-agent",
     "version": "0.1.0",
     "capabilities": ["FS.read", "IO.stdout"],
     "wasi_imports": ["fd_read", "fd_write", "fd_close", "path_open", "path_filestat_get"],
     "entry": "_start"
   }
   ```
   Orchestrator reads this before loading the WASM binary — verify capabilities match policy.

**Files to modify**:
- `crates/almide-codegen/src/emit_wasm/mod.rs` — conditional WASI import emission
- `crates/almide-codegen/src/emit_wasm/runtime.rs` — WASI import registration
- `src/cli/build.rs` — emit manifest.json

**Test**: Build with `allow = ["IO.stdout"]` → verify WASM binary has no `path_open` import.

### Phase 3: Per-Dependency Capability Restriction

**Goal**: Restrict what capabilities a dependency package is allowed to use.

```toml
[dependencies.json_parser]
version = "1.0"
allow = []                    # pure only — no I/O

[dependencies.http_client]
version = "2.0"
allow = ["Net.fetch"]
deny = ["Proc", "FS.write"]  # explicit deny
```

1. Parse per-dependency permissions from almide.toml
2. During module interface loading, compute the dependency's required capabilities
   - Walk its exported functions' transitive call graphs
   - Compare against the consumer's `allow` / `deny` for that dependency
3. Compile error if dependency exceeds its granted capabilities

**Files to modify**:
- `src/resolve.rs` — per-dependency permission parsing
- `crates/almide-frontend/src/check/` — cross-module capability propagation
- Module interface format — include capability requirements

### Phase 4: EffectSet in Ty::Fn (Internal Type System)

**Goal**: Track capabilities at the type level for higher-order functions.

```rust
// Internal compiler representation (not user-facing syntax)
Fn { params: Vec<Ty>, ret: Box<Ty>, effects: CapabilitySet }
```

- `list.map(f, xs)` where `f` requires `FS.read` → result expression requires `FS.read`
- Passing an effect fn to a pure HOF parameter → compile error
- Lambda inference: `(x) => fs.read_text(x)` → inferred capability `FS.read`

This is the most invasive change (touches Ty enum, unification, inference) but enables precise tracking through HOFs.

**Files to modify**:
- `crates/almide-types/src/types.rs` — CapabilitySet field on Ty::Fn
- `crates/almide-frontend/src/check/infer.rs` — effect propagation through HOFs
- `crates/almide-frontend/src/check/unify.rs` — CapabilitySet unification

### Phase 5: MCP Server Stdlib Module

**Goal**: `mcp` stdlib module implementing Model Context Protocol (JSON-RPC over stdio). No custom protocol, no language changes.

MCP is the de facto standard for AI agent tool communication (Claude Code, Cursor, Windsurf, GitHub Copilot). An Almide WASM binary that speaks MCP is immediately pluggable into any MCP client.

```almide
import mcp

effect fn main() -> Unit =
  mcp.serve([
    mcp.tool("read_file", "Read a file at the given path", (params) => {
      fs.read_text(mcp.arg(params, "path"))
    }),
    mcp.tool("list_dir", "List directory contents", (params) => {
      fs.list_dir(mcp.arg(params, "path")) |> list.join("\n")
    }),
  ])
```

```json
// Claude Code mcpServers config — works directly
{
  "mcpServers": {
    "code-reviewer": {
      "command": "wasmtime",
      "args": ["run", "--dir", "/workspace", "agent.wasm"]
    }
  }
}
```

- `mcp.serve(tools)` — JSON-RPC stdio loop (initialize → tools/list → tools/call)
- `mcp.tool(name, description, handler)` — tool definition
- `mcp.arg(params, key)` — parameter extraction
- `mcp.resource(uri, handler)` — resource exposure (future)
- All stdlib, no language changes. MCP spec compliance, not a custom protocol.

## AI Agent Container Example

```toml
# almide.toml
[project]
name = "code-review-agent"
version = "0.1.0"

[permissions]
allow = ["FS.read", "IO.stdin", "IO.stdout"]
```

```almide
// agent.almd
import mcp

effect fn main() -> Unit =
  mcp.serve([
    mcp.tool("read_file", "Read file contents", (params) => {
      fs.read_text(mcp.arg(params, "path"))
    }),
    mcp.tool("list_dir", "List directory", (params) => {
      fs.list_dir(mcp.arg(params, "path")) |> list.join("\n")
    }),
    mcp.tool("search", "Search for pattern in files", (params) => {
      let dir = mcp.arg(params, "dir")
      let pattern = mcp.arg(params, "pattern")
      fs.list_dir(dir)
        |> list.filter((f) => string.contains(f, pattern))
        |> list.join("\n")
    }),
  ])
```

```bash
# Build: small WASM binary + capability manifest
almide build agent.almd --target wasm -o agent.wasm
# → also outputs agent.manifest.json

# Use from Claude Code directly
# claude_code_config.json:
# { "mcpServers": { "code-reviewer": {
#     "command": "wasmtime",
#     "args": ["run", "--dir", "/workspace", "agent.wasm"]
# }}}

# Or run standalone with sandboxing
wasmtime run --dir /workspace agent.wasm
```

**What the compiler guarantees:**
- Agent cannot write files (FS.write not in permissions → compile error)
- Agent cannot access env vars (Env.read not in permissions → compile error)
- Agent cannot make HTTP requests (Net.fetch not in permissions → compile error)
- WASM binary doesn't even contain the WASI imports for those operations

**What WASI guarantees:**
- Agent can only read from /workspace (--dir scoping)
- Agent cannot access any other filesystem path

**What MCP guarantees:**
- Tool interface is discoverable (tools/list)
- Input/output schema is typed
- Any MCP client can use the agent without custom integration

**No other language provides all three layers.** Rust/Go agents can call any syscall. Python/JS MCP servers have no compile-time capability restriction. Deno has runtime permissions but no compile-time proof and no WASM sandboxing.

## Implementation Priority

| Phase | Impact | Effort | Ship independently? |
|-------|--------|--------|---------------------|
| **Phase 1** | Compile-time capability checking | M | ✅ Yes — immediate value |
| **Phase 2** | WASM import pruning + manifest | S | ✅ Yes — binary-level proof |
| **Phase 3** | Per-dependency restriction | S | ✅ Yes — supply chain safety |
| **Phase 4** | Type-level effects (HOF) | L | ✅ Yes — precision |
| **Phase 5** | Agent stdlib module | S | ✅ Yes — stdlib only, no language changes |

Phase 1 alone is enough to claim "compile-time sandboxed AI agent containers."

## What We Don't Do

- Algebraic effect handlers — too complex, not needed for capability checking
- User-defined effect categories — 13 built-in categories cover stdlib
- Effect syntax in function signatures — capabilities declared in almide.toml, not code
- Runtime capability requests — everything is compile-time
