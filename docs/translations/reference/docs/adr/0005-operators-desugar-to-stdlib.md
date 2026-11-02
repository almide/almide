# ADR-0005: Value-level operators are desugarings of named stdlib functions

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](0005-operators-desugar-to-stdlib.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0005-operators-desugar-to-stdlib.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0005-operators-desugar-to-stdlib.zh-CN.md)

> Non-normative reader copy of a pinned historical decision. The original remains authoritative; this copy is not automatically synchronized. Recorded measurements, comparisons and implementation claims are preserved, not independently revalidated. Code examples and comments are preserved verbatim, including their original language. Implementation status, proposed diagnostic numbering, deprecation, test-equivalence claims and MSR falsifiers are recorded as of this decision, not newly verified current behavior. The strong claims about LLM priors and self-repair are the source’s rationale, not independent measurements here. The doctrine explicitly excludes early-return control-flow operators. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0005-operators-desugar-to-stdlib.md)

- **Status**: Accepted
- **Date**: 2026-08-05
- **Decision scope**: The relationship between value-level operators (`??` / `?` / `?.`) and stdlib combinators: the policy for eliminating duplication, sugar definitions, removal of surplus spellings, and handling of no-ops.
- **Related**: [ADR-0004](../../../../adr/0004-error-branchability-doctrine.md) (the principle “do not add a second spelling for the same thing”—rejection of result.context), REJECTED_PATTERNS.md (the contradictory entry for `??`), the BUG ledger (pipe × `??` miscompile).
- **Background**: On 2026-08-05, four findings from the error-surface matrix were adjudicated with a yes/no decision on one concrete item per question: the triple spelling `??` / `unwrap_or`/`unwrap_or_else`; `??` being implemented while still documented as rejected; `?.` being implemented with no documentation; and the silent identity of `?` on Option.

<a id="context"></a>

## Context

Three spellings for fallback coexisted without a defining relationship:

```almide
port_opt ?? 8080                              // 演算子(lazy)
option.unwrap_or_else(port_opt, () => 8080)   // 関数(lazy)— 意味は ?? と同一
option.unwrap_or(port_opt, 8080)              // 関数(eager)— default が常に評価される
```

Also: REJECTED_PATTERNS still described `??` as rejected (despite implementation and use); `?.` worked and even had a dedicated diagnostic but no documentation (and no diagnostic code assigned); and `o?` (`?` on Option) silently acted as the identity mapping.

Contrasting precedents:

- **Rust / Haskell**: operators are sugar for functions; the functions are the substance (`x?` → `Try::branch`, `a + b` → `Add::add`; in Haskell, `(+)` is a passable value).
- **C# `??` / Kotlin `?:`**: isolated operators without underlying functions.

<a id="decision"></a>

## Decision

**Value-level operators are defined as desugarings to named stdlib functions. The function is the substance and the operator its surface; they are related by definition, not “two spellings.” Remove surplus spellings without a defining relationship. Control-flow operators (`!` and auto-`?`) perform early returns and cannot have underlying functions, so they are explicitly outside this doctrine.**

<a id="d1-糖衣定義の表contract-テストで等価を固定"></a>

### D1. Sugar-definition table (equivalence fixed by contract tests)

```almide
x ?? d      ≡  option.unwrap_or_else(x, () => d)          // Option operand
r ?? d      ≡  result.unwrap_or_else(r, (_) => d)          // Result operand(E 破棄は (_))
a ?? b ?? c ≡  右結合でネスト
r?          ≡  result.to_option(r)                          // Result → Option
o?.x        ≡  option.map(o, (v) => v.x)                    // Option のフィールド安全アクセス
```

Place an equivalence contract fixture on each row (native ⇄ wasm; output equality between operator and function forms).

<a id="d2--の公式化"></a>

### D2. Make `?.` official

Add the implemented and measured `?.` to the spec (`some(P{x:5})?.x ?? 0` → 5; `none` → 0). It is Option-only: **assign an E-code** to the existing dedicated diagnostic (currently codeless) for misuse on Result. From Result, it composes as `(r?)?.x` (exactly the composition of two D1 definitions).

<a id="d3-option-への-no-op--は警告"></a>

### D3. Warn about no-op `?` on Option

`o?` (operand already Option) is an identity mapping outside the domain. Stop silently accepting it; issue a warning:

```
warning[W0xx]: この ? は何もしません(operand は既に Option です)
  = help: ? は Result → Option の変換です。取り除いてください
```

During refactoring (return type Result → Option), compilation survives (MSR), while the dead operator becomes visible as cleanup work.

<a id="d4-eager-unwrap_or-は削除option--result-両方"></a>

### D4. Remove eager `unwrap_or` (both option and result)

