Description: Stdlib redesign L2/L3 — option: retire eager unwrap_or in the shared window, keep collect, and gate the option/result family as one matrix.
<a id="l2l3-option"></a>

# L2/L3: option

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-option.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-option.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-option.zh-CN.md)

> Non-normative dated redesign snapshot, original authoritative and unchanged. No automatic synchronization, new probes or current API verification. Removal windows and usage counts are historical. Eager unwrap_or to lazy ?? is a migration, not a promise that default expressions keep identical evaluation. Preserve Option collect versus removed Result collect, and the source-limited Result-only fallible map. The source says the older ADR fixes E to String while its probe found polymorphic E; do not silently unify epochs. The 14-function table includes the function proposed for removal. Cross-language classifications are source-era claims, not new comparisons. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-option.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md), [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md), shared decisions in [L2-result](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-result.md).
> Sources: Rust https://doc.rust-lang.org/std/option/enum.Option.html (F); Go — (no Option type; comma-ok https://go.dev/ref/spec#Index_expressions); Python — (only `None` and type annotations, https://docs.python.org/3/library/typing.html#typing.Optional); TS/JS — (`?.` and `??`, https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/Nullish_coalescing); Gleam https://gleam-stdlib.hexdocs.pm/gleam/option.html (F); Elixir — (`nil` and `||`, https://hexdocs.pm/elixir/Kernel.html); OCaml https://ocaml.org/manual/latest/api/Option.html (F); Kotlin https://kotlinlang.org/docs/null-safety.html; Swift https://developer.apple.com/documentation/swift/optional.

Reading guide: `??` (null coalescing) uses the right side when the left is none. `?.` (optional chaining) proceeds into the value only for some. `!` (propagation) returns a failure unchanged inside an effect fn. ADR-0005 determines that all desugar (replace one spelling with another) to stdlib functions.

Five of the 9 languages have Option as a type (Rust, Gleam, OCaml, Kotlin's `T?`, Swift's `T?`). Go, Python, JS and Elixir represent absence with `nil`/`None`/`undefined`, using operators or syntax rather than functions. Thus “at least 5/9” in this table effectively means all 5 languages with Option.

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Transform payload | `option.map` / `?.` | `map` (F) | — | — | `?.` | `map` (F) | — | `map` (F) | `?.let {}` | `map` |
| Chain transformations | `option.flat_map` | `and_then` (F) | — | — | `?.` | `then` (F) | — | `bind` (F) | `?.let {}` | `flatMap` |
| Flatten nesting | `option.flatten` | `flatten` (F) | — | — | — | `flatten` (F) | — | `join` (F) | — | — |
| Default (eager) | `option.unwrap_or` (deprecated) | `unwrap_or` (F) | — | `x if x is not None else d` | `??` | `unwrap` (F) | `\|\|` | `value ~default` (F) | `?:` | `??` |
| Default (lazy) | `??` = `option.unwrap_or_else` | `unwrap_or_else` (F) | — | — | `??` | `lazy_unwrap` (F) | `\|\|` | — | `?:` | `??` |
| Test some/none | `option.is_some` / `is_none` | `is_some` / `is_none` (F) | — | `is not None` | `!== undefined` | `is_some` / `is_none` (F) | `!= nil` | `is_some` / `is_none` (F) | `!= null` | `!= nil` |
| Convert to Result | `option.to_result` | `ok_or` (F) | — | — | — | `to_result` (F) | — | `to_result ~none` (F) | — | — |
| Filter by condition | `option.filter` | `filter` (F) | — | — | — | — | — | — | `takeIf {}` | — |
| Pair two values | `option.zip` | `zip` (F) | — | — | — | — | — | — | — | — |
| Another Option on none | `option.or_else` | `or_else` (F) | — | — | — | `lazy_or` (F) | — | — | `?:` | `??` |
| Convert to list | `option.to_list` | `iter` | — | — | — | — | — | `to_list` (F) | `listOfNotNull` | — |
| List[Option] → Option[List] (stop at first none) | `option.collect` | `collect` | — | — | — | `all` (F) | — | — | — | — |
| Same as above while transforming | `option.collect_map` | `map(..).collect()` | — | — | — | — | — | — | — | — |
| Extract payload; stop on none | `?? panic(msg)` | `expect` / `unwrap` (F) | — | — | — | `let assert` | — | `get` (F) | `!!` | `!` |
| Collect only some from List[Option] | `list.filter_map((x) => x)` | `flatten` | — | — | — | `values` (F) | — | `List.filter_map Fun.id` | `filterNotNull` | `compactMap` |
| Inspect payload (side effects) | — | `inspect` (F) | — | — | — | — | — | `iter` (F) | `also` | — |
| Another Option on none (eager) | — | `or` (F) | — | — | — | `or` (F) | — | — | `?:` | `??` |
| Unzip pair | — | `unzip` (F) | — | — | — | — | — | — | — | — |
| Conditional test | — | `is_some_and` (F) | — | — | — | — | — | — | — | — |

<a id="完全性の規則"></a>

