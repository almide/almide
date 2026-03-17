# Stdlib 1.0 Specification — Draft

> これは Almide 1.0 codefreeze 時の stdlib のあるべき姿。
> 現状との差分ではなく、理想状態を定義する。

---

## 設計原則

1. **1 verb = 1 meaning** — 同じ動詞は全モジュールで同じ意味
2. **data-first** — 第一引数がデータ。UFCS と `|>` で自然に使える
3. **`to_*` は infallible、`parse` は fallible** — 失敗しない変換は `to_`、失敗しうる解釈は `parse`
4. **`from_*` はターゲット側構築** — `int.from_hex("ff")` のように変換先モジュールに配置
5. **`as_*` は動的型抽出** — `json.as_string(v)` のように Value からの射影
6. **`is_*` は Bool 述語** — `?` suffix なし
7. **Option と Result は独立モジュール** — wrapper 型にも map/flat_map/unwrap_or
8. **Map はコレクションとして一人前** — fold/any/all/each/count が使える

---

## Module Index

| Module | 種別 | 関数数 | effect |
|--------|------|--------|--------|
| string | core | 40 | pure |
| int | core | 17 | pure |
| float | core | 16 | pure |
| list | collection | 54 | pure |
| map | collection | 24 | pure |
| option | wrapper | 10 | pure |
| result | wrapper | 11 | pure |
| math | utility | 19 | pure |
| regex | utility | 8 | pure |
| json | data format | 32 | pure |
| value | data format | 19 | pure |
| fs | I/O | 24 | effect |
| http | I/O | 26 | effect |
| io | I/O | 3 | effect |
| process | I/O | 6 | effect |
| env | I/O | 9 | effect |
| log | I/O | 8 | effect |
| random | I/O | 4 | effect |
| crypto | I/O | 3 | effect |
| datetime | I/O | 21 | now() のみ effect |
| uuid | I/O | 4 | v4 のみ effect |
| testing | utility | 7 | pure |
| error | utility | 3 | pure |

---

## Core Types

### string

```
// Access
len(s) -> Int
get(s, i) -> Option[String]                    // 旧 char_at
first(s) -> Option[String]                     // NEW
last(s) -> Option[String]                      // NEW
index_of(s, needle) -> Option[Int]
last_index_of(s, needle) -> Option[Int]

// Test
contains(s, sub) -> Bool
starts_with(s, prefix) -> Bool
ends_with(s, suffix) -> Bool
is_empty(s) -> Bool
is_digit(s) -> Bool
is_alpha(s) -> Bool
is_alphanumeric(s) -> Bool
is_whitespace(s) -> Bool
is_upper(s) -> Bool
is_lower(s) -> Bool

// Transform
to_upper(s) -> String
to_lower(s) -> String
capitalize(s) -> String
replace(s, from, to) -> String
replace_first(s, from, to) -> String
reverse(s) -> String
trim(s) -> String
trim_start(s) -> String
trim_end(s) -> String
pad_start(s, n, ch) -> String
pad_end(s, n, ch) -> String
strip_prefix(s, prefix) -> Option[String]
strip_suffix(s, suffix) -> Option[String]

// Slice
slice(s, start, end) -> String
take(s, n) -> String                           // NEW
drop(s, n) -> String                           // NEW
take_end(s, n) -> String                       // NEW
drop_end(s, n) -> String                       // NEW

// Decompose
split(s, sep) -> List[String]
lines(s) -> List[String]
chars(s) -> List[String]
count(s, sub) -> Int

// Combine
join(parts, sep) -> String                     // data-first: List[String]
repeat(s, n) -> String

// Convert
to_bytes(s) -> List[Int]
from_bytes(bs) -> String
codepoint(s) -> Option[Int]
from_codepoint(n) -> String

// REMOVED: to_int (use int.parse), to_float (use float.parse), char_at (use get), char_count (use len)
```

### int

```
// Convert
to_string(n) -> String
to_float(n) -> Float
to_hex(n) -> String

// Parse
parse(s) -> Result[Int, String]
from_hex(s) -> Result[Int, String]             // 旧 parse_hex

// Arithmetic
abs(n) -> Int
min(a, b) -> Int
max(a, b) -> Int
clamp(n, lo, hi) -> Int
sign(n) -> Int                                 // NEW: -1, 0, 1

// Bitwise
band(a, b) -> Int
bor(a, b) -> Int
bxor(a, b) -> Int
bshl(a, n) -> Int
bshr(a, n) -> Int
bnot(a) -> Int

// REMOVED: parse_hex (renamed to from_hex), wrap_add/wrap_mul/rotate_* (niche — move to math or remove)
```

