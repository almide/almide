説明：Flow[T] の遅延ストリーミング列。flow.* 名前空間の動詞を list.* と揃える。
<a id="flowt--lazy-streaming-sequences"></a>

# Flow[T] — 遅延ストリーミング列

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](flow-design.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](flow-design.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](flow-design.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/flow-design.md)

> 原文注記：未実装の歴史的提案であり、現行 Flow API や安全保証ではない。記号的診断は未割当で、W005/W006 も現行登録簿で要確認。Drop・unwind・cancel、定数メモリ、名前空間や LLM 比較は原文の設計主張で、独立検証した platform 挙動や測定済み優位ではない。O(1) は可変要素サイズや下流 buffer を含まず、上流 filter が停止しなければ take を置いても停止しない場合がある。元の ? 注釈と ! コードの相違、unordered fan・timeout・旧 TOML パス・将来 API は現行仕様を上書きしない。8/10 の 1-shot は目標で結果ではない。command・benchmark 未実行。


> 未実装の設計提案。以下の `FLOW_FORBIDDEN_OP`、`FLOW_CONSUMED`、
> `FLOW_COLLECT_LIST` は記号的な仮置きで、割り当て済み診断コードではない。
> E011/E012/E013 は既に `docs/diagnostics/` で別のエラーを表す。
> Flow 実装は必ず現行登録簿で診断を割り当てる。

<a id="design-philosophy"></a>

## 設計理念

Almide は一般に語彙削減を重視するが、Flow については **lazy/eager 境界と stream 性を局所的に明示する価値が高い** ため、`flow.*` 名前空間を採用する。

ただし学習コストを抑えるため、**操作名は `list.*` と動詞を揃える**。`list.map` と `flow.map`、`list.filter` と `flow.filter` のように、prefix だけ変える。LLM は動詞の語彙を 1 セット覚えればよく、「型 → prefix」のマッピングは 1 ルールだけ追加で覚えればよい。

境界 API は `flow.collect` 1 個のみ。**暗黙変換は行わない**。`file.lines` のような streaming source は必ず `Flow[T]` を返す。

> Almide は一般に語彙削減を重視するが、Flow については lazy/eager 境界と stream 性を局所的に明示する価値が高いため、`flow.*` 名前空間を採用する。ただし学習コストを抑えるため、操作名は `list.*` と可能な限り揃える。

この段落は `docs/specs/flow.md` の冒頭にも置き、全員がこの原則を共有する。

<a id="why-flow-not-shared-list"></a>

## なぜ `flow.*` なのか（`list.*` を共用しない理由）

`list.*` を List[T] / Flow[T] 両対応にする案は以下の理由で却下:

| 却下理由 | 詳細 |
|---|---|
| **前例がない** | MoonBit/Gleam/Kotlin/Rust どれも名前空間を分けている |
| **user 関数が不自然** | `Seq[T]` のような higher-kinded 抽象が必要、実装コスト高 |
| **転移学習が効かない** | LLM は `iterator.filter`/`stream.map` を膨大に訓練データで見てる |
| **長距離依存の追跡困難** | モジュール名が型情報を運ばないと LLM が遠くの定義を追えない |
| **stdlib 多態 / user 単型の不整合** | 「stdlib だけ魔法、user は普通」が LLM を最も混乱させる |

保守的な `flow.*` 分離 + 動詞揃えの方が **Almide のミッション (LLM が最も正確に書ける言語) により適合する**。

<a id="core-type"></a>

## 核となる型

`Flow[T]` は **lazy, single-pass, forward-only** なシーケンス (MoonBit の `Iter[T]` に対応)。

| 性質 | 意味 |
|---|---|
| **lazy** | terminal operation (fold/each/collect/find) が呼ばれるまで値を pull しない |
| **single-pass** | 一度 consume したら再利用不可。二重使用はコンパイルエラー (後述) |
| **forward-only** | ランダムアクセス不可。`len`, `get`, `reverse`, `sort` は型エラー |

<a id="decided-semantics"></a>

## 決定した意味論

以下の項目を Phase 1 前に決定する。ここが曖昧だと実装が沼る。

<a id="d1-filelines-の型-effect--lazy-境界"></a>

### D1. `file.lines` の型 (Effect × lazy 境界)

