<!-- description: Stdlib redesign L2/L3 — deque (new, R-L1-01): a double-ended queue whose push/pop at both ends are in-place mut verbs like list.push/pop, written in pure Almide -->
# L2/L3: deque（新しいモジュール）

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-01: 両端キュー `deque` を足す）
> 出典: Rust https://doc.rust-lang.org/std/collections/struct.VecDeque.html ・ Go https://pkg.go.dev/container/list ・ Python https://docs.python.org/3/library/collections.html#collections.deque ・ TS/JS https://jsr.io/@std/data-structures （Deno std。ECMAScript には無い）・ Gleam —（無い）・ Elixir https://www.erlang.org/doc/apps/stdlib/queue.html ・ OCaml https://ocaml.org/manual/latest/api/Queue.html ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.collections/-array-deque/ ・ Swift https://swiftpackageindex.com/apple/swift-collections/documentation/dequemodule

## L2: 役割の比較

| 役割 | Almide（案） | Rust | Go | Python | TS/JS | Gleam | Elixir(:queue) | OCaml(Queue) | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 空を作る・リストから | `new`・`from_list` | `new`・`from` | `list.New` | `deque()`・`deque(xs)` | `new Deque` | — | `new`・`from_list` | `create`・`of_seq` | `ArrayDeque()` | `Deque()` |
| 末尾に足す | `push_back` | `push_back` | `PushBack` | `append` | `push` | — | `in` | `push` | `addLast` | `append` |
| 先頭に足す | `push_front` | `push_front` | `PushFront` | `appendleft` | `unshift` | — | `in_r` | — | `addFirst` | `prepend` |
| 末尾から取る | `pop_back` | `pop_back` | `Remove(Back())` | `pop` | `pop` | — | `out_r` | — | `removeLast` | `popLast` |
| 先頭から取る | `pop_front` | `pop_front` | `Remove(Front())` | `popleft` | `shift` | — | `out` | `pop`/`take_opt` | `removeFirst` | `popFirst` |
| 先頭・末尾を見る | `first`・`last` | `front`・`back` | `Front`・`Back` | `d[0]`・`d[-1]` | `peekFront` | — | `peek`・`peek_r` | `peek_opt` | `first`・`last` | `first`・`last` |
| i 番目 | `get` | `get` | — | `d[i]` | — | — | — | — | `get` | `d[i]` |
| 長さ・空か | `len`・`is_empty` | `len`・`is_empty` | `Len` | `len` | `length` | — | `len`・`is_empty` | `length`・`is_empty` | `size`・`isEmpty` | `count`・`isEmpty` |
| リストへ | `to_list` | `iter().collect` | — | `list(d)` | `[...d]` | — | `to_list` | `to_seq` | `toList` | `Array(d)` |

### 完全性の規則

- 両端 × {足す, 取る, 見る} の 6 セルをすべて持ちます: `push_front`・`push_back`・`pop_front`・`pop_back`・`first`・`last`。
- **足す・取るは `mut` のその場書き換え**です。R-L0-04 の 2 は、端を名前に付けた `push_front`・`push_back`・`pop_front`・`pop_back` を `push`・`pop` と同じ族として動詞集合に入れています。R-L0-04 の 3「ひとつの動詞は全モジュールで同じ様式」により、`list.push(mut xs, x)`・`list.pop(mut xs) -> A?` と同じくその場書き換えです。L1 の R-L1-01 も同じ形に直っています。
- **組を返すコピー版の `pop` は置きません。** 動かせない値（関数の引数など）から取りたいときは `var q2 = q` に写してから `pop_front(q2)` と書きます。もし将来コピー版を足すなら、組の順は `(Option[A], Deque[A])`（取った要素が先）にします。`list.pop` が要素の `Option[A]` を返すのと、読む順を同じにするためです。
- 取り出せないときは none（R-L0-06）。`get` の範囲外も none。
- 順序のある型としては宣言しないので、`deque.compare` は置きません（R-L0-16 は順序のある型だけが対象）。

### L2 の判定

- 新しいモジュールです。9 言語の多数派の役割をすべて持ちます。

## 案

### R-deque-01 `deque` モジュールを足す — ○ 採用

