説明：Flow[T] の利用者向け仕様案 — Phase 1 後に docs/specs/flow.md へ昇格予定
<a id="flowt--user-specification-draft"></a>

# Flow[T] — 利用者向け仕様（草案）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](flow-spec-draft.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](flow-spec-draft.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](flow-spec-draft.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/flow-spec-draft.md)

> 原文注記：未提供の提案であり、現行 API や診断コード割当ではない。見出しは12関数だが列挙は13（source3、変換6、終端4）。file.lines 見出しに Result wrapper がない一方、本文・例は構築時エラーと ! を述べる。W005 と抑制は未検証の草案上の主張。メモリ境界は要素・行・accumulator・worker 結果の大きさが有界という前提を要し、値を出さない filter 後の take は停止を保証しない。panic 時の清理は提案の Rust Drop の説明で、abort や未実装 WASM backend の無条件保証ではない。混在例の lines は snippet 外を指す。他言語比較は歴史的原主張で新規検証ではない。


> 状態：**草案** — [flow-design.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/flow-design.md) の Phase 1 実装後、`docs/specs/flow.md` へ昇格予定。
>
> 提案された利用者向けリファレンスであり、出荷済み API ではない。設計根拠は [flow-design.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/flow-design.md)。
> `FLOW_FORBIDDEN_OP`、`FLOW_CONSUMED`、`FLOW_COLLECT_LIST` は記号的診断 placeholder で、割り当て済み compiler code ではない。以前の E011/E012/E013 は既に無関係な診断に属する。

---

<a id="1-overview"></a>

## 1. 概要

`Flow[T]` は**遅延・1回限り・前方向のみ**の値列。全てを memory に読み込まず処理したいとき、log file、streaming I/O、無限 generator、要素数不明の pipeline などに使う。

```almide
import fs

effect fn count_errors(path: String) -> Result[Int, String] = {
  let n = file.lines(path)!
    |> flow.filter((line) => string.contains(line, "ERROR"))
    |> flow.fold(0, (acc, _) => acc + 1)
  ok(n)
}
```

小さく有界な data で random access や複数 pass が必要なら、代わりに `List[T]` を使う。

---

<a id="2-core-properties"></a>

## 2. 基本性質

<a id="21-lazy"></a>

### 2.1 遅延

要素処理は**終端操作**（`fold`、`each`、`collect`、`find`）が呼ばれるまで遅延する。flow-design D1 の構築時 Err 例に定めるとおり、`file.lines(path)!` は source を即座に開き、構築時に失敗し得る。遅延はこの resource 取得を延期しない。pipeline の合成だけでは file を読まない。

```almide
let pipeline = file.lines("huge.log")!
  |> flow.filter(is_error)
  |> flow.map(parse_line)
// Nothing has been read yet

let count = pipeline |> flow.fold(0, (acc, _) => acc + 1)
// The already-open file is read one line at a time, counted, and closed
```

原コメント訳：まだ何も読んでいない。既に開いた file を1行ずつ読み、数えて閉じる。

<a id="22-single-pass"></a>

### 2.2 1回限り

Flow はちょうど1回消費できる。消費後の使用は compile error。

```almide
let flow = file.lines(path)!
let n1 = flow |> flow.fold(0, (acc, _) => acc + 1)  // consumes flow
let n2 = flow |> flow.fold(0, (acc, _) => acc + 1)  // ❌ FLOW_CONSUMED: use after consumption
```

原コメント訳：最初の行は flow を消費。次の行は消費後使用の FLOW_CONSUMED error。

複数 pass が必要なら先に `flow.collect()` で materialize する：

```almide
let xs: List[String] = file.lines(path)! |> flow.collect()  // now a List
let n1 = list.len(xs)      // OK
let n2 = list.len(xs)      // OK
```

原コメント訳：List になったので、その後の2回の長さ取得は OK。

<a id="23-forward-only"></a>

### 2.3 前方向のみ

消費せずに index access、反転、sort、count はできない。random-access 操作は compile error：