```almide
effect fn file.lines(path: String) -> Flow[String]
```

**決定**: ファイルを開くまでは fallible (`effect fn` → `Result` 経由)、開いた後の読み取りエラーは panic。

理由:
- ファイル未存在などの「よくあるエラー」は `!` or `?` で扱える
- 読み取り途中のエラー (ディスク切断など) は極めて稀、panic で十分
- 各行を `Result[String, IOError]` でラップすると pipeline が冗長 (Rust の `Lines::Item = io::Result<String>` の冗長さ参照)
- 80/20 で pragmatic な選択

```almide
// 使い方: 開けない場合は ? で早期リターン
effect fn count_errors(path: String) -> Result[Int, String] = {
  let lines = file.lines(path)!      // ← 開けなければ即 Err
  let n = lines
    |> flow.filter((l) => string.contains(l, "ERROR"))
    |> flow.fold(0, (acc, _) => acc + 1)
  ok(n)
}
```

堅牢な per-line エラー処理が必要な場合 (将来):
```almide
effect fn file.lines_checked(path: String) -> Flow[Result[String, IOError]]
```
は Phase 3 以降で検討。Phase 1 は `file.lines` のみ。

<a id="d2-resource-cleanup-drop-意味論"></a>

### D2. リソース清掃（Drop 意味論）

**決定**: Flow は Rust の `Drop` trait で cleanup する。ユーザーは何も書かない。

```
Flow の内部表現: Box<dyn Iterator<Item = T>>
           ↓
File-backed Flow: BufReader<File> を Iterator として wrap
           ↓
Iterator が drop される時点で BufReader → File と順に drop
           ↓
ファイルハンドル自動 close
```

保証される動作:

1. **scope 脱出**: 関数を抜けた時点で Flow が drop → 上流リソース cleanup
2. **早期終了** (`take(10)` の後に消費しない): 10 要素 pull した後、Flow が drop → 残り読まない + ハンドル close
3. **panic**: fold 中の panic で stack unwinding → Flow drop → cleanup
4. **fan スコープ中の失敗**: `fan.map` の worker 失敗 → scope 抜け → Flow drop → cleanup

**Haskell lazy I/O の失敗パターン (ハンドルが scope を超えて生存する) は Rust Drop で自然に回避**。

<a id="d3-single-pass-違反検出"></a>

### D3. Single-pass 違反検出

**決定**: Flow は **move-only**。Almide の所有権解析で二重使用はコンパイルエラー。

```almide
let flow = file.lines(path)!
let n1 = flow |> flow.fold(0, (acc, _) => acc + 1)   // flow を消費 (move)
let n2 = flow |> flow.fold(0, (acc, _) => acc + 1)   // ❌ error: use after move
// error[FLOW_CONSUMED]: Flow[String] is move-only and has been consumed
//   --> file.almd:3:11
//    |
//  2 |   let n1 = flow |> flow.fold(...)
//    |            ---- consumed here
//  3 |   let n2 = flow |> flow.fold(...)
//    |            ^^^^ used after consumption
//    = hint: Flow is single-pass. If you need to iterate twice, use flow.collect()
//            to materialize first: let xs = flow.collect(flow)
```

診断文訳：Flow[String] は移動のみ可能で、既に消費済み。消費位置と再使用位置を示す。Flow は単一走査なので、2回反復するなら先に flow.collect() で実体化するよう案内する。

Almide の既存 use-count 解析に「Flow は clone 不可」ルールを追加するだけで実装可能 (通常の型より厳しく、Move のみ)。

<a id="d4-flowcollect-without-flowtake-は-lint-warning"></a>

### D4. `flow.take` のない `flow.collect` は lint 警告

**決定**: `flow.collect(flow)` 単体 (上流に `take` がない) は lint 警告を出す。

```almide
file.lines(path)!
  |> flow.filter(p)
  |> flow.collect()       // ⚠️ warning: unbounded flow.collect
// warning[W005]: flow.collect() without upstream flow.take may OOM
//   hint: add `|> flow.take(n)` upstream if the source may be large,
//         or suppress with `_ = flow.collect(...)` if you are sure
```

警告文訳：上流に flow.take がない flow.collect() はメモリ不足の可能性がある。大きいソースなら上流に |> flow.take(n) を加え、確信があれば _ = flow.collect(...) で抑制する。

