# Almide Roadmap

> [GRAND_PLAN.md](GRAND_PLAN.md) — 5フェーズの全体戦略

## Active

| 項目 | 説明 | Grand Plan |
|---|---|---|
| [Direct WASM Emission](active/emit-wasm-direct.md) | 130/130 ✅, DCE完了, Hello World 1,028B | Architecture |
| [Test Coverage](active/test-coverage.md) | 130ファイル. Rust/WASM 100%, TS ~97% | Phase 1 |
| [User Generics & Protocol](active/user-generics-and-traits.md) | Protocol System 実装中 | Phase 3 |
| [Effect System](active/effect-system.md) | Phase 3-4 残 | Phase 3 |
| [Performance Research](active/performance-research.md) | Rust との差 2.9% | Research |
| [HTTPS Native](active/https-native.md) | rustls統合済。almide build対応+WASM残 | Phase 1 |
| [Compiler Architecture 10/10](active/compiler-architecture-10.md) | 95/110. Phase 5-7 残 | Architecture |

## 1.0 Remaining

- [PRODUCTION_READY.md](PRODUCTION_READY.md) — チェックリスト (残: examples/cookbook, LLM計測)

| 項目 | 説明 | Grand Plan |
|---|---|---|

## On Hold — Phase 3: Runtime Foundation (2.x)

| 項目 | 説明 | Grand Plan |
|---|---|---|
| [Self-Contained Compiler](on-hold/self-contained-compiler.md) | rustc 不要化 | Architecture |
| [LSP Server](on-hold/lsp.md) | diagnostics → hover → go-to-def | Phase 3 |
| [Incremental Compilation](on-hold/incremental-compilation.md) | rustc skip when unchanged | Phase 3 |
| [Package Registry](on-hold/package-registry.md) | 公開パッケージ配布 | Phase 3 |
| [Go Target](on-hold/go-target.md) | TOML + 2-3 pass | Phase 3 |
| [Platform Architecture](on-hold/platform-architecture.md) | 5層 app runtime ビジョン | Phase 3-5 |
| [Security Model](on-hold/security-model.md) | Layer 3-5, capability | Phase 3 |
| [Effect Type Integration](on-hold/effect-type-integration.md) | FnType に EffectSet を持たせる (構文変更なし) | Phase 3 |
| ~~Trait System~~ | → Protocol System (active) に統合 | — |
| [Secure by Design](on-hold/secure-by-design.md) | | Phase 3 |
| [Async Backend](on-hold/async-backend.md) | tokio opt-in runtime | Phase 3 |
| [Supervision & Actors](on-hold/supervision-and-actors.md) | | Phase 3 |
| [Rainbow Bridge](on-hold/rainbow-bridge.md) | 外部コード → Almide | Phase 3 |
| [Rainbow FFI Gate](on-hold/rainbow-gate.md) | Almide → 外部ライブラリ | Phase 3 |

## On Hold — Phase 4-5: App Runtime & Platform (2.x+)

| 項目 | 説明 |
|---|---|
| [Almide UI](on-hold/almide-ui.md) | Reactive UI framework |
| [Web Framework](on-hold/web-framework.md) | Hono 相当 |
| [Almide Shell](on-hold/almide-shell.md) | AI-native REPL |
| [LLM Integration](on-hold/llm-integration.md) | `almide forge`, `almide fix` |
| [LLM → IR Generation](on-hold/llm-ir-generation.md) | Parser bypass |
| [Self-Hosting](on-hold/self-hosting.md) | 350KB WASM bootstrap compiler → LLM 自律進化ループ |
| [The Rumbling](on-hold/rumbling.md) | OSS 書き直しキャンペーン: Dogfood → WASM Showcase → Multi-Target → LLM Modification → Platform |

## On Hold — Compiler Internals

| 項目 | 説明 | 状態 |
|---|---|---|
| [build.rs syn Scanner](on-hold/buildrs-syn-scanner.md) | runtime scanner堅牢化 | 壊れたらやる |
| [IR Interpreter](on-hold/ir-interpreter.md) | rustc不要で即実行 | 実験的 |

## On Hold — Research / Misc

| 項目 | 説明 |
|---|---|
| [Research: MSR Paper](on-hold/research-modification-survival-rate-paper.md) | LLM accuracy 論文 |
| [Benchmark Report](on-hold/benchmark-report.md) | |
| [REPL](on-hold/repl.md) | |

## Archived (完了 or 統合済み)

以下はdone/に移動済み、または他の項目に統合:

