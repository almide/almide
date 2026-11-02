<!-- description: Stdlib redesign L2/L3 — heap (new, R-L1-02): a min-first priority queue with key (_by) and comparator (_with) constructors, in-place push/pop like list, and a deterministic tie order -->
<a id="l2l3-heap新しいモジュール"></a>

# L2/L3: heap (New Module)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-heap.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-heap.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-heap.zh-CN.md)

> Non-normative dated redesign snapshot; fixed original authoritative and unchanged, no automatic synchronization, new measurements or implementation verification. Original F marks refer to its investigation, not ours. Initial ten-function proposal and later six additions coexist. R-heap-03 already quotes int.compare as current, while still proposing its correction: preserved rather than invented old syntax. Internal-order independence and cross-target sameness are source design assertions, not a new universal proof; no unspecified callback observation guarantee is added. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-heap.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md), [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md), R-L1-02: add priority queue `heap`, min-heap default, key version `new_by`.
> Sources: Rust https://doc.rust-lang.org/std/collections/struct.BinaryHeap.html ; Go https://pkg.go.dev/container/heap ; Python https://docs.python.org/3/library/heapq.html ; TS/JS https://jsr.io/@std/data-structures (Deno std `BinaryHeap`, absent from ECMAScript); Gleam — none; Elixir — none, substitute `:gb_sets`; OCaml https://ocaml.org/manual/latest/api/Pqueue.html ; Kotlin https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/util/PriorityQueue.html (JVM); Swift https://swiftpackageindex.com/apple/swift-collections/documentation/heapmodule
> (F) marks cells fetched and checked in the source investigation.

<a id="l2-役割の比較"></a>

## L2: Role Comparison

| Role | Almide (proposal) | Rust | Go | Python | TS/JS(Deno) | Gleam | Elixir | OCaml | Kotlin(JVM) | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Default direction | Minimum first | Maximum first | `Less`  dependent | Minimum (F), from 3.14  `_max`  variants too (F) | Comparator dependent | — | — | Minimum（`pop_min`） | Minimum | Both（`popMin`・`popMax`） |
| Create empty | `new` | `new` | `heap.Init` | `[]` | `new BinaryHeap()` | — | — | `create` | `PriorityQueue()` | `Heap()` |
| Order by key | `new_by(key)` | `Reverse`/tuple | `Less`  implementation | `(key, x)`  tuple | Comparator | — | — | `Make(Ord)` | `compareBy` | `Comparable` |
| Comparator ordering | `new_with(cmp)` | Implement `Ord` | `Less` | — | `new BinaryHeap(cmp)` | — | — | `Make(Ord)` | `PriorityQueue(cmp)` | — |
| From list | `from_list` | `from` | `heap.Init` | `heapify`(F) | `BinaryHeap.from` | — | — | `of_list` | `PriorityQueue(c)` | `Heap(xs)` |
| Add | `push` | `push` | `heap.Push` | `heappush`(F) | `push` | — | — | `add` | `add` | `insert` |
| Remove top | `pop` | `pop` | `heap.Pop` | `heappop`(F) | `pop` | — | — | `pop_min` | `poll` | `popMin` |
| Inspect top | `peek` | `peek` | `h[0]` | `h[0]` | `peek` | — | — | `min_elt` | `peek` | `min` |
| Length / empty | `len`・`is_empty` | `len`・`is_empty` | `Len` | `len` | `length` | — | — | `length`・`is_empty` | `size`・`isEmpty` | `count`・`isEmpty` |
| Sorted list | `to_list` | `into_sorted_vec` | — | `sorted(h)` | `drain` | — | — | — | — | `unordered` |

<a id="完全性の規則"></a>

### Completeness Rules

- Three ways to order: default, natural order of `A`, minimum first; key `new_by(key: (A) -> K)`; comparator `new_with(cmp: (A, A) -> Int)`. Suffixes follow R-L0-10: `_by` is a key function, `_with` a function received instead of a value; R-L0-16 specifies comparator `(A, A) -> Int`. Maximum-first is written e.g. `new_with((a, b) => int.compare(b, a))`.
- Only empty constructors accept an ordering. `from_list` uses default order only. To insert a list into a keyed/comparator heap, call `new_by`, then `push`. Omitting `from_list_by` and similar forms is deliberate, avoiding 3 × 2 constructors.
- **Adding/removing mutates in place with `mut`**, R-L0-04, same verbs as `list.push` / `list.pop`. No copying `pop` returning a pair. If added later, its pair order is `(Option[A], Heap[A])`, removed element first, same reading order as `list.pop`; same decision as L2-deque.
- If nothing can be removed, return none, R-L0-06.
- Equal-priority items leave in insertion order, stable. Native and wasm have one deterministic order, ADR-0016. `to_list` has the same order as repeated `pop`.
- `Heap` is not declared ordered, so no `heap.compare`; R-L0-16 covers only ordered types.

