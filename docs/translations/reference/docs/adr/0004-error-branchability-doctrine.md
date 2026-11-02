<a id="adr-0004-error-branchability--string-stays-terminal-doctrine-lint-and-context-instead-of-tags"></a>

# ADR-0004: Error branchability — String stays terminal; doctrine, lint, and context instead of tags

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](0004-error-branchability-doctrine.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0004-error-branchability-doctrine.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0004-error-branchability-doctrine.zh-CN.md)

> Non-normative pinned reader snapshot. The original remains authoritative; this copy is not automatically synchronized and has not independently revalidated implementations, measurements, or safety.
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0004-error-branchability-doctrine.md)
> Accepted is the historical decision status, not proof of current implementation. D2 and Alternative7 reject context sugar, but a retained code example still labels result.context as D2 sugar; that source inconsistency is preserved, not a supported-API claim. The unchanged-stdlib-surface consequence coexists with the proposed four-cell fs family. Cross-language theorems and demand estimates are source arguments, not independently established results.


- **Status**: Accepted
- **Date**: 2026-08-05
- **Decision scope**: Doctrine for representing error values: retain convergence on String, or add a structure that permits branching on contents (tags/error sets).
- **Related**: [ADR-0002](../../../../adr/0002-fallibility-effect-orthogonal.md) (fallibility axis), [ADR-0003](../../../../adr/0003-error-type-conversion-at-propagation.md) (lossless principle and official variant-E route), C-029 family (message-byte contracts, kept fixed in their entirety by this ADR).
- **History**: Continuation of the 2026-08-05 error-surface matrix discussion. Ratified through yes/no decisions on one concrete item per question. Adopted doctrine, lint and context; rejected tagged Strings (barcodes) and error-set types **including conditionally reserved proposals**. The initial staged-gate idea (Stage 1 tags → Stage 2 error sets) remains in Alternatives with its rejection reasons.

<a id="context--問題の定式化"></a>

## Context — formulating the problem

Almide errors converge on String: stdlib E is a String monoculture, `-> T!` has E=String (ADR-0002), and main's failure channel is String. This suffices for reporting (read by humans/LLMs), but lacks structure for **branching** (a program changes behavior based on error contents). Currently the only means is string matching, a pattern Go through 1.12 demonstrated harmful over ten years.

```almide
// 今日「なければデフォルト、それ以外は失敗」を書くとこうなる — Go 〜1.12 の再演
fn load_config(path: String) -> Result[String, String] =
  match fs.read_text(path) {
    ok(t)  => ok(t),
    err(e) =>
      if string.contains(e, "No such file")   // ✗ メッセージ本文に依存した分岐
      then ok(default_config())
      else err(e),
  }
```

Mitigating factors:

1. `??` provides first-class “fallback without inspecting contents,” absorbing most branching demand.
2. Predicates checked beforehand (`fs.exists`, etc.) can decide many branches before entering the error path.
3. User-defined domains can branch structurally using variant E (the official ADR-0003 D1 route).

<a id="各言語が証明した定理"></a>

### Theorems demonstrated by other languages

| Language | What it demonstrated |
|---|---|
| Go through 1.12 → 1.13 | At scale, String convergence inevitably leads to string-match branching. Retrofitting repairs (`%w`/`errors.As`) is possible but awkward |
| Rust (thiserror/anyhow split) | Even humans rejected per-type ceremony. Applications converge on type erasure; anyhow's essence is not erasure but context chains |
| Zig (error sets) | Zero ceremony and matchability coexist: no tag declaration, set inference, implicit lossless coercion to supersets |
| Swift (SE-0413) | The industry's destination is two layers: “erasure by default, typed errors as opt-in refinement” |
| Roc / OCaml polymorphic variants | Structural tag unions compose automatically without declarations, at the cost of difficult inference errors |
| Erlang/Elixir | Bare tags satisfy most branching demand; conventions work even without types |
| Gleam (snag) | In a small language without type classes, “String + cause chain” is a practical middle ground |

<a id="decision"></a>

## Decision

**Keep String as the default and endpoint. Do not introduce machine-readable structure (prefix tags/error sets) in error values. Contain string-match branching through doctrine and lint. Do not provide content-based branching for stdlib errors: use `??` fallback, predicates checked beforehand, and variant E in user domains. Provide reporting quality (context chains) through documentation of the `map_err` idiom without sugar (D2).**

<a id="d1-ドクトリン行"></a>

### D1. The doctrine line

Write this single line explicitly in spec / CHEATSHEET:
**“When you want to branch on an error's *contents*, that is the signal to switch to variant E (the official ADR-0003 D1 route). Do not branch by string-matching error messages.”**

```almide
// ✓ 自分の定義域では E を variant で設計する(0003 D1 の公式ルート)
type ConfigError = | Missing(String) | BadValue(String)

fn get_port(cfg: Map[String, String]) -> Result[Int, ConfigError] = ...

match get_port(cfg) {
  ok(p)            => p,
  err(Missing(_))  => 8080,                 // ← 構造で分岐。メッセージ変更に不感
  err(BadValue(m)) => process.exit(1),
}
```

