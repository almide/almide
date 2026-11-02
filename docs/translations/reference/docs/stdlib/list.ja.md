# list

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](list.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](list.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](list.zh-CN.md)



> 原文：[list.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/list.md)。読者向けの非規範的なソース固定スナップショットです。英語原文を正とします。API 索引は原文固定時点の内容で、この訳は自動生成・自動同期の対象ではありません。署名・型・バージョン注記とコード例（コメントを含む）は原文のまま保持し、全 74 件の API 説明の訳を末尾に併記しています。コード例・動作・主張はこの翻訳作業では独立に実行・再検証していません。
>
> 原文上の注意: 本文の検査スクリプトや機械検証の記述は、固定された英語原文に関する説明です。この翻訳ファイルが同じ検査対象に自動追加されたという意味ではありません。

リスト操作。自動インポートされます。

### `list.len(xs: List[A]) -> Int`

リストの要素数を返します。

```almd run
fn main() -> Unit = {
  println("${list.len([1, 2, 3])}")
}
```
```output
3
```

### `list.get(xs: List[A], i: Int) -> Option[A]`

インデックス i の要素を取得し、範囲外なら none を返します。

```almd run
fn main() -> Unit = {
  println("${list.get([10, 20, 30], 1)}")
}
```
```output
some(20)
```

### `list.set(xs: List[A], i: Int, x: A) -> List[A]`

インデックス i の要素を置き換えた新しいリストを返します。

```almd run
fn main() -> Unit = {
  println("${list.set([1, 2, 3], 1, 99)}")
}
```
```output
[1, 99, 3]
```

`list.set` はリスト全体をコピーします。呼び出しごとに O(n) です。`var` に対して `xs = list.set(xs, i, v)`（または `xs = xs.set(i, v)`）のように結果を元のリストに代入し直すと、その 1 スロットへのインプレース書き込みとしてコンパイルされます（#2244）。値の意味論は同じで、先に取得したエイリアスは古いリストを保持し、範囲外のインデックスは何も変更しません。共有されていないバッファはコピーなしで更新できます。copy-on-write は依然として生存するエイリアスを保護するため、エイリアスの作成を繰り返すとコピーのコストが加わることがあります。文形式の `xs[i] = v` は同じ書き込みを直接表記したものですが、範囲外のインデックスでは、何もしない `list.set` とは異なり、実行を中断します。

### `list.swap(xs: List[A], i: Int, j: Int) -> List[A]`

インデックス i と j の要素を入れ替えた新しいリストを返します。

```almd run
fn main() -> Unit = {
  println("${list.swap([1, 2, 3], 0, 2)}")
}
```
```output
[3, 2, 1]
```

### `list.sort(xs: List[A]) -> List[A]`

リストを昇順にソートします。

```almd run
fn main() -> Unit = {
  println("${list.sort([3, 1, 2])}")
}
```
```output
[1, 2, 3]
```

### `list.reverse(xs: List[A]) -> List[A]`

要素の順序を反転します。

```almd run
fn main() -> Unit = {
  println("${list.reverse([1, 2, 3])}")
}
```
```output
[3, 2, 1]
```

### `list.contains(xs: List[A], x: A) -> Bool`

リストに要素が含まれるかを調べます。

```almd run
fn main() -> Unit = {
  println("${list.contains([1, 2, 3], 2)}")
}
```
```output
true
```

### `list.enumerate(xs: List[A]) -> List[(Int, A)]`

各要素をそのインデックスと組にします。

```almd run
fn main() -> Unit = {
  println("${list.enumerate(["a", "b"])}")
}
```
```output
[(0, "a"), (1, "b")]
```

### `list.zip(xs: List[A], ys: List[B]) -> List[(A, B)]`

2 つのリストを組のリストにまとめます。

```almd run
fn main() -> Unit = {
  println("${list.zip([1, 2], ["a", "b"])}")
}
```
```output
[(1, "a"), (2, "b")]
```

### `list.flatten(xs: List[List[A]]) -> List[A]`

リストのリストを平坦化して 1 つのリストにします。

```almd run
fn main() -> Unit = {
  println("${list.flatten([[1, 2], [3]])}")
}
```
```output
[1, 2, 3]
```

### `list.take(xs: List[A], n: Int) -> List[A]`

先頭の n 要素を取ります。

```almd run
fn main() -> Unit = {
  println("${list.take([1, 2, 3, 4], 2)}")
}
```
```output
[1, 2]
```

### `list.drop(xs: List[A], n: Int) -> List[A]`

先頭の n 要素を取り除きます。

```almd run
fn main() -> Unit = {
  println("${list.drop([1, 2, 3, 4], 2)}")
}
```
```output
[3, 4]
```

### `list.unique(xs: List[A]) -> List[A]`

最初の出現を残して重複要素を除去します。

```almd run
fn main() -> Unit = {
  println("${list.unique([1, 2, 1, 3])}")
}
```
```output
[1, 2, 3]
```

### `list.index_of(xs: List[A], x: A) -> Option[Int]`

要素が最初に現れるインデックスを探し、なければ none を返します。

```almd run
fn main() -> Unit = {
  println("${list.index_of([10, 20, 30], 20)}")
}
```
```output
some(1)
```

