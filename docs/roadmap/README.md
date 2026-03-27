# Almide Roadmap

> Auto-generated from directory structure. Run `bash docs/roadmap/generate-readme.sh > docs/roadmap/README.md` to update.
>
> [GRAND_PLAN.md](GRAND_PLAN.md) — 5-phase strategy

## Active

13 items

| Item | Description |
|------|-------------|
| [LLM Benchmark Execution](active/benchmark-execution.md) | LLM accuracy benchmarks comparing Almide, Python, and MoonBit |
| [Compiler Architecture: All 10s](active/compiler-architecture-10.md) | Achieve 10/10 on every compiler architecture quality metric |
| [Cross-Target Parity Matrix](active/cross-target-parity-matrix.md) | Automated verification that Rust, TS, and WASM produce identical output |
| [Diagnostic end_col — Precise Error Underlines](active/diagnostic-end-col.md) | Track end column in diagnostics for precise error underlines |
| [Effect System — Auto-Inferred Capabilities](active/effect-system.md) | Auto-inferred effect capabilities with package-level permissions |
| [Emit Readability](active/emit-readability.md) | Improve readability of generated Rust and TypeScript output |
| [GPU Compute — Matrix Type and Compiler-Driven GPU Execution](active/gpu-compute.md) | Matrix primitive type with compiler-driven CPU/GPU execution |
| [HTTPS Native Support](active/https-native.md) | Native HTTPS support via rustls across all targets |
| [Purity Exploitation — Leveraging fn/effect fn Distinction](active/purity-exploitation.md) | Exploit fn/effect fn purity for auto-parallelism and escape analysis |
| [Stdlib in Almide: Unified Library Architecture](active/stdlib-in-almide.md) | Rewrite stdlib in Almide with a 3-layer architecture |
| [Unwrap Operators: `!` `??` `?`](active/unwrap-operators.md) | Postfix !, ??, and ? operators for explicit Result/Option unwrapping |
| [WASM HTTP Client](active/wasm-http-client.md) | HTTP client support for the WASM target via WASI or host imports |
| [WASM Remaining FS Operations](active/wasm-remaining-fs.md) | Implement remaining filesystem operations for the WASM target |

## On Hold

28 items

