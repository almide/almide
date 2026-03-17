# Lessons from MoonBit for Almide's Production Readiness

Research date: 2026-03-17

MoonBit is a WASM-first, multi-backend (wasm-gc, JS, native) language started in late 2022 by a team with OCaml/ReScript lineage. It has reached 0.8.x with an open-source compiler (SSPL), a mature build system (`moon`), a package registry (mooncakes.io), and IDE integration. This document distills actionable takeaways for Almide.

---

## 1. WASM-First vs. Rust-First: The Binary Size Gap

**MoonBit's approach**: Compiles directly to wasm-gc bytecode. A Fibonacci program produces 253 bytes of WASM. Their HTTP hello-world component model example is 27 kB. They achieved this by designing the language around WASM primitives from day one, with no runtime to strip.

**Almide's approach**: Emits Rust source, then delegates to `rustc --target wasm32-wasip1`. A minimal Almide WASM binary inherits Rust's allocator, panic handler, and stdlib shims. Even with `opt-level=s` and `wasm-opt`, the floor is several hundred kB.

**Recommendation for PRODUCTION_READY.md**: Almide should not try to compete on raw WASM binary size -- that is a fight Almide cannot win without a direct WASM backend. Instead, Almide's WASM story should lean into its strengths: (a) full WASI access via Rust's mature ecosystem, (b) production-grade optimized binaries from `rustc`, (c) the ability to link Rust crates. Document the intended WASM use cases (serverless, CLI tools, plugins) where binary size matters less than correctness and ecosystem access. Consider adding a `--target wasm-component` flag that generates Rust using the `wit-bindgen` crate for WASM Component Model interop -- this is where MoonBit is investing, and Almide could match it by leveraging existing Rust tooling.

---

## 2. Constrained Sampler: Compile-Time Validation During LLM Generation

**MoonBit's approach**: They built two custom sampling algorithms -- "local sampling" (enforces syntactic correctness token-by-token) and "global sampling" (verifies semantic/type correctness). A speculation buffer stores the last token and backtracks on validation failure, informing the LLM of valid continuations. This achieves significantly higher compilation rates with only ~3% inference overhead. Published at ICSE 2024's LLM4Code workshop.

**Almide's approach**: Almide's LLM story is "design the language so LLMs naturally write correct code" -- flat scopes, mandatory top-level types, actionable diagnostics for auto-repair. But there is no constrained decoding integration.

**Recommendation for PRODUCTION_READY.md**: Almide should build a grammar-constrained sampler or at minimum a fast `almide check --stdin` mode that returns structured JSON errors in <100ms. This enables IDE copilot integrations and agentic coding loops. The existing diagnostic system (file:line + actionable hint) is already designed for LLM auto-repair; the missing piece is latency. Consider: (a) a `--check-only` mode that skips lowering and codegen (parser + checker only), (b) JSON-structured error output for machine consumption, (c) an `almide lsp` that supports `textDocument/diagnostic` for real-time feedback during generation.

---

## 3. Expect/Snapshot Testing Built Into the Language

**MoonBit's approach**: `inspect(expr, content="expected")` is a first-class test primitive. Running `moon test --update` auto-fills or updates the `content=` parameter with the actual output. Three snapshot modes: Show-based, JSON-based, and block-level. Tests live inline (discarded in non-test compilation).

**Almide's approach**: `test "name" { assert_eq(a, b) }` blocks with `almide test`. No snapshot testing, no auto-update, no `inspect`-style assertion.

**Recommendation for PRODUCTION_READY.md**: Add `assert_snapshot` to the test stdlib. Semantics: `assert_snapshot(expr)` on first run writes the stringified result to a `__snapshots__/` sidecar file; on subsequent runs compares against it. `almide test --update` refreshes snapshots. This is especially valuable for LLM-generated code: the agent writes `assert_snapshot(result)`, runs once to capture, then the snapshot acts as a regression guard. Implementation: the Rust emitter already handles `test` blocks specially; extend the test harness to support snapshot file I/O.