### `list.last(xs: List[A]) -> Option[A]`

最後の要素を取得し、空なら none を返します。

```almd run
fn main() -> Unit = {
  println("${list.last([1, 2, 3])}")
}
```
```output
some(3)
```

### `list.chunk(xs: List[A], n: Int) -> List[List[A]]`

リストをサイズ n のチャンクに分割します。

```almd run
fn main() -> Unit = {
  println("${list.chunk([1, 2, 3, 4, 5], 2)}")
}
```
```output
[[1, 2], [3, 4], [5]]
```

### `list.sum(xs: List[A]) -> A`

リストの数値要素を合計します。空のリストではゼロです。整数オーバーフローはラップします。

```almd run
fn main() -> Unit = {
  println("${list.sum([1, 2, 3])}")
}
```
```output
6
```

### `list.product(xs: List[A]) -> A`

リストの数値要素を掛け合わせます。空のリストでは 1 です。整数オーバーフローはラップします。

```almd run
fn main() -> Unit = {
  println("${list.product([2, 3, 4])}")
}
```
```output
24
```

### `list.first(xs: List[A]) -> Option[A]`

最初の要素を取得し、空なら none を返します。

```almd run
fn main() -> Unit = {
  println("${list.first([1, 2, 3])}")
}
```
```output
some(1)
```

### `list.is_empty(xs: List[A]) -> Bool`

リストが空かを調べます。

```almd run
fn main() -> Unit = {
  println("${list.is_empty([]: List[Int])}")
}
```
```output
true
```

### `list.min(xs: List[A]) -> Option[A]`

最小の要素を探し、空なら none を返します。

```almd run
fn main() -> Unit = {
  println("${list.min([3, 1, 2])}")
}
```
```output
some(1)
```

### `list.max(xs: List[A]) -> Option[A]`

最大の要素を探し、空なら none を返します。

```almd run
fn main() -> Unit = {
  println("${list.max([3, 1, 2])}")
}
```
```output
some(3)
```

### `list.join(xs: List[T], sep: T) -> T`

String または Bytes のリストを、同じ型の区切りで結合します。

```almd run
fn main() -> Unit = {
  println("${list.join(["a", "b", "c"], "-")}")
}
```
```output
a-b-c
```

### `list.map(xs: List[A], f: (A) -> B) -> List[B]`

各要素に関数を適用し、新しいリストを返します。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3].map((x) => x * 2)}")
}
```
```output
[2, 4, 6]
```

### `list.filter(xs: List[A], f: (A) -> Bool) -> List[A]`

述語を満たす要素を残します。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3, 4].filter((x) => x > 2)}")
}
```
```output
[3, 4]
```

### `list.find(xs: List[A], f: (A) -> Bool) -> Option[A]`

述語に一致する最初の要素を探します。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3].find((x) => x > 1)}")
}
```
```output
some(2)
```

### `list.any(xs: List[A], f: (A) -> Bool) -> Bool`

述語を満たす要素があるかを調べます。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3].any((x) => x > 2)}")
}
```
```output
true
```

### `list.all(xs: List[A], f: (A) -> Bool) -> Bool`

すべての要素が述語を満たすかを調べます。

```almd run
fn main() -> Unit = {
  println("${[2, 4, 6].all((x) => x % 2 == 0)}")
}
```
```output
true
```

### `list.sort_by(xs: List[A], f: (A) -> B) -> List[A]`

比較関数ではなくキー抽出関数でソートします。f は各要素からソートキーを取り出します。

```almd run
fn main() -> Unit = {
  println("${["bb", "a", "ccc"].sort_by((s) => string.len(s))}")
}
```
```output
["a", "bb", "ccc"]
```

### `list.flat_map(xs: List[A], f: (A) -> List[B]) -> List[B]`

各要素をリストに写像し、結果を平坦化します。

```almd run
fn main() -> Unit = {
  println("${[1, 2].flat_map((x) => [x, x * 10])}")
}
```
```output
[1, 10, 2, 20]
```

### `list.filter_map(xs: List[A], f: (A) -> Option[B]) -> List[B]`

1 回の走査で写像とフィルタリングを行い、some の値だけを残します。

```almd run
fn main() -> Unit = {
  println("${["1", "x", "3"].filter_map((s) => int.parse(s)?)}")
}
```
```output
[1, 3]
```

### `list.find_map(xs: List[A], f: (A) -> Option[B]) -> Option[B]`

f が返す最初の some 値を返し、なければ none を返します。見つかった後は f を呼びません。つまり、短絡する `filter_map(xs, f) |> list.first` です。他のリスト高階関数と同様に、`!` で伝播するコールバックを渡すと呼び出し全体が失敗可能になります。

```almd run
fn main() -> Unit = {
  println("${["x", "2", "3"].find_map((s) => int.parse(s)?)}")
}
```
```output
some(2)
```

### `list.take_while(xs: List[A], f: (A) -> Bool) -> List[A]`

