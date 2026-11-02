<!-- description: Stdlib redesign L2/L3 — result: retire the eager unwrap_or with option's, keep filter / to_err_option / partition, and add nothing the operators already cover -->
<a id="l2l3-result"></a>

# L2/L3: result

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-result.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-result.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-result.zh-CN.md)

> Non-normative dated redesign snapshot. Fixed original authoritative and unchanged; no automatic synchronization, new census or current implementation check. Eager unwrap_or removal and lazy ?? replacement retain the source migration proposal, not a claim of unchanged evaluation of defaults. Cross-language F marks belong to the source investigation. Rejected headings that retain functions are preserved literally; no APIs are removed or renamed by this reader. Historical effect-fn propagation wording does not redefine current language-wide rules. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-result.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md), [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md), paired page [L2-option](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-option.md).
> Sources: Rust https://doc.rust-lang.org/std/result/enum.Result.html (F); Go — two values `(T, error)`, https://pkg.go.dev/errors ; Python — exceptions, https://docs.python.org/3/tutorial/errors.html ; TS/JS — exceptions and Promise, https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Statements/try...catch ; Gleam https://gleam-stdlib.hexdocs.pm/gleam/result.html (F); Elixir — `{:ok, v}` / `{:error, e}` with `with`, https://hexdocs.pm/elixir/Kernel.SpecialForms.html#with/1 ; OCaml https://ocaml.org/manual/latest/api/Result.html (F); Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-result/ (F); Swift https://developer.apple.com/documentation/swift/result

Reading: `??`, null coalescing, uses the right side on err. `?`, option conversion, converts Result to Option. `!`, propagation, returns err unchanged within an effect fn, explicit only under ADR-0008. All desugar into stdlib functions under ADR-0005; the Japanese term for desugaring is read “dattō.”

Five languages have Result as a type: Rust, Gleam, OCaml, Kotlin and Swift. Go uses two values, Python/JS exceptions, Elixir tuples.

<a id="l2-役割の比較"></a>

## L2: Role Comparison

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Transform ok contents | `result.map` | `map` (F) | — | — | `then` | `map` (F) | `with` | `map` (F) | `map` (F) | `map` |
| Transform err contents | `result.map_err` | `map_err` (F) | `fmt.Errorf("%w")` | `raise .. from` | — | `map_error` (F) | — | `map_error` (F) | `recover` (F) | `mapError` |
| Chain transformations | `result.flat_map` | `and_then` (F) | — | — | `then` | `try` (F) | `with` | `bind` (F) | — | `flatMap` |
| Remove nesting | `result.flatten` | `flatten` (F, 1.89) | — | — | — | `flatten` (F) | — | `join` (F) | — | — |
| Default, eager | `result.unwrap_or` (deprecated) | `unwrap_or` (F) | — | — | — | `unwrap` (F) | — | `value ~default` (F) | `getOrDefault` (F) | — |
| Default, evaluated only when needed | `??` = `result.unwrap_or_else` | `unwrap_or_else` (F) | — | — | `catch` | `lazy_unwrap` (F) | — | — | `getOrElse` (F) | — |
| Test ok/err | `result.is_ok` / `is_err` | `is_ok` / `is_err` (F) | `err == nil` | — | — | `is_ok` / `is_error` (F) | — | `is_ok` / `is_error` (F) | `isSuccess` / `isFailure` (F) | — |
| Convert to Option, ok side | `result.to_option` / `?` | `ok` (F) | — | — | — | — | — | `to_option` (F) | `getOrNull` (F) | `try?` |
| Convert to Option, err side | `result.to_err_option` | `err` (F) | — | — | — | — | — | `get_error` (exception-raising) | `exceptionOrNull` (F) | — |
| Split List[Result] into ok and err | `result.partition` | `Iterator::partition` | — | — | `Promise.allSettled` | `partition` (F) | `Enum.split_with` | — | — | — |
| Convert to list | `result.to_list` | `iter` | — | — | — | — | — | `to_list` (F) | — | — |
| Pair two values | `result.zip` | — | — | — | `Promise.all` | — | — | `product` (F) | — | — |
| Another Result on err | `result.or_else` | `or_else` (F) | — | `except` | `catch` | `try_recover` (F) | — | — | `recoverCatching` (F) | `flatMapError` |
| Conditionally turn ok into err | `result.filter` | — | — | — | — | — | — | — | — | — |
| Fold both sides to one value | `match` | `map_or_else` (F) | — | — | — | — | — | `fold` (F) | `fold` (F) | — |
| Extract contents, stop on err | `?? panic(msg)` | `expect` / `unwrap` (F) | — | — | — | `let assert` | — | `get_ok` (exception-raising, F) | `getOrThrow` (F) | `get()` |
| Extract err contents, stop on ok | `match` | `unwrap_err` / `expect_err` (F) | — | — | — | `unwrap_error` (F) | — | `get_error` (F) | `exceptionOrNull` | — |
| Inspect contents, side effects | — | `inspect` / `inspect_err` (F) | — | — | — | — | — | `iter` / `iter_error` (F) | `onSuccess` / `onFailure` (F) | — |

