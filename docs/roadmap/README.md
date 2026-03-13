# Almide Roadmap

## Active

- [LLM Integration](active/llm-integration.md) — `almide forge` (library generation), `almide fix` (self-repair), `almide explain`

## On Hold

- [Benchmark Report](on-hold/benchmark-report.md)
- [Cross-Target AOT Compilation](on-hold/cross-target-aot.md) — Compile to native binary via Rust, TS via Deno/Bun, WASM via wasm-pack
- [Direct WASM Emission](on-hold/emit-wasm-direct.md) — `.almd → WASM bytecode` without rustc
- [Editor & GitHub Integration](on-hold/editor-github-integration.md)
- [Function Reference Passing](on-hold/function-reference-passing.md) — low priority, verbose form is always correct
- [Rainbow FFI](on-hold/rainbow-ffi.md) — Rust, JS, C, Python, Swift, Kotlin, Erlang FFI
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
- [Built-in Protocols](on-hold/trait-impl.md) — Show (`show(x)`), Hash (Map key constraint) remaining; all automatic
- [Type System Extensions](on-hold/type-system.md)

## Done

- [Borrow Inference](done/borrow-inference-design.md) — Lobster-style move/clone analysis
- [CLI Tool Authoring](done/cli-tool-authoring.md) — err() exit, almide run args
- [Codegen Optimization](done/codegen-optimization.md) — move analysis, borrow inference (Phase 0-3)
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
- [Rust Test Coverage](done/rust-test-coverage.md) — 470 cargo tests
- [Self-Tooling](done/self-tooling.md) — tree-sitter grammar generator, TextMate grammar
- [stdin / Interactive I/O](done/stdin-io.md)
- [Stdlib Completeness](done/stdlib-completeness.md)
- [Stdlib Declarative Codegen](done/stdlib-codegen.md)
- [Stdlib Gaps](done/stdlib-gaps.md)
- [Stdlib Self-Hosting](done/stdlib-self-hosting.md) — bundled .almd, path/time/hash/encoding/term migrated
- [String Handling](done/string-handling.md)
- [Test Coverage](done/test-coverage.md) — 1501 almd tests achieved
- [Test Directory Structure](done/test-directory-structure.md) — `spec/` for almd, `tests/` for Rust
- [Top-Level Let](done/top-level-let.md) — `let PI = 3.14` at module scope
- [Tuple & Record](done/tuple-record.md)
- [Typed IR](done/typed-ir.md) — IR-based codegen, AST-direct codegen removed
- [Variant Record Fields](done/variant-record-fields.md) — named fields on enum variants, `..` rest pattern
- [Map Literal](done/map-literal.md) — `[:]` / `["key": value]` syntax, index access, direct iteration
- [Eq Protocol](on-hold/trait-impl.md) — automatic `==` for all value types, `Fn` types rejected
- [Error Recovery](done/error-recovery.md) — Multi-error reporting, statement/expression-level recovery, error AST nodes, common typo detection
- [While Loop](done/while-loop.md) — `while condition { }`, universal loop syntax
