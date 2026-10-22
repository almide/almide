<!-- description: Stdlib redesign L2/L3 — list: one spelling per role (window/windows, get_or, shuffle), comparator sort and min_by/max_by, unzip, Float sums, and one boundary rule for slice/chunk/window -->
# L2/L3: list

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)
> 出典: Rust https://doc.rust-lang.org/std/primitive.slice.html (F)・https://doc.rust-lang.org/std/iter/trait.Iterator.html ・ Go https://pkg.go.dev/slices (F) ・ Python https://docs.python.org/3/library/functions.html (F)・https://docs.python.org/3/library/stdtypes.html ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array (F) ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/list.html (F) ・ Elixir https://elixir.hexdocs.pm/Enum.html (F) ・ OCaml https://ocaml.org/manual/latest/api/List.html (F) ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.collections/ （`minByOrNull` は https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.collections/min-by-or-null.html (F)） ・ Swift https://developer.apple.com/documentation/swift/array (F)

読み方の約束（L0 と同じ）:
- `mut`（読み: ミュート）= 呼び出し元の `var` をその場で書き換える引数
- `??`（読み: ナル合体）= 左が none なら右を使う演算子
- `|>`（読み: パイプ）= 左の値を右の関数の第 1 引数に差し込む演算子
- (F) = 今回、公式ドキュメントを取り寄せて確かめたセル。印のないセルは facts/naming-conventions.md の記載か、よく知られた綴り

実測の環境: `almide 0.68.0 (dev, 437b8f127)`。プローブは `scratchpad/sr/probe-list/p1.almd`〜`p6.almd`。
使用数は `/usr/bin/grep -rEo '\blist\.<fn>\b'` の出現数で、spec / tests / docs / stdlib の順に書きます（定義行と生成された文書の行を含むので、上限の値です）。

---

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 長さ | `len`（`length` は非推奨） | `len` | `len()` | `len()` | `length` | `length` | `length`/`count` | `length` | `size` | `count` |
| 空か | `is_empty` | `is_empty` | — | — | — | `is_empty` | `empty?` | `is_empty` (F) | `isEmpty` | `isEmpty` |
| 位置で取る（Option） | `get` | `get` | — | — | `at` (F) | — | `at`/`fetch` (F) | `nth_opt` (F) | `getOrNull` | — |
| 既定値つきで取る | `get_or` | — | — | — | — | — | `at(xs,i,d)` | — | `getOrElse` | — |
| 先頭・末尾 | `first`/`last` | `first`/`last` (F) | — | — | `at(0)`/`at(-1)` | `first`/`last` (F) | `at`/`List.first` | — | `firstOrNull`/`lastOrNull` | `first`/`last` |
| 位置に書く（コピー） | `set`/`update` | — | — | — | `with` (F) | — | `List.replace_at` | — | — | — |
| 位置に挿入・位置を削除（コピー） | `insert`/`remove_at` | — | `Insert`/`Delete` (F) | — | `toSpliced` (F) | — | `List.insert_at`/`delete_at` | — | — | — |
| 末尾に足す・取る（その場） | `push`/`pop`（`mut`） | `push`/`pop` | `append` | `append`/`pop` | `push`/`pop` | — | — | — | `add`/`removeLastOrNull` | `append`/`popLast` |
| 全部消す（その場） | `clear`（`mut`） | `clear` | `clear()` | `clear` | `length = 0` | — | — | — | `clear` | `removeAll` |
| 連結 | `+` 演算子 | `concat` (F) | `Concat` (F) | `+` | `concat` | `append` (F) | `++` | `@`/`append` | `plus` | `+` |
| 含むか | `contains` | `contains` (F) | `Contains` (F) | `in` | `includes` (F) | `contains` | `member?` | `mem` (F) | `contains` | `contains` |
| 値の位置 | `index_of` | `iter().position` | `Index` (F) | `index` | `indexOf` (F) | — | `find_index` (F) | `find_index` (F) | `indexOf` | `firstIndex(of:)` (F) |
| 述語の位置 | `find_index` | `position` | `IndexFunc` (F) | — | `findIndex` | — | `find_index` (F) | `find_index` (F) | `indexOfFirst` | `firstIndex(where:)` |
| 述語で探す | `find` | `find` | — | — | `find` | `find` | `find` | `find_opt` | `find` | `first(where:)` |
| 変換・絞り込み | `map`/`filter` | `map`/`filter` | — | 内包表記 | `map`/`filter` | `map`/`filter` | `map`/`filter` | `map`/`filter` | `map`/`filter` | `map`/`filter` |
| none を除く変換 | `filter_map` | `filter_map` | — | — | — | `filter_map` | — | `filter_map` (F) | `mapNotNull` | `compactMap` (F) |
| 最初の some | `find_map` | `find_map` | — | — | — | `find_map` (F) | `find_value` (F) | `find_map` (F) | `firstNotNullOfOrNull` | — |
| 平らにする | `flatten`/`flat_map` | `flatten`/`flat_map` | — | — | `flat` (F)/`flatMap` (F) | `flatten` (F)/`flat_map` (F) | `concat`/`flat_map` (F) | `concat`/`concat_map` (F) | `flatten`/`flatMap` | `joined`/`flatMap` (F) |
| 畳み込み | `fold`/`reduce` | `fold`/`reduce` | — | `functools.reduce` | `reduce` (F) | `fold`/`reduce` | `reduce` | `fold_left` (F) | `fold`/`reduce` | `reduce` (F) |
| 途中経過の畳み込み | `scan` | `scan` | — | `itertools.accumulate` | — | `scan` | `scan` (F) | — | `runningFold`/`scan` | — |
| どれか・全部 | `any`/`all` | `any`/`all` | `ContainsFunc` (F) | `any`/`all` | `some`/`every` | `any`/`all` | `any?`/`all?` | `exists`/`for_all` (F) | `any`/`all` | `contains(where:)` (F)/`allSatisfy` (F) |
| 述語で数える | `count` | `filter().count()` | — | — | — | `count` (F) | `count/2` (F) | — | `count{}` | `count(where:)` |
| 並べ替え（自然順） | `sort` | `sort` | `Sort` | `sorted` (F) | `toSorted` (F) | `sort(by:)` | `sort` | `sort` (F) | `sorted` | `sorted()` (F) |
| キーで並べ替え | `sort_by` | `sort_by_key` (F) | — | `sorted(key=)` (F) | — | — | `sort_by` (F) | — | `sortedBy` | — |
| 比較関数で並べ替え | **なし** | `sort_by` (F) | `SortFunc` (F) | `cmp_to_key` | `toSorted(cmp)` (F) | `sort(by:)` | `sort/2` | `sort cmp` (F) | `sortedWith` | `sorted(by:)` (F) |
| 反転 | `reverse` | `reverse` | `Reverse` (F) | `reversed` | `toReversed` (F) | `reverse` | `reverse` | `rev` | `reversed` | `reversed()` (F) |
| 最小・最大 | `min`/`max` | `min`/`max` | `Min`/`Max` (F) | `min`/`max` (F) | — | `max(with:)` (F) | `min`/`max` | — | `minOrNull` | `min()`/`max()` |
| キーで最小・最大 | **なし** | `min_by_key` | `MinFunc` (F, 比較) | `min(key=)` (F) | — | — (F) | `min_by`/`max_by` (F) | — | `minByOrNull` (F) | `min(by:)` (F, 比較) |
| 合計・積（Int） | `sum`/`product` | `sum`/`product` | — | `sum` (F) | — | `int.sum` | `sum`/`product` (F) | — | `sum` | — |
| 合計（Float） | **なし**（`List[Int]` だけ） | `sum` | — | `sum` (F) | — | `float.sum` | `sum` (F) | — | `sum` | — |
| 重複除去 | `unique`/`unique_by` | `dedup`（隣接） | `Compact`（隣接, F） | `dict.fromkeys` | `new Set` | `unique` (F) | `uniq`/`uniq_by` (F) | `sort_uniq` | `distinct`/`distinctBy` | — |
| 隣接の重複をまとめる | `dedup` | `dedup` | `Compact` (F) | — | — | — | `dedup` (F) | — | — | — |
| キーで分ける | `group_by` | — | — | `itertools.groupby` | `Object.groupBy` | — | `group_by` | — | `groupBy` | `Dictionary(grouping:)` |
| 述語で 2 つに分ける | `partition` | `partition` | — | — | — | `partition` | `split_with` | `partition` (F) | `partition` | — |
| 先頭・末尾を取る／落とす | `take`/`drop`/`take_end`/`drop_end`/`tail` | `take`/`skip` | — | `xs[:n]` | `slice` | `take`/`drop` | `take`/`drop` | `take`/`drop` (F) | `take`/`drop`/`takeLast`/`dropLast` | `prefix`/`dropFirst`/`suffix`/`dropLast` |
| 条件の間だけ | `take_while`/`drop_while` | `take_while`/`skip_while` | — | `itertools.takewhile` | — | `take_while` (F) | `take_while` (F) | `take_while` (F) | `takeWhile` | `prefix(while:)` (F) |
| 区間 | `slice` | `&v[a..b]`/`get` (F) | `xs[a:b]` | `xs[a:b]` | `slice` (F) | — | `slice` (F) | — | `subList`/`slice` | `xs[a..<b]` |
| 位置で 2 つに分ける | `split_at` | `split_at` (F) | — | — | — | `split` | `split` | — | — | — |
| n 個ずつに分ける | `chunk` | `chunks` (F) | `Chunk` (F) | `itertools.batched` | — | `sized_chunk` (F) | `chunk_every` (F) | — | `chunked` (F) | — |
| 長さ n の窓 | `window` と `windows`（同じもの） | `windows` (F) | — | — | — | `window` (F) | `chunk_every(n,1,:discard)` | — | `windowed` | — |
| 組にする・関数で組む | `zip`/`zip_with` | `zip` | — | `zip` | — | `zip`/`map2` | `zip`/`zip_with` (F) | `combine` (F) | `zip` | `zip` (F) |
| 組をほどく | **なし** | `unzip` | — | `zip(*xs)` | — | `unzip` (F) | `unzip` (F) | `split` (F) | `unzip` | — |
| 添字をつける | `enumerate` | `enumerate` | — | `enumerate` | `entries` | `index_map` | `with_index` | `mapi` | `withIndex` | `enumerated()` (F) |
| 範囲 | `range` | `a..b` | — | `range` | — | `range` | `a..b` | `init` (F) | `until`/`..<` | `a..<b` |
| 刻み幅つきの範囲 | **なし** | `step_by` | — | `range(a,b,s)` | — | — | `a..b//s` | — | `step` | `stride(from:to:by:)` |
| 繰り返し・反復で作る | `repeat`/`iterate` | `repeat` | `Repeat` (F) | `[x]*n` | `Array(n).fill` | `repeat` | `List.duplicate` | `init` (F) | `List(n){}` | `Array(repeating:count:)` |
| 間に挟む・つなげる | `intersperse`/`join` | `join` (F) | — | `sep.join` | `join` (F) | `intersperse` | `intersperse`/`join` | — | `joinToString` | `joined(separator:)` (F) |
| 二分探索 | `binary_search`（`List[Int]` だけ） | `binary_search` (F) | `BinarySearch` (F) | `bisect` | — | — | — | — | `binarySearch` | — |
| 乱順 | `shuffle`（純粋な `fn`）と `random.shuffle`（effect） | `rand` 外部 | — | `random.shuffle` | — | `shuffle` | `shuffle` (F) | — | `shuffled` | `shuffled()` (F) |
| 後ろから探す | — | `rposition`/`rfind` | — | — | `lastIndexOf`/`findLast` (F) | — | — | — | `lastIndexOf`/`findLast` | `lastIndex(of:)` (F)/`last(where:)` (F) |
| 隣どうしの条件でまとめる | — | `chunk_by` (F) | — | `itertools.groupby` | — | `chunk(by:)` (F) | `chunk_by` (F) | — | — | — |
| 並んでいるか | — | `is_sorted` (F) | `IsSorted` (F) | — | — | — | — | — | — | — |
| 全部同じ値で埋める（その場） | — | `fill` (F) | — | — | `fill` (F) | — | — | — | `fill` | — |

