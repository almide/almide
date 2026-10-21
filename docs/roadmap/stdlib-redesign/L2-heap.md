<!-- description: Stdlib redesign L2/L3 — heap (new, R-L1-02): a min-first priority queue with key (_by) and comparator (_with) constructors, in-place push/pop like list, and a deterministic tie order -->
# L2/L3: heap（新しいモジュール）

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-02: 優先度キュー `heap` を足す。最小ヒープが既定、キー版は `new_by`）
> 出典: Rust https://doc.rust-lang.org/std/collections/struct.BinaryHeap.html ・ Go https://pkg.go.dev/container/heap ・ Python https://docs.python.org/3/library/heapq.html ・ TS/JS https://jsr.io/@std/data-structures （Deno std の `BinaryHeap`。ECMAScript には無い）・ Gleam —（無い）・ Elixir —（無い。`:gb_sets` で代用）・ OCaml https://ocaml.org/manual/latest/api/Pqueue.html ・ Kotlin https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/util/PriorityQueue.html （JVM）・ Swift https://swiftpackageindex.com/apple/swift-collections/documentation/heapmodule
> (F) = 今回取り寄せて確かめたセル。

## L2: 役割の比較

| 役割 | Almide（案） | Rust | Go | Python | TS/JS(Deno) | Gleam | Elixir | OCaml | Kotlin(JVM) | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 既定の向き | 最小が先 | 最大が先 | `Less` しだい | 最小(F)（3.14 から `_max` 版も(F)） | 比較関数しだい | — | — | 最小（`pop_min`） | 最小 | 両方（`popMin`・`popMax`） |
| 空を作る | `new` | `new` | `heap.Init` | `[]` | `new BinaryHeap()` | — | — | `create` | `PriorityQueue()` | `Heap()` |
| キーで順序 | `new_by(key)` | `Reverse`/タプル | `Less` を書く | `(key, x)` のタプル | 比較関数 | — | — | `Make(Ord)` | `compareBy` | `Comparable` |
| 比較関数で順序 | `new_with(cmp)` | `Ord` を実装 | `Less` | — | `new BinaryHeap(cmp)` | — | — | `Make(Ord)` | `PriorityQueue(cmp)` | — |
| リストから | `from_list` | `from` | `heap.Init` | `heapify`(F) | `BinaryHeap.from` | — | — | `of_list` | `PriorityQueue(c)` | `Heap(xs)` |
| 足す | `push` | `push` | `heap.Push` | `heappush`(F) | `push` | — | — | `add` | `add` | `insert` |
| 先頭を取る | `pop` | `pop` | `heap.Pop` | `heappop`(F) | `pop` | — | — | `pop_min` | `poll` | `popMin` |
| 先頭を見る | `peek` | `peek` | `h[0]` | `h[0]` | `peek` | — | — | `min_elt` | `peek` | `min` |
| 長さ・空か | `len`・`is_empty` | `len`・`is_empty` | `Len` | `len` | `length` | — | — | `length`・`is_empty` | `size`・`isEmpty` | `count`・`isEmpty` |
| 順に並べたリスト | `to_list` | `into_sorted_vec` | — | `sorted(h)` | `drain` | — | — | — | — | `unordered` |

### 完全性の規則

- 順序の決め方は 3 つ: 既定（`A` の自然な順、最小が先）、キー `new_by(key: (A) -> K)`、比較関数 `new_with(cmp: (A, A) -> Int)`。接尾辞の意味は R-L0-10（`_by` はキー関数、`_with` は値の代わりに受け取る関数）と R-L0-16（比較関数は `(A, A) -> Int`）のとおりです。最大が先にしたいときは `new_with((a, b) => b - a)` のように書きます。
- 順序を指定できるのは空の構築子だけです。`from_list` は既定の順だけを持ちます。キーや比較関数の付いたヒープにリストを入れるときは、`new_by` の後に `push` します（`from_list_by` などを足さないのは意図したものです。構築子の数を 3 × 2 にしないため）。
- **足す・取るは `mut` のその場書き換え**（R-L0-04。`list.push`・`list.pop` と同じ動詞）。組を返すコピー版の `pop` は置きません。将来足すなら組の順は `(Option[A], Heap[A])`（取った要素が先。`list.pop` の戻り値と同じ読み順）にします（L2-deque と同じ判定）。
- 取り出せないときは none（R-L0-06）。
- 同じ順位の要素は、入れた順に出ます（安定）。native と wasm で 1 つの順に決まります（ADR-0016 の決定性）。`to_list` は `pop` を繰り返したのと同じ順です。
- `Heap` は順序のある型として宣言しないので、`heap.compare` は置きません（R-L0-16 は順序のある型だけが対象）。

