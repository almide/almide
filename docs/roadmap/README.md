# Almide Roadmap

> Auto-generated from directory structure. Run `bash docs/roadmap/generate-readme.sh > docs/roadmap/README.md` to update.
>
> [GRAND_PLAN.md](GRAND_PLAN.md) — 5-phase strategy

## Active

13 items

| Item | Description |
|------|-------------|
| [LLM Benchmark Execution](active/benchmark-execution.md) | **優先度:** 高 — Almide の存在意義「LLM が最も正確に書ける言語」の実証 |
| [Compiler Architecture: All 10s [ACTIVE]](active/compiler-architecture-10.md) | **目標**: コンパイラアーキテクチャ全項目 10/10 |
| [Cross-Target Parity Matrix [ACTIVE]](active/cross-target-parity-matrix.md) | **優先度:** High — WASM 対応進行中の今が最適タイミング |
| [Diagnostic end_col — エラー波線の精度向上](active/diagnostic-end-col.md) | **優先度:** 低〜中 — 診断品質の仕上げ。セカンダリスパン活性化で主要な改善は完了済み |
| [Effect System — Auto-Inferred Capabilities](active/effect-system.md) | **優先度:** 1.x (情報表示) → 2.x (制限適用) |
| [Emit Readability [ACTIVE]](active/emit-readability.md) | **優先度:** Medium — LLM が修正する生成コードの品質に直結 |
| [GPU Compute — Matrix型とコンパイラ駆動のGPU実行](active/gpu-compute.md) | **優先度:** Phase 3 (Runtime Foundation) |
| [HTTPS Native Support [ACTIVE]](active/https-native.md) | **目標**: `http.get("https://...")` が全ターゲットで動く |
| [Purity Exploitation — fn/effect fn 区別の活用](active/purity-exploitation.md) | **優先度:** Medium — 構文追加ゼロで性能・表現力・実用性を向上 |
| [Stdlib in Almide: 統一ライブラリアーキテクチャ [ACTIVE]](active/stdlib-in-almide.md) | **目標**: stdlib を Almide で書き直し、userlib と同じ仕組みにする。全ライブラリが 3 層構造で動く。 |
| [Unwrap operators: `!` `??` `?`](active/unwrap-operators.md) | Three postfix operators that unify Result and Option handling. Replaces auto-`?` insertion, `From` c |
| [WASM HTTP Client](active/wasm-http-client.md) | **優先度:** 中 — V8 Isolate 環境での実用性に直結するが、WASI の制約で短期解決が難しい |
| [WASM Remaining FS Operations](active/wasm-remaining-fs.md) | **優先度:** 中 — read_text/write/exists は実装済み。残りは実用上の必要に応じて追加 |

## On Hold

28 items