### 完全性の規則

1. **コレクションの高階関数の族**（D3）: `map`・`filter`・`fold`・`any`・`all`・`count`・`find` の 7 つは list・map・set のすべてに置きます。list はすでに 7 つとも持っています。
2. **並べ替えと最小・最大の族**（R-L0-10）: 「自然順」「キー関数 `_by`」の 2 列を、`sort`・`min`・`max` の 3 行すべてに置きます。比較関数 `_with` の列は `sort_with` だけに置きます。`min_with`/`max_with` を置かないのは意図した省略です（`xs |> list.sort_with(cmp) |> list.first` で書けて、9 言語でも比較関数版の最小・最大を持つのは Go・Swift・Gleam・Kotlin だけで、綴りが揃っていないため）。
3. **組の族**: `zip` があるなら逆向きの `unzip` を置きます（`zip` 6/9、`unzip` 5/9）。
4. **数の畳み込みの族**: `sum`・`product`・`min`・`max` は、`Int` と `Float` の両方の要素型で使えるようにします。`min`/`max`/`sort` はすでに型で振り分けているので、`sum`/`product` だけが穴です。
5. **大きさを指定して分ける族**（`chunk`・`window`）: 名前は単数形の動詞にそろえ、`n <= 0` は契約違反として止めます（R-L0-06 の 3、R-L0-12）。
6. **その場書き換え**（R-L0-04）: list の `mut` 関数は `push`・`pop`・`clear` の 3 つだけです。`set`・`insert`・`update`・`remove_at`・`swap` はコピーです。`with_capacity` は主語のない構築子で、`mut` ではありません。これ以上の `mut` 関数（`extend`、`fill`、`sort_in_place`）は足しません。
7. **意図した省略**: 連結は `+` 演算子が担うので `concat`/`append` 関数は置きません。降順は `sort_with` か `sort |> reverse` で書けるので `sort_desc` は置きません。

### L2 の判定

- **重複**: `window` ≡ `windows`（別々の intrinsic で、本体は 1 文字も違わない。`runtime/rs/src/list.rs:52` と `:257`）→ R-list-03。`shuffle` ≡ `random.shuffle`（片方だけが純粋な `fn`）→ R-list-05。`get_or` ≡ `get ?? d` → R-list-01。`length` ≡ `len` → R-list-02。
- **表に載っているべきでないもの**: `bundled_probe` → R-list-04（#3850）。
- **抜けている役割（5/9 以上）**: 比較関数での並べ替え（8/9）、キーで最小・最大（5/9: Rust・Python・Elixir・Kotlin がキー関数、Go・Swift が比較関数）→ R-list-06。組をほどく（5/9）→ R-list-07。Float の合計（5/9）→ R-list-08。刻み幅つきの範囲（5/9）→ R-list-09。
- **族の穴・端の不揃い**: `slice` だけ負の開始位置で空を返す、`chunk` の負の n が全体を返す → R-list-10。
- **範囲外の書き込み**: `set`・`update`・`swap`・`remove_at` は範囲外で何もせず、`insert` は範囲外の位置を末尾に足します。添字代入 `xs[i] = v` は abort します → R-list-13。
- **古いヒント表**: E002 の「もしかして」表が、生きている関数から離れる向きを指している → R-list-11。
- **足さないもの**: 後ろから探す（4/9）、`chunk_by`（4/9）、`is_sorted`（2/9）、`fill`（3/9）、`concat` 関数 → R-list-12（×）。
- **公開面に出ない重複**: #1460 の `_str` 双子（`list_contains_str` など 120 本）は `stdlib/list_*.almd` の内部実装の名前で、公開の関数表（interface JSON の 67 関数）には 1 本も出ていません。綴りの問題としては L2 の対象外です。#1460 の protocol 化の裁定は、そのまま #1460 で扱います。

---

## 案

### R-list-00 その場書き換えとコピーの様式を据え置き、R-L0-05 の保険をかける — ○ 採用