---

## 4. Integrated Build System and Package Manager

**MoonBit's approach**: `moon` is a single binary providing build, test, fmt, doc, coverage, bench, publish, install, and dependency management. Package registry at mooncakes.io. `moon.pkg.json` defines packages. No external build tool needed.

**Almide's approach**: `almide` CLI provides run, build, test, check, fmt, clean, init. Dependencies use `almide.toml` with git-based resolution. No package registry. No `doc` or `coverage` or `bench` commands.

**Recommendation for PRODUCTION_READY.md**: The priority order should be: (1) `almide doc` -- generate HTML/Markdown docs from type signatures and `///` comments. MoonBit proved that doc generation bootstraps community adoption. (2) `almide coverage` -- instrument test blocks to report line coverage. (3) A package registry is premature until the language stabilizes, but the `almide.toml` + git model is fine for now. What matters more is that `almide init` generates a complete project scaffold with `almide.toml`, `src/main.almd`, and a `spec/` directory with a starter test.

---

## 5. Structural Trait Implementation

**MoonBit's approach**: Traits are structural -- if a type has the required methods, it implements the trait automatically. No `impl Trait for Type` ceremony needed (though explicit `impl` blocks are also supported). `derive(Show, Eq, Hash, ToJson, FromJson, Default)` auto-generates implementations.

**Almide's approach**: Almide has `trait` and `impl` declarations. Auto-derive is supported for Eq, Hash, and Codec (ToJson/FromJson). The trait system is nominal, not structural.

**Recommendation for PRODUCTION_READY.md**: Structural traits are appealing for LLM code generation because they reduce boilerplate the model must produce. However, they make error messages harder ("why does this type satisfy this trait?"). Almide should keep nominal traits but expand auto-derive coverage: add `derive(Show)` (for debug printing and snapshot tests), `derive(Ord)` (for sorted collections), and `derive(Default)`. These three are the most common boilerplate in exercises and real programs. MoonBit's lesson here is not "switch to structural" but "minimize the ceremony for common protocols."

---

## 6. Error Handling: raise/catch vs. effect fn

**MoonBit's approach**: Functions declare error capability via `T!E` return types. `raise` throws, `try/catch` handles. `f!()` propagates (like Rust's `?`), `f?()` converts to `Result`. Recent updates removed the need to mark effectful call sites with `!` -- the IDE uses semantic highlighting instead. No algebraic effect system; errors are the primary "effect."

**Almide's approach**: `effect fn` marks I/O functions. Rust emitter wraps returns in `Result<T, String>` with auto-`?`. TS emitter erases Result entirely (`ok(x)` -> `x`, `err(e)` -> `throw`). The effect system is binary: a function either is effectful or not.

**Recommendation for PRODUCTION_READY.md**: MoonBit's typed error approach (`T!DivisionByZero`) is more granular than Almide's `Result<T, String>`. For production readiness, consider: (a) allow `effect fn` to declare specific error types: `effect fn read_file(path: String) -> String ! IoError`, (b) the Rust emitter maps this to `Result<String, IoError>` instead of `Result<String, String>`, enabling downstream pattern matching on error kinds. This is a significant upgrade path. Near-term, the current `String` error model is fine, but the PRODUCTION_READY.md should acknowledge typed errors as a future milestone.

---

## 7. Compilation Speed as a Feature

**MoonBit's approach**: 626 packages in 1.06 seconds. Function-level parallel semantic analysis. Incremental reanalysis for IDE responsiveness. The compiler itself is bootstrapped in MoonBit.

**Almide's approach**: The Almide compiler is fast (pure Rust, ~20k lines), but the two-stage pipeline (Almide -> Rust -> rustc) means `almide run` is bottlenecked by `rustc`. Even with `opt-level=1`, `rustc` dominates wall-clock time for iterative development.