警告であってエラーではない。user が上流の finite 性を知っていれば問題ないが、LLM が機械的に `collect()` を付ける事故を防ぐ。

<a id="d5-flowgenerate-の有限性は型で区別しない"></a>

### D5. `flow.generate` の有限性は型で区別しない

**決定**: `flow.generate(seed, step)` は step が `None` を返すと終わる。型は常に `Flow[T]` (有限/無限は区別しない)。

```almide
flow.generate(0, (n) => if n < 10 then some((n, n + 1)) else none)   // 有限 (10 要素)
flow.generate(0, (n) => some((n, n + 1)))                             // 無限
// どちらも Flow[Int]、型は同じ
```

有限性は **定量化しにくい** ので型では追わない。無限事故は D4 の lint で防ぐ。

---

<a id="minimum-api-12-functions"></a>

## 最小 API（12 関数）

<a id="transformations-6"></a>

### 変換（6）

`list.*` と **動詞を完全に揃える**。

```almide
flow.map[T, U](xs: Flow[T], f: (T) -> U) -> Flow[U]
flow.filter[T](xs: Flow[T], pred: (T) -> Bool) -> Flow[T]
flow.filter_map[T, U](xs: Flow[T], f: (T) -> Option[U]) -> Flow[U]
flow.flat_map[T, U](xs: Flow[T], f: (T) -> Flow[U]) -> Flow[U]
flow.take[T](xs: Flow[T], n: Int) -> Flow[T]
flow.drop[T](xs: Flow[T], n: Int) -> Flow[T]
```

<a id="terminal-operations-4"></a>

### 終端操作（4）

**ここで評価が走る**。docs と cheatsheet で 🔴 マークを付ける。

```almide
flow.fold[T, U](xs: Flow[T], init: U, combine: (U, T) -> U) -> U       // 🔴
flow.each[T](xs: Flow[T], f: (T) -> Unit) -> Unit                       // 🔴
flow.collect[T](xs: Flow[T]) -> List[T]                                 // 🔴
flow.find[T](xs: Flow[T], pred: (T) -> Bool) -> Option[T]               // 🔴 短絡
```

`flow.find` は **短絡評価**する (最初に見つかった時点で上流の pull を止める)。これが `fold` と違うポイント。`find` を入れたのは「短絡検索」を fold で書くと全走査してしまうバグの典型を避けるため。

<a id="source-constructors-2"></a>

### ソース構築子（2）

```almide
flow.from_list[T](xs: List[T]) -> Flow[T]
flow.generate[S, T](seed: S, step: (S) -> Option[(T, S)]) -> Flow[T]
```

`flow.empty` は `flow.from_list([])` で代用。`flow { emit(...) }` builder は Phase 3 以降。

<a id="組み込みソース-stdlib-他モジュール経由"></a>

### 組み込みソース (stdlib 他モジュール経由)

- `file.lines(path: String) -> Flow[String]` (D1 参照、`effect fn`)
- 将来: `io.read_lines() -> Flow[String]` (stdin streaming)
- 将来: `http.stream(url: String) -> Flow[Bytes]`

---

<a id="key-rules-決定事項"></a>

## 主要ルール（決定事項）

<a id="r1-filelines-は-flowstring-を返す"></a>

### R1. `file.lines` は `Flow[String]` を返す

小さいファイル全体を読む場合は `fs.read_text` + `string.lines` を使う。**曖昧にしない**。

```almide
// 小さい config → fs.read_text + string.lines
let content = fs.read_text("config.toml")!
let lines: List[String] = string.lines(content)

// 巨大ログ → file.lines
let lines: Flow[String] = file.lines("system.log")!
```

<a id="r2-flowcollect-が唯一の-flow--list-境界"></a>

### R2. `flow.collect` が唯一の Flow → List 境界

暗黙変換なし。型注釈による強制変換もなし。

```almide
let xs: List[String] = flow |> flow.collect()   // 明示
```

<a id="r3-flowtaken-の戻り型は-flowt"></a>

### R3. `flow.take(n)` の戻り型は `Flow[T]`

特例なし。List が欲しければ `|> flow.collect()` を続ける。

```almide
let first_10: List[String] = file.lines(path)! |> flow.take(10) |> flow.collect()
```

<a id="r4-terminal-operations-は-docs-で明示マーク"></a>