- 内容: 変更しません。`push`・`pop`・`clear` は `mut`（その場）、ほかの関数はすべてコピーです。`list.set`・`list.insert`・`list.update`・`list.remove_at`・`list.swap` の動詞（`set`・`insert`・`update`・`remove`）は、R-L0-04 のとおりコピー側の動詞で、map・set と同じ様式です。`with_capacity` は据え置きます。範囲外の位置を渡したときの挙動は、別の案 R-list-13 で abort にそろえます。
- 根拠の L0 規則: R-L0-04、R-L0-05
- (a) Gleam・Elixir・OCaml List と同じ、コピーが既定の流儀です。Rust・Python・JS・Kotlin・Swift の `push`/`append`/`pop` だけが、その場書き換えとして残ります。
- (b) native と wasm の両方で、`mut` の書き戻し経路がすでにあります。
- (c) 危ないのは、他言語の癖で `list.sort(xs)` や `list.set(xs, 0, 9)` を文として書く書き方です。実測（`p1.almd`）では、`var xs = [3, 1, 2]` に `list.sort(xs)`・`list.reverse(xs)`・`list.set(xs, 0, 9)` を文として書いても、エラーにならず `[3, 1, 2]` のまま表示されます。R-L0-05 が入ると、この 3 行はどれも E042 で止まります。list で特に危ないのは `sort`・`sort_by`・`reverse`・`set`・`insert`・`update`・`remove_at`・`swap` です（Python・JS・Rust・Kotlin でその場書き換えの綴りと同じ名前のもの）。
- (d) 変更なし。
- ゲート案: R-L0-G の `mut` 動詞検査で、list の `mut` 関数が `push`・`pop`・`clear` だけであることを固定します。R-L0-05 の fixture の broken 側に `list.sort(xs)` を 1 行入れます。
- issue: R-L0-05 の新規 issue に含めます。
- issue: #3928

### R-list-01 `list.get_or` を撤去し `??` に一本化する — ○ 採用

- 内容: `list.get_or(xs, i, d)` を撤去します。置き換えは `list.get(xs, i) ?? d` です。
- 根拠の L0 規則: R-L0-06、R-L0-14（D1）
- (a) 既定値版を関数で持つのは Elixir の `Enum.at(xs, i, d)` と Kotlin の `getOrElse` だけです。
- (b) 影響なし。`??` は ADR-0005 で `option.unwrap_or_else` に脱糖され、両 target で同じです。実測（`p1.almd`）で `list.get_or([1,2], 5, 0)` と `list.get([1,2], 5) ?? 0` はどちらも `0` です。
- (c) `??` は CHEATSHEET が教える唯一の書き方です。`get_or` は既定値を必ず先に評価するので、`??` の遅延評価と微妙に違う点を覚える必要もなくなります。
- (d) 撤去。`@deprecated(since=15, use="??")`、`[[deprecation]]` の `removal_epoch = 16`。`almide fix` は `list.get_or(a, b, c)` → `list.get(a, b) ?? c` に書き換えます。`removed_spellings.rs` のヒント: 「`list.get(xs, i) ?? d`」。内部の双子 `list_get_or_str.almd` も一緒に消えます。使用数 99 / 9 / 21 / 7（research 9）。
- ゲート案: R-L0-G に「名前が `_or` で終わる公開関数は 0 本」の規則を足します（`option.unwrap_or`・`result.unwrap_or`・`map.get_or`・`bytes.get_or`・`args.option_or` と同じ窓で 0 にします）。
- issue: #3913

### R-list-02 `list.length` を epoch 16 で撤去する — ○ 採用

- 内容: すでに `@deprecated`（dialect 5 から）の `list.length` に、撤去の日付を付けます。
- 根拠の L0 規則: R-L0-11、R-L0-14（D7）
- (a) 長さの綴りは割れています（length 4、len 3）。R-L0-00 により `len` を据え置きます。
- (b) 影響なし。
- (c) 実測（`p3.almd`）: いまは E052 の警告と `almide fix` の書き換えが出ます。撤去後は `removed_spellings.rs` のヒント付きのエラーになります。
- (d) 撤去。いまの `@deprecated` は dialect 5 からなので、`[[deprecation]]` の `since_epoch = 5` のまま、`use="list.len"` と `removal_epoch = 16` を書き足します（since=15 の新しい窓は置きません。すでに窓が開いているため）。`almide fix` は `list.length(xs)` → `list.len(xs)` に 1 対 1 で書き換えます。撤去後のヒント: 「`list.len(xs)`」。使用数 4 / 7 / 9 / 1。
- ゲート案: 同義語ゲートの `len`/`length`/`size` 語彙の規則（既存）。
- issue: #3851 A（既存）

### R-list-03 `list.windows` を撤去し、`list.window` を残す — ○ 採用

- 内容: `list.window` と `list.windows` は同じ関数です（`runtime/rs/src/list.rs:52` と `:257` の本体が同一。実測 `p1.almd` で `[[1, 2], [2, 3], [3, 4]]` が一致）。別々の intrinsic（`almide_rt_list_windows` と `almide_rt_list_window`）に結び付いているので、いまの同義語ゲートは検出できません。単数形の `window` を残し、`windows` を撤去します。
- 根拠の L0 規則: R-L0-01、R-L0-00、R-L0-14
- 単数形を選ぶ理由:
  - Almide の中の一貫性: 同じ「大きさ n で分ける」族の `list.chunk` が単数形の動詞です。`window` にそろえると、list の中で族の形が 1 つになります。
  - 一貫性を最後まで通したときの変更の量: 単数形にそろえると、撤去は `list.windows`（18 / 3 / 10 / 1 = 32）と、bytes 側の `bytes.chunks` → `bytes.chunk`（6 / 0 / 5 / 1 = 12）で、合計 44 か所です。複数形にそろえると、`list.window`（6 / 4 / 7 / 1 = 18）と `list.chunk` → `list.chunks`（31 / 3 / 16 / 2 = 52）で、合計 70 か所です。
  - (a) 他言語の綴りは割れています（Rust `windows` (F)、Gleam `window` (F)、Kotlin `windowed`）。多数派はないので、Almide の中の衝突の解消（R-L0-00 の 1）で決めます。
- (b) 影響なし。本体は同じです。
- (c) 2 つあると、LLM は Rust の癖で `windows` を書き、`chunk` との対で `window` も書きます。1 つにすれば、`chunk` から類推した綴りが必ず当たります。
- (d) 撤去。`@deprecated(since=15, use="list.window")`、`removal_epoch = 16`、`almide fix` で 1 対 1 に書き換えます。`windows` の使用数 18 / 3 / 10 / 1。
- ゲート案: 同義語ゲートの規則 6（同じモジュールで別の intrinsic が同じ本体を持つもの、#3851 F）が、この組を検出できることを fixture で確かめます。
- issue: #3851（既存。C 節に list の組として追加します）

### R-list-04 `list.bundled_probe` を公開面から外す — ○ 採用

- 内容: `list.bundled_probe(n) = n + 1` は、バンドル呼び出しの経路を確かめるための関数です。公開面から外します。経路の回帰テストは、同じ経路を通る `list.split_at` と `list.iterate` に任せます（`crates/almide-mir/src/pipeline.rs:79` が 3 つを並べて挙げています）。どうしても専用の関数が要るなら、検査器が公開面に出さない `__bundled_probe` に改名します。
- 根拠の L0 規則: R-L0-01（意味のない綴りを表に置かない）、R-L0-14（D10）
- (a) どの言語の標準にも、この種の関数は公開されていません。
- (b) 影響なし。実測（`p1.almd`）で `list.bundled_probe(4)` は `5` です。
- (c) 関数表は、LLM が「list に何があるか」を判断する材料です（#1483）。意味のない関数が 1 本あるだけで、もっともらしい綴りが 1 つ増えます。
- (d) 撤去。`@deprecated(since=15, use="n + 1")`、`removal_epoch = 16`。置き換え先は関数ではなく式なので、`almide fix` は書き換えず、ヒント「`n + 1` と書く」を出します。撤去後は `removed_spellings.rs` が同じヒントを出します。使用数 5 / 2 / 5 / 0（すべて自分自身のテストと生成された文書の行）。同じ変更で `docs/stdlib/list.md`、`since.toml`、semantics-manifest の行、検品台帳の行、IDE outline の snapshot を作り直します。
- ゲート案: なし（撤去で終わります）。
- issue: #3850（既存）

