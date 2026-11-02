<a id="adr-0012-error-surface-end-state--refinement-stays-in-the-marker-te-erasure-stays-the-default"></a>

# ADR-0012: Error-surface end state — refinement stays in the marker (`T!E`), erasure stays the default

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](0012-typed-error-refinement-in-the-marker.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0012-typed-error-refinement-in-the-marker.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0012-typed-error-refinement-in-the-marker.zh-CN.md)

> Non-normative historical reader snapshot, not a current implementation guarantee. The unchanged pinned original is authoritative; no automatic synchronization, independent benchmark, literature search, or safety certification. Preserve the initial “implementation not started” status alongside later rollout evidence. The 2026-08-20 amendment retracts broad meaning-preserving fmt normalization in favor of collapse-only; the 2026-09-27 amendment replaces D4-4 and says its explicit-Result lambda workaround did not exist in measurement. Earlier contrary passages are historical, not simultaneous current guidance. Cross-language, best/only/no-study, download and LLM-success assertions are the source author’s dated survey claims, not independently validated here. Source code/comments remain unchanged. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0012-typed-error-refinement-in-the-marker.md)

- **Status**: Accepted, design ratified; implementation not started.
- **Date**: 2026-08-11
- **Scope**: the error-type specification's **end state**: assignment criteria for two layers, erased String and refined variant E; refined notation `T!E`; fmt canonical form; fixed non-goals concerning lambda / main / conversion hooks / union structures / implicit propagation.
- **Related**: [ADR-0002](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0002-fallibility-effect-orthogonal.md), **this ADR revises D2** from “E fixed to String” to “E defaults to String, refinable inside the marker”; [ADR-0003](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0003-error-type-conversion-at-propagation.md), unchanged, no-conversion-at-propagation applies to `T!E`; [ADR-0004](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0004-error-branchability-doctrine.md), unchanged, stdlib's String end state and rejection of tags/error sets remain; [ADR-0008](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0008-explicit-propagation-only.md), unchanged with added evidence; ADR-0006 / 0009 / 0010, notation consistency.
- **Background**: on 2026-08-11, surveyed other languages to settle “the best error-type specification as of 2026”: MoonBit / Swift / Zig / Rust / Gleam / Roc / Unison / Inko / Koka / Flix / Effekt. Based on 2024–2026 primary materials, proposals, release notes, issue rulings and production reports. D1–D4 ratified that day with one concrete item per yes/no question, in order D2 → D3 → D1 → D4, all yes.

<a id="context--現状の非対称"></a>

## Context — The Current Asymmetry

Almide's error surface reached two layers through the 0.54–0.56 arc:

- **Erased layer, default**: E = String. `T!` ≡ `Result[T, String]`, ADR-0002 D2; effect-fn implicit lift; all stdlib; main's failure channel. Context uses the `map_err` idiom + E036; E035 blocks string-match branching, ADR-0004.
- **Refined layer, opt-in**: variant E. Explicit `Result[T, MyError]` allows `!` to propagate **the structure intact** at matching-E propagation sites and permits match, ADR-0003 D1's “official typed-error route.” #1103 already removed ICE / implicit Debug degradation.

Semantics are complete. **Only refined-layer notation is missing**, causing two concrete harms:

1. **Model jump**: erased fallibility is a “function attribute,” postfix `!`, but refinement abruptly changes notation to a “generic type,” `Result[T, E]`. Swift `throws` → `throws(E)` and MoonBit `raise` → `raise E` refine inside the marker. Only Almide crosses models.
2. **Refinement forces body rewriting**: a pure fn's explicit `-> Result[T, E]` has no tail lift; lift belongs to the `!` marker and effect fn, as branched in `crates/almide-syntax/src/parser/declarations.rs`. Refining a `-> Config!` function into `-> Result[Config, ConfigError]` therefore requires not just its signature but **wrapping value tails with `ok(...)` in the body**. A most-common edit class, refining an error type one level, cannot stay minimal — directly harming MSR, modification survival rate.

<a id="survey--2026-年の他言語実測一次資料"></a>

## Survey — Other Languages in 2026, Primary-Source Evidence

<a id="収斂-1-消去が既定型付きは閉域の-opt-inは業界横断の到達点"></a>

### Convergence 1: Erasure by Default, Typed Errors as Closed-Scope Opt-In