<a id="l2-の判定"></a>

### L2 Judgment

- New module, with all majority roles of six languages.

<a id="案"></a>

## Proposals

<a id="r-heap-01-heap-モジュールを足す---採用"></a>

### R-heap-01 Add `heap` — ○ Accepted

- Contents: the ten L3 functions below, type `Heap[A]`. Pure-Almide binary heap, each element carrying its insertion sequence number to stabilize ties.
- L0 basis: R-L0-04, R-L0-06, R-L0-10, R-L0-02, R-L0-07 (`from_list` / `to_list`).
- Spelling: `push` / `pop` / `peek` follow Rust/Deno/Kotlin(JVM); Python uses `heappush`/`heappop` (F), OCaml `add`/`pop_min`. `peek` is a new Almide verb, but `first` already means the beginning of a sequence in `list`/`deque`. Heap internals are not ordered, so use another word.
- (a) Six languages provide it, L1. Minimum-first matches Python(F), Go, OCaml and Kotlin.
- (b) Pure computation. Native/wasm produce identical results, including tie order.
- (c) LLMs assume a priority queue in Dijkstra/top-k tasks. Fixed tie order makes expected test values target-independent.
- (d) Addition, new automatically imported module.
- Proposed gate: property tests, repeated `pop` matches `list.sort`, ties retain insertion order; native/wasm cross fixture; measurement that n push/pop operations are O(n log n).
- Issue: #3933.

<a id="r-heap-02-heap-には中の並びに依存しないセルだけを置くcontainsanyallcountfilterclear---採用"></a>

### R-heap-02 Only Internal-Order-Independent Operations (`contains`, `any`, `all`, `count`, `filter`, `clear`) — ○ Accepted

- Current: `heap` has ten functions, no D3 family or `contains`. L2-heap defines `to_list` as repeated `pop` order, but does not specify internal array order.
- Proposal, add six functions:
  - `fn heap.contains[A](h: Heap[A], x: A) -> Bool`
  - `fn heap.any[A](h: Heap[A], f: (A) -> Bool) -> Bool`, `all` with the same shape, `fn heap.count[A](h: Heap[A], f: (A) -> Bool) -> Int`
  - `fn heap.filter[A](h: Heap[A], f: (A) -> Bool) -> Heap[A]`, preserving ordering mode, default / `_by` / `_with`, and tie insertion order.
  - `fn heap.clear[A](mut h: Heap[A]) -> Unit`, preserving ordering mode, R-L0-04 `clear` matrix.
  - Deliberately absent: `map`, `fold`, `find`. For ordered traversal write `heap.to_list(h) |> list.<動詞>`.
- L0 basis: R-map-06, D3 family, but heap excludes operations exposing traversal order. R-L0-04: everything except `clear` copies.
- (a) Rust `BinaryHeap` `iter()` has “arbitrary order” and lacks `contains` (F), https://doc.rust-lang.org/std/collections/struct.BinaryHeap.html . JVM `PriorityQueue` also does not guarantee iteration order. No language promises heap internal order.
- (b) Only order-independent operations, so future internal array rearrangement changes neither target's result nor user-code results.
- (c) `map` cannot carry `new_by(key)` / `new_with(cmp)` ordering to `B`, silently resetting to default. `fold` / `find` either expose internal order or hide sorting not suggested by the name when traversing `pop` order.
- (d) Six additions, no compatibility impact for a new module.
- Completeness: only D3 operations whose result does not depend on traversal order, returning Bool, Int or Heap with the same ordering. Document `map` / `fold` / `find` on R-map-06's allowlist with reasons.
- Proposed gate: add heap column to R-map-06 matrix and allowlist the three omissions. Property: `heap.to_list(heap.filter(h, f)) == list.filter(heap.to_list(h), f)`.
- Issue: #3968.

<a id="r-heap-03-heapnew_with-の例を引き算にしない---採用文書だけ"></a>

### R-heap-03 Do Not Use Subtraction in `heap.new_with` Examples — ○ Accepted, Docs Only

