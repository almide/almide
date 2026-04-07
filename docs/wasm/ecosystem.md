# WASM Agent Ecosystem

Where Almide sits in the 2026 agent infrastructure landscape.

---

## 1. Protocol Layer Overview

| Protocol | Purpose | Status (Apr 2026) | Almide strategy |
|----------|---------|-------------------|-----------------|
| **MCP** | Agent <-> Tool | Standard (97M DL/mo, AAIF governance) | **hatch implements this** |
| **A2A** | Agent <-> Agent | Draft v0.3 (Google, 150+ orgs) | Future (separate tool) |
| **AG-UI** | Agent <-> Frontend | Growing (AWS/Google/MS) | Not relevant for containers |
| **WebMCP** | Browser <-> Agent | W3C draft, Chrome 146 preview | Not relevant for server-side |

**JSON Schema** is the universal tool description format across all protocols.

---

## 2. MCP Deep Dive

### 2.1 Protocol Version History

MCP uses date-based version identifiers (`YYYY-MM-DD`). Each version marks the last date backwards-incompatible changes were made.

| Version | Date | Key Changes |
|---------|------|-------------|
| **2024-11-05** | Nov 2024 | Initial release. Anthropic open-sources MCP. Python + TypeScript SDKs. stdio + HTTP+SSE transports. |
| **2025-03-26** | Mar 2025 | OAuth 2.1 authorization framework. **Streamable HTTP** replaces HTTP+SSE. Tool annotations (`readOnly`, `destructive`). Audio content type. Progress message fields. |
| **2025-06-18** | Jun 2025 | Structured JSON tool outputs. Enhanced OAuth security. **Elicitation**: servers can request user input mid-session via `elicitation/create`. JSON-RPC batching removed. |
| **2025-11-25** | Nov 2025 | **Tasks**: new abstraction for tracking server work with queryable status. OpenID Connect Discovery. Icons metadata for tools/resources/prompts. Incremental scope consent. URL mode elicitation. Sampling tool calling. **Extensions** framework for scenario-specific additions outside core spec. |

### 2.2 Transport Evolution

```
Nov 2024          Mar 2025           May 2025           Nov 2025
   |                 |                  |                  |
   v                 v                  v                  v
 stdio           Streamable        HTTP+SSE            Streamable HTTP
 HTTP+SSE        HTTP added         deprecated          is the standard
                                                        stdio remains
```

**stdio** -- Communication over stdin/stdout. Designed for local MCP connections. Most common, most interoperable. Recommended for local tooling.

**HTTP+SSE (deprecated)** -- Original remote transport from 2024-11-05. Server pushes events via Server-Sent Events. Client sends via HTTP POST. Stateful, hard to scale horizontally.

**Streamable HTTP** -- Replaces HTTP+SSE as of 2025-03-26. Single endpoint (e.g., `/mcp`) accepts POST (JSON-RPC messages) and GET (optional SSE streaming). Works with standard load balancers, CORS policies, and auth middleware. Stateless-friendly. The server operates as an independent process handling multiple client connections.

### 2.3 Key Concepts