### R4. Terminal operations は docs で明示マーク

cheatsheet で `fold`, `each`, `collect`, `find` に 🔴 terminal マーク。「ここで実行される」を視覚的に強調。

<a id="r5-list-と-flow-の動詞は必ず揃える"></a>

### R5. `list.*` と `flow.*` の動詞は必ず揃える

両方にある関数は **同じ動詞名**。片方だけ別名にしない。

**禁止例**:
- ❌ `flow.keep_if` (list が `filter` なら flow も `filter`)
- ❌ `flow.transform` (list が `map` なら flow も `map`)
- ❌ `flow.reduce` (list が `fold` なら flow も `fold`)
- ❌ `flow.fetch_first` (list が `find` なら flow も `find`)

<a id="r6-single-pass-を-cheatsheet-冒頭で宣言"></a>

### R6. Single-pass を cheatsheet 冒頭で宣言

```markdown
## Flow の基本性質

Flow は single-pass です。一度 consume (fold/each/collect/find) した Flow は再利用できません。
Almide は所有権解析で二重使用をコンパイル時に検出します。
```

<a id="r7-listt--flowt-は-flowfrom_list-で明示"></a>

### R7. `List[T] → Flow[T]` は `flow.from_list` で明示

自動昇格なし。**使う時は書く**。

```almide
let xs: List[Int] = [1, 2, 3]
let f: Flow[Int] = flow.from_list(xs)
```

---

<a id="forbidden-operations-compile-time-拒否"></a>

## 禁止操作（コンパイル時拒否）

以下は `Flow[T]` に対して呼ぶとコンパイルエラー (`FLOW_FORBIDDEN_OP`):

| 操作 | 代替 |
|---|---|
| `list.len(flow)` | `flow.fold(0, (acc, _) => acc + 1)` |
| `list.get(flow, i)` | `flow.drop(i) \|> flow.take(1) \|> flow.find((_) => true)` |
| `list.reverse(flow)` | `flow.collect()` してから `list.reverse` |
| `list.sort(flow)` | `flow.collect()` してから `list.sort` |
| `list.contains(flow, x)` | `flow.find((y) => y == x) != none` |
| `list.last(flow)` | `flow.fold(none, (_, x) => some(x))` |

---

<a id="error-message-templates"></a>

## エラーメッセージのひな形

<a id="flow_forbidden_op--forbidden-operation-on-flow"></a>

### FLOW_FORBIDDEN_OP — Flow で禁止される操作

```
error[FLOW_FORBIDDEN_OP]: list.len cannot be called on Flow[String]
  --> bad.almd:5:11
   |
 5 |   let n = list.len(lines)
   |           ^^^^^^^^^^^^^^^
   = note: Flow[T] is lazy and possibly infinite
   = hint: to count lazily without materialization:
           lines |> flow.fold(0, (acc, _) => acc + 1)
   = hint: to materialize first (uses unbounded memory):
           lines |> flow.collect() |> list.len
```

診断文訳：Flow[String] に list.len は呼べない（bad.almd:5:11）。Flow[T] は遅延評価で無限の可能性がある。実体化せず fold で数える案と、無制限メモリを使って collect 後に list.len を呼ぶ案を示す。

**hint は必ず 2 本** 提示する:
1. Flow 的な解決策 (推奨、メモリ安全)
2. materialize 経由の解決策 (memory 警告付き)

LLM がどちらも学習できるように。

<a id="flow_consumed--flow-move-after-consume"></a>

### FLOW_CONSUMED — 消費後の Flow 移動

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

診断文訳：Flow[String] は移動専用で消費済み（double.almd:4:11）。前行での消費と再使用を示す。二重消費不可なので、複数走査には先に collect して xs を作り、list.len(xs) を2回呼ぶ。

<a id="w005--unbounded-flowcollect"></a>

### W005 — 上限のない flow.collect

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

警告文訳：risky.almd:3:6 の flow.collect() には上流 flow.take がない。巨大または無限ソースではメモリ不足になる。flow.take(N) で収集に上限を設ける、flow.fold(init, combine) で集約する、有限かつ小さいと分かる場合 let _ = flow.collect(flow) で抑制する、の3案を示す。

<a id="flow_collect_list--flowcollect-on-already-list"></a>

