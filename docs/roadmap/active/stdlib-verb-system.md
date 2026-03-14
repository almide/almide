# Stdlib Verb System [ACTIVE]

## Summary

stdlib の関数名を「動詞の体系」として再設計する。全コンテナ型で同じ動詞が同じ意味を持ち、LLM が1つの動詞を学べば全型に適用できる状態を作る。

## Motivation

### 現状の問題

| 問題 | 例 |
|---|---|
| 同じ概念に違う名前 | `contains` (list) vs `contains?` (string) |
| 同じ名前で違う意味 | `count` — list: 述語で数える / string: 部分文字列の出現数 |
| 冗長な重複 | `string.to_int()` と `int.parse()` が両方存在 |
| List 偏重 | 高階関数 14 個、Map は 1 個、Result は 2 個 |
| 引数順バラバラ | `string.join(list, sep)` — 主語が list か string か不明瞭 |
| 述語 suffix 不統一 | `is_empty?` (? あり) vs `contains` (? なし) |
| Map の動詞不足 | `map_values` だけで `map`, `fold`, `any`, `all` がない |

### LLM にとって理想の stdlib とは

1. **推測可能** — `list.map(xs, f)` を知っていれば `result.map(r, f)` を推測できる
2. **1つの正解** — 同じことをする2つの関数がない
3. **型が語る** — 型シグネチャがドキュメント
4. **動詞が体系的** — 同じ動詞セットが全コンテナ型で同じ意味

### 調査した言語

| 言語 | 引数順 | 特徴 |
|---|---|---|
| Elm | data-last | `map`, `foldl`, `andThen` が全型横断。最小 API |
| Gleam | data-first | `try_*` 変種で Result 連携。labeled arguments |
| Kotlin | method syntax | `*OrNull`, `*By`, `*To` の suffix 体系。最大 API |
| Swift | method syntax | protocol 階層で動詞の一貫性を型システムが保証 |
| Clojure | data-last | seq 抽象で全型統一。「少数の抽象に多数の関数」 |

**全言語で一致する核心**: `map` が collection と wrapper の橋。`fold`/`reduce` が万能集約。`flatMap`/`andThen` がモナディック連鎖。

---

## Design Principles

### 1. `map` は橋

`map` が List/Map/Option/Result 全てで使える。LLM は「map できるものには map がある」と1回覚えればいい。

```almide
[1, 2, 3].map((x) => x * 2)           // List[Int]
scores.map((k, v) => (k, v + 10))     // Map[String, Int]
some(42).map((x) => x.to_string())     // Option[String]
ok(42).map((x) => x * 2)              // Result[Int, E]
```

### 2. 引数順は data-first

UFCS と `|>` パイプの両方で自然に使える。

```almide
// 全て同じ呼び出し
xs.map(f)
list.map(xs, f)
xs |> list.map(f)
```

### 3. 述語は `?` なし

戻り値が `Bool` なら述語であることは型から自明。suffix による区別は不要。

```almide
// Good: 型が語る
fn contains[A](xs: List[A], value: A) -> Bool
fn is_empty[A](xs: List[A]) -> Bool
fn any[A](xs: List[A], f: Fn(A) -> Bool) -> Bool

// Bad: ? suffix は冗長
fn contains?[A](xs: List[A], value: A) -> Bool
```

### 4. 変換は `to_`、構築は `from_`

方向が明確。`parse` は廃止し `from_string` に統一。

```almide
42.to_string()          // Int → String
42.to_float()           // Int → Float
int.from_string("42")   // String → Int (Result)
string.from_chars(cs)   // List[String] → String
```

### 5. 1つの正解

同じことをする2つの関数は作らない。

```
廃止: int.parse("42")     → 採用: int.from_string("42")
廃止: string.to_int("42") → 採用: int.from_string("42")
廃止: map.map_values(m,f) → 採用: map.map(m, f)
廃止: result.and_then(r,f) → 採用: result.flat_map(r, f)
```

### 6. 方向は `_start` / `_end`

left/right ではなく start/end で統一（Gleam 方式）。

```almide
"hello".trim_start()
"hello".trim_end()
[1,2,3].take(2)       // 先頭から（デフォルト）
[1,2,3].take_end(2)   // 末尾から
```

---

## Verb Taxonomy

### Category 1: Transform（変換）

全コンテナ型で使う最重要動詞群。

| Verb | List | Map | Option | Result | 意味 |
|---|---|---|---|---|---|
| `map` | ✅ | ✅ | ✅ | ✅ | 各要素/中身を変換 |
| `flat_map` | ✅ | ✅ | ✅ | ✅ | map → flatten |
| `filter` | ✅ | ✅ | - | - | 述語に合う要素を残す |
| `filter_map` | ✅ | ✅ | - | - | map → None 除去 |
| `flatten` | ✅ | - | ✅ | ✅ | ネスト解除 |

### Category 2: Aggregate（集約）