### Completeness rules

- Treat option and result as one family (a group of same-shaped functions). Both need these cells: `map`, `flat_map`, `flatten`, `unwrap_or_else`, the two `is_<状態>`, `to_<相手の型>`, `filter`, `zip`, `or_else`, `to_list`. The placeholders mean state and counterpart type.
- Intentional one-sided cells:
  - `collect` / `collect_map` only for option. ADR-0007 D1 removes them from result: `result.partition` collects all errors; `list.map(xs, (x) => f(x)!)!` (ADR-0006) stops at the first error. ADR-0007 D4 keeps option's version because its meaning matches Rust's `collect`.
  - `map_err`, `to_err_option`, `partition` only for result (none carries no payload, so there is no counterpart).
- Defaults use only `??` (R-L0-06); do not keep eager `unwrap_or`.

<a id="l2-の判定"></a>

### L2 verdict

- Duplicate: `option.unwrap_or` has the same role as `??`. Remove it in #1107's window (R-option-01).
- Missing roles reaching 5/9: “extract, stop on none” (5 languages), and “collect only some from List[Option]” (5 languages). Both have existing spellings (`?? panic(msg)`, `list.filter_map((x) => x)`), so add no functions (R-option-03).
- Family holes: none. Put option/result correspondences in a table and protect them with a gate (R-option-04).
- Placement: the first argument of `option.collect` / `collect_map` is List, so R-L0-02's subject rule alone suggests list. But its exception says functions whose meaning depends on container-element type belong to that element's module; placing them in option follows the rule (R-option-02).

<a id="案"></a>

## Proposals

<a id="r-option-01-optionunwrap_or-を-epoch-16-で撤去する---採用"></a>

### R-option-01 Remove `option.unwrap_or` at epoch 16 — ○ Accepted