### R-list-05 `list.shuffle` を撤去し、`random.shuffle` に一本化する — ○ 採用

- 内容: `list.shuffle` は純粋な `fn` なのに、実行のたびに違う結果を返します（実測 `p4.almd`: 純粋な `fn mix(xs) = list.shuffle(xs)` がコンパイルでき、2 回の実行で `[5, 4, 8, 6, 1, 2, 3, 7]` と `[1, 5, 6, 8, 2, 4, 3, 7]`）。同じ処理の `random.shuffle` は `effect fn` で、純粋な `fn` から呼ぶと E006 で止まります（`p5.almd`）。`list.shuffle` を撤去し、`random.shuffle` を残します。
- 根拠の L0 規則: R-L0-01（同じ役割に綴り 1 つ）、R-L0-13（環境を読む関数は effect。ADR-0022）、R-L0-02（乱数の持ち分は `random`、R-L1-04）
- (a) Python は `random.shuffle`、Rust は `rand` クレートで、乱数のモジュールに置く言語が多いです。Elixir・Kotlin・Swift はコレクション側に置きますが、純粋性を型で区別しない言語です。
- (b) wasm では `wasi:random` で乱数を取ります（`random.shuffle` と同じ経路）。ADR-0027 は `list.shuffle` を「141 番目の host 依存関数」として個別に検品しています。撤去すれば、この例外が 1 つ消えます。
- (c) 純粋な関数が同じ入力で違う値を返すと、テストやキャッシュ、fusion の前提（同じ式は同じ値）が崩れます。LLM は `list.shuffle` を見つけると、effect の印のない関数の中で使ってしまいます。
- (d) 撤去。`@deprecated(since=15, use="random.shuffle")`、`removal_epoch = 16`。`almide fix` は綴りを書き換え、呼び出し元が純粋な `fn` なら E006 が出て `effect fn` にするよう案内されます（書き換えだけでは通らない場合があることを、ヒントに書きます）。使用数 2 / 2 / 9 / 0（`random.shuffle` は 12 / 0 / 7 / 1）。
- ゲート案: R-L0-G に「ADR-0027 の host 依存関数の一覧に入る関数は `effect fn` である」を足します。
- issue: #3943

### R-list-06 比較関数の `sort_with` と、キー関数の `min_by`・`max_by` を足す — ○ 採用

- 内容: 次の 3 関数を足します。
  - `list.sort_with(xs: List[A], cmp: (A, A) -> Int) -> List[A]` — `cmp(a, b)` が負なら a が先、0 なら元の順（安定）、正なら b が先。
  - `list.min_by(xs: List[A], f: (A) -> K) -> Option[A]` — キー `f(x)` が最小の要素。同じキーが複数あれば最初のもの。空なら none。`f` は要素ごとに 1 回だけ呼びます（`sort_by` と同じ）。
  - `list.max_by(xs: List[A], f: (A) -> K) -> Option[A]` — 同様。同じキーが複数あれば最初のもの。
- 根拠の L0 規則: R-L0-10（`_by` はキー関数、`_with` は比較関数）、R-L0-06（失敗しうる取り出しは無印で Option）、D4・D5
- (a) 比較関数で並べ替える関数は 8/9 が持っています（Rust `sort_by` (F)、Go `SortFunc` (F)、JS `toSorted(cmp)` (F)、Gleam `sort(by:)`、Elixir `sort/2`、OCaml `sort cmp` (F)、Kotlin `sortedWith`、Swift `sorted(by:)` (F)）。比較関数の戻り値を整数の符号にするのは Go (F)・JS・OCaml (F)・Kotlin・Python の `cmp_to_key` の 5 つで、Almide の既存の `bytes.cmp`（-1/0/1）とも同じです。キーで最小・最大を取る関数は、Rust `min_by_key`、Python `min(key=)` (F)、Elixir `min_by` (F)、Kotlin `minByOrNull` (F) の 4 つがキー関数で、Go `MinFunc` (F)・Swift `min(by:)` (F) を加えると、この役割は 6/9 です。Kotlin の `minByOrNull` (F) は「同じ最小値なら最初の要素」で、ここでの約束と同じです。
- (b) 純粋な計算です。並べ替えは既存の安定マージソートを使い、native と wasm で同じ順になります。
- (c) 降順や複合キーの並べ替えで、LLM は比較関数を書きます。いまは `sort_by` にキーの組を渡すか、符号を反転させるしかありません。`min_by` は rainbow-bridge の設計メモ（`docs/roadmap/on-hold/rainbow-bridge.md:178`）でもすでに書かれている綴りです。
- (d) 追加。使用数はいずれも 0（`min_by` は docs に 1）。
- 比較関数に渡す `compare` 関数（`int.compare`、`float.compare`、`string.compare`、`bytes.compare` など）は、R-L0-16 によりそれぞれの型のモジュールに置きます。
- ゲート案: R-L0-G の `_by`/`_with` の型検査（`_by` の最後の引数は 1 引数の関数、`_with` は 2 引数の関数）。行列テスト: `sort`・`min`・`max` × {自然順, `_by`} の 6 つのセルが存在し、`sort_with(xs, cmp) |> list.first` と `min` が自然順で一致すること。
- issue: #3924

### R-list-07 `list.unzip` を足す — ○ 採用

- 内容: `list.unzip(xs: List[(A, B)]) -> (List[A], List[B])` を足します。
- 根拠の L0 規則: L2 の完全性の規則 3（`zip` があれば逆向きも置く）、R-L0-00（綴りは `unzip` が 4/9 で最多: Rust・Gleam (F)・Elixir (F)・Kotlin。OCaml は `split` (F)）
- (a) 役割としては 5/9 です（上の 4 つと OCaml）。
- (b) 純粋な計算です。
- (c) いまは `list.fold` で 2 つのリストを組み立てるか、`list.map` を 2 回書くしかありません。`unzip` は LLM が `zip` の対として迷わず書く綴りです。
- (d) 追加。使用数 0。
- ゲート案: `zip` と `unzip` の往復（同じ長さなら元に戻る）の性質テスト。
- issue: #3925

### R-list-08 `list.sum` と `list.product` を `List[Float]` でも使えるようにする — ○ 採用

- 内容: `list.sum` と `list.product` の要素型を、`list.sort`・`list.min`・`list.max` と同じく型で振り分けます。`List[Int]` はいまと同じ（折り返し）、`List[Float]` は左から順に IEEE の足し算・掛け算をします（空なら `0.0`・`1.0`）。いまは `list.sum([1.5, 2.5])` が E005「expects List[Int] but got List[Float]」で止まります（実測 `p2.almd`）。
- 根拠の L0 規則: L2 の完全性の規則 4、R-L0-02（主語は `List` なので `list` に置く。`float.sum` は作らない）、R-L0-14 の 4（既存の呼び出しの意味は変えない）
- (a) 小数の合計は 5/9 が持っています（Rust `sum`、Python `sum` (F)、Gleam `float.sum`、Elixir `sum` (F)、Kotlin `sum`）。
- (b) 足す順番を左からと固定すれば、native と wasm で 1 ビットも違わない結果になります（ADR-0016 の決定性）。Kahan 法などの補正はしません（Python の `math.fsum` に当たるものは足しません）。
- (c) `xs |> list.sum` は LLM が要素型を見ずに書く綴りです。いまは Float のときだけ `list.fold(xs, 0.0, (a, x) => a + x)` に書き直す必要があります。
- (d) 意味変更（定義域を広げるだけで、`List[Int]` の呼び出しの結果はいまと 1 つも変わりません。名前の意味の付け替えではありません）。使用数 92 / 51 / 18 / 1（sum）、14 / 5 / 13 / 1（product）。
- ゲート案: 要素型 × {sum, product, min, max} の行列テスト（Int・Float の 8 セル）を native と wasm の両方で回し、`spec/wasm_cross/` の fixture と契約を 1 つ足します（観測できる挙動の追加なので、als に先に契約を入れます）。
- issue: #3922

### R-list-09 刻み幅つきの `list.range_step` を足す — ○ 採用

