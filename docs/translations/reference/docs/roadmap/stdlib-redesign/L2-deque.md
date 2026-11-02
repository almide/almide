> Description (inactive source index metadata): Stdlib redesign L2/L3 — deque (new, R-L1-01): a double-ended queue whose push/pop at both ends are in-place mut verbs like list.push/pop, written in pure Almide
<a id="l2l3-deque新しいモジュール"></a>

# L2/L3: deque (new module)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-deque.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-deque.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-deque.zh-CN.md)

> Non-normative historical redesign snapshot. The pinned original is authoritative and unchanged; no automatic synchronization or independent performance/current implementation verification. Preserve the early 12-function surface alongside later family expansions and the final table. The early no-ordered-type statement and later Ordered hierarchy proposal are reproduced without inventing a reconciliation. O(1), cross-target agreement and speed statements are source design/claims, not newly established guarantees. In preserved pseudocode, 端 means end and 同名 means same name. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-deque.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-01: add the double-ended queue `deque`)
> Sources: Rust https://doc.rust-lang.org/std/collections/struct.VecDeque.html / Go https://pkg.go.dev/container/list / Python https://docs.python.org/3/library/collections.html#collections.deque / TS/JS https://jsr.io/@std/data-structures (Deno std; absent from ECMAScript) / Gleam — (absent) / Elixir https://www.erlang.org/doc/apps/stdlib/queue.html / OCaml https://ocaml.org/manual/latest/api/Queue.html / Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.collections/-array-deque/ / Swift https://swiftpackageindex.com/apple/swift-collections/documentation/dequemodule

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide (proposed) | Rust | Go | Python | TS/JS | Gleam | Elixir(:queue) | OCaml(Queue) | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Create empty / from list | `new`・`from_list` | `new`・`from` | `list.New` | `deque()`・`deque(xs)` | `new Deque` | — | `new`・`from_list` | `create`・`of_seq` | `ArrayDeque()` | `Deque()` |
| Add at back | `push`（R-deque-03） | `push_back` | `PushBack` | `append` | `push` | — | `in` | `push` | `addLast` | `append` |
| Add at front | `push_front` | `push_front` | `PushFront` | `appendleft` | `unshift` | — | `in_r` | — | `addFirst` | `prepend` |
| Take from back | `pop`（R-deque-03） | `pop_back` | `Remove(Back())` | `pop` | `pop` | — | `out_r` | — | `removeLast` | `popLast` |
| Take from front | `pop_front` | `pop_front` | `Remove(Front())` | `popleft` | `shift` | — | `out` | `pop`/`take_opt` | `removeFirst` | `popFirst` |
| Inspect front / back | `first`・`last` | `front`・`back` | `Front`・`Back` | `d[0]`・`d[-1]` | `peekFront` | — | `peek`・`peek_r` | `peek_opt` | `first`・`last` | `first`・`last` |
| Element at i | `get` | `get` | — | `d[i]` | — | — | — | — | `get` | `d[i]` |
| Length / emptiness | `len`・`is_empty` | `len`・`is_empty` | `Len` | `len` | `length` | — | `len`・`is_empty` | `length`・`is_empty` | `size`・`isEmpty` | `count`・`isEmpty` |
| To list | `to_list` | `iter().collect` | — | `list(d)` | `[...d]` | — | `to_list` | `to_seq` | `toList` | `Array(d)` |

<a id="完全性の規則"></a>

### Completeness rules

- Cover all 6 cells of both ends × {add, take, inspect}: `push_front`, `push`, `pop_front`, `pop`, `first`, `last`. The 2 tail operations are `push` / `pop`, like `list` (R-deque-03).
- **Adding and taking mutate in place via `mut`.** R-L0-04 item 2 includes front-named `push_front` / `pop_front` in the verb set as the same family as `push` / `pop`. R-L0-04 item 3, “a verb has the same style in every module,” makes them in-place operations like `list.push(mut xs, x)` / `list.pop(mut xs) -> A?`. L1 R-L1-01 has also been revised to this form.
- **Do not provide a copying `pop` that returns a pair.** To take from a value that cannot be mutated (such as a function argument), copy it with `var q2 = q`, then write `pop_front(q2)`. If a copying version is added in future, the pair order will be `(Option[A], Deque[A])` (taken element first), so its reading order matches `list.pop` returning the element's `Option[A]`.
- Return none when no element can be taken (R-L0-06). Out-of-bounds `get` also returns none.
- Do not declare it as an ordered type, so do not provide `deque.compare` (R-L0-16 applies only to ordered types).

