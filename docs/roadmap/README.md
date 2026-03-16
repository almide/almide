# Almide Roadmap

## Active

### Stdlib v2
- [Stdlib API Reform](active/stdlib-verb-system.md) — Verb 標準化 (from_string, to_upper, len 等)
- [Stdlib Strategy](active/stdlib-strategy.md) — 282→700+ 関数

### Security
- [Security Model](active/security-model.md) — Layer 2-5: Capability restriction, package boundary, runtime sandbox, supply chain

### Compiler
- [Borrow/Clone Gaps](active/borrow-clone-gaps.md) — 変数 clone 挿入漏れの修正（関数引数+補間、if/else 分岐）

### Ecosystem
- [CLI-First](active/cli-first.md) — `almide run` + `almide build`

### Multi-Target
- [Multi-Target Strategy](active/multi-target-strategy.md) — Python, Go, Kotlin, Swift, C

### Quality
- [Quality Improvements](active/quality-improvements.md) — エラーメッセージ、テストカバレッジ

## On Hold

- [Almide Shell](on-hold/almide-shell.md) — AI-native shell replacing Bash/Zsh
- [Almide UI](on-hold/almide-ui.md) — Almide 製 SolidJS ライクな reactive UI
- [Async Backend](on-hold/async-backend.md) — tokio opt-in runtime
- [Benchmark Report](on-hold/benchmark-report.md)
- [Built-in Protocols](on-hold/trait-impl.md) — Eq, Hash done; Repr remaining
- [Cross-Target AOT](on-hold/cross-target-aot.md)
- [Cross-Target Semantics](on-hold/cross-target-semantics.md) — TS 本格化時
- [Design Debt](on-hold/design-debt.md) — gen_generated_call, anonymous records, borrow analysis
- [Direct WASM Emission](on-hold/emit-wasm-direct.md)
- [Editor & GitHub Integration](on-hold/editor-github-integration.md)
- [Error-Fix Database](on-hold/error-fix-db.md) — Error → fix mapping
- [Fan Map Limit](on-hold/fan-map-limit.md) — `fan.map(xs, limit: n, f)` 並行数制限
- [Grammar Codegen](on-hold/grammar-codegen.md) — Single source of truth
- [Incremental Compilation](on-hold/incremental-compilation.md) — Skip rustc when unchanged
- [IR Interpreter](on-hold/ir-interpreter.md) — Direct IR execution
- [LLM Immutable Sugar](on-hold/llm-immutable-sugar.md)
- [LLM Integration](on-hold/llm-integration.md) — `almide forge`, `almide fix`
- [LLM → IR Generation](on-hold/llm-ir-generation.md) — LLM generates typed IR directly
- [LSP Server](on-hold/lsp.md)
- [New Codegen Targets](on-hold/new-codegen-targets.md) — Go, Python 優先
- [Package Registry](on-hold/package-registry.md)
- [Platform / Target Separation](on-hold/platform-target-separation.md)
- [Polish (Immediate)](on-hold/polish-immediate.md)
- [Production Ready](on-hold/production-ready.md)
- [Rainbow Bridge](on-hold/rainbow-bridge.md) — 外部コードを Almide パッケージとしてラップ
- [Rainbow FFI Gate](on-hold/rainbow-gate.md) — Almide コードを外の言語から呼べる
- [REPL](on-hold/repl.md) — Almide Shell に統合
- [Research: MSR Paper](on-hold/research-modification-survival-rate-paper.md)
- [Secure by Design](on-hold/secure-by-design.md) — opaque 型 + capability 推論
- [Self-Hosting](on-hold/self-hosting.md)
- [Server Async](on-hold/server-async.md) — `http.serve` ハンドラ effect 化
- [Stdlib 3-Layer Design](on-hold/stdlib-architecture-3-layer-design.md)
- [Streaming](on-hold/streaming.md) — WebSocket, SSE, Stream
- [Supervision & Actors](on-hold/supervision-and-actors.md)
- [Template](on-hold/template.md) — Typed document builder
- [Tooling (remaining)](on-hold/tooling.md)
- [TS Edge-Native Deployment](on-hold/ts-edge-native.md)
- [UFCS External](on-hold/ufcs-external.md) — User-defined UFCS
- [Web Framework](on-hold/web-framework.md) — First-party Hono-equivalent

## Done

~~Phase 0~~ ✅ | ~~Phase A~~ ✅ | ~~Phase B~~ ✅

