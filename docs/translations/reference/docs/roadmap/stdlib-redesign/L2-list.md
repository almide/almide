Reader description: Stdlib redesign L2/L3 — list: one spelling per role (window/windows, get_or, shuffle), comparator sort and min_by/max_by, unzip, Float sums, and one boundary rule for slice/chunk/window.
<a id="l2l3-list"></a>

# L2/L3: list

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-list.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-list.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-list.zh-CN.md)

> Non-normative fixed-source historical redesign reader. Pinned original unchanged; no automatic synchronization, new probes, implementation changes or gate execution. Accepted proposals are historical decisions, not independent confirmation they shipped. Preserve layered revisions: initial deliberate omission of comparator min/max versus later R-list-19 addition; original negative slice/count clamping versus sixth-round abort and L3 R-L0-29; Int/Float and String/Bytes overload expansion versus later Numeric/Text generics under R-L0-42. The original summary 67 + 5 = 72 rows is retained despite later additions. F marks and measurements belong to the original October 21 research/dev environment, not this translation. Role counts and inconsistent formulations are not reconciled. Original suggested out-of-range guard checks only the upper bound and is not endorsed as handling negative indices. Original bit-equality and LLM-behavior assertions remain scoped source claims, not new independent proofs. In preserved code placeholders, 型 means type. Commands and proposed fixes are historical text, not execution instructions. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-list.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> Sources: Rust https://doc.rust-lang.org/std/primitive.slice.html (F) / https://doc.rust-lang.org/std/iter/trait.Iterator.html / Go https://pkg.go.dev/slices (F) / Python https://docs.python.org/3/library/functions.html (F) / https://docs.python.org/3/library/stdtypes.html / TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array (F) / Gleam https://gleam-stdlib.hexdocs.pm/gleam/list.html (F) / Elixir https://elixir.hexdocs.pm/Enum.html (F) / OCaml https://ocaml.org/manual/latest/api/List.html (F) / Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.collections/ (`minByOrNull`: https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.collections/min-by-or-null.html (F)) / Swift https://developer.apple.com/documentation/swift/array (F)

Reading conventions (same as L0):
- `mut` (read “mute”) = an argument that rewrites the caller's `var` in place
- `??` (read “null coalescing”) = an operator using the right operand when the left is none
- `|>` (read “pipe”) = an operator inserting the left value as the right function's first argument
- (F) = a cell verified by retrieving official documentation in this round. Unmarked cells come from facts/naming-conventions.md or familiar spellings

Measurement environment: `almide 0.68.0 (dev, 437b8f127)`. Probes: `scratchpad/sr/probe-list/p1.almd`–`p6.almd`.
Usage counts are occurrences of `/usr/bin/grep -rEo '\blist\.<fn>\b'`, listed in spec / tests / docs / stdlib order (upper bounds because definition lines and generated documentation lines are included).

---

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Length | `len` (`length` deprecated) | `len` | `len()` | `len()` | `length` | `length` | `length`/`count` | `length` | `size` | `count` |
| Empty? | `is_empty` | `is_empty` | — | — | — | `is_empty` | `empty?` | `is_empty` (F) | `isEmpty` | `isEmpty` |
| Get by index (Option) | `get` | `get` | — | — | `at` (F) | — | `at`/`fetch` (F) | `nth_opt` (F) | `getOrNull` | — |
| Get with default | `get_or` | — | — | — | — | — | `at(xs,i,d)` | — | `getOrElse` | — |
| First / last | `first`/`last` | `first`/`last` (F) | — | — | `at(0)`/`at(-1)` | `first`/`last` (F) | `at`/`List.first` | — | `firstOrNull`/`lastOrNull` | `first`/`last` |
| Write at index (copy) | `set`/`update` | — | — | — | `with` (F) | — | `List.replace_at` | — | — | — |
| Insert / remove at index (copy) | `insert`/`remove_at` | — | `Insert`/`Delete` (F) | — | `toSpliced` (F) | — | `List.insert_at`/`delete_at` | — | — | — |
| Add / remove at end (in place) | `push`/`pop` (`mut`) | `push`/`pop` | `append` | `append`/`pop` | `push`/`pop` | — | — | — | `add`/`removeLastOrNull` | `append`/`popLast` |
| Remove all (in place) | `clear` (`mut`) | `clear` | `clear()` | `clear` | `length = 0` | — | — | — | `clear` | `removeAll` |
| Concatenate | `+` operator | `concat` (F) | `Concat` (F) | `+` | `concat` | `append` (F) | `++` | `@`/`append` | `plus` | `+` |
| Contains? | `contains` | `contains` (F) | `Contains` (F) | `in` | `includes` (F) | `contains` | `member?` | `mem` (F) | `contains` | `contains` |
| Index of value | `index_of` | `iter().position` | `Index` (F) | `index` | `indexOf` (F) | — | `find_index` (F) | `find_index` (F) | `indexOf` | `firstIndex(of:)` (F) |
| Index by predicate | `find_index` | `position` | `IndexFunc` (F) | — | `findIndex` | — | `find_index` (F) | `find_index` (F) | `indexOfFirst` | `firstIndex(where:)` |
| Find by predicate | `find` | `find` | — | — | `find` | `find` | `find` | `find_opt` | `find` | `first(where:)` |
| Transform / filter | `map`/`filter` | `map`/`filter` | — | Comprehensions | `map`/`filter` | `map`/`filter` | `map`/`filter` | `map`/`filter` | `map`/`filter` | `map`/`filter` |
| Transform, excluding none | `filter_map` | `filter_map` | — | — | — | `filter_map` | — | `filter_map` (F) | `mapNotNull` | `compactMap` (F) |
| First some | `find_map` | `find_map` | — | — | — | `find_map` (F) | `find_value` (F) | `find_map` (F) | `firstNotNullOfOrNull` | — |
| Flatten | `flatten`/`flat_map` | `flatten`/`flat_map` | — | — | `flat` (F)/`flatMap` (F) | `flatten` (F)/`flat_map` (F) | `concat`/`flat_map` (F) | `concat`/`concat_map` (F) | `flatten`/`flatMap` | `joined`/`flatMap` (F) |
| Fold | `fold`/`reduce` | `fold`/`reduce` | — | `functools.reduce` | `reduce` (F) | `fold`/`reduce` | `reduce` | `fold_left` (F) | `fold`/`reduce` | `reduce` (F) |
| Fold with intermediate results | `scan` | `scan` | — | `itertools.accumulate` | — | `scan` | `scan` (F) | — | `runningFold`/`scan` | — |
| Any / all | `any`/`all` | `any`/`all` | `ContainsFunc` (F) | `any`/`all` | `some`/`every` | `any`/`all` | `any?`/`all?` | `exists`/`for_all` (F) | `any`/`all` | `contains(where:)` (F)/`allSatisfy` (F) |
| Count by predicate | `count` | `filter().count()` | — | — | — | `count` (F) | `count/2` (F) | — | `count{}` | `count(where:)` |
| Sort (natural order) | `sort` | `sort` | `Sort` | `sorted` (F) | `toSorted` (F) | `sort(by:)` | `sort` | `sort` (F) | `sorted` | `sorted()` (F) |
| Sort by key | `sort_by` | `sort_by_key` (F) | — | `sorted(key=)` (F) | — | — | `sort_by` (F) | — | `sortedBy` | — |
| Sort by comparator | **Absent** | `sort_by` (F) | `SortFunc` (F) | `cmp_to_key` | `toSorted(cmp)` (F) | `sort(by:)` | `sort/2` | `sort cmp` (F) | `sortedWith` | `sorted(by:)` (F) |
| Reverse | `reverse` | `reverse` | `Reverse` (F) | `reversed` | `toReversed` (F) | `reverse` | `reverse` | `rev` | `reversed` | `reversed()` (F) |
| Minimum / maximum | `min`/`max` | `min`/`max` | `Min`/`Max` (F) | `min`/`max` (F) | — | `max(with:)` (F) | `min`/`max` | — | `minOrNull` | `min()`/`max()` |
| Minimum / maximum by key | **Absent** | `min_by_key` | `MinFunc` (F, comparator) | `min(key=)` (F) | — | — (F) | `min_by`/`max_by` (F) | — | `minByOrNull` (F) | `min(by:)` (F, comparator) |
| Sum / product (Int) | `sum`/`product` | `sum`/`product` | — | `sum` (F) | — | `int.sum` | `sum`/`product` (F) | — | `sum` | — |
| Sum (Float) | **Absent** (`List[Int]` only) | `sum` | — | `sum` (F) | — | `float.sum` | `sum` (F) | — | `sum` | — |
| Remove duplicates | `unique`/`unique_by` | `dedup` (adjacent) | `Compact` (adjacent, F) | `dict.fromkeys` | `new Set` | `unique` (F) | `uniq`/`uniq_by` (F) | `sort_uniq` | `distinct`/`distinctBy` | — |
| Collapse adjacent duplicates | `dedup` | `dedup` | `Compact` (F) | — | — | — | `dedup` (F) | — | — | — |
| Group by key | `group_by` | — | — | `itertools.groupby` | `Object.groupBy` | — | `group_by` | — | `groupBy` | `Dictionary(grouping:)` |
| Split into 2 by predicate | `partition` | `partition` | — | — | — | `partition` | `split_with` | `partition` (F) | `partition` | — |
| Take / drop from start / end | `take`/`drop`/`take_end`/`drop_end`/`tail` | `take`/`skip` | — | `xs[:n]` | `slice` | `take`/`drop` | `take`/`drop` | `take`/`drop` (F) | `take`/`drop`/`takeLast`/`dropLast` | `prefix`/`dropFirst`/`suffix`/`dropLast` |
| While condition holds | `take_while`/`drop_while` | `take_while`/`skip_while` | — | `itertools.takewhile` | — | `take_while` (F) | `take_while` (F) | `take_while` (F) | `takeWhile` | `prefix(while:)` (F) |
| Slice | `slice` | `&v[a..b]`/`get` (F) | `xs[a:b]` | `xs[a:b]` | `slice` (F) | — | `slice` (F) | — | `subList`/`slice` | `xs[a..<b]` |
| Split at index | `split_at` | `split_at` (F) | — | — | — | `split` | `split` | — | — | — |
| Split into groups of n | `chunk` | `chunks` (F) | `Chunk` (F) | `itertools.batched` | — | `sized_chunk` (F) | `chunk_every` (F) | — | `chunked` (F) | — |
| Windows of length n | `window` and `windows` (identical) | `windows` (F) | — | — | — | `window` (F) | `chunk_every(n,1,:discard)` | — | `windowed` | — |
| Pair / combine with function | `zip`/`zip_with` | `zip` | — | `zip` | — | `zip`/`map2` | `zip`/`zip_with` (F) | `combine` (F) | `zip` | `zip` (F) |
| Unpair | **Absent** | `unzip` | — | `zip(*xs)` | — | `unzip` (F) | `unzip` (F) | `split` (F) | `unzip` | — |
| Add indices | `enumerate` | `enumerate` | — | `enumerate` | `entries` | `index_map` | `with_index` | `mapi` | `withIndex` | `enumerated()` (F) |
| Range | `range` | `a..b` | — | `range` | — | `range` | `a..b` | `init` (F) | `until`/`..<` | `a..<b` |
| Stepped range | **Absent** | `step_by` | — | `range(a,b,s)` | — | — | `a..b//s` | — | `step` | `stride(from:to:by:)` |
| Build by repetition / iteration | `repeat`/`iterate` | `repeat` | `Repeat` (F) | `[x]*n` | `Array(n).fill` | `repeat` | `List.duplicate` | `init` (F) | `List(n){}` | `Array(repeating:count:)` |
| Intersperse / join | `intersperse`/`join` | `join` (F) | — | `sep.join` | `join` (F) | `intersperse` | `intersperse`/`join` | — | `joinToString` | `joined(separator:)` (F) |
| Binary search | `binary_search` (`List[Int]` only) | `binary_search` (F) | `BinarySearch` (F) | `bisect` | — | — | — | — | `binarySearch` | — |
| Shuffle | `shuffle` (pure `fn`) and `random.shuffle` (effect) | External `rand` | — | `random.shuffle` | — | `shuffle` | `shuffle` (F) | — | `shuffled` | `shuffled()` (F) |
| Search backward | — | `rposition`/`rfind` | — | — | `lastIndexOf`/`findLast` (F) | — | — | — | `lastIndexOf`/`findLast` | `lastIndex(of:)` (F)/`last(where:)` (F) |
| Group by neighboring condition | — | `chunk_by` (F) | — | `itertools.groupby` | — | `chunk(by:)` (F) | `chunk_by` (F) | — | — | — |
| Sorted? | — | `is_sorted` (F) | `IsSorted` (F) | — | — | — | — | — | — | — |
| Fill all with same value (in place) | — | `fill` (F) | — | — | `fill` (F) | — | — | — | `fill` | — |