- 内容: `list.range_step(start: Int, end: Int, step: Int) -> List[Int]` を足します。`step > 0` なら `start` から `end` 未満まで、`step < 0` なら `start` から `end` より大きい間を数え下げます。`step == 0` は契約違反として止めます（R-L0-06 の 3）。半開区間は `list.range` と同じです（R-L0-12）。
- 根拠の L0 規則: R-L0-02（主語のない構築子は返す型のモジュール）、R-L0-10（`_by` はキー関数専用なので `range_by` にはしない）、R-L0-12
- (a) 刻み幅つきの範囲は 5/9 が持っています（Rust `step_by`、Python `range(a, b, s)`、Elixir `a..b//s`、Kotlin `step`、Swift `stride(from:to:by:)`）。綴りは割れているので、Almide の既存の `range` に引数の意味を添える形にします。
- (b) 純粋な計算です。
- (c) いまは `list.range(0, n) |> list.map((i) => i * 2)` や `list.filter` で書き、端の計算を間違えやすいです。LLM は Python の `range(a, b, s)` の癖で 3 引数の `list.range` を書きがちなので、E002 の「もしかして」で `list.range_step` に案内します（R-list-11）。
- (d) 追加。使用数 0。
- ゲート案: `range_step(a, b, 1) == range(a, b)` と、負の刻みの境界（`range_step(5, 0, -2) == [5, 3, 1]`）の fixture。
- issue: #3923

### R-list-10 境界の扱いをそろえる: `slice` の負の開始位置、`chunk`・`window` の `n <= 0` — ○ 採用

- 内容:
  1. `list.slice(xs, start, end)` の負の `start` を 0 に丸めます。実測（`p6.almd`）で、同じ `(-1, 2)` に対して `string.slice("abc", -1, 2)` は `"ab"` を返し、`list.slice([1, 2, 3], -1, 2)` は `[]`、`bytes.slice(..., -1, 2)` は長さ 0 です。R-L0-12（範囲外は端に丸め、負の添字は範囲外なので 0 に丸める）と文書の「clamped」に合うのは string の挙動です。list と bytes をそれに合わせ、3 つの型すべてで `slice(-1, 2)` が先頭の 2 要素を返すようにします（bytes は L2-bytes の `slice` の行、string は据え置き）。
  2. `list.chunk(xs, n)` と `list.window(xs, n)` は、`n <= 0` を契約違反として止めます。いまは `n = 0` だけが止まり、`chunk(xs, -1)` は `[[1, 2, 3]]`、`window(xs, -1)` は `[]` を返します（実測 `p6.almd`）。
- 根拠の L0 規則: R-L0-12、R-L0-06 の 3
- (a) 半開区間と丸めは Python・JS・Gleam・Elixir と同じです。大きさ 0 以下で止めるのは Rust の `chunks`・`windows`（panic）と Kotlin の `chunked`（例外）と同じです。
- (b) 両 target で同じ挙動にそろえ、wasm と native の差は生じません（いまも両方で同じく食い違っています）。
- (c) 同じ引数で型ごとに結果が違うと、string で覚えた端の挙動が list で外れます。
- (d) 意味変更（端の入力の結果だけ。文書に書いてある挙動に実装を合わせる修正です）。`chunk`・`window` の負の n は定義域を狭める変更で、dialect epoch は上げません（定義域を狭めるだけで綴りと受理は変わらないため）。影響の件数は、spec・tests の中で負の引数を渡している呼び出しを数えて PR に書きます。
- ゲート案: R-L0-12 の行列テスト（list・string・bytes × `slice`・`take`・`drop`・`take_end`・`drop_end` × 境界入力 {-1, 0, len, len+1}）に、このページの 2 つの食い違いを最初のセルとして入れます。負の個数（`take(xs, -1)` は全部、`drop(xs, -1)` は空）は list と string ですでにそろっているので、いまの答えをそのままセルに固定します（L2-string の完全性の規則 1）。
- issue: #3944

### R-list-11 E002 の「もしかして」表を、生きている関数に向ける — ○ 採用

- 内容: `crates/almide-frontend/src/stdlib.rs:156-230` の `ALIASES`（未定義の名前を呼んだときの「もしかして」表）を直します。いまの list の行には、生きている関数から離れる向きのものがあります。
  - `("list", "push", "list.concat ...")` と `("list", "append", "list.concat ...")`: `list.concat` は存在しません。`push` は生きた関数です。→ `push` の行を消し、`append` は「`xs + ys`（その場なら `list.push`）」に直します。
  - `("list", "count", "list.len")`: `list.count` は生きていて、意味が違います（述語で数える）。→ 行を消します。
  - `("list", "find_index", "list.index_of")`、`("list", "reduce", "list.fold")`、`("list", "tail", "list.drop(xs, 1)")`: どれも生きた関数です。→ 行を消します。
  - 足す行（他言語の綴り → Almide の綴り）: `sorted`/`toSorted` → `list.sort`、`sorted_by`/`sortWith` → `list.sort_with`、`distinct`/`uniq` → `list.unique`、`windows`/`windowed` → `list.window`（撤去後）、`chunks`/`chunked` → `list.chunk`、`concat`/`extend` → `xs + ys`、`get_or`/`getOrElse` → `list.get(xs, i) ?? d`（撤去後）、`min_by_key`/`minBy` → `list.min_by`、`range` の 3 引数 → `list.range_step`。string のページが同じ表に `split_whitespace` などのヒントを足すのと、同じ仕組みです。
- 根拠の L0 規則: R-L0-01（「もしかして」が 2 つめの綴りを教えてはいけない）、R-L0-14 の 3（撤去した綴りにヒントを出す）
- (a) Rust・Kotlin のコンパイラも、似た名前の候補を出します。
- (b) 検査器だけの変更です。
- (c) LLM が他言語の綴りで書いたとき、最初のエラーで正しい綴りに着けるかどうかが、修正の生存率を決めます。存在しない `list.concat` に案内すると、2 回目もエラーになります。
- (d) 診断の変更だけです。互換の問題はありません。
- ゲート案: 「`ALIASES` のキーはすべて未定義の名前で、案内先の関数はすべて生きている」をテストで検査します（新しい `tests/stdlib_alias_table_test.rs`。interface JSON と突き合わせます）。撤去した綴りは `removed_spellings.rs` に移します。
- issue: #3905

### R-list-13 範囲外の位置への書き込みを abort にする — ○ 採用

- 内容: 位置を指定して書くコピー関数は、範囲外の位置を呼び出し側の契約違反として止めます（abort）。
  - `list.set(xs, i, v)`・`list.update(xs, i, f)`・`list.remove_at(xs, i)`: いまは範囲外の `i` で何もせず、元のリストを返します（`stdlib/list.almd` の説明文「no-op if i out of range」）。`0 <= i < len` の外は abort にします。
  - `list.swap(xs, i, j)`: いまは `i` か `j` が範囲外なら何もしません。どちらかが範囲外なら abort にします。
  - `list.insert(xs, i, v)`: `0..=len` の位置を受け付けます（`i == len` は末尾に足す）。いまは負の `i` と `len` より大きい `i` も末尾に足しています。その外は abort にします。
- 根拠の L0 規則: L0 の「位置を指定した書き込み」（R-L0-12 の後ろ、R-L0-13 の前）、R-L0-06 の 3（abort は契約違反だけ）
- (a) 範囲外の書き込みを、Rust・Go・Kotlin・Swift は panic・trap、Python・OCaml は例外で止めます。黙って何もしないのは Elixir の `List.replace_at` だけです。
- (b) 両 target で同じ abort の経路（添字代入 `xs[i] = v` がすでに使っているもの）を使います。差は生じません。
- (c) いまは `list.set(xs, 5, v)` が何もせず、`xs[5] = v` が abort します。同じ書き込みで 2 つの挙動があり、何もしない書き込みは間違えても気づけません。
- (d) 意味変更（定義域を狭める変更）。正しい位置への書き込みの結果は変わらないので、dialect epoch の `breaks` には入れません（定義域を狭めても epoch にしない、という既存の扱い）。名前と型は変えないので、`@deprecated` も `almide fix` も要りません。範囲外を何もしない挙動に頼っていた呼び出しは、`if i < list.len(xs) then list.set(xs, i, v) else xs` と書きます。影響の件数は、spec・tests の中で範囲外の位置を渡している呼び出しを数えて PR に書きます。
- ゲート案: list・bytes の位置書き込み × 境界入力 {-1, 0, len-1, len, len+1} の行列テストを native と wasm で回し、abort の位置が同じであることを確かめます（bytes の `put_<型>` と同じ表）。観測できる挙動の変更なので、契約の台帳を als に先に入れます。
- issue: #3937

