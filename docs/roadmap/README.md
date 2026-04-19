# Almide Roadmap

> Auto-generated from directory structure. Run `bash docs/roadmap/generate-readme.sh > docs/roadmap/README.md` to update.
>
> [GRAND_PLAN.md](GRAND_PLAN.md) — 5-phase strategy

## Active

28 items

| Item | Description |
|------|-------------|
| [almai — Multi-Provider LLM Client](active/almai-llm-client.md) | almai — multi-provider LLM client library, 8 providers shipped |
| [Almide Dojo — Continuous MSR Measurement](active/almide-dojo.md) | Daily automated MSR loop — 30 tasks, Claude 100%, Llama 61%, almai integration |
| [bytes: unify bounds-check semantics](active/bytes-bounds-check.md) | Unify bounds checking across bytes accessors with Option/Result returns |
| [Codegen Ideal Form](active/codegen-ideal-form.md) | WASM codegen redesign toward declarative dispatch and explicit symbol resolution |
| [Unify Diagnostic Emission with Docs](active/diagnostic-emit-doc-unification.md) | Unify diagnostic emission sites with their docs/diagnostics/*.md files |
| [Externalize `try:` Snippets from Rust Literals](active/diagnostic-snippet-externalization.md) | Move try: snippet text out of Rust literals into stdlib/diagnostics/*.almd |
| [Diagnostics: Here / Try / Hint Format](active/diagnostics-here-try-hint.md) | Standardize diagnostics to Here/Try/Hint three-part format with CI-verified hint correctness |
| [Dispatch Unification Plan (S3 Phase 1e)](active/dispatch-unification-plan.md) | Unify Rust + WASM stdlib dispatch via IR-level RuntimeCall; attributes become sugar |
| [DX & Codegen Papercuts](active/dx-codegen-papercuts.md) | Codegen bugs (effect fn unification) + DX papercuts (test stderr, explain, local imports) |
| [Capability-Based Effect System](active/effect-system-capability.md) | Capability-based effect system for sandboxed AI agent containers |
| [Fan Concurrency — Next Generation](active/fan-concurrency-next.md) | fan as a language-level concurrency primitive with rush/spawn/link/cancel |
| [Flow[T] — Lazy Streaming Sequences](active/flow-design.md) | Flow[T] lazy streaming sequences with flow.* namespace aligned with list.* verbs |
| [Flow[T] — User Specification (Draft)](active/flow-spec-draft.md) | Draft user-facing spec for Flow[T] — to be promoted to docs/specs/flow.md after Phase 1 |
| [Llama Inference Demo](active/llama-inference-demo.md) | End-to-end Llama inference demo on Almide, from 1-block to full token generation |
| [LLM-first Language](active/llm-first-language.md) | Plan to make Almide the language LLMs write most accurately, measured by dojo MSR |
| [`almide docs-gen` — llms.txt Auto-Generation](active/llms-txt-autogen.md) | Auto-generate llms.txt from canonical sources (CHEATSHEET, diagnostics, stdlib) |
| [lumen — Pure Graphics Math](active/lumen-graphics-math.md) | lumen — pure graphics math library (vec3, mat4, color), used by webgl/canvas/obsid |
| [MLIR Backend + Egg Rewrite Engine](active/mlir-backend-adoption.md) | MLIR backend + egg e-graph rewriter for pure-Almide optimal lowering |
| [`almide update` — Dependency Update Command](active/package-manager-update.md) | Add almide update command to refresh dependencies and rewrite lock file |
| [Package Version Resolution](active/package-version-resolution.md) | MVS version resolution with semver constraints for almide.toml |
| [Reimpl Lint: Signature-Match Detection of Stdlib Reimplementations](active/reimpl-lint.md) | Detect user fns whose signature matches a stdlib fn, suggest delegation |
| [Sized Numeric Types](active/sized-numeric-types.md) | Swift-style Int8/Int32/UInt32/Float32 scalar types; unblocks bytes redesign + Matrix[T] dtype |
| [Stdlib Declarative Unification — Toward a Single Source of Truth](active/stdlib-declarative-unification.md) | Drive stdlib toward a single source-of-truth: `.almd` + multi-target ABI attributes |
| [Stdlib Defs / Runtime Consistency Check](active/stdlib-defs-runtime-consistency.md) | CI check that stdlib/defs/*.toml declared types match runtime/rs/src/*.rs signatures |
| [Stdlib Symmetry Audit](active/stdlib-symmetry-audit.md) | Symmetry audit and lint for stdlib Option/Result/List/Set/Map to remove naming drift |
| [VarTable Unification](active/var-table-unification.md) | Unify program/module var_tables into a single program-level table |
| [Variant Exhaustiveness Refinement](active/variant-exhaustiveness-refinement.md) | Non-exhaustive match suggests missing arm code; unreachable arms become hard errors |
| [Whisper in Pure Almide](active/whisper-almide.md) | Whisper speech recognition implemented entirely in Almide |

## On Hold

26 items

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
| [Tiny ML Inference Runtime](on-hold/ml-inference.md) | Tiny ML inference runtime using compile-time model specialization |
| [Package Registry](on-hold/package-registry.md) | Lock file, semver resolution, and central package registry |
| [Performance Research: Path to World #1](on-hold/performance-research.md) | Research plan to surpass hand-written Rust via semantic-aware optimization |
| [Porta Embedded — Sub-10KB Almide IoT Agents on WASI Hosts](on-hold/porta-embedded.md) | Porta-style WASI agent runtime for IoT: <10KB Almide guests on tiny hosts |
| [Rainbow Bridge — Wrap External Code as Almide Packages](on-hold/rainbow-bridge.md) | Wrap external Rust/TS/Python code as native Almide packages via @extern |
| [Research: Modification Survival Rate Paper](on-hold/research-modification-survival-rate-paper.md) | Academic paper measuring LLM code modification survival across languages |
| [The Rumbling — Almide OSS Rewrite Campaign](on-hold/rumbling.md) | Campaign to rewrite OSS tools in Almide to prove WASM size and LLM accuracy |
| [Secure by Design](on-hold/secure-by-design.md) | Five-layer security model making web vulnerabilities compile-time errors |
| [Shell Completions](on-hold/shell-completions.md) | almide completions subcommand for bash/zsh/fish auto-completion |
| [Snapshot Testing](on-hold/snapshot-testing.md) | Built-in snapshot testing for output regression detection |
| [Supervision & Actors](on-hold/supervision-and-actors.md) | Erlang-style actors, supervisors, and typed channels as stdlib modules |
| [WASM Component Model](on-hold/wasm-component-model.md) | WebAssembly Component Model support with WIT bindings |
| [WASM Exception Handling](on-hold/wasm-exception-handling.md) | WASM native exception handling (try_table/throw) for zero-cost effect fn error propagation |
| [WASM HTTP Client](on-hold/wasm-http-client.md) | HTTP client support for the WASM target via WASI or host imports |
| [Web Framework](on-hold/web-framework.md) | First-party Hono-like web framework with template and Codec integration |

## Done

211 items

<details>
<summary>Show all 211 completed items</summary>

| Done | Item | Description |
|------|------|-------------|
| 2026-04-19 | [Compiler Version Pin](done/compiler-version-pin.md) | minimum compiler version pinning in almide.toml (Cargo rust-version style) |
| 2026-04-19 | [`cargo test --all` Cache Race](done/cargo-test-cache-race.md) | Fix parallel-cargo-test cache race in fix_test/run_test that masks real failures |
| 2026-04-17 | [Option/Result Bundled `.almd` — Not Cosmetic After All](done/option-result-bundled-cleanup.md) | Bundled option/result are signature-override layer; pick a path to consolidate |
| 2026-04-17 | [Bundled-Almide Stdlib — Ideal Form](done/bundled-almide-ideal-form.md) | Ideal form for bundled-Almide stdlib: one dispatch path, no patch-layer special cases |
| 2026-04-16 | [Cut v0.14.6 Release](done/release-0.14.6.md) | Cut v0.14.6 release from llm-first-phase2 branch |
| 2026-04-16 | [Bundled-Almide Dispatch for Stdlib Modules](done/bundled-almide-dispatch.md) | Let stdlib/<module>.almd extend TOML modules (codegen dispatch fix) |
| 2026-04-09 | [Distribution UX](done/distribution-ux.md) | GitHub Release binaries, one-line installer, and almide self-update |
| 2026-04-09 | [Custom WASM Host Imports](done/wasm-host-imports.md) | Allow WASM target to import functions from custom host modules |
| 2026-04-08 | [WASM Closure Mutable Capture](done/wasm-closure-mutable-capture.md) | WASM closure mutable capture via heap cell for var mutation in lambdas |
| 2026-04-08 | [Test Build with Native Deps](done/test-native-deps.md) | Fix almide test to compile tests that transitively import native-deps modules |
| 2026-04-08 | [Stdlib System APIs](done/stdlib-system-apis.md) | Add HTTP client, process spawn, and signal stdlib modules |
| 2026-04-08 | [Native Deps + Stdlib HTTP Coexistence](done/native-deps-stdlib-coexist.md) | Fix native-deps and stdlib HTTP coexistence in generated Cargo.toml |
| 2026-04-08 | [Fix WASM fs.list_dir Memory Corruption](done/wasm-fs-list-dir-corruption.md) | Fix WASM fs.list_dir memory corruption when building List[String] result |
| 2026-04-07 | [WASM Codegen Fixes (Resolved)](done/wasm-codegen-fixes.md) | WASM codegen issues were porta interpreter bugs, not codegen bugs |
| 2026-04-07 | [Variant Constructor as Function](done/variant-constructor-as-function.md) | Allow variant constructors to be passed as first-class functions |
| 2026-04-07 | [Import Self Submodule Resolution](done/import-self-submodule-resolution.md) | Fix import self.sub.module resolution for nested submodules |
| 2026-04-07 | [Float Bits Conversion](done/float-bits-conversion.md) | Add int.bits_to_float and float.to_bits for raw IEEE 754 conversion |
| 2026-04-06 | [WebAssembly 3.0 Target](done/wasm-3.md) | WebAssembly 3.0 v1: native tail calls (return_call) and multi-memory |
| 2026-04-06 | [Mono VarId Sharing](done/mono-varid-sharing.md) | Monomorphization shares VarIds across specializations, causing stale types in WASM/codegen |
| 2026-04-06 | [Crate Responsibility Violations](done/crate-responsibility-violations.md) | Fix 33 CLAUDE.md rule violations found by crate patrol audit |
| 2026-04-03 | [Type Inference Unification for Generic Functions](done/type-inference-unification.md) | Unify inference variables with named TypeVars in generic function bodies |
| 2026-04-03 | [Test Block Result Semantics](done/test-block-result-semantics.md) | Remove auto-unwrap of effect fn results in test blocks |
| 2026-04-03 | [Flexible Error Types](done/flexible-error-types.md) | User-defined error types in Result, and test block Result visibility |
| 2026-04-02 | [Versioned Module Codegen Bug](done/versioned-module-codegen-bug.md) | Fix versioned module name mismatch in codegen for package dependencies |
| 2026-04-02 | [UFCS Resolution in Dependency Modules](done/ufcs-in-dependency-modules.md) | UFCS method calls fail type resolution in dependency module context |
| 2026-04-02 | [Pipe Operator Precedence Redesign](done/pipe-operator-precedence.md) | Pipe |> precedence conflicts with + (list concat) and .. (range) |
| 2026-04-02 | [Almide Native cdylib — Scaffold in Almide, Not Rust](done/almide-native-cdylib.md) | Build .so/.dylib from pure Almide, eliminating Rust scaffolding from lander |
| 2026-04-02 | [`import self` in Dependency Packages Bug](done/import-self-in-dependency-bug.md) | Fix import self resolution in dependency packages (blocks almide-lander) |
| 2026-04-01 | [WASM Remaining FS Operations](done/wasm-remaining-fs.md) | Implement remaining filesystem operations for the WASM target |
| 2026-04-01 | [Typed AST Cache](done/typed-ast-cache.md) | Cache type annotations on AST nodes to eliminate re-inference in lowering |
| 2026-04-01 | [Type Expressiveness: Business Scenario Comparison](done/type-expressiveness-scenarios.md) | Business scenario comparison of type expressiveness across languages |
| 2026-04-01 | [Purity Exploitation — Leveraging fn/effect fn Distinction](done/purity-exploitation.md) | Exploit fn/effect fn purity for auto-parallelism and escape analysis |
| 2026-04-01 | [Nanopass Debug Dump](done/nanopass-debug-dump.md) | Environment-variable-controlled IR dump for each nanopass stage |
| 2026-04-01 | [Module Export: Almide Libraries for Every Language](done/module-export.md) | Export Almide modules as native packages for Python, JS/TS, Ruby, and WASM |
| 2026-04-01 | [LLM Benchmark Execution](done/benchmark-execution.md) | LLM accuracy benchmarks comparing Almide, Python, and MoonBit |
| 2026-04-01 | [Error Message Suggestions](done/error-message-suggestions.md) | Fuzzy matching suggestions in error messages (did you mean?) |
| 2026-04-01 | [Emit Readability](done/emit-readability.md) | Improve readability of generated Rust output |
| 2026-04-01 | [Crate Split](done/crate-split.md) | Split compiler into workspace crates for build parallelism and API boundaries |
| 2026-04-01 | [Compiler Fragility Hotspots](done/compiler-fragility-hotspots.md) | Fix fragile compiler internals: visitor pattern, ExprId duplication, UF isolation, Ty clone cost |
| 2026-04-01 | [Compiler Depth Matrix](done/compiler-depth-matrix.md) | Map Almide's compiler depth against industrial compilers and plan next tiers |
| 2026-04-01 | [Compiler Architecture: All 10s](done/compiler-architecture-10.md) | Achieve 10/10 on every compiler architecture quality metric |
| 2026-04-01 | [Codegen Perfection](done/codegen-perfection.md) | Make codegen bulletproof by learning from Gleam/Roc architecture patterns |
| 2026-04-01 | [Canonical AST](done/canonical-ast.md) | Introduce Canonical AST phase to separate name resolution from type checking |
| 2026-03-31 | [Rainbow FFI Gate](done/rainbow-gate.md) | Export Almide code as native-speed libraries callable from any language |
| 2026-03-31 | [Exhaustiveness Strengthening — Nested Patterns](done/exhaustiveness-strengthening.md) | Nested pattern exhaustiveness via Maranget's algorithm |
| 2026-03-31 | [Dead Code Elimination](done/dead-code-elimination.md) | Dependency-graph-based dead code elimination for smaller WASM binaries |
| 2026-03-30 | [Diagnostic end_col — Precise Error Underlines](done/diagnostic-end-col.md) | Track end column in diagnostics for precise error underlines |
| 2026-03-29 | [C FFI — Call C Libraries from Almide](done/c-ffi.md) | Call C libraries from Almide via @extern(c, ...) and extern "C" codegen |
| 2026-03-28 | [Module System Diamond Dependency Verification](done/module-system-diamond-verification.md) | Verify diamond dependency handling and fix remaining module system edge cases |
| 2026-03-27 | [Unwrap Operators: `!` `??` `?` `?.`](done/unwrap-operators.md) | Postfix !, ??, ? and ?. operators for explicit Result/Option unwrapping |
| 2026-03-27 | [HTTPS Native Support](done/https-native.md) | Native HTTPS support via rustls across all targets |
| 2026-03-27 | [Effect System — Auto-Inferred Capabilities](done/effect-system.md) | Auto-inferred effect capabilities with package-level permissions |
| 2026-03-25 | [WASM Filesystem I/O](done/wasm-fs-io.md) | WASI-based filesystem I/O for WASM target |
| 2026-03-25 | [TypeScript Test Runner](done/ts-test-runner.md) | almide test --target ts command with Deno/Node support |
| 2026-03-25 | [Diagnostic Secondary Spans](done/diagnostic-secondary-spans.md) | Activate secondary spans showing declaration sites in error messages |
| 2026-03-25 | [Benchmark Report: LLM Code Generation Cost by Language](done/benchmark-report.md) | Benchmark comparing LLM code generation cost across 16 languages |
| 2026-03-24 | [User Generics & Protocol System](done/user-generics-and-traits.md) | User-defined generics and protocol system for custom types |
| 2026-03-24 | [Test Coverage](done/test-coverage-v2.md) | Cross-target test coverage status (Rust/WASM/TS) |
| 2026-03-24 | [Stdlib Import Control](done/stdlib-import-control.md) | Three-tier import visibility for stdlib modules |
| 2026-03-24 | [Remove `do` Block](done/remove-do-block.md) | Complete removal of do block from the language |
| 2026-03-24 | [Effect fn Result Wrapping](done/effect-fn-result-wrapping.md) | Fix effect fn Rust codegen to wrap return type in Result |
| 2026-03-24 | [Direct WASM Emission](done/emit-wasm-direct.md) | Direct WASM binary emission with linear memory and WASI imports |
| 2026-03-23 | [WASM Validation Fixes](done/wasm-validation-fixes.md) | Fix WASM validation errors from union-find generic instantiation |
| 2026-03-23 | [WASM Tail Call Optimization](done/wasm-tco.md) | Tail call optimization for WASM target to prevent stack overflow |
| 2026-03-23 | [WASM Runtime Traps](done/wasm-runtime-traps.md) | Fix 44 WASM runtime traps (protocols, maps, closures, strings) |
| 2026-03-23 | [WASM Remaining 3 Failures — Root Cause Analysis & Fix Plan](done/wasm-remaining-3.md) | Root cause analysis of last 3 WASM test failures |
| 2026-03-23 | [WASM Local Allocation Redesign](done/wasm-local-allocation.md) | Redesign WASM function local allocation and scratch layout |
| 2026-03-23 | [WASM Compile Error Elimination Roadmap](done/wasm-compile-errors.md) | WASM compile error elimination (type mismatches, lambda issues) |
| 2026-03-23 | [IR Verification & Self-Describing IR](done/ir-verification.md) | Debug-time IR integrity checks and self-describing IR nodes |
| 2026-03-20 | [Stdlib Scope Reduction — Complete](done/stdlib-scope-reduction.md) | Move uuid, crypto, toml, compress, term out of stdlib to packages |
| 2026-03-20 | [Stdlib Additions — Complete](done/stdlib-additions.md) | Stdlib module additions (set expanded to 20 functions) |
| 2026-03-20 | [HKT Foundation — Complete](done/hkt-foundation.md) | Higher-kinded type foundation - all phases complete |
| 2026-03-20 | [Compiler Bugs and Gaps — Status](done/compiler-bugs.md) | Codegen bugs and runtime gaps found while writing 400+ test blocks |
| 2026-03-19 | [Test Architecture Redesign](done/test-architecture-redesign.md) | Separate effect permission from Result auto-unwrap in test infra |
| 2026-03-19 | [Streaming — WebSocket, SSE, Stream](done/streaming.md) | WebSocket, SSE, and streaming data support |
| 2026-03-19 | [HKT Foundation — Phase 1-4 + Stream Fusion (All 6 Laws)](done/hkt-foundation-phase1.md) | HKT foundation phases 1-4 with type constructors and algebraic laws |
| 2026-03-19 | [fan.map Concurrency Limit](done/fan-map-limit.md) | Add concurrency limit parameter to fan.map |
| 2026-03-19 | [Effect System — Phase 1-2](done/effect-system-phase1-2.md) | Effect inference engine with 7 categories and checker integration |
| 2026-03-18 | [UFCS for External Libraries](done/ufcs-external.md) | Extend UFCS resolution to external library functions |
| 2026-03-18 | [TS Edge-Native Deployment](done/ts-edge-native.md) | Native TS output for edge runtimes (Workers, Deno Deploy, Vercel) |
| 2026-03-18 | [Tooling [ON HOLD — items split to active]](done/tooling.md) | Tooling roadmap (LSP, REPL, doc gen, bench) split to active |
| 2026-03-18 | [Stdlib Strategy](done/stdlib-strategy.md) | Stdlib expansion strategy via Rust ecosystem wrapping |
| 2026-03-18 | [Stdlib Architecture: 3-Layer Design](done/stdlib-architecture-3-layer-design.md) | Three-layer stdlib design (core/platform/external) for WASM parity |
| 2026-03-18 | [Stdlib API Surface Reform](done/stdlib-verb-system.md) | Unified verb system across all stdlib container types |
| 2026-03-18 | [Showcase 5: dotenv Loader (Script)](done/showcase-5-script-dotenv.md) | Showcase: dotenv file loader and missing-key checker |
| 2026-03-18 | [Showcase 4: Markdown to HTML (DevTool)](done/showcase-4-devtool-md2html.md) | Showcase: Markdown-to-HTML converter using variant types and match |
| 2026-03-18 | [Showcase 3: CSV to JSON Pipeline (Data Processing)](done/showcase-3-data-pipeline.md) | Showcase: CSV-to-JSON data pipeline with list HOFs and pipes |
| 2026-03-18 | [Showcase 2: Todo API (HTTP API)](done/showcase-2-http-api.md) | Showcase: REST API server with http.serve, json, and codec |
| 2026-03-18 | [Showcase 1: almide-grep (CLI Tool)](done/showcase-1-cli-grep.md) | Showcase: CLI grep tool using fan concurrency and regex |
| 2026-03-18 | [Server Async — http.serve Effect Integration](done/server-async.md) | Make http.serve handler an effect context for I/O calls |
| 2026-03-18 | [Result Builder](done/template.md) | Swift-style result builder DSL for structured data construction |
| 2026-03-18 | [Quick-Win Polish Items](done/polish-immediate.md) | Quick-win polish items (ICE warnings, integration tests, TS E2E) |
| 2026-03-18 | [Production Readiness Requirements](done/production-ready.md) | Production readiness checklist (11 stdlib runtimes, CI, packaging) |
| 2026-03-18 | [Platform / Target Separation](done/platform-target-separation.md) | Separate --target (output language) from --platform (runtime env) |
| 2026-03-18 | [New Codegen Targets](done/new-codegen-targets.md) | Candidate new codegen targets (Go, Python, C, Swift, Kotlin) |
| 2026-03-18 | [Multi-Target Strategy](done/multi-target-strategy.md) | Strategy for adding new codegen targets with minimal cost |
| 2026-03-18 | [Lower Two-Pass Separation](done/lower-two-pass.md) | Separate AST-to-IR lowering from use-count analysis into two passes |
| 2026-03-18 | [LLM Immutable Sugar](done/llm-immutable-sugar.md) | Language-level sugar for immutable collection mutations |
| 2026-03-18 | [Grammar Codegen: Single Source of Truth [P1]](done/grammar-codegen.md) | Unify grammar definitions into a single source of truth |
| 2026-03-18 | [Editor & GitHub Integration](done/editor-github-integration.md) | TextMate grammar, VS Code extension, and Chrome syntax highlighting |
| 2026-03-18 | [Design Debt](done/design-debt.md) | Design debt including partial TOML dispatch and anonymous records |
| 2026-03-18 | [Cross-Target Semantics](done/cross-target-semantics.md) | Fix cases where same .almd produces different results on Rust vs TS |
| 2026-03-18 | [Cross-Target CI](done/cross-target-ci.md) | Run all tests on both Rust and TS targets, verify output match |
| 2026-03-18 | [Cross-Target AOT Compilation [PLANNED]](done/cross-target-aot.md) | AOT cross-compilation producing multiple target artifacts in one build |
| 2026-03-18 | [Concatenation Operator Reform](done/concat-operator-reform.md) | Reform ++ concatenation operator for strings and lists |
| 2026-03-18 | [Compiler Architecture Cleanup](done/compiler-architecture-cleanup.md) | Compiler structural cleanup including clone/deref IR conversion |
| 2026-03-18 | [Codegen v3: Transform Classification](done/codegen-v3-transform-classification.md) | Complete classification of codegen transforms by context depth |
| 2026-03-18 | [Codegen v3: Three-Layer Architecture](done/codegen-v3-architecture.md) | Three-layer codegen architecture for multi-target extensibility |
| 2026-03-18 | [Checker InferTy/Ty Unification](done/checker-type-unification.md) | Unify InferTy and Ty representations in the type checker |
| 2026-03-18 | [Built-in Protocols](done/trait-impl.md) | Built-in protocols (Eq, Hash, Repr, From) with automatic derivation |
| 2026-03-18 | [Anonymous Record Codegen Fix](done/anon-record-codegen.md) | Fix Rust codegen emitting invalid type for anonymous records |
| 2026-03-18 | [Almide Runtime](done/almide-runtime.md) | Runtime design targeting best-in-class compiler performance |
| 2026-03-17 | [Stdlib Runtime Architecture Reform](done/stdlib-self-hosted-redesign.md) | Self-hosted stdlib with .almd-first design and @extern for host deps |
| 2026-03-17 | [Stability Contract [DONE — 1.0 Phase II]](done/stability-contract.md) | Backward compatibility policy, edition system, and API freeze |
| 2026-03-17 | [Runtime Layout Unification](done/runtime-layout.md) | Unify Rust and TS runtime file layout and management |
| 2026-03-17 | [Runtime Gaps — Complete](done/runtime-gaps.md) | All 22 stdlib modules / 355 functions runtime implementation complete |
| 2026-03-17 | [Quality Improvements](done/quality-improvements.md) | Quality improvements (error line numbers, heredoc tracking) |
| 2026-03-17 | [Fan Concurrency](done/fan-concurrency.md) | Unified async/concurrency design using effect fn and fan syntax |
| 2026-03-17 | [Exercise Suite v0.6.0](done/exercises-v060.md) | Exercise suite with 20 exercises and 230 tests for LLM benchmarking |
| 2026-03-17 | [Error Codes + JSON Output [DONE — 1.0 Phase II]](done/error-codes-json.md) | Structured error codes (E001-E010) and JSON diagnostic output |
| 2026-03-17 | [Effect Isolation (Security Layer 1)](done/effect-isolation.md) | Static verification that pure functions cannot perform I/O |
| 2026-03-17 | [CLI-First](done/cli-first.md) | Enable comfortable CLI tool authoring with run, build, and WASM targets |
| 2026-03-17 | [Borrow/Clone Gaps](done/borrow-clone-gaps.md) | Fix cases where Rust codegen fails to insert necessary clones |
| 2026-03-17 | [almide.lock [DONE — 1.0 Phase III]](done/lockfile.md) | Dependency lockfile with git-based resolution and reproducible builds |
| 2026-03-17 | [2026 Ergonomics Roadmap](done/2026-ergonomics.md) | Ergonomics issues found via self-tooling, evaluated against design principles |
| 2026-03-16 | [Type System Theory Upgrade — HM Integration Plan](done/type-system-theory-upgrade.md) | Hindley-Milner integration plan (type schemes, let-polymorphism) |
| 2026-03-16 | [TS Target: Result Maintenance (Erasure to Object)](done/ts-result-maintenance.md) | Replace TS Result erasure (throw/catch) with Result objects |
| 2026-03-16 | [Recursive Type Box Insertion](done/recursive-type-box.md) | Auto-insert Box for recursive type members in Rust codegen |
| 2026-03-16 | [Open Record / Row Polymorphism — Implementation Guide](done/open-record-structural-typing.md) | Open record / row polymorphism implementation for structural typing |
| 2026-03-16 | [Let-Polymorphism (Algorithm W)](done/let-polymorphism.md) |  |
| 2026-03-16 | [Higher-Order Function Type Inference](done/higher-order-fn-inference.md) | Type inference for higher-order functions returning closures |
| 2026-03-16 | [Guard `ok(value)` Value Loss in Effect Do-Block](done/guard-ok-value-loss.md) | Fix ok(value) being lost in guard expressions within effect do-blocks |
| 2026-03-15 | [Unused Variable Warnings](done/unused-variable-warnings.md) | Warn on unused variables and imports, suppressible with _ prefix |
| 2026-03-15 | [Type System Soundness](done/type-system-soundness.md) | Type system soundness fixes (Unknown propagation, unification, occurs) |
| 2026-03-15 | [Type System Extensions](done/type-system.md) | Type system extensions (generics migration, inference improvements) |
| 2026-03-15 | [TS/JS Codegen Rewrite](done/ts-codegen-rewrite.md) | Rewrite TS codegen to two-stage pipeline (IR to TsIR to String) |
| 2026-03-15 | [Tail Call Optimization](done/tail-call-optimization.md) | Self-recursive tail call to labeled loop transformation |
| 2026-03-15 | [Syntax Sugar](done/syntax-sugar.md) | Syntax sugar (ranges, exhaustiveness check, lambda shorthand) |
| 2026-03-15 | [RustIR: Rust Codegen Intermediate Representation](done/rust-ir.md) | Two-stage Rust codegen pipeline via RustIR intermediate repr |
| 2026-03-15 | [Pattern Exhaustiveness Check](done/pattern-exhaustiveness-check.md) | Static exhaustiveness checking for match expressions |
| 2026-03-15 | [Parser Error Recovery](done/parser-error-recovery.md) | Continue parsing after syntax errors to report multiple diagnostics |
| 2026-03-15 | [Operator Protocol](done/operator-protocol.md) | Convention-based operator dispatch (==, repr, sort, hash) |
| 2026-03-15 | [Monomorphization](done/monomorphization.md) | Function monomorphization for generic structural bounds in Rust codegen |
| 2026-03-15 | [IR Optimization Passes](done/ir-optimization.md) | Constant folding, dead code elimination, and basic inlining passes |
| 2026-03-15 | [IR Optimization Passes](done/ir-optimization-passes.md) | IR-to-IR transform passes applied before codegen for all targets |
| 2026-03-15 | [Grammar Research Infrastructure](done/grammar-research.md) | A/B testing infrastructure for syntax design using LLM benchmarks |
| 2026-03-15 | [Generic Variant Type Instantiation](done/generic-variant-instantiation.md) | Fix type instantiation for generic variant constructors like Nothing |
| 2026-03-15 | [Formatter Rewrite](done/formatter-rewrite.md) | Ground-up rewrite of the 890-line source formatter |
| 2026-03-15 | [Exhaustiveness Check — Hard Error](done/exhaustiveness-check.md) | Promote pattern match exhaustiveness from warning to hard error |
| 2026-03-15 | [Derive Conventions](done/derive-conventions.md) | Fixed conventions with colon syntax for polymorphism without traits |
| 2026-03-15 | [Compiler Warnings](done/compiler-warnings.md) | Warning infrastructure for code quality issues like unused variables |
| 2026-03-15 | [Codegen Refinement](done/codegen-refinement.md) | Small independent optimizations improving generated Rust code quality |
| 2026-03-15 | [Codegen Correctness Fixes](done/codegen-correctness.md) | Fix correctness issues in generated code (auto-unwrap, range, guard) |
| 2026-03-15 | [Codec Test Specification](done/codec-test-spec.md) | Test case specification for codec based on Serde/Codable/Jackson patterns |
| 2026-03-15 | [Codec Remaining](done/codec-remaining.md) | Remaining codec features after Phase 0-2 completion |
| 2026-03-15 | [Codec Protocol & JSON](done/codec-and-json.md) | Format-agnostic codec protocol with JSON as first implementation |
| 2026-03-15 | [Codec Implementation Plan](done/codec-implementation.md) | Three-layer codec implementation: compiler, format library, user code |
| 2026-03-15 | [Codec Advanced](done/codec-advanced.md) | Advanced codec features: structured errors, validation, schema |
| 2026-03-15 | [Clone Reduction Phase 4](done/clone-reduction.md) | Phase 4 clone reduction targeting field-level borrow analysis |
| 2026-03-15 | [Architecture Hardening](done/architecture-hardening.md) | Fix structural weaknesses in compiler architecture |
| 2026-03-15 | [--emit-ir: IR JSON Export](done/emit-ir.md) | Export typed IR as JSON via --emit-ir flag |
| 2026-03-14 | [UFCS Type Resolution for Ambiguous Methods](done/ufcs-type-resolution.md) | Fix UFCS resolution for ambiguous methods on complex expressions |
| 2026-03-14 | [Trailing Lambda / Builder DSL [WON'T DO]](done/trailing-lambda-builder.md) | Trailing lambda / builder DSL exploration (rejected) |
| 2026-03-14 | [Structured Concurrency](done/structured-concurrency.md) | Async model with async fn, await, and async let constructs |
| 2026-03-14 | [LLM Developer Experience [DONE / MERGED]](done/llm-developer-experience.md) | UFCS and almide init improvements for LLM-assisted development |
| 2026-03-14 | [JSON Builder API [SUPERSEDED]](done/json-builder-api.md) | Superseded JSON builder API, replaced by Codec Protocol |
| 2026-03-14 | [Function Reference Passing [WON'T DO]](done/function-reference-passing.md) | Rejected: direct function reference passing deemed not worth complexity |
| 2026-03-14 | [Codegen Optimization [IN PROGRESS]](done/codegen-optimization.md) | Reduce clone overhead for heap types without exposing ownership |
| 2026-03-14 | [Codegen IR Redesign](done/ir-redesign.md) | Self-contained typed IR so codegen never references AST |
| 2026-03-14 | [Bidirectional Type Inference for Lambda Parameters](done/lambda-type-inference.md) | Bidirectional type inference for lambda parameters via two-pass checker |
| 2026-03-14 | [almide scaffold & Module Proliferation Pipeline [MERGED]](done/scaffold-and-proliferation.md) | Scaffold command and LLM module proliferation pipeline |
| 2026-03-13 | [Module System v2](done/module-system-v2.md) | File-based module system with visibility controls and mod.almd |
| 2026-03-13 | [Map Literal Syntax](done/map-literal.md) | Map literal syntax with Swift-style [:] empty map notation |
| 2026-03-13 | [Hint System Architecture [P0]](done/hint-system.md) | Decouple hint generation from parser into a dedicated system |
| 2026-03-13 | [Error Recovery](done/error-recovery.md) | Report all errors at once instead of stopping at the first one |
| 2026-03-13 | [Eq Protocol](done/eq-protocol.md) | Automatic deep equality for all value types without deriving |
| 2026-03-13 | [`import self` — Package Entry Point Access](done/import-self-entry.md) | Allow main.almd to access pub functions from same-package mod.almd |
| 2026-03-12 | [While Loop](done/while-loop.md) | Dedicated while loop syntax replacing do-block guard pattern |
| 2026-03-12 | [Variant Record Fields](done/variant-record-fields.md) | Named fields for enum variants (like Rust struct variants) |
| 2026-03-12 | [Typed IR](done/typed-ir.md) | Typed intermediate representation between checker and emitters |
| 2026-03-12 | [Top-Level Let](done/top-level-let.md) | Allow let bindings at module scope for constant values |
| 2026-03-12 | [Test Directory Structure Redesign](done/test-directory-structure.md) | Reorganize tests into spec/ (lang/stdlib/integration) and tests/ |
| 2026-03-12 | [Self-Tooling: Editor Tools Written in Almide](done/self-tooling.md) | Editor tools (tree-sitter grammar, TextMate) written in Almide |
| 2026-03-12 | [Rust Compiler Test Coverage](done/rust-test-coverage.md) | Rust-side unit/integration test coverage targets (600+ cases) |
| 2026-03-12 | [List Index Read (`xs[i]`)](done/list-index-read.md) | Add index-based read syntax for lists (xs[i]) |
| 2026-03-12 | [Language Test Coverage (`almide test`)](done/test-coverage.md) | Almide language-level test coverage targets (1500+ cases) |
| 2026-03-12 | [Default Field Values](done/default-field-values.md) | Default values for record fields to eliminate sentinel value patterns |
| 2026-03-12 | [Compiler Bugs Found by Test Expansion](done/compiler-bugs-from-tests.md) | Seven compiler bugs discovered during test coverage expansion |
| 2026-03-11 | [Tuple & Record](done/tuple-record.md) | Named record construction and tuple index access |
| 2026-03-11 | [String Handling](done/string-handling.md) | Heredoc multi-line strings and raw string literals |
| 2026-03-11 | [Stdlib Self-Hosting](done/stdlib-self-hosting.md) | Write stdlib in Almide for automatic multi-target support |
| 2026-03-11 | [Stdlib Gaps](done/stdlib-gaps.md) | Reduce AI-generated boilerplate via new stdlib functions |
| 2026-03-11 | [Stdlib Completeness](done/stdlib-completeness.md) | Fill stdlib gaps in int, string, list, and map modules |
| 2026-03-11 | [stdin / Interactive I/O](done/stdin-io.md) | stdin reading and interactive I/O via io module |
| 2026-03-11 | [Proliferation Blockers](done/proliferation-blockers.md) | Compiler bugs that blocked LLM module generation (all resolved) |
| 2026-03-11 | [Playground Repair Turn](done/playground-repair.md) | Playground AI-powered error repair and type checker integration |
| 2026-03-11 | [npm Package Target](done/npm-package-target-target-npm.md) | Compile Almide to publish-ready npm packages via --target npm |
| 2026-03-11 | [LLM and Immutable Data Structures](done/llm-immutable-patterns.md) | Mitigations for LLM failures with immutable data patterns |
| 2026-03-11 | [Literal Syntax Gaps](done/literal-syntax-gaps.md) | Cross-language comparison of numeric and collection literal syntax |
| 2026-03-11 | [List Stdlib Gaps](done/list-stdlib-gaps.md) | Cross-language comparison of missing list module operations |
| 2026-03-11 | [Language Test Suite](done/language-test-suite.md) | Systematic language feature test suite for regression detection |
| 2026-03-11 | [HTTP Module](done/http.md) | HTTP server and client module with multithreading support |
| 2026-03-11 | [Generics](done/generics.md) | Generic functions, records, variants, and recursive generics |
| 2026-03-11 | [Error Diagnostics](done/error-diagnostics.md) | LLM-critical diagnostics including lost mutation and type mismatch |
| 2026-03-11 | [Error Diagnostics — Visual Improvements](done/error-diagnostics-visual.md) | Visual diagnostic improvements: ANSI colors, multi-line spans |
| 2026-03-11 | [Declarative Stdlib Codegen](done/stdlib-codegen.md) | TOML-driven declarative stdlib codegen replacing manual match arms |
| 2026-03-11 | [Cross-Platform Support](done/cross-platform.md) | Write-once cross-platform support with transparent OS differences |
| 2026-03-11 | [Control Flow Extensions [CLOSED]](done/control-flow.md) | Rejected: while/break/continue/return contradict design philosophy |
| 2026-03-11 | [Compiler Hardening](done/compiler-hardening.md) | Eliminate all panics and unhandled edge cases in the compiler |
| 2026-03-11 | [CLI Tool Authoring Issues](done/cli-tool-authoring.md) | Fix issues discovered while implementing CLI tool benchmarks |
| 2026-03-11 | [Borrow Inference — Detailed Design [COMPLETE]](done/borrow-inference-design.md) | Detailed design for borrow inference to reduce unnecessary clones |

</details>