### FLOW_COLLECT_LIST — 既に List である値の flow.collect

```
error[FLOW_COLLECT_LIST]: flow.collect expects Flow[T], got List[T]
  --> wrong.almd:2:6
   |
 2 |   [1, 2, 3] |> flow.collect()
   |                ^^^^^^^^^^^^^^
   = note: List is already materialized, no conversion needed
   = hint: just use the list directly: [1, 2, 3]
```

診断文訳：flow.collect は Flow[T] を要求するが List[T] を受け取った（wrong.almd:2:6）。List は既に実体化済みで変換不要なので [1, 2, 3] をそのまま使う。

---

<a id="common-patterns"></a>

## よくあるパターン

<a id="pattern-1-ログ解析-flow-の代表例"></a>

### Pattern 1: ログ解析 (Flow の代表例)

```almide
import fs

effect fn count_errors(path: String) -> Result[Int, String] = {
  let n = file.lines(path)!
    |> flow.filter((line) => string.contains(line, "ERROR"))
    |> flow.fold(0, (acc, _) => acc + 1)
  ok(n)
}
```

**特性**: メモリ O(1)、10GB ファイルも可能、ハンドルは関数を抜けた瞬間 close。

<a id="pattern-2-csv--json-lines-変換"></a>

### Pattern 2: CSV → JSON Lines 変換

```almide
effect fn csv_to_jsonl(input: String, output: String) -> Result[Unit, String] = {
  file.lines(input)!
    |> flow.drop(1)                                        // ヘッダー除く
    |> flow.map((line) => row_to_json(string.split(line, ",")))
    |> flow.each((j) => fs.append_text(output, json.stringify(j) + "\n")!)
  ok(())
}
```

**特性**: 1 行ずつ変換して出力。メモリ O(1)。

<a id="pattern-3-並列ダウンロード-with-streaming-phase-2-以降"></a>

### Pattern 3: 並列ダウンロード with streaming (Phase 2 以降)

```almide
effect fn fetch_all_urls(url_file: String) -> Result[Unit, String] = {
  file.lines(url_file)!
    |> flow.filter((url) => string.starts_with(url, "https://"))
    |> fan.map(limit: 10, (url) => http.get(url)!)         // Flow → Flow
    |> flow.each((body) => write_to_disk(body)!)
  ok(())
}
```

**特性**: 最大 10 並列、upstream が自動バックプレッシャー、100 万 URL でもメモリは並列数分のみ。

<a id="pattern-4-無限ソース--有限化"></a>

### Pattern 4: 無限ソース + 有限化

```almide
fn primes_under(limit: Int) -> List[Int] = 
  flow.generate(2, (n) => some((n, n + 1)))
    |> flow.filter(is_prime)
    |> flow.take(100)                                      // 100 個で止める
    |> flow.filter((p) => p < limit)
    |> flow.collect()
```

**特性**: `generate` で無限列を作っても `take` で有限化されれば安全。

<a id="pattern-5-list-と-flow-の混在"></a>

### Pattern 5: List と Flow の混在

```almide
// 許可ユーザー (List、小さい)
let allowed_users: List[String] = ["alice", "bob"]

// ログ (Flow、巨大)
effect fn filter_user_logs(log_path: String) -> Result[Int, String] = {
  let n = file.lines(log_path)!
    |> flow.filter_map((line) => {
        let j = json.parse(line)?
        let user = json.get_string(j, "user")?
        if list.contains(allowed_users, user) then some(line) else none
      })
    |> flow.fold(0, (acc, _) => acc + 1)
  ok(n)
}
```

**特性**: `list.contains` は List (allowed_users) に対して呼べる。`flow.filter_map` は Flow (lines) に対して呼べる。**型ごとに自然に使い分け**。

<a id="pattern-6-早期検索-find"></a>

### Pattern 6: 早期検索 (find)

```almide
effect fn first_line_with_error(path: String) -> Result[Option[String], String] = {
  let found = file.lines(path)!
    |> flow.find((line) => string.contains(line, "ERROR"))
  ok(found)
}
```

**特性**: ERROR を含む最初の行で止まる。ファイル全体は読まない。**short-circuit** がキモ。

---

<a id="cost-model"></a>

## コストモデル

Flow と List の性能特性をユーザーが把握できるようにする。