### R-list-14 `list.tail` を撤去し、`list.drop(xs, 1)` に一本化する — ○ 採用

- いま: `list.tail(xs: List[A]) -> List[A]` は先頭を除いたリストを返し、`list.drop(xs, 1)` と同じです（空のリストでは空を返す）。
- 根拠の L0 規則: R-L0-01（ほかの公開関数を 1 回呼ぶだけの関数も、同じ役割の 2 つ目の綴りに数える）。L3 の表では理由なしに据え置きになっていたので、規則どおりに直します。
- (a) `tail` を持つのは OCaml（`List.tl`、空で例外）、Elixir（`tl`、空で例外）、Haskell（`tail`、空で例外）です。Kotlin・Swift・Rust・Python・JS は `drop(1)`・`dropFirst()`・`xs[1:]`・`slice(1)` で書きます。Gleam は `list.rest`（Result）です。綴りも空の扱いもそろっていません。
- (c) 英語の `tail` は「末尾の要素」とも読めるので、`list.last` と取り違えやすい名前です。`drop(xs, 1)` は意味が名前から読めます。
- (d) 撤去です。R-L0-14 のとおり、epoch 15 で `@deprecated(since=15, use="list.drop(xs, 1)")`、epoch 16 で撤去します。`almide fix` が `list.tail(xs)` を `list.drop(xs, 1)` に書き換えます。E002 の表（R-list-11）に `tail` → `list.drop(xs, 1)` の行を足します。
- 完全性の規則: R-L0-01 の転送の検出（#3851 のゲート）が `list.tail` を拾い、許容リストに残しません。
- issue: #3969

### R-list-12 多数派に届かない役割は足さない — × 不採用

- 内容: 次のものは足しません。
  - 後ろから探す（`last_index_of`・`find_last`）: 4/9（Rust、JS (F)、Kotlin、Swift (F)）。`list.reverse` と `find_index` で書けます。
  - 隣どうしの条件でまとめる（`chunk_by`）: 4/9（Rust (F)、Python、Gleam (F)、Elixir (F)）。
  - 並んでいるかの判定（`is_sorted`）: 2/9（Rust (F)、Go (F)）。
  - 全部同じ値で埋める（`fill`、その場）: 3/9（Rust (F)、JS (F)、Kotlin）。`list.repeat(v, list.len(xs))` で書けます。
  - 連結の関数（`concat`・`append`）と、複数要素をその場で足す `extend`: 役割は 9/9 ですが、Almide では `+` 演算子（`xs = xs + ys`）が担います。関数を足すと、同じ処理に書き方が 2 つになります（R-L0-01）。
  - 降順の並べ替え（`sort_desc`）、`zip3`、`frequencies`: どれも 4/9 未満です。
- 根拠の L0 規則: L1 の「カテゴリを足す基準」と同じ 5/9 の線、R-L0-01
- (c) 足さないかわりに、他言語の綴りを書いたときの「もしかして」を R-list-11 で出します。
- issue: なし

---

### R-list-15 `list.binary_search` を順序のある要素型すべてで使えるようにする — ○ 採用

- いま: `fn list.binary_search(xs: List[Int], target: Int) -> Option[Int]`。`list.sort`・`min`・`max` は順序のある型すべてで動くのに、探す側だけ `Int` 専用です。
- 案: `fn list.binary_search[A](xs: List[A], target: A) -> Option[Int]`。順序は `list.sort` と同じです（Float は IEEE totalOrder）。`List[Int]` の結果は変わりません。
- 根拠の L0 規則: R-L0-16、L2 の完全性の規則 4（R-list-08 と同じく定義域を広げる変更）。
- (a) Rust `binary_search`（`T: Ord`）、Go `slices.BinarySearch`（`cmp.Ordered`）、Python `bisect`、Kotlin `binarySearch`（`Comparable`）は、どれも要素型を限りません。
- (b) 純粋な計算です。順序は `list.sort` と同じ実装を使います。
- (c) 実測: `list.binary_search(["a","b"], "b")` は E005 で、ヒントが「`int.parse(s)` で Int に変える」と出ます。意味のない直し方に連れていきます。
- (d) 意味変更（定義域を広げるだけ。既存の呼び出しは変わらないので epoch にしません）。
- 完全性の規則: list の順序を使う関数（`sort`・`min`・`max`・`binary_search`）は、受け付ける要素型が同じです。
- ゲート案: 要素型（Int・Float・String・組）× {`sort`, `min`, `max`, `binary_search`} の行列テストを両 target で回します。観測できる挙動の追加なので、als に先に契約を入れます。
- issue: #3969

### R-list-16 `list.join` を `List[Bytes]` にも使えるようにする（`bytes.join` は置かない） — ○ 採用

- いま: `fn list.join(xs: List[String], sep: String) -> String`。`List[Bytes]` を区切りつきで 1 つにする関数はありません（`bytes.join` は E002）。
- 案: `list.join` を、連結 `+` を持つ列の型（`String`・`Bytes`）で受け付けます。`list.join(parts: List[Bytes], sep: Bytes) -> Bytes`。`list.sum` を `Int` と `Float` で振り分けた R-list-08 と同じ仕組みです。
- 根拠の L0 規則: R-string-01（join は list に置く）、R-L0-01（`bytes.join` を足すと、同じ役割の置き場が型で割れる）、R-bytes-16（`Bytes` も `+` で連結する）。
- (a) 区切りつきの連結は Go `bytes.Join`、Python `bytes.join`、OCaml `Bytes.concat`、Rust `<[V]>::join`、Swift `joined(separator:)` の 5 言語以上が持ちます。
- (b) 純粋な計算です。
- (c) `list.join` を覚えれば、文字列でもバイト列でも同じ形で書けます。
- (d) 意味変更（定義域を広げるだけ）。互換の影響はありません。
- 完全性の規則: `+` で連結できる列の型（String・Bytes）はすべて `list.join` で結べます（List どうしの連結は `list.flatten`）。
- issue: #3969

### R-list-17 `binary_search_by`・`binary_search_with` を足す — ○ 採用

- いま: 並べる側は `sort`・`sort_by`・`sort_with` の 3 つがあるのに、探す側は `binary_search` だけです。`sort_by(xs, key)` で並べたリストを、同じ鍵で探す関数がありません。
- 案: `fn list.binary_search_by[A, K](xs: List[A], target: K, key: (A) -> K) -> Int?`、`fn list.binary_search_with[A](xs: List[A], target: A, cmp: (A, A) -> Int) -> Int?`。並べ方と探し方が同じ `_by`・`_with` の対になります。
- 根拠の L0 規則: R-L0-12（族は行列で揃える）。L2 の完全性の規則 4（順序を使う関数は同じ要素型）。
- (a) Rust `binary_search_by`・`binary_search_by_key`、Go `slices.BinarySearchFunc`、Python `bisect(key=)`、Kotlin `binarySearch(comparator)`・`binarySearchBy`。
- (b) 純粋な計算です。
- (c) `sort_by` で並べたら `binary_search_by` で探す、と名前の対で当たります。
- (d) 追加 2。
- 完全性の規則: `sort` の 3 つの形（無印・`_by`・`_with`）は、`binary_search` にも同じ 3 つがあります。
- issue: #3980

### R-list-18 `list.repeat` を列全体の繰り返しにする — × 不採用

