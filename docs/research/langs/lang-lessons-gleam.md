# Lessons from Gleam for Almide's Production Readiness

Research date: 2026-03-17

Gleam is a statically typed functional language that compiles to both Erlang (BEAM) and JavaScript. Created by Louis Pilfold, it went through 34 pre-release versions over ~5 years before reaching 1.0 in March 2024. The compiler is written in Rust (95% of the codebase), has 21.3k GitHub stars, 289 contributors, and the stdlib has 147 contributors across 96 releases. This document distills actionable takeaways for Almide.

---

## 1. The 1.0 Was Radically Small -- And That Was the Point

**Gleam's approach**: Gleam 1.0 shipped with a deliberately minimal surface area. The language is described as learnable "in an afternoon." The 1.0 committed to stability for: the language design, compiler, build tool, package manager, code formatter, language server, and WASM compiler API. The standard library got its own v1 release shortly after. The stated philosophy: "reading and debugging code is more difficult than writing new code," so the language optimizes for reading.

What Gleam explicitly does NOT have: no exceptions, no null, no macros, no type classes/traits, no mutation, no OOP, no variadic functions, no operator overloading. Post-1.0, the team pledged to be "extremely conservative" about additions -- any new feature must be "generally useful and enable new things not otherwise possible in Gleam."

**Comparison to Almide**: Almide is already larger than Gleam 1.0: it has `trait`/`impl`, mutable variables (`var`), `effect fn`, string interpolation, `do` blocks, operator overloading via protocols, and a 22-module stdlib with 282 functions. This is not inherently bad -- Almide's mission (LLM accuracy) requires some features Gleam can omit. But it means Almide cannot claim "learn in an afternoon" simplicity.

**Recommendation**: Define Almide's 1.0 scope by listing what will NOT be in it. Gleam's power move was the exclusion list, not the feature list. For Almide, consider freezing these as "not in 1.0": user-defined generic functions, structural traits, algebraic effects, async/await, macros. Document the exclusion list in PRODUCTION_READY.md with rationale for each. A clear boundary prevents scope creep and gives users confidence the surface area is stable.

---

## 2. Multi-Target: Share the Language, Not the Runtime

**Gleam's approach**: Gleam compiles to Erlang and JavaScript from the same source. But the targets diverge on concurrency: Erlang uses the BEAM actor model with automatic preemption; JavaScript uses promise-based concurrency. Gleam does NOT try to make JavaScript behave like Erlang. The JS output is "human readable and pretty printed" and can be called from plain JavaScript/TypeScript. Target-specific code uses `@external` FFI declarations. The stdlib ships as a Hex package that works on both targets.

The JS backend arrived in v0.16 (June 2021), roughly 2 years into the project -- well before 1.0. Key challenge: JavaScript's Promise flattening broke sound typing. Gleam accepted the limitation rather than adding special-case type system rules.

**Comparison to Almide**: Almide compiles to Rust and TypeScript/JavaScript. Like Gleam, the targets diverge on semantics: Rust uses `Result<T, String>` with `?` propagation; TS erases Result entirely (`ok(x)` -> `x`, `err(e)` -> `throw`). Almide's approach is actually more aggressive than Gleam's -- the `effect fn` abstraction completely changes codegen behavior per target. Almide also emits Rust source that goes through `rustc`, adding a second compilation stage that Gleam avoids.

**Recommendation**: Gleam's lesson is: accept target divergence honestly rather than papering over it. Almide already does this well with the `effect fn` / Result erasure split. But the generated code quality matters for adoption. Gleam made JS output "human readable" as a feature. Almide should ensure generated Rust code is readable and auditable -- developers who choose Almide for Rust output will want to inspect and understand what they ship. Consider adding a `--emit-rust --fmt` flag that runs `rustfmt` on the output for inspection purposes. Similarly, TS output should be clean enough to commit to a repository if needed.

---

## 3. Type System: No Null, No Exceptions, Exhaustive Matching -- Then Stop

**Gleam's approach**: Gleam's type system is Hindley-Milner with full inference -- type annotations on function arguments are optional (though conventional). Custom types are algebraic (variants with associated data). Pattern matching is exhaustive. There is no null -- `Option` is a custom type. There are no type classes or traits. Generics are parametric. The team explicitly rejected type classes because they "enable creation of very nice APIs" but create "challenging-to-understand code, confusing error messages, and interoperability issues."

**Comparison to Almide**: Almide has algebraic types (record + variant), exhaustive pattern matching, no null (`Option` via `Some`/`None`), and Result types. But Almide also has traits and protocols (Eq, Hash, Codec, operator overloading), which Gleam deliberately excluded. Almide requires type annotations on function parameters (mandatory for LLM accuracy), while Gleam infers them. User-defined generic functions are not yet supported in Almide -- matching Gleam's conservatism here.

