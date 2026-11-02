<a id="adr-0003-error-type-conversion-at-propagation-points--lossy-conversion-must-be-spelled"></a>

# ADR-0003: Error-type conversion at propagation points — lossy conversion must be spelled

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](0003-error-type-conversion-at-propagation.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0003-error-type-conversion-at-propagation.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0003-error-type-conversion-at-propagation.zh-CN.md)

> Non-normative pinned reader snapshot. The original remains authoritative; this copy is not automatically synchronized and has not independently revalidated implementations, measurements, or safety.
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0003-error-type-conversion-at-propagation.md)
> Accepted records a historical decision, not independent confirmation of implementation. Measurements and language comparisons, including the claim that From conversions are bijective/lossless, are source assertions retained without fresh verification; this reader does not establish that claim as a general property. Existing behavior, the decision, and falsifiers remain distinct.


- **Status**: Accepted
- **Date**: 2026-08-05
- **Decision scope**: Conversion rules for error type E at propagation points (explicit `!` and auto-`?` in effect/fallible contexts): matching and mismatching types in both directions, whether to provide conversion hooks, and the position of the existing implicit none→err conversion.
- **Related**: [ADR-0002](../../../../adr/0002-fallibility-effect-orthogonal.md) (this ADR confirms and details its provisional D4; it does not amend it), #1103 (Phase 1).
- **History**: The measured matrix of 2026-08-05 revealed three different outcomes for the three E-type cells of `!`: matching=fully working / String→CustomE=ICE / CustomE→String=implicit Debug stringification. The initial draft proposed Swift-style “implicit widening to String=⊤,” but discussion chose strictness (see Alternative 1, retained with its rejection reasons).

<a id="context--実測が示した現状"></a>

## Context — what the measurements showed

```almide
type AppError = | NotFound(String) | Io(String)
```

| Operand E | Enclosing declaration E | Measured result (v0.53.6) |
|---|---|---|
| AppError | AppError | ✅ Propagates structurally without conversion; can match `err(NotFound(m))` |
| String (`int.parse`, etc.) | AppError | ❌ Passes check → “Almide bug” ICE during codegen (rustc E0277) |
| AppError | String | ⚠ Silently inserts `map_err(\|e\| format!("{:?}", e))` — degrades to a string in Rust **Debug** format |

Auto-`?` inside an effect fn uses the same mechanism and has the same three-cell structure (measured: a CustomE operand inside `effect fn -> Int` receives the same implicit Debug conversion).

Background facts:

1. **The stdlib has a single String culture for E.** `int.parse`, `value.field`, `fs.*`, `option.to_result`, and `-> T!` (ADR-0002) all use String.
2. **The failure channel of main is String.** An unhandled err ultimately becomes `Error: <msg>` + exit 1.
3. `!` implicitly converts none from an Option operand into `err("none")` (existing specification).

<a id="decision"></a>

## Decision

**E conversion at propagation points follows the principle “implicit if lossless; always visible in code if lossy.” An E mismatch is a check-time error in either direction, and the compiler supplies a canonical conversion form in its hint.**

<a id="d1-e-一致は無変換現状の一致セルを公式化"></a>

### D1. Matching E propagates without conversion (formalizing the working matching cell)

If operand E = declaration E, propagate the value unchanged. Structured errors (variant E) fully retain matchability on the matching path. State this in the spec as the “official route for typed errors.”

<a id="d2-e-不一致は方向を問わず-check-時エラー--machine-hintadr-0002-d4-の確認"></a>

### D2. An E mismatch in either direction is a check-time error plus a machine hint (confirming ADR-0002 D4)

The entire rule fits in one sentence: **`!` / auto-`?` do not convert E.** A mismatch is a check error with a new E-code, and its hint provides the canonical form for each direction:

```
// CustomE → String(上方向・損失あり)
error[E0xx]: `!` は err(ConfigError) を main の失敗チャネル(String)へ変換できない
  try: get_port(cfg) |> result.map_err((e) => "${e}")!

// String → CustomE(下方向)
error[E0xx]: `!` は err(String) を f の失敗チャネル(ConfigError)へ変換できない
  try: int.parse(s) |> result.map_err((e) => BadValue(e))!
```

For the downward direction, if only one constructor carries String, fill in its name too; otherwise list the alternatives for selection. Implementing this check eliminates both current implicit Debug stringification (§8-5) and the ICE that slips through check (§8-4).

<a id="d3-暗黙変換の唯一の許容条件は無損失-noneerrnone-は維持"></a>