### float

```
// Convert
to_string(n) -> String
to_int(n) -> Int
to_fixed(n, decimals) -> String
from_int(n) -> Float

// Parse
parse(s) -> Result[Float, String]

// Arithmetic
abs(n) -> Float
min(a, b) -> Float
max(a, b) -> Float
clamp(n, lo, hi) -> Float
sign(n) -> Float
round(n) -> Float
floor(n) -> Float
ceil(n) -> Float
sqrt(n) -> Float

// Test
is_nan(n) -> Bool
is_infinite(n) -> Bool
```

---

## Collections

### list

```
// Access
len(xs) -> Int
get(xs, i) -> Option[A]
get_or(xs, i, default) -> A
first(xs) -> Option[A]
last(xs) -> Option[A]
find(xs, f) -> Option[A]
find_index(xs, f) -> Option[Int]
index_of(xs, value) -> Option[Int]

// Test
contains(xs, value) -> Bool
is_empty(xs) -> Bool
any(xs, f) -> Bool
all(xs, f) -> Bool

// Transform
map(xs, f) -> List[B]
flat_map(xs, f) -> List[B]
filter(xs, f) -> List[A]
filter_map(xs, f) -> List[B]
flatten(xss) -> List[A]

// Aggregate
fold(xs, init, f) -> B
reduce(xs, f) -> Option[A]
scan(xs, init, f) -> List[B]
sum(xs) -> Int
sum_float(xs) -> Float
product(xs) -> Int
product_float(xs) -> Float
min(xs) -> Option[A]
max(xs) -> Option[A]
min_by(xs, f) -> Option[A]                    // NEW
max_by(xs, f) -> Option[A]                    // NEW
count(xs, f) -> Int

// Slice
take(xs, n) -> List[A]
drop(xs, n) -> List[A]
take_while(xs, f) -> List[A]
drop_while(xs, f) -> List[A]
slice(xs, start, end) -> List[A]

// Order
sort(xs) -> List[A]
sort_by(xs, f) -> List[A]
reverse(xs) -> List[A]

// Decompose
partition(xs, f) -> (List[A], List[A])
group_by(xs, f) -> Map[B, List[A]]
chunk(xs, n) -> List[List[A]]
windows(xs, n) -> List[List[A]]
zip(xs, ys) -> List[(A, B)]
zip_with(xs, ys, f) -> List[C]
enumerate(xs) -> List[(Int, A)]

// Combine
join(xs, sep) -> String
intersperse(xs, sep) -> List[A]

// Deduplicate
unique(xs) -> List[A]
unique_by(xs, f) -> List[A]                   // NEW
dedup(xs) -> List[A]

// Mutate (返り値は新しい List)
set(xs, i, value) -> List[A]
insert(xs, i, value) -> List[A]
remove(xs, i) -> List[A]                       // 旧 remove_at
swap(xs, i, j) -> List[A]
update(xs, i, f) -> List[A]
repeat(value, n) -> List[A]

// Side effect
each(xs, f) -> Unit

// Convert
range(start, end) -> List[Int]
```

### map

```
// Access
len(m) -> Int
get(m, key) -> Option[V]
get_or(m, key, default) -> V
keys(m) -> List[K]
values(m) -> List[V]
entries(m) -> List[(K, V)]
find(m, f) -> Option[(K, V)]                  // NEW: f(k, v) -> Bool

// Test
contains(m, key) -> Bool                       // key containment
is_empty(m) -> Bool
any(m, f) -> Bool                              // NEW: f(k, v) -> Bool
all(m, f) -> Bool                              // NEW: f(k, v) -> Bool

// Transform
map(m, f) -> Map[K, V2]                        // 旧 map_values。f(v) -> V2
filter(m, f) -> Map[K, V]                      // f(k, v) -> Bool

// Aggregate
fold(m, init, f) -> B                          // NEW: f(acc, k, v) -> B
count(m, f) -> Int                             // NEW: f(k, v) -> Bool
each(m, f) -> Unit                             // NEW: f(k, v) -> Unit

// Mutate (返り値は新しい Map)
set(m, key, value) -> Map[K, V]
remove(m, key) -> Map[K, V]
merge(m1, m2) -> Map[K, V]

// Construct
new() -> Map[K, V]
from_list(pairs) -> Map[K, V]                  // 旧 from_entries と統一

// REMOVED: map_values (renamed to map), from_entries (use from_list)
```