- Content: `option.unwrap_or(o, d)` remains `@deprecated(since=3, use="??")` without a removal date. Add a `[[deprecation]]` block (`since_epoch = 3`, `removal_epoch = 16`) in `proofs/dialect-epochs.toml`, removing it in the same window as `list.get_or`, `map.get_or`, `result.unwrap_or`. After removal, E002 says “write `o ?? d`; if you need function form, use `option.unwrap_or_else`” (ADR-0005 D4 wording).
- L0 basis: R-L0-06, R-L0-14.
- (a) Rust, Gleam, OCaml have eager-default functions. In Almide, however, `??` is the sole spelling and evaluates only when needed, like Rust's `unwrap_or_else` and Swift/TS `??`.
- (b) No impact: `??` already desugars to `unwrap_or_else` on both targets (C-291).
- (c) With only `??`, writers no longer need to wonder whether evaluation is eager.
- (d) Removal. `almide fix` rewrites `option.unwrap_or(o, d)` → `o ?? d` (implemented in PR #1902). Usage: spec 100 / tests 15 / docs 30 / stdlib 2 (including definitions), research 22. #1107 records 128 occurrences in `.almd`.
- Gate: post-removal E002-hint fixtures (`tests/diagnostics/`). Pass `check-interface-diff.sh --allow-breaking` as a declared breaking change.
- Issue: #1107 (existing).

<a id="r-option-02-optioncollect-と-optioncollect_map-を残し置き場所の例外として登録する---採用据え置き"></a>

### R-option-02 Keep `option.collect` and `option.collect_map`, register the placement exception — ○ Accepted (unchanged)

- Content: change neither names nor locations. `option.collect`, `option.collect_map`, `result.partition` fit R-L0-02's exception: functions whose meaning depends on the container-element type belong in that type's module (L0 itself cites `option.collect` and `result.partition`). For `collect_map`, the callback's returned `Option` determines the result type, so the same exception applies.
- L0 basis: R-L0-02, R-L0-00.
- (a) Rust uses `collect` into Option; Gleam puts `option.all` in option. No language puts it on List.
- (b) Pure computation, identical on both targets.
- (c) Moving to `list.collect` puts an Option-only function in `list`. It also reintroduces the spelling of ADR-0007's removed `result.collect` elsewhere, increasing the chance of confusing it with Rust's `collect`.
- (d) No change. Confirmation: `option.collect_map` substitutes for an Option version of `list.map((x) => f(x)!)`. Today's fallibility-polymorphic `list.map` works only for Result (E005 confirmed in probe-option/p.almd).
- Gate: write the R-L0-G placement check to accept a module-subject type not just as the first-argument type, but also as that argument's element type or the return type's outer type. Do not add a name allowlist.
- Issue: #3949.

<a id="r-option-03-抜けている役割の関数を足さない---不採用"></a>

### R-option-03 Do not add functions for missing roles — × Rejected

- Content: do not add Rust/Kotlin/etc. functions `expect`, `inspect`, eager `or`, `unzip`, `is_some_and`, `values`.
- L0 basis: R-L0-00, R-L0-01, R-L0-06.
- (a) Only “extract, stop on none” and “collect only some” reach 5/9. The first is `o ?? panic("msg")` (probe-option/q.almd confirmed `panic` on the right of `??` type-checks); the latter is `list.filter_map((x) => x)`. `inspect`, `or`, `unzip`, `is_some_and` appear in no more than 4 languages.
- (c) Adding `or` creates both eager `or` and lazy `or_else`, recreating the dual spelling ADR-0005 eliminated with `unwrap_or`. `?? some(9)` is a type error (E001) and directs the writer to `option.or_else`.
- (d) None.
- Gate: add no function. Instead add one `?? panic(msg)` example to CHEATSHEET “Unwrap operators”, using a ```` ```almide check ```` fence checked by `scripts/check-llm-surface.sh`.
- Issue: #3905 (one CHEATSHEET line only).

<a id="r-option-04-option-と-result-の対応を行列のゲートにする---採用"></a>

### R-option-04 Gate option/result correspondence as a matrix — ○ Accepted

- Content: make the completeness rules above one rule in `tests/stdlib_surface_conventions_test.rs`. Check that option's family cells exist in result and vice versa; intentionally one-sided cells (option: `collect`, `collect_map`; result: `map_err`, `to_err_option`, `partition`) go on an allowlist with reasons.
- L0 basis: R-L0-01, R-L0-G, CLAUDE.md “Expand API families as matrices”.
- (a) Rust, Gleam, OCaml have nearly same-named groups for Option and Result.
- (b) No impact.
- (c) Writers infer the other side's name (`option.zip` implies `result.zip`). They match now, but can drift without a gate.
- (d) No change (only a new gate).
- Issue: #3949.

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut shown) | Main spellings elsewhere | Verdict | Proposal |
|---|---|---|---|---|
| `option.map` | `map(o: Option[A], f: (A) -> B) -> Option[B]` | Rust map / Gleam map / OCaml map / Swift map / Kotlin `?.let` | Keep | — |
| `option.flat_map` | `flat_map(o: Option[A], f: (A) -> Option[B]) -> Option[B]` | Rust and_then / Gleam then / OCaml bind / Swift flatMap | Keep | R-L0-10 (`flat_map` spelling fixed by rule) |
| `option.flatten` | `flatten(o: Option[Option[A]]) -> Option[A]` | Rust flatten / Gleam flatten / OCaml join | Keep | — |
| `option.unwrap_or` | `unwrap_or(o: Option[A], default: A) -> A` (deprecated) | Rust unwrap_or / Gleam unwrap / OCaml value / Most: `??` operator | Remove | R-option-01 |
| `option.unwrap_or_else` | `unwrap_or_else(o: Option[A], f: () -> A) -> A` | Rust unwrap_or_else / Gleam lazy_unwrap / Swift・TS `??` | Keep | `??` desugaring target (ADR-0005 D1) |
| `option.is_some` | `is_some(o: Option[A]) -> Bool` | Rust・Gleam・OCaml is_some | Keep | — |
| `option.is_none` | `is_none(o: Option[A]) -> Bool` | Rust・Gleam・OCaml is_none | Keep | — |
| `option.to_result` | `to_result(o: Option[A], e: E) -> Result[A, E]` | Rust ok_or / Gleam to_result / OCaml to_result | Keep | Note: E is polymorphic (custom variant E confirmed in probe-error/p.almd). ADR-0007 D4 saying E is fixed to String is outdated wording |
| `option.filter` | `filter(o: Option[A], f: (A) -> Bool) -> Option[A]` | Rust filter / Kotlin takeIf | Keep | R-option-04 |
| `option.zip` | `zip(a: Option[A], b: Option[B]) -> Option[(A, B)]` | Rust zip | Keep | R-option-04 |
| `option.or_else` | `or_else(o: Option[A], f: () -> Option[A]) -> Option[A]` | Rust or_else / Gleam lazy_or / Kotlin・Swift operator | Keep | R-option-03 |
| `option.to_list` | `to_list(o: Option[A]) -> List[A]` | OCaml to_list / Kotlin listOfNotNull / Rust iter | Keep | — |
| `option.collect` | `collect(xs: List[Option[T]]) -> Option[List[T]]` | Rust collect / Gleam all | Keep | R-option-02 |
| `option.collect_map` | `collect_map(xs: List[T], f: (T) -> Option[U]) -> Option[List[U]]` | Rust map().collect() | Keep | R-option-02 |

(14 functions. No additions.)

Relation to R-L0-05: writing `map` as a statement for side effects, as in `option.map(o, (x) => println(x))`, discards `Option[Unit]` and becomes E042 after R-L0-05. Instead write `match o { some(x) => println(x), none => () }`.

<a id="l0-への差し戻し処理済み"></a>

## Referral back to L0 (resolved)

- R-L0-02's exception type: proposed adding the proviso that functions with first argument `List[Option[T]]` or `List[Result[T, E]]` whose meaning depends on element type (`option.collect`, `option.collect_map`, `result.partition`) belong in the element type's module.
  - Resolution: R-L0-02 now contains that proviso (“functions whose meaning depends on the container-element type belong in that type's module”). R-option-02 relies on it, not an allowlist.
