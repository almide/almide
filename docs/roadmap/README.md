# Almide Roadmap

## Active — 1.0 Critical Path

### Phase II: 安定性契約
- ~~Stdlib Verb Reform~~ ✅ — Steps 1-7 全完了。32 関数追加、`from_entries` 削除

### 1.0 Showcase
- ~~5 Showcases~~ ✅ — CLI/API/Data/DevTool/Script 全完成

### Compiler
- ~~Compiler Architecture Cleanup~~ ✅ — clone/deref IR化, HashMap→slice (emit_rust/ -2,340行削除)

### Codegen
- ~~Codegen v3~~ ✅ — Phase 1-5 全完了。is_rust()=0, cross-target 106/106。Go target は [on-hold](on-hold/go-target.md)


### 全体
- [PRODUCTION_READY.md](PRODUCTION_READY.md) — 1.0 基準、10 言語からの教訓

## On Hold

### 1.x (post-1.0)
- [Async Backend](on-hold/async-backend.md) — tokio opt-in runtime
- [Design Debt](on-hold/design-debt.md) — gen_generated_call, anonymous records
- [Error-Fix Database](on-hold/error-fix-db.md) — Error → fix mapping
- [Fan Map Limit](on-hold/fan-map-limit.md) — `fan.map(xs, limit: n, f)`
- [Grammar Codegen](on-hold/grammar-codegen.md) — Single source of truth
- [Incremental Compilation](on-hold/incremental-compilation.md) — Skip rustc when unchanged
- [IR Interpreter](on-hold/ir-interpreter.md) — Direct IR execution
- [LSP Server](on-hold/lsp.md)
- [Multi-Target Strategy](on-hold/multi-target-strategy.md) — Go, Python 優先
- [New Codegen Targets](on-hold/new-codegen-targets.md) — Go, Python
- [Go Target](on-hold/go-target.md) — TOML + 2-3 pass。v3 アーキテクチャ準備完了
- [Lower 2パス分離](on-hold/lower-two-pass.md) — AST→IR と use-count analysis を分離
- [Checker InferTy/Ty統一](on-hold/checker-type-unification.md) — 型推論の二重型システム統一
- [build.rs syn Scanner](on-hold/buildrs-syn-scanner.md) — runtime scanner堅牢化
- [Package Registry](on-hold/package-registry.md)
- [Polish (Immediate)](on-hold/polish-immediate.md)
- [Production Ready (old)](on-hold/production-ready.md)
- [Server Async](on-hold/server-async.md) — `http.serve` effect 化
- [Stdlib Strategy](on-hold/stdlib-strategy.md) — 282→700+ 関数 (段階的)
- [Streaming](on-hold/streaming.md) — WebSocket, SSE
- [Template](on-hold/template.md) — Typed document builder
- [UFCS External](on-hold/ufcs-external.md) — User-defined UFCS

### 2.x+
- [Almide Shell](on-hold/almide-shell.md) — AI-native REPL
- [Almide UI](on-hold/almide-ui.md) — Reactive UI framework
- [LLM Immutable Sugar](on-hold/llm-immutable-sugar.md)
- [LLM Integration](on-hold/llm-integration.md) — `almide forge`, `almide fix`
- [LLM → IR Generation](on-hold/llm-ir-generation.md) — Parser bypass
- [Rainbow Bridge](on-hold/rainbow-bridge.md) — 外部コード → Almide パッケージ
- [Rainbow FFI Gate](on-hold/rainbow-gate.md) — Almide → 外部言語ライブラリ
- [Security Model](on-hold/security-model.md) — Layer 2-5
- [Self-Hosting](on-hold/self-hosting.md)
- [Supervision & Actors](on-hold/supervision-and-actors.md)
- [Web Framework](on-hold/web-framework.md) — Hono 相当

### その他
- [Benchmark Report](on-hold/benchmark-report.md)
- [Built-in Protocols](on-hold/trait-impl.md) — Repr remaining
- [Cross-Target AOT](on-hold/cross-target-aot.md)
- [Cross-Target Semantics](on-hold/cross-target-semantics.md)
- [Direct WASM Emission](on-hold/emit-wasm-direct.md)
- [Editor & GitHub Integration](on-hold/editor-github-integration.md)
- [Platform / Target Separation](on-hold/platform-target-separation.md)
- [REPL](on-hold/repl.md)
- [Research: MSR Paper](on-hold/research-modification-survival-rate-paper.md)
- [Secure by Design](on-hold/secure-by-design.md)
- [Stdlib 3-Layer Design](on-hold/stdlib-architecture-3-layer-design.md)
- [Tooling (remaining)](on-hold/tooling.md)
- [TS Edge-Native](on-hold/ts-edge-native.md)