| Language | Default | Refined layer | Evidence |
|---|---|---|---|
| Swift | Untyped `throws` | `throws(E)`, SE-0413 | Proposal explicitly says “the existing (untyped) `throws` **remains the better default** error-handling mechanism for most Swift code.” Two years after shipping, actual use is largely confined to Embedded Swift and generic mechanisms, `AsyncSequence.Failure` |
| Rust | `anyhow` for apps | `thiserror` for libraries | Split continues in 2026, cumulative downloads ~860 million / ~1.3 billion. Std error improvements effectively stalled; provider API moved toward abandonment in 2026-02, pivoting to RFC 3885 |
| MoonBit | Bare `raise`, ≡ `raise Error` | `raise E`, suberror | Latest blessed `@string.parse_int` uses **bare raise + message string**. Core counts: `raise?` 329 / bare `raise` 68 / typed 29. Stringly versus structured **still disputed** in core PR #3997 |
| Unison | — | Has `Throw e` | Ecosystem converges on catch-all `Exception`/`Failure`, evidence a refined layer alone cannot serve the outer boundary |
| Zig | Typed sets default, countertrend | — | At scale: explicit sets at boundaries, inference internally, plus pressure toward `anyerror`. Inferred-set recursion unsupported, #2971 still open in 2026 |

<a id="収斂-2-エラーはペイロードを運べなければならない"></a>

### Convergence 2: Errors Must Carry Payloads

Zig **permanently rejected** payload proposal #2647 in 2024-12. Andrew Kelley's closing comment was one sentence: “**Error codes are for control flow.**” Practitioners describe the endorsed diagnostics out-parameter workaround as “extremely unergonomic”; even stdlib applies it inconsistently, std.json unsupported. Almide variant E constructors can carry payloads, so it never had this omission.

<a id="収斂-3-境界の自動合成union--row--setは対価が高い"></a>

### Convergence 3: Automatic Boundary Composition, Union/Row/Set, Has High Costs

- **Roc**, structural open tag unions, maximum composition: no boundary ceremony, but open/closed inference becomes the language's hardest concept.
- **Koka**, effect rows: automatic merging, but difficult row-inference errors, confirmed by 2025 practitioner reports exposing internal type-variable names.
- **Zig**, error sets: composition works **only with a closed-world compilation dependency**, matklad's analysis, not exportable to other languages.
- **Inko**, decisive datum: actually shipped the simplest “one nominal throw type in a signature” design, then **withdrew it in 0.11.0**. Verbatim reason: “This makes composing errors much easier.” Mandatory single nominal types broke down at boundaries. Almide **does not force** variant E, defaulting to String, so differs from this failure mode; cite it as evidence refined layers must not be default.
- **Gleam**, nominal plus explicit conversion only, like Almide: `map_error` ceremony remains, but the core decision is “do not change,” rejecting the `?` operator in 2024-11. The author of ecosystem escape hatch snag, one opaque type, explicitly says not to use it in libraries — evidence erased-layer demand exists.

<a id="収斂-4matklad-2025-12-現代エラーモデルの三点合意と未提供の中間点"></a>

### Convergence 4, Matklad 2025-12: Three-Point Agreement and the Missing Middle

Go/Rust/Swift/Zig converged on (1) **call-site fallibility markers**, (2) **a separate panic channel**, (3) **errors as values**, “The Second Great Error Model Convergence.” Java checked exceptions failed because changes to E **propagated through every signature chain**. Modern designs charge only for 0→1, total→fallible, not N→N+1, adding a variant. And “the **middle between exhaustive enumeration and total erasure is not adequately provided by any language**.”

Almide's current semantics satisfy all three. Nominal variant E also **passes the N→N+1 test**: adding a ConfigError variant changes no signature. The middle — erased default, payload-bearing refined layer, forced visibility of lossy conversion — is already occupied. Only notation remained: that is this survey's conclusion.

<a id="moonbit-の-te-放棄の解剖--記法ではなく呼び出し点モデルの問題だった"></a>

### Dissecting MoonBit's Abandonment of `T!E`: Call-Site Model, Not Notation

In 2024–2025 MoonBit abandoned `fn f() -> Int!String` for `-> Int raise E`. The painful points were all **call sites** and **glyph overload**:

