<!-- description: Stdlib redesign L2/L3 — deque (new, R-L1-01): a double-ended queue whose push/pop at both ends are in-place mut verbs like list.push/pop, written in pure Almide -->
# L2/L3: deque（新しいモジュール）

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-01: 両端キュー `deque` を足す）
> 出典: Rust https://doc.rust-lang.org/std/collections/struct.VecDeque.html ・ Go https://pkg.go.dev/container/list ・ Python https://docs.python.org/3/library/collections.html#collections.deque ・ TS/JS https://jsr.io/@std/data-structures （Deno std。ECMAScript には無い）・ Gleam —（無い）・ Elixir https://www.erlang.org/doc/apps/stdlib/queue.html ・ OCaml https://ocaml.org/manual/latest/api/Queue.html ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.collections/-array-deque/ ・ Swift https://swiftpackageindex.com/apple/swift-collections/documentation/dequemodule

## L2: 役割の比較

| 役割 | Almide（案） | Rust | Go | Python | TS/JS | Gleam | Elixir(:queue) | OCaml(Queue) | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 空を作る・リストから | `new`・`from_list` | `new`・`from` | `list.New` | `deque()`・`deque(xs)` | `new Deque` | — | `new`・`from_list` | `create`・`of_seq` | `ArrayDeque()` | `Deque()` |
| 末尾に足す | `push`（R-deque-03） | `push_back` | `PushBack` | `append` | `push` | — | `in` | `push` | `addLast` | `append` |
| 先頭に足す | `push_front` | `push_front` | `PushFront` | `appendleft` | `unshift` | — | `in_r` | — | `addFirst` | `prepend` |
| 末尾から取る | `pop`（R-deque-03） | `pop_back` | `Remove(Back())` | `pop` | `pop` | — | `out_r` | — | `removeLast` | `popLast` |
| 先頭から取る | `pop_front` | `pop_front` | `Remove(Front())` | `popleft` | `shift` | — | `out` | `pop`/`take_opt` | `removeFirst` | `popFirst` |
| 先頭・末尾を見る | `first`・`last` | `front`・`back` | `Front`・`Back` | `d[0]`・`d[-1]` | `peekFront` | — | `peek`・`peek_r` | `peek_opt` | `first`・`last` | `first`・`last` |
| i 番目 | `get` | `get` | — | `d[i]` | — | — | — | — | `get` | `d[i]` |
| 長さ・空か | `len`・`is_empty` | `len`・`is_empty` | `Len` | `len` | `length` | — | `len`・`is_empty` | `length`・`is_empty` | `size`・`isEmpty` | `count`・`isEmpty` |
| リストへ | `to_list` | `iter().collect` | — | `list(d)` | `[...d]` | — | `to_list` | `to_seq` | `toList` | `Array(d)` |

### 完全性の規則

- 両端 × {足す, 取る, 見る} の 6 セルをすべて持ちます: `push_front`・`push`・`pop_front`・`pop`・`first`・`last`。末尾の 2 つは `list` と同じ `push`・`pop` です（R-deque-03）。
- **足す・取るは `mut` のその場書き換え**です。R-L0-04 の 2 は、先頭を名前に付けた `push_front`・`pop_front` を `push`・`pop` と同じ族として動詞集合に入れています。R-L0-04 の 3「ひとつの動詞は全モジュールで同じ様式」により、`list.push(mut xs, x)`・`list.pop(mut xs) -> A?` と同じくその場書き換えです。L1 の R-L1-01 も同じ形に直っています。
- **組を返すコピー版の `pop` は置きません。** 動かせない値（関数の引数など）から取りたいときは `var q2 = q` に写してから `pop_front(q2)` と書きます。もし将来コピー版を足すなら、組の順は `(Option[A], Deque[A])`（取った要素が先）にします。`list.pop` が要素の `Option[A]` を返すのと、読む順を同じにするためです。
- 取り出せないときは none（R-L0-06）。`get` の範囲外も none。
- 順序のある型としては宣言しないので、`deque.compare` は置きません（R-L0-16 は順序のある型だけが対象）。

### L2 の判定

- 新しいモジュールです。9 言語の多数派の役割をすべて持ちます。

## 案

### R-deque-01 `deque` モジュールを足す — ○ 採用