```almide
let flow = file.lines(path)!
list.len(flow)       // ❌ FLOW_FORBIDDEN_OP: Flow does not support len
list.get(flow, 5)    // ❌ FLOW_FORBIDDEN_OP: Flow does not support random access
list.reverse(flow)   // ❌ FLOW_FORBIDDEN_OP: Flow is forward-only
list.sort(flow)      // ❌ FLOW_FORBIDDEN_OP: Flow cannot be sorted
```

原コメント訳：Flow は len、random access、反転、sort をサポートしない。前方向のみ。

全一覧と代替は6節（禁止操作）参照。

---

<a id="3-when-to-use-flow-vs-list"></a>

## 3. Flow と List の使い分け

| 状況 | 使用 | 理由 |
|---|---|---|
| 約1,000要素未満 | `List[T]` | Flow overhead に見合わない |
| 約100,000要素超 | `Flow[T]` | memory O(1) 対 O(n) |
| サイズ不明（file、network、stdin） | `Flow[T]` | I/O の最も安全な既定 |
| 無限かもしれない source | `Flow[T]` | 必須 |
| index random access が必要 | `List[T]` | Flow は前方向のみ |
| 複数回反復が必要 | `List[T]` | Flow は1回限り |
| 走査せず長さが必要 | `List[T]` | Flow は長さを持たない |
| 有界並列処理 | `Flow[T]` + `fan.map(limit: n)` | 自動 backpressure |

**単純な規則**：RAM に収まりサイズ既知なら `List`。収まらない可能性やサイズ不明なら `Flow`。

---

<a id="4-api-reference"></a>

## 4. API リファレンス

<a id="41-sources"></a>

### 4.1 source

<a id="flowfrom_listt---flowt"></a>
<a id="flowfrom_listtxs-listt---flowt"></a>

#### `flow.from_list[T](xs: List[T]) -> Flow[T]`

List を Flow に変換。List data を Flow pipeline に適合させるのに便利。

```almide
let f: Flow[Int] = flow.from_list([1, 2, 3])
f |> flow.map((n) => n * 2) |> flow.fold(0, (acc, n) => acc + n)  // => 12
```

<a id="flowgenerates-t---optiont-s---flowt"></a>
<a id="flowgenerates-tseed-s-step-s---optiont-s---flowt"></a>

#### `flow.generate[S, T](seed: S, step: (S) -> Option[(T, S)]) -> Flow[T]`

seed から Flow を生成。`step` は現状態を受け取り、次の値＋新状態、または終了用 `none` を返す。

```almide
// Finite: 1..10
flow.generate(1, (n) => if n <= 10 then some((n, n + 1)) else none)

// Infinite: all natural numbers
flow.generate(1, (n) => some((n, n + 1)))

// Fibonacci
flow.generate((0, 1), (pair) => {
  let (a, b) = pair
  some((a, (b, a + b)))
})
```

原コメント訳：有限の1..10、無限の全自然数、Fibonacci の3例。

無限 flow もよいが、`flow.collect()` や無界 accumulator の `flow.fold()` より前に必ず `flow.take(n)` を置く。

<a id="filelinespath-string---flowstring--effect"></a>

#### `file.lines(path: String) -> Flow[String]` 🔴 effect

file を streaming で読み、1行が1要素。Flow の消費または drop 時に file handle を自動で閉じる。

```almide
effect fn main() = {
  file.lines("log.txt")!
    |> flow.each((line) => println(line))
}
```

**error**：open 失敗は `Result::Err`。途中 read の I/O error は panic（将来 phase の `file.lines_checked` で行ごとの error 処理）。

小 file には `fs.read_text(path)!` + `string.lines(...)` で直接 `List[String]` を得る方を推奨。

---

<a id="42-transformations-lazy"></a>

### 4.2 変換（遅延）

全変換は遅延し、終端操作まで値を pull しない。動詞名は `list.*` と完全一致。

<a id="flowmapt-u---u---flowu"></a>
<a id="flowmapt-uxs-flowt-f-t---u---flowu"></a>

#### `flow.map[T, U](xs: Flow[T], f: (T) -> U) -> Flow[U]`

各要素に `f` を適用。

```almide
file.lines(path)!
  |> flow.map(string.trim)
  |> flow.map(string.to_upper)
```