## Done

~~Phase 0~~ ✅ | ~~Phase A~~ ✅ | ~~Phase B~~ ✅

- [2026 Ergonomics](done/2026-ergonomics.md) — do block 純粋化, unwrap_or, json.parse auto-?
- [Architecture Hardening](done/architecture-hardening.md)
- [Borrow Inference](done/borrow-inference-design.md)
- [CLI Tool Authoring](done/cli-tool-authoring.md)
- [Clone Reduction Phase 4](done/clone-reduction.md)
- [Codec Implementation](done/codec-implementation.md)
- [Codec Protocol & JSON](done/codec-and-json.md)
- [Codec Advanced](done/codec-advanced.md)
- [Codec Remaining](done/codec-remaining.md)
- [Codec Test Spec](done/codec-test-spec.md)
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
- [Derive Conventions](done/derive-conventions.md)
- [Effect Isolation](done/effect-isolation.md) — Security Layer 1
- [--emit-ir](done/emit-ir.md)
- [Eq Protocol](done/eq-protocol.md)
- [Error Diagnostics](done/error-diagnostics.md)
- [Error Diagnostics — Visual](done/error-diagnostics-visual.md)
- [Error Recovery](done/error-recovery.md)
- [Exhaustiveness Check](done/exhaustiveness-check.md)
- [Exercise Suite v0.6.0](done/exercises-v060.md)
- [Fan Concurrency](done/fan-concurrency.md) — fan { }, map, race, any, settle, timeout
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
- [Operator Protocol](done/operator-protocol.md)
- [Parser Error Recovery](done/parser-error-recovery.md)
- [Playground Repair](done/playground-repair.md)
- [Proliferation Blockers](done/proliferation-blockers.md)
- [Rust Test Coverage](done/rust-test-coverage.md)
- [Runtime Gaps](done/runtime-gaps.md) — 22 モジュール / 355 関数 (100%)
- [Runtime Layout Unification](done/runtime-layout.md)
- [RustIR Pipeline](done/rust-ir.md)
- [Scaffold & Proliferation](done/scaffold-and-proliferation.md)
- [Self-Tooling](done/self-tooling.md)
- [stdin / Interactive I/O](done/stdin-io.md)
- [Stdlib Completeness](done/stdlib-completeness.md)
- [Stdlib Declarative Codegen](done/stdlib-codegen.md)
- [Stdlib Gaps](done/stdlib-gaps.md)
- [Stdlib Runtime Architecture](done/stdlib-self-hosted-redesign.md)
- [Stdlib Self-Hosting](done/stdlib-self-hosting.md)
- [String Handling](done/string-handling.md)
- [Structured Concurrency (Phase 1)](done/structured-concurrency.md)
- [Syntax Sugar](done/syntax-sugar.md)
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
- [Borrow/Clone Gaps](done/borrow-clone-gaps.md) — Case 1-9 全 FIXED
- [Quality Improvements](done/quality-improvements.md) — エラー行番号、heredoc 行追跡
- [Cross-Target CI](done/cross-target-ci.md) — 106/106 (100%), is_rust()=0, codegen v3 完全移行
- [AnonRecord Codegen](done/anon-record-codegen.md) — 空リスト型注釈 Vec::<T>::new() テンプレート化
- [Codegen v3 Architecture](done/codegen-v3-architecture.md) — Phase 1-5 完了。is_rust()=0, 106/106 cross-target, 9 nanopass
- [Codegen v3 Transform Classification](done/codegen-v3-transform-classification.md) — 42 is_rust() → 0, 全てテンプレート/pass/annotation化
- [Compiler Architecture Cleanup](done/compiler-architecture-cleanup.md) — emit_rust/-2,340行, clone/deref IR化, HashMap→slice
- [CLI-First](done/cli-first.md) — run, build, test, check, fmt, clean, init, add
- [Error Codes + JSON](done/error-codes-json.md) — E001-E010, --json, --explain, test --json, check < 100ms
- [Lockfile](done/lockfile.md) — almide.lock, git deps, almide add, recursive resolution
- [Stability Contract](done/stability-contract.md) — edition, BREAKING_CHANGE_POLICY, FROZEN_API, REJECTED_PATTERNS, HIDDEN_OPERATIONS
- [While Loop](done/while-loop.md)
- [Stdlib Verb Reform](done/stdlib-verb-system.md) — Steps 1-7 全完了。option module, Map/String/List 拡張