- 理由: `string.repeat` と `bytes.repeat` は列全体を繰り返し、`list.repeat(v, n)` は 1 つの値を n 個並べます。揃えると、`list.repeat([1, 2], 3)` の結果が `[[1, 2], [1, 2], [1, 2]]` から `[1, 2, 1, 2, 1, 2]` に、型エラーなしで変わります（R-L0-14 の 4: 同じ名前で黙って結果を変えない）。要素を並べる形は Gleam `repeat`・Elixir `List.duplicate`・Kotlin `List(n) { v }`・Swift `Array(repeating:count:)` と同じで、多数派から外れてもいません。L3 の表の「Go slices.Repeat」は列全体の繰り返しなので、比較の欄から外しました。
- issue: なし

### R-list-19 `min_with`・`max_with` を足す — ○ 採用

- いま: 並べる・探すは無印・`_by`・`_with` の 3 つ（R-list-17）なのに、`min`・`max` は無印と `_by` だけです。
- 案: `fn list.min_with[A](xs: List[A], cmp: (A, A) -> Int) -> A?`、`fn list.max_with[A](xs: List[A], cmp: (A, A) -> Int) -> A?`。
- 根拠の L0 規則: R-L0-10（`_with` は比較関数）、R-L0-12（族は行列で揃える）。
- (a) Rust `min_by`/`max_by`（比較関数）、Kotlin `minWithOrNull`/`maxWithOrNull`、Swift `min(by:)`。
- (d) 追加 2。
- 完全性の規則（R-list-17 を広げる）: 順序を使う関数 {`sort`, `min`, `max`, `binary_search`} は、どれも無印・`_by`・`_with` の 3 つを持ちます。
- issue: #3985

## L3: 関数表

