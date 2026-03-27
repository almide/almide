<!-- description: Unified verb system across all stdlib container types -->
<!-- done: 2026-03-18 -->
# Stdlib API Surface Reform

## Vision

stdlib の全コンテナ型で同じ動詞が同じ意味を持ち、LLM が1つの動詞を学べば全型に適用できる状態を作る。

---

## Design Principles

1. **`map` は橋** — List/Map/Option/Result 全てで使える
2. **引数順は data-first** — UFCS と `|>` パイプの両方で自然
3. **述語は `?` なし** — 戻り値が Bool なら型から自明
4. **変換は `to_`、構築は `from_`** — `parse` は `from_string` に統一
5. **1つの正解** — 同じことをする2つの関数は作らない
6. **方向は `_start` / `_end`** — left/right ではなく start/end

---

## Implementation Status

### ✅ Step 1: `?` suffix 全廃止 (完了)

24 関数のリネーム。全て適用済み、CI green。

### ✅ Step 2: 冗長な重複を統一 (大部分完了)

| 変更 | 状態 |
|---|---|
| `and_then` → `flat_map` | ✅ 削除済 |
| `map_values` → `map.map` | ✅ 統一済 |
| `char_at` → `string.get` | ✅ 統一済 |
| `string.to_int` → 削除 | ✅ 削除済 |
| `string.to_float` → 削除 | ✅ 削除済 |
| `int.parse` → `int.from_string` | 🔲 新名追加が必要 |
| `float.parse` → `float.from_string` | 🔲 新名追加が必要 |
| `uuid.parse` → `uuid.from_string` | 🔲 新名追加が必要 |

Note: `json.parse` は業界標準のため維持（`json.from_string` も別途存在）。

### 🔲 Step 3: Map の動詞追加

Map を「コンテナとして一人前」にする。

| 関数 | 状態 | 説明 |
|---|---|---|
| `map.map` | ✅ | 各(K,V)ペアを変換 |
| `map.filter` | ✅ | 述語に合うペアを残す |
| `map.fold` | 🔲 | 初期値あり累積 |
| `map.any` | 🔲 | 1つでも合うか |
| `map.all` | 🔲 | 全て合うか |
| `map.count` | 🔲 | 述語に合う要素数 |
| `map.each` | 🔲 | 各ペアに副作用 |
| `map.find` | 🔲 | 述語で最初の一致 |
| `map.update` | 🔲 | キーの値を関数で更新 |

### 🔲 Step 4: String のスライス動詞追加

String と List のスライス操作を対称にする。

| 関数 | 状態 |
|---|---|
| `string.first` | 🔲 |
| `string.last` | 🔲 |
| `string.take` | 🔲 |
| `string.take_end` | 🔲 |
| `string.drop` | 🔲 |
| `string.drop_end` | 🔲 |

### 🔲 Step 5: Option モジュール新設

現在 Option 操作は分散している。独立モジュールに集約。

| 関数 | 状態 |
|---|---|
| `option.map` | 🔲 |
| `option.flat_map` | 🔲 |
| `option.flatten` | 🔲 |
| `option.unwrap_or` | 🔲 |
| `option.unwrap_or_else` | 🔲 |
| `option.is_some` | 🔲 |
| `option.is_none` | 🔲 |
| `option.to_result` | 🔲 |
| `option.filter` | 🔲 |
| `option.zip` | 🔲 |
| `option.or_else` | 🔲 |

### 🔲 Step 6: List の欠落補完

| 関数 | 状態 |
|---|---|
| `list.unique_by` | 🔲 |
| `list.shuffle` | 🔲 |
| `list.window` | 🔲 |
| `list.take_end` | 🔲 |
| `list.drop_end` | 🔲 |

### ✅ Step 7: 旧名削除 (完了)

- `map.from_entries` 削除 → `map.from_list` に統一 (spec準拠)
- `parse` は維持 (spec原則3: `parse` = fallible解釈。`from_string` はスコープ外)
- `option.to_list` 追加 (spec準拠)

---

## Verb Taxonomy (設計確定)

### Transform: map, flat_map, filter, filter_map, flatten
### Aggregate: fold, reduce, scan, sum, product, min, max, count
### Test: any, all, contains, is_empty
### Access: get, first, last, find, find_index, index_of, len
### Slice: take, take_end, take_while, drop, drop_end, drop_while, slice
### Order: sort, sort_by, reverse, shuffle
### Decompose: partition, group_by, chunk, window, zip, unzip, enumerate
### Combine: ++, join, intersperse, merge
### Deduplicate: unique, unique_by, dedup
### Side Effect: each
### Wrapper (Option/Result): map, flat_map, flatten, unwrap_or, unwrap_or_else, map_err, is_some/is_none, is_ok/is_err, to_option, to_result
### Convert: to_string, to_float, to_int, to_hex, from_string, from_chars, from_bytes, from_list, to_list, to_map

---

## Success Criteria

- [x] 述語に `?` suffix が1つもない
- [x] `and_then` → `flat_map` 統一
- [x] `map_values` → `map.map` 統一
- [ ] 全コンテナ型（List, Map, Option, Result）で `map` が使える
- [ ] 全コンテナ型で `flat_map` が使える
- [ ] 全コレクション型（List, Map）で `filter`, `fold`, `any`, `all`, `each` が使える
- [ ] Option モジュールが存在する
- [ ] `parse` → `from_string` 新名追加完了

## Implementation Notes

### from_string 追加方針
TOML 関数名は変えない。新エントリ `[from_string]` を追加し、同じ runtime 関数を呼ぶ。旧名 `[parse]` も維持。arg_transforms テーブルに両方登録される。

### 作業量見積
- Step 2 残り: 3 関数追加 (TOML + runtime共有)
- Step 3: 7 関数追加 (TOML + Rust runtime + TS runtime)
- Step 4: 6 関数追加
- Step 5: 11 関数追加 (新 TOML ファイル)
- Step 6: 5 関数追加
- 合計: 32 関数追加

## Files
```
stdlib/defs/*.toml
runtime/rs/src/*.rs
runtime/ts/*.ts
src/stdlib.rs (UFCS mappings)
spec/stdlib/ (tests)
```
