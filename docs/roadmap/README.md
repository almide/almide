# Almide Roadmap

## Active

- [LLM Integration](active/llm-integration.md) — `almide forge` (library generation), `almide fix` (self-repair), `almide explain`
- [Structured Concurrency](active/structured-concurrency.md) — Layer 2: `async let` / `await` for scoped parallel execution
- [Grammar Codegen](active/grammar-codegen.md) — Single source of truth for tokens/precedence, auto-generate tree-sitter + TextMate + lexer
- [Codec Protocol & JSON](active/codec-and-json.md) — `deriving Codec` + JSON as first format, 5-phase roadmap
- [Type System Extensions](active/type-system.md) — Row polymorphism, union types, container protocols (LLM-friendly HKT), structural generic bounds

## On Hold

- [Benchmark Report](on-hold/benchmark-report.md)
- [Direct WASM Emission](on-hold/emit-wasm-direct.md) — `.almd → WASM bytecode` without rustc
- [Editor & GitHub Integration](on-hold/editor-github-integration.md)
- [Rainbow FFI](on-hold/rainbow-ffi.md) — Rust, JS, C, Python, Swift, Kotlin, Erlang FFI
- [LLM Immutable Sugar](on-hold/llm-immutable-sugar.md) — var indexing, `with` expression
- [Package Registry](on-hold/package-registry.md)
- [Research: Modification Survival Rate Paper](on-hold/research-modification-survival-rate-paper.md)
- [Self-Hosting](on-hold/self-hosting.md) — rewrite compiler in Almide (after spec stabilization)
- [Stdlib Architecture: 3-Layer Design](on-hold/stdlib-architecture-3-layer-design.md) — Phase A done, B/C remaining
- [Supervision & Actors](on-hold/supervision-and-actors.md) — Layer 3: typed actors, channels, supervision trees (stdlib)
- [Syntax Sugar](on-hold/syntax-sugar.md) — range, exhaustiveness done; comprehensions, raw strings, block comments pending
- [Tooling](on-hold/tooling.md)
- [Built-in Protocols](on-hold/trait-impl.md) — Eq, Hash done; Show (`show(x)`) remaining

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
- [Eq Protocol](done/eq-protocol.md) — automatic `==` for all value types, `Fn` types rejected
- [Error Recovery](done/error-recovery.md) — Multi-error reporting, statement/expression-level recovery, error AST nodes, common typo detection
- [Lambda Type Inference](done/lambda-type-inference.md) — Bidirectional inference for lambda params (implemented commit 002180d)
- [JSON Builder API](done/json-builder-api.md) — Superseded by [Codec Protocol & JSON](active/codec-and-json.md)
- [While Loop](done/while-loop.md) — `while condition { }`, universal loop syntax
- [Hint System](done/hint-system.md) — Pluggable hint registry, 5 modules, 61 tests, catalog
- [`import self`](done/import-self-entry.md) — `main.almd` can access `mod.almd` pub definitions via `import self`
- [UFCS Type Resolution](done/ufcs-type-resolution.md) — Recursive type inference in lowerer for member access UFCS (`g.words.len()`)
- [LLM Developer Experience](done/llm-developer-experience.md) — UFCS done; remaining merged into LLM Integration
- [Scaffold & Proliferation](done/scaffold-and-proliferation.md) — Merged into LLM Integration as `almide forge`
- [Trailing Lambda / Builder DSL](done/trailing-lambda-builder.md) — Won't do; stdlib approach preferred
- [Function Reference Passing](done/function-reference-passing.md) — Won't do; verbose form is always correct
- [2026 Ergonomics](2026-ergonomics.md) — `do` block pure fn support, `guard else break/continue`, `unwrap_or` UFCS fix, `json.parse` auto-`?` fix
