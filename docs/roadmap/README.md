# Almide Roadmap

## Active

### Stdlib v2
- [Stdlib Runtime Architecture](active/stdlib-self-hosted-redesign.md) — Runtime crate `almide_rt` 基盤完成。@extern 移行中
- [Stdlib API Reform](active/stdlib-verb-system.md) — Verb 標準化 (from_string, to_upper, len 等)
- [Stdlib Strategy](active/stdlib-strategy.md) — 282→700+ 関数
- [Runtime Layout Unification](active/runtime-layout.md) — `runtime/{rust,ts}/` に統一、TS を素 .ts に切り出し

### Language
- [Template](active/template.md) — Typed document builder
- [UFCS External](active/ufcs-external.md) — User-defined UFCS

### Security
- [Security Model](active/security-model.md) — 5 層セキュリティ。Layer 1 (Effect Isolation) 実装済み

### Compiler
- [Borrow/Clone Gaps](active/borrow-clone-gaps.md) — 変数 clone 挿入漏れの修正（関数引数+補間、if/else 分岐）

### Testing
- [Exercise Suite v0.6.0](active/exercises-v060.md) — 23 exercises / 330+ tests, 6 tiers, cross-target

### Ecosystem
- [Almide Shell](active/almide-shell.md) — AI-native shell replacing Bash/Zsh
- [Web Framework](active/web-framework.md) — First-party Hono-equivalent
- [CLI-First](active/cli-first.md) — `almide run` + `almide build`

### Multi-Target
- [Multi-Target Strategy](active/multi-target-strategy.md) — Python, Go, Kotlin, Swift, C
- [New Codegen Targets](active/new-codegen-targets.md) — Go, Python 優先

### Tooling
- [Incremental Compilation](active/incremental-compilation.md) — Skip rustc when unchanged
- [IR Interpreter](active/ir-interpreter.md) — Direct IR execution
- [Error-Fix Database](active/error-fix-db.md) — Error → fix mapping
- [Grammar Codegen](active/grammar-codegen.md) — Single source of truth

### LLM
- [LLM Integration](active/llm-integration.md) — `almide forge`, `almide fix`
- [LLM → IR Generation](active/llm-ir-generation.md) — LLM generates typed IR directly

## On Hold

- [Benchmark Report](on-hold/benchmark-report.md)
- [Built-in Protocols](on-hold/trait-impl.md) — Eq, Hash done; Repr remaining
- [Cross-Target AOT](on-hold/cross-target-aot.md)
- [Cross-Target Semantics](on-hold/cross-target-semantics.md) — TS 本格化時
- [Direct WASM Emission](on-hold/emit-wasm-direct.md)
- [Platform / Target Separation](on-hold/platform-target-separation.md) — `--target` と `--platform` 分離、@extern プラットフォーム階層
- [Secure by Design](on-hold/secure-by-design.md) — Web 安全を言語の性質に。opaque 型 + capability 推論 + supply chain integrity
- [TS Edge-Native Deployment](on-hold/ts-edge-native.md) — Almide→TS でエッジ最速。WASM 不要、V8 JIT フル活用
- [Almide UI](on-hold/almide-ui.md) — Almide 製 SolidJS ライクな reactive UI。コンパイラ汎用最適化で Svelte 級
- [Editor & GitHub Integration](on-hold/editor-github-integration.md)
- [LSP Server](on-hold/lsp.md)
- [LLM Immutable Sugar](on-hold/llm-immutable-sugar.md)
- [Package Registry](on-hold/package-registry.md)
- [Rainbow Bridge](on-hold/rainbow-bridge.md) — 外部コード (Rust/TS/Python等) を Almide パッケージとしてラップ
- [Rainbow FFI Gate](on-hold/rainbow-gate.md) — Almide コードを外の言語から呼べるライブラリとして出力
- [REPL](on-hold/repl.md) — Almide Shell に統合
- [Research: MSR Paper](on-hold/research-modification-survival-rate-paper.md)
- [Self-Hosting](on-hold/self-hosting.md)
- [Stdlib 3-Layer Design](on-hold/stdlib-architecture-3-layer-design.md)
- [Async Backend](on-hold/async-backend.md) — tokio opt-in runtime
- [Fan Map Limit](on-hold/fan-map-limit.md) — `fan.map(xs, limit: n, f)` 並行数制限
- [Server Async](on-hold/server-async.md) — `http.serve` ハンドラ effect 化
- [Streaming](on-hold/streaming.md) — WebSocket, SSE, Stream
- [Supervision & Actors](on-hold/supervision-and-actors.md)
- [Tooling (remaining)](on-hold/tooling.md)

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
- [--emit-ir](done/emit-ir.md)
- [Eq Protocol](done/eq-protocol.md)
- [Error Diagnostics](done/error-diagnostics.md)
- [Error Diagnostics — Visual](done/error-diagnostics-visual.md)
- [Error Recovery](done/error-recovery.md)
- [Exhaustiveness Check](done/exhaustiveness-check.md)
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
- [RustIR Pipeline](done/rust-ir.md)
- [Scaffold & Proliferation](done/scaffold-and-proliferation.md)
- [Self-Tooling](done/self-tooling.md)
- [stdin / Interactive I/O](done/stdin-io.md)
- [Stdlib Completeness](done/stdlib-completeness.md)
- [Stdlib Declarative Codegen](done/stdlib-codegen.md)
- [Stdlib Gaps](done/stdlib-gaps.md)
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
- [Fan Concurrency](done/fan-concurrency.md) — `fan { }` / `fan.map` / `fan.race` / `fan.any` / `fan.settle` / `fan.timeout` — sync/thread backend
- Runtime Gaps — 全 22 モジュール / 355 関数 (100%)。regex は自前エンジン（350 行、外部 crate 不要）
- [While Loop](done/while-loop.md)