- Three call-marker turns in two years: `f(x)!` → `f!(x)`, explicitly for IDE completion, → removal.
- `!` overloaded with distinct meanings in `type!` / `f!` / `!!` / `catch!`.
- Semantic trap in `f?(g!(..))`, explicitly acknowledged as not mechanically migratable.

**No primary material demonstrates a defect in return notation `T!E` itself**, no dedicated withdrawal document. MoonBit then made propagation **implicit**, compensating visibility by IDE underlines on fallible calls: beta-release says “The IDE will automatically _underline_ functions that may throw.” This compensation is **invisible to raw-text writers, LLMs and diff reviewers**, exactly why Almide chose explicit propagation in ADR-0008. Almide's `!` has only propagation-operator and return-marker meanings, not trap on `!`, ADR-0002 D4; MoonBit's overload conditions do not recur.

<a id="実証の穴機会"></a>

### Empirical Gap, an Opportunity

**No study exists** measuring typed versus erased E's effects on LLM generation accuracy or modification survival rate, as of 2026-08, including none at LMPL 2025. No language other than MoonBit published error design justified by LLM writability. With its MSR measurement infrastructure, Dojo, Almide uniquely can fill this gap with measured falsifiable evidence.

<a id="decision"></a>

## Decision

**Declare the two-layer error model the end state; keep refined notation inside the marker. Introduce `-> T!E` ≡ `Result[T, E]` at fn declaration returns and fn-type slots. String remains default, `T!` ≡ `T!String`; stdlib remains String, ADR-0004 unchanged; no conversion at propagation remains, ADR-0003 unchanged.**

<a id="d1-二層ドクトリンの終着宣言層割当規準の明文化"></a>

### D1. Declare the Two-Layer Doctrine Final, Explicit Assignment Criteria

Write layer assignment as one spec paragraph:

- **Erased layer, default**: E = String is a **reporting channel**, read by humans/LLMs. Prefix context with `map_err`, ADR-0004 D2.
- **Refined layer, opt-in**: variant E is a **branching channel**, programs vary behavior based on content. The switch signal is exactly ADR-0004 D1's doctrine: “when you want to branch on content, variant E.” Scope is **closed**, places catching all errors within a module/package, same boundary as SE-0413's three conditions.
- Downgrading between layers, variant E → String, must visibly use `map_err` at boundaries, ADR-0003.

Two layers + no conversion + payload-bearing refinement satisfy matklad's missing middle, the form this ADR advocates as the right 2026 design.

<a id="d2-te--精密化はマーカー内で完結する"></a>

### D2. `T!E` — Refinement Stays in the Marker

```almide
type ConfigError = | Missing(String) | BadValue(String)

// read_cache: (String) -> String!ConfigError
fn load(path: String) -> Config!ConfigError = {
  let text = read_cache(path)!          // E 一致 → 構造のまま伝搬(ADR-0003 D1)
  guard valid(text) else err(BadValue(path))
  parse_config(text)                    // 値 tail → ok(...) に lift(T! と同一の人間工学)
}

effect fn main() -> Unit = {
  let cfg = load(p) |> result.map_err((e) => "loading config: ${e}")!  // 降格は可視
  ...
}
```

- **Desugaring**: `-> T!E` ≡ `-> Result[T, E]`. Redefine `T!` as shorthand for `T!String`, revising ADR-0002 D2 from fixed String to default String.
- **Positions**: exactly those where the `!` marker is legal today: fn declaration returns and fn-type-slot returns, `op: (Int) -> Int!ParseError`. Not a type constructor: `List[Int!E]` invalid, preserving ADR-0002 D2's attribute model.
- **E grammar**: a **named type atom** immediately after `!` on the same line; qualified `mod.Error` and generics permitted. No postfix `?` / `!` on E itself; use explicit Result for Option E, demand unobserved. Current grammar has vacant space for a type name after `!`, where only `=` `{` `,` `)` and newline can appear, so one-token lookahead parses unambiguously.
- **Body ergonomics**: **identical** to `T!`, value-tail ok-lift, `!` propagation, legal `err(...)`. Refinement `T!` → `T!E` therefore **changes one signature location only**, eliminating Context 2's mandatory body rewrite.
- **Composition with effect fn**: `effect fn f() -> T!E` is effectful and fallible with E. It is the sugar side of the existing “do not double-wrap explicit Result” rule, not a new rule.
- **Propagation**: unchanged, “`!` does not convert E,” ADR-0003. `T!E` extends application of existing rules, adding no conversion, inference, or exception.

