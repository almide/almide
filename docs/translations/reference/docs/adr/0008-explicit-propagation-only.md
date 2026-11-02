<a id="adr-0008-propagation-is-always-explicit--auto--is-abolished--is-the-only-marker-result-is-must-use"></a>

# ADR-0008: Propagation is always explicit — auto-? is abolished, `!` is the only marker, Result is must-use

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](0008-explicit-propagation-only.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0008-explicit-propagation-only.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0008-explicit-propagation-only.zh-CN.md)

> Non-normative reader snapshot of the [Japanese canonical source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0008-explicit-propagation-only.md), pinned at `35297a612fb29e3f78f6587aba253aa91d51f466`. The original ADR remains authoritative and immutable. This snapshot is not automatically synchronized and is not checker or generator input. Decisions, dates, measurements, implementation status and claims below describe the pinned historical source, not independent verification or a current-release guarantee. Code blocks and comments are preserved verbatim.

> Source caveat: the v0.53.6 matrix, language-precedent assertions and migration schedule are historical statements of this ADR, not independently reproduced here. E0xx is the source placeholder, not a newly assigned diagnostic. The dojo MSR falsifier is retained as a historical criterion, not a new benchmark result.

- **Status**: Accepted
- **Date**: 2026-08-05
- **Decision scope**: the failure-propagation mechanism inside effect fn / fallible fn — whether to retain implicit propagation (auto-`?`), unify propagation markers, and how to handle discarded Result values.
- **Related**: [ADR-0002](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0002-fallibility-effect-orthogonal.md) (D4: `!` is propagation-only. This ADR makes D3's lift parity trivial), [ADR-0005](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0005-operators-desugar-to-stdlib.md) (the sugar definitions of `?` and `??` — this ADR makes them composable without exceptions), [ADR-0006](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0006-fallibility-polymorphic-hofs.md) (the canonical explicit `!` form), #1103 (Phase 1).
- **History**: arose on 2026-08-05 from a deep dive into the error-surface matrix's 21 cells in track B (position-dependent auto-`?`). The yes/no adjudication chose “fully explicit + must-use is an error.”

<a id="context--実測したまだらの全景v053621-セル"></a>

## Context — the full measured picture of the patchwork (v0.53.6, 21 cells)

auto-`?` (the mechanism that implicitly propagates fallible calls inside an effect fn) **works in some positions and not others**:

| Implicit propagation works (5 positions) | Does not work — explicit `!` required (5 positions) |
|---|---|
| Statement position (`fail()` alone) | Function arguments (`double(get())` → E005) |
| Unannotated let (`let r = int.parse(s)`) | Pipe (`get() \|> double` → E005) |
| if condition | record field (→ E001) |
| match (with value patterns) | List element (→ E001) |
| String interpolation (with zero warnings) | Tuple component (→ unnumbered error) |

Two still worse measured cases:

```almide
// 1. パターンの形が実行時の生死を決める:
match get() { ok(v) => ..., err(e) => ... }   // Result のまま受ける(err は自分で捌く)
match get() { 42 => ..., _ => ... }           // auto-? が差し込まれ、err ならプログラム停止

// 2. 注釈の有無が挙動を反転させる:
let r = int.parse("zz")                        // 暗黙伝搬 — err で停止
let xs: List[Result[Int, String]] = [get()]    // 保持 — 停止しない
```

Precedent survey: **there is no language with “implicit propagation only in some positions.”** All are at one pole or the other — fully explicit (Rust `?` / Swift `try` / Zig `try`: a marker at every propagation site, no positional variation), or fully implicit (Java/Python exceptions, OCaml 5 / Koka effects: no marker, uniform across all positions). The essential point is that fully implicit languages have no Result **value** (exceptions are not values). If Almide, which has Result as a value, moves toward full implicitness, the ambiguity between “retain as a value” and “propagate” spreads to every position.

<a id="decision"></a>

## Decision

<a id="d1-auto--は廃止--伝搬の綴りは--一本"></a>

### D1. Abolish auto-`?` — `!` is the sole spelling of propagation

Inside effect fn / fallible fn, a fallible call **produces a Result value in every position**. Write `!` to propagate. There is no positional variation:

```almide
effect fn main() -> Unit = {
  let cfg  = fs.read_text(p)!            // 伝搬は常に明示
  let port = int.parse(s)!
  double(get()!)                          // 引数位置も同じ綴り
  println("v=${get()!}")                  // 補間も同じ綴り
  match get() { ok(v) => ..., err(e) => ... }   // Result は普通の値 — 普通に match
}
```

<a id="d2-捨てられた-result-は-check-エラーmust-use"></a>

### D2. Discarded Result is a check error (must-use)

A Result cannot silently be discarded in statement position (closing the hole that would otherwise turn removal of auto-`?` into silent error swallowing). There are two escape spellings, explicitly shown in the hint:

```
error[E0xx]: この Result は使われていません — エラーが黙って捨てられます
  --> app.almd:3
   |   fail()
   = help: 伝搬するなら fail()!、意図的に捨てるなら let _ = fail()
```

`let _ =` follows the same lineage of “explicit intentional discard” as `(_)` in ADR-0004 D3-(b).

<a id="d3----の特例則は消滅しadr-0005-の定義だけが残る"></a>

### D3. Special rules for `?` / `??` disappear; only ADR-0005's definitions remain

The current interaction rules with the implicit mechanism — “`e()?` acts before auto-?” and “`??` suppresses auto-?” — disappear with that mechanism. An effect call always produces a Result value, so:

```almide
e()?          // ≡ result.to_option(e()) — ADR-0005 D1 そのまま
e() ?? fb     // ≡ unwrap_or_else — 同上。特例なしの普通の値演算
```

<a id="d4-移行は-e040-窓方式1-リリースの警告期間"></a>

### D4. Migration uses an E040 window (one release of warnings)

1. Release N: a deprecation warning plus a precise `!` insertion hint at every site where implicit propagation is inserted (the detected classes are the 5 positional classes in Context; matrix probes serve as reproduction tests).
2. Release N+1: switch over (warning → error) and enable D2's must-use rule.
3. Mechanically migrate all .almd files in the repository (stdlib, tools, spec). Sweep the 6 downstream repositories at the same time (a known procedure).
4. At switchover, rewrite CLAUDE.md's codegen test rule (“fs.read_text() inside effect fn must compile without a handwritten ?”) to comply with this ADR.

<a id="d5-adr-0002-d3lift-対等性は自明化"></a>

### D5. ADR-0002 D3 (lift parity) becomes trivial

`-> T!` (pure fallible) and effect fn now have exactly the same propagation ergonomics: “both use explicit `!`.” Read #1103's D3 parity acceptance condition as compliance with this ADR.

<a id="rationale"></a>

## Rationale

- **The patchwork of 21 cells disappears structurally**: “pattern shape determines survival” and “annotations reverse behavior” can only be removed by abolishing the mechanism, not repairing it.
- **One spelling for propagation**: once ADR-0002 D4 designated `!` as propagation-only, a second propagation mechanism (auto-`?`) violated the single-spelling principle (ADR-0004/0005).
- **Prior distribution**: Rust `?` / Swift `try` / Zig `try` — the fully explicit side predominates. Swift works while requiring `try` on every throwing call.
- **Mechanical migration**: just insert `!` as the warning hint says — a class of fixes LLMs handle especially well.
- **Result-as-value consistency**: `?`, `??`, match and `|>` compose without exceptions (ADR-0005's definition table becomes true in every position, unchanged).

<a id="alternatives--検討して却下した案"></a>

## Alternatives — considered and rejected

1. **Complete the move to full implicitness** (exception-language style): in a language where Result exists as a value, the ambiguity between “keep as value / propagate” expands to every position, and annotation-dependent behavior becomes central to the specification. **Rejected**.
2. **Specify the current position list and repair only surprising cells**: documenting pattern-dependent survival does not make it benign. The two mechanisms (auto-? and `!`) also remain. **Rejected**.
3. **Keep auto-? only in statement position** (minimal implicitness): equally guilty of retaining two mechanisms. In statement position, D2 (must-use) + `!` differs by only one character. **Rejected**.
4. **Keep must-use as a warning** (Rust style): swallowed errors are the last hole in the silent-loss family, and with a one-spelling escape hatch (`let _ =`), making it an error does not strand authors. **Rejected** (with a reconsideration clause in Falsifier 3).

<a id="consequences"></a>

## Consequences

- `!` is mechanically added to all existing effect fn code (stdlib, tools, spec, downstream) — a large but simple diff. The warning period avoids a simultaneous cutover.
- More verbosity (equivalent to Swift's `try`): one character per call.
- Track B's 21 cells collapse into one specification line: “effect/fallible calls always produce Result values.” All undocumented exceptions, such as `e()?` interception and `??` suppression, are documented as normal composition under ADR-0005's definitions.
- The design dependency for #1103 (Phase 1) is resolved, allowing work to begin fully.

<a id="falsifier"></a>

## Falsifier

1. **If dojo's MSR significantly degrades after migration** (the additional `!` acts as noise) — re-evaluate minimal implicitness (Alternatives 3).
2. **If the warning period reveals many ambiguous sites that hints cannot mechanically repair** — postpone the switch, design each ambiguous class separately, then resume.
3. **If false positives in must-use errors are measured** (legitimate fire-and-forget actually exists) — soften D2 to a warning.

<a id="references"></a>

## References

- Swift — [Error Handling (explicit `try` on every call)](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/errorhandling/), SE-0413
- Rust — [The `?` operator](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator), [`#[must_use]` / unused_must_use](https://doc.rust-lang.org/std/result/#results-must-be-used)
- Zig — [try](https://ziglang.org/documentation/master/#try)
- Internal: the 21 cells `autoq/*`, `qop/effect-*` and `qq/effect-*` from the 2026-08-05 matrix (probes saved); ADR-0002 / 0004 / 0005 / 0006.
