# Almide Roadmap

## Active

### Phase B: Type System & Safety
- [Derive Conventions](active/derive-conventions.md) — Phase 1 ✅ (parser+checker), Phase 2: method resolution `dog.show()` → UFCS
- [Type System Extensions](active/type-system.md) — OpenRecord ✅, structural bounds ✅, remaining: container protocols

### Phase C: Language Extensions
- [Syntax Sugar](active/syntax-sugar.md) — Lambda ✅, default args ✅, remaining: comprehensions, named args, raw strings, block comments
- [Codec Protocol & JSON](active/codec-and-json.md) — `deriving Codec` + JSON as first format
- [Template: Typed Document Builder](active/template.md) — `template` keyword, `html {}`/`text {}` builders
- [UFCS for External Libraries](active/ufcs-external.md) — Type-directed UFCS for user-defined types
- [Structured Concurrency](active/structured-concurrency.md) — Conservative async: explicit fork/join, fail-fast semantics
- [Platform Async](active/platform-async.md) — `effect fn` = async on all targets, `parallel` block

### Phase D: Stdlib & Ecosystem
- [Stdlib Runtime Architecture](active/stdlib-self-hosted-redesign.md) — `.almd` 中心の stdlib, `@extern` でホスト依存
- [Stdlib API Surface Reform](active/stdlib-verb-system.md) — Verb 標準化: 1 verb を全コンテナ型に適用
- [Stdlib Strategy](active/stdlib-strategy.md) — 282→700+ 関数, Tier 1-3 モジュール拡充
- [Web Framework](active/web-framework.md) — First-party Hono-equivalent, template/Codec integration
- [CLI-First](active/cli-first.md) — CLI tool authoring: `almide run` for dev, `almide build` for native binary

### Phase E: Multi-Target Expansion
- [Multi-Target Strategy](active/multi-target-strategy.md) — Python, Go, Kotlin, Swift, C expansion plan
- [New Codegen Targets](active/new-codegen-targets.md) — Go, Python 優先。IR redesign により低コスト追加

### Phase F: Tooling & Infrastructure
- [Incremental Compilation](active/incremental-compilation.md) — Skip rustc when generated code unchanged, module-level IR caching
- [IR Interpreter](active/ir-interpreter.md) — Direct IR execution for REPL, playground, fast test runs
- [Error-Fix Database](active/error-fix-db.md) — Compiler error → fix suggestion mapping with before/after examples

### Ongoing: LLM & Grammar Research
- [LLM Integration](active/llm-integration.md) — `almide forge` (library generation), `almide fix` (self-repair)
- [LLM → IR Direct Generation](active/llm-ir-generation.md) — LLM が型付き IR (JSON) を直接生成
- [Grammar Codegen](active/grammar-codegen.md) — Single source of truth for tokens/precedence
- [Grammar Research Infrastructure](active/grammar-research.md) — A/B test syntax variants across LLMs

## On Hold

- [Benchmark Report](on-hold/benchmark-report.md)
- [Cross-Target AOT](on-hold/cross-target-aot.md)
- [Cross-Target Semantics](on-hold/cross-target-semantics.md) — TS ターゲット本格化時に対応。実装は大半完了、残りはテスト
- [Direct WASM Emission](on-hold/emit-wasm-direct.md) — `.almd → WASM bytecode` without rustc
- [Editor & GitHub Integration](on-hold/editor-github-integration.md)
- [LSP Server](on-hold/lsp.md) — Editor integration: diagnostics, hover, go-to-def, completion
- [LLM Immutable Sugar](on-hold/llm-immutable-sugar.md) — var indexing, `with` expression
- [Package Registry](on-hold/package-registry.md) — Lock file, semver resolution, central registry
- [Rainbow FFI](on-hold/rainbow-ffi.md) — Rust, JS, C, Python, Swift, Kotlin, Erlang FFI
- [REPL](on-hold/repl.md) — Interactive evaluation, state accumulation, history
- [Research: Modification Survival Rate Paper](on-hold/research-modification-survival-rate-paper.md)
- [Self-Hosting](on-hold/self-hosting.md) — Rewrite compiler in Almide
- [Stdlib Architecture: 3-Layer Design](on-hold/stdlib-architecture-3-layer-design.md)
- [Supervision & Actors](on-hold/supervision-and-actors.md) — Typed actors, channels, supervision trees
- [Tooling (remaining)](on-hold/tooling.md) — doc comments, benchmarking, fmt comment preservation
- [Built-in Protocols](on-hold/trait-impl.md) — Eq, Hash done; Show remaining

## Done

~~Phase 0: Compiler Integrity~~ ✅ | ~~Phase A: Generated Code Quality~~ ✅

