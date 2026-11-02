<a id="adr-0009-function-types-complete-the-capability-quadrant-fn-params-are-transparent-by-default"></a>

# ADR-0009: Function types complete the capability quadrant; fn params are transparent by default

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](0009-fn-type-quadrant-transparency.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0009-fn-type-quadrant-transparency.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0009-fn-type-quadrant-transparency.zh-CN.md)

> Non-normative reader translation of the [mixed Japanese/English canonical source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0009-fn-type-quadrant-transparency.md), pinned at 35297a612fb29e3f78f6587aba253aa91d51f466. This snapshot is not automatically synchronized. The original remains authoritative; normative statements belong to the original, not this reader. Historical implementation, compatibility, soundness and measurement claims are reproduced without independent execution or validation. Code blocks and comments remain verbatim.

- **Status**: Accepted
- **Date**: 2026-08-06
- **Decision scope**: Representation of effect / fallibility in fn types (function types in parameter positions) — introducing four quadrants, the default semantics of plain fn-typed parameters, uses of explicit slots, and closing the effect-laundering hole
- **Related**: [ADR-0002](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0002-fallibility-effect-orthogonal.md) (four declaration-side quadrants — this ADR mirrors them on the type side), [ADR-0006](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0006-fallibility-polymorphic-hofs.md) (one-bit polymorphism of the `!` bit — this ADR completes it as two bits), [ADR-0008](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0008-explicit-propagation-only.md) (all explicit — establishes the meaning of `!` inside lambdas), #1055 (this ADR supplies its design), #1051 / #489 (the current contextual-qualification model)
- **Background**: On 2026-08-06, a deep dive into the 15 cells tagged #1055 in the error-surface matrix. ADR-0006/0008 had already resolved 10 cells; yes/no rulings resolve the remaining 5 (including the effect-laundering soundness bug).

<a id="context"></a>

## Context

The current model (fixed in #1051): **a lambda inherits the effect qualification of its enclosing fn** (contextual qualification). Because fn types do not carry qualification, two defects have been observed at the boundary:

```almide
// 1. 健全性バグ: pure fn からコンビネータ経由で効果が実行できる(ロンダリング)
effect fn g(x: Int) -> Result[Int, String] = ...   // fs を触る
fn main() -> Unit = println("${list.try_map([1, 9], g)}")
// 実測: コンパイル通過・効果実行・exit 0。purity.rs の最適化前提を破る

// 2. 非対称な拒否: 同じ effect fn を auto-lift 形(-> Int)で渡すと
//    今度は E005 で拒否され、しかもエラーが lift 前の型を表示して意味不明
```

There is also no way to express in a type an API such as http.serve that wants to fix a handler's qualification together with its runtime contract (Err→500) (the original subject of #1055).

The remainder of the 15 cells (ADR-0008 already resolved the 6 auto-? cells, and ADR-0006 the 4 cells concerning prohibition of `!` inside lambdas).

<a id="decision"></a>

## Decision

<a id="d1-fn-型は4象限を持つadr-0002-の宣言文法の鏡映"></a>

### D1. fn types have four quadrants (mirroring ADR-0002's declaration syntax)

```almide
(A) -> B            // pure ・総
(A) -> B!           // pure ・可謬(ADR-0006)
effect (A) -> B     // effect・総 — 2026-09-24 撤回(ADR-0002 §D6): effect は可謬を含意し、下の行と同義
effect (A) -> B!    // effect・可謬
```

<a id="d2-素の-fn-型パラメータは透過-引数のビットが呼び出しへ流れる"></a>

### D2. Plain fn-typed parameters are “transparent” — the argument's bits flow into the call

A parameter written `f: (A) -> B` accepts any of the four quadrants, and **the supplied argument's bits flow into the HOF call expression** (the two-bit generalization of ADR-0006's one-bit rule; each bit is independent):

```almide
fn apply(f: (Int) -> Int, x: Int) -> Int = f(x)

apply((n) => n * 2, 5)               // pure 総 → apply(...) も pure 総
apply((n) => int.parse(s)! + n, 5)   // 可謬   → apply(...)! が要る
apply((n) => count_files(n)!, 5)     // effect → apply(...) は effect 呼び出し
```

