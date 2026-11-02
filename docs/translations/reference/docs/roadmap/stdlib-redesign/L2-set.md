> Description (inactive source index metadata): Stdlib redesign L2/L3 — set: complete the higher-order family with count and find, keep insert/remove as copies, and decline superset and partition
<a id="l2l3-set"></a>

# L2/L3: set

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-set.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-set.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-set.zh-CN.md)

> Non-normative historical redesign snapshot. The pinned original remains authoritative and unchanged. No automatic synchronization, independent rerun or current implementation verification. Preserve the original all-copy/no-mut/no-clear rule versus the later clear mut table row, partition rejection versus its later added row, and the stated 21-row/19+2 counts versus the expanded table. The dated probe and majority claims are source reports, not new measurements; E042 is described conditionally on the proposed rule landing. Original code comments mean: no error occurs and nothing happens; the assigned form is correct. The (F) labels report the original author’s retrieval pass, not a new retrieval for this translation. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-set.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> Sources: Rust https://doc.rust-lang.org/std/collections/struct.HashSet.html (F) / Go https://pkg.go.dev/maps (F) (no set type) / Python https://docs.python.org/3/library/stdtypes.html#set-types-set-frozenset / TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Set (F) / Gleam https://gleam-stdlib.hexdocs.pm/gleam/set.html (F) / Elixir https://elixir.hexdocs.pm/MapSet.html (F) / OCaml https://ocaml.org/manual/latest/api/Set.S.html (F) / Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.collections/-set/ / Swift https://developer.apple.com/documentation/swift/set (F)
> Common verdicts: follow D1–D11 as on the list/map/option/result/string pages ([L2-map correspondence table](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-map.md#共通の判定との対応))

Reading conventions (same as L0):
- `mut` (read “mute”) = an argument that rewrites the caller's `var` in place
- `|>` (read “pipe”) = pass the left-hand value as the right-hand function's first argument

There are currently 19 public functions (interface JSON, with `mut` notation). Not 1 function has a `mut` argument. All set operations are copying operations.

---

<a id="l2-役割の比較"></a>

## L2: Role comparison

(F) marks cells verified by fetching official pages in this pass. Go has no set type and substitutes `map[T]struct{}` (all rows —).

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Create empty / from list | `set.new`・`set.from_list` | `new`・`from_iter` | — | `set()`・`set(xs)` | `new Set(xs)` (F) | `new`・`from_list` (F) | `new/0`・`new/1` (F) | `empty`・`of_list` (F) | `setOf`・`toSet` | `Set(xs)` |
| Add | `set.insert`（copy） | `insert` (F, in-place) | — | `add`（in-place） | `add` (F, in-place) | `insert` (F, copy) | `put` (F, copy) | `add` (F, copy) | `add`（in-place）/ `s + x` | `insert` (F, mutating) |
| Remove | `set.remove`（copy） | `remove` (F) | — | `remove`・`discard` | `delete` (F) | `delete` (F) | `delete` (F) | `remove` (F) | `remove` / `s - x` | `remove` (F) |
| Membership | `set.contains` | `contains` (F) | — | `x in s` | `has` (F) | `contains` (F) | `member?` (F) | `mem` (F) | `contains` | `contains` (F) |
| Count / emptiness | `set.len`・`set.is_empty` | `len`・`is_empty` (F) | — | `len` | `size` (F) | `size`・`is_empty` (F) | `size` (F) | `cardinal`・`is_empty` (F) | `size`・`isEmpty` | `count`・`isEmpty` |
| To list | `set.to_list` | `iter().collect` | — | `list(s)` | `[...s]` | `to_list` (F) | `to_list` (F) | `to_list`・`elements` (F) | `toList` | `Array(s)` |
| Union / intersection / difference / symmetric difference | `union`・`intersection`・`difference`・`symmetric_difference` | Same 4 (F) | — | Same 4 | `union`・`intersection`・`difference`・`symmetricDifference` (F) | Same 4 (F) | Same 4 (F) | `union`・`inter`・`diff` (F) | `union`・`intersect`・`subtract` | `union`・`intersection`・`subtracting`・`symmetricDifference` (F) |
| Subset predicate | `set.is_subset` | `is_subset` (F) | — | `issubset` | `isSubsetOf` (F) | `is_subset` (F) | `subset?` (F) | `subset` (F) | `b.containsAll(a)` | `isSubset(of:)` (F) |
| Superset predicate | — | `is_superset` (F) | — | `issuperset` | `isSupersetOf` (F) | — (F) | — (F) | — (F) | `containsAll` | `isSuperset(of:)` (F) |
| Disjointness | `set.is_disjoint` | `is_disjoint` (F) | — | `isdisjoint` | `isDisjointFrom` (F) | `is_disjoint` (F) | `disjoint?` (F) | `disjoint` (F) | — | `isDisjoint(with:)` (F) |
| Filter / map / fold | `set.filter`・`set.map`・`set.fold` | `retain`・`iter().map`・`fold` | — | Comprehension | — | `filter`・`map`・`fold` (F) | `filter` (F)・`Enum.map` | `filter`・`map`・`fold` (F) | `filter`・`map`・`fold` | `filter`・`map` (F)・`reduce` |
| any / all | `set.any`・`set.all` | `iter().any`・`all` | — | `any()`・`all()` | — | — (F) | `Enum.any?`・`all?` | `exists`・`for_all` (F) | `any`・`all` | `contains(where:)`・`allSatisfy` |
| Count by predicate | — | `iter().filter().count()` | — | `sum(...)` | — | — (F) | `Enum.count/2` | — (F) | `count {}` | `count(where:)` (F) |
| Find 1 by predicate | — | `iter().find` | — | `next(...)` | — | — (F) | `Enum.find` | `find_first`（monotonic predicates only）(F) | `find` | `first(where:)` (F) |
| Split in 2 by predicate | — | — | — | — | — | — (F) | `split_with` (F) | `partition` (F) | `partition` | — |

<a id="完全性の規則"></a>

### Completeness rules

1. **Higher-order family (shared by list/map/set)**: all 3 collections provide the 7 functions `map`, `filter`, `fold`, `any`, `all`, `count`, `find` (common verdict D3, [L2-map R-map-06](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-map.md)). Set lacks `count` and `find` → R-set-01.
2. **Basic set of operations (shared by list/map/set)**: `new`, `from_list`, `len`, `is_empty`, `contains`. Set has all of them.
3. **Set-operation family**: 4 operations producing a set from 2 sets (`union`, `intersection`, `difference`, `symmetric_difference`) and 2 Bool comparisons of 2 sets (`is_subset`, `is_disjoint`). Intentionally omit superset `is_superset`, since it is the same predicate as `is_subset` with arguments swapped (R-set-02).
4. **Mutation style**: all public set functions use copying, and none have `mut` arguments. `insert` / `remove` are copying verbs (R-L0-04). Set has no operations requiring in-place verbs (`push`, `pop`, `clear`).
5. **set.map property**: overlapping mapped results reduce element count (measured: mapping `{1, 2, 3}` with `x % 2` produces `[1, 0]`). This follows the definition of a set and is unchanged.

<a id="l2-の判定"></a>

### L2 verdict

- **Family gaps**: `set.count` and `set.find` are missing → R-set-01. Writing `set.count` currently yields E002 with a hint to “use `set.len`” (measured). This comes from the `("set", "count", "set.len")` row in the `ALIASES` table (`crates/almide-frontend/src/stdlib.rs:162`) and steers a writer who wants a predicate count toward the wrong function. R-set-01 removes this row.
- **Missing roles (at least 5/9)**:
  - Superset predicate: only Rust, Python, JS and Swift have dedicated names, 4/9 (Kotlin's `containsAll` is a general collection function) → R-set-02 (×).
  - Split in 2 by predicate: Elixir, OCaml, Kotlin, 3/9 → R-set-03 (×).
- **Spelling conflicts**: none. `insert` has Rust/Gleam/Swift, 3/9; `add` has Python/JS/OCaml/Kotlin, 4/9. Without a majority, retain the current spelling (R-L0-00). `remove` matches the majority: Rust/Python/OCaml/Kotlin/Swift, 5/9.
- **Silently discarded calls (R-L0-05)**: the exact measured example from L0. It still silently passes on both native and wasm as below (2026-10-21, `almide 0.68.0 (dev, 437b8f127)`, `scratchpad/sr/probe-set/s.almd`):

  ```almide
  var s = set.new()
  set.insert(s, 1)     // エラーにならない。何も起きない
  println(int.to_string(set.len(s)))   // 0
  s = set.insert(s, 1) // 正しい書き方
  ```

  Since all set functions copy, writing `set.insert`, `set.remove`, `set.union`, `set.filter` as statements always discards the result. Python/JS/Rust/Kotlin/Swift `add`/`insert` mutate in place, making this the module most prone to LLM mistakes. Once R-L0-05 lands, E042 will stop this. Do not propose renaming on the set side (R-L0-04 already defines these as copying verbs).

---

<a id="案"></a>

## Proposals

<a id="r-set-01-setcount-と-setfind-を足す---採用"></a>

### R-set-01 Add `set.count` and `set.find` — ○ Accepted

- Content:
  - `set.count(s: Set[A], f: (A) -> Bool) -> Int` — number of elements satisfying `f`; 0 if empty.
  - `set.find(s: Set[A], f: (A) -> Bool) -> Option[A]` — the first element in insertion order satisfying `f`, or none if absent. Do not call `f` after finding one (same as `list.find`).
  - Write both in pure Almide in `stdlib/set_core.almd` and register them in `self_host_registry.rs`.
  - Remove `("set", "count", "set.len")` (`stdlib.rs:162`) from `ALIASES`. Add `("set", "add", "set.insert")` / `("set", "has", "set.contains")` to the same table (neither exists as of 2026-10-21; same mechanism as R-map-03).
- Supporting L0 rules: R-L0-01 (same spelling for the same role; same names and callback forms as list/map `count` / `find`), R-L0-10, CLAUDE.md's “extend families as matrices.”
- (a) Languages with dedicated names: for counting, Kotlin `count {}`, Elixir `Enum.count/2`, Swift `count(where:)` (F); for finding, Kotlin `find`, Elixir `Enum.find`, Swift `first(where:)` (F). Neither reaches 5/9. This addition nevertheless completes an internal Almide family (completeness rule 1), rather than applying L1's criterion for adding a new role. If list/map have them but set does not, LLMs infer the rule, write them and fail.
- (b) Pure computation. No host functionality needed. Runs in the same order (insertion order) on native and wasm.
- (c) One can write `s |> set.count(f)` in the same form as `xs |> list.count(f)` and `m |> map.count(f)`. The current “use `set.len`” hint encourages incorrectly discarding the predicate.
- (d) Addition, no compatibility problem. Usage count: 0 (new spelling). No calls writing `set.count` and getting E002 occur in spec / tests / docs / stdlib.
- Proposed gate: L2-map R-map-06's collection-family matrix test requires these 2 set cells. Put boundary tests (empty, no matches, all match) in `spec/stdlib/set_test.almd`, and add a bridge to almide-interp's 3-way oracle.
- issue: #3901

<a id="r-set-02-setis_superset-を足す---不採用"></a>

### R-set-02 Add `set.is_superset` — × Rejected

- Content: do not add. Write whether `a` is a superset of `b` as `set.is_subset(b, a)`.
- Supporting L0 rules: R-L0-01 (do not give 2 spellings to the same predicate with arguments merely swapped), L1 addition criterion (at least 5/9).
- (a) Dedicated names exist in Rust `is_superset` (F), Python `issuperset`, JS `isSupersetOf` (F), Swift `isSuperset(of:)` (F), 4/9. Gleam/Elixir/OCaml have only the subset predicate (F).
- (c) For writers who enter `set.is_superset`, add `("set", "is_superset", "set.is_subset (arguments swapped)")` to `ALIASES`.
- (b)(d) No change (hint only).
- issue: add the hint in the same issue as R-set-01.

<a id="r-set-03-setpartition-を足す---不採用"></a>

### R-set-03 Add `set.partition` — × Rejected

- Content: do not add. It can be written as `(set.filter(s, f), set.filter(s, (x) => not f(x)))`.
- Supporting L0 rules: L1 addition criterion (at least 5/9), completeness rule 1 (`partition` is not one of the family's 7 functions).
- (a) Elixir `split_with` (F), OCaml `partition` (F), Kotlin `partition`, 3/9.
- (b)(d) No change.
- issue: none

<a id="r-set-04-setinsert-を-setadd-に改名する---不採用"></a>

### R-set-04 Rename `set.insert` to `set.add` — × Rejected

- Content: retain it.
- Supporting L0 rules: R-L0-00 (do not change without a majority), R-L0-04 (`insert` is consistently a copying verb across all modules).
- (a) `add`: Python/JS/OCaml/Kotlin, 4/9; `insert`: Rust/Gleam/Swift, 3/9; `put`: Elixir. None reaches 5/9.
- (c) If someone writes `add`, guide them to `set.insert` through the `ALIASES` row (added in R-set-01). Stop silent-discard mistakes via R-L0-05, not by naming.
- (d) No change.
- issue: none

---

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut notation) | Main spellings in other languages | Verdict | Proposed |
|---|---|---|---|---|
| `set.new` | `set.new() -> Set[A]` | Rust `new` / Gleam `new` / Elixir `new/0` / OCaml `empty` | Unchanged | — |
| `set.from_list` | `set.from_list(xs: List[A]) -> Set[A]` | Gleam `from_list` / OCaml `of_list` / Kotlin `toSet` / Rust `from_iter` | Unchanged | — (R-L0-07) |
| `set.insert` | `set.insert(s: Set[A], value: A) -> Set[A]` | Rust・Gleam・Swift `insert` / Python・JS・OCaml・Kotlin `add` / Elixir `put` | Unchanged | R-set-04 |
| `set.remove` | `set.remove(s: Set[A], value: A) -> Set[A]` | Rust・Python・OCaml・Kotlin・Swift `remove` / JS・Gleam・Elixir `delete` | Unchanged | — |
| `set.contains` | `set.contains(s: Set[A], value: A) -> Bool` | Rust・Gleam・Kotlin・Swift `contains` / JS `has` / OCaml `mem` / Elixir `member?` | Unchanged | — |
| `set.len` | `set.len(s: Set[A]) -> Int` | Rust `len` / Gleam・JS・Elixir `size` / OCaml `cardinal` / Swift `count` | Unchanged | — (R-L0-11) |
| `set.is_empty` | `set.is_empty(s: Set[A]) -> Bool` | Rust・Gleam・OCaml `is_empty` / Kotlin `isEmpty` / Swift `isEmpty` | Unchanged | — |
| `set.to_list` | `set.to_list(s: Set[A]) -> List[A]` | Gleam・Elixir・OCaml `to_list` / Kotlin `toList` | Unchanged | — |
| `set.union` | `set.union(a: Set[A], b: Set[A]) -> Set[A]` | 8 of 9 languages `union`（Go —） | Unchanged | — |
| `set.intersection` | `set.intersection(a: Set[A], b: Set[A]) -> Set[A]` | Rust・Python・JS・Gleam・Elixir・Swift `intersection` / OCaml `inter` / Kotlin `intersect` | Unchanged | — |
| `set.difference` | `set.difference(a: Set[A], b: Set[A]) -> Set[A]` | Rust・Python・JS・Gleam・Elixir `difference` / Swift `subtracting` / OCaml `diff` / Kotlin `subtract` | Unchanged | — |
| `set.symmetric_difference` | `set.symmetric_difference(a: Set[A], b: Set[A]) -> Set[A]` | Rust・Python・Gleam・Elixir `symmetric_difference` / JS・Swift `symmetricDifference` | Unchanged | — |
| `set.is_subset` | `set.is_subset(a: Set[A], b: Set[A]) -> Bool` | Rust・Gleam `is_subset` / Python `issubset` / JS `isSubsetOf` / OCaml `subset` / Swift `isSubset(of:)` | Unchanged | R-set-02 |
| `set.is_disjoint` | `set.is_disjoint(a: Set[A], b: Set[A]) -> Bool` | Rust・Gleam `is_disjoint` / Python `isdisjoint` / JS `isDisjointFrom` / OCaml `disjoint` / Swift `isDisjoint(with:)` | Unchanged | — |
| `set.filter` | `set.filter(s: Set[A], f: (A) -> Bool) -> Set[A]` | Gleam・Elixir・OCaml・Kotlin・Swift `filter` / Rust `retain` | Unchanged | R-map-06 |
| `set.map` | `set.map(s: Set[A], f: (A) -> B) -> Set[B]` | Gleam・OCaml `map` / Kotlin `map`（returns List）/ Swift `map`（returns Array） | Unchanged | R-map-06 |
| `set.fold` | `set.fold(s: Set[A], init: B, f: (B, A) -> B) -> B` | Gleam・OCaml `fold` / Kotlin `fold` / Swift `reduce` | Unchanged | R-map-06 |
| `set.any` | `set.any(s: Set[A], f: (A) -> Bool) -> Bool` | Kotlin `any` / OCaml `exists` / Elixir `Enum.any?` / Swift `contains(where:)` | Unchanged | R-map-06 |
| `set.all` | `set.all(s: Set[A], f: (A) -> Bool) -> Bool` | Kotlin `all` / OCaml `for_all` / Elixir `Enum.all?` / Swift `allSatisfy` | Unchanged | R-map-06 |
| `set.count` | `set.count(s: Set[A], f: (A) -> Bool) -> Int`（Proposed） | Kotlin `count {}` / Elixir `Enum.count/2` / Swift `count(where:)` / Gleam — | Add | R-set-01 |
| `set.find` | `set.find(s: Set[A], f: (A) -> Bool) -> Option[A]`（Proposed） | Kotlin `find` / Elixir `Enum.find` / Swift `first(where:)` / Rust `iter().find` | Add | R-set-01 |
| `set.clear` | `set.clear(mut s: Set[A]) -> Unit` | Rust `clear` / Python `clear` / JS `clear` / Kotlin `clear` | Add | R-L0-04（`clear` matrix） |
| `set.enumerate` | （Add）`set.enumerate(s: Set[A]) -> List[(Int, A)]` | Swift `enumerated` / Rust `enumerate` | Add | R-L0-42 |
| `set.zip` | （Add）`set.zip(s: Set[A], ys: List[B]) -> List[(A, B)]` | Swift `zip` / Rust `zip` | Add | R-L0-42 |
| `set.zip_with` | （Add）`set.zip_with(s: Set[A], ys: List[B], f: (A, B) -> C) -> List[C]` | Kotlin `zip(other, f)` | Add | R-L0-42 |
| `set.filter_map` | （Add）`set.filter_map(s: Set[A], f: (A) -> B?) -> List[B]` | Swift `compactMap` / Rust `filter_map` | Add | R-L0-42 |
| `set.find_map` | （Add）`set.find_map(s: Set[A], f: (A) -> B?) -> B?` | Rust `find_map` | Add | R-L0-42 |
| `set.reduce` | （Add）`set.reduce(s: Set[A], f: (A, A) -> A) -> A?` | Swift `reduce` / Rust `reduce` | Add | R-L0-42 |
| `set.scan` | （Add）`set.scan(s: Set[A], init: B, f: (B, A) -> B) -> List[B]` | Rust `scan` / Kotlin `runningFold` | Add | R-L0-42 |
| `set.partition` | （Add）`set.partition(s: Set[A], f: (A) -> Bool) -> (List[A], List[A])` | Kotlin `partition` | Add | R-L0-42 |
| `set.group_by` | （Add）`set.group_by(s: Set[A], f: (A) -> B) -> Map[B, List[A]]` | Kotlin `groupBy` / Swift `Dictionary(grouping:)` | Add | R-L0-42 |
| `set.sort` | （Add）`set.sort(s: Set[A]) -> List[A]` | Swift `sorted()` | Add | R-L0-42 |
| `set.sort_by` | （Add）`set.sort_by(s: Set[A], f: (A) -> B) -> List[A]` | Kotlin `sortedBy` | Add | R-L0-42 |
| `set.sort_with` | （Add）`set.sort_with(s: Set[A], cmp: (A, A) -> Int) -> List[A]` | Swift `sorted(by:)` | Add | R-L0-42 |
| `set.min` | （Add）`set.min(s: Set[A]) -> A?` | Swift `min()` / Rust `min` | Add | R-L0-42 |
| `set.max` | （Add）`set.max(s: Set[A]) -> A?` | Swift `max()` / Rust `max` | Add | R-L0-42 |
| `set.min_by` | （Add）`set.min_by(s: Set[A], f: (A) -> K) -> A?` | Kotlin `minByOrNull` | Add | R-L0-42 |
| `set.max_by` | （Add）`set.max_by(s: Set[A], f: (A) -> K) -> A?` | Kotlin `maxByOrNull` | Add | R-L0-42 |
| `set.min_with` | （Add）`set.min_with(s: Set[A], cmp: (A, A) -> Int) -> A?` | Swift `min(by:)` | Add | R-L0-42 |
| `set.max_with` | （Add）`set.max_with(s: Set[A], cmp: (A, A) -> Int) -> A?` | Swift `max(by:)` | Add | R-L0-42 |
| `set.sum` | （Add）`set.sum[A: Numeric](s: Set[A]) -> A` | Rust `sum` / Kotlin `sum` | Add | R-L0-42 |
| `set.product` | （Add）`set.product[A: Numeric](s: Set[A]) -> A` | Rust `product` | Add | R-L0-42 |

L3 row count: 21 (current 19 + 2 additions). Verdict breakdown: 19 unchanged, 2 additions.

Usage counts (reference; occurrences of `\bset\.<fn>\b`, an upper bound including definitions and generated docs; spec / tests / docs / stdlib): insert 32 / 26 / 24 / 6, remove 8 / 20 / 13 / 0, to_list 33 / 20 / 5 / 9, map 20 / 15 / 16 / 6.

---

<a id="l0-への差し戻し"></a>

## Returned to L0

None.