<a id="flowfiltert---bool---flowt"></a>
<a id="flowfiltertxs-flowt-pred-t---bool---flowt"></a>

#### `flow.filter[T](xs: Flow[T], pred: (T) -> Bool) -> Flow[T]`

`pred` が `true` を返す要素を保持。

```almide
flow.filter(lines, (l) => string.len(l) > 0)
```

<a id="flowfilter_mapt-u---optionu---flowu"></a>
<a id="flowfilter_mapt-uxs-flowt-f-t---optionu---flowu"></a>

#### `flow.filter_map[T, U](xs: Flow[T], f: (T) -> Option[U]) -> Flow[U]`

`f` を適用し、`some(...)` の結果だけ保持して unwrap。無効入力が混ざる parse に便利。

```almide
file.lines(path)!
  |> flow.filter_map((line) => parse_int(line))
```

<a id="flowflat_mapt-u---flowu---flowu"></a>
<a id="flowflat_mapt-uxs-flowt-f-t---flowu---flowu"></a>

#### `flow.flat_map[T, U](xs: Flow[T], f: (T) -> Flow[U]) -> Flow[U]`

Flow を返す `f` を適用し、結果を連結。

```almide
file.lines(path)!
  |> flow.flat_map((line) => flow.from_list(string.split(line, " ")))
```

<a id="flowtaket---flowt"></a>
<a id="flowtaketxs-flowt-n-int---flowt"></a>

#### `flow.take[T](xs: Flow[T], n: Int) -> Flow[T]`

最初の `n` 要素のみ保持。`n` 後は上流 pull を停止。

```almide
file.lines("big.log")!
  |> flow.take(100)            // only pull first 100 lines
  |> flow.collect()            // List[String] with up to 100 elements
```

原コメント訳：最初の100行のみ pull、最大100要素の List[String]。

<a id="flowdropt---flowt"></a>
<a id="flowdroptxs-flowt-n-int---flowt"></a>

#### `flow.drop[T](xs: Flow[T], n: Int) -> Flow[T]`

最初の `n` 要素を飛ばし、残りを出す。

```almide
file.lines("csv.txt")!
  |> flow.drop(1)              // skip header
  |> flow.each(process_row)
```

原コメント訳：header を飛ばす。

---

<a id="43-terminal-operations-"></a>

### 4.3 終端操作 🔴

終端操作は Flow を消費して結果を生成。**実際の評価はここで起こる。**

<a id="flowfoldt-u---u---u-"></a>
<a id="flowfoldt-uxs-flowt-init-u-combine-u-t---u---u-"></a>

#### `flow.fold[T, U](xs: Flow[T], init: U, combine: (U, T) -> U) -> U` 🔴

accumulator を `combine` に通して Flow を単一値に畳み込む。

```almide
// Count elements
flow.fold(0, (acc, _) => acc + 1)

// Sum
flow.fold(0, (acc, n) => acc + n)

// Build a Map
flow.fold(map.empty(), (m, (k, v)) => map.insert(m, k, v))
```

原コメント訳：要素を数える、合計する、Map を作る。

`fold` は Flow **全体**を走査。早期終了には `flow.find`、または上流で `flow.take` を使う。

<a id="floweacht---unit---unit-"></a>
<a id="floweachtxs-flowt-f-t---unit---unit-"></a>

#### `flow.each[T](xs: Flow[T], f: (T) -> Unit) -> Unit` 🔴

副作用のため各要素へ `f` を適用。何も返さない。

```almide
file.lines(path)!
  |> flow.each((line) => println(line))
```

<a id="flowcollectt---listt-"></a>
<a id="flowcollecttxs-flowt---listt-"></a>

#### `flow.collect[T](xs: Flow[T]) -> List[T]` 🔴

Flow を List に materialize。**唯一の Flow → List 境界。**

```almide
let first_10: List[String] = file.lines(path)!
  |> flow.take(10)
  |> flow.collect()
```

**警告（W005）**：上流 `flow.take` なしの `flow.collect()` は、source サイズが無界なら compiler warning。有限かつ小さいと分かる場合 `let _ = flow.collect(xs)` で抑制するか `|> flow.take(N)` を追加。