<a id="完全性の規則"></a>

### Completeness Rules

- Preserve option/result correspondence through the same table as [L2-option](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-option.md) completeness rules, R-option-04. Result-only cells: `map_err`, `to_err_option`, `partition`. Absent from result: `collect` / `collect_map`, removed by ADR-0007 D1. Collect all errors with `partition`; stop on first error with `list.map(xs, (x) => f(x)!)!`.
- Error type E defaults to String, ADR-0002 D2. Keep function types polymorphic in E; no String-only functions such as context-prefixing, ADR-0004 D2, see [L2-error](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-error.md).
- Defaults use only `??`, R-L0-06.

<a id="l2-の判定"></a>

### L2 Judgment

- Duplicate: `result.unwrap_or` duplicates the role of `??`; retire in #1107's window, R-result-01.
- Missing roles reaching at least 5/9: extracting or stopping on err, five languages, is expressible as `r ?? panic(msg)`. Folding both sides, extracting err contents and inspecting appear in three or four languages and can use `match`. Add no functions, R-result-04.
- Almide-only shapes: three-argument `result.filter` and name `result.to_err_option`; retain both, R-result-02/03.
- Location: `result.partition` takes List first, but R-L0-02's proviso puts functions where container element types carry meaning in the element-type module. Thus result is correct, R-result-05, same as R-option-02.

<a id="案"></a>

## Proposals

<a id="r-result-01-resultunwrap_or-を-epoch-16-で撤去する---採用"></a>

### R-result-01 Retire `result.unwrap_or` in Epoch 16 — ○ Accepted

- Contents: same as R-option-01. Add `[[deprecation]]` block, `since_epoch = 3`, `removal_epoch = 16`, to `@deprecated(since=3, use="??")`; remove in epoch 16 `breaks`. Post-removal E002 hint: “write `r ?? d`; when using err contents, write `result.unwrap_or_else(r, (e) => ...)`.”
- L0 basis: R-L0-06, R-L0-14.
- (a)–(c) Same as R-option-01.
- (d) Removal. `almide fix` rewrites `result.unwrap_or(r, d)` → `r ?? d`. Usage: spec 27 / tests 6 / docs 16 / stdlib 2 including definitions / research 1.
- Gate: same as R-option-01.
- Issue: #1107, existing.

<a id="r-result-02-resultfilter-の-3-引数を据え置く---不採用撤去も改名もしない"></a>

### R-result-02 Retain Three-Argument `result.filter` — × Rejected, Neither Remove nor Rename

- Contents: retain `result.filter(r, pred, err_val)`.
- L0 basis: R-L0-00, R-L0-14 item 4, no reassignment of meaning.
- (a) Zero languages have filter on Result; verified absent from Rust Result (F).
- (c) Writing two-argument `result.filter(r, pred)` by analogy with option causes an argument-count type error, never silently passes. Without an err value no err can be built, so semantics determine three arguments. Removal only lengthens it to `result.flat_map(r, (x) => if pred(x) then ok(x) else err(e))`, without resolving spelling collision.
- (d) None. Usage: spec 11 / tests 2 / docs 12 / stdlib 1.
- Issue: none.

<a id="r-result-03-resultto_err_option-を改名しない---不採用"></a>

### R-result-03 Do Not Rename `result.to_err_option` — × Rejected

- Contents: no rename toward Rust `err()` or Kotlin `exceptionOrNull`.
- L0 basis: R-L0-00, R-L0-07.
- (a) Spellings split: Rust `err`, Kotlin `exceptionOrNull`, OCaml only exception-raising `get_error`; no majority.
- (c) `to_option` for ok and `to_err_option` for err read as a pair, still in R-L0-07's `to_<先の型>` pattern, meaning conversion to the destination type.
- (d) None. Usage: spec 9 / tests 4 / docs 7 / stdlib 1.
- Issue: none.

<a id="r-result-04-抜けている役割の関数を足さない---不採用"></a>

### R-result-04 Do Not Add Functions for Missing Roles — × Rejected

- Contents: add none of `expect`, `unwrap_err`, `map_or_else` / `fold`, `inspect` / `inspect_err`, eager `and` / `or`.
- L0 basis: R-L0-00, R-L0-01, R-L0-06.
- (a) Only extraction-or-stop-on-err reaches 5/9, expressible as `r ?? panic("msg")`, checked in probe-option/q.almd. `fold` in OCaml/Kotlin plus Rust `map_or_else` totals three languages; `inspect` also three.
- (c) Two-function `fold` argument order, ok-first versus err-first, differs by language and invites mistakes. `match r { ok(v) => .., err(e) => .. }` names the branches.
- (d) None.
- Issue: none.

