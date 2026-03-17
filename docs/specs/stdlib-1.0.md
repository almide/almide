# Stdlib 1.0 Specification

> これは Almide 1.0 codefreeze 時の stdlib のあるべき姿。
> 現状との差分ではなく、理想状態を定義する。
> LLM validation: Claude 100%, Gemini 100% (58/58 関数名予測正答)

---

## 設計原則

1. **1 verb = 1 meaning** — 同じ動詞は全モジュールで同じ意味。エイリアスなし (Canonicity)
2. **data-first** — 第一引数がデータ。UFCS と `|>` で自然に使える
3. **`to_*` は infallible、`parse` は fallible** — 失敗しない変換は `to_`、失敗しうる解釈は `parse`
4. **`from_*` はターゲット側構築** — `int.from_hex("ff")` のように変換先モジュールに配置
5. **`as_*` は動的型抽出** — `json.as_string(v)` のように Value からの射影
6. **`is_*` は Bool 述語** — `?` suffix なし
7. **Option と Result は独立モジュール** — wrapper 型にも map/flat_map/unwrap_or
8. **Map はコレクションとして一人前** — fold/any/all/each/count が使える
9. **Char 型なし** — 文字 = 長さ 1 の String。`string.get(s, i)` は `Option[String]` を返す
10. **半開区間** — `range(start, end)` は `[start, end)` (Python/Rust/Go と同じ)

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
// NOT INCLUDED: format — 文字列補間 "${expr}" が言語構文として存在するため不要
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
sum(xs) -> Int | Float                        // 要素型に応じて返り値型が決まる
product(xs) -> Int | Float                     // 同上
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
remove_at(xs, i) -> List[A]                    // インデックス指定を明示。map.remove(key) との混同防止
swap(xs, i, j) -> List[A]
update(xs, i, f) -> List[A]
repeat(value, n) -> List[A]

// Side effect
each(xs, f) -> Unit

// Convert
range(start, end) -> List[Int]                 // [start, end) 半開区間
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
map(m, f) -> Map[K, V2]                        // f(k, v) -> V2。filter と対称
filter(m, f) -> Map[K, V]                      // f(k, v) -> Bool

// Aggregate
fold(m, init, f) -> B                          // NEW: f(acc, k, v) -> B
count(m, f) -> Int                             // NEW: f(k, v) -> Bool
each(m, f) -> Unit                             // NEW: f(k, v) -> Unit

// Mutate (返り値は新しい Map)
set(m, key, value) -> Map[K, V]
remove(m, key) -> Map[K, V]
update(m, key, f) -> Map[K, V]                // NEW: f(v) -> V。キーが存在すれば値を変換
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

## Data Format

### json

json モジュールは Value 型（動的型付きデータ）を扱う。命名規則:
- **`parse` / `stringify`** — 文字列 ↔ Value の変換
- **`as_*`** — Value からの型抽出 (Option を返す)
- **`get`** — キーで子 Value を取得 (Option)
- **`get_*`** — キー指定 + 型抽出のショートカット (利便性で維持)

```
// Parse / Stringify
parse(s) -> Result[Value, String]
stringify(v) -> String
stringify_pretty(v) -> String

// Construct
object(entries) -> Value                       // List[(String, Value)] -> Value
array(items) -> Value                          // List[Value] -> Value
string(s) -> Value                             // String -> Value (json.s は予約語回避)
int(n) -> Value
float(n) -> Value
bool(b) -> Value
null() -> Value

// Type extraction (as_* = dynamic, returns Option)
as_string(v) -> Option[String]                 // 旧 to_string
as_int(v) -> Option[Int]                       // 旧 to_int
as_float(v) -> Option[Float]
as_bool(v) -> Option[Bool]
as_array(v) -> Option[List[Value]]

// Key access
get(v, key) -> Option[Value]
get_string(v, key) -> Option[String]           // get + as_string のショートカット
get_int(v, key) -> Option[Int]                 // get + as_int のショートカット
get_float(v, key) -> Option[Float]
get_bool(v, key) -> Option[Bool]
get_array(v, key) -> Option[List[Value]]

// Path access
get_path(v, path) -> Option[Value]
set_path(v, path, value) -> Value
remove_path(v, path) -> Value
field(path, name) -> JsonPath
index(path, i) -> JsonPath

// Keys
keys(v) -> List[String]

// REMOVED: to_string, to_int (use as_*), s/i/f/b (use from_string/from_int 等)
// KEPT: get_string, get_int 等 — 2段パイプのショートカットとして実用的
// NOTE: from_string, from_int 等は json.string(), json.int() に統一検討中
```

**json / value 整合性ルール:**
- `as_*` は両モジュールで **`Option`** を返す (型抽出の失敗 = None)
- `get` は両モジュールで **`Option[Value]`** を返す (キーアクセス)
- construct は `json.string()` / `value.str()` — 短縮形の扱いは要議論

```
```

### value

Value 型の操作。json モジュールと共通の Value 型を使うが、value は Codec (encode/decode) 用途。