判定の凡例: 据え置き / 改名 / 意味変更 / 撤去 / 追加。他言語の列は主な綴りだけを書きます。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `len` | `list.len(xs: List[A]) -> Int` | Rust len / Gleam length / Kotlin size / Swift count | 据え置き | R-L0-11 |
| `length` | `list.length(xs: List[A]) -> Int`（非推奨） | Gleam length / JS length | 撤去（→ `list.len`） | R-list-02 |
| `get` | `list.get(xs: List[A], i: Int) -> Option[A]` | Rust get / Kotlin getOrNull / OCaml nth_opt | 据え置き | — |
| `get_or` | `list.get_or(xs: List[A], i: Int, default: A) -> A` | Kotlin getOrElse / Elixir at(xs,i,d) | 撤去（→ `list.get(xs, i) ?? d`） | R-list-01 |
| `set` | `list.set(xs: List[A], i: Int, val: A) -> List[A]` | JS with / Elixir List.replace_at | 意味変更（範囲外は abort） | R-list-00、R-list-13 |
| `swap` | `list.swap(xs: List[A], i: Int, j: Int) -> List[A]` | Rust swap（その場）/ Swift swapAt | 意味変更（範囲外は abort） | R-list-00、R-list-13 |
| `sort` | `list.sort(xs: List[A]) -> List[A]` | Kotlin sorted / Swift sorted() / Python sorted | 据え置き | R-list-00 |
| `reverse` | `list.reverse(xs: List[A]) -> List[A]` | Gleam reverse / Kotlin reversed / JS toReversed | 据え置き | R-list-00 |
| `contains` | `list.contains(xs: List[A], x: A) -> Bool` | Rust contains / JS includes / Gleam contains | 据え置き | — |
| `enumerate` | `list.enumerate(xs: List[A]) -> List[(Int, A)]` | Rust enumerate / Python enumerate / Swift enumerated | 据え置き | — |
| `zip` | `list.zip(xs: List[A], ys: List[B]) -> List[(A, B)]` | Rust zip / Gleam zip / OCaml combine | 据え置き | — |
| `flatten` | `list.flatten(xss: List[List[T]]) -> List[T]` | Rust flatten / Gleam flatten / JS flat | 据え置き | — |
| `take` | `list.take(xs: List[A], n: Int) -> List[A]` | Gleam take / Kotlin take / Swift prefix | 据え置き | R-L0-12 |
| `drop` | `list.drop(xs: List[A], n: Int) -> List[A]` | Gleam drop / Rust skip / Swift dropFirst | 据え置き | R-L0-12 |
| `tail` | `list.tail(xs: List[A]) -> List[A]` | OCaml tl / Elixir tl / Kotlin drop(1) | 撤去（→ `list.drop(xs, 1)`） | R-list-14 |
| `unique` | `list.unique(xs: List[A]) -> List[A]` | Gleam unique / Elixir uniq / Kotlin distinct | 据え置き | — |
| `index_of` | `list.index_of(xs: List[A], x: A) -> Option[Int]` | JS indexOf / Go Index / Swift firstIndex(of:) | 据え置き | — |
| `last` | `list.last(xs: List[A]) -> Option[A]` | Rust last / Gleam last / Swift last | 据え置き | — |
| `chunk` | `list.chunk(xs: List[A], n: Int) -> List[List[A]]` | Rust chunks / Kotlin chunked / Gleam sized_chunk | 意味変更（`n <= 0` は abort） | R-list-10 |
| `sum` | `list.sum(xs: List[Int]) -> Int` | Rust sum / Python sum / Elixir sum | 意味変更（`List[Float]` も受け取る） | R-list-08 |
| `product` | `list.product(xs: List[Int]) -> Int` | Rust product / Elixir product | 意味変更（`List[Float]` も受け取る） | R-list-08 |
| `first` | `list.first(xs: List[A]) -> Option[A]` | Rust first / Gleam first / Kotlin firstOrNull | 据え置き | — |
| `is_empty` | `list.is_empty(xs: List[A]) -> Bool` | Rust is_empty / Gleam is_empty / Swift isEmpty | 据え置き | — |
| `min` | `list.min(xs: List[A]) -> Option[A]` | Rust min / Go Min / Kotlin minOrNull | 据え置き | — |
| `max` | `list.max(xs: List[A]) -> Option[A]` | Rust max / Go Max / Kotlin maxOrNull | 据え置き | — |
| `join` | `list.join(xs: List[String], sep: String) -> String` | Rust join / JS join / Kotlin joinToString / Go strings.Join | 意味変更（`List[Bytes]` と `Bytes` の区切りも受け取る） | R-string-01、R-list-16 |
| `map` | `list.map(xs: List[A], f: (A) -> B) -> List[B]` | 全言語 map | 据え置き | — |
| `filter` | `list.filter(xs: List[A], f: (A) -> Bool) -> List[A]` | 全言語 filter | 据え置き | — |
| `find` | `list.find(xs: List[A], f: (A) -> Bool) -> Option[A]` | Rust find / Kotlin find / Swift first(where:) | 据え置き | — |
| `any` | `list.any(xs: List[A], f: (A) -> Bool) -> Bool` | Rust any / JS some / OCaml exists | 据え置き | — |
| `all` | `list.all(xs: List[A], f: (A) -> Bool) -> Bool` | Rust all / JS every / Swift allSatisfy | 据え置き | — |
| `count` | `list.count(xs: List[A], f: (A) -> Bool) -> Int` | Kotlin count{} / Gleam count / Elixir count/2 | 据え置き | — |
| `flat_map` | `list.flat_map(xs: List[A], f: (A) -> List[B]) -> List[B]` | Rust flat_map / JS flatMap / OCaml concat_map | 据え置き | — |
| `filter_map` | `list.filter_map(xs: List[A], f: (A) -> Option[B]) -> List[B]` | Rust filter_map / Kotlin mapNotNull / Swift compactMap | 据え置き | — |
| `fold` | `list.fold(xs: List[A], init: B, f: (B, A) -> B) -> B` | Rust fold / Gleam fold / OCaml fold_left | 据え置き | — |
| `sort_by` | `list.sort_by(xs: List[A], f: (A) -> B) -> List[A]` | Rust sort_by_key / Kotlin sortedBy / Elixir sort_by | 据え置き | R-list-06 |
| `take_while` | `list.take_while(xs: List[A], f: (A) -> Bool) -> List[A]` | Rust take_while / Kotlin takeWhile / Swift prefix(while:) | 据え置き | — |
| `drop_while` | `list.drop_while(xs: List[A], f: (A) -> Bool) -> List[A]` | Rust skip_while / Kotlin dropWhile / Swift drop(while:) | 据え置き | — |
| `partition` | `list.partition(xs: List[A], f: (A) -> Bool) -> (List[A], List[A])` | Rust partition / Gleam partition / Elixir split_with | 据え置き | — |
| `reduce` | `list.reduce(xs: List[A], f: (A, A) -> A) -> Option[A]` | Rust reduce / Gleam reduce / Kotlin reduceOrNull | 据え置き | — |
| `group_by` | `list.group_by(xs: List[A], f: (A) -> B) -> Map[B, List[A]]` | Kotlin groupBy / Elixir group_by / JS Object.groupBy | 据え置き | — |
| `find_index` | `list.find_index(xs: List[A], f: (A) -> Bool) -> Option[Int]` | JS findIndex / Go IndexFunc / OCaml find_index | 据え置き | — |
| `update` | `list.update(xs: List[A], i: Int, f: (A) -> A) -> List[A]` | Elixir List.update_at | 意味変更（範囲外は abort） | R-list-00、R-list-13 |
| `scan` | `list.scan(xs: List[A], init: B, f: (B, A) -> B) -> List[B]` | Rust scan / Kotlin runningFold / Elixir scan | 据え置き | — |
| `zip_with` | `list.zip_with(xs: List[A], ys: List[B], f: (A, B) -> C) -> List[C]` | Elixir zip_with / Gleam map2 / Kotlin zip(other){} | 据え置き | — |
| `unique_by` | `list.unique_by(xs: List[A], f: (A) -> K) -> List[A]` | Elixir uniq_by / Kotlin distinctBy | 据え置き | — |
| `range` | `list.range(start: Int, end: Int) -> List[Int]` | Python range / Gleam range / Rust a..b | 据え置き | — |
| `slice` | `list.slice(xs: List[A], start: Int, end: Int) -> List[A]` | JS slice / Python xs[a:b] / Elixir slice | 意味変更（負の開始位置を 0 に丸める） | R-list-10 |
| `insert` | `list.insert(xs: List[A], i: Int, val: A) -> List[A]` | Go slices.Insert / Elixir List.insert_at / JS toSpliced | 意味変更（`0..=len` の外は abort） | R-list-00、R-list-13 |
| `remove_at` | `list.remove_at(xs: List[A], i: Int) -> List[A]` | Elixir List.delete_at / Kotlin removeAt / Swift remove(at:) | 意味変更（範囲外は abort） | R-list-00、R-list-13 |
| `repeat` | `list.repeat(val: A, n: Int) -> List[A]` | Gleam repeat / Elixir List.duplicate / Kotlin `List(n) { v }` | 据え置き | — |
| `intersperse` | `list.intersperse(xs: List[A], sep: A) -> List[A]` | Gleam intersperse / Elixir intersperse | 据え置き | — |
| `windows` | `list.windows(xs: List[A], n: Int) -> List[List[A]]` | Rust windows | 撤去（→ `list.window`） | R-list-03 |
| `dedup` | `list.dedup(xs: List[A]) -> List[A]` | Rust dedup / Elixir dedup / Go Compact | 据え置き | — |
| `take_end` | `list.take_end(xs: List[A], n: Int) -> List[A]` | Kotlin takeLast / Swift suffix | 据え置き | R-L0-12 |
| `drop_end` | `list.drop_end(xs: List[A], n: Int) -> List[A]` | Kotlin dropLast / Swift dropLast | 据え置き | R-L0-12 |
| `shuffle` | `list.shuffle(xs: List[A]) -> List[A]` | Python random.shuffle / Kotlin shuffled / Elixir shuffle | 撤去（→ `random.shuffle`） | R-list-05 |
| `window` | `list.window(xs: List[A], n: Int) -> List[List[A]]` | Gleam window / Rust windows / Kotlin windowed | 意味変更（`n <= 0` は abort。`windows` の残る側） | R-list-03、R-list-10 |
| `binary_search` | `list.binary_search(xs: List[A], target: A) -> Option[Int]` | Rust binary_search / Go BinarySearch / Kotlin binarySearch | 意味変更（要素型を順序のある型すべてに広げる） | R-list-15 |
| `push` | `list.push(mut xs: List[A], x: A) -> Unit` | Rust push / JS push / Python append | 据え置き | R-list-00 |
| `with_capacity` | `list.with_capacity(cap: Int) -> List[A]` | Rust Vec::with_capacity / Go make(.., 0, cap) | 据え置き | R-list-00 |
| `pop` | `list.pop(mut xs: List[A]) -> Option[A]` | Rust pop / Swift popLast / Kotlin removeLastOrNull | 据え置き | R-list-00 |
| `clear` | `list.clear(mut xs: List[A]) -> Unit` | Rust clear / Python clear / Kotlin clear | 据え置き | R-list-00 |
| `bundled_probe` | `list.bundled_probe(n: Int) -> Int` | —（どの言語にもない） | 撤去（→ `n + 1`） | R-list-04 |
| `split_at` | `list.split_at(xs: List[T], n: Int) -> (List[T], List[T])` | Rust split_at / Gleam split / Elixir split | 据え置き | — |
| `iterate` | `list.iterate(seed: T, f: (T) -> T, n: Int) -> List[T]` | Kotlin generateSequence / Elixir Stream.iterate / Rust successors | 据え置き | — |
| `find_map` | `list.find_map(xs: List[A], f: (A) -> Option[B]) -> Option[B]` | Rust find_map / Gleam find_map / Elixir find_value | 据え置き | — |
| `sort_with` | `list.sort_with(xs: List[A], cmp: (A, A) -> Int) -> List[A]`（案） | Kotlin sortedWith / Go SortFunc / Swift sorted(by:) | 追加 | R-list-06 |
| `min_by` | `list.min_by(xs: List[A], f: (A) -> K) -> Option[A]`（案） | Rust min_by_key / Kotlin minByOrNull / Elixir min_by | 追加 | R-list-06 |
| `max_by` | `list.max_by(xs: List[A], f: (A) -> K) -> Option[A]`（案） | Rust max_by_key / Kotlin maxByOrNull / Elixir max_by | 追加 | R-list-06 |
| `unzip` | `list.unzip(xs: List[(A, B)]) -> (List[A], List[B])`（案） | Rust unzip / Gleam unzip / OCaml split | 追加 | R-list-07 |
| `range_step` | `list.range_step(start: Int, end: Int, step: Int) -> List[Int]`（案） | Python range(a,b,s) / Rust step_by / Kotlin step | 追加 | R-list-09 |
| `binary_search_by` | （追加）`list.binary_search_by(xs: List[A], target: K, key: (A) -> K) -> Option[Int]` | Rust binary_search_by_key / Kotlin binarySearchBy | 追加 | R-list-17 |
| `binary_search_with` | （追加）`list.binary_search_with(xs: List[A], target: A, cmp: (A, A) -> Int) -> Option[Int]` | Rust binary_search_by / Go BinarySearchFunc | 追加 | R-list-17 |
| `min_with` | （追加）`list.min_with(xs: List[A], cmp: (A, A) -> Int) -> Option[A]` | Kotlin minWithOrNull / Swift min(by:) | 追加 | R-list-19 |
| `max_with` | （追加）`list.max_with(xs: List[A], cmp: (A, A) -> Int) -> Option[A]` | Kotlin maxWithOrNull / Swift max(by:) | 追加 | R-list-19 |

行数: 現行 67 関数 + 追加 5 関数 = 72 行。

---

## L0 への差し戻し（処理済み）

1. **比較関数の綴りと型。** L0 に R-L0-16（順序のある型には `compare(a, b) -> Int` を置く）が入りました。`list.sort_with` は R-L0-16 の形の関数を受け取ります（R-list-06）。`bytes.cmp` は `bytes.compare` に改名します（L2-bytes R-bytes-08）。`int.compare`・`float.compare`・`string.compare` は各ページで足します。
2. **「大きさ n で分ける」族の綴り。** R-L0-01 に「n 個ずつに分ける関数は単数形の動詞 `chunk` にそろえる」が入りました。`list.window` を残し（R-list-03）、`bytes.chunks` は `bytes.chunk` に改名します（L2-bytes R-bytes-08）。
3. **`string.join` と `list.join` の重複。** R-L0-02（主語の型のモジュールに置く）により `list.join` を残し、`string.join` を撤去します（L2-string R-string-01）。