<a id="d3-fmt-正準形--戻り位置の-result-はマーカーへ"></a>

### D3. Fmt Canonical Form — Return-Position Result Becomes a Marker

`almide fmt` normalizes `Result[T, String]` in fn-declaration and fn-type-slot **return positions** to `T!`, and `Result[T, E]` to `T!E`, also `T!String` to `T!`. **Outside return positions**, let annotations, fields, type arguments, `Result` remains unchanged. Follow ADR-0002/0010: `!` is an arrow attribute, not a value type.

This closes ADR-0002's open question of which form fmt normalizes to, resolving “one type, two notations” with the same approach as ADR-0010 D3, `Option[T]` → `T?`. Normalization preserves meaning: everything legal in an explicit Result body stays isomorphic under lift; lift only expands acceptance, not changes the meaning of existing Result-typed tails.

<a id="d4-非目標の固定再確認--新証拠"></a>

### D4. Fixed Non-Goals, Reconfirmation and New Evidence

1. **Stdlib remains String**, ADR-0004 unchanged. MoonBit core's actual blessed parser, bare raise + message with structuring disputed, and Unison catch-all convergence reaffirm erased stdlib. ADR-0004's falsifier still governs reconsideration.
2. **No conversion hooks, From equivalent, union/row/error set**, ADR-0003 D4 / 0004 unchanged. New evidence: Koka row-inference message difficulty measured in 2025, Roc open/closed difficulty, Zig sets' closed-world dependence, Rust provider-API abandonment.
3. **Propagation stays explicit**, ADR-0008. Reject MoonBit's implicit + IDE compensation because visibility becomes zero through raw text. This survey **provides evidence** for that decision; Convergence 4's call-site-marker agreement points the same way.
4. **Lambda failure channels remain String**, ADR-0009 L3. Use-driven E inference imports Koka-style inference-error difficulty. Typed-E lambdas use explicitly annotated Result, working fully as ordinary values.
5. **Main's failure channel remains String**. Boundary-downgrade `map_err` is the visibility point, ADR-0003 Consequences.

<a id="rationale"></a>

## Rationale

<a id="精密化は表記モデルを跨いではならない"></a>

### Refinement Must Not Cross Notation Models

Swift `throws` → `throws(E)`, MoonBit `raise` → `raise E`, Zig `!T` → `E!T`: **no language refines outside the marker**. Only Almide forced an attribute→generic-type jump, doubling notation models to learn and cascading into body rewriting through lost tail lift, Context harm 2. `T!E` is the smallest move eliminating that jump.

<a id="最小差分こそ-msr"></a>

### Minimal Diffs Are MSR

Refining error types is a most-frequent closed-scope edit class. After D2, adding one signature token suffices; all propagation E mismatches are caught by check errors plus canonical-form hints, already implemented in #1103. “Read error text and paste canonical form” is among LLMs' highest-success edit classes, ADR-0003 Rationale. The entire refinement path rests on the existing safety net.

<a id="string-単一文化の希釈リスクには表記ではなくドクトリンで答える"></a>

### Answer String-Monoculture Dilution with Doctrine, Not Notation

Cheap `T!E` could encourage excessive variant E, the same concern SE-0413 explicitly warns about: “Resist the temptation to use typed throws because there is only a single kind of error.” Almide answers through doctrine: **branching demand**, ADR-0004 D1 + E035 lint, not cheap spelling, signals switching to variant E. Even a non-branching variant E causes little harm; lowering it to String is visibly `map_err`. As long as stdlib stays fixed to String, D4-1, monoculture's framework remains.

<a id="儀式コストは-gleam-水準に留まりinko-の失敗形にはならない"></a>

### Ceremony Stays at Gleam's Level, Not Inko's Failure Mode

Nominal E plus explicit boundary `map_err` costs what Gleam judged acceptable in 2024-11; Almide makes it lighter with #1103 hints. Unlike Inko's withdrawn mandatory single nominal type, Almide refinement is opt-in and erasure default, so composition difficulty does not escape the closed scope.

