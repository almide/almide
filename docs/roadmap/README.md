# Almide Roadmap

## Active

- [Test Coverage](active/test-coverage.md) — 790 cases, target 1500+

## On Hold

- [Benchmark Report](on-hold/benchmark-report.md)
- [Direct WASM Emission](on-hold/emit-wasm-direct.md)
- [Editor & GitHub Integration](on-hold/editor-github-integration.md)
- [Function Reference Passing](on-hold/function-reference-passing.md) — low priority, verbose form is always correct
- [Interop / FFI](on-hold/interop.md)
- [LLM Developer Experience](on-hold/llm-developer-experience.md)
- [LLM Immutable Sugar](on-hold/llm-immutable-sugar.md) — var indexing, `with` expression
- [Package Registry](on-hold/package-registry.md)
- [Research: Modification Survival Rate Paper](on-hold/research-modification-survival-rate-paper.md)
- [Scaffold & Proliferation Pipeline](on-hold/scaffold-and-proliferation.md)
- [Self-Hosting](on-hold/self-hosting.md) — rewrite compiler in Almide (after spec stabilization)
- [Stdlib Architecture: 3-Layer Design](on-hold/stdlib-architecture-3-layer-design.md) — Phase A done, B/C remaining
- [Structured Concurrency](on-hold/structured-concurrency.md)
- [Syntax Sugar](on-hold/syntax-sugar.md) — range, raw strings, exhaustiveness done; comprehensions pending
- [Tooling](on-hold/tooling.md)
- [Trailing Lambda / Builder DSL](on-hold/trailing-lambda-builder.md) — won't do, solve with stdlib instead
- [trait / impl](on-hold/trait-impl.md) — parser done, checker/emitter partial
- [Type System Extensions](on-hold/type-system.md)

## Done

- [CLI Tool Authoring](done/cli-tool-authoring.md) — err() exit, almide run args
- [Codegen Optimization](done/codegen-optimization.md) — move analysis, borrow inference (Phase 0-3)
- [Compiler Hardening](done/compiler-hardening.md)
- [Default Field Values](done/default-field-values.md) — `field: Type = expr`, 5 variants → 3
- [Control Flow Extensions](done/control-flow.md)
- [Cross-Platform Support](done/cross-platform.md)
- [Error Diagnostics](done/error-diagnostics.md) — lost mutation, "did you mean?", immutability hints
- [Error Diagnostics — Visual](done/error-diagnostics-visual.md) — color, carets, multi-span
- [Generics](done/generics.md)
- [HTTP Module](done/http.md) — server, client, multi-target
- [List Stdlib Gaps](done/list-stdlib-gaps.md) — all 3 tiers complete (52 functions)
- [LLM Immutable Patterns](done/llm-immutable-patterns.md) — Tier 1-2 complete, caret underlines
- [Language Test Suite](done/language-test-suite.md)
- [Literal Syntax Gaps](done/literal-syntax-gaps.md)
- [Module System v2](done/module-system-v2.md)
- [npm Package Target](done/npm-package-target-target-npm.md)
- [Playground Repair](done/playground-repair.md) — Fix with AI, repair loop, streaming
- [Proliferation Blockers](done/proliferation-blockers.md)
- [Self-Tooling](done/self-tooling.md) — tree-sitter grammar generator, Chrome extension, TextMate grammar
- [Variant Record Fields](done/variant-record-fields.md) — named fields on enum variants, `..` rest pattern
- [stdin / Interactive I/O](done/stdin-io.md)
- [Stdlib Completeness](done/stdlib-completeness.md)
- [Stdlib Declarative Codegen](done/stdlib-codegen.md)
- [Stdlib Gaps](done/stdlib-gaps.md)
- [Stdlib Self-Hosting](done/stdlib-self-hosting.md) — bundled .almd, path/time/hash/encoding/term migrated
- [String Handling](done/string-handling.md)
- [Tuple & Record](done/tuple-record.md)