- 内容: 下の L3 の 12 関数。型は `Deque[A]`。中身は純粋な Almide（先頭用と末尾用の 2 つのリストか、`list` の上のリングバッファ。どちらでも、両端の足す・取るがならして O(1)）。
- 根拠の L0 規則: R-L0-04（その場書き換えは決まった動詞だけ。`push_<端>`・`pop_<端>` は R-L0-04 の 2 で `push`・`pop` の族として数える）、R-L0-06、R-L0-02（構築子は返す型のモジュール）、R-L0-07（`from_list`・`to_list`）。
- (a) 7 言語が標準に持っています（L1）。綴りは Rust の `push_back`/`pop_front` に合わせました（Go の `PushBack`/`PushFront` と同じ組）。
- (b) 純粋な計算です。native と wasm で同じです。host の機能は要りません。
- (c) 幅優先探索で、いまは `list.drop(xs, 1)` が毎回 O(n) です。他言語の deque を前提に書いた LLM のコードが、そのまま速く動きます。
- (d) 追加（新しいモジュール。`import` の要否は、`list`・`map` と同じく自動の import に入れる。stdlib_info の `AUTO_IMPORT_BUNDLED` に足す）。
- ゲート案: list と同じ境界入力の行列テスト（空からの `pop`、`get` の範囲外）、native と wasm の cross fixture、`push_back` を n 回して `pop_front` を n 回する時間が線形であることの計測（`check-perf-ratio.sh` の形）。
- issue: #3932

## L3: 関数表

| 関数 | シグネチャ(案) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `deque.new` | `fn new[A]() -> Deque[A]` | Rust `new` / Python `deque()` / Elixir `new` | 追加 | R-deque-01 |
| `deque.from_list` | `fn from_list[A](xs: List[A]) -> Deque[A]` | Rust `from` / Elixir `from_list` | 追加 | R-deque-01 |
| `deque.to_list` | `fn to_list[A](d: Deque[A]) -> List[A]` | Elixir `to_list` / Kotlin `toList` | 追加 | R-deque-01 |
| `deque.len` | `fn len[A](d: Deque[A]) -> Int` | Rust `len` / Python `len` | 追加 | R-deque-01 |
| `deque.is_empty` | `fn is_empty[A](d: Deque[A]) -> Bool` | Rust `is_empty` / Kotlin `isEmpty` | 追加 | R-deque-01 |
| `deque.push_back` | `fn push_back[A](mut d: Deque[A], x: A) -> Unit` | Rust `push_back` / Go `PushBack` / Python `append` | 追加 | R-deque-01 |
| `deque.push_front` | `fn push_front[A](mut d: Deque[A], x: A) -> Unit` | Rust `push_front` / Go `PushFront` / Python `appendleft` | 追加 | R-deque-01 |
| `deque.pop_back` | `fn pop_back[A](mut d: Deque[A]) -> A?` | Rust `pop_back` / Python `pop` / Swift `popLast` | 追加 | R-deque-01 |
| `deque.pop_front` | `fn pop_front[A](mut d: Deque[A]) -> A?` | Rust `pop_front` / Python `popleft` / Swift `popFirst` | 追加 | R-deque-01 |
| `deque.first` | `fn first[A](d: Deque[A]) -> A?` | Kotlin・Swift `first` / Rust `front` | 追加 | R-deque-01 |
| `deque.last` | `fn last[A](d: Deque[A]) -> A?` | Kotlin・Swift `last` / Rust `back` | 追加 | R-deque-01 |
| `deque.get` | `fn get[A](d: Deque[A], i: Int) -> A?` | Rust・Kotlin `get` / Python `d[i]` | 追加 | R-deque-01 |

## L0 への差し戻し（処理済み）

1. 端を付けた動詞（`push_front`・`push_back`・`pop_front`・`pop_back`）: R-L0-04 の 2 に、`push`・`pop` と同じ族として入りました。R-L0-G のゲートは、この 4 つを `mut` の動詞集合として受け付けます。
2. L1 の R-L1-01 の「`pop_*` はコピー版で組を返す」: L1 の本文を、R-L0-04 の 3 に合わせて `mut` のその場書き換えに直しました。このページと L1 は一致しています。