<a id="完全性の規則"></a>

### Completeness rules

1. **Collection higher-order function family** (D3): all of list/map/set receive the 7 functions `map`, `filter`, `fold`, `any`, `all`, `count`, `find`. List already has all 7.
2. **Sorting and minimum/maximum family** (R-L0-10): provide the 2 columns “natural order” and “key function `_by`” across all 3 rows `sort`, `min`, `max`. The comparator `_with` column contains only `sort_with`. Omitting `min_with`/`max_with` is intentional (expressible as `xs |> list.sort_with(cmp) |> list.first`; among the 9 languages, only Go, Swift, Gleam and Kotlin provide comparator minima/maxima, with inconsistent spellings).
3. **Pair family**: if `zip` exists, provide its reverse `unzip` (`zip` 6/9, `unzip` 5/9).
4. **Numeric fold family**: make `sum`, `product`, `min`, `max` work for both `Int` and `Float` element types. `min`/`max`/`sort` already dispatch by type; only `sum`/`product` remain holes.
5. **Size-based splitting family** (`chunk`, `window`): standardize names as singular verbs and stop on `n <= 0` as a contract violation (R-L0-06 item 3, R-L0-12).
6. **In-place mutation** (R-L0-04): only 3 list functions are `mut`: `push`, `pop`, `clear`. `set`, `insert`, `update`, `remove_at`, `swap` copy. `with_capacity` is a subjectless constructor, not `mut`. Add no further `mut` functions (`extend`, `fill`, `sort_in_place`).
7. **Intentional omissions**: concatenation is handled by `+`, so no `concat`/`append` functions. Descending order is expressible with `sort_with` or `sort |> reverse`, so no `sort_desc`.

<a id="l2-の判定"></a>

### L2 assessment