<a id="d2-context-チェーンは糖衣なし--map_err-イディオムを正準形として文書化"></a>

### D2. Context chains without sugar — document the `map_err` idiom as canonical

Keep the existing spelling as the canonical form for prefixing context to errors (reject `result.context` sugar — Alternative 7):

```almide
// 正準形(CHEATSHEET に明記): 区切りは ": "、元エラーは ${e} で末尾に残す
let cfg = fs.read_text(path) |> result.map_err((e) => "loading config: ${e}")!

// 連鎖すると anyhow 流の文脈チェーンになる(v0.53.6 実測):
//   Error: starting server: loading config: No such file or directory (os error 2)
```

Known hazard: omitting `${e}` still typechecks and compiles, silently losing the original error (measured and confirmed). Address it with diagnostics, not sugar (D3-(b)).

```almide
// 今も書ける正準形:
let cfg = fs.read_text(path) |> result.map_err((e) => "loading config: ${e}")!

// D2 の糖衣(意味は上と同一):
let cfg = fs.read_text(path) |> result.context("loading config")!

// 呼び出しが連なると anyhow 流の文脈チェーンになる:
//   Error: starting server: loading config: No such file or directory
```

<a id="d3-エラー扱いの-lint-2-種実装-1105"></a>

### D3. Two error-handling lints (implementation: #1105)

**(a) Warning for string-match branching** — branching using `string.contains` or equality on a String originating in an err value. Anticipate Go's failure pattern through diagnostics:

```
warning[W0xx]: エラー文字列の本文で分岐しています
  --> app.almd:12
   |   if string.contains(e, "No such file") then ok(default_config())
   = help: エラーの内容で分岐するなら variant E を定義してください(ADR-0004 D1)。
           内容を見ない fallback なら ?? を検討
```

**(b) Warning for missing `${e}`** — when the lambda passed to `map_err` does not use its error argument. Catch the sole mistake in D2's canonical idiom that types cannot catch (silently losing the original error, measured and confirmed). Writing `(_) => ...` denotes intentional discard and produces no warning:

```
warning[W0xx]: map_err のラムダがエラー値 `e` を使っていません
  --> app.almd:5
   |   result.map_err((e) => "loading config")
   = help: 元のエラーを残すなら "loading config: ${e}"。
           意図的に破棄するなら (_) => ... と書いてください
```

<a id="d4-stdlib-エラーの内容分岐は提供しない明示的な非目標"></a>

### D4. No content-based branching of stdlib errors (explicit non-goal)

Provide no official means of branching by kind on stdlib-originated errors (E=String). Official means are:

1. **`??` fallback** — continue with a default regardless of kind (most demand).
2. **Predicates checked beforehand** — decide with `fs.exists(path)` etc. before entering the error path.
3. **Variant E** — design domains where branching is essential using your own E (0003 D1).

Where branching demand is real, address it individually through **API shape**, not added error structure: variants return “absence” as Option on the success channel (the same principle as `map.get` returning Option, not err). As the first application, the fs content-reader family (`read_text_if_exists`, etc., four cells) has been ratified — #1106.

```almide
let cfg = fs.read_text_if_exists(path)! ?? default_config()
//        none = 不在(正常系) / err = 権限・IO 等の本物の失敗
```

<a id="rationale"></a>

## Rationale

<a id="分岐需要の実態が構造を正当化しなかった"></a>

### Actual branching demand did not justify “structure”