<a id="alternatives--検討して却下した案"></a>

## Alternatives — Considered and Rejected

1. **Keyword approach**, new keyword like `raise E` / `throws(E)`, same as MoonBit's destination. Rejected: Almide already shipped postfix `!`, ADR-0002; a keyword adds one meaning/two spellings **to the language core**. MoonBit's pain that led it to abandon `!`, glyph overload and call markers, is absent here.
2. **Implicit propagation + IDE compensation**, MoonBit 2025 style. Rejected: directly contradicts ADR-0008. IDE underlines do not exist in raw text, invisible to LLMs and review diffs. MSR's reader is the future editor, who reads only text.
3. **Error sets / structural unions / rows**, Zig/Roc/Koka. Reaffirm rejection, ADR-0004 Alternatives 2/4. Survey Convergence 3: composition costs, inference messages and closed-world dependence, corroborated by 2025–2026 evidence.
4. **From conversion hooks**: reaffirm rejection, ADR-0003 D4. Rust's own provider-API abandonment, 2026-02, indirectly supports the maintenance cost of mechanisms dependent on distant impls.
5. **Status quo, explicit Result only for refinement**: rejected, retaining model jumps and forced body edits, Context harms 1/2. It works, but not with minimal diffs.
6. **Type part of stdlib alongside `T!E`**: rejected, encroaching on ADR-0004's scope. Stdlib reconsideration only through 0004's falsifier.

<a id="consequences"></a>

## Consequences

**Gains**: refinement becomes a one-location signature edit. All error-spec decisions fit three clauses: marker = fallibility attribute; E defaults to String and refines with one atom; conversion always visible. Closes ADR-0002's fmt canonical-form question.

**Costs**:
- Implementation: two parser sites, allow a type atom after `Bang` in declaration returns and fn-type slots; one resolver site, pseudo-generic `!` args.len()==2; fmt printing and normalization; spec/lang tests and CHEATSHEET / result-option-effect.md / llms.txt updates. Existing `!` matchers, module_inference / lower / check, match names only without checking arg counts, so unchanged.
- D3 normalization causes a one-time diff across stdlib/spec. Splice-context sources use `--no-import-edit`, negative diagnostic fixtures excluded, following existing fmt practice.
- Observable behavior unchanged, notation only, so **no contract-ledger update needed**. `T!E` semantics equal the existing explicit-Result route, and existing fixtures remain evidence. Add only `spec/lang/` notation tests: parse, fmt, lift isomorphism.

<a id="falsifier--何が起きたらこの決定を撤回するか"></a>

## Falsifier — When to Withdraw

1. **If Dojo MSR shows significantly increased edit failures caused by E-mismatch check errors after `T!E`**, proving cheap notation proliferates variant E and hints cannot recover String-monoculture dilution, supersede with an ADR retiring `T!E` from declaration positions and restoring explicit-Result-only refinement.
2. **If D3 normalization causes real harm**, evidence intentional return-position Result distinctions exist, withdraw only D3 and narrow normalization to `T!String` → `T!`.
3. **If ADR-0004's falsifier fires first**, reconsidering stdlib structure, this ADR's D4-1 follows 0004's new ruling. Layer structure D1 and notation D2 survive independently.

<a id="references"></a>

## References