| 操作 | List[T] | Flow[T] |
|---|---|---|
| 作成 | O(n) メモリ | O(1) メモリ |
| `map`/`filter` | O(n) メモリ確保 (現状) | O(1) (チェーン構築のみ) |
| `fold` | O(n) 時間 | O(n) 時間 |
| `len`/`get` | O(1) | **コンパイルエラー** |
| `collect` | — | O(n) メモリ (materialize) |
| メモリピーク | O(n) 常時 | O(1) (fold/each) / O(n) (collect) |

**使い分けのガイドライン**:

| シチュエーション | 選択 | 理由 |
|---|---|---|
| 要素数 < 1000 | List | オーバーヘッドなし、ランダムアクセス OK |
| 要素数 > 10万 | Flow | メモリ O(1) |
| 要素数不明 (I/O 由来) | Flow | 安全側 |
| 無限かもしれない | Flow | 必須 |
| 複数回走査したい | List | single-pass 制約なし |
| ランダムアクセスしたい | List | Flow は forward-only |
| 副作用を pipeline 中で起こしたい | Flow | lazy 評価でタイミング明確 |

---

<a id="fan--flow-integration"></a>

## fan × Flow の統合

Flow と fan は **直交しつつ統合される**。詳細は `fan-concurrency-next.md` （tree から削除済み — `git log --diff-filter=D -- docs/roadmap/done/fan-concurrency-next.md`） に委譲。以下は interaction の 5 原則。

<a id="原則-1-fanmap-は入力型で戻り型がディスパッチされる"></a>

### 原則 1: `fan.map` は入力型で戻り型がディスパッチされる

```almide
// List 入力 → List 出力 (バッチ並行)
fan.map(xs: List[T], limit: Int?, f: (T) -> U) -> List[U]

// Flow 入力 → Flow 出力 (ストリーミング並行)
fan.map(xs: Flow[T], limit: Int?, f: (T) -> U) -> Flow[U]
```

戻り型が入力型に追従するので、`file.lines |> fan.map(limit: 10, f) |> flow.filter(p) |> flow.collect()` が自然に書ける。

```almide
// List
[1, 2, 3] |> fan.map(process) |> list.len           // List[U] → Int

// Flow
file.lines(path)! |> fan.map(limit: 10, process) |> flow.fold(0, ...)   // Flow[U] → Int
```

<a id="原則-2-flow--limit-で自動バックプレッシャー"></a>

### 原則 2: Flow + `limit:` で自動バックプレッシャー

`limit` は「最大同時ワーカー数」。Flow に使うと、**ワーカーが空くまで上流から pull しない**。

```almide
file.lines("huge.log")!                  // 100 GB
  |> fan.map(limit: 10, heavy_process)   // 最大 10 並列
  |> flow.each(store)                    // 消費
// メモリピーク: 並列数 10 + buffer 分のみ、ファイルサイズに関係ない
```

<a id="原則-3-limit-省略時の挙動"></a>

### 原則 3: `limit:` 省略時の挙動

| 入力型 | `limit:` 省略時 |
|---|---|
| `List[T]` | 要素数分の並列 (上限なし) |
| `Flow[T]` | **コンパイラ警告 W006** (unbounded parallel on possibly-infinite source) |

<a id="原則-4-fan-スコープ終了で-flow-を-drop"></a>

### 原則 4: fan スコープ終了で Flow を drop

```almide
fan(timeout: 30000) {
  file.lines("huge.log")!
    |> flow.filter(is_error)
    |> fan.map(limit: 10, process)
    |> flow.each(write_result)
}
// 30 秒 timeout → 全 worker cancel → Flow drop → BufReader drop → file close
```

**構造的キャンセル** が Flow にも波及。Rust `thread::scope` + `Drop` で自然に実装可能。ユーザーは cancel 処理を書かない。

<a id="原則-5-順序保証"></a>

### 原則 5: 順序保証

`fan.map(flow, limit: n, f)` の出力順は **未定義** (worker 終了順)。入力順が必要な場合は:

- Phase 2: 手動で `enumerate + sort` を書く
- 将来: `fan.ordered_map(flow, limit: n, f)` を追加検討

Phase 2 はまず unordered で実装。

---

<a id="implementation-phases"></a>

## 実装段階

