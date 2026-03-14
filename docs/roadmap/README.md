# Almide Roadmap

## Active

### Phase 0: Compiler Integrity (健全性・正確性・構造)
- [Type Soundness Fixes](active/type-soundness.md) — ExprId 導入, Unknown 伝播修正, occurs check, TypeVar 健全化
- [Codegen Correctness Fixes](active/codegen-correctness.md) — auto-? 統一, Range 型, guard 二重ラップ, Box パターン
- [Architecture Hardening](active/architecture-hardening.md) — IrProgram clone 除去, Emitter リファクタ, fixpoint 収束, 循環 import 検出, パーサー再帰制限
- [Cross-Target Semantics](active/cross-target-semantics.md) — Rust/TS 意味論統一: Map 比較, entries 順序, 整数オーバーフロー, Float 精度

### Phase A: Generated Code Quality
- [Clone Reduction Phase 4](active/clone-reduction.md) — for-in/list/member/match/spread clone elimination, field-level borrow analysis
- [Codegen Refinement](active/codegen-refinement.md) — let mut→let, #[inline], constant folding, string literal context, light DCE
- [Tail Call Optimization](active/tail-call-optimization.md) — Self-recursive tail calls → labeled loop in Rust codegen
- [--emit-ir: IR JSON Export](active/emit-ir.md) — `--emit-ir` flag to output typed IR as JSON
- [IR Optimization Passes](active/ir-optimization-passes.md) — IR→IR transforms: constant folding, DCE, inlining (unlocked by IR redesign)
- [RustIR: Codegen 中間表現](active/rust-ir.md) — IR → RustIR → String の 2 段パイプライン。auto-?/clone/Ok ラップを構造的に解決

### Phase B: Type System & Safety
- [Exhaustiveness Check](active/exhaustiveness-check.md) — Non-exhaustive match → compile error (currently warning)
- [Compiler Warnings](active/compiler-warnings.md) — Unused variables, dead code, unused imports, warning infrastructure
- [Type System Extensions](active/type-system.md) — Row polymorphism, union types, container protocols (LLM-friendly HKT), structural generic bounds
- [Monomorphization](active/monomorphization.md) — Generic function instantiation for structural bounds (`T: { .. }`) and container protocols

### Phase C: Tooling & Developer Experience
- [Incremental Compilation](active/incremental-compilation.md) — Skip rustc when generated code unchanged, module-level IR caching
- [IR Interpreter](active/ir-interpreter.md) — Direct IR execution for REPL, playground, fast test runs (unlocked by IR redesign)
- [New Codegen Targets](active/new-codegen-targets.md) — Go, Python, C, etc. — `&IrProgram` 入力のみで新バックエンド追加 (unlocked by IR redesign)

### Phase D: Language Extensions
- [Platform Async](active/platform-async.md) — Transparent async: `effect fn` = async on all targets. `parallel` block. No user-facing async/await
- [Template: Typed Document Builder](active/template.md) — `template` keyword, `html {}`/`text {}` builders
- [Syntax Sugar](active/syntax-sugar.md) — Lambda short syntax `(x) => expr`, default arguments, comprehensions
- [Codec Protocol & JSON](active/codec-and-json.md) — `deriving Codec` + JSON as first format, 5-phase roadmap
- [Web Framework](active/web-framework.md) — First-party Hono-equivalent, template/Codec integration

### Stdlib & Ecosystem
- [Stdlib Strategy](active/stdlib-strategy.md) — Tier 1-3 モジュール拡充、282→700+ 関数、4 戦略（TOML/extern/self-host/x-package）

### Ongoing
- [LLM Integration](active/llm-integration.md) — `almide forge` (library generation), `almide fix` (self-repair), `almide explain`
- [LLM → IR Direct Generation](active/llm-ir-generation.md) — LLM が型付き IR (JSON) を直接生成、パーサーエラーゼロ (unlocked by IR redesign)
- [Grammar Codegen](active/grammar-codegen.md) — Single source of truth for tokens/precedence
- [UFCS for External Libraries](active/ufcs-external.md) — Type-directed UFCS for user-defined types
- [Grammar Research Infrastructure](active/grammar-research.md) — A/B test syntax variants across LLMs

## On Hold

- [Benchmark Report](on-hold/benchmark-report.md)
- [Direct WASM Emission](on-hold/emit-wasm-direct.md) — `.almd → WASM bytecode` without rustc (433KB → 1-5KB)
- [Editor & GitHub Integration](on-hold/editor-github-integration.md)
- [Rainbow FFI](on-hold/rainbow-ffi.md) — Rust, JS, C, Python, Swift, Kotlin, Erlang FFI
- [LLM Immutable Sugar](on-hold/llm-immutable-sugar.md) — var indexing, `with` expression
- [Package Registry](on-hold/package-registry.md) — Lock file, semver resolution, central registry
- [Research: Modification Survival Rate Paper](on-hold/research-modification-survival-rate-paper.md)
- [Self-Hosting](on-hold/self-hosting.md) — rewrite compiler in Almide (after spec stabilization)
- [Stdlib Architecture: 3-Layer Design](on-hold/stdlib-architecture-3-layer-design.md) — Phase A done, B/C remaining
- [Supervision & Actors](on-hold/supervision-and-actors.md) — Layer 3: typed actors, channels, supervision trees
- [LSP Server](on-hold/lsp.md) — Editor integration: diagnostics, hover, go-to-def, completion
- [REPL](on-hold/repl.md) — Interactive evaluation, state accumulation, history
- [Tooling (remaining)](on-hold/tooling.md) — doc comments, benchmarking, fmt comment preservation
- [Built-in Protocols](on-hold/trait-impl.md) — Eq, Hash done; Show (`show(x)`) remaining

