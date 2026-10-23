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

- 順序の決め方は 3 つ: 既定（`A` の自然な順、最小が先）、キー `new_by(key: (A) -> K)`、比較関数 `new_with(cmp: (A, A) -> Int)`。接尾辞の意味は R-L0-10（`_by` はキー関数、`_with` は値の代わりに受け取る関数）と R-L0-16（比較関数は `(A, A) -> Int`）のとおりです。最大が先にしたいときは `new_with((a, b) => int.compare(b, a))` のように書きます。
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

### R-heap-02 heap には中の並びに依存しないセルだけを置く（`contains`・`any`・`all`・`count`・`filter`・`clear`） — ○ 採用

- いま: `heap` は 10 関数で、D3 の族も `contains` も持ちません。L2-heap は `to_list` を「`pop` を繰り返した順」と決めていますが、中の配列の並びは決めていません。
- 案（6 関数を追加）:
  - `fn heap.contains[A](h: Heap[A], x: A) -> Bool`
  - `fn heap.any[A](h: Heap[A], f: (A) -> Bool) -> Bool`、`all`（同じ形）、`fn heap.count[A](h: Heap[A], f: (A) -> Bool) -> Int`
  - `fn heap.filter[A](h: Heap[A], f: (A) -> Bool) -> Heap[A]`（順序の決め方（既定・`_by`・`_with`）と、同順位の入れた順を保つ）
  - `fn heap.clear[A](mut h: Heap[A]) -> Unit`（順序の決め方を保つ。R-L0-04 の `clear` の行列）
  - 意図して置かないもの: `map`・`fold`・`find`。並べた順が欲しいときは `heap.to_list(h) |> list.<動詞>` と書きます。
- 根拠の L0 規則: R-map-06（D3 の族）。ただし heap では、どの順で回るかを外から観測できるセルを置きません。R-L0-04（`clear` 以外はコピー）。
- (a) Rust `BinaryHeap` の `iter()` は「arbitrary order」で回り、`contains` を持ちません (F) https://doc.rust-lang.org/std/collections/struct.BinaryHeap.html 。JVM の `PriorityQueue` も反復の順を保証しません。どの言語も heap の中の並びを約束していません。
- (b) 並びに依存しないセルだけなので、中の配列の並べ方を将来変えても、両 target の結果も利用者のコードの結果も変わりません。
- (c) `map` を置くと、`new_by(key)`・`new_with(cmp)` の順序の決め方を `B` に引き継げず、黙って既定の順に戻ります。`fold`・`find` は、中の並びで回せば並びが外に漏れ、`pop` 順で回せば名前から想像できない並べ替えが隠れます。
- (d) 追加 6 です。新しいモジュールなので互換の影響はありません。
- 完全性の規則: heap は、戻り値が並びの順に依存しないセル（Bool・Int・同じ順序の Heap を返すもの）だけを D3 から持ちます。`map`・`fold`・`find` は R-map-06 の許容リストに理由つきで載せます。
- ゲート案: R-map-06 の行列に heap の列を足し、3 つの省略を許容リストに置きます。性質テスト: `heap.to_list(heap.filter(h, f)) == list.filter(heap.to_list(h), f)`。
- issue: #3968

### R-heap-03 `heap.new_with` の例を引き算にしない — ○ 採用（文書だけ）

- いま: 完全性の規則が「最大を先にしたいときは `new_with((a, b) => int.compare(b, a))`」と教えています。引き算は `Int` の端で折り返して符号が逆になります。
- 案: 例を `heap.new_with((a, b) => int.compare(b, a))` に直します（R-L0-16 の `int.compare`）。CHEATSHEET と docs/stdlib にも同じ例を使い、```` ```almide check ```` のフェンスにします。
- (c) LLM は文書の例をそのまま写します。
- (d) 文書だけです。`int.compare` が入ってから差し替えます。
- issue: #3968

### R-heap-04 `new_min`・`new_max` に分ける — × 不採用

- 理由: 既定は「最小が先」で、Python・Go・OCaml・Kotlin と同じです（R-L1-02）。最大を先にしたいときは `heap.new_with((a, b) => int.compare(b, a))`（差の `b - a` は `Int` の端で溢れるので使いません） か `new_by` で書きます。既定の向きは一覧の「共通の約束」に書きました。
- issue: なし

### R-heap-05 `heap.fold`・`heap.find` を足す — × 不採用

- 理由: 中の並び（配列の順）で回すと、その並びが外に漏れ、実装を変えると結果が変わります。取り出し順（小さい順）で回すと、名前から想像できない並べ替えが隠れます。順が要るときは `heap.to_list(h) |> list.fold(…)` と書けば、並べ替えが目に見えます。この判断は L2 の「完全性の規則」（戻り値が並びの順に依存しないセルだけを持つ）に書いたものと同じです。
- issue: なし

### R-heap-06 `==` は取り出す順の要素の並びで比べ、`"${h}"` もその順で書く — ○ 採用

- いま（#4033 の実装）: `Heap` は配列の二分ヒープです。組み込みの `==` は内部の配列を比べるので、同じ要素を持つ 2 つのヒープでも、入れた順が違うと等しくなりません。
- 案: `==` は、取り出す順（`pop` を繰り返した順）の要素の並びで比べます。`"${h}"` も `heap[1, 2, 5]` のようにその順で書きます。順序の決め方（`new`・`new_by`・`new_with`）が違う 2 つのヒープは、並びが同じなら等しいとします。
- 根拠の L0 規則: R-L0-37、R-heap-02（中の並びを外に見せない）。
- (a) Rust の `BinaryHeap`、Swift の `Heap` は等しさを持ちません。Almide はすべての値の型が `==` を持つ（`Fn` を除く）ので、持たない選択はできません。内部の配列で比べると R-heap-02 に反するため、外から見える並びで比べます。
- issue: #4033

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
| `heap.contains` | `fn contains[A](h: Heap[A], x: A) -> Bool` | Python `in`（list として） | 追加 | R-heap-02 |
| `heap.any` | `fn any[A](h: Heap[A], f: (A) -> Bool) -> Bool` | Kotlin `any`（PriorityQueue） | 追加 | R-heap-02 |
| `heap.all` | `fn all[A](h: Heap[A], f: (A) -> Bool) -> Bool` | Kotlin `all` | 追加 | R-heap-02 |
| `heap.count` | `fn count[A](h: Heap[A], f: (A) -> Bool) -> Int` | Kotlin `count {}` | 追加 | R-heap-02 |
| `heap.filter` | `fn filter[A](h: Heap[A], f: (A) -> Bool) -> Heap[A]` | Rust `retain`（その場） | 追加 | R-heap-02 |
| `heap.clear` | `fn clear[A](mut h: Heap[A]) -> Unit` | Rust `clear` / Kotlin `clear` | 追加 | R-heap-02、R-L0-04 |