A third spelling with no defining relationship. Deprecate, then remove (with a warning period following the E040-window precedent). After removal, E002 must carry a self-repair hint:

```
error[E002]: undefined function 'option.unwrap_or'
  hint: fallback は ?? を使ってください: port_opt ?? 8080
        関数形が必要なら option.unwrap_or_else
```

<a id="d5-rejected_patterns-の--記載を訂正"></a>

### D5. Correct the `??` entry in REJECTED_PATTERNS

“Rejected” → “adopted as sugar for `unwrap_or_else` (ADR-0005).” This closes the flagged contradiction.

<a id="d6-パイプ---miscompile-の修正方針"></a>

### D6. Policy for fixing the pipe × `??` miscompile

Structurally eliminate the BUG ledger's silent miscompile of `??` inside a pipe (native≠wasm, wrong value, exit 0) by arranging the implementation to apply D1 desugaring **before** pipe transformation. The fix PR follows this policy.

<a id="rationale"></a>

## Rationale

- **Discomfort with an operator lacking an underlying function started the adjudication**: removing functions and keeping only operators (the C# approach) would orphan an operator that cannot be used in higher-order or pipe positions. The Rust/Haskell approach (a defining relationship) is compatible with ADR-0004's “one spelling” principle: sugar is not a second spelling, but the surface of the same single definition.
- **The laziness of `??` matches `unwrap_or_else` exactly**, so this is not a contrived after-the-fact definition. The spelling `(_)` for discarding E, chosen as an expression of intent in ADR-0004 D3-(b), appears directly in the definition.
- **`?.` has the strongest prior in the family** (JS/TS/Swift/Kotlin/C# all have it). There is no reason to leave an implemented feature undocumented.
- **The eager version's only reason to exist is brevity**: when the default side has effects or cost, eager is inferior (always evaluated). E002 with a hint lets misspellings self-repair.

<a id="alternatives--検討して却下した案"></a>

## Alternatives — considered and rejected

1. **Remove functions and consolidate on operators** (C# / Kotlin approach): `??` becomes isolated; the discomfort about lacking an underlying function (raised during adjudication) remains, and the higher-order form is lost. **Rejected**.
2. **Explicitly fix coexistence of all three spellings**: parallel forms without a defining relationship violate ADR-0004's principle. **Rejected**.
3. **Remove `?.`**: it has the strongest prior and is already implemented; removal would turn the LLM's reflex into an error every time. **Rejected**.
4. **Specify silent acceptance of `o?`**: permits no-ops to accumulate. A warning can preserve both MSR (survival) and visibility for cleanup. **Rejected**.
5. **Keep the status quo** (contradictory entries, no documentation, triple spelling): **rejected** as out of the question.

<a id="consequences"></a>

## Consequences

- Fallback spellings converge on the defining pair “`??` (surface) + `unwrap_or_else` (substance).” `unwrap_or` disappears after a warning period (a breaking change, migrated through release notes and hints).
- The sugar-definition table (D1) enters the spec and contracts fix equivalence: room for semantic divergence between operators and functions is mechanically eliminated.
- Documenting `?.`, assigning its E-code, warning on `o?`, and correcting REJECTED_PATTERNS join the SPEC / diagnostics batch.
- The design policy for the pipe × `??` bug fix is settled (D6).

<a id="falsifier"></a>

## Falsifier

1. **If a case is found where desugaring-first implementation does not match current observable `??` behavior** (interactions of precedence and evaluation order), retreat from D1's “implementation desugaring” to “documented equivalence + contract,” and specify the difference explicitly.
2. **If dojo MSR degrades significantly after removing `unwrap_or`, even with hints**, withdraw D4 and restore it as a deprecated alias.
3. **If measurements show nesting or method-call forms of `?.` differ from D1's option.map definition**, revise the definition or explicitly limit its scope to field access in the spec.

<a id="references"></a>

## References

- Rust — [The question mark operator](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator), [`Add`](https://doc.rust-lang.org/std/ops/trait.Add.html) (operator = sugar for a trait function)
- C# — [?? operator](https://learn.microsoft.com/en-us/dotnet/csharp/language-reference/operators/null-coalescing-operator), Kotlin — [Elvis operator](https://kotlinlang.org/docs/null-safety.html#elvis-operator) (contrasting operators without underlying functions)
- TypeScript — [Optional chaining](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-3-7.html#optional-chaining) (the prior for `?.`)
- Internal: ADR-0004 (the one-spelling principle and expression of intent through `(_)`), the 2026-08-05 matrix (`coalesce/*` and `qop/optional-chaining/*` cells, pipe × `??` bug), REJECTED_PATTERNS.md (the target of D5's correction)