- [2026 Ergonomics](2026-ergonomics.md)
- [Architecture Hardening](done/architecture-hardening.md)
- [Borrow Inference](done/borrow-inference-design.md)
- [CLI Tool Authoring](done/cli-tool-authoring.md)
- [Clone Reduction Phase 4](done/clone-reduction.md)
- [Codec Implementation](done/codec-implementation.md) — Value, auto-derive, JSON roundtrip, runtime crate
- [Codec Protocol & JSON](done/codec-and-json.md) — 設計仕様完成
- [Codec Advanced](done/codec-advanced.md) — Variant decode, DecodeError, value utils
- [Codec Remaining](done/codec-remaining.md) — Variant encode, value utils, naming strategy
- [Codec Test Spec](done/codec-test-spec.md) — P0 14/14 ✅
- [Codegen Correctness](done/codegen-correctness.md)
- [Codegen IR Redesign](done/ir-redesign.md)
- [Codegen Optimization](done/codegen-optimization.md)
- [Codegen Refinement](done/codegen-refinement.md)
- [Compiler Bug Fixes](done/compiler-bugs-from-tests.md)
- [Compiler Hardening](done/compiler-hardening.md)
- [Compiler Warnings](done/compiler-warnings.md)
- [Control Flow Extensions](done/control-flow.md)
- [Cross-Platform Support](done/cross-platform.md)
- [Default Field Values](done/default-field-values.md)
- [Derive Conventions](done/derive-conventions.md) — Eq/Repr/Ord/Hash
- [Effect Isolation](done/effect-isolation.md) — Security Layer 1: pure fn cannot call effect fn
- [--emit-ir](done/emit-ir.md)
- [Eq Protocol](done/eq-protocol.md)
- [Error Diagnostics](done/error-diagnostics.md)
- [Error Diagnostics — Visual](done/error-diagnostics-visual.md)
- [Error Recovery](done/error-recovery.md)
- [Exhaustiveness Check](done/exhaustiveness-check.md)
- [Exercise Suite v0.6.0](done/exercises-v060.md) — 23 exercises / 330+ tests
- [Fan Concurrency](done/fan-concurrency.md) — `fan { }` / `fan.map` / `fan.race` / `fan.any` / `fan.settle` / `fan.timeout`
- [Formatter Rewrite](done/formatter-rewrite.md)
- [Function Reference Passing](done/function-reference-passing.md)
- [Generics](done/generics.md)
- [Grammar Research](done/grammar-research.md)
- [Hint System](done/hint-system.md)
- [HTTP Module](done/http.md)
- [`import self`](done/import-self-entry.md)
- [IR Optimization Passes](done/ir-optimization-passes.md)
- [IR Optimization (Tier 1)](done/ir-optimization.md)
- [JSON Builder API](done/json-builder-api.md)
- [Lambda Type Inference](done/lambda-type-inference.md)
- [Language Test Suite](done/language-test-suite.md)
- [List Index Read](done/list-index-read.md)
- [List Stdlib Gaps](done/list-stdlib-gaps.md)
- [Literal Syntax Gaps](done/literal-syntax-gaps.md)
- [LLM Developer Experience](done/llm-developer-experience.md)
- [LLM Immutable Patterns](done/llm-immutable-patterns.md)
- [Map Literal](done/map-literal.md)
- [Module System v2](done/module-system-v2.md)
- [Monomorphization](done/monomorphization.md)
- [npm Package Target](done/npm-package-target-target-npm.md)
- [Operator Protocol](done/operator-protocol.md) — `==` dispatch, auto-derive
- [Parser Error Recovery](done/parser-error-recovery.md)
- [Playground Repair](done/playground-repair.md)
- [Proliferation Blockers](done/proliferation-blockers.md)
- [Rust Test Coverage](done/rust-test-coverage.md)
- [Runtime Gaps](done/runtime-gaps.md) — 全 22 モジュール / 355 関数 (100%)
- [Runtime Layout Unification](done/runtime-layout.md) — `runtime/{rust,ts}/` 統一完了
- [RustIR Pipeline](done/rust-ir.md)
- [Scaffold & Proliferation](done/scaffold-and-proliferation.md)
- [Self-Tooling](done/self-tooling.md)
- [stdin / Interactive I/O](done/stdin-io.md)
- [Stdlib Completeness](done/stdlib-completeness.md)
- [Stdlib Declarative Codegen](done/stdlib-codegen.md)
- [Stdlib Gaps](done/stdlib-gaps.md)
- [Stdlib Runtime Architecture](done/stdlib-self-hosted-redesign.md) — Runtime crate `almide_rt` 基盤完成
- [Stdlib Self-Hosting](done/stdlib-self-hosting.md)
- [String Handling](done/string-handling.md)
- [Structured Concurrency (Phase 1)](done/structured-concurrency.md)
- [Syntax Sugar](done/syntax-sugar.md) — Lambda, default/named args, block comments, raw strings
- [Tail Call Optimization](done/tail-call-optimization.md)
- [Test Coverage](done/test-coverage.md)
- [Test Directory Structure](done/test-directory-structure.md)
- [Top-Level Let](done/top-level-let.md)
- [Trailing Lambda / Builder DSL](done/trailing-lambda-builder.md)
- [TS/JS Codegen Rewrite](done/ts-codegen-rewrite.md)
- [Tuple & Record](done/tuple-record.md)
- [Typed IR](done/typed-ir.md)
- [Type System Extensions](done/type-system.md)
- [Type System Soundness](done/type-system-soundness.md)
- [UFCS Type Resolution](done/ufcs-type-resolution.md)
- [Unused Variable Warnings](done/unused-variable-warnings.md)
- [Variant Record Fields](done/variant-record-fields.md)
- [While Loop](done/while-loop.md)