**Recommendation for PRODUCTION_READY.md**: Two concrete actions: (1) Add a `--cached` flag to `almide run` that skips `rustc` if the generated `.rs` file is unchanged (content-hash based). This is free for repeated runs during debugging. (2) Longer-term, consider an interpreter mode (`almide eval`) for quick scripting and REPL-like usage, bypassing `rustc` entirely. MoonBit's speed advantage comes from being a single-stage compiler; Almide cannot match that, but caching eliminates the penalty for the common case.

---

## 8. AI Code Agent as a First-Party Product

**MoonBit's approach**: "MoonBit Pilot" is a code agent integrated into the toolchain -- it generates libraries with docs and tests, handles refactoring, and uses the compiler's type checker as a feedback loop. The agent is not a third-party plugin; it is part of the product.

**Almide's approach**: Almide is designed for LLM accuracy (flat scopes, mandatory types, actionable errors) but has no first-party agent. LLM integration is implicit: "write Almide, and GPT/Claude will get it right more often."

**Recommendation for PRODUCTION_READY.md**: Almide does not need to build its own agent product, but it should provide the infrastructure that makes agents effective: (a) `almide check --json` for structured error output that agents can parse, (b) `almide test --json` for structured test results, (c) a documented "agent loop" pattern: generate -> check -> read errors -> fix -> re-check. The CLAUDE.md already describes this workflow informally; formalize it as a supported use case with stable JSON schemas for errors and test results.

---

## Summary Table

| Area | MoonBit | Almide | Priority |
|------|---------|--------|----------|
| WASM binary size | 253 bytes (wasm-gc) | ~100+ kB (via rustc) | Low -- different tradeoffs |
| Constrained LLM sampler | Built-in, ~3% overhead | None (relies on language design) | Medium -- add fast check mode |
| Snapshot testing | `inspect` + `--update` | Not yet | High -- easy win |
| Build system completeness | build/test/fmt/doc/coverage/bench/publish | build/test/fmt/check/clean/init | Medium -- add doc, coverage |
| Trait ceremony | Structural + derive | Nominal + limited derive | Medium -- expand derive |
| Error typing | `T!ErrorType` | `Result<T, String>` | Low -- future milestone |
| Compilation caching | Single-stage, inherently fast | Two-stage, rustc bottleneck | High -- add content-hash cache |
| Agent integration | MoonBit Pilot (first-party) | Implicit (language design) | Medium -- add JSON output modes |

---

## Sources

- [MoonBit Official Site](https://www.moonbitlang.com/)
- [MoonBit: Exploring the Design of an AI-Native Language Toolchain](https://www.moonbitlang.com/blog/moonbit-ai)
- [The Future of Programming Languages in the Era of LLM](https://www.moonbitlang.com/blog/ai-coding)
- [MoonBit First Announcement](https://www.moonbitlang.com/blog/first-announce)
- [MoonBit Core Library (GitHub)](https://github.com/moonbitlang/core)
- [MoonBit Build System (GitHub)](https://github.com/moonbitlang/moon)
- [MoonBit Error Handling Docs](https://docs.moonbitlang.com/en/latest/language/error-handling.html)
- [MoonBit WASM Component Model](https://www.moonbitlang.com/blog/component-model)
- [MoonBit Expect Testing](https://www.moonbitlang.com/blog/expect-testing)
- [MoonBit Compiler Open Source](https://www.moonbitlang.com/blog/compiler-opensource)
- [Designing for AI and Humans (Deep Engineering interview)](https://medium.com/deep-engineering/deep-engineering-3-designing-for-ai-and-humans-with-moonbit-core-contributor-zihang-ye-1145dfe1692d)
- [MoonBit: Explore the Design of an AI-Friendly Programming Language (ICSE 2024 / LLM4Code)](https://dl.acm.org/doi/10.1145/3643795.3648376)
- [MoonBit Pilot Introduction](https://www.moonbitlang.com/blog/intro-moonbit-pilot)
- [MoonBit: Wasm-Optimized Language Creates Less Code Than Rust (The New Stack)](https://thenewstack.io/moonbit-wasm-optimized-language-creates-less-code-than-rust/)