### D3. The only condition permitting implicit conversion is “lossless” — keep none→err("none")

Because none carries no payload, embedding it into `err("none")` **discards no information** (lossless). The existing implicit behavior is therefore an application of the principle, not an exception, and remains. In contrast, `NotFound("alice")` → `"NotFound alice"` destroys matchability (lossy), so explicit `map_err` is required. This distinction is the principle of this ADR.

<a id="d4-from-型の暗黙変換フックは導入しない"></a>

### D4. Do not introduce a From-style implicit conversion hook

Do not introduce the equivalent of Rust's `From`/`?` (automatically applying user-defined conversions). Having the same `!` change meaning depending on a distant impl reduces predictability (LLM writability). If Dojo's MSR measurements show map_err noise causes actual harm, reconsider in a separate ADR with that evidence (see Falsifier).

<a id="d5-適用範囲は伝搬点で統一"></a>

### D5. Use “propagation points” as the unified scope

Explicit `!` and auto-`?` share the mechanism and rules. `?` (to-Option) and `??` discard E and are outside this ADR. The `try_*` family is E-polymorphic (the callback and return E share one type variable), so it too is outside scope.

<a id="rationale"></a>

## Rationale

<a id="損失のある変換は次の修正者への情報である"></a>

### A lossy conversion communicates information to the next modifier

MSR measures **whether the next modification survives**; the reader is a future modifier (primarily an LLM). The line `|> result.map_err((e) => "${e}")` makes “structure collapses to String at this boundary” visible right there. The fact that a modification depending on structured errors must not be written downstream becomes greppable and reviewable. Making it implicit pushes this information into non-local comparisons of type signatures.

<a id="儀式の書き手コストは-hint-がほぼゼロにする"></a>

### Hints reduce the writer's ceremony cost to almost zero

The upward canonical form has **only one spelling**, `|> result.map_err((e) => "${e}")`, and the check-error hint supplies it verbatim. For an LLM, “read the error and paste the canonical form” is among the highest-success classes of repair; there is no room to guess the conversion direction. The initial draft judged this “ceremony that carries no information,” but discussion concluded that it carries information **for the reader**, not the writer.

<a id="規則が一文になる"></a>

### The rule becomes one sentence

“`!` does not convert E.” Compared with direction-dependent cases (implicit upward, error downward), the spec, diagnostics, teaching material and LLM prompts all become shorter. The single exception is D3's lossless condition, explainable not as “conversion” but as “embedding from nothing.”

<a id="先例調査--暗黙の広げ先が-lossy-な言語は存在しない"></a>

### Precedent survey — no language has a lossy implicit widening target

| Language | Propagation with mismatched E | Equivalent of ⊤ | Does ⊤ retain structure? |
|---|---|---|---|
| Rust | Implicit conversion with a `From` impl; otherwise E0277 | `Box<dyn Error>` / `anyhow::Error` | ✅ Can downcast |
| Swift (SE-0413) | `throws(E)` → `throws` is implicit widening | `any Error` (existential type) | ✅ Can match with `catch let e as E` |
| Zig | Implicit coercion to a superset error set | `anyerror` | ✅ Retains tags; can `switch` |
| Go | Explicit wrap (`fmt.Errorf("…: %w", err)`) | `error` interface | ✅ `errors.As` / `errors.Is` |
| Kotlin/Java | throw widens along subtyping (exception mechanism) | `Throwable` | ✅ Catch by type |
| Gleam / Elm / Haskell `Either` | No implicit conversion — explicit `map_error` | None provided | — |

Common thread: **every permitted implicit conversion is lossless**, without exception (bijection through From, existential widening, superset coercion, subtyping), and the original structure can be recovered after conversion. No precedent equivalent to “implicit stringification” was found: Alternative 1 was an invention without precedent. Gleam/Elm, which have no ⊤, use explicit map_error exclusively, matching this ADR's decision (Gleam even shares the design constraint of rejecting type classes). D3's lossless principle verbalizes the condition satisfied by every language's implicit conversions.

<a id="一致セルの完動が-typed-error-の公式ルートを保証する"></a>

### The working matching cell guarantees the official typed-error route

Strictness loses nothing: align E throughout a domain (D1, measured as fully working), and convert explicitly only at boundaries. This naturally yields the idiom “write stdlib-facing helpers with E=String, then convert in the boundary fn.”

<a id="alternatives--検討して却下した案"></a>

## Alternatives — considered and rejected