述語が成り立つ間、先頭から要素を取ります。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3, 1].take_while((x) => x < 3)}")
}
```
```output
[1, 2]
```

### `list.drop_while(xs: List[A], f: (A) -> Bool) -> List[A]`

述語が成り立つ間、先頭から要素を取り除きます。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3, 1].drop_while((x) => x < 3)}")
}
```
```output
[3, 1]
```

### `list.count(xs: List[A], f: (A) -> Bool) -> Int`

述語を満たす要素を数えます。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3, 4].count((x) => x > 2)}")
}
```
```output
2
```

### `list.partition(xs: List[A], f: (A) -> Bool) -> (List[A], List[A])`

リストを、述語に一致する要素と一致しない要素の 2 つに分けます。

```almd check
fn main() -> Unit = {
  let (evens, odds) = [1, 2, 3, 4].partition((x) => x % 2 == 0)
  println("${evens}")
  println("${odds}")
}
```

### `list.reduce(xs: List[A], f: (A, A) -> A) -> Option[A]`

要素を 2 つずつ結合してリストを集約します。空なら none を返します。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3].reduce((a, b) => a + b)}")
}
```
```output
some(6)
```

### `list.group_by(xs: List[A], f: (A) -> B) -> Map[B, List[A]]`

キー関数で要素をグループ化してマップにします。

```almd run
fn main() -> Unit = {
  let groups = ["hi", "hey", "bye"].group_by((s) => string.take(s, 1))
  println("${map.get(groups, "h")}")
  println("${map.get(groups, "b")}")
}
```
```output
some(["hi", "hey"])
some(["bye"])
```

### `list.range(start: Int, end: Int) -> List[Int]`

start（含む）から end（含まない）までの整数リストを作ります。

```almd run
fn main() -> Unit = {
  println("${list.range(1, 5)}")
}
```
```output
[1, 2, 3, 4]
```

### `list.slice(xs: List[A], start: Int, end: Int) -> List[A]`

start から end インデックスまでの部分リストを取り出します。

```almd run
fn main() -> Unit = {
  println("${list.slice([1, 2, 3, 4, 5], 1, 4)}")
}
```
```output
[2, 3, 4]
```

### `list.insert(xs: List[A], i: Int, x: A) -> List[A]`

インデックス i に要素を挿入し、要素を右にずらします。

```almd run
fn main() -> Unit = {
  println("${list.insert([1, 3], 1, 2)}")
}
```
```output
[1, 2, 3]
```

### `list.remove_at(xs: List[A], i: Int) -> List[A]`

インデックス i の要素を削除します。

```almd run
fn main() -> Unit = {
  println("${list.remove_at([1, 2, 3], 1)}")
}
```
```output
[1, 3]
```

### `list.find_index(xs: List[A], f: (A) -> Bool) -> Option[Int]`

述語が成り立つ最初のインデックスを探します。

```almd run
fn main() -> Unit = {
  println("${[10, 20, 30].find_index((x) => x > 15)}")
}
```
```output
some(1)
```

### `list.update(xs: List[A], i: Int, f: (A) -> A) -> List[A]`

インデックス i の要素を f で変換した新しいリストを返します。

```almd run
fn main() -> Unit = {
  println("${list.update([1, 2, 3], 1, (x) => x * 10)}")
}
```
```output
[1, 20, 3]
```

### `list.repeat(x: A, n: Int) -> List[A]`

値を n 回繰り返したリストを作ります。

```almd run
fn main() -> Unit = {
  println("${list.repeat(0, 3)}")
}
```
```output
[0, 0, 0]
```

### `list.scan(xs: List[A], init: B, f: (B, A) -> B) -> List[B]`

fold と似ていますが、アキュムレータのすべての途中の値を返します。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3].scan(0, (acc, x) => acc + x)}")
}
```
```output
[1, 3, 6]
```

### `list.intersperse(xs: List[A], sep: A) -> List[A]`

各要素の間に区切りを挿入します。

```almd run
fn main() -> Unit = {
  println("${list.intersperse([1, 2, 3], 0)}")
}
```
```output
[1, 0, 2, 0, 3]
```

### `list.dedup(xs: List[A]) -> List[A]`

連続する重複を除去します。

```almd
list.dedup([1, 1, 2, 2, 1]) // => [1, 2, 1]
```

隣接重複の規則を固定する実行可能な例です（dedup は distinct ではなく、末尾の `1` は残ります）。`scripts/check-doc-fences.sh` がゲートとして検査します。

```almd run
fn main() -> Unit = {
  let xs = list.dedup([1, 1, 2, 2, 1])
  println(xs |> list.map((x) => int.to_string(x)) |> list.join(","))
  println(int.to_string(list.len(xs)))
}
```
```output
1,2,1
3
```

### `list.zip_with(xs: List[A], ys: List[B], f: (A, B) -> C) -> List[C]`

関数を使い、2 つのリストを要素ごとに組み合わせます。

```almd run
fn main() -> Unit = {
  println("${list.zip_with([1, 2], [10, 20], (a, b) => a + b)}")
}
```
```output
[11, 22]
```

### `list.fold(xs: List[A], init: B, f: (B, A) -> B) -> B`

初期アキュムレータを用いて、リストを左から畳み込みます。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3].fold(0, (acc, x) => acc + x)}")
}
```
```output
6
```

