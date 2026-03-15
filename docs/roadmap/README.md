# Almide Roadmap

## Active

### Language
- [Operator Protocol](active/operator-protocol.md) — `==` → `Dog.eq`, `"${d}"` → `Dog.repr`, auto-derive (Encode/Decode は Codec に統合)
- [Syntax Sugar](active/syntax-sugar.md) — Lambda ✅, default args ✅. Remaining: comprehensions, named args, raw strings, block comments
- [Codec Protocol & JSON](active/codec-and-json.md) — `deriving Codec` + JSON, Encode/Decode convention
- [Template](active/template.md) — Typed document builder: `html {}`/`text {}` builders
- [UFCS External](active/ufcs-external.md) — Type-directed UFCS for user-defined/external functions

### Runtime & Async
- [Structured Concurrency](active/structured-concurrency.md) — Conservative async: explicit fork/join, fail-fast
- [Platform Async](active/platform-async.md) — `effect fn` = async on all targets, `parallel` block

### Stdlib & Ecosystem
- [Stdlib Runtime Architecture](active/stdlib-self-hosted-redesign.md) — `.almd` 中心, `@extern` でホスト依存
- [Stdlib API Reform](active/stdlib-verb-system.md) — Verb 標準化
- [Stdlib Strategy](active/stdlib-strategy.md) — 282→700+ 関数
- [Web Framework](active/web-framework.md) — First-party Hono-equivalent
- [CLI-First](active/cli-first.md) — `almide run` for dev, `almide build` for native binary

### Multi-Target
- [Multi-Target Strategy](active/multi-target-strategy.md) — Python, Go, Kotlin, Swift, C
- [New Codegen Targets](active/new-codegen-targets.md) — Go, Python 優先

### Tooling
- [Incremental Compilation](active/incremental-compilation.md) — Skip rustc when unchanged
- [IR Interpreter](active/ir-interpreter.md) — Direct IR execution for REPL, playground
- [Error-Fix Database](active/error-fix-db.md) — Error → fix suggestion mapping
- [Grammar Codegen](active/grammar-codegen.md) — Single source of truth for tokens/precedence

### LLM
- [LLM Integration](active/llm-integration.md) — `almide forge`, `almide fix`
- [LLM → IR Generation](active/llm-ir-generation.md) — LLM generates typed IR directly

## On Hold

- [Benchmark Report](on-hold/benchmark-report.md)
- [Built-in Protocols](on-hold/trait-impl.md) — Eq, Hash done; Repr remaining
- [Cross-Target AOT](on-hold/cross-target-aot.md)
- [Cross-Target Semantics](on-hold/cross-target-semantics.md) — TS 本格化時。実装大半完了
- [Direct WASM Emission](on-hold/emit-wasm-direct.md)
- [Editor & GitHub Integration](on-hold/editor-github-integration.md)
- [LSP Server](on-hold/lsp.md)
- [LLM Immutable Sugar](on-hold/llm-immutable-sugar.md)
- [Package Registry](on-hold/package-registry.md)
- [Rainbow FFI](on-hold/rainbow-ffi.md)
- [REPL](on-hold/repl.md)
- [Research: MSR Paper](on-hold/research-modification-survival-rate-paper.md)
- [Self-Hosting](on-hold/self-hosting.md)
- [Stdlib 3-Layer Design](on-hold/stdlib-architecture-3-layer-design.md)
- [Supervision & Actors](on-hold/supervision-and-actors.md)
- [Tooling (remaining)](on-hold/tooling.md)

## Done

~~Phase 0~~ ✅ | ~~Phase A~~ ✅

- [2026 Ergonomics](2026-ergonomics.md)
- [Architecture Hardening](done/architecture-hardening.md)
- [Borrow Inference](done/borrow-inference-design.md)
- [CLI Tool Authoring](done/cli-tool-authoring.md)
- [Clone Reduction Phase 4](done/clone-reduction.md)
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
- [Derive Conventions](done/derive-conventions.md) — Eq/Repr/Ord/Hash, convention declaration + method resolution
- [--emit-ir](done/emit-ir.md)
- [Eq Protocol](done/eq-protocol.md)
- [Error Diagnostics](done/error-diagnostics.md)
- [Error Diagnostics — Visual](done/error-diagnostics-visual.md)
- [Error Recovery](done/error-recovery.md)
- [Exhaustiveness Check](done/exhaustiveness-check.md)
- [Formatter Rewrite](done/formatter-rewrite.md)
- [Function Reference Passing](done/function-reference-passing.md)
- [Generics](done/generics.md)
- [Grammar Research](done/grammar-research.md) — A/B test framework, lambda syntax experiment
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
- [Monomorphization](done/monomorphization.md) — Structural bounds, transitive, 16 tests
- [npm Package Target](done/npm-package-target-target-npm.md)
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
- [Tail Call Optimization](done/tail-call-optimization.md)
- [Test Coverage](done/test-coverage.md)
- [Test Directory Structure](done/test-directory-structure.md)
- [Top-Level Let](done/top-level-let.md)
- [Trailing Lambda / Builder DSL](done/trailing-lambda-builder.md)
- [TS/JS Codegen Rewrite](done/ts-codegen-rewrite.md)
- [Tuple & Record](done/tuple-record.md)
- [Typed IR](done/typed-ir.md)
- [Type System Extensions](done/type-system.md) — OpenRecord, structural bounds, union parsing
- [Type System Soundness](done/type-system-soundness.md)
- [UFCS Type Resolution](done/ufcs-type-resolution.md)
- [Unused Variable Warnings](done/unused-variable-warnings.md)
- [Variant Record Fields](done/variant-record-fields.md)
- [While Loop](done/while-loop.md)
