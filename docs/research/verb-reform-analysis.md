# Verb Reform Analysis

Almide stdlib 355 関数の動詞一貫性分析。他言語の慣例と比較して判定。

---

## 1. Cross-Module Verbs（2+ モジュールで使用）

### 一貫している（変更不要）

| Verb | モジュール | 他言語 | 判定 |
|------|-----------|--------|------|
| `get` | list, map, json, env, http | 全言語共通 | ✅ 維持 |
| `set` | list, map, json, env, http | 全言語共通 | ✅ 維持 |
| `contains` | list, map, string | Rust `.contains()`, Gleam `contains` | ✅ 維持 |
| `len` | list, map, string | Go `len()`, Rust `.len()` | ✅ 維持 |
| `is_empty` | list, map, string | Rust `.is_empty()`, Kotlin `.isEmpty()` | ✅ 維持 |
| `map` | list, result | Rust/Gleam/Elm/Kotlin 全て `map` | ✅ 維持 |
| `filter` | list, map | 全言語共通 | ✅ 維持 |
| `min` / `max` | list, int, float, math | 全言語共通 | ✅ 維持 |
| `abs` | int, float, math | 全言語共通 | ✅ 維持 |
| `reverse` | list, string | 全言語共通 | ✅ 維持 |
| `slice` | list, string | JS `.slice()`, Go `[a:b]` | ✅ 維持 |
| `repeat` | list, string | Rust `.repeat()`, Kotlin `.repeat()` | ✅ 維持 |
| `replace` | string, regex | 全言語共通 | ✅ 維持 |
| `split` | string, regex | 全言語共通 | ✅ 維持 |
| `join` | list, string | 全言語共通 | ✅ 維持 |
| `sort` | list | 全言語共通 | ✅ 維持 |
| `find` | list, regex | 全言語共通 | ✅ 維持 |
| `is_*` | 9 モジュール | Rust `is_ok()`, Kotlin `isEmpty()` | ✅ 維持 |

### 不一致がある（要検討）

| 問題 | 現在 | 提案 | 根拠 |
|------|------|------|------|
| `parse` vs `from_string` | `int.parse`, `float.parse`, `json.parse` | **`parse` を正式採用** | Rust `str::parse`, Gleam `int.parse`, Go `strconv.Parse*`, JS `JSON.parse` — 全言語で `parse` |
| `and_then` vs `flat_map` | `result.and_then` のみ | **`flat_map` エイリアス追加** | Rust は `and_then`, Kotlin/Scala は `flatMap`, Gleam は `try_map`。`flat_map` は list.flat_map と対称 |
| `map_values` vs `map` | `map.map_values` | 将来 `map.map` 追加検討 | Kotlin `mapValues`, Rust `iter().map()` |
| `parse_hex` vs `from_hex` | `int.parse_hex` | **`from_hex` エイリアス追加** | `from_*` prefix が変換系で一貫 (`from_int`, `from_string`, `from_bytes`) |
| `char_at` vs `get` | `string.char_at` | 将来 `string.get` 追加検討 | `list.get(xs, i)` との対称性 |

---

## 2. 冗長な重複

| 関数 A | 関数 B | 判定 |
|--------|--------|------|
| `string.to_int(s)` | `int.parse(s)` | **string.to_int を deprecate** — 変換先モジュールに配置が自然 |
| `string.to_float(s)` | `float.parse(s)` | **string.to_float を deprecate** — 同上 |
| `string.char_count(s)` | `string.len(s)` | **char_count を deprecate** — 同じ実装 (chars().count()) |
| `map.from_entries(es)` | `map.from_list(es)` | 両方ある。`from_list` が Almide の命名規則に合う |

---

## 3. `to_` / `from_` の一貫性

### 現在の `to_` 系 (出力型を示す)

| 関数 | 方向 | 判定 |
|------|------|------|
| `int.to_string(n)` | Int → String | ✅ 正しい |
| `int.to_float(n)` | Int → Float | ✅ 正しい |
| `int.to_hex(n)` | Int → String (hex) | ✅ 正しい |
| `float.to_string(n)` | Float → String | ✅ 正しい |
| `float.to_int(n)` | Float → Int | ✅ 正しい |
| `float.to_fixed(n, d)` | Float → String | ✅ 正しい |
| `string.to_upper(s)` | String → String | ✅ 正しい (ケース変換) |
| `string.to_lower(s)` | String → String | ✅ 正しい |
| `string.to_bytes(s)` | String → List[Int] | ✅ 正しい |
| `string.to_int(s)` | String → Int | ⚠️ **冗長** — `int.parse(s)` がある |
| `string.to_float(s)` | String → Float | ⚠️ **冗長** — `float.parse(s)` がある |
| `datetime.to_iso(d)` | DateTime → String | ✅ 正しい |
| `datetime.to_unix(d)` | DateTime → Int | ✅ 正しい |
| `result.to_option(r)` | Result → Option | ✅ 正しい |

### 現在の `from_` 系 (入力型を示す)