### `list.take_end(xs: List[A], n: Int) -> List[A]`

末尾の N 要素を取ります。

```almd run
fn main() -> Unit = {
  println("${list.take_end([1, 2, 3, 4], 2)}")
}
```
```output
[3, 4]
```

### `list.drop_end(xs: List[A], n: Int) -> List[A]`

末尾の N 要素を取り除きます。

```almd run
fn main() -> Unit = {
  println("${list.drop_end([1, 2, 3, 4], 2)}")
}
```
```output
[1, 2]
```

### `list.unique_by(xs: List[A], f: (A) -> K) -> List[A]`

キー関数に基づいて重複を除去し、最初の出現を残します。

```almd check
fn main() -> Unit = {
  let firsts = list.unique_by(["aa", "ab", "ba"], (s) => string.get(s, 0))
  println("${firsts}")
}
```

### `list.window(xs: List[A], n: Int) -> List[List[A]]`

リスト上をサイズ N のウィンドウで走査します。

```almd run
fn main() -> Unit = {
  println("${list.window([1, 2, 3, 4], 2)}")
}
```
```output
[[1, 2], [2, 3], [3, 4]]
```

### `list.push(mut xs: List[A], x: A) -> Unit`

要素をインプレースで末尾に追加します。var 束縛が必要です。

```almd run
fn main() -> Unit = {
  var xs = [1, 2, 3]
  list.push(xs, 42)
  println("${xs}")
}
```
```output
[1, 2, 3, 42]
```

### `list.pop(mut xs: List[A]) -> Option[A]`

最後の要素をインプレースで削除して返します（`some(x)`、リストが空なら `none`）。var 束縛が必要です。

```almd run
fn main() -> Unit = {
  var xs = [1, 2, 3]
  let popped = list.pop(xs)
  println("${popped}")
  println("${xs}")
}
```
```output
some(3)
[1, 2]
```

### `list.clear(mut xs: List[A]) -> Unit`

すべての要素をインプレースで削除します。var 束縛が必要です。

```almd run
fn main() -> Unit = {
  var xs = [1, 2, 3]
  list.clear(xs)
  println("${xs}")
  println("${list.is_empty(xs)}")
}
```
```output
[]
true
```


<a id="fallible-pipelines--the-polymorphic-core-adr-0006"></a>

## 失敗可能なパイプライン — 多相的な中核（ADR-0006）

`!` で伝播するコールバック（または名前付きの `-> T!E` 関数）は、中核コンビネータの失敗可能な形をインスタンス化します。呼び出しはコールバックのエラー型（単なる `T!` では `String`）を持つ `Result[_, E]` を返し、**最初の err で短絡します**（それ以降の要素は評価されません）。コンビネータごとに名前は 1 つで、マーカーが戦略を指定します。

```almd
effect fn read_all(files: List[String]) -> List[Entry] =
  files |> list.map((f) => read_meta(f)!)!
```

`map` / `filter` / `flat_map` / `filter_map` / `fold` / `find` / `each` を対象とします。削除済みの対応する `list.try_*`（v0.55.0 で 1 回の非推奨期間を経て、v0.56.0 以降は削除）は機械的に書き換えられます。E043 に正確な形が示されます。

```almd
list.try_map(xs, f)      →  list.map(xs, (x) => f(x)!)!
list.try_fold(xs, z, f)  →  list.fold(xs, z, (a, x) => f(a, x)!)!
```

コールバックを取るリスト関数はすべて、失敗可能なコールバックを受け取れます（#3163）。`map` だけでなく、述語クエリ（`any` / `all` / `count` / `find_index` / `partition` / `take_while` / `drop_while`）、アキュムレータ（`reduce` / `scan` / `zip_with` / `update` / `iterate`）、キー抽出（`sort_by` / `group_by` / `unique_by`。並べ替えの前にリスト順で各キーを 1 回ずつ計算するため、最初の err が定まる）も同様です。この規則は map、set、option の高階関数にも適用されます。対応表は `almide_types::fallible_hofs` にあり、最終状態（各セルがソースからは名前で呼べない `__fallible_*` の担い手を持ち、公開 try_ API が空であること）は `tests/list_fallible_family_gate_test.rs` により機械的に検査されます。

エラーを返さないコールバックによって `E` に制約が付かない場合は、結果に型注釈を付けてください：`let evens: Result[List[Int], String] = list.filter(xs, (n) => ok(n % 2 == 0)!)`。


<a id="signature-index-74-functions"></a>

## 固定スナップショットの API 索引（74 関数）