## Done

- [Borrow Inference](done/borrow-inference-design.md) — Lobster-style move/clone analysis
- [CLI Tool Authoring](done/cli-tool-authoring.md) — err() exit, almide run args
- [Codegen Optimization](done/codegen-optimization.md) — move analysis, borrow inference (Phase 0-3). Next: [Phase 4](active/clone-reduction.md)
- [Compiler Bug Fixes](done/compiler-bugs-from-tests.md) — 7 bugs found by test expansion, all fixed
- [Compiler Hardening](done/compiler-hardening.md)
- [Control Flow Extensions](done/control-flow.md)
- [Cross-Platform Support](done/cross-platform.md)
- [Default Field Values](done/default-field-values.md) — `field: Type = expr`, 5 variants → 3
- [Error Diagnostics](done/error-diagnostics.md) — lost mutation, "did you mean?", immutability hints
- [Error Diagnostics — Visual](done/error-diagnostics-visual.md) — color, carets, multi-span
- [Generics](done/generics.md)
- [HTTP Module](done/http.md) — server, client, multi-target
- [Language Test Suite](done/language-test-suite.md)
- [List Index Read](done/list-index-read.md) — `xs[i]` for reads
- [List Stdlib Gaps](done/list-stdlib-gaps.md) — all 3 tiers complete (52 functions)
- [Literal Syntax Gaps](done/literal-syntax-gaps.md)
- [LLM Immutable Patterns](done/llm-immutable-patterns.md) — Tier 1-2 complete, caret underlines
- [Module System v2](done/module-system-v2.md)
- [npm Package Target](done/npm-package-target-target-npm.md)
- [Playground Repair](done/playground-repair.md) — Fix with AI, repair loop, streaming
- [Proliferation Blockers](done/proliferation-blockers.md)
- [Rust Test Coverage](done/rust-test-coverage.md) — 567 cargo tests
- [Self-Tooling](done/self-tooling.md) — tree-sitter grammar generator, TextMate grammar
- [stdin / Interactive I/O](done/stdin-io.md)
- [Stdlib Completeness](done/stdlib-completeness.md)
- [Stdlib Declarative Codegen](done/stdlib-codegen.md)
- [Stdlib Gaps](done/stdlib-gaps.md)
- [Stdlib Self-Hosting](done/stdlib-self-hosting.md) — bundled .almd, path/time/hash/encoding/term migrated
- [String Handling](done/string-handling.md)
- [Test Coverage](done/test-coverage.md) — 1,700+ almd tests
- [Test Directory Structure](done/test-directory-structure.md) — `spec/` for almd, `tests/` for Rust
- [Top-Level Let](done/top-level-let.md) — `let PI = 3.14` at module scope
- [Tuple & Record](done/tuple-record.md)
- [Typed IR](done/typed-ir.md) — IR-based codegen, AST-direct codegen removed
- [Codegen IR Redesign](done/ir-redesign.md) — Self-contained typed IR, Phase 1-5 complete, AST-free codegen
- [Variant Record Fields](done/variant-record-fields.md) — named fields on enum variants, `..` rest pattern
- [Map Literal](done/map-literal.md) — `[:]` / `["key": value]` syntax, index access, direct iteration
- [Eq Protocol](done/eq-protocol.md) — automatic `==` for all value types, `Fn` types rejected
- [Error Recovery](done/error-recovery.md) — Multi-error reporting, statement/expression-level recovery
- [Lambda Type Inference](done/lambda-type-inference.md) — Bidirectional inference for lambda params
- [JSON Builder API](done/json-builder-api.md) — Superseded by [Codec Protocol & JSON](active/codec-and-json.md)
- [While Loop](done/while-loop.md) — `while condition { }`, universal loop syntax
- [Hint System](done/hint-system.md) — Pluggable hint registry, 5 modules, 61 tests, catalog
- [`import self`](done/import-self-entry.md) — `main.almd` can access `mod.almd` pub definitions via `import self`
- [UFCS Type Resolution](done/ufcs-type-resolution.md) — Recursive type inference in lowerer for member access UFCS
- [LLM Developer Experience](done/llm-developer-experience.md) — UFCS done; remaining merged into LLM Integration
- [Scaffold & Proliferation](done/scaffold-and-proliferation.md) — Merged into LLM Integration as `almide forge`
- [Trailing Lambda / Builder DSL](done/trailing-lambda-builder.md) — Won't do; stdlib approach preferred
- [Function Reference Passing](done/function-reference-passing.md) — Won't do; verbose form is always correct
- [2026 Ergonomics](2026-ergonomics.md) — `do` block pure fn support, `guard else break/continue`, `unwrap_or` UFCS fix, `json.parse` auto-`?` fix