<a id="flowfindt---bool---optiont--short-circuit"></a>
<a id="flowfindtxs-flowt-pred-t---bool---optiont--short-circuit"></a>

#### `flow.find[T](xs: Flow[T], pred: (T) -> Bool) -> Option[T]` 🔴（短絡）

`pred` が `true` を返す最初の要素、または `none`。**短絡**し、一致を見つけ次第上流 pull を停止。

```almide
// First ERROR line in the file (stops reading after finding it)
file.lines("big.log")!
  |> flow.find((l) => string.contains(l, "ERROR"))
```

原コメント訳：file の最初の ERROR 行。見つかったら read 停止。

検索は `fold` より `find`。`fold` は source 全体を走査する。

---

<a id="5-resource-cleanup"></a>

## 5. resource cleanup

外部 resource（file handle、network socket）を持つ Flow は、消費または scope 外に出ると自動 cleanup。手動 close は不要。

```almide
effect fn read_first_5(path: String) -> Result[List[String], String] = {
  let xs = file.lines(path)!
    |> flow.take(5)
    |> flow.collect()
  // File is already closed here — flow was consumed by collect()
  ok(xs)
}
```

原コメント訳：collect() が Flow を消費したため、ここでは file は既に閉じている。

途中放棄（panic、early return、scope exit）でも Flow が drop され resource が解放される：

```almide
effect fn process(path: String) -> Result[Unit, String] = {
  let _n = file.lines(path)!
    |> flow.take(10)           // only read 10 lines
    |> flow.fold(0, (acc, _) => acc + 1)
  // Remaining lines are never read; file handle closed after take(10)
  ok(())
}
```

原コメント訳：10行だけ読み、残りは読まない。take(10) 後に file handle を閉じる。

Almide では **`.close()` を書くことはない**。scope が lifetime を管理。

---

<a id="6-forbidden-operations"></a>

## 6. 禁止操作

`Flow[T]` の以下の操作は **compile error**（`FLOW_FORBIDDEN_OP`）。各々代替案がある。

| 操作 | 禁止理由 | 代替 |
|---|---|---|
| `list.len(flow)` | Flow は無限かもしれない | `flow.fold(0, (acc, _) => acc + 1)` |
| `list.get(flow, i)` | Flow は前方向のみ | `flow.drop(i) \|> flow.take(1) \|> flow.find((_) => true)` |
| `list.reverse(flow)` | Flow は前方向のみ | `flow.collect() \|> list.reverse` |
| `list.sort(flow)` | sort は全要素が必要 | `flow.collect() \|> list.sort` |
| `list.contains(flow, x)` | 無限 source を走査し得る | `flow.find((y) => y == x) != none` |
| `list.last(flow)` | 無限 source を走査することになる | `flow.fold(none, (_, x) => some(x))` |

compiler は「遅延の代替」（推奨、memory safe）と「先に materialize」（memory 警告付き）の両 hint を出す。error 例は8節。

---

<a id="7-common-patterns"></a>

## 7. よくある pattern

<a id="71-log-analysis"></a>

### 7.1 log 解析

```almide
import fs

effect fn count_errors(path: String) -> Result[Int, String] = {
  let n = file.lines(path)!
    |> flow.filter((line) => string.contains(line, "ERROR"))
    |> flow.fold(0, (acc, _) => acc + 1)
  ok(n)
}
```

**memory**：file サイズによらず O(1)。

<a id="72-csv--json-lines-conversion"></a>

### 7.2 CSV → JSON Lines 変換

```almide
effect fn csv_to_jsonl(input: String, output: String) -> Result[Unit, String] = {
  file.lines(input)!
    |> flow.drop(1)                                       // skip header
    |> flow.map((line) => row_to_json(string.split(line, ",")))
    |> flow.each((j) => fs.append_text(output, json.stringify(j) + "\n")!)
  ok(())
}
```

原コメント訳：header を飛ばす。

**memory**：O(1)。1行ずつ処理。

<a id="73-early-search-find"></a>

### 7.3 早期検索（find）

