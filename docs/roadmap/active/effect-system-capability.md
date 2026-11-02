<!-- description: Capability-based effect system for sandboxed AI agent containers -->
# Capability-Based Effect System

> **Current scope**: permission analysis/checking already exists; the remaining
> work is the stronger capability chain and the proposals below. Use
> `almide check` / `almide check --effects`, not the unimplemented
> `--capabilities` spelling. Passing checks is not a whole-runtime sandbox proof.
> **設計裁定 2026-10-03**: [ADR-0026](../../adr/0026-effect-categories-ride-the-transparent-fn-type.md) — 型レベル(Phase 4)の形、カテゴリ階層(下の `IO` shorthand を置き換え)、静的⊆manifest⊆host の鎖。

## Implemented baseline and remaining design

`crates/almide-ir/src/effect.rs` defines 14 leaves and four parents (`FS`,
`Net`, `IO`, `Env`). `IO` contains only stdin/stdout/stderr, not filesystem
access; `Abort` is a leaf. This replaces the original 13-category sketch.
`src/cli/check.rs` and `src/cli/mod.rs` enforce project permissions against
inferred effects; `src/project.rs` parses the grants. The effect bit and the
category analysis are distinct: a pure `fn` is not a promise of zero categories
(output/abort are admitted by ADR-0022).

The intended least-privilege chain is inferred effects ⊆ manifest ⊆ host
(ADR-0026). Existing analysis is not a proof that arbitrary foreign code or
host behavior respects it. `@extern` bodies are opaque and receive all leaves
in inference; native permission and process command grants have their own
boundaries. Host configuration remains necessary.

The phase sketches below record the original implementation decomposition,
not five wholly unimplemented tasks or a set of shipped end-to-end guarantees.
In particular, a declared category does not by itself remove a WASI import:
read/write operations can share an import and rely on handle rights. Import
inspection cannot prove all application or host behavior. A host is a separate
trust boundary, not a catch-all proof of safety.

## Implementation Phases

### Phase 1: Original checking plan (core enforcement now exists)

**Goal**: `[permissions]` in almide.toml → compile error on violation.

1. Define the capability → stdlib function mapping in the compiler
   - Each stdlib module function annotated with its capability category
   - `fs.read_text` → `FS.read`, `fs.write_text` → `FS.write`, etc.
   - Stored as a static table in `almide-frontend`

2. Parse `[permissions]` from almide.toml
   - `allow = ["FS.read", "IO.stdout"]` → the planned `CapabilitySet` representation (current registry has 14 leaves)
   - Default when `[permissions]` absent: all capabilities allowed (backward compat)

3. Capability checking pass in the type checker
   - After type checking, walk the call graph
   - For each stdlib call, look up its required capability
   - If not in `allow` → emit error with actionable hint:
     ```
     error[CAPABILITY_VIOLATION]: capability violation: fs.write_text requires FS.write
       --> agent.almd:5:3
       |
     5 |   fs.write_text(path, content)
       |   ^^^^^^^^^^^^^
       = note: [permissions] allows: FS.read, IO.stdout
       = hint: add "FS.write" to [permissions].allow in almide.toml
     ```
   - The symbolic diagnostic above is illustrative, not an allocated code (`E010` is exhaustiveness).
   - Transitive: if `fn foo` calls `fn bar` calls `fs.write`, `foo` requires `FS.write`

4. Effect function inference update
   - `effect fn` without [permissions] → all capabilities (existing behavior)
   - `effect fn` with [permissions] → only declared capabilities
   - Pure `fn` does not imply zero categories: output/abort remain admissible (ADR-0022).

**Files to modify**:
- stdlib `.almd` sources — declare the capability on each function (the stdlib/defs TOML pipeline is gone; per-fn metadata lives in the .almd bodies)
- `crates/almide-frontend/src/check/` — capability checking pass
- `src/project.rs` — current manifest parsing (the original resolver attribution was stale)
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
   - Original assumption: all WASI functions were imported. This is not a current emitter inventory.
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
- `crates/almide-wasm/src/` — current structural emitter; the old `render_wasm*` / `self_host_registry.rs` modification targets are retired
- `src/cli/build.rs` — emit manifest.json

**Test**: Build with `allow = ["IO.stdout"]` → verify WASM binary has no `path_open` import.

### Phase 3: Proposed per-dependency category restriction

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

### Phase 4: Original Ty::Fn sketch superseded by category-flow analysis

The original plan stored `CapabilitySet` in `Ty::Fn`. Current
`crates/almide-codegen/src/effect_flow/mod.rs` instead computes sets after type
checking, tracks them through function values/positions, and stores the result
in the `EffectMap`, not `Ty::Fn`. Its tracked-position and conservative-pool
rules define the implemented scope; this does not complete every future phase.

Bare fn-typed callback parameters are transparent (ADR-0009/0026): their
callers inherit the supplied callback's effects. Passing an effectful callback
to such a parameter is not categorically rejected. An explicit purity boundary
is different; see `crates/almide-frontend/src/check/pure_attr.rs`.
The old mandatory-rejection rule is not current language guidance.

### Phase 5: Proposed MCP Server Stdlib Module

There is no shipped `mcp` stdlib module. The compiler's MCP tooling is a different interface; the examples below are design sketches, not runnable deployment instructions.

**Goal**: `mcp` stdlib module implementing Model Context Protocol (JSON-RPC over stdio). No custom protocol, no language changes.

MCP is the de facto standard for AI agent tool communication (Claude Code, Cursor, Windsurf, GitHub Copilot). The proposed module would require explicit client/protocol compatibility tests; speaking JSON-RPC alone does not establish compatibility with every MCP client.

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

## Proposed AI Agent Container Example (not a shipped end-to-end recipe)

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
# Proposed build/manifest output — not current CLI behavior
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

### Required boundaries before claiming a secure deployment

- Compiler permission checks cover the analysis implemented for the selected
  route, not arbitrary external/host code. The proposed manifest-emission and
  import-pruning steps need their own implementation and evidence.
- `wasmtime --dir /workspace` is directory preopening, **not a read-only grant**.
  Read/write rights and all other host capabilities must be configured and
  verified separately. These sketches do not establish a read-only container.
- The proposed MCP module would need protocol/schema compatibility tests.
  A typed Almide handler does not establish compatibility with every client.
- No uniqueness claim against all other languages follows from this plan.

## Implementation Priority

| Phase | Impact | Effort | Ship independently? |
|-------|--------|--------|---------------------|
| **Phase 1** | Compile-time capability checking | M | ✅ Yes — immediate value |
| **Phase 2** | WASM import pruning + manifest | S | Potentially — import inventory, not a complete behavior proof |
| **Phase 3** | Per-dependency restriction | S | Potentially — one supply-chain control, not complete safety |
| **Phase 4** | Type-level effects (HOF) | L | ✅ Yes — precision |
| **Phase 5** | Agent stdlib module | S | ✅ Yes — stdlib only, no language changes |

Phase 1 supplies a static permission check, not a sufficient basis for calling an arbitrary agent deployment sandboxed.

## What We Don't Do

- Algebraic effect handlers — too complex, not needed for capability checking
- User-defined effect categories — current registry has 14 built-in leaves
- Effect syntax in function signatures — capabilities declared in almide.toml, not code
- Dynamic capability requests are not part of this proposal; host grants and runtime enforcement still matter