| Item | Description |
|------|-------------|
| [Almide Shell](on-hold/almide-shell.md) | An interactive shell that replaces Bash/Zsh — combining Almide's type system, Result-based error han |
| [Almide UI — Reactive Web Framework as Almide Library [ON HOLD]](on-hold/almide-ui.md) | Almide で書かれた SolidJS ライクなリアクティブ UI フレームワークを **Almide のライブラリとして** 構築する。コンパイラにフレームワーク固有の最適化パスを追加しない。コン |
| [Async Backend — tokio opt-in](on-hold/async-backend.md) | 現在の sync/thread backend に加え、async backend を追加する。tokio は言語仕様に混ぜず、backend の1実装として提供。 |
| [build.rs Runtime Scanner 堅牢化](on-hold/buildrs-syn-scanner.md) | **優先度:** post-1.0 |
| [Compile-Time Contracts [ON HOLD]](on-hold/compile-time-contracts.md) | **優先度:** 2.x — 型システム安定後 |
| [Effect Type Integration — FnType に EffectSet を持たせる](on-hold/effect-type-integration.md) | **優先度:** 2.x |
| [Error-Fix Database [ON HOLD]](on-hold/error-fix-db.md) | Structured mapping from every compiler error to fix suggestions with before/after code examples. Ena |
| [Go Target](on-hold/go-target.md) | **優先度:** post-1.0 |
| [Incremental Compilation [ON HOLD]](on-hold/incremental-compilation.md) | Every `almide run` / `almide build` performs the full pipeline from scratch: |
| [IR Interpreter [ON HOLD]](on-hold/ir-interpreter.md) | IR を直接実行するインタプリタ。codegen → rustc を経由せずに即時実行。 |
| [IR Optimization Tier 2 [ON HOLD]](on-hold/ir-optimization-tier2.md) | **優先度:** Medium — 全ターゲットに自動適用される最適化 |
| [LLM Integration [ON HOLD]](on-hold/llm-integration.md) | 「LLMが最も正確に書ける言語」のコンパイラに LLM を組み込む。LLMが書いて、LLMが直して、LLMがライブラリを生やす — このループがコンパイラ1つで回る。 |
| [LLM → IR Direct Generation [ON HOLD]](on-hold/llm-ir-generation.md) | LLM がテキストではなく型付き IR (JSON) を直接生成し、パーサーエラーをゼロにする。 |
| [LSP Server [ON HOLD]](on-hold/lsp.md) | Language Server Protocol implementation for editor integration. Split from [tooling.md](../on-hold/t |
| [Package Registry [ON HOLD]](on-hold/package-registry.md) | Pin exact commit hashes for reproducible builds: |
| [Performance Research: Path to World #1 [ON-HOLD]](on-hold/performance-research.md) | **Research thesis**: High-level semantic information preserved through the compilation pipeline enab |
| [Almide Platform Architecture Vision](on-hold/platform-architecture.md) | **優先度:** post-1.0 (2.x) |
| [Rainbow Bridge — 外部コードを Almide パッケージにする [ON HOLD]](on-hold/rainbow-bridge.md) | Rust crate、npm パッケージ、Python ライブラリ等の外部コードを Almide パッケージとしてラップし、ユーザーには **普通の Almide ライブラリにしか見えない** 形で提 |
| [Rainbow FFI Gate [ON HOLD]](on-hold/rainbow-gate.md) | Almide で書いたコードを、どの言語からでもネイティブ速度で呼べるライブラリとして出力する。トランスパイラではなく **ライブラリコンパイラ**。 |
| [REPL [ON HOLD]](on-hold/repl.md) | Interactive Read-Eval-Print Loop for Almide. Split from [tooling.md](../on-hold/tooling.md). |
| [Research: Modification Survival Rate Paper [ON HOLD]](on-hold/research-modification-survival-rate-paper.md) | Target: arXiv preprint — *"Designing Programming Languages for LLM Code Modification: Measuring Surv |
| [The Rumbling — Almide OSS Rewrite Campaign](on-hold/rumbling.md) | **Status**: On Hold (Block 0 は言語機能の成熟後) |
| [Secure by Design [ON HOLD]](on-hold/secure-by-design.md) | Almide は Rust がメモリ安全であるのと同じ意味で **Web 安全** な言語になる。「気をつけて書けば安全」ではなく「普通に書いたら安全。意図的に壊そうとしない限り壊れない」。 |
| [Security Model — Layer 3–5](on-hold/security-model.md) | Almide のセキュリティは 5 層で構成される。 |
| [Self-Contained Compiler: Remove rustc Dependency [ACTIVE]](on-hold/self-contained-compiler.md) | **目標**: `almide build` が rustc を必要としない。Go のように自己完結したコンパイラ。 |
| [Self-Hosting: Autonomous Bootstrap Compiler](on-hold/self-hosting.md) | **Status**: On Hold (Phase 3+ prerequisite) |
| [Supervision & Actors [ON HOLD]](on-hold/supervision-and-actors.md) | Layer 3 of Almide's async model. Provides long-lived concurrent processes, typed message passing, an |
| [Web Framework [ON HOLD]](on-hold/web-framework.md) | Almide の first-party web framework。Hono 相当の DX を Almide の思想で実現する。 |

## Done

158 items

<details>
<summary>Show all 158 completed items</summary>

| Item | Description |
|------|-------------|
| [2026 Ergonomics Roadmap](done/2026-ergonomics.md) | Self-tooling (Chrome extension, TextMate generator, Playground modules) で発見された |
| [Almide Runtime — 地球上最高の性能を目指すコンパイラ](done/almide-runtime.md) | 既存の言語は汎用性の代償を払っている。Almide は制約が強い。それが武器になる: |
| [Anonymous Record Codegen 修正](done/anon-record-codegen.md) | **優先度:** High — Grammar Lab 実験で全タスクの 30% が影響 |
| [Architecture Hardening [ACTIVE]](done/architecture-hardening.md) | コンパイラの構造的脆弱性の修正。言語の成長・新機能追加に伴い必ず踏む地雷を事前に除去する。 |
| [Benchmark Report: LLM Code Generation Cost by Language [ON HOLD]](done/benchmark-report.md) | Publish a credible benchmark report showing that Almide achieves the lowest LLM code generation cost |
| [Borrow/Clone Gaps [ACTIVE]](done/borrow-clone-gaps.md) | Rust codegen が変数の clone を挿入し損ねるケースを徹底的に潰す。 |
| [Borrow Inference — Detailed Design [COMPLETE]](done/borrow-inference-design.md) | All phases implemented. See `src/emit_rust/borrow.rs` for the analysis and `src/emit_rust/program.rs |
| [Checker InferTy/Ty 統一](done/checker-type-unification.md) | **優先度:** post-1.0 (1.x) |
| [CLI-First: Almide で CLI ツールを快適に書ける状態を作る [ACTIVE]](done/cli-first.md) | Almide で実用的な CLI ツールを書き、開発時は `almide run` で即実行、配布時は `almide build` で単一ネイティブバイナリを生成できる。同じコードが TS パスでも |
| [CLI Tool Authoring Issues [DONE]](done/cli-tool-authoring.md) | Issues discovered while implementing the miniconf benchmark in Almide. Both fixed. |
| [Clone Reduction Phase 4 [ACTIVE]](done/clone-reduction.md) | Phases 0-3 (done in [codegen-optimization](../done/codegen-optimization.md)) established single-use  |
| [Codec Advanced [ACTIVE]](done/codec-advanced.md) | Codec 基盤 (encode/decode/Value/JSON roundtrip) は完成。高度な機能。 |
| [Codec Protocol & JSON [ACTIVE]](done/codec-and-json.md) | Trustworthy structured data boundaries for humans, programs, and models. |
| [Codec Implementation Plan [ACTIVE]](done/codec-implementation.md) | Layer 1: Codec (コンパイラ)     T ←→ Value |
| [Codec Remaining [ACTIVE]](done/codec-remaining.md) | Phase 0-2 完了。残りの機能。 |
| [Codec Test Specification [ACTIVE]](done/codec-test-spec.md) | Swift Codable / Serde / Kotlin serialization / Jackson の知見に基づくテストケース集。 |
| [Codegen Correctness Fixes [DONE]](done/codegen-correctness.md) | 生成コードの正確性に関わる問題の修正。 |
| [Codegen Optimization [IN PROGRESS]](done/codegen-optimization.md) | Almide generates Rust code that is near-identical in performance to hand-written Rust for numeric wo |
| [Codegen Refinement [ACTIVE]](done/codegen-refinement.md) | Small, independent optimizations that improve generated Rust code quality. Each is low-difficulty an |
| [Codegen v3: 三層アーキテクチャ](done/codegen-v3-architecture.md) | **優先度:** High — 1.0 後の target 拡張（Go, Python）の前提条件 |
| [Codegen v3: 変換の完全分類](done/codegen-v3-transform-classification.md) | Codegen がやっている全変換を、必要な文脈の深さで 3 段階に分類する。 |
| [Compiler Architecture Cleanup](done/compiler-architecture-cleanup.md) | **優先度:** Medium — 1.0後でもいいが、やるなら早い方がいい |
| [Compiler Bugs Found by Test Expansion [ACTIVE]](done/compiler-bugs-from-tests.md) | テストカバレッジ拡大（806→1501）で発見されたコンパイラバグ7件。テスト側で回避中だが、コンパイラを修正してテストをあるべき姿に戻す。 |
| [Compiler Bugs and Gaps — Status](done/compiler-bugs.md) | Discovered while writing 400+ new test blocks across 32 test files (v0.8.4). |
| [Compiler Hardening [DONE]](done/compiler-hardening.md) | Eliminate all panics and unhandled edge cases. Other languages never crash on invalid input — Almide |
| [Compiler Warnings [ACTIVE]](done/compiler-warnings.md) | Infrastructure for emitting warnings (distinct from errors) for code quality issues. Currently the c |
| [Concatenation Operator Reform](done/concat-operator-reform.md) | **優先度:** High — 1.0 前の breaking change 候補 |
| [Control Flow Extensions [CLOSED]](done/control-flow.md) | **Closed**: `while`, `break`, `continue`, `return` contradict the design philosophy. |
| [Cross-Platform Support [DONE]](done/cross-platform.md) | Almide is a write-once language — platform differences are the compiler's problem, never the user's. |
| [Cross-Target AOT Compilation [PLANNED]](done/cross-target-aot.md) | Almide は既に複数の emit ターゲット（Rust / TS / JS / 将来 WASM）を持ち、`@extern` でターゲット別の実装を宣言的に切り替えられる。この構造を活かし、`alm |
| [Cross-Target CI [DONE]](done/cross-target-ci.md) | 全テストを Rust ターゲットと TS ターゲットの両方で実行し、出力が一致することを自動検証する。 |
| [Cross-Target Semantics [ACTIVE]](done/cross-target-semantics.md) | 同じ `.almd` が Rust と TS で異なる結果を出すケースの修正。Almide の「同じコードが両方で動く」前提を保証する。 |
| [Default Field Values [DONE]](done/default-field-values.md) | Self-tooling exposed three design smells that share a single root cause: |
| [Derive Conventions [DONE]](done/derive-conventions.md) | trait/typeclass を導入せず、固定 convention + コロン構文で polymorphism を実現する。 |
| [D. 設計的な債務](done/design-debt.md) | **状態:** TOML テンプレート dispatch が list/string/map/int/float/math/result/option のみ接続。残りのモジュール (fs, http, |
| [Diagnostic Secondary Spans [DONE]](done/diagnostic-secondary-spans.md) | **完了日:** 2026-03-25 |
| [Editor & GitHub Integration [ON HOLD]](done/editor-github-integration.md) | Repository: [almide/almide-editors](https://github.com/almide/almide-editors) |
| [Effect fn Result Wrapping [DONE]](done/effect-fn-result-wrapping.md) | **優先度:** 1.0 blocker |
| [Effect Isolation (Security Layer 1)](done/effect-isolation.md) | pure fn は I/O 不可能。コンパイラが静的に検証。 |
| [Effect System — Phase 1-2](done/effect-system-phase1-2.md) | **完了日:** 2026-03-19 |
| [--emit-ir: IR JSON Export [ACTIVE]](done/emit-ir.md) | Add `--emit-ir` flag to output the typed IR as JSON, complementing the existing `--emit-ast`. |
| [Direct WASM Emission [ACTIVE]](done/emit-wasm-direct.md) | src/codegen/emit_wasm/ |
| [Eq Protocol [DONE]](done/eq-protocol.md) | Automatic `==` / `!=` for all value types. No `deriving` needed. |
| [Error Codes + JSON Output [DONE — 1.0 Phase II]](done/error-codes-json.md) |  |
| [Error Diagnostics — Visual Improvements [ACTIVE]](done/error-diagnostics-visual.md) | Tier 2+ items split from the original error-diagnostics roadmap. These improve human developer exper |
| [Error Diagnostics [DONE]](done/error-diagnostics.md) | warning: return value of list.set() is unused |
| [Error Recovery [DONE]](done/error-recovery.md) | LLMはコードを1箇所ずつ直すのではなく、**全エラーを一括で見て一発で直す**のが最も効率的。現状の「最初の1エラーで止まる」挙動は、LLMとの対話ループを不必要に増やしている。 |
| [Exercise Suite v0.6.0](done/exercises-v060.md) | **Compiler bugs found & fixed during Tier 4:** |
| [Exhaustiveness Check → Hard Error [ACTIVE]](done/exhaustiveness-check.md) | Pattern matching exhaustiveness is checked but only emits **warnings**, not errors. Non-exhaustive ` |
| [Fan Concurrency — Almide 非同期統合設計 [ACTIVE]](done/fan-concurrency.md) | 本ドキュメントは以下を統合し、Almide の非同期・並行処理の唯一の設計仕様とする: |
| [fan.map 並行数制限 (limit)](done/fan-map-limit.md) | `fan.map(xs, limit: n, f)` — 同時実行数の上限付き fan.map。 |
| [Formatter Rewrite [ACTIVE]](done/formatter-rewrite.md) | `src/fmt.rs` (890行) を 0 から書き直し。旧コードは lambda 構文更新済みだが、全体設計が古い。 |
| [Function Reference Passing [WON'T DO]](done/function-reference-passing.md) | Make passing named functions as arguments seamless, eliminating unnecessary closure wrappers. |
| [Generic Variant Type Instantiation](done/generic-variant-instantiation.md) | **テスト:** `spec/lang/type_system_test.almd` |
| [Generics [DONE]](done/generics.md) | Generic functions, generic record types, generic variant types, call-site type arguments, and recurs |
| [Grammar Codegen: Single Source of Truth [ACTIVE] [P1]](done/grammar-codegen.md) | Almideの文法が3箇所に分散している: |
| [Grammar Research Infrastructure [ACTIVE]](done/grammar-research.md) | Almide の文法設計判断を「勘」ではなく「数字」で回す。 |
| [Guard `ok(value)` Value Loss in Effect Do-Block](done/guard-ok-value-loss.md) | **Test:** `spec/lang/error_test.almd` |
| [Higher-Order Function Type Inference](done/higher-order-fn-inference.md) | **Test:** `spec/lang/function_test.almd` |
| [Hint System Architecture [ACTIVE] [P0]](done/hint-system.md) | Almideの差別化は「LLMが全エラーを見て一発で直せる」こと。そのためにはエラーメッセージが**原因**を指す必要がある。現状、親切処理（ヒント、typo検出、missing comma等）がパー |
| [HKT Foundation — Phase 1-4 + Stream Fusion 全 6 法則](done/hkt-foundation-phase1.md) | **完了日:** 2026-03-19 |
| [HKT Foundation — 完了](done/hkt-foundation.md) | **全 Phase 完了。** このドキュメントはアーカイブ待ち。 |
| [HTTP Module [DONE]](done/http.md) |  |
| [`import self` — Package Entry Point Access [DONE]](done/import-self-entry.md) | `main.almd` から同パッケージの `mod.almd`（ライブラリエントリーポイント）の pub 関数にアクセスできない。 |
| [IR Optimization Passes [ACTIVE]](done/ir-optimization-passes.md) | IR → IR 変換パスを codegen の前に挟み、生成コードの品質を向上させる。 |
| [IR Optimization Passes [ACTIVE]](done/ir-optimization.md) | IR → IR の最適化パスを追加。定数畳み込み、デッドコード除去 (DCE)、簡易インライニング。 |
| [Codegen IR Redesign [DONE]](done/ir-redesign.md) | Self-contained typed IR — codegen が AST を一切参照せず、IR のみで完全なコード生成を行う。Phase 1〜5 全完了。 |
| [IR Verification & Self-Describing IR [ACTIVE]](done/ir-verification.md) | Debug-only integrity checks + IR self-description improvements. Verification runs after optimization |
| [JSON Builder API [SUPERSEDED]](done/json-builder-api.md) | The current `json.from_string`, `json.from_int`, etc. API is verbose for constructing JSON objects: |
| [Bidirectional Type Inference for Lambda Parameters [DONE]](done/lambda-type-inference.md) | **Status**: 実装完了 (commit 002180d, 2026-03-14)。checker の two-pass 推論 + lowerer への型伝搬を実装。 |
| [Language Test Suite [DONE]](done/language-test-suite.md) | Almide の言語機能を体系的にテストするスイート。`lang/` に配置。 |
| [Let-Polymorphism (Algorithm W)](done/let-polymorphism.md) | let f = (x) => x        // f : fn(?0) -> ?0 (monomorphic) |
| [List Index Read (`xs[i]`) [ACTIVE]](done/list-index-read.md) | Almide supports index-based **write** (`xs[i] = value`) but not index-based **read** (`xs[i]`). Read |
| [List Stdlib Gaps](done/list-stdlib-gaps.md) | Almide's `list` module compared against 7 languages. All operations are immutable (return new list). |
| [Literal Syntax Gaps [DONE]](done/literal-syntax-gaps.md) | All items implemented as of v0.4.7. |
| [LLM Developer Experience [DONE / MERGED]](done/llm-developer-experience.md) | Currently `almide init` always generates a `CLAUDE.md` file for AI-assisted development. |
| [LLM × Immutable Data Structures [ACTIVE]](done/llm-immutable-patterns.md) | LLMs trained on Python/JS/Go default to mutable algorithms. Almide's immutable lists cause systemati |
| [LLM Immutable Sugar [ON HOLD]](done/llm-immutable-sugar.md) | Language-level sugar for immutable collection mutations. Split from llm-immutable-patterns.md Tier 3 |
| [almide.lock [DONE — 1.0 Phase III]](done/lockfile.md) |  |
| [Lower 2パス分離](done/lower-two-pass.md) | **優先度:** post-1.0 |
| [Map Literal Syntax](done/map-literal.md) | Add Map literal syntax to the language, enabling bidirectional type inference for empty Maps. |
| [Module System v2 [DONE]](done/module-system-v2.md) | myapp/ (application)               mylib/ (library) |
| [Monomorphization [ACTIVE]](done/monomorphization.md) | Generic structural bounds (`T: { name: String, .. }`) の Rust codegen に必要な、関数のモノモーフィゼーション基盤。 |
| [Multi-Target Strategy [ACTIVE]](done/multi-target-strategy.md) | Almide のマルチターゲット設計が新しいターゲット言語の追加コストを最小化する構造になっていることを活かし、ターゲット言語の拡充戦略を定める。 |
| [New Codegen Targets [ACTIVE]](done/new-codegen-targets.md) | IR redesign 完了により、新ターゲット追加のコストが大幅低下。`&IrProgram` を受け取って文字列を返すだけで新バックエンドが書ける。 |
| [npm Package Target [DONE]](done/npm-package-target-target-npm.md) | Compile Almide code into a publish-ready npm package. Write libraries in Almide and distribute them  |
| [Open Record / Row Polymorphism — 実装ガイド](done/open-record-structural-typing.md) | **テスト:** `spec/lang/open_record_test.almd` |
| [Operator Protocol [ACTIVE]](done/operator-protocol.md) | Convention 宣言に基づく演算子・言語機能のディスパッチ。 |
| [Parser Error Recovery [ACTIVE]](done/parser-error-recovery.md) | 1 つのシンタックスエラーでパースが停止する現状を改善。複数エラーを報告し、エラー後もパースを継続する。 |
| [Platform / Target Separation [ON HOLD]](done/platform-target-separation.md) | `--target` に出力形式とプラットフォームの 2 つの意味が混在している。これを分離する。 |
| [Playground Repair Turn [DONE]](done/playground-repair.md) | ユーザーがPlaygroundでコードを書いて Run → エラー → 「Fix with AI」→ LLMがエラーを読んで修正 → ユーザーが修正過程を見る。 |
| [A. すぐ効果が出る磨き](done/polish-immediate.md) | **状態:** `[ICE] lower: missing type for expr id=N` が毎回 stderr に出る |
| [B. 実用化に必要なもの](done/production-ready.md) | **状態:** 以下のモジュールが `runtime/rust/src/` に存在しない |
| [Proliferation Blockers [DONE]](done/proliferation-blockers.md) | Issues discovered during the first `almide proliferate` run (csv module). These were compiler-level  |
| [C. 品質向上](done/quality-improvements.md) | `emit()` メソッドで全 checker diagnostic に自動 span 付与。22 箇所を修正。 |
| [Recursive Type Box Insertion](done/recursive-type-box.md) | **Test:** `spec/lang/eq_protocol_test.almd` |
| [Remove `do` Block [DONE]](done/remove-do-block.md) | **完了**: 2026-03-24 |
| [Runtime Gaps — Complete](done/runtime-gaps.md) | 全 22 モジュール / 355 関数のランタイム実装完了 (100%)。 |
| [Runtime Layout Unification [ACTIVE]](done/runtime-layout.md) | Rust と TypeScript のランタイムが別々の場所・別々の形式で管理されている。 |
| [RustIR: Rust Codegen 中間表現 [ACTIVE]](done/rust-ir.md) | 現在の Rust codegen は `IR → 文字列` を1パスで行い、25+ フィールドの Emitter 構造体が状態フラグ（`in_effect`, `in_do_block`, `skip |
| [Rust Compiler Test Coverage [ACTIVE]](done/rust-test-coverage.md) | Rust-side unit/integration tests (`cargo test`). Separate from `.almd` language tests. |
| [almide scaffold & Module Proliferation Pipeline [MERGED]](done/scaffold-and-proliferation.md) | Infrastructure for mass-producing Almide modules. Enables LLMs to autonomously generate, verify, and |
| [Self-Tooling: Editor Tools Written in Almide [DONE]](done/self-tooling.md) | Demonstrate that Almide's entire editor ecosystem can be written in Almide itself, produced by AI fr |
| [サーバー非同期 — http.serve effect 化](done/server-async.md) | `http.serve` のハンドラを effect コンテキスト化し、ハンドラ内から effect fn を呼べるようにする。 |
| [Showcase 1: almide-grep (CLI Tool)](done/showcase-1-cli-grep.md) | **領域:** CLI tool |
| [Showcase 2: Todo API (HTTP API)](done/showcase-2-http-api.md) | **領域:** HTTP API server |
| [Showcase 3: CSV→JSON Pipeline (Data Processing)](done/showcase-3-data-pipeline.md) | **領域:** Data processing |
| [Showcase 4: Markdown→HTML (DevTool)](done/showcase-4-devtool-md2html.md) | **領域:** DevTool / テキスト変換 |
| [Showcase 5: dotenv Loader (Script)](done/showcase-5-script-dotenv.md) | **領域:** Script / 設定管理 |
| [Stability Contract [DONE — 1.0 Phase II]](done/stability-contract.md) |  |
| [stdin / Interactive I/O [DONE]](done/stdin-io.md) | import io |
| [Stdlib Additions — 完了](done/stdlib-additions.md) | **優先度:** 1.x — 1.0後に段階的追加 |
| [Stdlib Architecture: 3-Layer Design [ON HOLD]](done/stdlib-architecture-3-layer-design.md) | Almide の stdlib を 3 層に分離する。WASM を一級市民として扱い、pure な計算と OS 依存を明確に分ける。 |
| [Declarative Stdlib Codegen [DONE]](done/stdlib-codegen.md) | Inspired by React Native's TurboModules architecture: define stdlib once in a declarative format, au |
| [Stdlib Completeness [DONE]](done/stdlib-completeness.md) | Fill gaps that make Almide less capable than Python/Go for everyday tasks. |
| [Stdlib Gaps [DONE]](done/stdlib-gaps.md) | Reduce boilerplate in AI-generated code, improving LOC, token count, and generation time. |
| [Stdlib Import Control [DONE]](done/stdlib-import-control.md) | **優先度:** 1.0 |
| [Stdlib Scope Reduction — 完了](done/stdlib-scope-reduction.md) | **優先度:** 1.0前 — 凍結前に外に出すものを決める |
| [Stdlib Runtime Architecture Reform [ACTIVE]](done/stdlib-self-hosted-redesign.md) | stdlib は `.almd` を中心に定義される。純粋ロジックは Almide 自身で実装される。ホスト依存機能だけ `@extern` でターゲット実装を持つ。ネイティブ実装は本物の Rust/ |
| [Stdlib Self-Hosting [DONE]](done/stdlib-self-hosting.md) | As of v0.2.1, all stdlib functions have been extracted from inline codegen to separated runtime file |
| [Stdlib Strategy [ACTIVE]](done/stdlib-strategy.md) | 普及には「書きたいものがすぐ書ける」stdlib の厚みが必要。現在 15 モジュール 266 関数。主要言語と比較すると： |
| [Stdlib API Surface Reform [ACTIVE]](done/stdlib-verb-system.md) | stdlib の全コンテナ型で同じ動詞が同じ意味を持ち、LLM が1つの動詞を学べば全型に適用できる状態を作る。 |
| [ストリーミング — WebSocket, SSE, Stream](done/streaming.md) | リアルタイム通信とストリーミングデータ処理のサポート。 |
| [String Handling [DONE]](done/string-handling.md) | Multi-line strings with `"""..."""` syntax. |
| [Structured Concurrency [ACTIVE]](done/structured-concurrency.md) | Non-goals: novel concurrency syntax, implicit parallelism, actor primitives in the language. Almide' |
| [Syntax Sugar [ACTIVE]](done/syntax-sugar.md) | let xs = 0..10        // [0, 1, 2, ..., 9] |
| [Tail Call Optimization [ACTIVE]](done/tail-call-optimization.md) | Self-recursive tail calls → labeled loop transformation in Rust codegen. |
| [Result Builder [ACTIVE]](done/template.md) | Swift の Result Builder と同じ思想の **汎用 builder 機構** を Almide に導入する。`builder` は言語コアの機能であり、Html / Text / C |
| [Test Architecture Redesign](done/test-architecture-redesign.md) | Test infrastructure is complex because `in_effect` conflates two orthogonal concerns: |
| [Test Coverage](done/test-coverage-v2.md) | **Current**: 129 test files, 2,042 .almd test blocks (Rust target). All 129 pass on WASM target too. |
| [Language Test Coverage (`almide test`) [ACTIVE]](done/test-coverage.md) | `.almd` language-level tests. Separate from Rust compiler tests (`cargo test`, see [rust-test-covera |
| [Test Directory Structure Redesign [DONE]](done/test-directory-structure.md) | テスト関連がルートに散らばっていた（`lang/`, `stdlib/`, `exercises/`, `tests/`）問題を解決。 |
| [Tooling [ON HOLD — items split to active]](done/tooling.md) | Most items from this roadmap have been promoted to dedicated active roadmaps: |
| [Top-Level Let [DONE]](done/top-level-let.md) | Constant values require zero-argument functions as a workaround: |
| [Trailing Lambda / Builder DSL [WON'T DO]](done/trailing-lambda-builder.md) | Explore Kotlin-style trailing lambda or builder patterns for structured data construction. |
| [Built-in Protocols [ON HOLD]](done/trait-impl.md) | **All protocols are automatic.** The compiler determines protocol support from the type structure. N |
| [TS/JS Codegen Rewrite [ACTIVE]](done/ts-codegen-rewrite.md) | `src/emit_ts/` を書き直し。Rust codegen と同じ 2 段パイプライン (IR → TsIR → String) に統一。 |
| [TS Edge-Native Deployment [ON HOLD]](done/ts-edge-native.md) | Almide の `--target ts` 出力は **素の TypeScript/JavaScript** であり、V8 が直接実行する。WASM を経由しない。これにより Cloudflare  |
| [TS Target: Result 維持 (Erasure → Object)](done/ts-result-maintenance.md) | // 現在 (erasure): effect fn → throw/catch |
| [TypeScript Test Runner [DONE]](done/ts-test-runner.md) | **完了日:** 2026-03-25 |
| [Tuple & Record [DONE]](done/tuple-record.md) | type Point = {x: Int, y: Int} |
| [Type System Soundness [ACTIVE]](done/type-system-soundness.md) | 型システムの健全性を B+ → A+ に引き上げる。Critical 3 + High 4 + Medium 3 + P1 4 = 14 修正完了。 |
| [Type System Theory Upgrade — HM Integration Plan](done/type-system-theory-upgrade.md) | Almide's checker is **constraint-based without type schemes** — a pragmatic simplification of Hindle |
| [Type System Extensions [ACTIVE]](done/type-system.md) | The type system is Almide's primary lever for surpassing other AI-targeted languages. The goal: **ca |
| [Typed IR [ACTIVE]](done/typed-ir.md) | Almide's codegen goes directly from AST to target language strings. This causes: |
| [UFCS for External Libraries [ACTIVE]](done/ufcs-external.md) | UFCS は現在 stdlib にハードコードされている（`src/stdlib.rs` の `resolve_ufcs_candidates`）。外部ライブラリの関数は module prefix  |
| [UFCS Type Resolution for Ambiguous Methods [DONE]](done/ufcs-type-resolution.md) | Ambiguous UFCS methods (`len`, `join`, `contains`, `slice`, `reverse`, `index_of`, `count`) fail whe |
| [Unused Variable Warnings [ACTIVE]](done/unused-variable-warnings.md) | 未使用の変数・インポートに対して warning を出す。`_` プレフィックスで抑制可能。 |
| [User Generics & Protocol System](done/user-generics-and-traits.md) | **優先度:** 1.x |
| [Variant Record Fields [DONE]](done/variant-record-fields.md) | Allow enum variants to carry named fields (like Rust's struct variants), instead of only positional  |
| [WASM Compile Error 根絶ロードマップ](done/wasm-compile-errors.md) | **症状**: `expected i32, found i64` or `expected i64, found i32` |
| [WASM Filesystem I/O [DONE]](done/wasm-fs-io.md) | **完了日:** 2026-03-25 |
| [WASM Local Allocation 再設計](done/wasm-local-allocation.md) | WASM function local layout: |
| [WASM Remaining 3 Failures — Root Cause Analysis & Fix Plan](done/wasm-remaining-3.md) | **Symptom**: `values remaining on stack at end of block` in func 73 (fold closure) |
| [WASM Runtime Traps [ACTIVE]](done/wasm-runtime-traps.md) | basic protocol method, basic protocol satisfaction, builder pattern via protocol, |
| [WASM Tail Call Optimization](done/wasm-tco.md) | Almide の TCO 戦略はターゲット依存: |
| [WASM Validation Fixes](done/wasm-validation-fixes.md) | generic 関数 `fn either_map_right[A, B, C](e: Either[A, B], f: (B) -> C) -> Either[A, C]` で: |
| [While Loop [DONE]](done/while-loop.md) | Almide has no dedicated conditional loop syntax. The current workaround uses `do` blocks with guards |

</details>

## Stdlib

22 items

| Item | Description |
|------|-------------|
| [stdlib: compress [Tier 3]](stdlib/compress.md) | 圧縮・展開。ファイル操作やネットワーク通信で必要。 |
| [stdlib: crypto [Tier 2]](stdlib/crypto.md) | 暗号機能。Almide は現在 `hash` モジュール（bundled .almd, SHA/MD5 のみ）を持つが、HMAC・暗号化・署名・安全な乱数がない。 |
| [stdlib: csv [Tier 2]](stdlib/csv.md) | CSV パース/生成。データ処理の基本フォーマット。 |
| [stdlib: datetime [Tier 1]](stdlib/datetime.md) | 日時操作。これがないと実用的なアプリケーションが書けない。現在 `time` モジュール（bundled .almd）に基本的な Duration/Timestamp があるが、日時パース・フォーマッ |
| [stdlib: error [Tier 1]](stdlib/error.md) | 構造化エラー型。現在 Almide の `Result[T, E]` のエラー型は常に `String`。エラーの分類・チェーン・コンテキスト付加ができない。 |
| [stdlib: fs (拡充) [Tier 1]](stdlib/fs.md) | 現在 19 関数。基本的な read/write/exists はあるが、ディレクトリ操作・メタデータ・temp が足りない。 |
| [stdlib: html [Tier 2]](stdlib/html.md) | HTML パース・クエリ・テキスト抽出。スクレイピング、テスト、テンプレート出力検証に使う。 |
| [stdlib: http (拡充) [Tier 1]](stdlib/http-expansion.md) | 現在 8 関数（GET/POST/PUT/DELETE + サーバー基本）。ヘッダ操作・ステータス・レスポンスビルダーが足りない。 |
| [stdlib: log [Tier 3]](stdlib/log.md) | 構造化ログ。アプリケーション開発の基盤。 |
| [stdlib: mime [Tier 3]](stdlib/mime.md) | MIME タイプ判定。Go, Python, Deno に存在。ファイルアップロード、HTTP レスポンス、コンテンツ判定で使う。 |
| [stdlib: net (TCP/UDP) [Tier 3]](stdlib/net.md) | 低レベルネットワーキング。Go, Python, Rust に標準で存在。 |
| [Stdlib Module References](stdlib/README.md) | モジュールごとの他言語比較と Almide への追加候補。 |
| [stdlib: set [Tier 2]](stdlib/set.md) | 集合型。Python, Rust, JS 全てにある基本データ構造。Almide にはない。 |
| [stdlib: sorted collections [Tier 3]](stdlib/sorted.md) | ソート済みデータ構造。Go/Python/Rust/Deno 全てに何らかの形で存在。 |
| [stdlib: sql [Tier 3]](stdlib/sql.md) | データベースアクセス。パラメタライズドクエリ中心の安全な SQL 実行。 |
| [stdlib: test [Tier 3]](stdlib/test.md) | テストユーティリティ。現在 `assert`, `assert_eq`, `assert_ne` のみ。 |
| [stdlib: toml [Tier 2]](stdlib/toml.md) | TOML パース/生成。設定ファイルの標準フォーマット。Almide 自身も `almide.toml` を使う。 |
| [stdlib: unicode [Tier 3]](stdlib/unicode.md) | Unicode 文字分類。Go, Python, Rust に標準で存在。 |
| [stdlib: url [Tier 2]](stdlib/url.md) | URL のパース・構築・クエリパラメータ操作。現在 Almide にはない。 |
| [stdlib: uuid [Tier 2]](stdlib/uuid.md) | UUID の生成・パース・フォーマット。小さいが頻出するモジュール。 |
| [stdlib: websocket [Tier 3]](stdlib/websocket.md) | WebSocket クライアント/サーバー。リアルタイム通信の基盤。 |
| [stdlib: yaml [Tier 2]](stdlib/yaml.md) | YAML パース/生成。設定ファイル・CI 定義で広く使われる。 |