<a id="r-result-05-resultpartition-を-result-に残す---採用据え置き"></a>

### R-result-05 Keep `result.partition` in result — ○ Accepted, Unchanged

- Contents: do not relocate. Fits R-L0-02's element-type-module proviso, whose text uses `result.partition` as example. Do not add a name allowlist; make R-L0-G's location check inspect the element type, as with R-option-02.
- L0 basis: R-L0-02, R-L0-01.
- (a) Gleam uses `result.partition` (F).
- (c) `list.partition(xs, f)` already splits by a predicate. Moving List[Result] into `list` creates two meanings for one name, violating R-L0-01. ADR-0007 D2 makes `result.partition` the sole implementation for collecting all errors.
- (d) None. Usage: spec 11 / tests 7 / docs 17 / stdlib 2.
- Issue: #3949.

<a id="r-result-06-resultcollect-を戻す---不採用"></a>

### R-result-06 Restore `result.collect` — × Rejected

- Reason: removed by ADR-0007 D1. A fallible element sequence has one spelling: `!` in a `list.map` callback, stops on first err, ADR-0006. Restoring `collect` adds a second spelling for the same role, R-L0-01, and differs from Rust `collect`, a generic container-construction function.
- Issue: none.

<a id="l3-関数表"></a>

## L3: Function Table

| Function | Signature, current, mut shown | Main spellings elsewhere | Judgment | Proposal |
|---|---|---|---|---|
| `result.map` | `map(r: Result[A, E], f: (A) -> B) -> Result[B, E]` | Rust・Gleam・OCaml・Kotlin・Swift map | Keep | — |
| `result.map_err` | `map_err(r: Result[A, E], f: (E) -> F) -> Result[A, F]` | Rust map_err / Gleam・OCaml map_error / Swift mapError | Keep | ADR-0004 D2 canonical form |
| `result.flat_map` | `flat_map(r: Result[A, E], f: (A) -> Result[B, E]) -> Result[B, E]` | Rust and_then / Gleam try / OCaml bind / Swift flatMap | Keep | R-L0-10 |
| `result.unwrap_or` | `unwrap_or(r: Result[A, E], default: A) -> A`（deprecated） | Rust unwrap_or / Gleam unwrap / Kotlin getOrDefault | Remove | R-result-01 |
| `result.unwrap_or_else` | `unwrap_or_else(r: Result[A, E], f: (E) -> A) -> A` | Rust unwrap_or_else / Gleam lazy_unwrap / Kotlin getOrElse | Keep | `??` desugars here, ADR-0005 D1 |
| `result.is_ok` | `is_ok(r: Result[A, E]) -> Bool` | Rust・Gleam・OCaml is_ok / Kotlin isSuccess | Keep | — |
| `result.is_err` | `is_err(r: Result[A, E]) -> Bool` | Rust is_err / Gleam・OCaml is_error / Kotlin isFailure | Keep | — |
| `result.to_option` | `to_option(r: Result[A, E]) -> Option[A]` | Rust ok / OCaml to_option / Kotlin getOrNull | Keep | `?` desugars here |
| `result.to_err_option` | `to_err_option(r: Result[A, E]) -> Option[E]` | Rust err / Kotlin exceptionOrNull | Keep | R-result-03 |
| `result.partition` | `partition(rs: List[Result[T, E]]) -> (List[T], List[E])` | Gleam partition | Keep | R-result-05 |
| `result.flatten` | `flatten(r: Result[Result[A, E], E]) -> Result[A, E]` | Rust・Gleam flatten / OCaml join | Keep | R-option-04 |
| `result.to_list` | `to_list(r: Result[A, E]) -> List[A]` | OCaml to_list / Rust iter | Keep | R-option-04 |
| `result.zip` | `zip(a: Result[A, E], b: Result[B, E]) -> Result[(A, B), E]` | OCaml product | Keep | R-option-04 |
| `result.or_else` | `or_else(r: Result[A, E], f: (E) -> Result[A, F]) -> Result[A, F]` | Rust or_else / Gleam try_recover / Swift flatMapError | Keep | — |
| `result.filter` | `filter(r: Result[A, E], pred: (A) -> Bool, err_val: E) -> Result[A, E]` |  (none in other languages) | Keep | R-result-02 |

15 functions, no additions.

Relation to R-L0-05: a statement for side effects such as `result.map(r, (x) => println(x))` becomes E042 after R-L0-05; current E042 already blocks discarding Result in a statement. Use `match`.

<a id="l0-への差し戻し処理済み"></a>

## Returned to L0, Resolved

- R-L0-02's `result.partition` exception is the same item as [L2-option](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-option.md)'s return to L0. It became a proviso in R-L0-02, on which R-result-05 relies.
