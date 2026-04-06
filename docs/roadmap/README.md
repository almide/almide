# Almide Roadmap

> Auto-generated from directory structure. Run `bash docs/roadmap/generate-readme.sh > docs/roadmap/README.md` to update.
>
> [GRAND_PLAN.md](GRAND_PLAN.md) — 5-phase strategy

## Active

4 items

| Item | Description |
|------|-------------|
| [Capability-Based Effect System](active/effect-system-capability.md) | Capability-based effect system for sandboxed AI agent containers |
| [Fan Concurrency — Next Generation](active/fan-concurrency-next.md) | fan as a language-level concurrency primitive with Flow[T] and compiler-driven optimization |
| [`almide update` — Dependency Update Command](active/package-manager-update.md) | Add almide update command to refresh dependencies and rewrite lock file |
| [Package Version Resolution](active/package-version-resolution.md) | MVS version resolution with semver constraints for almide.toml |

## On Hold

23 items

| Item | Description |
|------|-------------|
| [Almide-to-Almide FFI via almide-lander](on-hold/almide-to-almide-ffi.md) | Use almide-lander to call compiled Almide libraries from Almide via shared library FFI |
| [Almide UI — Reactive Web Framework as Almide Library](on-hold/almide-ui.md) | SolidJS-like reactive UI framework built as a pure Almide library |
| [API Diff & Automatic Versioning](on-hold/api-diff-auto-versioning.md) | Automatic semver bump detection via public API diffing |
| [LLM Benchmark: Next Phase](on-hold/benchmark-next-phase.md) | LLM benchmark Phase 2-3: cross-language comparison, harder problems, publication |
| [Compile-Time Contracts](on-hold/compile-time-contracts.md) | Compile-time preconditions and type invariants via where clauses |
| [Error-Fix Database](on-hold/error-fix-db.md) | Structured error-to-fix mapping for LLM auto-repair of compiler errors |
| [GPU Compute — Matrix Type and Compiler-Driven GPU Execution](on-hold/gpu-compute.md) | Matrix primitive type with compiler-driven CPU/GPU execution |
| [IR Optimization Tier 2](on-hold/ir-optimization-tier2.md) | CSE and inlining passes for cross-target IR optimization |
| [LLM Integration](on-hold/llm-integration.md) | Built-in LLM commands for library generation, auto-fix, and code explanation |
| [LSP Code Actions](on-hold/lsp-code-actions.md) | LSP code actions for auto-fix, refactoring, and import management |
| [LSP Server](on-hold/lsp.md) | Language Server Protocol for editor completion, diagnostics, and navigation |
| [Package Registry](on-hold/package-registry.md) | Lock file, semver resolution, and central package registry |
| [Performance Research: Path to World #1](on-hold/performance-research.md) | Research plan to surpass hand-written Rust via semantic-aware optimization |
| [Rainbow Bridge — Wrap External Code as Almide Packages](on-hold/rainbow-bridge.md) | Wrap external Rust/TS/Python code as native Almide packages via @extern |
| [Research: Modification Survival Rate Paper](on-hold/research-modification-survival-rate-paper.md) | Academic paper measuring LLM code modification survival across languages |
| [The Rumbling — Almide OSS Rewrite Campaign](on-hold/rumbling.md) | Campaign to rewrite OSS tools in Almide to prove WASM size and LLM accuracy |
| [Secure by Design](on-hold/secure-by-design.md) | Five-layer security model making web vulnerabilities compile-time errors |
| [Snapshot Testing](on-hold/snapshot-testing.md) | Built-in snapshot testing for output regression detection |
| [Supervision & Actors](on-hold/supervision-and-actors.md) | Erlang-style actors, supervisors, and typed channels as stdlib modules |
| [WASM Component Model](on-hold/wasm-component-model.md) | WebAssembly Component Model support with WIT bindings |
| [WASM Exception Handling](on-hold/wasm-exception-handling.md) | WASM native exception handling (try_table/throw) for zero-cost effect fn error propagation |
| [WASM HTTP Client](on-hold/wasm-http-client.md) | HTTP client support for the WASM target via WASI or host imports |
| [Web Framework](on-hold/web-framework.md) | First-party Hono-like web framework with template and Codec integration |

## Done

194 items

<details>
<summary>Show all 194 completed items</summary>

| Done | Item | Description |
|------|------|-------------|