| Verb | List | Map | 意味 |
|---|---|---|---|
| `fold` | ✅ | ✅ | 初期値あり累積 |
| `reduce` | ✅ | - | 初期値なし累積（先頭が初期値） |
| `scan` | ✅ | - | fold の中間結果を全て返す |
| `sum` | ✅ | - | 数値リストの合計 |
| `product` | ✅ | - | 数値リストの積 |
| `min` | ✅ | - | 最小値 |
| `max` | ✅ | - | 最大値 |
| `count` | ✅ | ✅ | 述語に合う要素数 |

### Category 3: Test（検査）

| Verb | List | Map | String | Option | Result | 意味 |
|---|---|---|---|---|---|---|
| `any` | ✅ | ✅ | - | - | - | 1つでも合うか |
| `all` | ✅ | ✅ | - | - | - | 全て合うか |
| `contains` | ✅ | ✅ | ✅ | - | - | 値が含まれるか |
| `is_empty` | ✅ | ✅ | ✅ | ✅ | - | 空か |

### Category 4: Access（取得）

| Verb | List | Map | String | 意味 |
|---|---|---|---|---|
| `get` | ✅ | ✅ | ✅ | インデックス/キーで取得 (Option) |
| `first` | ✅ | - | ✅ | 先頭要素 (Option) |
| `last` | ✅ | - | ✅ | 末尾要素 (Option) |
| `find` | ✅ | ✅ | - | 述語で最初の一致を取得 (Option) |
| `find_index` | ✅ | - | - | 述語で最初の一致位置 (Option) |
| `index_of` | ✅ | - | ✅ | 値/部分文字列の位置 (Option) |
| `len` | ✅ | ✅ | ✅ | 要素数/文字数 |

### Category 5: Slice（切出）

| Verb | List | String | 意味 |
|---|---|---|---|
| `take` | ✅ | ✅ | 先頭 N 個 |
| `take_end` | ✅ | ✅ | 末尾 N 個 |
| `take_while` | ✅ | - | 述語が真の間 |
| `drop` | ✅ | ✅ | 先頭 N 個除去 |
| `drop_end` | ✅ | ✅ | 末尾 N 個除去 |
| `drop_while` | ✅ | - | 述語が真の間除去 |
| `slice` | ✅ | ✅ | 範囲指定 |

### Category 6: Order（順序）

| Verb | List | 意味 |
|---|---|---|
| `sort` | ✅ | 自然順ソート |
| `sort_by` | ✅ | キー関数でソート |
| `reverse` | ✅ | 逆順 |
| `shuffle` | ✅ | ランダム順 |

### Category 7: Decompose（分解）

| Verb | List | Map | 意味 |
|---|---|---|---|
| `partition` | ✅ | ✅ | 述語で2グループに分割 |
| `group_by` | ✅ | - | キー関数でグループ化 → Map |
| `chunk` | ✅ | - | N 個ずつに分割 |
| `window` | ✅ | - | スライディングウィンドウ |
| `zip` | ✅ | - | 2リストをペアに |
| `unzip` | ✅ | - | ペアリストを2リストに |
| `enumerate` | ✅ | - | インデックス付与 |

### Category 8: Combine（結合）

| Verb | List | String | 意味 |
|---|---|---|---|
| `++` | ✅ | ✅ | 連結（演算子） |
| `join` | ✅ | - | セパレータで結合 → String |
| `intersperse` | ✅ | - | 要素間にセパレータ挿入 |
| `merge` | - (Map) | - | 2つの Map を統合 |

### Category 9: Deduplicate（重複除去）

| Verb | List | 意味 |
|---|---|---|
| `unique` | ✅ | 重複除去（順序保持） |
| `unique_by` | ✅ | キー関数で重複判定 |
| `dedup` | ✅ | 連続重複のみ除去 |

### Category 10: Side Effect（副作用）

| Verb | List | Map | 意味 |
|---|---|---|---|
| `each` | ✅ | ✅ | 各要素に副作用を実行 |

### Category 11: Wrapper（Option/Result 専用）

| Verb | Option | Result | 意味 |
|---|---|---|---|
| `map` | ✅ | ✅ | 中身を変換 |
| `flat_map` | ✅ | ✅ | chain（= and_then） |
| `flatten` | ✅ | ✅ | ネスト解除 |
| `unwrap_or` | ✅ | ✅ | デフォルト値で取り出す |
| `unwrap_or_else` | ✅ | ✅ | 関数でデフォルト値を計算 |
| `map_err` | - | ✅ | エラー側を変換 |
| `is_some` | ✅ | - | Some か |
| `is_none` | ✅ | - | None か |
| `is_ok` | - | ✅ | Ok か |
| `is_err` | - | ✅ | Err か |
| `to_option` | - | ✅ | Result → Option（エラーを捨てる） |
| `to_result` | ✅ | - | Option → Result（None をエラーに） |

### Category 12: Convert（型変換）

一貫した命名規則: `to_` で出力型、`from_` で入力型を示す。

```
Int:    to_string, to_float, to_hex, from_string
Float:  to_string, to_int, from_string
String: to_bytes, from_chars, from_bytes
Map:    to_list, from_list
List:   to_map (ペアリストから)
```

### Category 13: String 専用