**Recommendation**: Almide's mandatory type annotations are a strength for the LLM use case -- do not weaken this to add inference. Gleam validated that a language can thrive without type classes; Almide's traits serve a different purpose (LLM-friendly derive for Eq/Hash/Codec). The key lesson: keep the type system predictable. Every feature that makes types harder to read (higher-kinded types, associated types, trait specialization) should be rejected for 1.0. Almide's traits should remain simple -- nominal, single-dispatch, with auto-derive as the primary usage pattern. Do not add trait bounds on generic type parameters until there is overwhelming demand.

---

## 4. Effects: Gleam Chose Impurity, and It Worked

**Gleam's approach**: Gleam is an impure functional language. Any function can perform I/O -- there is no `IO` monad, no `effect` keyword, no purity tracking in the type system. The FAQ states the compiler does "limited effects tracking internally" that could expand, but as of 1.0+, side effects are simply allowed everywhere. This is a deliberate rejection of Haskell's approach.

Gleam's `use` expression (added in v0.25) handles the callback/cleanup pattern that effect systems often address. `use file <- with_file("data.txt")` desugars the rest of the block into a callback. This is syntactic sugar, not an effect system -- it works for any function that takes a callback, including Result handling, resource management, and iteration.

**Comparison to Almide**: Almide's `effect fn` is a middle ground between Gleam's impurity and Haskell's purity. It marks I/O functions and auto-propagates errors in Rust. This is more structured than Gleam but simpler than algebraic effects. The TS emitter erases the distinction entirely.

**Recommendation**: Gleam proved that a simple, impure approach can succeed. Almide's `effect fn` adds value because it serves the Rust target (where `Result` propagation is real and necessary), not because purity tracking is essential. The lesson: do not expand `effect fn` into a general effect system. Keep it as "marks functions that can fail with I/O errors" -- which is exactly what it is. If developers want more granular error types (per the MoonBit research), that can be layered on `effect fn` later without changing the language's fundamental character. Do NOT add algebraic effects, effect handlers, or effect polymorphism.

---

## 5. Package Manager and Ecosystem: Piggyback on an Existing Registry

**Gleam's approach**: Gleam publishes packages to Hex, the Erlang/Elixir package registry. The stdlib itself is a Hex package. This gave Gleam instant access to an established registry infrastructure (search, versioning, docs hosting on HexDocs) without building anything from scratch. It also enabled interop: Gleam packages can depend on Erlang/Elixir packages and vice versa. The build tool and package manager were bundled into the main `gleam` binary from early on.

**Comparison to Almide**: Almide uses `almide.toml` with git-based dependency resolution. There is no package registry. The stdlib is embedded in the compiler, not a separate package.

**Recommendation**: Almide's Rust target could, in theory, publish to crates.io -- but the generated code is not idiomatic Rust, so this is a poor fit. The TS target could publish to npm, but same issue. The real lesson from Gleam is: the package manager and build tool must be part of the core binary, and they must work before 1.0. Almide already has `almide init` and `almide.toml`. The gap is discoverability: there is no `almide search` or package index. For 1.0, the recommendation is NOT to build a registry (premature for a pre-1.0 language). Instead: (a) ensure `almide.toml` git dependencies work flawlessly with version pinning, (b) add `almide deps` to list/update dependencies, (c) create a curated "awesome-almide" list on GitHub as the discovery mechanism. A registry can come post-1.0 when there are enough packages to justify it.

---

## 6. Error Messages: Invest Disproportionately Early

**Gleam's approach**: The project stated that "excellent developer tooling is just as much a core concern of the language as a fast & reliable compiler, and ergonomic & productive language design." The language server shipped in v0.21 (April 2022, ~2 years before 1.0) with hover types, go-to-definition, formatting, and real-time diagnostics. Error messages are famously friendly -- Gleam is often compared to Elm in this regard. Fault-tolerant compilation was added pre-1.0, allowing the language server to provide diagnostics even in partially invalid code.

**Comparison to Almide**: Almide already has strong diagnostic design: every error includes what/where/how-to-fix, specifically designed for LLM auto-repair. The compiler has context-aware error recovery hints (7 files in `parser/hints/`). There is no language server yet.

**Recommendation**: Almide's error messages are already well-designed for the LLM use case. Two gaps relative to Gleam: (1) **Language server**: Gleam shipped LSP 2 years before 1.0. For Almide, a basic LSP providing diagnostics-on-save + hover-for-type would dramatically improve the human developer experience. The compiler already has all the information; the missing piece is the LSP protocol wrapper. This should be on the 1.0 roadmap. (2) **Fault-tolerant parsing**: Gleam's parser continues after errors to report multiple issues per compilation. If Almide's parser bails on the first error, adding recovery (skip to next declaration on error) would improve both human and LLM workflows -- an LLM that sees 5 errors at once fixes them in one pass instead of 5 round-trips.

---

## 7. Stdlib: 19 Modules Is Enough

**Gleam's approach**: The Gleam stdlib at 1.0 contained 19 modules: bit_array, bool, bytes_tree, dict, dynamic, dynamic/decode, float, function, int, io, list, option, order, pair, result, set, string, string_tree, uri. No filesystem, no HTTP, no JSON, no regex, no crypto, no datetime. These are all in separate community packages (e.g., `gleam_http`, `gleam_json`). The stdlib focused exclusively on data structures and core types that every program needs.