- ~~Design Debt~~ → 完了 (gen_generated_call排除、emit_rust/emit_ts削除)
- ~~Grammar Codegen~~ → 完了 (build.rs TOML codegen)
- ~~Multi-Target Strategy~~ → Go Target に統合
- ~~New Codegen Targets~~ → Go Target に統合
- ~~Lower 2パス分離~~ → 確認済み (既に分離されてた)
- ~~Checker InferTy/Ty統一~~ → 完了 (InferTy廃止)
- ~~Polish (Immediate)~~ → 完了
- ~~Production Ready (old)~~ → PRODUCTION_READY.md に統合
- ~~Stdlib Strategy~~ → Stdlib v2: 21 native + 2 bundled, http 20関数, json 23関数
- ~~JS Target~~ → v0.9.0 で廃止。TS に統一 (Node --strip-types)
- ~~Template~~ → codegen v3 TOML templates に統合
- ~~UFCS External~~ → stdlib UFCS で対応済み
- ~~Concat Operator Reform~~ → 完了 (++ → +)
- ~~Cross-Target AOT~~ → codegen v3 で対応
- ~~Cross-Target Semantics~~ → cross-target CI 106/106 で検証済み
- ~~Platform / Target Separation~~ → Platform Architecture に統合
- ~~Stdlib 3-Layer Design~~ → stdlib verb reform で対応
- ~~TS Edge-Native~~ → v3 TS codegen で対応
- ~~Tooling (remaining)~~ → CLI-First 完了
- ~~Editor & GitHub Integration~~ → LSP に統合
- ~~LLM Immutable Sugar~~ → LLM Immutable Patterns 完了
- ~~Built-in Protocols~~ → Derive Conventions 完了
- ~~Almide Runtime~~ → Platform Architecture に統合
- ~~Direct WASM Emission (old tasks)~~ → active に集約
- ~~IR Verification~~ → 完了 (Phase 2, 25検証)

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
- [IR Verification](done/ir-verification.md) — Phase 2完了, 25検証, IrVisitor trait
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
- [Test Coverage (Phase 1-2)](done/test-coverage.md)
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
- [WASM Compile Errors](done/wasm-compile-errors.md) — 全解消, 129/129
- [WASM Local Allocation](done/wasm-local-allocation.md) — ScratchAllocator + DepthGuard
- [WASM Remaining 3](done/wasm-remaining-3.md) — lambda_id導入で全解消
- [WASM Runtime Traps](done/wasm-runtime-traps.md) — 全44 trap解消
- [WASM TCO](done/wasm-tco.md) — TailCallOptPass
- [WASM Validation Fixes](done/wasm-validation-fixes.md) — Union-Find汚染回避
- [Borrow/Clone Gaps](done/borrow-clone-gaps.md) — Case 1-9 全 FIXED
- [Quality Improvements](done/quality-improvements.md) — エラー行番号、heredoc 行追跡
- [Cross-Target CI](done/cross-target-ci.md) — 106/106 (100%), is_rust()=0, codegen v3 完全移行
- [AnonRecord Codegen](done/anon-record-codegen.md) — 空リスト型注釈 Vec::<T>::new() テンプレート化
- [Codegen v3 Architecture](done/codegen-v3-architecture.md) — Phase 1-5 完了。is_rust()=0, 106/106 cross-target, 9 nanopass
- [Codegen v3 Transform Classification](done/codegen-v3-transform-classification.md) — 42 is_rust() → 0
- [Compiler Architecture Cleanup](done/compiler-architecture-cleanup.md) — emit_rust/-2,340行, clone/deref IR化, HashMap→slice
- [CLI-First](done/cli-first.md) — run, build, test, check, fmt, clean, init, add
- [Error Codes + JSON](done/error-codes-json.md) — E001-E010, --json, --explain, test --json, check < 100ms
- [Lockfile](done/lockfile.md) — almide.lock, git deps, almide add, recursive resolution
- [Stability Contract](done/stability-contract.md) — edition, BREAKING_CHANGE_POLICY, FROZEN_API, REJECTED_PATTERNS
- [While Loop](done/while-loop.md)
- [Stdlib Verb Reform](done/stdlib-verb-system.md) — Steps 1-7 全完了。option module, Map/String/List 拡張
- [Concat Operator Reform](done/concat-operator-reform.md) — ++ → + 統一
- [Checker InferTy/Ty統一](done/checker-type-unification.md) — InferTy 廃止、Ty に統一
- [Lower 2パス分離](done/lower-two-pass.md) — 確認済み（既に分離されていた）
- [Design Debt](done/design-debt.md) — gen_generated_call排除、emit_rust/emit_ts削除
- [Test Infrastructure](done/test-infrastructure.md) — 110/110 全通過、__test_almd_ prefix、strip_tail_try
- [Test Architecture Redesign](done/test-architecture-redesign.md) — in_effect → can_call_effect + auto_unwrap 分離、in_test 除去
- [HKT Foundation](done/hkt-foundation-phase1.md) — Phase 1-4 完了。Ty::Applied 統一、Stream Fusion 全6法則、TypeConstructor/Kind/AlgebraicLaw
- [Effect System Phase 1-2](done/effect-system-phase1-2.md) — Effect推論, almide check --effects, Security Layer 2 ([permissions])
- [Compiler Bugs v0.8.4](done/compiler-bugs.md) — 12バグ全修正、400+ テスト追加
- [Stdlib Scope Reduction](done/stdlib-scope-reduction.md) — uuid/crypto削除、toml/compress/term除外、22モジュール381関数に確定
- [Stdlib Additions](done/stdlib-additions.md) — set モジュール 11→20関数 (symmetric_difference, is_subset, is_disjoint, filter, map, fold, each, any, all)