### L2 の判定

- 新しいモジュールです。6 言語の多数派の役割をすべて持ちます。

## 案

### R-heap-01 `heap` モジュールを足す — ○ 採用

- 内容: 下の L3 の 10 関数。型は `Heap[A]`。中身は純粋な Almide の二分ヒープで、各要素に入れた順の番号を持たせて同順位を安定にします。
- 根拠の L0 規則: R-L0-04、R-L0-06、R-L0-10、R-L0-02、R-L0-07（`from_list`・`to_list`）。
- 綴り: `push`・`pop`・`peek` は Rust・Deno・Kotlin(JVM) に合わせました（Python は `heappush`/`heappop`(F)、OCaml は `add`/`pop_min`）。`peek` は Almide に初めて出る動詞ですが、`first` は「並びの先頭」の意味で `list`・`deque` が使っており、ヒープの中は並んでいないので別の語にします。
- (a) 6 言語が持っています（L1）。最小が先は Python(F)・Go・OCaml・Kotlin と同じです。
- (b) 純粋な計算です。native と wasm で同じ結果（同順位の順まで）になります。
- (c) ダイクストラ法や上位 k 件の課題で、LLM は優先度キューを前提に書きます。同順位の順が決まっていれば、テストの期待値が target で変わりません。
- (d) 追加（新しいモジュール。自動の import に入れる）。
- ゲート案: 性質のテスト（`pop` を繰り返すと `list.sort` と同じ並び、同順位は入れた順）、native と wasm の cross fixture、n 件の push/pop が O(n log n) であることの計測。
- issue: #3933

## L3: 関数表

| 関数 | シグネチャ(案) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `heap.new` | `fn new[A]() -> Heap[A]` | Rust `new` / Kotlin `PriorityQueue()` / Swift `Heap()` | 追加 | R-heap-01 |
| `heap.new_by` | `fn new_by[A, K](key: (A) -> K) -> Heap[A]` | Kotlin `compareBy` / Python `(key, x)` | 追加 | R-heap-01 |
| `heap.new_with` | `fn new_with[A](cmp: (A, A) -> Int) -> Heap[A]` | Kotlin `PriorityQueue(cmp)` / Deno `new BinaryHeap(cmp)` | 追加 | R-heap-01 |
| `heap.from_list` | `fn from_list[A](xs: List[A]) -> Heap[A]` | Python `heapify`(F) / Rust `from` / OCaml `of_list` | 追加 | R-heap-01 |
| `heap.push` | `fn push[A](mut h: Heap[A], x: A) -> Unit` | Rust・Deno `push` / Python `heappush`(F) | 追加 | R-heap-01 |
| `heap.pop` | `fn pop[A](mut h: Heap[A]) -> A?` | Rust・Deno `pop` / Python `heappop`(F) / Swift `popMin` | 追加 | R-heap-01 |
| `heap.peek` | `fn peek[A](h: Heap[A]) -> A?` | Rust・Kotlin・Deno `peek` / Swift `min` | 追加 | R-heap-01 |
| `heap.len` | `fn len[A](h: Heap[A]) -> Int` | Rust `len` / Python `len` | 追加 | R-heap-01 |
| `heap.is_empty` | `fn is_empty[A](h: Heap[A]) -> Bool` | Rust `is_empty` / Kotlin `isEmpty` | 追加 | R-heap-01 |
| `heap.to_list` | `fn to_list[A](h: Heap[A]) -> List[A]` | Rust `into_sorted_vec` / Python `sorted(h)` | 追加 | R-heap-01 |