<a id="phase-1-core-flow-type--12-api-目標"></a>

### Phase 1：核となる Flow 型 + 12 API（目標）

**成果物**: Flow 単体で ログ解析・CSV 変換・無限列処理ができる。fan との統合はまだ。

- [ ] `Ty::Flow(Box<Ty>)` を checker に追加 (`crates/almide-types`)
- [ ] Flow の move-only 制約を use-count 解析に追加 (D3)
- [ ] `stdlib/flow.almd` に提案 API を定義（実装時に現行 stdlib 登録手順を確認）
- [ ] `file.lines` の runtime 実装 (`runtime/rs/src/file.rs`)
- [ ] Forbidden ops のコンパイル時エラー (FLOW_FORBIDDEN_OP) + hint 2 本立て
- [ ] FLOW_CONSUMED (move after consume), FLOW_COLLECT_LIST (collect on List), W005 (unbounded collect)
- [ ] Rust codegen: `Flow[T]` → `Box<dyn Iterator<Item = T>>` (runtime 埋め込み)
- [ ] WASM codegen: Phase 1 は無効化 (型エラーで拒否、Phase 4 で対応)
- [ ] `spec/lang/flow_test.almd` — 12 関数の単体テスト
- [ ] `spec/lang/flow_error_test.almd` — FLOW_FORBIDDEN_OP/FLOW_CONSUMED/FLOW_COLLECT_LIST/W005 の expect-fail テスト
- [ ] `docs/specs/flow.md` — ユーザー向け詳細仕様 (この roadmap の詳細版)
- [ ] cheatsheet に Flow セクション追加 + terminal op の 🔴 マーク

**完了条件**: Pattern 1-6 (fan 除く) が全部動く。

<a id="phase-2-fan--flow-統合"></a>

### Phase 2: fan × Flow 統合

**成果物**: `fan.map` が Flow と相互作用する。並列ストリーミング、自動バックプレッシャー、構造的 cancel が動く。

- [ ] `fan.map(xs: Flow[T], limit: Int?, f)` の実装
- [ ] Rust codegen: `thread::scope` + bounded channel で pull-based worker pool
- [ ] W006 (unbounded parallel on Flow) の warning
- [ ] 構造的 cancel: fan scope 抜けで Flow drop、file handle close
- [ ] `spec/integration/flow_fan_test.almd` — streaming × 並行のテスト

**完了条件**: Pattern 3 (並列ダウンロード with streaming) が動く。10GB データ + 10 並列でメモリピークが並列数分のみに収まる。

<a id="phase-3-polish--extension"></a>

### Phase 3：改善と拡張

demand ベースで追加:

- [ ] `flow.take_while(pred)` / `flow.drop_while(pred)`
- [ ] `flow.enumerate` — index 付き
- [ ] `flow.zip(other)` — 2 Flow を lockstep
- [ ] `flow.chain(other)` — 2 Flow を連結
- [ ] `flow.inspect(f)` — デバッグ用 passthrough
- [ ] `flow.empty()` (もし `from_list([])` より欲しければ)
- [ ] `flow.scan(init, step)` — 中間状態を出力
- [ ] `file.lines_checked` — per-line エラー版 (堅牢性が必要な場合)
- [ ] `flow { emit(...) }` builder 構文の要否判断

<a id="phase-4-async--wasm-backend"></a>

### Phase 4：非同期 / WASM backend

`fan-concurrency-next.md` （tree から削除済み — `git log --diff-filter=D -- docs/roadmap/done/fan-concurrency-next.md`） の Phase 5-6 と並行:

- [ ] Rust async 移行時の `Flow[T]` → `Stream[T]` mapping
- [ ] WASI 0.3 の `stream<T>` への mapping
- [ ] JSPI での Flow 実装
- [ ] 機能検出

---

<a id="original-implementation-sketch--not-a-current-file-edit-checklist"></a>

## 当初の実装素案 — 現行のファイル編集一覧ではない

以下のパスは提案当初のコンパイラ配置を記録する。特に `stdlib/defs/*.toml` は既に標準ライブラリの正本ではなく、宣言は `stdlib/*.almd` にある。実装前に型・checker・lowering・backend 作業を現行 tree に対応づけ直すこと。これらのパスは Flow 実装済みを意味しない。

<a id="phase-1"></a>