This closes the laundering hole **at the right place**: `try_map([1,9], g)` receives g's effect bit and becomes an effect call, so E006 appears **at the call site** inside pure main. Effectful map lambdas inside effect fns (legal code today) remain legal unchanged — **zero migration breakage**.

<a id="d3-明示スロットは資格の要求固定"></a>

### D3. Explicit slots “require and fix qualification”

A lambda checked against an explicitly written `effect (A) -> B!` slot gains that qualification's body ergonomics (effect calls and `!` are allowed). APIs with runtime contracts declare them this way:

```almide
effect fn serve(port: Int, f: effect (HttpRequest) -> HttpResponse!) -> Unit
// handler の err はランタイム契約(almide_http_serve)により 500 応答へ
```

<a id="d4-象限の上方包摂は暗黙無損失"></a>

### D4. Upward inclusion of quadrants is implicit (lossless)

Passing a pure lambda to an `effect` slot, or a total lambda to a `!` slot, is implicitly legal (using fewer qualifications than required is always safe — analogous to Swift's non-throwing closure fitting a throws slot). This applies ADR-0003's lossless principle.

<a id="d5-実装の段階は-1108-に接続"></a>

### D5. Implementation stages connect to #1108

The `!` bit = #1108 (Phase 2). The effect bit is added as Phase 3 of the same mechanism. At the Phase 3 transition, #1051's contextual-qualification rule is replaced by D2's type rule (D2 causes no breakage of observable behavior). #489's invariant (propagation does not cross closure boundaries) remains — `!` inside a lambda falls into the lambda's own channel (ADR-0006).

<a id="rationale"></a>

## Rationale

- **Completing the axes**: 0002 established two axes on declarations; 0006 put one bit into types. There is no reason not to add the other; the contextual-qualification model without it has demonstrably let a soundness bug through.
- **Transparent defaults follow Swift rethrows' proven shape**: “implicitly flow the bit” is a design that has held up for 10 years. All existing code survives with no annotation obligation.
- **The closed two-bit position**: Between Swift (one bit, effects unrestricted) and Koka / OCaml 5 (row polymorphism, powerful but heavy inference and a sparse prior). The only point that fits predictability-first almide.
- **Soundness**: “Pure fns do not execute effects,” which purity.rs trusts, is guaranteed by types.

<a id="alternatives--検討して却下した案"></a>

## Alternatives — considered and rejected

1. **Strict default** (plain fn type accepts only pure-total): all existing effectful HOF lambdas break, requiring migration to annotate every HOF. **Rejected**.
2. **Keep the contextual-qualification model and patch individual holes**: laundering and asymmetric rejection are structural products of this model; corner cases keep appearing despite patches. **Rejected**.
3. **Row polymorphism (Koka-style)**: maximum S cost; inference errors are out-of-distribution. **Rejected**.
4. **Keep the effect bit exclusive to named fns rather than putting it on fn types**: http.serve's handler cannot be expressed in a type, leaving the original #1055 problem unsolved. **Rejected**.

<a id="consequences"></a>

## Consequences

- The type grammar gains fn types with an `effect` prefix and `!` suffix (parser, checker, mono, native ⇄ wasm). Generic HOFs have up to four monomorphized states.
- #1051's contextual-qualification rule is absorbed into the type rule in Phase 3 (behavior-compatible).
- The laundering hole (#1055 cells) becomes a check error in Phase 3 — until then it is recorded in #1055 as a known soundness bug.
- Handler types for runtime-contract APIs such as http.serve become declarable.

<a id="falsifier"></a>

## Falsifier

1. **If transparency makes it impossible to assume callback purity inside generic HOFs, breaking the performance ratchet of the walled-real baseline** — retreat to explicit opt-in polymorphism (a rethrows-keyword style).
2. **If four-state monomorphization significantly increases the binary-size budget (measurements in the minigit 418KB series)** — move the implementation toward sharing states (a single implementation in Result form).
3. **If distinguishing transparent and explicit slots becomes a significant source of errors in dojo** — restrict explicit slots to stdlib internals (runtime-contract APIs).

<a id="references"></a>

## References

- Swift — [rethrows](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/declarations/#Rethrowing-Functions-and-Methods), SE-0413 (inclusion from non-throwing into a throws slot)
- Koka — [Effect types](https://koka-lang.github.io/koka/doc/book.html#sec-effect-types), OCaml 5 — effect handlers (the row-side comparison)
- Internal: #1055 (original problem sketch), #1051 / #489 (fixed points of the current model), the `purity/*` and `effectctx/*` cells of the 2026-08-05 matrix (laundering probes saved), ADR-0002 / 0003 / 0006 / 0008

<a id="implementation-note--the-effect-bit-through-values-4151-2026-10-25"></a>

## Implementation note — the effect bit through values (#4151, 2026-10-25)

D2's laundering hole was still open for named effect fns used as values
(`list.map(xs, fs.read_text)`, a user HOF, a record field, a closure handed
down through pure helpers). It is closed as follows; the choices below are the
conservative ones where this ADR is silent.

1. **Naming an effect fn as an argument is an effect call** (D2's rule, at the
   checker): a bare name already was E006 from a pure fn; a module path
   (`fs.read_text`) now is too. An explicit `effect` slot (D3) is exempt.
2. **The bit rides the value, not `Ty::Fn`.** D5 planned the bit as a type
   bit. It is carried instead by the value-flow analysis that already carries
   ADR-0026's category sets (`crates/almide-codegen/src/effect_flow`), as one
   more bit of the same mask: an `effect fn`'s own node and every unscanned
   stdlib `effect fn` call set it. That follows a value through record fields,
   lists, options and closures (ADR-0026 D1: a stored value keeps its set)
   with no new surface syntax and nothing for monomorphisation to multiply.
3. **E109.** A pure fn whose solved set holds the bit concretely — through a
   value it reads, not through a fn-typed parameter of its own (that is D2's
   transparency, charged at its call sites) — is refused, with the path. A
   record field of plain fn type is *not* transparent: the pure fn that runs
   it is refused, the effect fn that stored it is not.
4. **Soundness over precision.** A value leaving the tracked positions is
   pooled per fn signature, and a call of a pooled value is charged with the
   pool; a pure fn reading a fn from a list is refused if any fn of that
   signature that runs an effect fn was put in a list anywhere. Measured on
   the repo (spec/, stdlib/, examples/, tests/: 3,459 files, build route; all
   5,102 `.almd` files, `check`): no legitimate program is refused; two
   fixtures that passed `process.exec` from a pure `main` were made `effect`.
5. **Abort is not an effect.** `process.exit` (bodyless, `-> Never`) does not
   carry the bit — ADR-0022's abort, which the lowered `assert` arm calls.
   Read off the declaration, not a name list.
6. **Not changed:** an `effect (A) -> B` value passed to a plain `(A) -> B`
   parameter still passes check (unify ignores `is_effect`) and then fails
   rustc (E0271) on the two return representations. That is a typing gap of
   its own, not laundering: the analysis charges the call site.

## Source caveats

The original four-quadrant presentation and zero-migration-breakage claims describe the decision at its historical stage. D1's total-effect quadrant was withdrawn on 2026-09-24 (ADR-0002 §D6), as the retained code comment states. The 2026-10-25 #4151 addition replaces D5's planned type bit with value-flow tracking, without additional monomorphization states. Its pooled analysis is conservative; the reported absence of legitimate rejections is limited to the measured repository corpus, not a universal absence of false positives. The E0271 typing gap remains explicitly unresolved; Abort is not Effect. This reader does not reconcile these stages into a stronger current guarantee.

### Translation of the retained Japanese code comments

- Context example: the soundness bug lets effects run from a pure fn through a combinator; g touches the filesystem. The observed probe compiled, executed the effect and exited with 0, breaking purity.rs optimization assumptions. Passing the same effect fn in auto-lift form (returning Int) was instead rejected with E005, whose pre-lift type made the error unclear.
- D1 lists pure-total, pure-fallible (ADR-0006), effect-total, and effect-fallible. The effect-total row was withdrawn on 2026-09-24 under ADR-0002 §D6: effect implies fallibility, making it synonymous with the next row.
- D2's example comments say that a pure-total callback leaves apply pure-total; a fallible callback requires a bang on the apply call; an effectful callback makes apply an effect call.
- D3's handler error becomes a 500 response under the almide_http_serve runtime contract.