- Swift SE-0413 — [Typed throws](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md)
  (verbatim source of “untyped `throws` remains the better default” and “Resist the temptation…”)、
  [FullTypedThrows not shipped](https://forums.swift.org/t/where-is-fulltypedthrows/72346)、
  [SE-0421 AsyncSequence.Failure](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0421-generalize-async-sequence.md)
- Zig — [#2647 payload permanently rejected: “Error codes are for control flow.”](https://github.com/ziglang/zig/issues/2647)、
  [#2971 inferred-set recursion, open](https://github.com/ziglang/zig/issues/2971)、
  [practitioner report on diagnostics patterns](https://srcreigh.ca/posts/error-payloads-in-zig/)
- Rust — [thiserror](https://docs.rs/thiserror) / [anyhow](https://docs.rs/anyhow)(downloads measured 2026-08)、
  [provider API moving toward abandonment, #99301](https://github.com/rust-lang/rust/issues/99301)
- MoonBit — [error-handling docs](https://docs.moonbitlang.com/en/latest/language/error-handling.html)、
  [2025-06-03](https://www.moonbitlang.com/updates/2025/06/03/index) /
  [2025-06-16 migration table](https://www.moonbitlang.com/updates/2025/06/16/index)、
  [beta-release, verbatim IDE-underline compensation](https://www.moonbitlang.com/blog/beta-release)、
  [core PR #3997, stringly versus structured dispute](https://github.com/moonbitlang/core/pull/3997)、
  Old notation: [0722](https://discuss.moonbitlang.com/t/the-moonbit-update-0722/271) /
  [0729](https://discuss.moonbitlang.com/t/the-moonbit-update-0729/272)
- matklad — [The Second Great Error Model Convergence(2025-12-29)](https://matklad.github.io/2025/12/29/second-error-model-convergence.html)
- Inko — [0.11.0 withdrawal](https://inko-lang.org/news/inko-0-11-0-released/)
- Gleam — [`?` operator rejected, 2024-11](https://github.com/gleam-lang/gleam/discussions/3908)、
  [snag](https://github.com/lpil/snag)
- Roc — [FAQ(tag union rationale)](https://www.roc-lang.org/faq)、
  [open/closed union analysis](https://gist.github.com/j-maas/ed3d2811d808d0fa1386478575df928d)
- Unison — [abilities error handling](https://www.unison-lang.org/docs/fundamentals/abilities/error-handling/)
- Koka — [row-polymorphic effects](https://arxiv.org/abs/1406.2061)、
  [2025 practitioner report, row-inference difficulty](https://gfxmonk.net/2025/04/13/im-excited-about-koka.html)
- Internal — ADR-0002/0003/0004/0006/0008/0009/0010、#1103、C-211、
  `crates/almide-syntax/src/parser/declarations.rs`(lift branch)、
  `crates/almide-frontend/src/canonicalize/resolve.rs`(`!` desugaring)


<a id="amendment-2026-08-20-d3-は-collapse-only-に縮小"></a>

## Amendment, 2026-08-20: Narrow D3 to Collapse Only

The premise “normalization preserves meaning” received **three classes of counterexample** during #1194 rollout, all reproduced in real programs/fixtures, formatter-induced behavioral changes:

1. **Named-callback routing**: ADR-0006 D1 keys HOF first-err routing on the callee declaration's **marker spelling**; `fallible_marker_fns` text-matches bare `Ident`. Rewriting `fn f() -> Result[Int, String]` to `-> Int!` changes `list.map(xs, f)` from `List[Result[..]]` to first-err `Result[List[..], E]`. In forms such as `"${list.map(ss, f)}"`, both spellings compile and **only output changes**.
2. **Fn-type-slot acceptance**: explicit `(Int) -> Result[Int, String]` accepts neither marker-declared callbacks nor marker lambdas, three errors, while `(Int) -> Int!` accepts them. Rewriting slots changes the checker's acceptance set.
3. **V1 lowering wall parity**: effect fns in fan contexts, the `ok11` family in `fan_any_early_winner`, render to v1 with explicit Result but hit the heap-result-match wall with marker spelling. A green fixture turns red from spelling alone.

Therefore fmt canonicalization in D3 is for now **only collapse `T!String` → `T!`**, unifying spelling within the marker without moving any semantic axis. Rewriting `Result[T, E]` → `T!E` waits until **marker/explicit behavioral equivalence holds compiler-wide**, requiring unification of 1–3 above. For freeze A0-1, the canonical rule is settled: “both spellings legal; fmt performs collapse only.”

<a id="amendment-2026-09-27-d4-4-は-adr-0021-で置き換え"></a>

## Amendment, 2026-09-27: ADR-0021 Replaces D4-4

D4-4, lambda failure channels remaining String, is replaced by [ADR-0021](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0021-lambda-failure-channel-is-the-join-of-its-bangs.md). Channel E uses an expected type when available, otherwise the join of E from body `!` operands, a total least upper bound in E ≤ String, default String. Feared Koka-style inference-error difficulty does not arise for a total join without row variables. The fallback “typed-E lambdas use explicitly annotated Result” did not exist in measurement: typed slots E005, typed `let` E001. D4-4's “ADR-0009 L3” actually refers to L3 of `docs/specs/result-option-effect.md`.