```almide
effect fn first_error_line(path: String) -> Result[Option[String], String] = {
  let found = file.lines(path)!
    |> flow.find((line) => string.contains(line, "ERROR"))
  ok(found)
}
```

**memory**：O(1)。最初の一致で read 停止。

<a id="74-first-n-lines"></a>

### 7.4 最初の N 行

```almide
effect fn head(path: String, n: Int) -> Result[List[String], String] = {
  let lines = file.lines(path)!
    |> flow.take(n)
    |> flow.collect()
  ok(lines)
}
```

**memory**：O(n)、引数で制限。

<a id="75-infinite-source--finite-use"></a>

### 7.5 無限 source＋有限使用

```almide
fn first_100_primes() -> List[Int] = 
  flow.generate(2, (n) => some((n, n + 1)))
    |> flow.filter(is_prime)
    |> flow.take(100)
    |> flow.collect()
```

**memory**：O(100)。`take` が消費を制限するので無限 generator は安全。

<a id="76-mixing-list-and-flow"></a>

### 7.6 List と Flow の混在

```almide
let allowed_users: List[String] = ["alice", "bob"]          // small, List

effect fn filter_user_logs(log_path: String) -> Result[Int, String] = {
  let n = file.lines(log_path)!                              // huge, Flow
    |> flow.filter_map((line) => {
        let j = json.parse(line)?
        let user = json.get_string(j, "user")?
        if list.contains(allowed_users, user) then some(line) else none
      })
    |> flow.fold(0, (acc, _) => acc + 1)
  ok(n)
}
```

原コメント訳：小さい allowed_users は List、大きい log は Flow。

**注**：`list.contains` は List の `allowed_users` に、`flow.filter_map` は Flow の `lines` に作用。各 module は自分の型を処理する。

<a id="77-parallel-processing-phase-2"></a>

### 7.7 並列処理（Phase 2+）

```almide
effect fn fetch_all(url_file: String) -> Result[Unit, String] = {
  file.lines(url_file)!
    |> flow.filter((url) => string.starts_with(url, "https://"))
    |> fan.map(limit: 10, (url) => http.get(url)!)           // bounded parallel
    |> flow.each((body) => write_to_disk(body)!)
  ok(())
}
```

原コメント訳：有界並列。

**memory**：`limit`（10並列 worker）で制限。backpressure は自動で、worker が空くまで上流 Flow を pull しない。

---

<a id="8-error-messages"></a>

## 8. error message

<a id="flow_forbidden_op--forbidden-operation-on-flow"></a>

### FLOW_FORBIDDEN_OP — Flow の禁止操作

```
error[FLOW_FORBIDDEN_OP]: list.len cannot be called on Flow[String]
  --> bad.almd:5:11
   |
 5 |   let n = list.len(lines)
   |           ^^^^^^^^^^^^^^^
   = note: Flow[T] is lazy and possibly infinite
   = hint: to count lazily without materialization:
           lines |> flow.fold(0, (acc, _) => acc + 1)
   = hint: to materialize first (may use unbounded memory):
           lines |> flow.collect() |> list.len
```

診断全文訳：Flow[String] に list.len は呼べない（bad.almd:5:11、該当呼出しを指す）。Flow[T] は遅延で無限かもしれない。materialize せず遅延 count するには提示の fold を使う。先に materialize する代替は提示の collect → list.len だが無界 memory を使う可能性がある。

全 FLOW_FORBIDDEN_OP は**2 hint**を含む。推奨の遅延代替、memory 警告付き materialize-first fallback。

<a id="flow_consumed--flow-consumed-twice"></a>

### FLOW_CONSUMED — Flow を2回消費

```
error[FLOW_CONSUMED]: Flow[String] is move-only and has been consumed
  --> double.almd:4:11
   |
 3 |   let n1 = flow |> flow.fold(0, (acc, _) => acc + 1)
   |            ---- consumed here
 4 |   let n2 = flow |> flow.fold(0, (acc, _) => acc + 1)
   |            ^^^^ used after consumption
   = note: Flow is single-pass and cannot be consumed twice
   = hint: materialize first if you need multiple passes:
           let xs = flow |> flow.collect()
           let n1 = list.len(xs)
           let n2 = list.len(xs)
```