- 内容: 下の L3 の 12 関数。型は `Deque[A]`。中身は純粋な Almide（先頭用と末尾用の 2 つのリストか、`list` の上のリングバッファ。どちらでも、両端の足す・取るがならして O(1)）。
- 根拠の L0 規則: R-L0-04（その場書き換えは決まった動詞だけ。`push_<端>`・`pop_<端>` は R-L0-04 の 2 で `push`・`pop` の族として数える）、R-L0-06、R-L0-02（構築子は返す型のモジュール）、R-L0-07（`from_list`・`to_list`）。
- (a) 7 言語が標準に持っています（L1）。先頭の綴りは Rust の `push_front`/`pop_front` に合わせました。末尾は `list` と同じ `push`・`pop` です（R-deque-03。Swift の `Deque` も `Array` と同じ `append`・`popLast`）。
- (b) 純粋な計算です。native と wasm で同じです。host の機能は要りません。
- (c) 幅優先探索で、いまは `list.drop(xs, 1)` が毎回 O(n) です。他言語の deque を前提に書いた LLM のコードが、そのまま速く動きます。
- (d) 追加（新しいモジュール。`import` の要否は、`list`・`map` と同じく自動の import に入れる。stdlib_info の `AUTO_IMPORT_BUNDLED` に足す）。
- 採らなかった案: `List` の先頭からの取り出しを Go の `s[1:]` のように写さずに済ませ、deque を足さない。覚える型は減るが、`List` の表現を変えるのでコンパイラの深い所に手が入り、先頭に足す側は速くならない。他言語で LLM が見る形も「待ち行列は deque」（Python `collections.deque`、Rust `VecDeque`、Kotlin `ArrayDeque`）なので、deque を足す。`List` の先頭操作の速さは、`List` の性能を見直すときに別に測る。
- ゲート案: list と同じ境界入力の行列テスト（空からの `pop`、`get` の範囲外）、native と wasm の cross fixture、`push` を n 回して `pop_front` を n 回する時間が線形であることの計測（`check-perf-ratio.sh` の形）。
- issue: #3932

### R-deque-02 deque に高階関数の族と `contains`・`clear` を足す — ○ 採用

- いま: `deque` は 12 関数で、`contains` と D3 の族（`map`・`filter`・`fold`・`any`・`all`・`count`・`find`）を持ちません。`for x in d` も書けないので、中を調べるには毎回 `deque.to_list` を経ます。
- 案（先頭から末尾の順に走ります）:
  - `fn deque.contains[A](d: Deque[A], x: A) -> Bool`
  - `fn deque.map[A, B](d: Deque[A], f: (A) -> B) -> Deque[B]`
  - `fn deque.filter[A](d: Deque[A], f: (A) -> Bool) -> Deque[A]`
  - `fn deque.fold[A, B](d: Deque[A], init: B, f: (B, A) -> B) -> B`
  - `fn deque.any[A](d: Deque[A], f: (A) -> Bool) -> Bool`、`all`（同じ形）、`fn deque.count[A](d: Deque[A], f: (A) -> Bool) -> Int`
  - `fn deque.find[A](d: Deque[A], f: (A) -> Bool) -> A?`（先頭に近いものが先）
  - `fn deque.clear[A](mut d: Deque[A]) -> Unit`（R-L0-04 の `clear` の行列）
- 根拠の L0 規則: L2-list の完全性の規則 1・R-map-06（D3 の 7 つと `contains` はコレクションすべてに置く）、R-L0-01（list と同じ名前・同じコールバックの形）、R-L0-04（`clear` 以外はコピー）。
- (a) deque を調べる・変換する手段は 8/9 が持ちます（Python は反復と `in`、Erlang/Elixir の `:queue` は `all`・`any`・`filter`・`fold`・`member`、Rust `VecDeque` は `contains` と `iter()`、Kotlin・Swift はコレクションとして全部）。Go の `container/list` だけが手で回します。
- (b) 純粋な Almide の計算です。走る順は先頭から末尾に決まっているので、両 target で同じです。
- (c) list・map・set で `xs |> X.any(f)` と書けるのに deque だけ外れると、LLM は規則から推測して E002 になります。幅優先探索で「訪問済みか」を `deque.contains` で書くのはよくある形です。
- (d) 追加 9 です。新しいモジュールなので互換の影響はありません。
- 完全性の規則: 順序の決まったコレクション（list・deque）と集合系（map・set）は、D3 の 7 つと `contains` をすべて持ちます。走る順は `to_list` の順と同じです。
- ゲート案: R-map-06 のコレクションの族の行列に deque の列を足します。性質テスト: どのセルも `deque.to_list(d) |> list.<同名>(f)` と同じ結果です。
- issue: #3968