- Current: completeness rules teach `new_with((a, b) => int.compare(b, a))` for maximum-first. Subtraction wraps at `Int` extremes and reverses sign.
- Proposal: change example to `heap.new_with((a, b) => int.compare(b, a))`, R-L0-16 `int.compare`. Use it in CHEATSHEET and docs/stdlib, in a ```` ```almide check ```` fence.
- (c) LLMs copy documentation examples verbatim.
- (d) Docs only; replace after `int.compare` lands.
- Issue: #3968.

<a id="r-heap-04-new_minnew_max-に分ける---不採用"></a>

### R-heap-04 Split into `new_min` / `new_max` — × Rejected

- Reason: default minimum-first matches Python/Go/OCaml/Kotlin, R-L1-02. Maximum-first uses `heap.new_with((a, b) => int.compare(b, a))`, not difference `b - a`, which overflows at `Int` extremes, or `new_by`. Default direction is recorded in the index's common conventions.
- Issue: none.

<a id="r-heap-05-heapfoldheapfind-を足す---不採用"></a>

### R-heap-05 Add `heap.fold` / `heap.find` — × Rejected

- Reason: internal-array traversal exposes order, changing results when implementation changes. Removal-order traversal, ascending, hides sorting not implied by the name. Where order is needed, `heap.to_list(h) |> list.fold(…)` makes sorting visible. Same decision as L2 completeness: operations only when their returned value is independent of traversal order.
- Issue: none.

<a id="r-heap-06--は取り出す順の要素の並びで比べh-もその順で書く---採用"></a>

### R-heap-06 Compare `==` by Removal-Order Elements, Print `"${h}"` in That Order — ○ Accepted

- Current implementation, #4033: `Heap` is an array binary heap. Builtin `==` compares internal arrays, so two heaps with identical elements need not be equal when insertion orders differ.
- Proposal: `==` compares element sequences in removal order, repeated `pop`. `"${h}"` uses that order too, e.g. `heap[1, 2, 5]`. Two heaps with different ordering definitions, `new` / `new_by` / `new_with`, are equal if these sequences match.
- L0 basis: R-L0-37 and R-heap-02, do not expose internal order.
- (a) Rust `BinaryHeap` and Swift `Heap` have no equality. All Almide value types except `Fn` have `==`, so omission is not possible. Internal-array comparison violates R-heap-02; compare externally visible sequences instead.
- Issue: #4033.

<a id="l3-関数表"></a>

## L3: Function Table

| Function | Signature (proposed) | Main spellings elsewhere | Judgment | Proposal |
|---|---|---|---|---|
| `heap.new` | `fn new[A]() -> Heap[A]` | Rust `new` / Kotlin `PriorityQueue()` / Swift `Heap()` | Add | R-heap-01 |
| `heap.new_by` | `fn new_by[A, K](key: (A) -> K) -> Heap[A]` | Kotlin `compareBy` / Python `(key, x)` | Add | R-heap-01 |
| `heap.new_with` | `fn new_with[A](cmp: (A, A) -> Int) -> Heap[A]` | Kotlin `PriorityQueue(cmp)` / Deno `new BinaryHeap(cmp)` | Add | R-heap-01 |
| `heap.from_list` | `fn from_list[A](xs: List[A]) -> Heap[A]` | Python `heapify`(F) / Rust `from` / OCaml `of_list` | Add | R-heap-01 |
| `heap.push` | `fn push[A](mut h: Heap[A], x: A) -> Unit` | Rust・Deno `push` / Python `heappush`(F) | Add | R-heap-01 |
| `heap.pop` | `fn pop[A](mut h: Heap[A]) -> A?` | Rust・Deno `pop` / Python `heappop`(F) / Swift `popMin` | Add | R-heap-01 |
| `heap.peek` | `fn peek[A](h: Heap[A]) -> A?` | Rust・Kotlin・Deno `peek` / Swift `min` | Add | R-heap-01 |
| `heap.len` | `fn len[A](h: Heap[A]) -> Int` | Rust `len` / Python `len` | Add | R-heap-01 |
| `heap.is_empty` | `fn is_empty[A](h: Heap[A]) -> Bool` | Rust `is_empty` / Kotlin `isEmpty` | Add | R-heap-01 |
| `heap.to_list` | `fn to_list[A](h: Heap[A]) -> List[A]` | Rust `into_sorted_vec` / Python `sorted(h)` | Add | R-heap-01 |
| `heap.contains` | `fn contains[A](h: Heap[A], x: A) -> Bool` | Python `in` (as list) | Add | R-heap-02 |
| `heap.any` | `fn any[A](h: Heap[A], f: (A) -> Bool) -> Bool` | Kotlin `any`（PriorityQueue） | Add | R-heap-02 |
| `heap.all` | `fn all[A](h: Heap[A], f: (A) -> Bool) -> Bool` | Kotlin `all` | Add | R-heap-02 |
| `heap.count` | `fn count[A](h: Heap[A], f: (A) -> Bool) -> Int` | Kotlin `count {}` | Add | R-heap-02 |
| `heap.filter` | `fn filter[A](h: Heap[A], f: (A) -> Bool) -> Heap[A]` | Rust `retain` (in place) | Add | R-heap-02 |
| `heap.clear` | `fn clear[A](mut h: Heap[A]) -> Unit` | Rust `clear` / Kotlin `clear` | Add | R-heap-02、R-L0-04 |