診断全文訳：Flow[String] は move-only で既に消費済み（double.almd:4:11）。3行目で消費、4行目は消費後の使用。Flow は1回限りで2回消費不可。複数 pass が必要なら先に materialize する。提示の xs = collect 後、n1、n2 で List の長さを取得する。

<a id="flow_collect_list--flowcollect-on-a-list"></a>

### FLOW_COLLECT_LIST — List に対する `flow.collect`

```
error[FLOW_COLLECT_LIST]: flow.collect expects Flow[T], got List[T]
  --> wrong.almd:2:6
   |
 2 |   [1, 2, 3] |> flow.collect()
   |                ^^^^^^^^^^^^^^
   = note: List is already materialized, no conversion needed
   = hint: just use the list directly: [1, 2, 3]
```

診断全文訳：flow.collect は Flow[T] を期待したが List[T] を受け取った（wrong.almd:2:6）。List は既に materialize され変換不要。提示された [1, 2, 3] を直接使う。

<a id="w005--unbounded-flowcollect"></a>

### W005 — 無界 `flow.collect`

```
warning[W005]: flow.collect() on a flow without upstream flow.take
  --> risky.almd:3:6
   |
 3 |   |> flow.collect()
   |      ^^^^^^^^^^^^^^
   = note: if the source is large or infinite, this will OOM
   = hint: bound the collection:
           flow.take(N) |> flow.collect()
   = hint: or use flow.fold for accumulation:
           flow.fold(init, combine)
   = hint: if you know the source is finite and small, suppress with:
           let _ = flow.collect(flow)
```

警告全文訳：上流 flow.take のない flow.collect()（risky.almd:3:6）。source が大きいか無限なら OOM。提示の take(N) → collect で制限する、または fold(init, combine) で蓄積する。有限かつ小さいと分かる場合は提示の let _ = flow.collect(flow) で抑制。

---

<a id="9-quick-reference"></a>

## 9. 早見表

<a id="all-12-functions-at-a-glance"></a>

### 全12関数を一覧

```
Sources
  flow.from_list(xs)                          List[T]     -> Flow[T]
  flow.generate(seed, step)                   S           -> Flow[T]
  file.lines(path)            🔴 effect       String      -> Flow[String]

Transformations (lazy)
  flow.map(xs, f)                             Flow[T]     -> Flow[U]
  flow.filter(xs, pred)                       Flow[T]     -> Flow[T]
  flow.filter_map(xs, f)                      Flow[T]     -> Flow[U]
  flow.flat_map(xs, f)                        Flow[T]     -> Flow[U]
  flow.take(xs, n)                            Flow[T]     -> Flow[T]
  flow.drop(xs, n)                            Flow[T]     -> Flow[T]

Terminal operations
  flow.fold(xs, init, combine)  🔴            Flow[T]     -> U
  flow.each(xs, f)              🔴            Flow[T]     -> Unit
  flow.collect(xs)              🔴            Flow[T]     -> List[T]
  flow.find(xs, pred)           🔴 (short)    Flow[T]     -> Option[T]
```

表内英語訳：Sources は source、Transformations (lazy) は遅延変換、Terminal operations は終端操作。effect は効果付き、short は短絡を指す。各署名・型対応は上表をそのまま保持。

<a id="verb-alignment-with-list"></a>

### `list.*` との動詞対応

| list.* | flow.* |
|---|---|
| `list.map` | `flow.map` |
| `list.filter` | `flow.filter` |
| `list.filter_map` | `flow.filter_map` |
| `list.flat_map` | `flow.flat_map` |
| `list.take` | `flow.take` |
| `list.drop` | `flow.drop` |
| `list.fold` | `flow.fold` |
| `list.each` | `flow.each` |
| `list.find` | `flow.find` |
| `list.from_list` | （不要） |
| （list にない） | `flow.collect`（境界） |
| （list にない） | `flow.generate`（source） |

---

<a id="10-faq"></a>

## 10. FAQ

**Q：Flow を2回反復できる？**
A：不可。1回限り。2回必要なら `let xs = flow |> flow.collect()` で先に materialize し、List の `xs` を複数回反復。

