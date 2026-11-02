# ADR-0010: T? is general Option shorthand — every type position, atom-tight binding, fmt-normalized

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](0010-option-type-shorthand.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0010-option-type-shorthand.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0010-option-type-shorthand.zh-CN.md)

> Non-normative reader copy of the pinned historical decision, not a new authority or automatically synchronized document. Historical measurements and implementation claims have not been independently revalidated. The source calls `Option[T]` “19 characters”; this apparent counting inconsistency is preserved, not corrected. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0010-option-type-shorthand.md)

- **Status**: Accepted
- **Date**: 2026-08-06
- **Decision scope**: adoption, valid positions, binding rules and canonical form (fmt normalization direction) of the type notation `T?`.
  The expression operators `?` (Result→Option)/`??`/`?.` are out of scope (already settled in ADR-0005).
- **Related**: [ADR-0002](../../../../adr/0002-fallibility-effect-orthogonal.md) (the precedent for `-> T!`; its Alternatives 5 separated `T?` as out of scope, leaving adoption for another discussion),
  [ADR-0009](../../../../adr/0009-fn-type-quadrant-transparency.md) (`!` in fn-type slots — align the binding direction).
- **Background**: settled on 2026-08-06, immediately after v0.54.0, through three yes/no decisions (Q1 scope / Q2 binding / Q3 normalization).

## Context

With `-> T!` (ADR-0002), fallibility became one character, but absence (Option) remained the 19-character `Option[T]`. Corpus measurements (2026-08-06):

| Position of Option annotation | Count |
|---|---|
| Return position `-> Option[T]` | 387 |
| Parameter/field position `: Option[T]` | 331 |
| Generic argument position `[Option[T]]` | 147 |
| Nested `Option[Option[T]]` | 45 |
| Return of a fn-type slot `(A) -> Option[B]` | 52 |
| Wrapping the fn type itself `Option[(A) -> B]` | **0** |

`!` means “fallibility of calling a function”: an attribute of the arrow, so restricting it to return positions is correct.
`?` means “absence of a value”: an attribute of a type, and measurements place a majority (478/865) outside return positions.

## Decision

**D1 (Q1 yes): adopt `T?` ≡ `Option[T]` as general type sugar valid in every type position.**
Do not restrict it to return positions: position-dependent notation would repeat the surface-rule violation eliminated in v0.51.0 (a rule generalized by the writer stops working depending on location). Fix the denotation rule to one line: `T?` and `Option[T]` have identical denotations (like `16` and `0x10`).

**D2 (Q2 yes): `?` binds most tightly to the immediately preceding type atom and does not cross `->`.**
A type atom is a name (possibly generic or module-qualified) or a parenthesized type.
Explicit parentheses mark an entire fn type or nested Option:

```almide
f: (Int) -> Int?            // = (Int) -> Option[Int]   実在 52 件の形
on_tick: ((Int) -> Unit)?   // = Option[(Int) -> Unit]  実在 0 件の形に括弧税
pair: (String, Int)?        // = Option[(String, Int)]  タプルは括弧済みアトム
nested: (Int?)?             // = Option[Option[Int]]    `Int??` は ?? にレクスされるため不可
fn parse_opt(s: String) -> Int?!   // = Result[Option[Int], String]  ? が先、! は戻りマーカー
```

**D3 (Q3 yes): fmt normalizes `Option[T]` → `T?` in every type position.**
The short form is canonical. Mixed notation directly affects LLM writability (corpus consistency = generation consistency), so collapse it to one form along the one-name-one-meaning direction of v0.53.5. Print parentheses around nested, fn-type and record-type interiors as needed for reparsing.

**D4 (updated 2026-08-07: permanently rejected)**: fmt normalization of `Result[T, String] → T!` will **never be performed** — moved from pending proof to rejection. Observed evidence: under the 1-bit rule in #1108, the callback bit resides on the **declaration marker**, so even with the same body, the meaning of `list.map(xs, f)` depends on how f is written (long form = map-of-results `[ok(1), err(…)]` / `T!` = stop at first err). Mechanical replacement would turn every map-of-results into first-err behavior. This distinction is specified and pinned by “the marker, not the type, carries the callback bit” in spec/lang/fallible_hof_test.almd. Thus, from 0.55 onward, these are not “one type, two notations” but **two declarations through which the writer chooses a strategy**. T? has no HOF bit, so the analogous problem does not arise; the soundness of T? normalization is unchanged.

## Rationale

- The short form covers the full observed distribution (return 387 / parameter 331 / generic 147), while only the form observed zero times pays the parentheses cost.
- `!` (ADR-0009: fallibility belongs to the arrow's return) and `?` align their binding direction.
- Implementation is structurally identical to ADR-0002 Phase 1a: pseudo-generic `Generic { name: "?" }` → `Ty::option(T)` in the resolver; fmt reprints the surface spelling. Downstream (checker/IR/codegen/mir) sees only Option — no cross-target behavior change.

<a id="alternatives--検討して却下した案"></a>

## Alternatives — considered and rejected

1. **Start with return positions only** (matching `T!`): introduces a position-dependent rule. It excludes 478 observed cases and requires teaching “allowed after `->`, but not after `:`.” **Rejected**.
2. **Attach `?` to the entire fn type** (`(A) -> B?` = Option[fn]): reverses the cost distribution, leaving the zero-occurrence form without parentheses and charging parentheses to 52 actual occurrences. **Rejected**.
3. **No fmt normalization** (both spellings coexist): no migration diff, but the mixture persists and splits the model's corpus between two forms. **Rejected**.
4. **Special lexing for `T??`** (split `??` in type contexts): makes the lexer context-dependent. Nested Option has 45 observed occurrences, and parentheses `(T?)?` suffice. **Rejected**.

## Consequences

**Gains**: 1,044 Option annotations in 224 files shrink mechanically. The symbol family `?` (absence)/`!` (fallibility)/`?.`/`??` aligns the same semantic axes across types and expressions.

**Costs**:
- Additional grammar surface (postfix `?` on type atoms) and its teaching cost.
- A one-time normalization diff (spec/ + examples/, the fmt gate's scope).
- **stdlib migrated too (added 2026-08-07)**: `almide fmt --no-import-edit` resolved the splice-context hazard (automatic import insertion) that had justified leaving it unchanged. All 258 stdlib files were batch-normalized, with no added or removed import lines, mechanically checked. The old rationale — retaining a long-spelling boundary witness — transfers to the spec's unification tests (fallible_marker / option_marker).

## Falsifier

- If Dojo MSR measurements show significant observed binding mistakes for `?` in fn-type slots or nesting, reconsider D2's parentheses rule.
- If handwritten reversion to `Option[` spelling dominates after normalization, reconsider D3's direction.

## References

- Decision log: 2026-08-06 session (Q1/Q2/Q3 all yes).
- Measurements: the table above (corpus counts using rg, 2026-08-06).