After `??` and preliminary predicates absorb demand, remaining requests to branch by error kind are localized to things like fs not_found. Paying for rewriting every stdlib message (tags) or extending the type system (error sets) is disproportionate. Local demand fits existing types through API shape (D4's Option variants).

<a id="即時-3-点d1d3は安くて可逆"></a>

### The immediate three measures (D1–D3) are cheap and reversible

The doctrine line is documentation only, context names an existing map_err idiom, and lint only warns without breaking existing code. They are worth adopting now to anticipate string-match use in production, independently of introducing structure.

<a id="契約の単純さを守る"></a>

### Preserve simplicity of contracts

Tagging (rejected proposal A) involves redesigning the contract surface from “full message bytes” to “tags only,” complicating reproducibility guarantees. Fixing every message byte has the downside of Hyrum-style freezing. But as long as “do not branch on messages” (D1) is followed, practical harm is limited to friction when improving diagnostics, payable through normal contract-update procedures.

<a id="alternatives--検討して却下した案"></a>

## Alternatives — considered and rejected

1. **Tagged Strings (barcodes):** Standardize stdlib errors as “`タグ: 詳細文`,” inspect with `error.is(e, "not_found")`, and fix only the tag in the contract (similar to Node's `err.code === 'ENOENT'` or C errno). **Rejected**: rewriting the whole stdlib and redesigning contracts is excessive for localized branching demand (Rationale 1).
   ```almide
   fs.read_text("x.toml")        // => err("not_found: x.toml")(却下案)
   ```
2. **Error-set types (Zig family):** First-class `#not_found` tags + set inference + exhaustiveness checks. The sole implicitly composable structure consistent with ADR-0003's lossless principle, and theoretically the strongest candidate, but **rejected even as a conditional reservation “if A/B proves effective.”** Current demand does not justify the type-system extension (S cost). Any future revival should start from this section's analysis: choose set semantics, not nominal enums.
   ```almide
   fn load(p: String) -> Result[Config, #not_found | #parse] = ...   // 却下案
   ```
3. **Nominal typed E throughout stdlib (Rust-style):** Doubly countered by Rust's history (applications rejecting ceremony and moving to anyhow) and ADR-0003 (map_err ceremony at every boundary). **Rejected.**
4. **Roc/OCaml-style structural tag unions:** Row-polymorphic inference errors are difficult, with the highest S cost. **Rejected.**
5. **A rule borrowing tag vocabulary from standard taxonomies (errno/HTTP):** Vocabulary is a design decision each time, not constrained by a rule (with tags rejected, the rule no longer has a subject). **Rejected.**
6. **Do nothing (also defer D1–D3):** Leaves production undefended against string-match proliferation, repeating Go through 1.12. **Rejected.**
7. **`result.context(r, "msg")` sugar** (equivalent to anyhow/snag's `context`): It could prevent the missing-`${e}` hazard through a spelling with no degrees of freedom, but **rejected** in favor of **not adding a second stdlib spelling for the same thing** (2026-08-05: final decision after ratifying once, then sending it back). Replace it with idiom documentation (D2) and diagnostic measures (Falsifier 3). #1104 closed as rejected.

<a id="consequences"></a>

## Consequences

- Gains: the error-spec endpoint is settled (String + variant E, no added structure). No new mechanism in spec, contracts or implementation. D1/D3 contain string-match proliferation.
- Costs: branching by stdlib error kind remains officially impossible. Actual demand in fs-like areas is handled individually through D4's API shape (Option variants).
- Implementation is only #1105 (lint + doctrine line + D2 idiom documentation) and #1106 (fs family). Contracts, type system, stdlib messages and stdlib surface remain unchanged.

<a id="falsifier"></a>

## Falsifier

1. If Dojo measures that **string-match branches continue to be written despite D3 lint and `??`**, the ADR's premise that branching demand can be absorbed is wrong. Restart discussion by reevaluating Alternatives 1 (tags) / 2 (error sets).
2. **If D4's Option-variant APIs begin proliferating** (for example, `*_if_exists` families spread to at least three modules besides fs), demand was general rather than local: reconsider adding structure (Alternative 2).
3. **If context loss due to omitted `${e}` is still significantly detected in Dojo / real corpora despite D3-(b)'s warning**, treat that as evidence diagnostics cannot prevent it and reconsider the rejected sugar (Alternative 7).

<a id="references"></a>

## References

- Go — [Working with Errors in Go 1.13](https://go.dev/blog/go1.13-errors), [Errors are values](https://go.dev/blog/errors-are-values).
- Rust — [anyhow](https://docs.rs/anyhow), [thiserror](https://docs.rs/thiserror), [`io::ErrorKind`](https://doc.rust-lang.org/std/io/enum.ErrorKind.html) (same family as rejected proposal 1).
- Node.js — [`err.code`(errno)](https://nodejs.org/api/errors.html#errorcode) (same family as rejected proposal 1).
- Zig — [Errors](https://ziglang.org/documentation/master/#Errors) (same family as rejected proposal 2).
- Swift Evolution SE-0413 — [Typed throws](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md).
- Gleam — [snag](https://hexdocs.pm/snag/) (String + cause-chain middle ground).
- Internal: ADR-0002 / ADR-0003, C-029 family, 2026-08-05 matrix, #1104, #1105.

## Reading translations of retained code comments and diagnostics

In source order: the first example says today's “default if missing, otherwise fail” repeats Go through 1.12, and marks branching on the message body as wrong. The next designs variant E in one's own domain (0003 D1); the Missing arm branches structurally and is insensitive to message changes. The canonical context example specifies the separator “: ” and retaining the original error at the end via interpolation; chaining gives the displayed anyhow-style context message measured at v0.53.6. The next example labels the existing canonical spelling, then labels the rejected result.context spelling “D2 sugar, same meaning,” and says chained calls produce an anyhow-style chain. This label remains a historical inconsistency, not an adopted API.

The first warning says “you are branching on an error message's body”; help says to define variant E for content-dependent branching (ADR-0004 D1), or consider the fallback operator if not inspecting contents. The second warning says “the map_err lambda does not use error value e”; help asks to retain the original error through the displayed interpolation or write an underscore argument when discarding intentionally. The fs example says none is ordinary absence, while err is a real permission/IO failure. The final two code examples are explicitly labeled rejected proposals. In Alternative1 the retained Japanese inline label means “tag: detailed message.”