- [2026 Ergonomics](2026-ergonomics.md) — `do` block pure fn, `guard else break/continue`, `unwrap_or`, `json.parse` auto-`?`
- [Architecture Hardening](done/architecture-hardening.md) — RustIR pipeline eliminated Emitter clones
- [Borrow Inference](done/borrow-inference-design.md) — Lobster-style move/clone analysis
- [CLI Tool Authoring](done/cli-tool-authoring.md) — err() exit, almide run args
- [Clone Reduction Phase 4](done/clone-reduction.md) — Single-use var move, range/literal skip
- [Codegen Correctness](done/codegen-correctness.md) — P1 7項 + P2 1項, auto-?, guard, do-block
- [Codegen IR Redesign](done/ir-redesign.md) — Self-contained typed IR, Phase 1-5
- [Codegen Optimization](done/codegen-optimization.md) — move analysis, borrow inference (Phase 0-3)
- [Codegen Refinement](done/codegen-refinement.md) — let mut→let demotion via IR post-pass
- [Compiler Bug Fixes](done/compiler-bugs-from-tests.md) — 7 bugs found by test expansion
- [Compiler Hardening](done/compiler-hardening.md)
- [Compiler Warnings](done/compiler-warnings.md) — Unused variables, unused imports
- [Control Flow Extensions](done/control-flow.md)
- [Cross-Platform Support](done/cross-platform.md)
- [Default Field Values](done/default-field-values.md) — `field: Type = expr`
- [--emit-ir](done/emit-ir.md) — `--emit-ir` flag for typed IR JSON export
- [Eq Protocol](done/eq-protocol.md) — automatic `==` for all value types
- [Error Diagnostics](done/error-diagnostics.md) — lost mutation, "did you mean?", immutability hints
- [Error Diagnostics — Visual](done/error-diagnostics-visual.md) — color, carets, multi-span
- [Error Recovery](done/error-recovery.md) — Multi-error reporting, recovery
- [Exhaustiveness Check](done/exhaustiveness-check.md) — Non-exhaustive match → compile error
- [Formatter Rewrite](done/formatter-rewrite.md) — 890 → 397 lines
- [Function Reference Passing](done/function-reference-passing.md) — Won't do
- [Generics](done/generics.md)
- [Hint System](done/hint-system.md) — Pluggable hint registry, 61 tests
- [HTTP Module](done/http.md) — server, client, multi-target
- [`import self`](done/import-self-entry.md)
- [IR Optimization Passes](done/ir-optimization-passes.md) — Constant folding, DCE, constant propagation
- [IR Optimization (Tier 1)](done/ir-optimization.md) — Constant folding, DCE
- [JSON Builder API](done/json-builder-api.md) — Superseded by Codec Protocol
- [Lambda Type Inference](done/lambda-type-inference.md) — Bidirectional inference
- [Language Test Suite](done/language-test-suite.md)
- [List Index Read](done/list-index-read.md) — `xs[i]`
- [List Stdlib Gaps](done/list-stdlib-gaps.md) — 52 functions
- [Literal Syntax Gaps](done/literal-syntax-gaps.md)
- [LLM Developer Experience](done/llm-developer-experience.md)
- [LLM Immutable Patterns](done/llm-immutable-patterns.md) — Tier 1-2
- [Map Literal](done/map-literal.md) — `[:]` / `["key": value]`
- [Module System v2](done/module-system-v2.md)
- [Monomorphization](done/monomorphization.md) — Generic structural bounds, mono.rs, 16 tests
- [npm Package Target](done/npm-package-target-target-npm.md)
- [Parser Error Recovery](done/parser-error-recovery.md)
- [Playground Repair](done/playground-repair.md)
- [Proliferation Blockers](done/proliferation-blockers.md)
- [Rust Test Coverage](done/rust-test-coverage.md) — 567 cargo tests
- [RustIR Pipeline](done/rust-ir.md) — IR → RustIR → String 2-stage codegen
- [Scaffold & Proliferation](done/scaffold-and-proliferation.md)
- [Self-Tooling](done/self-tooling.md)
- [stdin / Interactive I/O](done/stdin-io.md)
- [Stdlib Completeness](done/stdlib-completeness.md)
- [Stdlib Declarative Codegen](done/stdlib-codegen.md)
- [Stdlib Gaps](done/stdlib-gaps.md)
- [Stdlib Self-Hosting](done/stdlib-self-hosting.md)
- [String Handling](done/string-handling.md)
- [Structured Concurrency (Phase 1)](done/structured-concurrency.md)
- [Tail Call Optimization](done/tail-call-optimization.md) — Self-recursive → labeled loop
- [Test Coverage](done/test-coverage.md) — 1,700+ almd tests
- [Test Directory Structure](done/test-directory-structure.md)
- [Top-Level Let](done/top-level-let.md) — `let PI = 3.14`
- [Trailing Lambda / Builder DSL](done/trailing-lambda-builder.md) — Won't do
- [TS/JS Codegen Rewrite](done/ts-codegen-rewrite.md) — 2-stage pipeline
- [Tuple & Record](done/tuple-record.md)
- [Typed IR](done/typed-ir.md)
- [Type System Soundness](done/type-system-soundness.md) — B+ → A+: 14 fixes
- [UFCS Type Resolution](done/ufcs-type-resolution.md)
- [Unused Variable Warnings](done/unused-variable-warnings.md)
- [Variant Record Fields](done/variant-record-fields.md)
- [While Loop](done/while-loop.md)