### R-deque-03 末尾の `push_back`・`pop_back` を `push`・`pop` にする — ○ 採用

- 内容: `deque.push(mut d, x)`・`deque.pop(mut d) -> A?` を末尾の足し引きにし、`push_back`・`pop_back` は置きません。`push_front`・`pop_front` は `deque` だけの関数に残します。
- 理由: [protocol の階層](protocol-view.md)の `Growable` の要件は `push`・`pop`（末尾）です。`list` と `deque` を同じ総称関数で扱えます。Swift の `Deque`（swift-collections）も、`Array` と同じ `append`・`popLast` に、`prepend`・`popFirst` を足す形です。
- (d) `deque` はこのリデザインで足すモジュールなので、改名ではなく追加する名前の変更です。
- issue: #4000

### R-deque-04 `deque` に `Ordered`・`Indexed`・`Growable` の要件をそろえる — ○ 採用

- 内容: `reverse`・`take`・`drop`・`take_end`・`drop_end`・`slice`・`set`・`insert`・`remove_at` を置きます。形と範囲の扱いは `list` の同じ関数と同じです。
- 理由: `deque` は `list` と同じく位置で読み書きできる列（Swift の `Deque` は `RandomAccessCollection`・`MutableCollection`・`RangeReplaceableCollection`）なので、階層の `Indexed`・`Growable` に入ります。
- (d) 追加 9。
- issue: #4000

## L3: 関数表