| 関数 | 方向 | 判定 |
|------|------|------|
| `float.from_int(n)` | Int → Float | ✅ 正しい |
| `string.from_bytes(bs)` | List[Int] → String | ✅ 正しい |
| `string.from_codepoint(n)` | Int → String | ✅ 正しい |
| `datetime.from_parts(...)` | Parts → DateTime | ✅ 正しい |
| `datetime.from_unix(n)` | Int → DateTime | ✅ 正しい |
| `map.from_entries(es)` | List[(K,V)] → Map | ✅ 正しい |
| `map.from_list(es)` | List[(K,V)] → Map | ⚠️ `from_entries` と重複 |
| `json.from_*` | 各型 → Json | ✅ 正しい |

### `parse` 系 (文字列パース)

| 関数 | 方向 | 他言語 | 判定 |
|------|------|--------|------|
| `int.parse(s)` | String → Result[Int, String] | Rust, Gleam, Go | ✅ 維持 |
| `float.parse(s)` | String → Result[Float, String] | Rust, Go | ✅ 維持 |
| `json.parse(s)` | String → Result[Value, String] | **全言語** `JSON.parse` | ✅ 維持 |
| `int.parse_hex(s)` | String → Result[Int, String] | — | `from_hex` の方が `from_*` と一貫 |
| `datetime.parse_iso(s)` | String → Result[DateTime, String] | — | ✅ 維持 (`parse_iso` は十分明確) |
| `uuid.parse(s)` | String → Result[String, String] | — | ✅ 維持 |

---

## 4. 他言語との比較

### parse vs from_string

| 言語 | Int パース | JSON パース |
|------|-----------|------------|
| Rust | `str::parse::<i32>()`, `i32::from_str()` | `serde_json::from_str()` |
| Go | `strconv.ParseInt()`, `strconv.Atoi()` | `json.Unmarshal()` |
| Python | `int("42")` | `json.loads()` |
| Kotlin | `"42".toInt()` | `Gson().fromJson()` |
| Swift | `Int("42")` | `JSONDecoder().decode()` |
| Gleam | `int.parse("42")` | `json.decode()` |
| TypeScript | `parseInt("42")` | `JSON.parse()` |
| **Almide** | **`int.parse("42")`** | **`json.parse(s)`** |

**結論**: `parse` は全言語で広く使われる動詞。`from_string` への変更は不要。

### flat_map vs and_then

| 言語 | 名前 |
|------|------|
| Rust | `and_then` (Option/Result), `flat_map` (Iterator) |
| Kotlin | `flatMap` |
| Scala | `flatMap` |
| Swift | `flatMap` |
| Gleam | `try` (deprecated), `try_map` |
| Haskell | `>>=` (bind) |
| **Almide** | `and_then` (現在) → **`flat_map` エイリアス追加** |

**結論**: `flat_map` は list.flat_map と対称。`and_then` も維持（Rust ユーザー向け）。

---

## 5. アクションプラン

### 即実行 (1.0 前)

| # | 変更 | 種類 | 影響 |
|---|------|------|------|
| 1 | `result.flat_map` 追加 | エイリアス追加 | 非破壊 |
| 2 | `int.from_hex` 追加 | エイリアス追加 | 非破壊 |
| 3 | `string.to_int` に deprecation 注記 | ドキュメント | 非破壊 |
| 4 | `string.to_float` に deprecation 注記 | ドキュメント | 非破壊 |
| 5 | `string.char_count` に deprecation 注記 | ドキュメント | 非破壊 |

### 1.x で段階的に

| # | 変更 | 種類 |
|---|------|------|
| 6 | `string.get(s, i)` 追加 (`char_at` のエイリアス) | エイリアス追加 |
| 7 | `map.map(m, f)` 追加 (`map_values` のエイリアス) | エイリアス追加 |
| 8 | `option` モジュール新設 (map, flat_map, unwrap_or, is_some, is_none) | 新規追加 |
| 9 | Map の動詞追加 (fold, any, all, count, each, partition) | 新規追加 |
| 10 | String スライス動詞 (first, last, take, drop, take_end, drop_end) | 新規追加 |

### 変更しないもの

| 関数 | 理由 |
|------|------|
| `int.parse` | 全言語で `parse` が標準。`from_string` にしない |
| `json.parse` | `JSON.parse` は業界標準 |
| `result.and_then` | `flat_map` を追加するが `and_then` は削除しない |
| `map.from_entries` | `from_list` と共存。どちらも有効 |

---

## 6. 凍結する動詞セット

1.0 で以下の動詞体系を凍結:

| カテゴリ | 動詞 |
|---------|------|
| Transform | `map`, `flat_map`, `filter`, `filter_map`, `flatten` |
| Aggregate | `fold`, `reduce`, `scan`, `sum`, `product`, `min`, `max`, `count` |
| Test | `any`, `all`, `contains`, `is_empty`, `is_*` |
| Access | `get`, `first`, `last`, `find`, `find_index`, `index_of`, `len` |
| Slice | `take`, `drop`, `slice`, `take_while`, `drop_while` |
| Order | `sort`, `sort_by`, `reverse` |
| Combine | `++`, `join`, `intersperse`, `merge` |
| Deduplicate | `unique`, `dedup` |
| Side Effect | `each` |
| Convert | `to_*`, `from_*`, `parse` |