Gleam's stdlib has 147 contributors across 96 releases, showing active community involvement even in the core library.

**Comparison to Almide**: Almide's stdlib has 22 modules and 282 functions, including fs, http, json, regex, crypto, datetime, env, process, log, and uuid. This is significantly larger than Gleam's -- Almide bundles what Gleam delegates to the ecosystem.

**Recommendation**: Almide's embedded stdlib (compiled into the binary via TOML definitions) is a different architectural choice than Gleam's Hex-published stdlib. Almide cannot easily extract modules into separate packages because the codegen is tightly coupled to the compiler. This is fine -- the tradeoff is: larger compiler binary, but zero dependency resolution for common tasks. The lesson from Gleam is not "shrink your stdlib" but "do not let the stdlib block 1.0." If http, crypto, or datetime have rough edges, it is better to ship them as "experimental" (documented, but not stability-guaranteed) than to delay 1.0 to perfect them. Consider a `@experimental` annotation for stdlib modules that are included but not yet stability-locked.

---

## 8. The Bootstrap Problem: Tooling > Marketing

**Gleam's approach**: Gleam grew from zero to 21.3k stars and 289 contributors without corporate backing (the sole full-time developer is ~50% funded by Fly.io sponsorship, rest from individual sponsors). The growth strategy was: (a) ship a complete toolchain (compiler + build + fmt + lsp + package manager) so the first experience is polished, (b) maintain an interactive browser-based language tour at tour.gleam.run, (c) provide cheatsheets for developers coming from Elixir, Elm, Erlang, PHP, Python, and Rust, (d) foster a friendly community (Discord-centric, enforced code of conduct), (e) release consistently (~monthly) with detailed blog posts for each version. Gleam did NOT try to compete on features or performance -- it competed on developer experience and community warmth.

**Comparison to Almide**: Almide has a different growth thesis -- it targets LLM-generated code, not human community-building. The "community" includes AI agents as primary consumers. But human developers still need to understand, debug, and maintain the code LLMs write.

**Recommendation**: Gleam's lesson for Almide is that the first 5 minutes matter more than the feature list. Concrete actions: (a) **Interactive playground**: A web-based "try Almide" page (the WASM target makes this feasible) where users paste code and see Rust/TS output. Gleam's browser tour was a major adoption driver. (b) **"Almide for X developers" cheatsheets**: At minimum, write "Almide for Rust developers" and "Almide for TypeScript developers" -- these are the two communities most likely to adopt Almide. (c) **One-command setup**: `almide init` should produce a project that compiles and passes tests immediately. Gleam's build tool makes `gleam new myapp && cd myapp && gleam run` work in under 10 seconds. (d) **Monthly release blog posts**: Even if the release is small, a blog post with concrete examples shows momentum. Gleam published detailed release notes for all 34 pre-release versions.

---

## Summary Table

| Area | Gleam at 1.0 | Almide (current) | Recommendation |
|------|-------------|-------------------|----------------|
| Language size | Tiny -- no traits, no mutation, no macros | Medium -- traits, var, effect fn, protocols | Define exclusion list for 1.0 |
| Multi-target | Erlang + JS, divergent runtimes | Rust + TS, divergent Result semantics | Accept divergence; ensure readable output |
| Type system | HM inference, no type classes | Mandatory annotations, simple traits | Keep annotations; no HKT/associated types |
| Effects | Impure; `use` for callbacks | `effect fn` for I/O + auto-`?` | Do not expand to algebraic effects |
| Package manager | Hex (piggyback on Erlang ecosystem) | Git-based `almide.toml` | Perfect git deps; registry is post-1.0 |
| Error messages | Elm-quality; LSP from v0.21 | LLM-oriented; no LSP yet | Add basic LSP before 1.0 |
| Stdlib size | 19 modules (data structures only) | 22 modules (incl. fs, http, json, crypto) | Ship everything; mark unstable modules |
| Community bootstrap | Tooling + playground + cheatsheets | LLM-first; no playground yet | Build WASM playground; write cheatsheets |

---

## Sources

- [Gleam v1.0.0 Release Announcement](https://gleam.run/news/gleam-version-1/)
- [Gleam FAQ](https://gleam.run/frequently-asked-questions/)
- [Gleam v0.16: JavaScript Compilation](https://gleam.run/news/v0.16-gleam-compiles-to-javascript/)
- [Gleam v0.21: Language Server](https://gleam.run/news/v0.21-introducing-the-gleam-language-server/)
- [Gleam v0.25: Use Expressions](https://gleam.run/news/v0.25-introducing-use-expressions/)
- [Gleam Standard Library (HexDocs)](https://hexdocs.pm/gleam_stdlib/)
- [Gleam Standard Library (GitHub)](https://github.com/gleam-lang/stdlib)
- [Gleam Compiler (GitHub)](https://github.com/gleam-lang/gleam)
- [Gleam News Archive](https://gleam.run/news/)
- [Gleam Documentation](https://gleam.run/documentation/)