```
// Element count; 0 for an empty list.
// @since 0.5.0 or earlier
list.len(xs: List[A]) -> Int

// Alias of list.len.
// @since 0.19.2 or earlier
list.length(xs: List[A]) -> Int   (deprecated — use list.len)

// Element at index i, or none when out of range.
// @since 0.5.0 or earlier
list.get(xs: List[A], i: Int) -> Option[A]

// Copy with slot i = x; no-op if i out of range.
// @since 0.5.0 or earlier
list.set(xs: List[A], i: Int, x: A) -> List[A]

// Copy with i and j swapped; no-op if either OOB.
// @since 0.5.0 or earlier
list.swap(xs: List[A], i: Int, j: Int) -> List[A]

// Ascending copy; Floats in IEEE totalOrder.
// @since 0.5.0 or earlier
list.sort(xs: List[A]) -> List[A]

// Elements in reverse order.
// @since 0.5.0 or earlier
list.reverse(xs: List[A]) -> List[A]

// true if some element == x; NaN never matches.
// @since 0.5.0 or earlier
list.contains(xs: List[A], x: A) -> Bool

// (index, element) pairs, index from 0.
// @since 0.5.0 or earlier
list.enumerate(xs: List[A]) -> List[(Int, A)]

// Pairs by position; stops at the shorter list.
// @since 0.5.0 or earlier
list.zip(xs: List[A], ys: List[B]) -> List[(A, B)]

// Inner lists concatenated in order.
// @since 0.5.0 or earlier
list.flatten(xs: List[List[A]]) -> List[A]

// First n elements; all if n >= len, [] if n < 0.
// @since 0.5.0 or earlier
list.take(xs: List[A], n: Int) -> List[A]

// All but the first n; [] if n >= len, all if n < 0.
// @since 0.5.0 or earlier
list.drop(xs: List[A], n: Int) -> List[A]

// Duplicates removed; first occurrences kept.
// @since 0.5.0 or earlier
list.unique(xs: List[A]) -> List[A]

// Index of the first x, or none.
// @since 0.5.0 or earlier
list.index_of(xs: List[A], x: A) -> Option[Int]

// Final element, or none when empty.
// @since 0.5.0 or earlier
list.last(xs: List[A]) -> Option[A]

// Runs of n, last may be short; n <= 0 aborts.
// @since 0.5.0 or earlier
list.chunk(xs: List[A], n: Int) -> List[List[A]]

// Sum of any numbers (R-L0-42); 0 when empty; integers wrap on overflow.
// @since 0.5.0 or earlier
list.sum(xs: List[A]) -> A

// Product of any numbers (R-L0-42); 1 when empty; integers wrap on overflow.
// @since 0.5.0 or earlier
list.product(xs: List[A]) -> A

// Head element, or none when empty.
// @since 0.5.0 or earlier
list.first(xs: List[A]) -> Option[A]

// true iff xs has no elements.
// @since 0.6.0 or earlier
list.is_empty(xs: List[A]) -> Bool

// Least element, or none when empty.
// @since 0.5.0 or earlier
list.min(xs: List[A]) -> Option[A]

// Greatest element, or none when empty.
// @since 0.5.0 or earlier
list.max(xs: List[A]) -> Option[A]

// The parts joined by sep (R-L0-42): Strings or Bytes; empty for [].
// @since 0.5.0 or earlier
list.join(xs: List[T], sep: T) -> T

// f applied to each element, in order.
// @since 0.5.0 or earlier
list.map(xs: List[A], f: (A) -> B) -> List[B]

// Elements where f is true, order kept.
// @since 0.5.0 or earlier
list.filter(xs: List[A], f: (A) -> Bool) -> List[A]

// First element where f holds, or none.
// @since 0.5.0 or earlier
list.find(xs: List[A], f: (A) -> Bool) -> Option[A]

// true if f holds for some x; false when empty.
// @since 0.5.0 or earlier
list.any(xs: List[A], f: (A) -> Bool) -> Bool

// true if f holds for every x; true when empty.
// @since 0.5.0 or earlier
list.all(xs: List[A], f: (A) -> Bool) -> Bool

// Number of elements where f holds.
// @since 0.5.0 or earlier
list.count(xs: List[A], f: (A) -> Bool) -> Int

// f(x) lists concatenated in order.
// @since 0.5.0 or earlier
list.flat_map(xs: List[A], f: (A) -> List[B]) -> List[B]

// The some values of f(x), in order.
// @since 0.5.0 or earlier
list.filter_map(xs: List[A], f: (A) -> Option[B]) -> List[B]

// Left fold from init; init when empty.
// @since 0.5.0 or earlier
list.fold(xs: List[A], init: B, f: (B, A) -> B) -> B

// Stable sort by key f(x); f runs once per x.
// @since 0.5.0 or earlier
list.sort_by(xs: List[A], f: (A) -> B) -> List[A]

// Longest prefix where f holds.
// @since 0.5.0 or earlier
list.take_while(xs: List[A], f: (A) -> Bool) -> List[A]

// Rest after the longest prefix where f holds.
// @since 0.5.0 or earlier
list.drop_while(xs: List[A], f: (A) -> Bool) -> List[A]

// (where f holds, the rest), order kept.
// @since 0.5.0 or earlier
list.partition(xs: List[A], f: (A) -> Bool) -> (List[A], List[A])

// Left fold seeded by xs[0]; none when empty.
// @since 0.5.0 or earlier
list.reduce(xs: List[A], f: (A, A) -> A) -> Option[A]

// Key f(x) -> elements; keys in first-seen order.
// @since 0.5.0 or earlier
list.group_by(xs: List[A], f: (A) -> B) -> Map[B, List[A]]

// Index of the first x where f holds, or none.
// @since 0.5.0 or earlier
list.find_index(xs: List[A], f: (A) -> Bool) -> Option[Int]

// Copy with slot i = f(old); no-op if i out of range.
// @since 0.5.0 or earlier
list.update(xs: List[A], i: Int, f: (A) -> A) -> List[A]

// Running fold values, init excluded.
// @since 0.5.0 or earlier
list.scan(xs: List[A], init: B, f: (B, A) -> B) -> List[B]

// f over pairs; stops at the shorter list.
// @since 0.5.0 or earlier
list.zip_with(xs: List[A], ys: List[B], f: (A, B) -> C) -> List[C]

// First element per distinct key f(x).
// @since 0.7.0 or earlier
list.unique_by(xs: List[A], f: (A) -> K) -> List[A]

// start up to end, exclusive; [] if end <= start.
// @since 0.5.0 or earlier
list.range(start: Int, end: Int) -> List[Int]

// Indices start..<end, each clamped to 0..len; [] if start >= end.
// @since 0.5.0 or earlier
list.slice(xs: List[A], start: Int, end: Int) -> List[A]

// Copy with x at i; i >= len or i < 0 appends.
// @since 0.5.0 or earlier
list.insert(xs: List[A], i: Int, x: A) -> List[A]

// Copy without slot i; no-op if i out of range.
// @since 0.5.0 or earlier
list.remove_at(xs: List[A], i: Int) -> List[A]

// n copies of x; [] for n <= 0.
// @since 0.5.0 or earlier
list.repeat(x: A, n: Int) -> List[A]

// sep between adjacent elements.
// @since 0.5.0 or earlier
list.intersperse(xs: List[A], sep: A) -> List[A]

// Adjacent equal runs collapsed; not global.
// @since 0.5.0 or earlier
list.dedup(xs: List[A]) -> List[A]

// Last n elements; all if n >= len, [] if n < 0.
// @since 0.7.0 or earlier
list.take_end(xs: List[A], n: Int) -> List[A]

// All but the last n; [] if n >= len, all if n < 0.
// @since 0.7.0 or earlier
list.drop_end(xs: List[A], n: Int) -> List[A]

// All n-long runs; [] if n > len; n <= 0 aborts.
// @since 0.7.0 or earlier
list.window(xs: List[A], n: Int) -> List[List[A]]

// Index of target in xs sorted ascending (list.sort's order), or none.
// @since 0.14.6 or earlier
list.binary_search(xs: List[A], target: A) -> Option[Int]

// Appends x to xs in place.
// @since 0.9.3 or earlier
list.push(mut xs: List[A], x: A) -> Unit

// Empty list; cap is only a preallocation hint.
// @since 0.15.1 or earlier
list.with_capacity(cap: Int) -> List[A]

// Removes and yields the last element; none if empty.
// @since 0.9.3 or earlier
list.pop(mut xs: List[A]) -> Option[A]

// Removes every element of xs in place.
// @since 0.9.3 or earlier
list.clear(mut xs: List[A]) -> Unit

// (take n, drop n); n past the end gives (xs, []), n < 0 gives ([], xs).
// @since 0.14.6 or earlier
list.split_at(xs: List[A], n: Int) -> (List[A], List[A])

// [seed, f(seed), ...] of length n; [] if n <= 0.
// @since 0.14.6 or earlier
list.iterate(seed: A, f: (A) -> A, n: Int) -> List[A]

// The first some that f gives, or none; f is not called after the hit.
// @since unreleased
list.find_map(xs: List[A], f: (A) -> Option[B]) -> Option[B]

// An empty list; the same value as [].
// @since unreleased
list.new() -> List[A]

// xs itself; the Container constructor from a list.
// @since unreleased
list.from_list(xs: List[A]) -> List[A]

// xs itself; the Container view as a list.
// @since unreleased
list.to_list(xs: List[A]) -> List[A]

// Stable sort by cmp: negative puts a first, 0 keeps input order.
// @since unreleased
list.sort_with(xs: List[A], cmp: (A, A) -> Int) -> List[A]

// Element with the least key f(x); the first on ties; f runs once per x.
// @since unreleased
list.min_by(xs: List[A], f: (A) -> K) -> Option[A]

// Element with the greatest key f(x); the first on ties; f runs once per x.
// @since unreleased
list.max_by(xs: List[A], f: (A) -> K) -> Option[A]

// Least element by cmp; the first on ties; none when empty.
// @since unreleased
list.min_with(xs: List[A], cmp: (A, A) -> Int) -> Option[A]

// Greatest element by cmp; the first on ties; none when empty.
// @since unreleased
list.max_with(xs: List[A], cmp: (A, A) -> Int) -> Option[A]

// Index of target among keys f(x) of xs sorted by key, or none.
// @since unreleased
list.binary_search_by(xs: List[A], target: K, f: (A) -> K) -> Option[Int]

// Index of target in xs sorted by cmp, or none; cmp(x, target) is asked.
// @since unreleased
list.binary_search_with(xs: List[A], target: A, cmp: (A, A) -> Int) -> Option[Int]

// (firsts, seconds) of a list of pairs; the inverse of zip.
// @since unreleased
list.unzip(xs: List[(A, B)]) -> (List[A], List[B])

// start, start + step, ... before end; counts down for step < 0; step = 0 aborts.
// @since unreleased
list.range_step(start: Int, end: Int, step: Int) -> List[Int]
```



