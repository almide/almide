<a id="adr-0006-one-bit-fallibility-polymorphism-for-hofs-the-try_-family-is-dissolved"></a>

# ADR-0006: One-bit fallibility polymorphism for HOFs; the try_* family is dissolved

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](0006-fallibility-polymorphic-hofs.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0006-fallibility-polymorphic-hofs.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0006-fallibility-polymorphic-hofs.zh-CN.md)

> Non-normative pinned reader snapshot. The original remains authoritative; this copy is not automatically synchronized and has not independently revalidated implementations, measurements, or safety.
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0006-fallibility-polymorphic-hofs.md)
> This historical accepted ADR includes later amendments. The 2026-09-27 amendment replaces fixed-String E and withdraws D4; the 2026-10-01 amendment expands fallible HOF coverage with stated TOTAL_ONLY exceptions. Earlier freeze/removal plans, usage measurements and MSR expectations remain historical claims, not newly executed results or current compatibility guarantees.


- **Status**: Accepted
- **Date**: 2026-08-05
- **Decision scope**: How to expose fallibility when a higher-order function's (HOF's) callback can fail: dedicated `try_*` functions (the current state) or fallibility polymorphism (the callback's `!` flows to the HOF)
- **Related**: [ADR-0002](../../../../adr/0002-fallibility-effect-orthogonal.md) (`-> T!`, E=String; this ADR applies its consequence to HOFs), [ADR-0004](../../../../adr/0004-error-branchability-doctrine.md) (String as the endpoint, supporting D4's intentional omission), [ADR-0005](../../../../adr/0005-operators-desugar-to-stdlib.md) (minimize spellings), #1103 (Phase 1), #1055 (capability axis; this ADR extracts a subset first)
- **Background**: On 2026-08-05, investigated the discomfort “I dislike the `try_` prefix.” Research found that the try_* family was temporary scaffolding filling the absence of a language feature equivalent to rethrows. After language comparisons and measured usage, decided to dismantle it.

<a id="context"></a>

## Context

The `list.try_*` family shipped in v0.53.6 (7 functions: `try_map` / `try_filter` / `try_flat_map` / `try_filter_map` / `try_fold` / `try_find` / `try_each`) consists of dedicated monomorphic functions: callback returns Result, stop at the first err, preserve E.

Three problems:

1. **`try` is borrowed vocabulary absent from Almide**, with meanings differing among its lenders (Rust `try_fold` = short-circuiting; Swift/Zig `try` = propagation = Almide's `!`). LLMs exposed to Swift/Zig may misread `try_map`.
2. **The surface grows multiplicatively as strategy × HOF**: every new HOF raises the question of whether it needs a try_ sister.
3. Almide lists are eager, so Rust-style lazy composition (`map(f).collect::<Result<_,_>>()`, where collect stops at the first err) is **unavailable in principle**. try_map arose as a fused function under this constraint.

<a id="他言語比較一次情報"></a>

### Comparison with other languages (primary sources)

| Language | Fallible map | Mechanism |
|---|---|---|
| Swift 6 (SE-0413) | `try xs.map(f)` — only one map | **Fallibility polymorphism**: `map<U, E: Error>(body: (Element) throws(E) -> U) throws(E) -> [U]`. A total callback infers E=Never, making map total too |
| Rust | `map(f).collect::<Result<_,_>>()` | Lazy iterator + FromIterator (“returns the first Err,” explicit in std docs). The `try_fold` family also exists because closures cannot be `?`-polymorphic |
| Zig | Loop + `try f(x)` | Error sets are inferred transparently through calls |
| Haskell / Scala | `traverse` | Strategy resides in the type (Either=short-circuit / Validation=collect all) |
| Gleam | `list.try_map` | **Dedicated monomorphic function — a compromise without type classes or effects** (same as Almide's current state) |
| OCaml 5 / Koka | `List.map` unchanged | The effect system is transparent |

Only Gleam chose P1 (strategy in the name), a compromise caused by missing tools. Almide has decided in ADR-0002 to build Swift's throws model—the tool.

<a id="使用量実測2026-08-05"></a>

### Measured usage (2026-08-05)

Excluding the family's own definitions, tests and docs, real usage is **one line in the entire repository**: `tools/almide-gates/src/main.almd:46` (through an effect fn, E=String). The `try_fold` occurrences in compiler `.rs` files are Rust's own `Iterator::try_fold` and unrelated. E-generic (non-String) usage is **zero**.

<a id="decision"></a>

## Decision

**HOF fallibility flows from the callback type (one-bit fallibility polymorphism). Freeze the `try_*` family immediately, deprecate it when polymorphic HOFs land, and remove it in the next minor.**

<a id="d1-1-ビット可謬性多相"></a>

### D1. One-bit fallibility polymorphism

A function-type parameter may have `!` (`(A) -> B!`). A generic HOF becomes fallible when its callback is fallible. One rule: **“if the callback is `!`, the HOF is `!`.”** E is fixed to String under ADR-0002 D2 (one step simpler than Swift's generic E). Short-circuiting follows naturally from propagation, requiring no dedicated implementation:

```almide
nums  |> list.map((n) => n * 2)              // 総 callback → List[Int]
files |> list.map((f) => read_meta(f)!)!     // 可謬 callback → List[Meta]!(first-err 打ち切り)
```

<a id="d2-try_-family-は即時凍結"></a>

### D2. Freeze the try_* family immediately

Add no new siblings. Repurpose the existing family gate (`tests/list_try_family_gate_test.rs`) as the guardian of “fixed at 7.” CLAUDE.md's recommended try_map idiom **remains valid until polymorphic HOFs land** (removing it first would revive var+for; removal order matters).

<a id="d3-着地時に-deprecated--次-minor-で削除"></a>

### D3. Deprecate on landing → remove in the next minor

The deprecation hint gives the mechanical rewrite directly:

```
list.try_map(xs, f)      →  list.map(xs, (x) => f(x)!)!
list.try_filter(xs, p)   →  list.filter(xs, (x) => p(x)!)!
list.try_fold(xs, z, f)  →  list.fold(xs, z, (a, x) => f(a, x)!)!
// find / each / flat_map / filter_map も同型
```

Measured migration cost: one line.

<a id="d4-e-ジェネリックな-traverse-は意図的に非サポート"></a>

### D4. E-generic traverse is intentionally unsupported

Write short-circuit traversal with custom E using explicit recursion (or conversion to String at the boundary). This is an intentional omission consistent with ADR-0004 (String is the endpoint; variant E remains within a custom domain). Reconsider under Falsifier 3 if demand is measured.

<a id="rationale"></a>

## Rationale

- **A consequence of ADR-0002**: having made fallibility an axis of fn attributes, reflecting it in fn **types** (parameter position) completes the axis rather than adding a concept.
- **Stops surface proliferation**: removes the strategy × HOF multiplication; each future HOF has one name.
- **MSR**: writers need not remember which try_ sisters exist; write the usual `map` and place `!`. Check identifies too much or too little fallibility, with hints enabling self-repair.
- **Cheapest now**: one line of usage, first release shipped. Dismantling gets more expensive as adoption spreads.

<a id="alternatives--検討して却下した案"></a>

## Alternatives — considered and rejected

1. **Rename only** (`traverse` / `map_try`, etc.): if D1 later dissolves the family, this means **renaming twice**. **Rejected** as the worst move.
2. **Keep try_* permanently** (Gleam-style): a language committed to building the tool has no reason to keep a compromise caused by its absence. **Rejected**.
3. **Wait for full #1055 (effect polymorphism) and do everything together**: one-bit fallibility polymorphism is independently small—fixed E, Boolean propagation only. No reason to hold it hostage. **Rejected** (extract it first).
4. **Introduce lazy iterators for Rust-style composition** (P3): changes the entire collection model, with orders-of-magnitude wider impact. **Rejected**.

<a id="consequences"></a>

## Consequences

- Phase 2 implementation (#1108): `!` on fn types, lambda fallibility inference (`(x) => f(x)!` is `(A) -> B!`), total/fallible two-state monomorphization of generic HOFs, native ⇄ wasm equivalence.
- Double-`!` spelling `((x) => f(x)!)!` becomes standard. It interacts with B-track design (auto-`?` positions), so cross-reference during Phase 2 design is essential.
- try_* dies in 3 stages (freeze → deprecate → remove), each in release notes.

<a id="falsifier"></a>

## Falsifier

1. **One-bit propagation inference becomes unpredictable** (annotation presence changes behavior, or other B-track irregularities cannot be resolved in Phase 2): withdraw D1 and keep try_* permanently.
2. **Dojo A/B finds `((x) => f(x)!)!` significantly worsens MSR versus try_map**: stop dismantling; retain polymorphic HOFs alongside try_* (try_* as sugar, an ADR-0005-style definitional relationship).
3. **Measured custom-E traverse demand** (≥3 real usage sites): consider E-generic polymorphism (Swift's full form) in a new ADR.

<a id="references"></a>

## References

- Swift Evolution SE-0413 — [Typed throws](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md) (before/after for `map`'s rethrows → `throws(E)` generalization already retrieved as quotations)
- Rust — [std::result: Collecting into Result](https://doc.rust-lang.org/std/result/index.html) (quotation “returns the first Err” retrieved), [Iterator::try_fold](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.try_fold)
- Zig — [Errors](https://ziglang.org/documentation/master/#Errors)
- Gleam — [gleam/list.try_map](https://hexdocs.pm/gleam_stdlib/gleam/list.html#try_map)
- Internal: ADR-0002 / 0004 / 0005, measured usage (above), #1103 (Phase 1), `tests/list_try_family_gate_test.rs` (freeze guardian)

<a id="amendment-2026-09-27-e-は-callback-の失敗チャネルから流れる--adr-0021"></a>

## Amendment (2026-09-27): E flows from the callback's failure channel — ADR-0021

D1's “E is fixed to String under ADR-0002 D2” is amended by [ADR-0021](../../../../adr/0021-lambda-failure-channel-is-the-join-of-its-bangs.md). The HOF's E flows from the callback failure channel ε (expected type → join of `!` operands → default String). Withdraw D4 (intentional lack of E-generic traverse). `__fallible_*` carriers already operated E-generically on the canonical-form route. Falsifier 3 (≥3 real demand) was triggered by 3 human-written baselines in the Dojo bank and 37 attempts of bank-pilot output (#2601).

<a id="amendment-2026-10-01-d1-は全-hof-に及ぶ--3163"></a>

## Amendment (2026-10-01): D1 applies to all HOFs — #3163

In implementation, D1's “callback `!` means HOF `!`” rule had applied only to 7 list functions (map / filter / flat_map / filter_map / fold / find / each). Meanwhile E005's hint said “core list HOFs accept fallible callbacks,” rejecting `list.all(xs, (x) => p(x)!)` beneath a hint saying it was accepted (#3163). Withdraw the former “intentional omissions” (any/all/count belong to the find-shaped domain; fallible keys have no ordering for sort_by): predicate queries stop at the same short-circuit points as total forms; key-extraction functions compute each key once in element order before sorting, defining the first err.

All callback-taking functions in list / map / set / option now have fallible forms. The single table is `almide_types::fallible_hofs`, read by checker rewriting, the dead-carrier guard, hints and interp's HOF allowlist. Result HOFs and `option.collect_map` are excluded with reasons as TOTAL_ONLY (result's container itself is a failure channel). `tests/list_fallible_family_gate_test.rs` checks completeness as an executable matrix.
