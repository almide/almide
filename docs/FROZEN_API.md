# Frozen API Surface

> Post-1.0 で以下の関数のシグネチャ（引数型・戻り型）は変更しない。
> 関数の **追加** は可能。既存関数の **変更・削除** は BREAKING_CHANGE_POLICY.md に従う。

---

## string (41 functions)

| Function | Signature |
|----------|-----------|
| trim | `(String) -> String` |
| split | `(String, String) -> List[String]` |
| join | `(List[String], String) -> String` |
| len | `(String) -> Int` |
| contains | `(String, String) -> Bool` |
| starts_with | `(String, String) -> Bool` |
| ends_with | `(String, String) -> Bool` |
| slice | `(String, Int, Int) -> String` |
| to_upper | `(String) -> String` |
| to_lower | `(String) -> String` |
| capitalize | `(String) -> String` |
| replace | `(String, String, String) -> String` |
| replace_first | `(String, String, String) -> String` |
| char_at | `(String, Int) -> Option[String]` |
| index_of | `(String, String) -> Option[Int]` |
| last_index_of | `(String, String) -> Option[Int]` |
| lines | `(String) -> List[String]` |
| chars | `(String) -> List[String]` |
| repeat | `(String, Int) -> String` |
| reverse | `(String) -> String` |
| is_empty | `(String) -> Bool` |
| is_digit | `(String) -> Bool` |
| is_alpha | `(String) -> Bool` |
| is_alphanumeric | `(String) -> Bool` |
| is_whitespace | `(String) -> Bool` |
| is_upper | `(String) -> Bool` |
| is_lower | `(String) -> Bool` |
| to_int | `(String) -> Result[Int, String]` |
| to_float | `(String) -> Result[Float, String]` |
| to_bytes | `(String) -> List[Int]` |
| from_bytes | `(List[Int]) -> String` |
| codepoint | `(String) -> Option[Int]` |
| from_codepoint | `(Int) -> String` |
| char_count | `(String) -> Int` |
| count | `(String, String) -> Int` |
| pad_left | `(String, Int, String) -> String` |
| pad_right | `(String, Int, String) -> String` |
| trim_start | `(String) -> String` |
| trim_end | `(String) -> String` |
| strip_prefix | `(String, String) -> Option[String]` |
| strip_suffix | `(String, String) -> Option[String]` |

## int (19 functions)

| Function | Signature |
|----------|-----------|
| to_string | `(Int) -> String` |
| to_hex | `(Int) -> String` |
| to_float | `(Int) -> Float` |
| parse | `(String) -> Result[Int, String]` |
| parse_hex | `(String) -> Result[Int, String]` |
| abs | `(Int) -> Int` |
| min | `(Int, Int) -> Int` |
| max | `(Int, Int) -> Int` |
| clamp | `(Int, Int, Int) -> Int` |
| band/bor/bxor | `(Int, Int) -> Int` |
| bshl/bshr | `(Int, Int) -> Int` |
| bnot | `(Int) -> Int` |
| wrap_add/wrap_mul | `(Int, Int, Int) -> Int` |
| rotate_right/rotate_left | `(Int, Int, Int) -> Int` |

## float (16 functions)

| Function | Signature |
|----------|-----------|
| to_string | `(Float) -> String` |
| to_int | `(Float) -> Int` |
| to_fixed | `(Float, Int) -> String` |
| parse | `(String) -> Result[Float, String]` |
| from_int | `(Int) -> Float` |
| round/floor/ceil | `(Float) -> Float` |
| abs/sqrt/sign | `(Float) -> Float` |
| min/max | `(Float, Float) -> Float` |
| clamp | `(Float, Float, Float) -> Float` |
| is_nan/is_infinite | `(Float) -> Bool` |

## list (54 functions)

| Function | Signature |
|----------|-----------|
| len | `(List[A]) -> Int` |
| get | `(List[A], Int) -> Option[A]` |
| set | `(List[A], Int, A) -> List[A]` |
| map | `(List[A], Fn(A) -> B) -> List[B]` |
| filter | `(List[A], Fn(A) -> Bool) -> List[A]` |
| fold | `(List[A], B, Fn(B, A) -> B) -> B` |
| find | `(List[A], Fn(A) -> Bool) -> Option[A]` |
| any | `(List[A], Fn(A) -> Bool) -> Bool` |
| all | `(List[A], Fn(A) -> Bool) -> Bool` |
| sort | `(List[A]) -> List[A]` |
| reverse | `(List[A]) -> List[A]` |
| contains | `(List[A], A) -> Bool` |
| first/last | `(List[A]) -> Option[A]` |
| take/drop | `(List[A], Int) -> List[A]` |
| flatten | `(List[List[A]]) -> List[A]` |
| unique | `(List[A]) -> List[A]` |
| join | `(List[String], String) -> String` |
| sum/product | `(List[Int]) -> Int` |
| is_empty | `(List[A]) -> Bool` |
| ... | (54 functions total — see stdlib/defs/list.toml) |

## map (16 functions)

| Function | Signature |
|----------|-----------|
| new | `() -> Map[K, V]` |
| get | `(Map[K, V], K) -> Option[V]` |
| set | `(Map[K, V], K, V) -> Map[K, V]` |
| contains | `(Map[K, V], K) -> Bool` |
| remove | `(Map[K, V], K) -> Map[K, V]` |
| keys | `(Map[K, V]) -> List[K]` |
| values | `(Map[K, V]) -> List[V]` |
| entries | `(Map[K, V]) -> List[(K, V)]` |
| len | `(Map[K, V]) -> Int` |
| merge | `(Map[K, V], Map[K, V]) -> Map[K, V]` |
| is_empty | `(Map[K, V]) -> Bool` |
| from_entries | `(List[(K, V)]) -> Map[K, V]` |
| map_values | `(Map[K, V], Fn(V) -> B) -> Map[K, B]` |
| filter | `(Map[K, V], Fn(K, V) -> Bool) -> Map[K, V]` |

## result (9 functions)

| Function | Signature |
|----------|-----------|
| map | `(Result[A, E], Fn(A) -> B) -> Result[B, E]` |
| map_err | `(Result[A, E], Fn(E) -> F) -> Result[A, F]` |
| and_then | `(Result[A, E], Fn(A) -> Result[B, E]) -> Result[B, E]` |
| unwrap_or | `(Result[A, E], A) -> A` |
| unwrap_or_else | `(Result[A, E], Fn(E) -> A) -> A` |
| is_ok | `(Result[A, E]) -> Bool` |
| is_err | `(Result[A, E]) -> Bool` |
| to_option | `(Result[A, E]) -> Option[A]` |

## 凍結対象外

以下のモジュールは 1.x で追加・変更される可能性がある:
- effect モジュール: fs, http, io, process, env, log, random, datetime
- データモジュール: json, regex, value, set
- 将来追加: csv, url, html, sorted

これらは「安定だが凍結はしない」— シグネチャ変更は deprecation cycle に従う。