### Phase 1
- `crates/almide-types/src/types/mod.rs` — `Ty::Flow`、subtype 関係なし (D3)
- `crates/almide-frontend/src/check/calls.rs` — forbidden ops 検出、FLOW_FORBIDDEN_OP/FLOW_CONSUMED/FLOW_COLLECT_LIST
- `crates/almide-frontend/src/check/lint.rs` — W005 の lint
- `crates/almide-ir/src/use_count.rs` — Flow の move-only ルール
- `crates/almide-codegen/src/walker/expressions.rs` — Flow codegen
- `runtime/rs/src/file.rs` — `file.lines` 実装 (Box<dyn Iterator>)
- `stdlib/defs/flow.toml` — **新規**、12 関数定義
- `stdlib/defs/file.toml` — `lines` 関数追加
- `build.rs` — flow module 登録
- `spec/lang/flow_test.almd` — **新規**
- `spec/lang/flow_error_test.almd` — **新規**
- `docs/specs/flow.md` — **新規**、詳細仕様
- `docs/CHEATSHEET.md` — Flow セクション追加

<a id="phase-2"></a>

### Phase 2
- `runtime/rs/src/fan.rs` — `fan_map_flow` bounded consumer
- `crates/almide-codegen/src/pass_fan_lowering.rs` — Flow 対応
- `stdlib/defs/fan.toml` — `fan.map` の Flow overload
- `spec/integration/flow_fan_test.almd` — **新規**

---

<a id="msr-benchmark-方向の実証"></a>

## MSR ベンチマーク（方向の実証）

Phase 1 実装と並行して、Flow を活かす exercise を **10 問** 追加:

1. ログファイルから ERROR 件数を集計 (`flow.filter + flow.fold`) — Pattern 1
2. CSV → JSON Lines 変換 (`flow.drop + flow.map + flow.each`) — Pattern 2
3. URL リストを並列ダウンロード (`flow + fan.map + flow.each`) — Pattern 3 (Phase 2 依存)
4. 巨大ファイルの先頭 N 行取得 (`flow.take + flow.collect`)
5. 指定パターン match 行抽出 (grep-like, `flow.filter + flow.each`)
6. 最初に match する行を取得 (`flow.find`、短絡検証)
7. 重複行除去 (`flow.fold` で accumulator に set)
8. stdin の streaming 処理 (`io.read_lines + flow.*`、将来)
9. 無限生成から先頭 N 個 (`flow.generate + flow.take + flow.collect`)
10. 複雑な pipeline (`flow.filter + flow.map + flow.filter_map + flow.fold`)

`research/benchmark/msr/exercises/flow/` に配置。LLM が `flow.*` を自然に書けるか、`list.*` との混同が起きないかを実測する。

**成功基準**: 10 問中 8 問以上を Haiku 4.5 以上のモデルが 1 shot で通せる。

---

<a id="references"></a>

## 参考

| 言語 | 学んだもの |
|---|---|
| **MoonBit `Iter[T]`** | single-pass の明示、関数間で渡す慣用 (最近い前例) |
| **Kotlin Flow** | `flow { emit(...) }` builder、suspension-based backpressure |
| **Gleam `iterator`** | List / Iterator の分離、`fold` on infinite 注意喚起 |
| **Rust `Iterator`** | Drop での resource cleanup、短絡 `find` |
| **C# LINQ** | 同じ verb の type-directed 動作 (Almide は拒否したが参考) |
| **Haskell lazy I/O** | 反面教師。`file.lines` の resource cleanup 設計で踏まない |

---

<a id="scope-boundary"></a>

## 対象範囲の境界

**やること**:
- `Flow[T]` 型 + `flow.*` 12 関数
- `file.lines` の Flow 返却
- Forbidden ops の compile-time 拒否
- Move-only 制約 (single-pass 強制)
- Drop による resource cleanup
- `fan.map` × Flow 統合 (Phase 2)

**やらないこと**:
- `list.*` と `flow.*` の名前空間共有 (却下済)
- `Seq[T]` 的な higher-kinded 抽象
- 暗黙 List → Flow 昇格
- 型注釈による暗黙 materialize
- 有限性の型レベル追跡 (D5)
- 代数的 effect handler
- ストリーム専用の独自言語構文 (`flow { emit }` は Phase 3 検討)