| Verb | 意味 |
|---|---|
| `trim`, `trim_start`, `trim_end` | 空白除去 |
| `pad_start`, `pad_end` | パディング |
| `to_upper`, `to_lower`, `capitalize` | ケース変換 |
| `starts_with`, `ends_with` | 前方/後方一致 |
| `strip_prefix`, `strip_suffix` | 前方/後方除去 |
| `replace`, `replace_first` | 置換 |
| `split` | 分割 → List[String] |
| `chars` | 文字リストに分解 |
| `lines` | 行リストに分解 |
| `repeat` | N 回繰り返し |

---

## Breaking Changes

### 廃止する関数

| 現在 | 移行先 | 理由 |
|---|---|---|
| `int.parse(s)` | `int.from_string(s)` | `from_` に統一 |
| `float.parse(s)` | `float.from_string(s)` | `from_` に統一 |
| `string.to_int(s)` | `int.from_string(s)` | 変換元に配置 |
| `string.to_float(s)` | `float.from_string(s)` | 変換元に配置 |
| `map.map_values(m,f)` | `map.map(m, f)` | 統一 |
| `result.and_then(r,f)` | `result.flat_map(r, f)` | 統一 |
| `map.from_entries(es)` | `map.from_list(es)` | 統一 |
| `string.char_at(s,i)` | `string.get(s, i)` | `get` に統一 |
| `string.char_count(s)` | `string.len(s)` | `len` に統一 |
| 全 `?` suffix 関数 | `?` なし版 | 述語 suffix 廃止 |

### リネームする関数

| 現在 | 新名 | 理由 |
|---|---|---|
| `list.remove_at(xs, i)` | `list.remove(xs, i)` | List はインデックスアクセス |
| `int.parse_hex(s)` | `int.from_hex(s)` | `from_` に統一 |
| `string.from_codepoint(n)` | `string.from_char_code(n)` | 明確化 |

### 追加する関数

| モジュール | 関数 | 理由 |
|---|---|---|
| map | `map`, `flat_map`, `fold`, `any`, `all`, `count`, `each`, `partition` | コンテナ動詞の統一 |
| string | `first`, `last`, `take`, `take_end`, `drop`, `drop_end` | スライス動詞の統一 |
| option | `map`, `flat_map`, `flatten`, `unwrap_or`, `unwrap_or_else`, `is_some`, `is_none`, `is_empty`, `to_result` | Wrapper 動詞の完備 |
| result | `flat_map`, `flatten` | Wrapper 動詞の完備 |
| list | `unique_by`, `shuffle`, `window`, `take_end`, `drop_end` | 欠落の補完 |

---

## Map の動詞設計

Map は `(key, value)` ペアのコンテナとして動詞を適用する:

```almide
let scores = {"alice": 90, "bob": 75, "carol": 88}

// map: (K, V) → (K, V2)
scores.map((k, v) => (k, v + 10))

// filter: (K, V) → Bool
scores.filter((k, v) => v >= 80)

// fold: (Acc, K, V) → Acc
scores.fold(0, (acc, k, v) => acc + v)

// any/all: (K, V) → Bool
scores.any((k, v) => v >= 90)

// each: (K, V) → Unit
scores.each((k, v) => println("{k}: {v}"))

// 既存の keys/values/entries は維持
scores.keys()      // List[String]
scores.values()    // List[Int]
scores.entries()   // List[(String, Int)]
```

---

## Implementation Order

### Phase 1: 命名統一（破壊的変更）
- `?` suffix 全廃止
- `parse` → `from_string` 統一
- `char_at` → `get` 統一
- `and_then` → `flat_map` 統一
- `map_values` → `map` 統一
- 旧名はエラーメッセージで移行先を提示

### Phase 2: Map の動詞追加
- `map.map`, `map.flat_map`, `map.fold`, `map.any`, `map.all`, `map.count`, `map.each`, `map.partition`
- Map を「コンテナとして一人前」にする

### Phase 3: String のスライス動詞追加
- `string.first`, `string.last`, `string.take`, `string.take_end`, `string.drop`, `string.drop_end`
- String と List のスライス操作を対称にする

### Phase 4: Option モジュール新設
- 現在 Result に混在している Option 操作を独立モジュールに
- `option.map`, `option.flat_map`, `option.unwrap_or`, `option.is_some`, `option.is_none`, `option.to_result`
- Wrapper 動詞の完備

### Phase 5: List の欠落補完
- `unique_by`, `shuffle`, `window`, `take_end`, `drop_end`

---

## Success Criteria

- 全コンテナ型（List, Map, Option, Result）で `map` が使える
- 全コンテナ型で `flat_map` が使える
- 全コレクション型（List, Map）で `filter`, `fold`, `any`, `all`, `each` が使える
- 述語に `?` suffix が1つもない
- 同じことをする関数が2つない
- LLM が1つの動詞を学べば全型に適用できる

## Dependencies

- [Stdlib Self-Hosted Redesign](stdlib-self-hosted-redesign.md) — self-host 移行と並行して実施可能

## Files
```
stdlib/defs/*.toml (Phase 1-3: rename/add within TOML)
stdlib/*.almd (Phase 4-5: new modules, or self-hosted rewrites)
src/stdlib.rs (UFCS mappings update)
```