```
// Construct
str(s) -> Value
int(n) -> Value
float(f) -> Value
bool(b) -> Value
object(pairs) -> Value                         // List[(String, Value)] -> Value
array(items) -> Value                          // List[Value] -> Value
null() -> Value

// Type extraction (as_* = Option に統一。json.as_* と同じ戻り値型)
as_string(v) -> Option[String]                 // 現状は Result — Option に変更
as_int(v) -> Option[Int]                       // 同上
as_float(v) -> Option[Float]                   // 同上
as_bool(v) -> Option[Bool]                     // 同上
as_array(v) -> Option[List[Value]]             // 同上

// Access
get(v, key) -> Option[Value]                   // 旧 field。json.get と対称

// Transform
pick(v, keys) -> Value
omit(v, keys) -> Value
merge(a, b) -> Value
to_camel_case(v) -> Value
to_snake_case(v) -> Value

// Stringify
stringify(v) -> String

// BREAKING: as_* の戻り値を Result → Option に変更
// BREAKING: field → get にリネーム (json.get と対称)
```

### 他の I/O / ユーティリティモジュール

以下のモジュールは命名規則の違反なし。1.0 で現状の API をそのまま凍結:

| Module | Functions | 備考 |
|--------|-----------|------|
| value | 19 | json と対称。as_*/from_* 命名済み |
| math | 19 | min/max/abs/sqrt/sin/cos/pow 等 |
| regex | 8 | is_match/find/find_all/replace/split 等 |
| fs | 24 | read_text/write/mkdir_p/glob 等 (全 effect) |
| http | 26 | get/post/serve 等 (全 effect) |
| io | 3 | read_line/read_all/print (全 effect) |
| process | 6 | exec/exit 等 (全 effect) |
| env | 9 | get/set/args/os 等 (大半 effect) |
| log | 8 | debug/info/warn/error 等 (全 effect) |
| random | 4 | int/float/choice/shuffle (全 effect) |
| crypto | 3 | sha256/hmac_sha256/hmac_verify (全 effect) |
| datetime | 21 | now/parse_iso/format 等 (now のみ effect) |
| uuid | 4 | v4/v5/parse/is_valid (v4 のみ effect) |
| testing | 7 | assert/assert_eq/assert_ne 等 |
| error | 3 | message/wrap/chain |

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
| `string.char_at` → `string.get` | リネーム | list.get との対称性。LLM 100% 正答 |
| `string.pad_left` → `string.pad_start` | リネーム | start/end 統一 |
| `string.pad_right` → `string.pad_end` | リネーム | 同上 |
| `int.parse_hex` → `int.from_hex` | リネーム | from_* パターン。LLM 100% 正答 |
| `result.and_then` 削除 → `result.flat_map` | リネーム + 旧名削除 | エイリアスなし。Canonicity |
| `map.map_values` → `map.map` | リネーム | callback は f(k, v) -> V2 |
| `map.from_entries` 削除 | 関数削除 | `map.from_list` に統一 |
| `list.sum_float` / `list.product_float` 削除 | 関数削除 | sum/product が型に応じて動作 |
| `json.to_string` → `json.as_string` | リネーム | 動的抽出は as_* |
| `json.to_int` → `json.as_int` | リネーム | 同上 |
| 非変更: `list.remove_at` 維持 | — | map.remove との混同防止 |
| 非変更: int bitwise niche 関数 | — | hash.almd が依存 |
| 非変更: `json.get_string` 等 維持 | — | get + as のショートカットとして実用的 |
| `value.as_*` 戻り値 Result → Option | 型変更 | json.as_* と統一 |
| `value.field` → `value.get` | リネーム | json.get と対称 |
| `json.to_string` / `json.to_int` 削除 | 関数削除 | as_* と重複 |
| `json.s` / `json.i` / `json.f` / `json.b` 削除 | 関数削除 | from_string 等を使う |

## 新規追加リスト

| 追加 | モジュール |
|------|-----------|
| option モジュール全体 (TOML+runtime) | option (11 関数: map, flat_map, filter, flatten, unwrap_or, unwrap_or_else, is_some, is_none, to_result, to_list) |
| map.fold, map.each, map.any, map.all, map.count, map.find, map.update | map (+7) |
| string.get, string.first, string.last, string.take, string.drop, string.take_end, string.drop_end | string (+7) |
| list.min_by, list.max_by, list.unique_by | list (+3) |
| result.flat_map, result.flatten | result (+2) |
| int.from_hex, int.sign | int (+2) |

---

## Resolved Questions

1. **`json.to_*` → `json.as_*`** — ✅ 全て as_* にリネーム。`json.stringify` は維持
2. **Map の `map` callback** — ✅ `(v) -> V2` (value-only)。他の動詞 (fold, each, any, all, filter, find) は `(k, v)`
3. **Option モジュール実装** — ✅ TOML + runtime (Rust Option<T> / TS nullable の最適 codegen のため)
4. **`and_then` retention** — ✅ **削除**。エイリアスなし。`flat_map` が唯一の名前 (Canonicity 原則)

## LLM Validation

| Model | Score | Note |
|-------|-------|------|
| Claude Sonnet | 58/58 (100%) | 全関数名を正確に予測 |
| Gemini | 58/58 (100%) | 全関数名を正確に予測 |

**この命名体系は LLM が迷わず書ける。** Almide の理念「LLM が最も正確に書ける言語」に合致。