## API 説明の訳（上記署名順）

- `list.len(xs: List[A]) -> Int` — 要素数。空のリストでは 0。
- `list.length(xs: List[A]) -> Int   (deprecated — use list.len)` — list.len の別名。
- `list.get(xs: List[A], i: Int) -> Option[A]` — インデックス i の要素。範囲外なら none。
- `list.set(xs: List[A], i: Int, x: A) -> List[A]` — スロット i = x としたコピー。i が範囲外なら何もしない。
- `list.swap(xs: List[A], i: Int, j: Int) -> List[A]` — i と j を交換したコピー。どちらかが範囲外なら何もしない。
- `list.sort(xs: List[A]) -> List[A]` — 昇順のコピー。Float は IEEE totalOrder に従う。
- `list.reverse(xs: List[A]) -> List[A]` — 要素を逆順にする。
- `list.contains(xs: List[A], x: A) -> Bool` — いずれかの要素が == x なら true。NaN は決して一致しない。
- `list.enumerate(xs: List[A]) -> List[(Int, A)]` — （インデックス、要素）の組。インデックスは 0 から。
- `list.zip(xs: List[A], ys: List[B]) -> List[(A, B)]` — 位置ごとの組。短い方のリストの終わりで止まる。
- `list.flatten(xs: List[List[A]]) -> List[A]` — 内側のリストを順に連結する。
- `list.take(xs: List[A], n: Int) -> List[A]` — 先頭の n 要素。n >= len なら全体、n < 0 なら []。
- `list.drop(xs: List[A], n: Int) -> List[A]` — 先頭の n 要素を除いた全体。n >= len なら []、n < 0 なら全体。
- `list.unique(xs: List[A]) -> List[A]` — 重複を除去し、最初の出現を残す。
- `list.index_of(xs: List[A], x: A) -> Option[Int]` — 最初の x のインデックス。なければ none。
- `list.last(xs: List[A]) -> Option[A]` — 最後の要素。空なら none。
- `list.chunk(xs: List[A], n: Int) -> List[List[A]]` — n 要素ずつの連なり。最後は短い場合がある。n <= 0 なら実行を中断する。
- `list.sum(xs: List[A]) -> A` — 任意の数値の和（R-L0-42）。空なら 0。整数はオーバーフロー時にラップする。
- `list.product(xs: List[A]) -> A` — 任意の数値の積（R-L0-42）。空なら 1。整数はオーバーフロー時にラップする。
- `list.first(xs: List[A]) -> Option[A]` — 先頭の要素。空なら none。
- `list.is_empty(xs: List[A]) -> Bool` — xs に要素がない場合に限り true。
- `list.min(xs: List[A]) -> Option[A]` — 最小の要素。空なら none。
- `list.max(xs: List[A]) -> Option[A]` — 最大の要素。空なら none。
- `list.join(xs: List[T], sep: T) -> T` — 各部分を sep で結合（R-L0-42）。String または Bytes。[] なら空。
- `list.map(xs: List[A], f: (A) -> B) -> List[B]` — 各要素に順に f を適用する。
- `list.filter(xs: List[A], f: (A) -> Bool) -> List[A]` — f が true となる要素。順序は保持する。
- `list.find(xs: List[A], f: (A) -> Bool) -> Option[A]` — f が成り立つ最初の要素。なければ none。
- `list.any(xs: List[A], f: (A) -> Bool) -> Bool` — いずれかの x で f が成り立つなら true。空なら false。
- `list.all(xs: List[A], f: (A) -> Bool) -> Bool` — すべての x で f が成り立つなら true。空なら true。
- `list.count(xs: List[A], f: (A) -> Bool) -> Int` — f が成り立つ要素の数。
- `list.flat_map(xs: List[A], f: (A) -> List[B]) -> List[B]` — f(x) のリストを順に連結する。
- `list.filter_map(xs: List[A], f: (A) -> Option[B]) -> List[B]` — f(x) の some 値を順に返す。
- `list.fold(xs: List[A], init: B, f: (B, A) -> B) -> B` — init からの左畳み込み。空なら init。
- `list.sort_by(xs: List[A], f: (A) -> B) -> List[A]` — キー f(x) による安定ソート。f は x ごとに 1 回実行される。
- `list.take_while(xs: List[A], f: (A) -> Bool) -> List[A]` — f が成り立つ最長の接頭部分。
- `list.drop_while(xs: List[A], f: (A) -> Bool) -> List[A]` — f が成り立つ最長の接頭部分を除いた残り。
- `list.partition(xs: List[A], f: (A) -> Bool) -> (List[A], List[A])` — （f が成り立つ要素、それ以外）。順序は保持する。
- `list.reduce(xs: List[A], f: (A, A) -> A) -> Option[A]` — xs[0] を初期値とする左畳み込み。空なら none。
- `list.group_by(xs: List[A], f: (A) -> B) -> Map[B, List[A]]` — キー f(x) → 要素。キーは最初に現れた順。
- `list.find_index(xs: List[A], f: (A) -> Bool) -> Option[Int]` — f が成り立つ最初の x のインデックス。なければ none。
- `list.update(xs: List[A], i: Int, f: (A) -> A) -> List[A]` — スロット i = f(old) としたコピー。i が範囲外なら何もしない。
- `list.scan(xs: List[A], init: B, f: (B, A) -> B) -> List[B]` — 畳み込みの途中の値。init は含めない。
- `list.zip_with(xs: List[A], ys: List[B], f: (A, B) -> C) -> List[C]` — 組に f を適用する。短い方のリストの終わりで止まる。
- `list.unique_by(xs: List[A], f: (A) -> K) -> List[A]` — 異なるキー f(x) ごとに最初の要素。
- `list.range(start: Int, end: Int) -> List[Int]` — start から end の手前まで。end <= start なら []。
- `list.slice(xs: List[A], start: Int, end: Int) -> List[A]` — インデックス start..<end。それぞれを 0..len に収める。start >= end なら []。
- `list.insert(xs: List[A], i: Int, x: A) -> List[A]` — i の位置に x を置いたコピー。i >= len または i < 0 なら末尾に追加。
- `list.remove_at(xs: List[A], i: Int) -> List[A]` — スロット i を除いたコピー。i が範囲外なら何もしない。
- `list.repeat(x: A, n: Int) -> List[A]` — x を n 個並べる。n <= 0 なら []。
- `list.intersperse(xs: List[A], sep: A) -> List[A]` — 隣接する要素の間に sep を置く。
- `list.dedup(xs: List[A]) -> List[A]` — 隣接して等しい連なりをまとめる。全体での重複除去ではない。
- `list.take_end(xs: List[A], n: Int) -> List[A]` — 末尾の n 要素。n >= len なら全体、n < 0 なら []。
- `list.drop_end(xs: List[A], n: Int) -> List[A]` — 末尾の n 要素を除いた全体。n >= len なら []、n < 0 なら全体。
- `list.window(xs: List[A], n: Int) -> List[List[A]]` — 長さ n のすべての連続部分。n > len なら []。n <= 0 なら実行を中断する。
- `list.binary_search(xs: List[A], target: A) -> Option[Int]` — 昇順（list.sort の順序）にソートされた xs 内の target のインデックス。なければ none。
- `list.push(mut xs: List[A], x: A) -> Unit` — xs の末尾に x をインプレースで追加する。
- `list.with_capacity(cap: Int) -> List[A]` — 空のリスト。cap は事前割り当てのヒントにすぎない。
- `list.pop(mut xs: List[A]) -> Option[A]` — 最後の要素を削除して返す。空なら none。
- `list.clear(mut xs: List[A]) -> Unit` — xs の全要素をインプレースで削除する。
- `list.split_at(xs: List[A], n: Int) -> (List[A], List[A])` — （先頭 n 要素、先頭 n 要素を除いた残り）。n が末尾を超えるなら (xs, [])、n < 0 なら ([], xs)。
- `list.iterate(seed: A, f: (A) -> A, n: Int) -> List[A]` — 長さ n の [seed, f(seed), ...]。n <= 0 なら []。
- `list.find_map(xs: List[A], f: (A) -> Option[B]) -> Option[B]` — f が返す最初の some。なければ none。見つかった後は f を呼ばない。
- `list.new() -> List[A]` — 空のリスト。[] と同じ値。
- `list.from_list(xs: List[A]) -> List[A]` — xs 自体。リストからの Container コンストラクター。
- `list.to_list(xs: List[A]) -> List[A]` — xs 自体。リストとしての Container ビュー。
- `list.sort_with(xs: List[A], cmp: (A, A) -> Int) -> List[A]` — cmp による安定ソート。負なら a を先にし、0 なら入力順を維持する。
- `list.min_by(xs: List[A], f: (A) -> K) -> Option[A]` — キー f(x) が最小の要素。同点なら最初のもの。f は x ごとに 1 回実行される。
- `list.max_by(xs: List[A], f: (A) -> K) -> Option[A]` — キー f(x) が最大の要素。同点なら最初のもの。f は x ごとに 1 回実行される。
- `list.min_with(xs: List[A], cmp: (A, A) -> Int) -> Option[A]` — cmp による最小の要素。同点なら最初のもの。空なら none。
- `list.max_with(xs: List[A], cmp: (A, A) -> Int) -> Option[A]` — cmp による最大の要素。同点なら最初のもの。空なら none。
- `list.binary_search_by(xs: List[A], target: K, f: (A) -> K) -> Option[Int]` — キー順にソートされた xs のキー f(x) における target のインデックス。なければ none。
- `list.binary_search_with(xs: List[A], target: A, cmp: (A, A) -> Int) -> Option[Int]` — cmp でソートされた xs 内の target のインデックス。なければ none。cmp(x, target) が呼ばれる。
- `list.unzip(xs: List[(A, B)]) -> (List[A], List[B])` — 組のリストを（第 1 要素群、第 2 要素群）にする。zip の逆。
- `list.range_step(start: Int, end: Int, step: Int) -> List[Int]` — start, start + step, ... と end の手前まで。step < 0 なら減少する。step = 0 なら実行を中断する。