| Item | Description |
|------|-------------|
| [Almide Shell](on-hold/almide-shell.md) | Interactive shell replacing Bash/Zsh with type-safe LLM-friendly syntax |
| [Almide UI — Reactive Web Framework as Almide Library](on-hold/almide-ui.md) | SolidJS-like reactive UI framework built as a pure Almide library |
| [Async Backend — tokio opt-in](on-hold/async-backend.md) | Optional tokio-based async backend for high-concurrency workloads |
| [build.rs Runtime Scanner Hardening](on-hold/buildrs-syn-scanner.md) | Replace regex-based runtime scanner with syn crate for robust parsing |
| [Compile-Time Contracts](on-hold/compile-time-contracts.md) | Compile-time preconditions and type invariants via where clauses |
| [Effect Type Integration — Embed EffectSet in FnType](on-hold/effect-type-integration.md) | Embed EffectSet into FnType for type-level effect tracking |
| [Error-Fix Database](on-hold/error-fix-db.md) | Structured error-to-fix mapping for LLM auto-repair of compiler errors |
| [Go Target](on-hold/go-target.md) | Go codegen target via TOML templates and Go-specific nanopass passes |
| [Incremental Compilation](on-hold/incremental-compilation.md) | Skip redundant rustc invocations by hashing generated Rust source |
| [IR Interpreter](on-hold/ir-interpreter.md) | Direct IR execution for instant REPL, playground, and fast test runs |
| [IR Optimization Tier 2](on-hold/ir-optimization-tier2.md) | CSE and inlining passes for cross-target IR optimization |
| [LLM Integration](on-hold/llm-integration.md) | Built-in LLM commands for library generation, auto-fix, and code explanation |
| [LLM → IR Direct Generation](on-hold/llm-ir-generation.md) | LLM generates typed IR as JSON directly, bypassing parser errors |
| [LSP Server](on-hold/lsp.md) | Language Server Protocol for editor completion, diagnostics, and navigation |
| [Package Registry](on-hold/package-registry.md) | Lock file, semver resolution, and central package registry |
| [Performance Research: Path to World #1](on-hold/performance-research.md) | Research plan to surpass hand-written Rust via semantic-aware optimization |
| [Almide Platform Architecture Vision](on-hold/platform-architecture.md) | Multi-layer platform vision with pluggable renderer and host bindings |
| [Rainbow Bridge — Wrap External Code as Almide Packages](on-hold/rainbow-bridge.md) | Wrap external Rust/TS/Python code as native Almide packages via @extern |
| [Rainbow FFI Gate](on-hold/rainbow-gate.md) | Export Almide code as native-speed libraries callable from any language |
| [REPL](on-hold/repl.md) | Interactive read-eval-print loop with persistent state across inputs |
| [Research: Modification Survival Rate Paper](on-hold/research-modification-survival-rate-paper.md) | Academic paper measuring LLM code modification survival across languages |
| [The Rumbling — Almide OSS Rewrite Campaign](on-hold/rumbling.md) | Campaign to rewrite OSS tools in Almide to prove WASM size and LLM accuracy |
| [Secure by Design](on-hold/secure-by-design.md) | Five-layer security model making web vulnerabilities compile-time errors |
| [Security Model — Layer 3–5](on-hold/security-model.md) | Package boundary, runtime sandbox, and supply chain integrity layers |
| [Self-Contained Compiler: Remove rustc Dependency](on-hold/self-contained-compiler.md) | Emit LLVM IR directly to eliminate rustc dependency for end users |
| [Self-Hosting: Autonomous Bootstrap Compiler](on-hold/self-hosting.md) | Rewrite the compiler in Almide for a self-contained 350KB WASM toolchain |
| [Supervision & Actors](on-hold/supervision-and-actors.md) | Erlang-style actors, supervisors, and typed channels as stdlib modules |
| [Web Framework](on-hold/web-framework.md) | First-party Hono-like web framework with template and Codec integration |

## Done

158 items

<details>
<summary>Show all 158 completed items</summary>

| Item | Description |
|------|-------------|
| [2026 Ergonomics Roadmap](done/2026-ergonomics.md) | Ergonomics issues found via self-tooling, evaluated against design principles |
| [Almide Runtime](done/almide-runtime.md) | Runtime design targeting best-in-class compiler performance |
| [Anonymous Record Codegen Fix](done/anon-record-codegen.md) | Fix Rust codegen emitting invalid type for anonymous records |
| [Architecture Hardening](done/architecture-hardening.md) | Fix structural weaknesses in compiler architecture |
| [Benchmark Report: LLM Code Generation Cost by Language](done/benchmark-report.md) | Benchmark comparing LLM code generation cost across 16 languages |
| [Borrow/Clone Gaps](done/borrow-clone-gaps.md) | Fix cases where Rust codegen fails to insert necessary clones |
| [Borrow Inference — Detailed Design [COMPLETE]](done/borrow-inference-design.md) | Detailed design for borrow inference to reduce unnecessary clones |
| [Checker InferTy/Ty Unification](done/checker-type-unification.md) | Unify InferTy and Ty representations in the type checker |
| [CLI-First](done/cli-first.md) | Enable comfortable CLI tool authoring with run, build, and WASM targets |
| [CLI Tool Authoring Issues](done/cli-tool-authoring.md) | Fix issues discovered while implementing CLI tool benchmarks |
| [Clone Reduction Phase 4](done/clone-reduction.md) | Phase 4 clone reduction targeting field-level borrow analysis |
| [Codec Advanced](done/codec-advanced.md) | Advanced codec features: structured errors, validation, schema |
| [Codec Protocol & JSON](done/codec-and-json.md) | Format-agnostic codec protocol with JSON as first implementation |
| [Codec Implementation Plan](done/codec-implementation.md) | Three-layer codec implementation: compiler, format library, user code |
| [Codec Remaining](done/codec-remaining.md) | Remaining codec features after Phase 0-2 completion |
| [Codec Test Specification](done/codec-test-spec.md) | Test case specification for codec based on Serde/Codable/Jackson patterns |
| [Codegen Correctness Fixes](done/codegen-correctness.md) | Fix correctness issues in generated code (auto-unwrap, range, guard) |
| [Codegen Optimization [IN PROGRESS]](done/codegen-optimization.md) | Reduce clone overhead for heap types without exposing ownership |
| [Codegen Refinement](done/codegen-refinement.md) | Small independent optimizations improving generated Rust code quality |
| [Codegen v3: Three-Layer Architecture](done/codegen-v3-architecture.md) | Three-layer codegen architecture for multi-target extensibility |
| [Codegen v3: Transform Classification](done/codegen-v3-transform-classification.md) | Complete classification of codegen transforms by context depth |
| [Compiler Architecture Cleanup](done/compiler-architecture-cleanup.md) | Compiler structural cleanup including clone/deref IR conversion |
| [Compiler Bugs Found by Test Expansion](done/compiler-bugs-from-tests.md) | Seven compiler bugs discovered during test coverage expansion |
| [Compiler Bugs and Gaps — Status](done/compiler-bugs.md) | Codegen bugs and runtime gaps found while writing 400+ test blocks |
| [Compiler Hardening](done/compiler-hardening.md) | Eliminate all panics and unhandled edge cases in the compiler |
| [Compiler Warnings](done/compiler-warnings.md) | Warning infrastructure for code quality issues like unused variables |
| [Concatenation Operator Reform](done/concat-operator-reform.md) | Reform ++ concatenation operator for strings and lists |
| [Control Flow Extensions [CLOSED]](done/control-flow.md) | Rejected: while/break/continue/return contradict design philosophy |
| [Cross-Platform Support](done/cross-platform.md) | Write-once cross-platform support with transparent OS differences |
| [Cross-Target AOT Compilation [PLANNED]](done/cross-target-aot.md) | AOT cross-compilation producing multiple target artifacts in one build |
| [Cross-Target CI](done/cross-target-ci.md) | Run all tests on both Rust and TS targets, verify output match |
| [Cross-Target Semantics](done/cross-target-semantics.md) | Fix cases where same .almd produces different results on Rust vs TS |
| [Default Field Values](done/default-field-values.md) | Default values for record fields to eliminate sentinel value patterns |
| [Derive Conventions](done/derive-conventions.md) | Fixed conventions with colon syntax for polymorphism without traits |
| [Design Debt](done/design-debt.md) | Design debt including partial TOML dispatch and anonymous records |
| [Diagnostic Secondary Spans](done/diagnostic-secondary-spans.md) | Activate secondary spans showing declaration sites in error messages |
| [Editor & GitHub Integration](done/editor-github-integration.md) | TextMate grammar, VS Code extension, and Chrome syntax highlighting |
| [Effect fn Result Wrapping](done/effect-fn-result-wrapping.md) | Fix effect fn Rust codegen to wrap return type in Result |
| [Effect Isolation (Security Layer 1)](done/effect-isolation.md) | Static verification that pure functions cannot perform I/O |
| [Effect System — Phase 1-2](done/effect-system-phase1-2.md) | Effect inference engine with 7 categories and checker integration |
| [--emit-ir: IR JSON Export](done/emit-ir.md) | Export typed IR as JSON via --emit-ir flag |
| [Direct WASM Emission](done/emit-wasm-direct.md) | Direct WASM binary emission with linear memory and WASI imports |
| [Eq Protocol](done/eq-protocol.md) | Automatic deep equality for all value types without deriving |
| [Error Codes + JSON Output [DONE — 1.0 Phase II]](done/error-codes-json.md) | Structured error codes (E001-E010) and JSON diagnostic output |
| [Error Diagnostics — Visual Improvements](done/error-diagnostics-visual.md) | Visual diagnostic improvements: ANSI colors, multi-line spans |
| [Error Diagnostics](done/error-diagnostics.md) | LLM-critical diagnostics including lost mutation and type mismatch |
| [Error Recovery](done/error-recovery.md) | Report all errors at once instead of stopping at the first one |
| [Exercise Suite v0.6.0](done/exercises-v060.md) | Exercise suite with 20 exercises and 230 tests for LLM benchmarking |
| [Exhaustiveness Check — Hard Error](done/exhaustiveness-check.md) | Promote pattern match exhaustiveness from warning to hard error |
| [Fan Concurrency](done/fan-concurrency.md) | Unified async/concurrency design using effect fn and fan syntax |
| [fan.map Concurrency Limit](done/fan-map-limit.md) | Add concurrency limit parameter to fan.map |
| [Formatter Rewrite](done/formatter-rewrite.md) | Ground-up rewrite of the 890-line source formatter |
| [Function Reference Passing [WON'T DO]](done/function-reference-passing.md) | Rejected: direct function reference passing deemed not worth complexity |
| [Generic Variant Type Instantiation](done/generic-variant-instantiation.md) | Fix type instantiation for generic variant constructors like Nothing |
| [Generics](done/generics.md) | Generic functions, records, variants, and recursive generics |
| [Grammar Codegen: Single Source of Truth [P1]](done/grammar-codegen.md) | Unify grammar definitions into a single source of truth |
| [Grammar Research Infrastructure](done/grammar-research.md) | A/B testing infrastructure for syntax design using LLM benchmarks |
| [Guard `ok(value)` Value Loss in Effect Do-Block](done/guard-ok-value-loss.md) | Fix ok(value) being lost in guard expressions within effect do-blocks |
| [Higher-Order Function Type Inference](done/higher-order-fn-inference.md) | Type inference for higher-order functions returning closures |
| [Hint System Architecture [P0]](done/hint-system.md) | Decouple hint generation from parser into a dedicated system |
| [HKT Foundation — Phase 1-4 + Stream Fusion (All 6 Laws)](done/hkt-foundation-phase1.md) | HKT foundation phases 1-4 with type constructors and algebraic laws |
| [HKT Foundation — Complete](done/hkt-foundation.md) | Higher-kinded type foundation - all phases complete |
| [HTTP Module](done/http.md) | HTTP server and client module with multithreading support |
| [`import self` — Package Entry Point Access](done/import-self-entry.md) | Allow main.almd to access pub functions from same-package mod.almd |
| [IR Optimization Passes](done/ir-optimization-passes.md) | IR-to-IR transform passes applied before codegen for all targets |
| [IR Optimization Passes](done/ir-optimization.md) | Constant folding, dead code elimination, and basic inlining passes |
| [Codegen IR Redesign](done/ir-redesign.md) | Self-contained typed IR so codegen never references AST |
| [IR Verification & Self-Describing IR](done/ir-verification.md) | Debug-time IR integrity checks and self-describing IR nodes |
| [JSON Builder API [SUPERSEDED]](done/json-builder-api.md) | Superseded JSON builder API, replaced by Codec Protocol |
| [Bidirectional Type Inference for Lambda Parameters](done/lambda-type-inference.md) | Bidirectional type inference for lambda parameters via two-pass checker |
| [Language Test Suite](done/language-test-suite.md) | Systematic language feature test suite for regression detection |
| [Let-Polymorphism (Algorithm W)](done/let-polymorphism.md) |  |
| [List Index Read (`xs[i]`)](done/list-index-read.md) | Add index-based read syntax for lists (xs[i]) |
| [List Stdlib Gaps](done/list-stdlib-gaps.md) | Cross-language comparison of missing list module operations |
| [Literal Syntax Gaps](done/literal-syntax-gaps.md) | Cross-language comparison of numeric and collection literal syntax |
| [LLM Developer Experience [DONE / MERGED]](done/llm-developer-experience.md) | UFCS and almide init improvements for LLM-assisted development |
| [LLM and Immutable Data Structures](done/llm-immutable-patterns.md) | Mitigations for LLM failures with immutable data patterns |
| [LLM Immutable Sugar](done/llm-immutable-sugar.md) | Language-level sugar for immutable collection mutations |
| [almide.lock [DONE — 1.0 Phase III]](done/lockfile.md) | Dependency lockfile with git-based resolution and reproducible builds |
| [Lower Two-Pass Separation](done/lower-two-pass.md) | Separate AST-to-IR lowering from use-count analysis into two passes |
| [Map Literal Syntax](done/map-literal.md) | Map literal syntax with Swift-style [:] empty map notation |
| [Module System v2](done/module-system-v2.md) | File-based module system with visibility controls and mod.almd |
| [Monomorphization](done/monomorphization.md) | Function monomorphization for generic structural bounds in Rust codegen |
| [Multi-Target Strategy](done/multi-target-strategy.md) | Strategy for adding new codegen targets with minimal cost |
| [New Codegen Targets](done/new-codegen-targets.md) | Candidate new codegen targets (Go, Python, C, Swift, Kotlin) |
| [npm Package Target](done/npm-package-target-target-npm.md) | Compile Almide to publish-ready npm packages via --target npm |
| [Open Record / Row Polymorphism — Implementation Guide](done/open-record-structural-typing.md) | Open record / row polymorphism implementation for structural typing |
| [Operator Protocol](done/operator-protocol.md) | Convention-based operator dispatch (==, repr, sort, hash) |
| [Parser Error Recovery](done/parser-error-recovery.md) | Continue parsing after syntax errors to report multiple diagnostics |
| [Platform / Target Separation](done/platform-target-separation.md) | Separate --target (output language) from --platform (runtime env) |
| [Playground Repair Turn](done/playground-repair.md) | Playground AI-powered error repair and type checker integration |
| [Quick-Win Polish Items](done/polish-immediate.md) | Quick-win polish items (ICE warnings, integration tests, TS E2E) |
| [Production Readiness Requirements](done/production-ready.md) | Production readiness checklist (11 stdlib runtimes, CI, packaging) |
| [Proliferation Blockers](done/proliferation-blockers.md) | Compiler bugs that blocked LLM module generation (all resolved) |
| [Quality Improvements](done/quality-improvements.md) | Quality improvements (error line numbers, heredoc tracking) |
| [Recursive Type Box Insertion](done/recursive-type-box.md) | Auto-insert Box for recursive type members in Rust codegen |
| [Remove `do` Block](done/remove-do-block.md) | Complete removal of do block from the language |
| [Runtime Gaps — Complete](done/runtime-gaps.md) | All 22 stdlib modules / 355 functions runtime implementation complete |
| [Runtime Layout Unification](done/runtime-layout.md) | Unify Rust and TS runtime file layout and management |
| [RustIR: Rust Codegen Intermediate Representation](done/rust-ir.md) | Two-stage Rust codegen pipeline via RustIR intermediate repr |
| [Rust Compiler Test Coverage](done/rust-test-coverage.md) | Rust-side unit/integration test coverage targets (600+ cases) |
| [almide scaffold & Module Proliferation Pipeline [MERGED]](done/scaffold-and-proliferation.md) | Scaffold command and LLM module proliferation pipeline |
| [Self-Tooling: Editor Tools Written in Almide](done/self-tooling.md) | Editor tools (tree-sitter grammar, TextMate) written in Almide |
| [Server Async — http.serve Effect Integration](done/server-async.md) | Make http.serve handler an effect context for I/O calls |
| [Showcase 1: almide-grep (CLI Tool)](done/showcase-1-cli-grep.md) | Showcase: CLI grep tool using fan concurrency and regex |
| [Showcase 2: Todo API (HTTP API)](done/showcase-2-http-api.md) | Showcase: REST API server with http.serve, json, and codec |
| [Showcase 3: CSV to JSON Pipeline (Data Processing)](done/showcase-3-data-pipeline.md) | Showcase: CSV-to-JSON data pipeline with list HOFs and pipes |
| [Showcase 4: Markdown to HTML (DevTool)](done/showcase-4-devtool-md2html.md) | Showcase: Markdown-to-HTML converter using variant types and match |
| [Showcase 5: dotenv Loader (Script)](done/showcase-5-script-dotenv.md) | Showcase: dotenv file loader and missing-key checker |
| [Stability Contract [DONE — 1.0 Phase II]](done/stability-contract.md) | Backward compatibility policy, edition system, and API freeze |
| [stdin / Interactive I/O](done/stdin-io.md) | stdin reading and interactive I/O via io module |
| [Stdlib Additions — Complete](done/stdlib-additions.md) | Stdlib module additions (set expanded to 20 functions) |
| [Stdlib Architecture: 3-Layer Design](done/stdlib-architecture-3-layer-design.md) | Three-layer stdlib design (core/platform/external) for WASM parity |
| [Declarative Stdlib Codegen](done/stdlib-codegen.md) | TOML-driven declarative stdlib codegen replacing manual match arms |
| [Stdlib Completeness](done/stdlib-completeness.md) | Fill stdlib gaps in int, string, list, and map modules |
| [Stdlib Gaps](done/stdlib-gaps.md) | Reduce AI-generated boilerplate via new stdlib functions |
| [Stdlib Import Control](done/stdlib-import-control.md) | Three-tier import visibility for stdlib modules |
| [Stdlib Scope Reduction — Complete](done/stdlib-scope-reduction.md) | Move uuid, crypto, toml, compress, term out of stdlib to packages |
| [Stdlib Runtime Architecture Reform](done/stdlib-self-hosted-redesign.md) | Self-hosted stdlib with .almd-first design and @extern for host deps |
| [Stdlib Self-Hosting](done/stdlib-self-hosting.md) | Write stdlib in Almide for automatic multi-target support |
| [Stdlib Strategy](done/stdlib-strategy.md) | Stdlib expansion strategy via Rust ecosystem wrapping |
| [Stdlib API Surface Reform](done/stdlib-verb-system.md) | Unified verb system across all stdlib container types |
| [Streaming — WebSocket, SSE, Stream](done/streaming.md) | WebSocket, SSE, and streaming data support |
| [String Handling](done/string-handling.md) | Heredoc multi-line strings and raw string literals |
| [Structured Concurrency](done/structured-concurrency.md) | Async model with async fn, await, and async let constructs |
| [Syntax Sugar](done/syntax-sugar.md) | Syntax sugar (ranges, exhaustiveness check, lambda shorthand) |
| [Tail Call Optimization](done/tail-call-optimization.md) | Self-recursive tail call to labeled loop transformation |
| [Result Builder](done/template.md) | Swift-style result builder DSL for structured data construction |
| [Test Architecture Redesign](done/test-architecture-redesign.md) | Separate effect permission from Result auto-unwrap in test infra |
| [Test Coverage](done/test-coverage-v2.md) | Cross-target test coverage status (Rust/WASM/TS) |
| [Language Test Coverage (`almide test`)](done/test-coverage.md) | Almide language-level test coverage targets (1500+ cases) |
| [Test Directory Structure Redesign](done/test-directory-structure.md) | Reorganize tests into spec/ (lang/stdlib/integration) and tests/ |
| [Tooling [ON HOLD — items split to active]](done/tooling.md) | Tooling roadmap (LSP, REPL, doc gen, bench) split to active |
| [Top-Level Let](done/top-level-let.md) | Allow let bindings at module scope for constant values |
| [Trailing Lambda / Builder DSL [WON'T DO]](done/trailing-lambda-builder.md) | Trailing lambda / builder DSL exploration (rejected) |
| [Built-in Protocols](done/trait-impl.md) | Built-in protocols (Eq, Hash, Repr, From) with automatic derivation |
| [TS/JS Codegen Rewrite](done/ts-codegen-rewrite.md) | Rewrite TS codegen to two-stage pipeline (IR to TsIR to String) |
| [TS Edge-Native Deployment](done/ts-edge-native.md) | Native TS output for edge runtimes (Workers, Deno Deploy, Vercel) |
| [TS Target: Result Maintenance (Erasure to Object)](done/ts-result-maintenance.md) | Replace TS Result erasure (throw/catch) with Result objects |
| [TypeScript Test Runner](done/ts-test-runner.md) | almide test --target ts command with Deno/Node support |
| [Tuple & Record](done/tuple-record.md) | Named record construction and tuple index access |
| [Type System Soundness](done/type-system-soundness.md) | Type system soundness fixes (Unknown propagation, unification, occurs) |
| [Type System Theory Upgrade — HM Integration Plan](done/type-system-theory-upgrade.md) | Hindley-Milner integration plan (type schemes, let-polymorphism) |
| [Type System Extensions](done/type-system.md) | Type system extensions (generics migration, inference improvements) |
| [Typed IR](done/typed-ir.md) | Typed intermediate representation between checker and emitters |
| [UFCS for External Libraries](done/ufcs-external.md) | Extend UFCS resolution to external library functions |
| [UFCS Type Resolution for Ambiguous Methods](done/ufcs-type-resolution.md) | Fix UFCS resolution for ambiguous methods on complex expressions |
| [Unused Variable Warnings](done/unused-variable-warnings.md) | Warn on unused variables and imports, suppressible with _ prefix |
| [User Generics & Protocol System](done/user-generics-and-traits.md) | User-defined generics and protocol system for custom types |
| [Variant Record Fields](done/variant-record-fields.md) | Named fields for enum variants (like Rust struct variants) |
| [WASM Compile Error Elimination Roadmap](done/wasm-compile-errors.md) | WASM compile error elimination (type mismatches, lambda issues) |
| [WASM Filesystem I/O](done/wasm-fs-io.md) | WASI-based filesystem I/O for WASM target |
| [WASM Local Allocation Redesign](done/wasm-local-allocation.md) | Redesign WASM function local allocation and scratch layout |
| [WASM Remaining 3 Failures — Root Cause Analysis & Fix Plan](done/wasm-remaining-3.md) | Root cause analysis of last 3 WASM test failures |
| [WASM Runtime Traps](done/wasm-runtime-traps.md) | Fix 44 WASM runtime traps (protocols, maps, closures, strings) |
| [WASM Tail Call Optimization](done/wasm-tco.md) | Tail call optimization for WASM target to prevent stack overflow |
| [WASM Validation Fixes](done/wasm-validation-fixes.md) | Fix WASM validation errors from union-find generic instantiation |
| [While Loop](done/while-loop.md) | Dedicated while loop syntax replacing do-block guard pattern |

</details>