| 関数 | シグネチャ(案) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `deque.new` | `fn new[A]() -> Deque[A]` | Rust `new` / Python `deque()` / Elixir `new` | 追加 | R-deque-01 |
| `deque.from_list` | `fn from_list[A](xs: List[A]) -> Deque[A]` | Rust `from` / Elixir `from_list` | 追加 | R-deque-01 |
| `deque.to_list` | `fn to_list[A](d: Deque[A]) -> List[A]` | Elixir `to_list` / Kotlin `toList` | 追加 | R-deque-01 |
| `deque.len` | `fn len[A](d: Deque[A]) -> Int` | Rust `len` / Python `len` | 追加 | R-deque-01 |
| `deque.is_empty` | `fn is_empty[A](d: Deque[A]) -> Bool` | Rust `is_empty` / Kotlin `isEmpty` | 追加 | R-deque-01 |
| `deque.push` | `fn push[A](mut d: Deque[A], x: A) -> Unit` | Rust `push_back` / Go `PushBack` / Python `append` | 追加 | R-deque-01、R-deque-03 |
| `deque.push_front` | `fn push_front[A](mut d: Deque[A], x: A) -> Unit` | Rust `push_front` / Go `PushFront` / Python `appendleft` | 追加 | R-deque-01 |
| `deque.pop` | `fn pop[A](mut d: Deque[A]) -> A?` | Rust `pop_back` / Python `pop` / Swift `popLast` | 追加 | R-deque-01、R-deque-03 |
| `deque.pop_front` | `fn pop_front[A](mut d: Deque[A]) -> A?` | Rust `pop_front` / Python `popleft` / Swift `popFirst` | 追加 | R-deque-01 |
| `deque.first` | `fn first[A](d: Deque[A]) -> A?` | Kotlin・Swift `first` / Rust `front` | 追加 | R-deque-01 |
| `deque.last` | `fn last[A](d: Deque[A]) -> A?` | Kotlin・Swift `last` / Rust `back` | 追加 | R-deque-01 |
| `deque.get` | `fn get[A](d: Deque[A], i: Int) -> A?` | Rust・Kotlin `get` / Python `d[i]` | 追加 | R-deque-01 |
| `deque.contains` | `fn contains[A](d: Deque[A], x: A) -> Bool` | Rust `contains` / Python `in` / Erlang `member` | 追加 | R-deque-02 |
| `deque.map` | `fn map[A, B](d: Deque[A], f: (A) -> B) -> Deque[B]` | Kotlin・Swift（コレクション） | 追加 | R-deque-02 |
| `deque.filter` | `fn filter[A](d: Deque[A], f: (A) -> Bool) -> Deque[A]` | Erlang `:queue.filter` | 追加 | R-deque-02 |
| `deque.fold` | `fn fold[A, B](d: Deque[A], init: B, f: (B, A) -> B) -> B` | Erlang `:queue.fold` / OCaml `Queue.fold` | 追加 | R-deque-02 |
| `deque.any` | `fn any[A](d: Deque[A], f: (A) -> Bool) -> Bool` | Erlang `:queue.any` | 追加 | R-deque-02 |
| `deque.all` | `fn all[A](d: Deque[A], f: (A) -> Bool) -> Bool` | Erlang `:queue.all` | 追加 | R-deque-02 |
| `deque.count` | `fn count[A](d: Deque[A], f: (A) -> Bool) -> Int` | Kotlin `count {}` | 追加 | R-deque-02 |
| `deque.find` | `fn find[A](d: Deque[A], f: (A) -> Bool) -> A?` | Kotlin `find` / Swift `first(where:)` | 追加 | R-deque-02 |
| `deque.clear` | `fn clear[A](mut d: Deque[A]) -> Unit` | Rust `clear` / Python `clear` | 追加 | R-deque-02、R-L0-04 |
| `deque.reverse` | `fn reverse[A](d: Deque[A]) -> Deque[A]` | Swift `reversed` / Kotlin `reversed` | 追加 | R-deque-04 |
| `deque.take` | `fn take[A](d: Deque[A], n: Int) -> Deque[A]` | Swift `prefix` | 追加 | R-deque-04 |
| `deque.drop` | `fn drop[A](d: Deque[A], n: Int) -> Deque[A]` | Swift `dropFirst` | 追加 | R-deque-04 |
| `deque.take_end` | `fn take_end[A](d: Deque[A], n: Int) -> Deque[A]` | Swift `suffix` | 追加 | R-deque-04 |
| `deque.drop_end` | `fn drop_end[A](d: Deque[A], n: Int) -> Deque[A]` | Swift `dropLast` | 追加 | R-deque-04 |
| `deque.slice` | `fn slice[A](d: Deque[A], start: Int, end: Int) -> Deque[A]` | Swift `d[a..<b]` / Rust `range` | 追加 | R-deque-04 |
| `deque.set` | `fn set[A](d: Deque[A], i: Int, x: A) -> Deque[A]` | Swift `d[i] = x` / Rust `d[i] = x` | 追加 | R-deque-04 |
| `deque.insert` | `fn insert[A](d: Deque[A], i: Int, x: A) -> Deque[A]` | Swift `insert(_:at:)` / Rust `insert` | 追加 | R-deque-04 |
| `deque.remove_at` | `fn remove_at[A](d: Deque[A], i: Int) -> Deque[A]` | Swift `remove(at:)` / Rust `remove` | 追加 | R-deque-04 |
| `deque.enumerate` | （追加）`deque.enumerate(d: Deque[A]) -> List[(Int, A)]` | Swift `enumerated` / Rust `enumerate` | 追加 | R-L0-42 |
| `deque.zip` | （追加）`deque.zip(d: Deque[A], ys: List[B]) -> List[(A, B)]` | Swift `zip` / Rust `zip` | 追加 | R-L0-42 |
| `deque.zip_with` | （追加）`deque.zip_with(d: Deque[A], ys: List[B], f: (A, B) -> C) -> List[C]` | Kotlin `zip(other, f)` | 追加 | R-L0-42 |
| `deque.filter_map` | （追加）`deque.filter_map(d: Deque[A], f: (A) -> B?) -> List[B]` | Swift `compactMap` / Rust `filter_map` | 追加 | R-L0-42 |
| `deque.find_map` | （追加）`deque.find_map(d: Deque[A], f: (A) -> B?) -> B?` | Rust `find_map` | 追加 | R-L0-42 |
| `deque.reduce` | （追加）`deque.reduce(d: Deque[A], f: (A, A) -> A) -> A?` | Swift `reduce` / Rust `reduce` | 追加 | R-L0-42 |
| `deque.scan` | （追加）`deque.scan(d: Deque[A], init: B, f: (B, A) -> B) -> List[B]` | Rust `scan` / Kotlin `runningFold` | 追加 | R-L0-42 |
| `deque.partition` | （追加）`deque.partition(d: Deque[A], f: (A) -> Bool) -> (List[A], List[A])` | Kotlin `partition` | 追加 | R-L0-42 |
| `deque.group_by` | （追加）`deque.group_by(d: Deque[A], f: (A) -> B) -> Map[B, List[A]]` | Kotlin `groupBy` / Swift `Dictionary(grouping:)` | 追加 | R-L0-42 |
| `deque.sort` | （追加）`deque.sort(d: Deque[A]) -> List[A]` | Swift `sorted()` | 追加 | R-L0-42 |
| `deque.sort_by` | （追加）`deque.sort_by(d: Deque[A], f: (A) -> B) -> List[A]` | Kotlin `sortedBy` | 追加 | R-L0-42 |
| `deque.sort_with` | （追加）`deque.sort_with(d: Deque[A], cmp: (A, A) -> Int) -> List[A]` | Swift `sorted(by:)` | 追加 | R-L0-42 |
| `deque.min` | （追加）`deque.min(d: Deque[A]) -> A?` | Swift `min()` / Rust `min` | 追加 | R-L0-42 |
| `deque.max` | （追加）`deque.max(d: Deque[A]) -> A?` | Swift `max()` / Rust `max` | 追加 | R-L0-42 |
| `deque.min_by` | （追加）`deque.min_by(d: Deque[A], f: (A) -> K) -> A?` | Kotlin `minByOrNull` | 追加 | R-L0-42 |
| `deque.max_by` | （追加）`deque.max_by(d: Deque[A], f: (A) -> K) -> A?` | Kotlin `maxByOrNull` | 追加 | R-L0-42 |
| `deque.min_with` | （追加）`deque.min_with(d: Deque[A], cmp: (A, A) -> Int) -> A?` | Swift `min(by:)` | 追加 | R-L0-42 |
| `deque.max_with` | （追加）`deque.max_with(d: Deque[A], cmp: (A, A) -> Int) -> A?` | Swift `max(by:)` | 追加 | R-L0-42 |
| `deque.sum` | （追加）`deque.sum[A: Numeric](d: Deque[A]) -> A` | Rust `sum` / Kotlin `sum` | 追加 | R-L0-42 |
| `deque.product` | （追加）`deque.product[A: Numeric](d: Deque[A]) -> A` | Rust `product` | 追加 | R-L0-42 |
| `deque.find_index` | （追加）`deque.find_index(d: Deque[A], f: (A) -> Bool) -> Int?` | Swift `firstIndex(where:)` | 追加 | R-L0-42 |
| `deque.take_while` | （追加）`deque.take_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]` | Swift `prefix(while:)` | 追加 | R-L0-42 |
| `deque.drop_while` | （追加）`deque.drop_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]` | Swift `drop(while:)` | 追加 | R-L0-42 |
| `deque.chunk` | （追加）`deque.chunk(d: Deque[A], n: Int) -> List[Deque[A]]` | Kotlin `chunked` | 追加 | R-L0-42 |
| `deque.window` | （追加）`deque.window(d: Deque[A], n: Int) -> List[Deque[A]]` | Kotlin `windowed` | 追加 | R-L0-42 |
| `deque.split_at` | （追加）`deque.split_at(d: Deque[A], n: Int) -> (Deque[A], Deque[A])` | Rust `split_at` | 追加 | R-L0-42 |
| `deque.dedup` | （追加）`deque.dedup(d: Deque[A]) -> Deque[A]` | Rust `dedup` | 追加 | R-L0-42 |
| `deque.intersperse` | （追加）`deque.intersperse(d: Deque[A], sep: A) -> Deque[A]` | Rust `intersperse` | 追加 | R-L0-42 |
| `deque.unique` | （追加）`deque.unique(d: Deque[A]) -> Deque[A]` | Kotlin `distinct` | 追加 | R-L0-42 |
| `deque.unique_by` | （追加）`deque.unique_by(d: Deque[A], f: (A) -> K) -> Deque[A]` | Kotlin `distinctBy` | 追加 | R-L0-42 |

## L0 への差し戻し（処理済み）

1. 先頭を付けた動詞（`push_front`・`pop_front`）: R-L0-04 の 2 に、`push`・`pop` と同じ族として入りました。R-L0-G のゲートは、この 2 つを `mut` の動詞集合として受け付けます。
2. L1 の R-L1-01 の「`pop_*` はコピー版で組を返す」: L1 の本文を、R-L0-04 の 3 に合わせて `mut` のその場書き換えに直しました。このページと L1 は一致しています。