1. **Implicit widening with String as ⊤ (this ADR's initial draft; Swift SE-0413-style):** Like implicit `throws(E)` → `throws(any Error)` widening, implicitly allow CustomE → String and stringify with a canonical repr. Its appeal is eliminating ceremony from glue code near main. Reasons for rejection: (a) a lossy conversion becomes invisible exactly where the next modifier should see it (the reverse of Rationale 1); (b) E=String returns become the path of least resistance, silently accepting a fn that accidentally fails to carry typed errors through (an inadvertent `-> Result[User, String]`), whereas strict checking demands “choose E”; (c) hints nearly eliminate the writer-cost difference, so the reward for selling visibility is small. Swift's ⊤ is matchable `any Error` (an existential type) and preserves structure, whereas Almide's conversion to String is irreversible. They were not isomorphic: that was the decisive difference.
2. **Rust-style From hook:** Rejected in favor of predictability (reconsider with evidence, D4).
3. **Automatic wrapping into a designated variant** (automatic injection into a String-carrying case): Ambiguous when there are multiple candidates; changes to a type definition change behavior. Limit this to suggesting a constructor in the hint (D2). Rejected.
4. **An operator with a converter** (`x!(Io)`, etc.): New syntax is not worth spending on sugar for something map_err can express. Rejected.
5. **Zig-style error-set union** (infer E1 ∪ E2): Requires importing the entire worldview in which error types compose as sets, losing the simplicity of Result[T, E]. Rejected.

<a id="consequences"></a>

## Consequences

- The specification becomes one sentence. The E-type matrix's three cells collapse into two states: “matching=official route / mismatch=error+hint.” §8-4 (ICE) and §8-5 (implicit Debug degradation) disappear through the same check.
- Boundary crossings near main (roughly 5–15 places in a CLI) explicitly contain `|> result.map_err((e) => "${e}")`. This is paid for as boundary visibility, not treated as overhead.
- **Compatibility:** Code relying on current implicit Debug conversion becomes a check error. Actual harm is estimated to be nearly absent because the behavior was SURPRISE-level, but release notes must state migration hints. There is no output-format contract change: the implicit conversion itself disappears.
- none→err("none") stays as an application of the lossless principle (D3). Reconsidering the wording (a message more informative than “none”) is outside this ADR.

<a id="falsifier"></a>

## Falsifier

1. **If Dojo's MSR measurements show that map_err ceremony itself is a significant source of modification failures** (wrong direction, mistyping `"${e}"`, etc.), that is evidence hints have not prevented it. Reconsider Alternative 1 (implicit widening) in a new ADR.
2. **If measurements in a real corpus show boundary ceremony is dominant** (for example, a substantial proportion of fallible fns have map_err lines), retract D4 and propose an ADR for a From-style hook or widening.
3. **If a conversion appears whose losslessness is ambiguous** (a third case D3's boundary cannot classify), revise the principle starting with its definition.

<a id="references"></a>

## References

- Measurements: the `bang/pure-Result-CustomE-fn/*` and `bang/effect-fn->T/CustomE-operand` cells of the 2026-08-05 matrix (v0.53.6, probes retained).
- Swift Evolution SE-0413 — [Typed throws](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md) (comparison for Alternative 1: `any Error` is matchable and not isomorphic to Almide's stringification).
- Rust — [`std::convert::From` and `?`](https://doc.rust-lang.org/std/convert/trait.From.html), [The question mark operator](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator).
- Zig — [Error Set Type](https://ziglang.org/documentation/master/#Error-Set-Type), [anyerror](https://ziglang.org/documentation/master/#The-Global-Error-Set) (examples of lossless coercion).
- Go — [Error wrapping: `%w`, `errors.Is` / `errors.As`](https://go.dev/blog/go1.13-errors) (explicit wrapping + retained structure).
- Gleam — [`gleam/result.map_error`](https://hexdocs.pm/gleam_stdlib/gleam/result.html#map_error) (isomorphic precedent with no ⊤ and explicit conversion only).
- Internal: ADR-0002 D4 (confirmed and detailed by this ADR), C-211, `!`'s none→err("none") (an application of D3).

## Translation of diagnostic text retained above

The first diagnostic is the upward, lossy CustomE-to-String case: the propagation operator cannot convert err(ConfigError) to main’s String failure channel. The second is the downward String-to-CustomE case: it cannot convert err(String) to f’s ConfigError failure channel. The try lines preserve the exact suggested calls.
