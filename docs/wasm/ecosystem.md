# WASM Agent Ecosystem

Where Almide sits in the 2026 agent infrastructure landscape.

## Protocol Layer

| Protocol | Purpose | Status | Almide strategy |
|----------|---------|--------|-----------------|
| **MCP** | Agent ↔ Tool | Standard (97M DL/月, AAIF governance) | **hatch implements this** |
| **A2A** | Agent ↔ Agent | Draft (Google, 150+ orgs) | Future (separate tool) |
| **AG-UI** | Agent ↔ Frontend | Growing (AWS/Google/MS) | Not relevant for containers |
| **WebMCP** | Browser ↔ Agent | W3C draft, Chrome preview | Not relevant for server-side |

**JSON Schema** is the universal tool description format across all protocols.

## Runtime Layer

| Runtime | WASM 3.0 | Tail calls | Multi-memory | Container ecosystem |
|---------|----------|------------|--------------|---------------------|
| **Wasmtime** | Yes | default ON | default ON | Spin, wasmCloud, Docker+WASM |
| **WasmEdge** | Yes | default ON | default ON | Docker+WASM, K8s |
| **V8** | Yes | Yes | Yes | Cloudflare Workers |
| **Wasmer** | Partial | Unclear | Unclear | Wasmer Edge |

Almide targets wasmtime. hatch embeds wasmtime.

## Tool Layer

| Tool | What | Almide relationship |
|------|------|---------------------|
| **Wassette** (Microsoft) | Generic WASM Component → MCP bridge | Competitor to hatch. Needs WIT + Component Model. |
| **hatch** (Almide) | Almide WASM → MCP bridge | Almide-specific. Uses manifest.json, not WIT. Thinner. |
| **wasm-tools** | WASM validation, Component wrapping | Used for validation (`wasm-tools validate`) |

### hatch vs Wassette

| | hatch | Wassette |
|---|---|---|
| Input | Almide core module + manifest.json | Any WASM Component + WIT |
| WIT parser | Not needed | Required |
| Component Model | Not needed | Required |
| Tool definitions | From manifest.json (compile-time) | From WIT exports (runtime) |
| Capability system | Almide compiler + WASI | Deny-by-default (runtime) |
| Size | ~500 LOC Rust | ~5000+ LOC Rust |
| Scope | Almide-only | Any WASM Component |

hatch is smaller and faster because it only needs to work with Almide's known output format. Wassette is more general but more complex.

## Framework Layer

| Framework | Protocol | Relevance |
|-----------|----------|-----------|
| LangChain / LangGraph | MCP adapter | Can call hatch-served agents |
| CrewAI | MCP + A2A native | Can call hatch-served agents |
| OpenAI Agents SDK | MCP integration | Can call hatch-served agents |
| Microsoft Agent Framework | MCP extension | Can call hatch-served agents |

All major frameworks are MCP clients. hatch-served agents are automatically compatible.

## Governance

**AAIF (Agentic AI Foundation)** under Linux Foundation. Platinum members: Anthropic, OpenAI, Google, Microsoft, AWS, Block, Bloomberg, Cloudflare. Houses MCP, AGENTS.md, goose.

MCP is not Anthropic-proprietary. It's industry-governed.

## Where Almide Is Unique

No other system provides all of:

1. **Compile-time capability enforcement** — violation = compile error, not runtime exception
2. **Binary-level import pruning** — disallowed WASI imports physically absent
3. **WASM 3.0 sandbox** — tail calls + multi-memory + capability-based security
4. **MCP-native** — standard protocol, zero custom integration
5. **KB-sized agents** — vs MB/GB Docker images

Closest alternatives:
- **Deno**: Runtime permissions, not compile-time. No WASM sandbox.
- **Wassette + Rust**: WASM sandbox, but no compile-time capability checking. Rust has no effect system.
- **Docker**: OS-level sandbox, but MB-sized images, no compile-time guarantees.