<a id="l2-の判定"></a>

### L2 verdict

- New module. It covers all majority roles across the 9 languages.

<a id="案"></a>

## Proposals

<a id="r-deque-01-deque-モジュールを足す---採用"></a>

### R-deque-01 Add a `deque` module — ○ Accepted

- Content: the 12 L3 functions below. Type: `Deque[A]`. Internals are pure Almide (2 lists for front and back, or a ring buffer on `list`; either gives amortized O(1) addition/removal at both ends).
- Supporting L0 rules: R-L0-04 (only fixed verbs mutate in place; `push_<端>` / `pop_<端>` count as the `push` / `pop` families in R-L0-04 item 2), R-L0-06, R-L0-02 (constructors belong to the returned type's module), R-L0-07 (`from_list` / `to_list`).
- (a) 7 languages have it in their standard libraries (L1). Front spellings follow Rust's `push_front`/`pop_front`. Tail spellings are `push` / `pop` like `list` (R-deque-03; Swift's `Deque` also uses `append` / `popLast` like `Array`).
- (b) Pure computation, identical on native and wasm. No host functionality needed.
- (c) In breadth-first search, `list.drop(xs, 1)` currently costs O(n) each time. LLM code written with another language's deque in mind runs quickly as written.
- (d) Addition (new module; for `import`, make it automatic like `list` / `map`: add it to stdlib_info's `AUTO_IMPORT_BUNDLED`).
- Rejected alternative: avoid copying when removing from the front of `List`, like Go's `s[1:]`, instead of adding deque. This reduces types to learn, but changing `List` representation touches deep compiler internals and does not accelerate front insertion. LLMs also see “use deque for a queue” in other languages (Python `collections.deque`, Rust `VecDeque`, Kotlin `ArrayDeque`), so add deque. Measure `List` front-operation speed separately when revisiting `List` performance.
- Proposed gate: boundary-input matrix tests like list (a `pop` from empty, out-of-bounds `get`), native/wasm cross fixtures, and measurement that n `push` operations followed by n `pop_front` operations take linear time (in the form of `check-perf-ratio.sh`).
- issue: #3932

<a id="r-deque-02-deque-に高階関数の族と-containsclear-を足す---採用"></a>

### R-deque-02 Add the higher-order family and `contains` / `clear` to deque — ○ Accepted

- Now: `deque` has 12 functions but lacks `contains` and the D3 family (`map`, `filter`, `fold`, `any`, `all`, `count`, `find`). Since `for x in d` is unavailable too, inspecting contents always goes through `deque.to_list`.
- Proposal (traverse from front to back):
  - `fn deque.contains[A](d: Deque[A], x: A) -> Bool`
  - `fn deque.map[A, B](d: Deque[A], f: (A) -> B) -> Deque[B]`
  - `fn deque.filter[A](d: Deque[A], f: (A) -> Bool) -> Deque[A]`
  - `fn deque.fold[A, B](d: Deque[A], init: B, f: (B, A) -> B) -> B`
  - `fn deque.any[A](d: Deque[A], f: (A) -> Bool) -> Bool`, `all` (same form), `fn deque.count[A](d: Deque[A], f: (A) -> Bool) -> Int`
  - `fn deque.find[A](d: Deque[A], f: (A) -> Bool) -> A?` (nearest the front first)
  - `fn deque.clear[A](mut d: Deque[A]) -> Unit` (the `clear` matrix of R-L0-04)
- Supporting L0 rules: L2-list completeness rule 1 / R-map-06 (all collections have D3's 7 functions and `contains`), R-L0-01 (same names and callback forms as list), R-L0-04 (all except `clear` are copying operations).
- (a) 8/9 have ways to inspect / transform deque (Python uses iteration and `in`; Erlang/Elixir `:queue` has `all`, `any`, `filter`, `fold`, `member`; Rust `VecDeque` has `contains` and `iter()`; Kotlin / Swift provide all as collections). Only Go's `container/list` requires a manual loop.
- (b) Pure Almide computation. Traversal is fixed front-to-back, hence the same on both targets.
- (c) If `xs |> X.any(f)` works for list/map/set but not deque, LLMs infer the rule and get E002. Writing “already visited?” as `deque.contains` is common in breadth-first search.
- (d) Add 9. No compatibility impact because this is a new module.
- Completeness rule: ordered-sequence collections (list/deque) and set-like collections (map/set) all have D3's 7 functions and `contains`. Traversal order matches `to_list`.
- Proposed gate: add a deque column to R-map-06's collection-family matrix. Property test: every cell produces the same result as `deque.to_list(d) |> list.<同名>(f)`.
- issue: #3968

<a id="r-deque-03-末尾の-push_backpop_back-を-pushpop-にする---採用"></a>

### R-deque-03 Use `push` / `pop` for tail `push_back` / `pop_back` — ○ Accepted

- Content: `deque.push(mut d, x)` / `deque.pop(mut d) -> A?` add/remove at the tail; do not provide `push_back` / `pop_back`. Keep `push_front` / `pop_front` as `deque`-specific functions.
- Reason: the [protocol hierarchy](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md)'s `Growable` requirements are `push` / `pop` (tail). The same generic functions can handle `list` and `deque`. Swift's `Deque` (swift-collections) also adds `prepend` / `popFirst` to the same `append` / `popLast` as `Array`.
- (d) `deque` is added in this redesign, so this changes the names to add rather than renaming existing ones.
- issue: #4000

<a id="r-deque-04-deque-に-orderedindexedgrowable-の要件をそろえる---採用"></a>

### R-deque-04 Complete `Ordered` / `Indexed` / `Growable` requirements in `deque` — ○ Accepted

- Content: provide `reverse`, `take`, `drop`, `take_end`, `drop_end`, `slice`, `set`, `insert`, `remove_at`. Forms and range handling match the same `list` functions.
- Reason: like `list`, `deque` is a positionally readable/writable sequence (Swift's `Deque` is a `RandomAccessCollection`, `MutableCollection`, `RangeReplaceableCollection`), hence falls under `Indexed` / `Growable` in the hierarchy.
- (d) Add 9.
- issue: #4000

<a id="r-deque-05--と-d-は要素を前から順に比べ書く---採用"></a>

### R-deque-05 `==` and `"${d}"` compare / write elements from front to back — ○ Accepted

- Now (#4033 implementation): `Deque` is a record with 2 lists, front and back. Built-in `==` compares record fields, so 2 deques with the same elements in the same order are not equal if internally split differently. `"${d}"` also writes the 2 internal lists.
- Proposal: `==` compares elements front-to-back; `"${d}"` writes them front-to-back as in `deque[1, 2, 3]`. The internal split is not externally visible.
- Supporting L0 rules: R-L0-37 (`Eq` / `Repr` follow value semantics), R-L0-01.
- (a) Swift's `Deque` (swift-collections), Rust's `VecDeque`, Python's `collections.deque` and Kotlin's `ArrayDeque` all determine equality from the element sequence.
- Implementation: use conventional `Eq` and `Repr` methods (`fn Deque.eq` / `fn Deque.to_string`; R-L0-38).
- issue: #4033

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (proposed) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `deque.new` | `fn new[A]() -> Deque[A]` | Rust `new` / Python `deque()` / Elixir `new` | Add | R-deque-01 |
| `deque.from_list` | `fn from_list[A](xs: List[A]) -> Deque[A]` | Rust `from` / Elixir `from_list` | Add | R-deque-01 |
| `deque.to_list` | `fn to_list[A](d: Deque[A]) -> List[A]` | Elixir `to_list` / Kotlin `toList` | Add | R-deque-01 |
| `deque.len` | `fn len[A](d: Deque[A]) -> Int` | Rust `len` / Python `len` | Add | R-deque-01 |
| `deque.is_empty` | `fn is_empty[A](d: Deque[A]) -> Bool` | Rust `is_empty` / Kotlin `isEmpty` | Add | R-deque-01 |
| `deque.push` | `fn push[A](mut d: Deque[A], x: A) -> Unit` | Rust `push_back` / Go `PushBack` / Python `append` | Add | R-deque-01、R-deque-03 |
| `deque.push_front` | `fn push_front[A](mut d: Deque[A], x: A) -> Unit` | Rust `push_front` / Go `PushFront` / Python `appendleft` | Add | R-deque-01 |
| `deque.pop` | `fn pop[A](mut d: Deque[A]) -> A?` | Rust `pop_back` / Python `pop` / Swift `popLast` | Add | R-deque-01、R-deque-03 |
| `deque.pop_front` | `fn pop_front[A](mut d: Deque[A]) -> A?` | Rust `pop_front` / Python `popleft` / Swift `popFirst` | Add | R-deque-01 |
| `deque.first` | `fn first[A](d: Deque[A]) -> A?` | Kotlin・Swift `first` / Rust `front` | Add | R-deque-01 |
| `deque.last` | `fn last[A](d: Deque[A]) -> A?` | Kotlin・Swift `last` / Rust `back` | Add | R-deque-01 |
| `deque.get` | `fn get[A](d: Deque[A], i: Int) -> A?` | Rust・Kotlin `get` / Python `d[i]` | Add | R-deque-01 |
| `deque.contains` | `fn contains[A](d: Deque[A], x: A) -> Bool` | Rust `contains` / Python `in` / Erlang `member` | Add | R-deque-02 |
| `deque.map` | `fn map[A, B](d: Deque[A], f: (A) -> B) -> Deque[B]` | Kotlin・Swift（collections） | Add | R-deque-02 |
| `deque.filter` | `fn filter[A](d: Deque[A], f: (A) -> Bool) -> Deque[A]` | Erlang `:queue.filter` | Add | R-deque-02 |
| `deque.fold` | `fn fold[A, B](d: Deque[A], init: B, f: (B, A) -> B) -> B` | Erlang `:queue.fold` / OCaml `Queue.fold` | Add | R-deque-02 |
| `deque.any` | `fn any[A](d: Deque[A], f: (A) -> Bool) -> Bool` | Erlang `:queue.any` | Add | R-deque-02 |
| `deque.all` | `fn all[A](d: Deque[A], f: (A) -> Bool) -> Bool` | Erlang `:queue.all` | Add | R-deque-02 |
| `deque.count` | `fn count[A](d: Deque[A], f: (A) -> Bool) -> Int` | Kotlin `count {}` | Add | R-deque-02 |
| `deque.find` | `fn find[A](d: Deque[A], f: (A) -> Bool) -> A?` | Kotlin `find` / Swift `first(where:)` | Add | R-deque-02 |
| `deque.clear` | `fn clear[A](mut d: Deque[A]) -> Unit` | Rust `clear` / Python `clear` | Add | R-deque-02、R-L0-04 |
| `deque.reverse` | `fn reverse[A](d: Deque[A]) -> Deque[A]` | Swift `reversed` / Kotlin `reversed` | Add | R-deque-04 |
| `deque.take` | `fn take[A](d: Deque[A], n: Int) -> Deque[A]` | Swift `prefix` | Add | R-deque-04 |
| `deque.drop` | `fn drop[A](d: Deque[A], n: Int) -> Deque[A]` | Swift `dropFirst` | Add | R-deque-04 |
| `deque.take_end` | `fn take_end[A](d: Deque[A], n: Int) -> Deque[A]` | Swift `suffix` | Add | R-deque-04 |
| `deque.drop_end` | `fn drop_end[A](d: Deque[A], n: Int) -> Deque[A]` | Swift `dropLast` | Add | R-deque-04 |
| `deque.slice` | `fn slice[A](d: Deque[A], start: Int, end: Int) -> Deque[A]` | Swift `d[a..<b]` / Rust `range` | Add | R-deque-04 |
| `deque.set` | `fn set[A](d: Deque[A], i: Int, x: A) -> Deque[A]` | Swift `d[i] = x` / Rust `d[i] = x` | Add | R-deque-04 |
| `deque.insert` | `fn insert[A](d: Deque[A], i: Int, x: A) -> Deque[A]` | Swift `insert(_:at:)` / Rust `insert` | Add | R-deque-04 |
| `deque.remove_at` | `fn remove_at[A](d: Deque[A], i: Int) -> Deque[A]` | Swift `remove(at:)` / Rust `remove` | Add | R-deque-04 |
| `deque.enumerate` | （Add）`deque.enumerate(d: Deque[A]) -> List[(Int, A)]` | Swift `enumerated` / Rust `enumerate` | Add | R-L0-42 |
| `deque.zip` | （Add）`deque.zip(d: Deque[A], ys: List[B]) -> List[(A, B)]` | Swift `zip` / Rust `zip` | Add | R-L0-42 |
| `deque.zip_with` | （Add）`deque.zip_with(d: Deque[A], ys: List[B], f: (A, B) -> C) -> List[C]` | Kotlin `zip(other, f)` | Add | R-L0-42 |
| `deque.filter_map` | （Add）`deque.filter_map(d: Deque[A], f: (A) -> B?) -> List[B]` | Swift `compactMap` / Rust `filter_map` | Add | R-L0-42 |
| `deque.find_map` | （Add）`deque.find_map(d: Deque[A], f: (A) -> B?) -> B?` | Rust `find_map` | Add | R-L0-42 |
| `deque.reduce` | （Add）`deque.reduce(d: Deque[A], f: (A, A) -> A) -> A?` | Swift `reduce` / Rust `reduce` | Add | R-L0-42 |
| `deque.scan` | （Add）`deque.scan(d: Deque[A], init: B, f: (B, A) -> B) -> List[B]` | Rust `scan` / Kotlin `runningFold` | Add | R-L0-42 |
| `deque.partition` | （Add）`deque.partition(d: Deque[A], f: (A) -> Bool) -> (List[A], List[A])` | Kotlin `partition` | Add | R-L0-42 |
| `deque.group_by` | （Add）`deque.group_by(d: Deque[A], f: (A) -> B) -> Map[B, List[A]]` | Kotlin `groupBy` / Swift `Dictionary(grouping:)` | Add | R-L0-42 |
| `deque.sort` | （Add）`deque.sort(d: Deque[A]) -> List[A]` | Swift `sorted()` | Add | R-L0-42 |
| `deque.sort_by` | （Add）`deque.sort_by(d: Deque[A], f: (A) -> B) -> List[A]` | Kotlin `sortedBy` | Add | R-L0-42 |
| `deque.sort_with` | （Add）`deque.sort_with(d: Deque[A], cmp: (A, A) -> Int) -> List[A]` | Swift `sorted(by:)` | Add | R-L0-42 |
| `deque.min` | （Add）`deque.min(d: Deque[A]) -> A?` | Swift `min()` / Rust `min` | Add | R-L0-42 |
| `deque.max` | （Add）`deque.max(d: Deque[A]) -> A?` | Swift `max()` / Rust `max` | Add | R-L0-42 |
| `deque.min_by` | （Add）`deque.min_by(d: Deque[A], f: (A) -> K) -> A?` | Kotlin `minByOrNull` | Add | R-L0-42 |
| `deque.max_by` | （Add）`deque.max_by(d: Deque[A], f: (A) -> K) -> A?` | Kotlin `maxByOrNull` | Add | R-L0-42 |
| `deque.min_with` | （Add）`deque.min_with(d: Deque[A], cmp: (A, A) -> Int) -> A?` | Swift `min(by:)` | Add | R-L0-42 |
| `deque.max_with` | （Add）`deque.max_with(d: Deque[A], cmp: (A, A) -> Int) -> A?` | Swift `max(by:)` | Add | R-L0-42 |
| `deque.sum` | （Add）`deque.sum[A: Numeric](d: Deque[A]) -> A` | Rust `sum` / Kotlin `sum` | Add | R-L0-42 |
| `deque.product` | （Add）`deque.product[A: Numeric](d: Deque[A]) -> A` | Rust `product` | Add | R-L0-42 |
| `deque.find_index` | （Add）`deque.find_index(d: Deque[A], f: (A) -> Bool) -> Int?` | Swift `firstIndex(where:)` | Add | R-L0-42 |
| `deque.take_while` | （Add）`deque.take_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]` | Swift `prefix(while:)` | Add | R-L0-42 |
| `deque.drop_while` | （Add）`deque.drop_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]` | Swift `drop(while:)` | Add | R-L0-42 |
| `deque.chunk` | （Add）`deque.chunk(d: Deque[A], n: Int) -> List[Deque[A]]` | Kotlin `chunked` | Add | R-L0-42 |
| `deque.window` | （Add）`deque.window(d: Deque[A], n: Int) -> List[Deque[A]]` | Kotlin `windowed` | Add | R-L0-42 |
| `deque.split_at` | （Add）`deque.split_at(d: Deque[A], n: Int) -> (Deque[A], Deque[A])` | Rust `split_at` | Add | R-L0-42 |
| `deque.dedup` | （Add）`deque.dedup(d: Deque[A]) -> Deque[A]` | Rust `dedup` | Add | R-L0-42 |
| `deque.intersperse` | （Add）`deque.intersperse(d: Deque[A], sep: A) -> Deque[A]` | Rust `intersperse` | Add | R-L0-42 |
| `deque.unique` | （Add）`deque.unique(d: Deque[A]) -> Deque[A]` | Kotlin `distinct` | Add | R-L0-42 |
| `deque.unique_by` | （Add）`deque.unique_by(d: Deque[A], f: (A) -> K) -> Deque[A]` | Kotlin `distinctBy` | Add | R-L0-42 |

<a id="l0-への差し戻し処理済み"></a>

## Returned to L0 (resolved)

1. Front-qualified verbs (`push_front` / `pop_front`): included in R-L0-04 item 2 as the same families as `push` / `pop`. The R-L0-G gate accepts these 2 in the `mut` verb set.
2. L1 R-L1-01's “`pop_*` returns a pair as a copying operation”: revised the L1 prose to in-place `mut` mutation under R-L0-04 item 3. This page agrees with L1.