**Q：数 KB の file なら `file.lines` と `fs.read_text` のどちら？**
A：`fs.read_text(path)!` + `string.lines(...)`。小 file には単純な `List[String]` を得られる。

**Q：巨大 file の行数を数える最良方法は？**
A：`file.lines(path)! |> flow.fold(0, (acc, _) => acc + 1)`。memory O(1)。

**Q：要素の random access が必要。Flow を使える？**
A：不可。前方向のみ。List を使うか先に `flow.collect()`。

**Q：無限 Flow を `flow.collect` できる？**
A：compiler は W005 警告を出すが、実行を試み最終的に OOM になる。無界 source の collect 前には常に `flow.take(n)`。

**Q：2つの Flow を結合するには？**
A：Phase 1 に `flow.chain` はない。代替に `flow.flat_map(flow.from_list([a, b]), (f) => f)`、または Phase 3 を待つ。

**Q：`flow.fold` 中に panic した場合 file handle は？**
A：自動で閉じる。Almide は Rust の `Drop` trait を使い、panic を含め Flow drop 時に保持 resource を解放する。

**Q：消費せず Flow の長さを取得できる？**
A：不可。遅延かつ無限かもしれない列の長さは未知。`flow.fold(0, (acc, _) => acc + 1)` で数える（Flow 消費）、または `flow.collect() |> list.len` で materialize。

**Q：Rust Iterator や Kotlin Sequence と比べると？**
A：遅延・前方向のみという意味は同様。差は：
- Almide の module 名は `flow.*`、Rust は Iterator の method call、Kotlin は `Sequence` extension function。
- Almide は `list.len(flow)` を compile 時拒否。Rust と Kotlin は許すが無限 source で hang。
- Almide は Rust 同様 Drop で file handle を自動 cleanup。scope block が必要な Kotlin `useLines` とは異なる。

**Q：`fan.map` との連携は？**
A：`fan.map(flow, limit: n, f)` は自動 backpressure 付き並列処理。結果も `Flow[U]` で、遅延・streaming のまま。並列 HTTP fetch や file 処理に便利。

---

<a id="11-comparison-with-other-languages"></a>

## 11. 他言語との比較

LLM／開発者の転移学習用：

| Almide | Rust | Kotlin | MoonBit | Haskell |
|---|---|---|---|---|
| `Flow[T]` | `impl Iterator<Item=T>` | `Sequence<T>` | `Iter[T]` | `[T]`（全て遅延） |
| `flow.map` | `.map` | `.map` | `.map` | `fmap` |
| `flow.filter` | `.filter` | `.filter` | `.filter` | `filter` |
| `flow.fold` | `.fold` | `.fold` | `.fold` | `foldl'` |
| `flow.find` | `.find` | `.find` | `.find` | `find` |
| `flow.collect` | `.collect::<Vec<_>>()` | `.toList()` | `.collect()` | （自動） |
| `flow.take(n)` | `.take(n)` | `.take(n)` | `.take(n)` | `take n` |
| `file.lines(path)` | `BufRead::lines()` | `File.useLines { }` | `@fs.lines` | `lines <$> readFile` |
| move-only | move/borrow | （mutable） | single-pass | （pure lazy） |
| 自動 cleanup | Drop | `useLines { }` scope | 手動 | lazy I/O（問題あり） |

**最も近い先例**：MoonBit の `Iter[T]`。Almide Flow は1回限り・遅延・chain 推奨設計を共有し、module 命名規約が異なる。

---

<a id="12-implementation-status"></a>

## 12. 実装状態

本書は**目標設計**。実装追跡は [flow-design.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/flow-design.md)。

- [ ] Phase 1：基本 `Flow[T]` 型＋12 API＋compile 時検査
- [ ] Phase 2：`fan.map` × Flow 統合（並列 streaming）
- [ ] Phase 3：追加操作（`zip`、`chain`、`take_while` 等）
- [ ] Phase 4：Async／WASM backend

Phase 1 完了時に `docs/specs/flow.md` へ移し、`> Last updated: YYYY-MM-DD` header を付け、draft 状態を外す。