- **Duplication**: `window` ≡ `windows` (separate intrinsics, bodies do not differ by a single character: `runtime/rs/src/list.rs:52` and `:257`) → R-list-03. `shuffle` ≡ `random.shuffle` (only one is pure `fn`) → R-list-05. `get_or` ≡ `get ?? d` → R-list-01. `length` ≡ `len` → R-list-02.
- **Should not be in the table**: `bundled_probe` → R-list-04 (#3850).
- **Missing roles (at least 5/9)**: comparator sorting (8/9), key-based minimum/maximum (5/9: Rust/Python/Elixir/Kotlin use keys, Go/Swift comparators) → R-list-06. Unpairing (5/9) → R-list-07. Float sums (5/9) → R-list-08. Stepped ranges (5/9) → R-list-09.
- **Family holes / inconsistent boundaries**: only `slice` returns empty for a negative start; negative n in `chunk` returns the whole list → R-list-10.
- **Out-of-range writes**: `set`, `update`, `swap`, `remove_at` do nothing out of range; `insert` appends for an out-of-range position. Indexed assignment `xs[i] = v` aborts → R-list-13.
- **Old hint table**: E002's “did you mean” table points away from live functions → R-list-11.
- **Not added**: backward search (4/9), `chunk_by` (4/9), `is_sorted` (2/9), `fill` (3/9), `concat` function → R-list-12 (×).
- **Duplication not exposed publicly**: #1460's `_str` twins (120 functions such as `list_contains_str`) are internal implementation names in `stdlib/list_*.almd`; not one appears in the public function table (67 functions in interface JSON). As a spelling issue, this is outside L2. #1460's protocol-conversion decision remains in #1460.

---

<a id="案"></a>

## Proposals

<a id="r-list-00-その場書き換えとコピーの様式を据え置きr-l0-05-の保険をかける---採用"></a>

### R-list-00 Retain the in-place/copy convention and add R-L0-05 insurance — ○ Accepted

- Content: no change. `push`, `pop`, `clear` are `mut` (in place); all other functions copy. The verbs in `list.set`, `list.insert`, `list.update`, `list.remove_at`, `list.swap` (`set`, `insert`, `update`, `remove`) are copying verbs under R-L0-04, matching map/set. Retain `with_capacity`. Out-of-range positions will uniformly abort under separate proposal R-list-13.
- Supporting L0 rules: R-L0-04, R-L0-05.
- (a) Copy-by-default, like Gleam/Elixir/OCaml List. Only the Rust/Python/JS/Kotlin/Swift `push`/`append`/`pop` pattern remains as in-place mutation.
- (b) Both native and wasm already have `mut` write-back paths.
- (c) The danger is writing `list.sort(xs)` or `list.set(xs, 0, 9)` as statements out of habit from other languages. Measurement (`p1.almd`): for `var xs = [3, 1, 2]`, statements `list.sort(xs)`, `list.reverse(xs)`, `list.set(xs, 0, 9)` produce no error and still display `[3, 1, 2]`. Once R-L0-05 lands, all 3 lines stop with E042. Particularly risky list names are `sort`, `sort_by`, `reverse`, `set`, `insert`, `update`, `remove_at`, `swap` (names also used for in-place mutation in Python/JS/Rust/Kotlin).
- (d) No change.
- Proposed gate: R-L0-G's `mut`-verb check fixes list's `mut` functions to only `push`, `pop`, `clear`. Add one `list.sort(xs)` line to the broken side of an R-L0-05 fixture.
- Issue: include in the new R-L0-05 issue.
- Issue: #3928.

<a id="r-list-01-listget_or-を撤去し--に一本化する---採用"></a>

### R-list-01 Remove `list.get_or` and unify on `??` — ○ Accepted

- Content: remove `list.get_or(xs, i, d)`. Replace with `list.get(xs, i) ?? d`.
- Supporting L0 rules: R-L0-06, R-L0-14 (D1).
- (a) Only Elixir `Enum.at(xs, i, d)` and Kotlin `getOrElse` provide default-value versions as functions.
- (b) No impact. Under ADR-0005, `??` desugars to `option.unwrap_or_else`, identically on both targets. Measurement (`p1.almd`): `list.get_or([1,2], 5, 0)` and `list.get([1,2], 5) ?? 0` both yield `0`.
- (c) `??` is the only spelling taught by CHEATSHEET. As `get_or` always evaluates the default first, removing it also removes the need to remember its subtle difference from lazy `??`.
- (d) Removal. `@deprecated(since=15, use="??")`, `removal_epoch = 16` in `[[deprecation]]`. `almide fix` rewrites `list.get_or(a, b, c)` → `list.get(a, b) ?? c`. Hint in `removed_spellings.rs`: “`list.get(xs, i) ?? d`.” Internal twin `list_get_or_str.almd` disappears too. Usage 99 / 9 / 21 / 7 (research 9).
- Proposed gate: add to R-L0-G “0 public functions with names ending in `_or`” (reach 0 in the same window as `option.unwrap_or`, `result.unwrap_or`, `map.get_or`, `bytes.get_or`, `args.option_or`).
- Issue: #3913.

<a id="r-list-02-listlength-を-epoch-16-で撤去する---採用"></a>

### R-list-02 Remove `list.length` at epoch 16 — ○ Accepted

- Content: assign a removal date to `list.length`, already `@deprecated` since dialect 5.
- Supporting L0 rules: R-L0-11, R-L0-14 (D7).
- (a) Length spelling is split (length 4, len 3). Retain `len` under R-L0-00.
- (b) No impact.
- (c) Measurement (`p3.almd`): currently an E052 warning and an `almide fix` rewrite. After removal, an error with a `removed_spellings.rs` hint.
- (d) Removal. Since the existing `@deprecated` dates from dialect 5, keep `since_epoch = 5` in `[[deprecation]]` and add `use="list.len"` and `removal_epoch = 16` (no new since=15 window: the window is already open). `almide fix` rewrites `list.length(xs)` → `list.len(xs)` one-to-one. Post-removal hint: “`list.len(xs)`.” Usage 4 / 7 / 9 / 1.
- Proposed gate: existing synonym-gate vocabulary rule for `len`/`length`/`size`.
- Issue: #3851 A (existing).

<a id="r-list-03-listwindows-を撤去しlistwindow-を残す---採用"></a>

### R-list-03 Remove `list.windows`, retain `list.window` — ○ Accepted

- Content: `list.window` and `list.windows` are identical (bodies at `runtime/rs/src/list.rs:52` and `:257` match; probe `p1.almd` yields identical `[[1, 2], [2, 3], [3, 4]]`). Because they bind to different intrinsics (`almide_rt_list_windows`, `almide_rt_list_window`), the current synonym gate cannot detect them. Keep singular `window`, remove `windows`.
- Supporting L0 rules: R-L0-01, R-L0-00, R-L0-14.
- Reasons for singular:
  - Internal consistency: `list.chunk`, in the same “split by size n” family, is a singular verb. Standardizing on `window` gives the family one form within list.
  - Total changes when carrying consistency through: singular removes `list.windows` (18 / 3 / 10 / 1 = 32) and changes bytes `bytes.chunks` → `bytes.chunk` (6 / 0 / 5 / 1 = 12), total 44 sites. Plural changes `list.window` (6 / 4 / 7 / 1 = 18) and `list.chunk` → `list.chunks` (31 / 3 / 16 / 2 = 52), total 70 sites.
  - (a) Other-language spellings split (Rust `windows` (F), Gleam `window` (F), Kotlin `windowed`). No majority; decide by resolving an internal Almide collision (R-L0-00 item 1).
- (b) No impact: identical bodies.
- (c) With both, an LLM writes `windows` from Rust habit and `window` as a pair with `chunk`. With one, the spelling inferred from `chunk` always matches.
- (d) Removal. `@deprecated(since=15, use="list.window")`, `removal_epoch = 16`, one-to-one `almide fix` rewrite. Usage of `windows`: 18 / 3 / 10 / 1.
- Proposed gate: a fixture verifies synonym-gate rule 6 (different intrinsics in one module sharing a body, #3851 F) can detect this pair.
- Issue: #3851 (existing; add as a list pair in section C).

<a id="r-list-04-listbundled_probe-を公開面から外す---採用"></a>

### R-list-04 Remove `list.bundled_probe` from the public surface — ○ Accepted

- Content: `list.bundled_probe(n) = n + 1` tests the bundled-call path. Remove it from public exposure. Let `list.split_at` and `list.iterate`, which use the same path, cover regressions (`crates/almide-mir/src/pipeline.rs:79` lists all 3). If a dedicated function is essential, rename to `__bundled_probe`, which the checker does not expose publicly.
- Supporting L0 rules: R-L0-01 (no meaningless spelling in the table), R-L0-14 (D10).
- (a) No language's standard library exposes this sort of function.
- (b) No impact. Measurement (`p1.almd`): `list.bundled_probe(4)` is `5`.
- (c) The function table informs the LLM's judgment of “what list has” (#1483). Just one meaningless function adds one plausible spelling.
- (d) Removal. `@deprecated(since=15, use="n + 1")`, `removal_epoch = 16`. The replacement is an expression, not a function, so `almide fix` does not rewrite; it hints “write `n + 1`.” After removal, `removed_spellings.rs` gives the same hint. Usage 5 / 2 / 5 / 0 (all its own tests and generated documentation lines). Regenerate `docs/stdlib/list.md`, `since.toml`, semantics-manifest rows, inspection-ledger rows and IDE-outline snapshots in the same change.
- Proposed gate: none (removal completes it).
- Issue: #3850 (existing).

<a id="r-list-05-listshuffle-を撤去しrandomshuffle-に一本化する---採用"></a>

### R-list-05 Remove `list.shuffle`, unify on `random.shuffle` — ○ Accepted

- Content: `list.shuffle` is pure `fn` but returns different results each run (probe `p4.almd`: pure `fn mix(xs) = list.shuffle(xs)` compiles; two executions produce `[5, 4, 8, 6, 1, 2, 3, 7]` and `[1, 5, 6, 8, 2, 4, 3, 7]`). The equivalent `random.shuffle` is `effect fn`; calling from pure `fn` stops with E006 (`p5.almd`). Remove `list.shuffle`, retain `random.shuffle`.
- Supporting L0 rules: R-L0-01 (one spelling per role), R-L0-13 (environment-reading functions are effect; ADR-0022), R-L0-02 (randomness belongs to `random`, R-L1-04).
- (a) Python has `random.shuffle`, Rust the `rand` crate; many languages place it in randomness modules. Elixir/Kotlin/Swift put it in collections but do not distinguish purity through types.
- (b) wasm gets randomness through `wasi:random` (same path as `random.shuffle`). ADR-0027 individually inspects `list.shuffle` as the “141st host-dependent function.” Removing it eliminates this exception.
- (c) A pure function returning different values for the same input breaks test/cache/fusion assumptions (same expression, same value). When an LLM finds `list.shuffle`, it uses it inside functions without an effect marker.
- (d) Removal. `@deprecated(since=15, use="random.shuffle")`, `removal_epoch = 16`. `almide fix` changes the spelling; if the caller is pure `fn`, E006 guides changing to `effect fn` (the hint says rewriting alone may not suffice). Usage 2 / 2 / 9 / 0 (`random.shuffle`: 12 / 0 / 7 / 1).
- Proposed gate: add to R-L0-G “functions listed as host-dependent in ADR-0027 are `effect fn`.”
- Issue: #3943.

<a id="r-list-06-比較関数の-sort_with-とキー関数の-min_bymax_by-を足す---採用"></a>

### R-list-06 Add comparator `sort_with` and key functions `min_by` / `max_by` — ○ Accepted

- Content: add these 3 functions.
  - `list.sort_with(xs: List[A], cmp: (A, A) -> Int) -> List[A]` — negative `cmp(a, b)` puts a first, 0 preserves original order (stable), positive puts b first.
  - `list.min_by(xs: List[A], f: (A) -> K) -> Option[A]` — element with smallest key `f(x)`; first among tied keys; none when empty. Call `f` exactly once per element (same as `sort_by`).
  - `list.max_by(xs: List[A], f: (A) -> K) -> Option[A]` — analogous; first among tied keys.
- Supporting L0 rules: R-L0-10 (`_by` for key functions, `_with` for comparators), R-L0-06 (fallible retrieval returns Option without suffix), D4/D5.
- (a) Comparator sorting exists in 8/9 languages (Rust `sort_by` (F), Go `SortFunc` (F), JS `toSorted(cmp)` (F), Gleam `sort(by:)`, Elixir `sort/2`, OCaml `sort cmp` (F), Kotlin `sortedWith`, Swift `sorted(by:)` (F)). Comparator results use an integer's sign in Go (F), JS, OCaml (F), Kotlin and Python `cmp_to_key`: 5 cases, also matching existing Almide `bytes.cmp` (-1/0/1). Key minimum/maximum uses key functions in 4: Rust `min_by_key`, Python `min(key=)` (F), Elixir `min_by` (F), Kotlin `minByOrNull` (F). Including Go `MinFunc` (F) and Swift `min(by:)` (F), this role is 6/9. Kotlin `minByOrNull` (F) returns the first element for tied minima, matching this contract.
- (b) Pure computation. Sorting uses existing stable mergesort, producing identical native/wasm order.
- (c) LLMs write comparators for descending or composite-key sorting. Currently they must give `sort_by` a tuple key or negate signs. `min_by` already appears in the rainbow-bridge design memo (`docs/roadmap/on-hold/rainbow-bridge.md:178`).
- (d) Addition. All usage counts 0 (`min_by` has 1 in docs).
- The `compare` functions passed to comparators (`int.compare`, `float.compare`, `string.compare`, `bytes.compare`, etc.) belong in each type's module under R-L0-16.
- Proposed gate: R-L0-G `_by`/`_with` type checks (last `_by` argument is unary, `_with` binary). Matrix test: all 6 cells of `sort`/`min`/`max` × {natural order, `_by`} exist, and `sort_with(xs, cmp) |> list.first` agrees with `min` in natural order.
- Issue: #3924.

<a id="r-list-07-listunzip-を足す---採用"></a>

### R-list-07 Add `list.unzip` — ○ Accepted

- Content: add `list.unzip(xs: List[(A, B)]) -> (List[A], List[B])`.
- Supporting L0 rules: L2 completeness rule 3 (provide the inverse of `zip`), R-L0-00 (`unzip` is the most common spelling, 4/9: Rust, Gleam (F), Elixir (F), Kotlin; OCaml has `split` (F)).
- (a) The role is 5/9 (those 4 plus OCaml).
- (b) Pure computation.
- (c) Currently one must build two lists with `list.fold` or write `list.map` twice. `unzip` is the spelling an LLM readily writes as the counterpart to `zip`.
- (d) Addition. Usage 0.
- Proposed gate: round-trip property test for `zip` and `unzip` (equal lengths restore the originals).
- Issue: #3925.

<a id="r-list-08-listsum-と-listproduct-を-listfloat-でも使えるようにする---採用"></a>

### R-list-08 Make `list.sum` and `list.product` usable with `List[Float]` — ○ Accepted

- Content: dispatch element types of `list.sum` and `list.product` as with `list.sort`, `list.min`, `list.max`. `List[Int]` remains unchanged (wrapping); `List[Float]` performs IEEE addition/multiplication left-to-right (`0.0`/`1.0` when empty). Currently `list.sum([1.5, 2.5])` stops with E005 “expects List[Int] but got List[Float]” (probe `p2.almd`).
- Supporting L0 rules: L2 completeness rule 4, R-L0-02 (subject `List`, so place in `list`; do not create `float.sum`), R-L0-14 item 4 (do not alter existing call meanings).
- (a) Float sums exist in 5/9 (Rust `sum`, Python `sum` (F), Gleam `float.sum`, Elixir `sum` (F), Kotlin `sum`).
- (b) Fixing addition order left-to-right yields bit-identical native/wasm results (ADR-0016 determinism). No Kahan-style correction (no equivalent of Python `math.fsum` added).
- (c) LLMs write `xs |> list.sum` without checking element type. Currently only Float requires rewriting to `list.fold(xs, 0.0, (a, x) => a + x)`.
- (d) Semantic change (only broadens the domain; no existing `List[Int]` result changes. Not repurposing a name). Usage 92 / 51 / 18 / 1 (sum), 14 / 5 / 13 / 1 (product).
- Proposed gate: element type × {sum, product, min, max} matrix (8 Int/Float cells) on native and wasm; add one `spec/wasm_cross/` fixture and contract (new observable behavior, so enter the contract in als first).
- Issue: #3922.

<a id="r-list-09-刻み幅つきの-listrange_step-を足す---採用"></a>

### R-list-09 Add stepped `list.range_step` — ○ Accepted

- Content: add `list.range_step(start: Int, end: Int, step: Int) -> List[Int]`. With `step > 0`, count from `start` while below `end`; with `step < 0`, count down from `start` while above `end`. `step == 0` stops as a contract violation (R-L0-06 item 3). Half-open interval, as with `list.range` (R-L0-12).
- Supporting L0 rules: R-L0-02 (subjectless constructors belong in the result type's module), R-L0-10 (`_by` is reserved for key functions, so not `range_by`), R-L0-12.
- (a) Stepped ranges exist in 5/9 (Rust `step_by`, Python `range(a, b, s)`, Elixir `a..b//s`, Kotlin `step`, Swift `stride(from:to:by:)`). Spellings split, so qualify existing Almide `range` with the argument's meaning.
- (b) Pure computation.
- (c) Currently written as `list.range(0, n) |> list.map((i) => i * 2)` or `list.filter`, inviting boundary mistakes. LLMs tend to write three-argument `list.range` from Python `range(a, b, s)` habits; E002 “did you mean” should direct them to `list.range_step` (R-list-11).
- (d) Addition. Usage 0.
- Proposed gate: fixtures for `range_step(a, b, 1) == range(a, b)` and negative-step boundaries (`range_step(5, 0, -2) == [5, 3, 1]`).
- Issue: #3923.

<a id="r-list-10-境界の扱いをそろえる-slice-の負の開始位置chunkwindow-の-n--0---採用"></a>

### R-list-10 Align boundaries: negative `slice` start, `chunk` / `window` with `n <= 0` — ○ Accepted

- Sixth-round note: clamping negative `slice` starts to 0 was changed to abort by R-L0-29. The `chunk`/`window` treatment of `n <= 0` remains as proposed here.
- Content:
  1. Clamp negative `start` in `list.slice(xs, start, end)` to 0. Measurement (`p6.almd`): for the same `(-1, 2)`, `string.slice("abc", -1, 2)` returns `"ab"`, `list.slice([1, 2, 3], -1, 2)` returns `[]`, and `bytes.slice(..., -1, 2)` has length 0. String matches R-L0-12 (clamp out-of-range indices to endpoints; negative indices are out of range, hence 0) and documented “clamped.” Align list and bytes so all 3 types return the first 2 elements for `slice(-1, 2)` (bytes in L2-bytes' `slice` row; string unchanged).
  2. `list.chunk(xs, n)` and `list.window(xs, n)` stop on `n <= 0` as a contract violation. Currently only `n = 0` stops; `chunk(xs, -1)` returns `[[1, 2, 3]]`, `window(xs, -1)` returns `[]` (probe `p6.almd`).
- Supporting L0 rules: R-L0-12, R-L0-06 item 3.
- (a) Half-open intervals and clamping match Python/JS/Gleam/Elixir. Stopping for nonpositive sizes matches Rust `chunks`/`windows` (panic) and Kotlin `chunked` (exception).
- (b) Align both targets, introducing no wasm/native difference (both currently share the same inconsistency).
- (c) Different results for the same arguments across types invalidate a boundary behavior learned with string when applied to list.
- (d) Semantic change (boundary inputs only; implementation corrected to documented behavior). Negative n in `chunk`/`window` narrows the domain without raising the dialect epoch (only domain narrowing; spelling and acceptance unchanged). Count negative-argument calls in spec/tests and record impact in the PR.
- Proposed gate: put these 2 discrepancies in the first cells of the R-L0-12 matrix (list/string/bytes × `slice`/`take`/`drop`/`take_end`/`drop_end` × boundaries {-1, 0, len, len+1}). Negative counts (`take(xs, -1)` gives all, `drop(xs, -1)` empty) already match in list/string; fix current answers as cells (L2-string completeness rule 1).
- Issue: #3944.

<a id="r-list-11-e002-のもしかして表を生きている関数に向ける---採用"></a>

### R-list-11 Point E002's “did you mean” table toward live functions — ○ Accepted

- Content: fix `ALIASES` in `crates/almide-frontend/src/stdlib.rs:156-230` (suggestions for undefined names). Some list rows currently point away from live functions.
  - `("list", "push", "list.concat ...")` and `("list", "append", "list.concat ...")`: `list.concat` does not exist; `push` is live. → Remove `push` row, change `append` to “`xs + ys` (`list.push` for in-place).”
  - `("list", "count", "list.len")`: `list.count` is live and means something different (predicate count). → Remove row.
  - `("list", "find_index", "list.index_of")`, `("list", "reduce", "list.fold")`, `("list", "tail", "list.drop(xs, 1)")`: all live functions. → Remove rows.
  - Rows to add (other-language spelling → Almide spelling): `sorted`/`toSorted` → `list.sort`; `sorted_by`/`sortWith` → `list.sort_with`; `distinct`/`uniq` → `list.unique`; `windows`/`windowed` → `list.window` (after removal); `chunks`/`chunked` → `list.chunk`; `concat`/`extend` → `xs + ys`; `get_or`/`getOrElse` → `list.get(xs, i) ?? d` (after removal); `min_by_key`/`minBy` → `list.min_by`; three-argument `range` → `list.range_step`. Same mechanism as the string page's `split_whitespace` and other hints in this table.
- Supporting L0 rules: R-L0-01 (“did you mean” must not teach a second spelling), R-L0-14 item 3 (hint for removed spellings).
- (a) Rust and Kotlin compilers also suggest similar names.
- (b) Checker-only change.
- (c) Whether an LLM reaches the right spelling on the first error after using another language's spelling determines modification survival. Directing to nonexistent `list.concat` causes a second error.
- (d) Diagnostic change only; no compatibility problem.
- Proposed gate: test “every `ALIASES` key is undefined and every target function is live” (new `tests/stdlib_alias_table_test.rs`, compare interface JSON). Move removed spellings into `removed_spellings.rs`.
- Issue: #3905.

<a id="r-list-13-範囲外の位置への書き込みを-abort-にする---採用"></a>

### R-list-13 Abort writes to out-of-range positions — ○ Accepted

- Content: index-writing copy functions stop (abort) on out-of-range positions as caller contract violations.
  - `list.set(xs, i, v)`, `list.update(xs, i, f)`, `list.remove_at(xs, i)`: currently do nothing for out-of-range `i`, returning the original list (`stdlib/list.almd` says “no-op if i out of range”). Abort outside `0 <= i < len`.
  - `list.swap(xs, i, j)`: currently no-op if `i` or `j` is out of range; abort if either is.
  - `list.insert(xs, i, v)`: accept positions `0..=len` (`i == len` appends). Currently negative `i` and `i` greater than `len` also append. Abort outside that range.
- Supporting L0 rules: L0 “writes to specified positions” (after R-L0-12, before R-L0-13), R-L0-06 item 3 (abort only for contract violations).
- (a) Rust/Go/Kotlin/Swift stop out-of-range writes with panic/trap, Python/OCaml with exceptions. Only Elixir `List.replace_at` silently does nothing.
- (b) Use the same abort path on both targets (already used by indexed assignment `xs[i] = v`); no difference.
- (c) Currently `list.set(xs, 5, v)` does nothing, `xs[5] = v` aborts. Two behaviors for the same write; no-op writes hide mistakes.
- (d) Semantic change (domain narrowing). Valid-position results do not change, so omit from dialect epoch `breaks` (existing rule: domain narrowing does not trigger an epoch). Names/types unchanged, so neither `@deprecated` nor `almide fix` is needed. Calls relying on out-of-range no-ops become `if i < list.len(xs) then list.set(xs, i, v) else xs`. Count out-of-range-position calls in spec/tests and record impact in the PR.
- Proposed gate: run list/bytes positional-write × boundary {-1, 0, len-1, len, len+1} matrix on native/wasm, checking identical abort positions (same table as bytes `put_<型>`). Observable change, so enter the contract ledger in als first.
- Issue: #3937.

<a id="r-list-14-listtail-を撤去しlistdropxs-1-に一本化する---採用"></a>

### R-list-14 Remove `list.tail`, unify on `list.drop(xs, 1)` — ○ Accepted

- Current: `list.tail(xs: List[A]) -> List[A]` returns the list without its first element, identical to `list.drop(xs, 1)` (empty stays empty).
- Supporting L0 rules: R-L0-01 (a function merely calling another public function once also counts as a second spelling for the same role). L3 retained it without a reason; correct to the rule.
- (a) `tail` exists in OCaml (`List.tl`, exception on empty), Elixir (`tl`, exception on empty), Haskell (`tail`, exception on empty). Kotlin/Swift/Rust/Python/JS use `drop(1)`, `dropFirst()`, `xs[1:]`, `slice(1)`. Gleam uses `list.rest` (Result). Neither spelling nor empty behavior aligns.
- (c) English `tail` can also read as “last element,” easily confused with `list.last`. `drop(xs, 1)` exposes its meaning in its name.
- (d) Removal. Under R-L0-14, `@deprecated(since=15, use="list.drop(xs, 1)")` at epoch 15, removal at epoch 16. `almide fix` rewrites `list.tail(xs)` to `list.drop(xs, 1)`. Add `tail` → `list.drop(xs, 1)` to E002's table (R-list-11).
- Completeness rule: R-L0-01 forwarding detection (#3851 gate) catches `list.tail` and does not leave it allowlisted.
- Issue: #3969.

<a id="r-list-12-多数派に届かない役割は足さない---不採用"></a>

### R-list-12 Do not add roles short of a majority — × Rejected

- Content: do not add:
  - Backward search (`last_index_of`, `find_last`): 4/9 (Rust, JS (F), Kotlin, Swift (F)); expressible with `list.reverse` and `find_index`.
  - Group by neighboring condition (`chunk_by`): 4/9 (Rust (F), Python, Gleam (F), Elixir (F)).
  - Sortedness (`is_sorted`): 2/9 (Rust (F), Go (F)).
  - Fill with one value (`fill`, in-place): 3/9 (Rust (F), JS (F), Kotlin); expressible as `list.repeat(v, list.len(xs))`.
  - Concatenation functions (`concat`, `append`) and in-place multiple-element `extend`: the role is 9/9, but Almide uses `+` (`xs = xs + ys`). Adding functions gives 2 spellings for the same operation (R-L0-01).
  - Descending sort (`sort_desc`), `zip3`, `frequencies`: all below 4/9.
- Supporting L0 rules: same 5/9 threshold as L1's “category addition criteria,” R-L0-01.
- (c) Instead of additions, R-list-11 gives “did you mean” hints for other-language spellings.
- Issue: none.

---

<a id="r-list-15-listbinary_search-を順序のある要素型すべてで使えるようにする---採用"></a>

### R-list-15 Make `list.binary_search` work for every ordered element type — ○ Accepted

- Current: `fn list.binary_search(xs: List[Int], target: Int) -> Option[Int]`. `list.sort`, `min`, `max` work for all ordered types, but searching alone is `Int`-only.
- Proposal: `fn list.binary_search[A](xs: List[A], target: A) -> Option[Int]`. Same order as `list.sort` (IEEE totalOrder for Float). `List[Int]` results do not change.
- Supporting L0 rules: R-L0-16, L2 completeness rule 4 (domain expansion like R-list-08).
- (a) Rust `binary_search` (`T: Ord`), Go `slices.BinarySearch` (`cmp.Ordered`), Python `bisect`, Kotlin `binarySearch` (`Comparable`) do not restrict to one element type.
- (b) Pure computation. Reuse `list.sort`'s ordering implementation.
- (c) Measurement: `list.binary_search(["a","b"], "b")` gives E005 with “convert to Int using `int.parse(s)`,” directing to a meaningless repair.
- (d) Semantic change (domain expansion only; existing calls unchanged, no epoch).
- Completeness rule: list's order-based functions (`sort`, `min`, `max`, `binary_search`) accept the same element types.
- Proposed gate: element type (Int/Float/String/tuple) × {`sort`, `min`, `max`, `binary_search`} matrix on both targets. New observable behavior, so contract in als first.
- Issue: #3969.

<a id="r-list-16-listjoin-を-listbytes-にも使えるようにするbytesjoin-は置かない---採用"></a>

### R-list-16 Support `List[Bytes]` in `list.join` (no `bytes.join`) — ○ Accepted

- Current: `fn list.join(xs: List[String], sep: String) -> String`. No function combines `List[Bytes]` with a separator (`bytes.join` gives E002).
- Proposal: `list.join` accepts sequence types with concatenation `+` (`String`, `Bytes`). `list.join(parts: List[Bytes], sep: Bytes) -> Bytes`. Same mechanism as R-list-08 dispatching `list.sum` over `Int`/`Float`.
- Supporting L0 rules: R-string-01 (join belongs in list), R-L0-01 (adding `bytes.join` splits location of the same role by type), R-bytes-16 (`Bytes` also concatenates with `+`).
- (a) At least 5 languages have separator-based joining: Go `bytes.Join`, Python `bytes.join`, OCaml `Bytes.concat`, Rust `<[V]>::join`, Swift `joined(separator:)`.
- (b) Pure computation.
- (c) Learn `list.join` once, use the same form for strings and bytes.
- (d) Semantic change (domain expansion only); no compatibility impact.
- Completeness rule: all sequence types concatenable with `+` (String/Bytes) can use `list.join` (List concatenation uses `list.flatten`).
- Issue: #3969.

<a id="r-list-17-binary_search_bybinary_search_with-を足す---採用"></a>

### R-list-17 Add `binary_search_by` / `binary_search_with` — ○ Accepted

- Current: sorting has `sort`, `sort_by`, `sort_with`, but searching only `binary_search`. No function searches a list sorted with `sort_by(xs, key)` using that same key.
- Proposal: `fn list.binary_search_by[A, K](xs: List[A], target: K, key: (A) -> K) -> Int?`, `fn list.binary_search_with[A](xs: List[A], target: A, cmp: (A, A) -> Int) -> Int?`. Sorting/searching form matching `_by`/`_with` pairs.
- Supporting L0 rules: R-L0-12 (align families as matrices), L2 completeness rule 4 (ordered functions share element types).
- (a) Rust `binary_search_by`/`binary_search_by_key`, Go `slices.BinarySearchFunc`, Python `bisect(key=)`, Kotlin `binarySearch(comparator)`/`binarySearchBy`.
- (b) Pure computation.
- (c) Sorting with `sort_by` implies searching with `binary_search_by`: names pair predictably.
- (d) Add 2.
- Completeness rule: `binary_search` has the same 3 forms as `sort` (unsuffixed, `_by`, `_with`).
- Issue: #3980.

<a id="r-list-18-listrepeat-を列全体の繰り返しにする---不採用"></a>

### R-list-18 Make `list.repeat` repeat the whole sequence — × Rejected

- Reason: `string.repeat` and `bytes.repeat` repeat whole sequences, while `list.repeat(v, n)` places one value n times. Aligning would change `list.repeat([1, 2], 3)` from `[[1, 2], [1, 2], [1, 2]]` to `[1, 2, 1, 2, 1, 2]` without a type error (R-L0-14 item 4: no silent result changes under the same name). Repeating elements matches Gleam `repeat`, Elixir `List.duplicate`, Kotlin `List(n) { v }`, Swift `Array(repeating:count:)`, and is not outside the majority. L3's “Go slices.Repeat” repeats entire sequences, so it was removed from the comparison cell.
- Issue: none.

<a id="r-list-19-min_withmax_with-を足す---採用"></a>

### R-list-19 Add `min_with` / `max_with` — ○ Accepted

- Current: sort/search have unsuffixed/`_by`/`_with` (R-list-17), but `min`/`max` only unsuffixed and `_by`.
- Proposal: `fn list.min_with[A](xs: List[A], cmp: (A, A) -> Int) -> A?`, `fn list.max_with[A](xs: List[A], cmp: (A, A) -> Int) -> A?`.
- Supporting L0 rules: R-L0-10 (`_with` for comparators), R-L0-12 (matrix-complete families).
- (a) Rust `min_by`/`max_by` (comparators), Kotlin `minWithOrNull`/`maxWithOrNull`, Swift `min(by:)`.
- (d) Add 2.
- Completeness rule (extends R-list-17): all order-based functions {`sort`, `min`, `max`, `binary_search`} have 3 forms: unsuffixed, `_by`, `_with`.
- Issue: #3985.

<a id="r-list-20-listnewlistfrom_listlistto_list-を置く---採用"></a>

### R-list-20 Provide `list.new`, `list.from_list`, `list.to_list` — ○ Accepted

- Content: `fn list.new() -> List[A]` (`[]`), `fn list.from_list(xs: List[A]) -> List[A]` / `fn list.to_list(xs: List[A]) -> List[A]` (return unchanged).
- Reason: requirements of `Container` in the [protocol hierarchy](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md). When generic functions write `T.new()` / `T.from_list(xs)`, `List` must behave like other containers. Swift `Array` also has `init()` and `init(_ s: some Sequence)`.
- (c) For direct use, keep writing `[]`. Guidance (R-L0-19) points to the literal.
- (d) Add 3.
- Issue: #4000.

<a id="l3-関数表"></a>

## L3: Function table

Decision legend: retain / rename / semantic change / remove / add. Other-language columns list only major spellings.

| Function | Signature (current, mut shown) | Major spellings in other languages | Decision | Proposal |
|---|---|---|---|---|
| `len` | `list.len(xs: List[A]) -> Int` | Rust len / Gleam length / Kotlin size / Swift count | Retain | R-L0-11 |
| `length` | `list.length(xs: List[A]) -> Int` (deprecated) | Gleam length / JS length | Remove (→ `list.len`) | R-list-02 |
| `get` | `list.get(xs: List[A], i: Int) -> Option[A]` | Rust get / Kotlin getOrNull / OCaml nth_opt | Retain | — |
| `get_or` | `list.get_or(xs: List[A], i: Int, default: A) -> A` | Kotlin getOrElse / Elixir at(xs,i,d) | Remove (→ `list.get(xs, i) ?? d`) | R-list-01 |
| `set` | `list.set(xs: List[A], i: Int, val: A) -> List[A]` | JS with / Elixir List.replace_at | Semantic change (out of range aborts) | R-list-00、R-list-13 |
| `swap` | `list.swap(xs: List[A], i: Int, j: Int) -> List[A]` | Rust swap (in place)/ Swift swapAt | Semantic change (out of range aborts) | R-list-00、R-list-13 |
| `sort` | `list.sort(xs: List[A]) -> List[A]` | Kotlin sorted / Swift sorted() / Python sorted | Retain | R-list-00 |
| `reverse` | `list.reverse(xs: List[A]) -> List[A]` | Gleam reverse / Kotlin reversed / JS toReversed | Retain | R-list-00 |
| `contains` | `list.contains(xs: List[A], x: A) -> Bool` | Rust contains / JS includes / Gleam contains | Retain | — |
| `enumerate` | `list.enumerate(xs: List[A]) -> List[(Int, A)]` | Rust enumerate / Python enumerate / Swift enumerated | Retain | — |
| `zip` | `list.zip(xs: List[A], ys: List[B]) -> List[(A, B)]` | Rust zip / Gleam zip / OCaml combine | Retain | — |
| `flatten` | `list.flatten(xss: List[List[T]]) -> List[T]` | Rust flatten / Gleam flatten / JS flat | Retain | — |
| `take` | `list.take(xs: List[A], n: Int) -> List[A]` | Gleam take / Kotlin take / Swift prefix | Semantic change (negative counts abort; R-L0-29) | R-L0-12、R-L0-29 |
| `drop` | `list.drop(xs: List[A], n: Int) -> List[A]` | Gleam drop / Rust skip / Swift dropFirst | Semantic change (negative counts abort; R-L0-29) | R-L0-12、R-L0-29 |
| `tail` | `list.tail(xs: List[A]) -> List[A]` | OCaml tl / Elixir tl / Kotlin drop(1) | Remove (→ `list.drop(xs, 1)`) | R-list-14 |
| `unique` | `list.unique(xs: List[A]) -> List[A]` | Gleam unique / Elixir uniq / Kotlin distinct | Retain | — |
| `index_of` | `list.index_of(xs: List[A], x: A) -> Option[Int]` | JS indexOf / Go Index / Swift firstIndex(of:) | Retain | — |
| `last` | `list.last(xs: List[A]) -> Option[A]` | Rust last / Gleam last / Swift last | Retain | — |
| `chunk` | `list.chunk(xs: List[A], n: Int) -> List[List[A]]` | Rust chunks / Kotlin chunked / Gleam sized_chunk | Semantic change (abort for `n <= 0`) | R-list-10 |
| `sum` | `list.sum[A: Numeric](xs: List[A]) -> A` | Rust sum / Python sum / Elixir sum | Semantic change (replace the 2 `Int`/`Float` forms with 1 generic function accepting all numeric types) | R-L0-42 |
| `product` | `list.product[A: Numeric](xs: List[A]) -> A` | Rust product / Elixir product | Semantic change (replace the 2 `Int`/`Float` forms with 1 generic function accepting all numeric types) | R-L0-42 |
| `first` | `list.first(xs: List[A]) -> Option[A]` | Rust first / Gleam first / Kotlin firstOrNull | Retain | — |
| `is_empty` | `list.is_empty(xs: List[A]) -> Bool` | Rust is_empty / Gleam is_empty / Swift isEmpty | Retain | — |
| `min` | `list.min(xs: List[A]) -> Option[A]` | Rust min / Go Min / Kotlin minOrNull | Retain | — |
| `max` | `list.max(xs: List[A]) -> Option[A]` | Rust max / Go Max / Kotlin maxOrNull | Retain | — |
| `join` | `list.join[T: Text](xs: List[T], sep: T) -> T` | Rust join / JS join / Kotlin joinToString / Go strings.Join | Semantic change (replace the 2 `String`/`Bytes` forms with 1 generic `Text` function) | R-L0-42 |
| `map` | `list.map(xs: List[A], f: (A) -> B) -> List[B]` | map in all languages | Retain | — |
| `filter` | `list.filter(xs: List[A], f: (A) -> Bool) -> List[A]` | filter in all languages | Retain | — |
| `find` | `list.find(xs: List[A], f: (A) -> Bool) -> Option[A]` | Rust find / Kotlin find / Swift first(where:) | Retain | — |
| `any` | `list.any(xs: List[A], f: (A) -> Bool) -> Bool` | Rust any / JS some / OCaml exists | Retain | — |
| `all` | `list.all(xs: List[A], f: (A) -> Bool) -> Bool` | Rust all / JS every / Swift allSatisfy | Retain | — |
| `count` | `list.count(xs: List[A], f: (A) -> Bool) -> Int` | Kotlin count{} / Gleam count / Elixir count/2 | Retain | — |
| `flat_map` | `list.flat_map(xs: List[A], f: (A) -> List[B]) -> List[B]` | Rust flat_map / JS flatMap / OCaml concat_map | Retain | — |
| `filter_map` | `list.filter_map(xs: List[A], f: (A) -> Option[B]) -> List[B]` | Rust filter_map / Kotlin mapNotNull / Swift compactMap | Retain | — |
| `fold` | `list.fold(xs: List[A], init: B, f: (B, A) -> B) -> B` | Rust fold / Gleam fold / OCaml fold_left | Retain | — |
| `sort_by` | `list.sort_by(xs: List[A], f: (A) -> B) -> List[A]` | Rust sort_by_key / Kotlin sortedBy / Elixir sort_by | Retain | R-list-06 |
| `take_while` | `list.take_while(xs: List[A], f: (A) -> Bool) -> List[A]` | Rust take_while / Kotlin takeWhile / Swift prefix(while:) | Retain | — |
| `drop_while` | `list.drop_while(xs: List[A], f: (A) -> Bool) -> List[A]` | Rust skip_while / Kotlin dropWhile / Swift drop(while:) | Retain | — |
| `partition` | `list.partition(xs: List[A], f: (A) -> Bool) -> (List[A], List[A])` | Rust partition / Gleam partition / Elixir split_with | Retain | — |
| `reduce` | `list.reduce(xs: List[A], f: (A, A) -> A) -> Option[A]` | Rust reduce / Gleam reduce / Kotlin reduceOrNull | Retain | — |
| `group_by` | `list.group_by(xs: List[A], f: (A) -> B) -> Map[B, List[A]]` | Kotlin groupBy / Elixir group_by / JS Object.groupBy | Retain | — |
| `find_index` | `list.find_index(xs: List[A], f: (A) -> Bool) -> Option[Int]` | JS findIndex / Go IndexFunc / OCaml find_index | Retain | — |
| `update` | `list.update(xs: List[A], i: Int, f: (A) -> A) -> List[A]` | Elixir List.update_at | Semantic change (out of range aborts) | R-list-00、R-list-13 |
| `scan` | `list.scan(xs: List[A], init: B, f: (B, A) -> B) -> List[B]` | Rust scan / Kotlin runningFold / Elixir scan | Retain | — |
| `zip_with` | `list.zip_with(xs: List[A], ys: List[B], f: (A, B) -> C) -> List[C]` | Elixir zip_with / Gleam map2 / Kotlin zip(other){} | Retain | — |
| `unique_by` | `list.unique_by(xs: List[A], f: (A) -> K) -> List[A]` | Elixir uniq_by / Kotlin distinctBy | Retain | — |
| `range` | `list.range(start: Int, end: Int) -> List[Int]` | Python range / Gleam range / Rust a..b | Retain | — |
| `slice` | `list.slice(xs: List[A], start: Int, end: Int) -> List[A]` | JS slice / Python xs[a:b] / Elixir slice | Semantic change (negative positions abort; R-L0-29) | R-list-10、R-L0-29 |
| `insert` | `list.insert(xs: List[A], i: Int, val: A) -> List[A]` | Go slices.Insert / Elixir List.insert_at / JS toSpliced | Semantic change (abort outside `0..=len`) | R-list-00、R-list-13 |
| `remove_at` | `list.remove_at(xs: List[A], i: Int) -> List[A]` | Elixir List.delete_at / Kotlin removeAt / Swift remove(at:) | Semantic change (out of range aborts) | R-list-00、R-list-13 |
| `repeat` | `list.repeat(val: A, n: Int) -> List[A]` | Gleam repeat / Elixir List.duplicate / Kotlin `List(n) { v }` | Retain | — |
| `intersperse` | `list.intersperse(xs: List[A], sep: A) -> List[A]` | Gleam intersperse / Elixir intersperse | Retain | — |
| `windows` | `list.windows(xs: List[A], n: Int) -> List[List[A]]` | Rust windows | Remove (→ `list.window`) | R-list-03 |
| `dedup` | `list.dedup(xs: List[A]) -> List[A]` | Rust dedup / Elixir dedup / Go Compact | Retain | — |
| `take_end` | `list.take_end(xs: List[A], n: Int) -> List[A]` | Kotlin takeLast / Swift suffix | Semantic change (negative counts abort; R-L0-29) | R-L0-12、R-L0-29 |
| `drop_end` | `list.drop_end(xs: List[A], n: Int) -> List[A]` | Kotlin dropLast / Swift dropLast | Semantic change (negative counts abort; R-L0-29) | R-L0-12、R-L0-29 |
| `shuffle` | `list.shuffle(xs: List[A]) -> List[A]` | Python random.shuffle / Kotlin shuffled / Elixir shuffle | Remove (→ `random.shuffle`) | R-list-05 |
| `window` | `list.window(xs: List[A], n: Int) -> List[List[A]]` | Gleam window / Rust windows / Kotlin windowed | Semantic change (abort for `n <= 0`; retained counterpart of `windows`) | R-list-03、R-list-10 |
| `binary_search` | `list.binary_search(xs: List[A], target: A) -> Option[Int]` | Rust binary_search / Go BinarySearch / Kotlin binarySearch | Semantic change (expand element types to all ordered types) | R-list-15 |
| `push` | `list.push(mut xs: List[A], x: A) -> Unit` | Rust push / JS push / Python append | Retain | R-list-00 |
| `with_capacity` | `list.with_capacity(cap: Int) -> List[A]` | Rust Vec::with_capacity / Go make(.., 0, cap) | Retain | R-list-00 |
| `pop` | `list.pop(mut xs: List[A]) -> Option[A]` | Rust pop / Swift popLast / Kotlin removeLastOrNull | Retain | R-list-00 |
| `clear` | `list.clear(mut xs: List[A]) -> Unit` | Rust clear / Python clear / Kotlin clear | Retain | R-list-00 |
| `bundled_probe` | `list.bundled_probe(n: Int) -> Int` | — (absent from every language) | Remove (→ `n + 1`) | R-list-04 |
| `split_at` | `list.split_at(xs: List[T], n: Int) -> (List[T], List[T])` | Rust split_at / Gleam split / Elixir split | Retain | — |
| `iterate` | `list.iterate(seed: T, f: (T) -> T, n: Int) -> List[T]` | Kotlin generateSequence / Elixir Stream.iterate / Rust successors | Retain | — |
| `find_map` | `list.find_map(xs: List[A], f: (A) -> Option[B]) -> Option[B]` | Rust find_map / Gleam find_map / Elixir find_value | Retain | — |
| `sort_with` | `list.sort_with(xs: List[A], cmp: (A, A) -> Int) -> List[A]` (proposal) | Kotlin sortedWith / Go SortFunc / Swift sorted(by:) | Add | R-list-06 |
| `min_by` | `list.min_by(xs: List[A], f: (A) -> K) -> Option[A]` (proposal) | Rust min_by_key / Kotlin minByOrNull / Elixir min_by | Add | R-list-06 |
| `max_by` | `list.max_by(xs: List[A], f: (A) -> K) -> Option[A]` (proposal) | Rust max_by_key / Kotlin maxByOrNull / Elixir max_by | Add | R-list-06 |
| `unzip` | `list.unzip(xs: List[(A, B)]) -> (List[A], List[B])` (proposal) | Rust unzip / Gleam unzip / OCaml split | Add | R-list-07 |
| `range_step` | `list.range_step(start: Int, end: Int, step: Int) -> List[Int]` (proposal) | Python range(a,b,s) / Rust step_by / Kotlin step | Add | R-list-09 |
| `binary_search_by` | (addition) `list.binary_search_by(xs: List[A], target: K, key: (A) -> K) -> Option[Int]` | Rust binary_search_by_key / Kotlin binarySearchBy | Add | R-list-17 |
| `binary_search_with` | (addition) `list.binary_search_with(xs: List[A], target: A, cmp: (A, A) -> Int) -> Option[Int]` | Rust binary_search_by / Go BinarySearchFunc | Add | R-list-17 |
| `min_with` | (addition) `list.min_with(xs: List[A], cmp: (A, A) -> Int) -> Option[A]` | Kotlin minWithOrNull / Swift min(by:) | Add | R-list-19 |
| `max_with` | (addition) `list.max_with(xs: List[A], cmp: (A, A) -> Int) -> Option[A]` | Kotlin maxWithOrNull / Swift max(by:) | Add | R-list-19 |
| `new` | (addition) `list.new() -> List[A]` | Swift `Array()` / Rust `Vec::new` | Add | R-list-20 |
| `from_list` | (addition) `list.from_list(xs: List[A]) -> List[A]` | Swift `Array(seq)` | Add | R-list-20 |
| `to_list` | (addition) `list.to_list(xs: List[A]) -> List[A]` | Swift `Array(seq)` / Kotlin `toList` | Add | R-list-20 |

Rows: 67 current functions + 5 additions = 72 rows.

---

<a id="l0-への差し戻し処理済み"></a>

## Returned to L0 (processed)

1. **Comparator spelling and type.** L0 gained R-L0-16 (ordered types provide `compare(a, b) -> Int`). `list.sort_with` accepts the R-L0-16 function shape (R-list-06). Rename `bytes.cmp` to `bytes.compare` (L2-bytes R-bytes-08). Add `int.compare`, `float.compare`, `string.compare` on their respective pages.
2. **Spelling of the “split by size n” family.** R-L0-01 gained “functions splitting into n-item groups use the singular verb `chunk`.” Keep `list.window` (R-list-03); rename `bytes.chunks` to `bytes.chunk` (L2-bytes R-bytes-08).
3. **Duplication between `string.join` and `list.join`.** Under R-L0-02 (place in the subject type's module), keep `list.join`, remove `string.join` (L2-string R-string-01).