MCP uses [JSON-RPC 2.0](https://www.jsonrpc.org/) messages between three roles:

| Role | Description |
|------|-------------|
| **Host** | LLM application that initiates connections (e.g., Claude Desktop, Cursor, VS Code) |
| **Client** | Connector within the host that manages a 1:1 session with a server |
| **Server** | Service that provides context and capabilities |

#### Server Features (server -> client)

| Feature | Description |
|---------|-------------|
| **Tools** | Functions the AI model can execute. Schema-defined with typed inputs/outputs. Annotated with behavior hints (`readOnly`, `destructive`). |
| **Resources** | Contextual data (files, database records, API responses) exposed for the model or user to read. URI-addressed. |
| **Prompts** | Templated messages and workflows. Reusable instruction sets that guide model behavior for specific tasks. |

#### Client Features (client -> server)

| Feature | Description |
|---------|-------------|
| **Sampling** | Server requests the client to run an LLM completion on its behalf. Enables agentic workflows where a tool needs model reasoning to complete its task. User must approve. |
| **Elicitation** | Server requests additional information from the user mid-session. Sends a message + JSON schema describing what input is needed. Supports text, confirmation, and URL modes. |
| **Roots** | Server queries the client about URI/filesystem boundaries it should operate within. Defines the working scope. |

#### Additional Utilities

Logging, progress tracking, cancellation, error reporting, configuration negotiation.

### 2.4 SDK Landscape

| Language | Maintainer | Maturity | Notes |
|----------|-----------|----------|-------|
| **TypeScript** | Official (MCP org) | Stable | Most downloads. First SDK released Nov 2024. |
| **Python** | Official (MCP org) | Stable | Second-most downloads. Released Nov 2024. |
| **Java** | Official (MCP org) | Stable | Spring AI integration. |
| **Kotlin** | Official + JetBrains | Stable | Collaboration with JetBrains. |
| **Go** | Official (MCP org) | Stable | |
| **C#** | Official (MCP org) | Stable | .NET integration. |
| **Ruby** | Official (MCP org) | GA | |
| **PHP** | Official + PHP Foundation | GA | |
| **Swift** | Official (MCP org) | GA | |
| **Rust** | Community | Growing | Not yet official. Multiple crates. |

All SDKs provide: MCP server creation (tools, resources, prompts), MCP client creation (connect to any server), protocol compliance, type safety.

### 2.5 Adoption (Apr 2026)

| Metric | Value |
|--------|-------|
| Monthly SDK downloads | **97M+** (Python + TypeScript combined) |
| Published MCP servers | **10,000+** across public registries |
| Company-operated servers | **1,412** (Feb 2026, up 232% in 6 months) |
| Remote MCP servers | **4x growth** since May 2025 |
| Global monthly searches | **622,000+** for top 50 servers |

**First-class client support**: Claude, ChatGPT, Gemini, Microsoft Copilot, Cursor, VS Code, Zed, Replit, Windsurf.

**Vendor-published servers**: GitHub, Stripe, Atlassian, Salesforce, Snowflake, Cloudflare, Elastic, and hundreds more.

### 2.6 AAIF Governance

In December 2025, Anthropic donated MCP to the **Agentic AI Foundation (AAIF)**, a directed fund under the Linux Foundation. MCP is not Anthropic-proprietary. It is industry-governed.

**Co-founders**: Anthropic, Block, OpenAI.

**Platinum members**: Amazon Web Services, Anthropic, Block, Bloomberg, Cloudflare, Google, Microsoft, OpenAI.

**Hosted projects**: MCP, goose (Block), AGENTS.md (OpenAI).

#### Governance Structure

| Role | Responsibility |
|------|---------------|
| **Lead Maintainers** | Final authority. Can veto any decision. |
| **Core Maintainers** | Steer specification and project direction. Appoint/remove Maintainers. |
| **Maintainers** | Day-to-day spec work within Working Groups. |
| **Working Groups** | Transports WG, Auth WG, Registry WG, Governance WG, and others. |
| **Governing Board** | Strategic investments, budget, member recruitment, project approval. |

Changes are proposed through **Specification Enhancement Proposals (SEPs)**. The Governance WG is building a contributor ladder and delegation model so Working Groups can accept SEPs in their domain without full core review.

#### 2026 Roadmap Priorities

1. **Transport scalability** -- Stateless session model, horizontal scaling, `.well-known` metadata discovery
2. **MCP Registry** -- Centralized discovery service ("app store for MCP servers"), namespace verification via DNS TXT records
3. **Extensions framework** -- Scenario-specific additions outside core spec, published by Working Groups
4. **Governance maturation** -- Contributor ladder, WG delegation, charter templates
5. **Enterprise readiness** -- Production patterns for multi-agent systems at scale

**MCP Dev Summit North America 2026**: April 2-3, New York City. 95+ sessions.

---

## 3. A2A Deep Dive

### 3.1 Overview

The **Agent-to-Agent (A2A) protocol** was launched by Google in April 2025 with 50+ technology partners. It enables AI agents to discover each other, communicate, and coordinate actions across organizational boundaries.

A2A addresses a problem MCP explicitly does not solve: **agent-to-agent orchestration**. MCP gives agents hands (tool access). A2A gives agents colleagues.

### 3.2 Agent Card Specification

The Agent Card is a JSON document hosted at `/.well-known/agent-card.json`. It is the agent's "business card" for discovery.

```json
{
  "name": "Recipe Agent",
  "description": "Finds and recommends recipes based on ingredients",
  "version": "1.0.0",
  "url": "https://recipe-agent.example.com",
  "provider": {
    "organization": "FoodCorp",
    "url": "https://foodcorp.example.com"
  },
  "documentationUrl": "https://docs.example.com/recipe-agent",
  "iconUrl": "https://example.com/icon.png",
  "capabilities": {
    "streaming": true,
    "pushNotifications": true,
    "stateTransitionHistory": false
  },
  "authentication": {
    "schemes": ["Bearer"]
  },
  "defaultInputModes": ["text/plain", "application/json"],
  "defaultOutputModes": ["text/plain", "application/json"],
  "skills": [
    {
      "id": "find-recipe",
      "name": "Find Recipe",
      "description": "Search recipes by ingredients",
      "inputModes": ["text/plain"],
      "outputModes": ["application/json"]
    }
  ]
}
```

Key fields: `name`, `description`, `version`, `url`, `provider`, `capabilities`, `authentication`, `skills` (with `id`, `name`, `description`, `inputModes`, `outputModes`, `examples`), `defaultInputModes`, `defaultOutputModes`, `supported_interfaces`.

### 3.3 Task Lifecycle

The **Task** is the fundamental unit of work. Each task has a unique ID and progresses through defined states:

```
                    +---> input-required ---+
                    |                       |
submitted ---> working ---> completed       |
                    |                       |
                    +---> failed            |
                    |                       |
                    +<----------------------+
```

| State | Description |
|-------|-------------|
| `submitted` | Client has sent the task request |
| `working` | Agent is actively processing |
| `input-required` | Agent needs additional information from the client |
| `completed` | Task finished successfully |
| `failed` | Task failed with error |

Tasks can complete immediately or run long, with agents exchanging status updates via streaming or push notifications. The protocol supports both synchronous (single request-response) and asynchronous (long-running with polling/streaming) patterns.

### 3.4 Architecture Layers

| Layer | Purpose |
|-------|---------|
| **Layer 1: Canonical Data Model** | Core data structures as Protocol Buffer messages |
| **Layer 2: Abstract Operations** | Task creation, status query, cancellation |
| **Layer 3: Protocol Bindings** | Concrete mappings to JSON-RPC, gRPC, HTTP/REST |

### 3.5 How A2A Complements MCP

| Dimension | MCP | A2A |
|-----------|-----|-----|
| **Relationship** | Agent <-> Tool | Agent <-> Agent |
| **Discovery** | Registry / `.well-known` (planned) | Agent Card at `/.well-known/agent-card.json` |
| **Communication** | JSON-RPC 2.0 | JSON-RPC / gRPC / HTTP |
| **State model** | Stateful sessions | Task lifecycle with defined states |
| **Opacity** | Server internals visible (tools, resources) | Agents are opaque to each other |
| **Use case** | "Call this function" | "Delegate this job to a specialist" |

A complete enterprise agent stack in 2026 uses both: MCP for tool access, A2A for agent coordination.

### 3.6 Adoption Status

- **v0.3** released with more stable interfaces for enterprise adoption
- 150+ organizations supporting the protocol
- Integrated into Google Vertex AI Agent Engine, Amazon Bedrock AgentCore, Spring AI
- Now also under AAIF governance alongside MCP
- Production-ready version planned for 2026
- Adoption slower than MCP due to: later launch (Apr 2025 vs Nov 2024), draft spec status, and MCP's head start on the simpler tool-access problem

---

## 4. Wassette Deep Dive

### 4.1 Overview

**Wassette** is a security-oriented runtime from Microsoft's Azure Core Upstream team. Written in Rust, built on Wasmtime. It bridges WebAssembly Components to MCP, making any WASM component instantly available as an MCP tool.

Released: August 2025. Open source on GitHub.

### 4.2 Architecture

```
                    +-----------------+
  MCP Client  <-->  |    Wassette     |  <-->  OCI Registry
  (Claude,          |  (Rust/Wasmtime)|        (ghcr.io, etc.)
   Cursor, etc.)    +--------+--------+
                             |
                    +--------v--------+
                    | WASM Component  |
                    | (WIT interfaces)|
                    +-----------------+
```

1. Agent (or user) requests a tool
2. Wassette fetches the WASM Component from an OCI registry
3. Wassette reads the component's **WIT** (WebAssembly Interface Types) to discover exported functions
4. WIT exports are mapped to MCP tool definitions (name, description, input/output schemas)
5. Tool calls are routed to the sandboxed WASM component
6. Results are returned via MCP

### 4.3 Permission Model

**Deny-by-default**. Every component starts with zero access:

| Resource | Default | Granting |
|----------|---------|----------|
| File system | Denied | Explicit path allow-list |
| Network | Denied | Explicit endpoint allow-list |
| Environment variables | Denied | Explicit variable allow-list |
| System clock | Denied | Explicit grant |

Interactive permission prompts let users approve/deny at runtime. Fine-grained allow/deny lists provide control over specific paths and endpoints.

### 4.4 OCI Registry Integration

Wassette can fetch WASM Components directly from OCI-compliant registries (ghcr.io, Docker Hub, Azure Container Registry). This enables:

- **Dynamic tool loading**: Agents fetch exactly the tool they need, run it, discard it
- **Version pinning**: OCI tags/digests ensure reproducible tool versions
- **Distribution**: Standard container registry infrastructure, no custom hosting needed

The missing piece (as of early 2026): teaching Wassette to automatically discover components in OCI registries without pre-configured URLs.

### 4.5 hatch vs Wassette (Detailed)

| Dimension | hatch (Almide) | Wassette (Microsoft) |
|-----------|---------------|---------------------|
| **Input format** | Almide core module + `manifest.json` | Any WASM Component + WIT |
| **WIT parser** | Not needed | Required (reads WIT to discover tools) |
| **Component Model** | Not needed | Required |
| **Tool definitions** | From `manifest.json` (compile-time) | From WIT exports (runtime) |
| **Capability enforcement** | Compile-time (Almide compiler) + WASI | Runtime (deny-by-default sandbox) |
| **Security guarantee** | Disallowed imports physically absent from binary | Disallowed calls intercepted at runtime |
| **OCI integration** | Not needed (Almide builds directly) | Core feature (dynamic fetch) |
| **Codebase size** | ~500 LOC Rust | ~5,000+ LOC Rust |
| **Language scope** | Almide only | Any language that compiles to WASM Components |
| **Cold start** | Microseconds (pre-built binary) | Milliseconds (fetch + validate + instantiate) |
| **When to use** | You write the agent in Almide | You want to bridge existing WASM tools |

hatch is smaller and faster because it only works with Almide's known output format. Wassette is more general but more complex. They solve different problems: hatch is a deployment tool for Almide agents, Wassette is a universal WASM-to-MCP bridge.

---

## 5. WASI Ecosystem

### 5.1 Version Timeline

```
2019            2024-01          2025            2026-02         2026 late / 2027
  |                |               |                |               |
  v                v               v                v               v
Preview 1       Preview 2       P2 stable       Preview 3 RC    WASI 1.0
(0.1)           (0.2.0)         (0.2.1)         (0.3.0-rc)      (target)
POSIX-like      Component       Refinements     Native async    Stable standard
                Model                           Future/Stream
```

### 5.2 Version Comparison

| Feature | Preview 1 (0.1) | Preview 2 (0.2) | Preview 3 (0.3) |
|---------|-----------------|------------------|------------------|
| **Released** | 2019 | Jan 2024 | RC: Feb 2026 |
| **API style** | POSIX-like functions | Component Model interfaces | Async Component Model |
| **File I/O** | Yes | Yes (wasi:filesystem) | Yes |
| **Networking** | No | Yes (wasi:sockets, wasi:http) | Yes |
| **Threading** | No | No | No (gap remains) |
| **Async I/O** | No | No | Yes (native futures/streams) |
| **Component Model** | No | Yes | Yes (enhanced) |
| **Language interop** | Via imports/exports | Via WIT interfaces | Via WIT + async |

### 5.3 Component Model Status

The Component Model enables composing WASM modules from different languages into a single application via WIT interfaces.

| Status | Details |
|--------|---------|
| **W3C phase** | Phase 2/3 (proposal, not final standard) |
| **Server-side** | Production-ready on WASI 0.2 |
| **Browser** | Not yet supported |
| **Threading** | Real gap for compute-heavy workloads |
| **Leading runtime** | Wasmtime (first full support for loading components) |

### 5.4 Runtime Support Matrix

| Runtime | WASI 0.1 | WASI 0.2 | WASI 0.3 | Component Model | Tail calls | Multi-memory |
|---------|----------|----------|----------|-----------------|------------|--------------|
| **Wasmtime** | Yes | Yes (full) | RC support (v37+) | Full | Default ON | Default ON |
| **WasmEdge** | Yes | Yes | In progress | Catching up | Default ON | Default ON |
| **V8** | Yes | Partial | No | No | Yes | Yes |
| **Wasmer** | Yes | Partial | No | Catching up | Partial | Partial |

### 5.5 Timeline to WASI 1.0

The WASI 1.0 release is expected in **late 2026 or early 2027**. The Component Model proposal will start advancing through W3C specification phases after WASI 0.3 or 1.0 ships. WASI 1.0 will unify the interfaces and provide a stable target for long-term binary compatibility.

---

## 6. Container Runtime Comparison

### 6.1 Platform Matrix

| Platform | Engine | WASI Version | Component Model | Cold Start | Language Support | MCP Support |
|----------|--------|-------------|-----------------|------------|-----------------|-------------|
| **Docker+WASM** | Wasmtime, WasmEdge, Spin (via runwasi shims) | 0.1, 0.2 | Via Spin shim | ~ms | Any -> WASM | Via sidecar |
| **Spin (Fermyon)** | Wasmtime | 0.2, 0.3-rc (v3.5+) | Yes | Microseconds | Rust, Go, JS, Python, C# | Via Spin MCP plugin |
| **wasmCloud** | Wasmtime | 0.2, 0.3 (roadmap) | Yes (core design) | Microseconds | Rust, Go, TinyGo, JS | Via capability provider |
| **Fastly Compute** | Wasmtime (custom) | 0.1, 0.2 | Limited | Microseconds | Rust, Go, JS | No native support |
| **Cloudflare Workers** | V8 | N/A (V8 isolates) | No | <5ms | JS/TS, Rust->WASM | Native (Cloudflare MCP) |
| **Wasmer Edge** | Wasmer | 0.1, partial 0.2 | Limited | ~ms | Rust, C, JS, Python | No native support |

### 6.2 Architecture Approaches

**Docker+WASM**: Uses `containerd` shims via the `runwasi` project. Run WASM workloads with `--runtime=io.containerd.wasmtime.v1 --platform=wasi/wasm`. Multi-company effort (Microsoft, Docker, Fermyon, Second State). Bridges the existing Docker ecosystem to WASM.

**Spin**: Opinionated framework for serverless WASM apps. Built-in support for HTTP handlers, queues, timers. Runs on SpinKube (Kubernetes), Fermyon Cloud, Azure AKS. First WASIp3 RC support (Spin v3.5, Nov 2025). Can even run Cloudflare Workers inside Spin apps.

**wasmCloud**: Capability-driven WASM platform. Components declare capabilities via WIT; the runtime provides them. Strong emphasis on portability and zero-trust networking. Next-gen runtime (`wash-runtime`) embeds `wasi:http` as core host plugin.

**Fastly Compute**: Custom Wasmtime fork optimized for edge. Instance instantiation in microseconds. Focuses on HTTP request handling at CDN scale. Less emphasis on general-purpose components.

**Cloudflare Workers**: V8 isolates, not traditional WASM runtimes. Rust/C++ compile to WASM and run inside V8. Isolates spin up in <1ms. Massive global network (300+ cities). Native MCP support via Cloudflare's agents framework.

### 6.3 MCP/A2A Integration Status

| Platform | MCP Server | MCP Client | A2A | Notes |
|----------|-----------|-----------|-----|-------|
| **Docker+WASM** | Via app code | Via app code | Via app code | No built-in protocol support |
| **Spin** | Plugin available | Via app code | No | Fermyon MCP plugin |
| **wasmCloud** | Via capability provider | Via capability provider | No | Capability-based integration |
| **Fastly Compute** | No | No | No | Focus on HTTP edge, not agents |
| **Cloudflare Workers** | **Native** | **Native** | No | First-class MCP in agents framework |
| **Wassette** | **Native** | N/A (is an MCP server) | No | Purpose-built for MCP |
| **hatch** | **Native** | N/A (is an MCP server) | No | Purpose-built for MCP |

---

## 7. Competitive Landscape for "AI Agent Containers"

### 7.1 What Exists Today

| Approach | Sandbox Model | Image Size | Cold Start | Capability Control | Example |
|----------|--------------|-----------|------------|-------------------|---------|
| **Docker containers (Python)** | OS-level (shared kernel) | 100MB-1GB+ | 1-10s | None (full OS access) | LangChain agent in Docker |
| **MicroVMs (Firecracker)** | Hardware-level | 5MB+ overhead | ~125ms | VM boundary only | E2B, Modal |
| **gVisor containers** | User-space kernel | Same as Docker | ~500ms | Syscall filtering | GKE Sandbox |
| **Deno agents** | Runtime permissions | 50-100MB | ~100ms | Runtime flags (`--allow-net`, etc.) | Deno Deploy agents |
| **V8 isolates** | Isolate boundary | <1MB | <1ms | Platform-defined | Cloudflare Workers |
| **WASM (generic)** | Linear memory sandbox | <1MB | Microseconds | WASI capability-based | Wassette + Rust |
| **WASM (Almide)** | Linear memory + compile-time | **KB-sized** | Microseconds | **Compile-time + binary-level** | hatch |

### 7.2 The Security Spectrum

```
Weaker                                                          Stronger
   |                                                               |
   v                                                               v
Docker/runc --> gVisor --> MicroVM --> V8 isolate --> WASM --> Almide WASM
(shared        (syscall   (hardware   (memory      (linear   (compile-time
 kernel)        filter)    isolation)   isolation)   memory    enforcement +
                                                    sandbox)  binary pruning)
```

The industry consensus in 2026: shared-kernel container isolation (Docker/runc) is insufficient for executing untrusted AI agent code. The shared kernel expands the blast radius. Platforms are moving toward stronger isolation:

- **Cloudflare, Vercel, Ramp, Modal** shipped sandbox features in 2025-2026
- **E2B, Northflank, Firecrawl** built entire platforms around agent sandboxing
- **Docker** launched experimental Docker Sandboxes specifically for AI isolation

### 7.3 Almide's Unique Position

The gap in the market:

| Existing approach | What it lacks |
|-------------------|---------------|
| Docker + Python agents | No compile-time guarantees. Full OS access unless manually restricted. GB-sized images. |
| Deno agents | Runtime permission checks (not compile-time). Not WASM-sandboxed. No binary pruning. |
| Wassette + Rust | WASM sandbox, but no compile-time capability checking. Rust has no effect system. Permissions enforced at runtime. |
| Cloudflare Workers | V8 isolates, not WASM sandbox. Platform-locked. No compile-time capability proof. |

What Almide provides that no other system does -- all at once:

1. **Compile-time capability enforcement** -- Violation = compile error, not runtime exception. The type system tracks effects. If your code does not declare `effect fn`, it physically cannot perform I/O.

2. **Binary-level import pruning** -- Disallowed WASI imports are physically absent from the binary. Not intercepted at runtime. Not present at all. There is no code path to exploit.

3. **WASM 3.0 sandbox** -- Tail calls (O(1) stack recursive agents) + multi-memory + capability-based security. Linear memory isolation with bounds checking on every access.

4. **MCP-native** -- Standard protocol. Zero custom integration. Any MCP client (Claude, ChatGPT, Cursor, Copilot) connects to a hatch-served agent immediately.

5. **KB-sized agents** -- Typical agent binary: 10-50 KB. Compare: Docker Python agent image: 500MB-1GB. That is a 10,000x-100,000x difference. Cold start in microseconds vs seconds.

### 7.4 The Market Opportunity

The fundamental tension in the AI agent ecosystem:

> **"Agents can do anything"** vs **"Agents should do only what they are allowed to do"**

Every sandbox approach today enforces restrictions at runtime. The agent binary contains the code to do forbidden things; the runtime just blocks it. This is the browser security model: the code is untrusted, so the sandbox intercepts dangerous calls.

Almide inverts this. The compiler guarantees that forbidden code does not exist in the binary. The sandbox still runs (defense in depth), but it has nothing to catch. This is the capability-based security model: you cannot call what you do not have.

For regulated industries (healthcare, finance, government) where auditability matters, "the binary physically cannot do X" is a fundamentally stronger guarantee than "the runtime will try to prevent X."

---

## 8. Standards Bodies

### 8.1 AAIF (Agentic AI Foundation) -- Linux Foundation

| Aspect | Details |
|--------|---------|
| **Founded** | December 2025 |
| **Parent** | Linux Foundation (directed fund) |
| **Co-founders** | Anthropic, Block, OpenAI |
| **Platinum members** | AWS, Anthropic, Block, Bloomberg, Cloudflare, Google, Microsoft, OpenAI |
| **Hosted projects** | MCP, goose, AGENTS.md |
| **Governance** | Governing Board (strategy) + project-level autonomy (technical direction) |
| **Key events** | MCP Dev Summit NA 2026 (Apr 2-3, NYC, 95+ sessions) |

AAIF provides neutral, vendor-independent governance for agentic AI standards. Individual projects (MCP, goose, AGENTS.md) maintain full autonomy over technical direction. The Governing Board handles strategic investments, budget, and member recruitment.

Both MCP and A2A are now under AAIF governance.

### 8.2 W3C -- WebMCP and Agent Protocol

#### WebMCP (Web Machine Learning Community Group)

| Aspect | Details |
|--------|---------|
| **Developers** | Google, Microsoft (joint) |
| **Status** | W3C Draft Community Group Report (Feb 10, 2026) |
| **Browser preview** | Chrome 146 Canary |
| **API** | `navigator.modelContext` -- exposes structured tools to AI agents in the browser |
| **Purpose** | Replace unreliable DOM manipulation / visual recognition with semantic, tool-based protocols for browser automation |

WebMCP is for browser-side agent interaction. Not relevant for server-side Almide agents, but indicates the broader standardization trend.

#### AI Agent Protocol Community Group

| Aspect | Details |
|--------|---------|
| **Proposed** | May 2025 |
| **Mission** | Develop open protocols for AI agent discovery, identification, and collaboration on the Web |
| **Distinct from** | WebML CG (which develops WebMCP) |

### 8.3 IETF -- Agent Authentication and Authorization

Multiple Internet-Drafts are active as of early 2026, reflecting urgent industry need for standardized agent identity:

| Draft | Published | Focus |
|-------|-----------|-------|
| **draft-prakash-aip-00** (Agent Identity Protocol) | Mar 2026 | Verifiable, delegable identity via Invocation-Bound Capability Tokens (IBCTs). JWT + Ed25519 for single-hop, Biscuit tokens for multi-hop delegation chains. |
| **draft-klrc-aiagent-auth-01** (AI Agent Auth) | Mar 2026 | Applies existing WIMSE architecture and OAuth 2.0 to agent authentication/authorization. |
| **draft-aap-oauth-profile-00** (Agent Authorization Profile) | Feb 2026 | OAuth 2.0 + JWT profile for autonomous AI agents. Structured claims for agent identity, task context, constraints, delegation chains, human oversight. |
| **draft-chen-ai-agent-auth-new-requirements-00** | 2026 | Requirements analysis: managing dynamic agent behavior rather than static identity verification. |
| **draft-yl-agent-id-requirements-00** | 2026 | Digital identity management requirements for agent communication protocols. |

None of these are RFCs yet. The space is fragmented but converging. The MCP spec already includes OAuth 2.1 for transport-level auth; the IETF work addresses the harder problem of agent-level identity and delegation chains across organizational boundaries.

---

## 9. Framework Integration

| Framework | Protocol Support | Almide Integration |
|-----------|-----------------|-------------------|
| **LangChain / LangGraph** | MCP adapter | Can call hatch-served agents |
| **CrewAI** | MCP + A2A native | Can call hatch-served agents |
| **OpenAI Agents SDK** | MCP integration | Can call hatch-served agents |
| **Microsoft Agent Framework** | MCP extension | Can call hatch-served agents |
| **Google ADK (Agent Dev Kit)** | MCP + A2A native | Can call hatch-served agents |
| **Spring AI** | MCP + A2A (v5) | Can call hatch-served agents |
| **Amazon Bedrock AgentCore** | MCP + A2A | Can call hatch-served agents |

All major frameworks are MCP clients. hatch-served agents are automatically compatible with the entire ecosystem.

---

## 10. Where Almide Is Unique

No other system provides all of:

1. **Compile-time capability enforcement** -- violation = compile error, not runtime exception
2. **Binary-level import pruning** -- disallowed WASI imports physically absent
3. **WASM 3.0 sandbox** -- tail calls + multi-memory + capability-based security
4. **MCP-native** -- standard protocol, zero custom integration
5. **KB-sized agents** -- vs MB/GB Docker images

Closest alternatives and what they lack:

| Alternative | Sandbox | Compile-time capability proof | Binary pruning | Agent size |
|-------------|---------|------------------------------|----------------|------------|
| **Deno** | Runtime permissions | No | No | 50-100MB |
| **Wassette + Rust** | WASM (runtime deny-by-default) | No (Rust has no effect system) | No | ~MB |
| **Docker** | OS-level | No | No | 100MB-1GB |
| **Cloudflare Workers** | V8 isolates | No | No | <1MB |
| **Almide + hatch** | **WASM (compile-time + runtime)** | **Yes** | **Yes** | **10-50 KB** |