---

## Wrappers

### option (NEW MODULE)

```
// Transform
map(opt, f) -> Option[B]
flat_map(opt, f) -> Option[B]                  // f(a) -> Option[B]
flatten(opt) -> Option[A]                      // Option[Option[A]] -> Option[A]

// Unwrap
unwrap_or(opt, default) -> A
unwrap_or_else(opt, f) -> A

// Test
is_some(opt) -> Bool
is_none(opt) -> Bool

// Convert
to_result(opt, err_msg) -> Result[A, String]
to_list(opt) -> List[A]                        // Some(x) -> [x], None -> []
```

### result

```
// Transform
map(r, f) -> Result[B, E]
map_err(r, f) -> Result[A, F]
flat_map(r, f) -> Result[B, E]                 // 旧 and_then と統一
flatten(r) -> Result[A, E]                     // NEW: Result[Result[A, E], E] -> Result[A, E]

// Unwrap
unwrap_or(r, default) -> A
unwrap_or_else(r, f) -> A

// Test
is_ok(r) -> Bool
is_err(r) -> Bool

// Convert
to_option(r) -> Option[A]
to_err_option(r) -> Option[E]

// REMOVED: and_then (renamed to flat_map)
```

---

## Naming Conventions Summary

| パターン | 意味 | 例 | 失敗 |
|---------|------|-----|------|
| `to_*` | infallible 出力方向変換 | `int.to_string(42)` | しない |
| `from_*` | ターゲット側構築 | `int.from_hex("ff")` | Result |
| `parse` | fallible 文字列解釈 | `int.parse("42")` | Result |
| `as_*` | 動的 Value 型抽出 | `json.as_string(v)` | Option |
| `is_*` | Bool 述語 | `list.is_empty(xs)` | しない |
| `get` | インデックス/キーアクセス | `list.get(xs, 0)` | Option |
| `find` | 述語で検索 | `list.find(xs, f)` | Option |
| `map` | 各要素/中身を変換 | `list.map(xs, f)` | しない |
| `flat_map` | map → flatten | `result.flat_map(r, f)` | しない |
| `fold` | 初期値あり累積 | `list.fold(xs, 0, f)` | しない |
| `each` | 副作用実行 | `list.each(xs, f)` | しない |

---

## 破壊的変更リスト（現状 → 1.0）

| 変更 | 種別 | 影響 |
|------|------|------|
| `string.to_int` 削除 | 関数削除 | `int.parse` を使う |
| `string.to_float` 削除 | 関数削除 | `float.parse` を使う |
| `string.char_count` 削除 | 関数削除 | `string.len` を使う |
| `string.char_at` → `string.get` | リネーム | Option[String] の戻り値は同じ |
| `int.parse_hex` → `int.from_hex` | リネーム | シグネチャ同じ |
| `result.and_then` → `result.flat_map` | リネーム | シグネチャ同じ |
| `map.map_values` → `map.map` | リネーム | シグネチャ同じ |
| `map.from_entries` 削除 | 関数削除 | `map.from_list` を使う |
| `list.remove_at` → `list.remove` | リネーム | シグネチャ同じ |
| `json.to_string` → `json.as_string` | リネーム | 動的抽出の意図を明確に |
| `json.to_int` → `json.as_int` | リネーム | 同上 |
| int bitwise の niche 関数移動 | 移動 | wrap_add, rotate_* → math or 削除 |

## 新規追加リスト

| 追加 | モジュール |
|------|-----------|
| option モジュール全体 | option (10 関数) |
| map.fold, map.each, map.any, map.all, map.count, map.find | map (+6) |
| string.get, string.first, string.last, string.take, string.drop, string.take_end, string.drop_end | string (+7) |
| list.min_by, list.max_by, list.unique_by | list (+3) |
| result.flat_map, result.flatten | result (+2) |
| int.from_hex, int.sign | int (+2) |

---

## Open Questions

1. **`json.to_string` → `json.as_string`** — `json` モジュール内の `to_*` は全部 `as_*` にすべきか？`to_string` は他で stringify の意味もあるので混乱する
2. **Map の `map` callback** — `map.map(m, f)` の `f` は `(v) -> V2` か `(k, v) -> V2` か？現在の `map_values` は `(v) -> V2`
3. **Option モジュールの実装方式** — TOML 定義 + ランタイム？それとも bundled .almd？
4. **`and_then` を残すか完全に消すか** — Rust ユーザー向けエイリアスとして残す価値はあるか
