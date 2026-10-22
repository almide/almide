<!-- description: Stdlib redesign — the target surface regrouped by protocol: every standard type as the protocols it conforms to plus its own functions, with the holes each protocol exposes (R-L0-37) -->
# リデザイン後の標準ライブラリ（protocol ごと）

> Last updated: 2026-10-22
> 元の記録: [target-surface.md](target-surface.md)（モジュールごとの一覧）・[R-L0-37](L0-conventions.md)

[target-surface.md](target-surface.md) と同じ関数を、型ごとではなく protocol ごとに並べ直した一覧です。R-L0-37 のとおり、標準の具体型を「準拠する protocol の組み合わせ」と「その型だけの関数」で書き表します。target-surface.md から機械的に組み立てています。

- 状態の列: 「組み込み」は言語にある protocol、「採用」は記録で決めた protocol、「候補」は形が同じ関数の組で、protocol にするかどうかはまだ決めていません。
- 穴の列: 準拠する型のうち、その関数を持たない型です。足すか、意図した抜けとして書くかは、これから決めます。すでに決めてある抜けは「決めてある抜け」の節に理由と一緒に書きます。
- 値の型を持つ 30 モジュールの関数 1,008 個のうち、637 個（63%）が下のどれかの protocol の関数です。残りの 371 個が、その型だけの関数です。

## protocol の一覧

| protocol | 状態 | 関数 | 準拠する型 | 穴 | 言語に要るもの | 案 |
|---|---|---|---|---|---|---|
| `Ord` | 組み込み | `compare` | int・int8・int16・int32・int64・uint8・uint16・uint32・uint64・float・float32・float64・bigint・bool・string・bytes・duration・compute・uuid | なし | 今の言語で書ける | R-L0-16・R-L0-22 |
| `Integer` | 採用 | `compare`・`min`・`max`・`clamp`・`parse`・`parse_radix`・`to_string`・`to_string_radix`・`min_value`・`max_value` | int・int8・int16・int32・int64・uint8・uint16・uint32・uint64 | なし | 型引数を通した、受け手のない呼び出し（`T.parse(s)`・`T.nan()`） | R-sized-08 |
| `FloatingPoint` | 採用 | `compare`・`min`・`max`・`clamp`・`parse`・`to_string`・`min_value`・`max_value`・`is_nan`・`is_infinite`・`is_finite`・`nan`・`infinity`・`epsilon`・`to_bits`・`from_bits` | float・float32・float64 | なし | 型引数を通した、受け手のない呼び出し（`T.parse(s)`・`T.nan()`） | R-sized-08 |
| `Sized` | 採用 | `len`・`is_empty`・`clear` | list・string・bytes・map・set・deque・heap | なし | 今の言語で書ける | R-L0-31 |
| `TimeAmount` | 採用 | `ns`・`us`・`ms`・`s`・`min`・`h`・`compare` | duration・compute | なし | 型引数を通した、受け手のない呼び出し（`T.parse(s)`・`T.nan()`） | R-duration-03 |
| `HttpMessage` | 採用 | `header`・`header_values`・`headers`・`body`・`body_bytes`・`set_header`・`set_body`・`set_body_bytes` | HttpRequest・HttpResponse | なし | 今の言語で書ける | R-http-12 |
| `Parse` | 候補 | `parse` | int・int8・int16・int32・int64・uint8・uint16・uint32・uint64・float・float32・float64・bigint・bool・uuid・url・regex | なし | 型引数を通した、受け手のない呼び出し（`T.parse(s)`・`T.nan()`） | R-L0-31（× 言語待ち） |
| `Repr` | 組み込み | `to_string` | int・int8・int16・int32・int64・uint8・uint16・uint32・uint64・float・float32・float64・bigint・uuid・url・bool | `bool.to_string` | 今の言語で書ける | — |
| `Radix` | 候補 | `parse_radix`・`to_string_radix` | int・int8・int16・int32・int64・uint8・uint16・uint32・uint64・bigint | なし | 型引数を通した、受け手のない呼び出し（`T.parse(s)`・`T.nan()`） | — |
| `Arithmetic` | 候補 | `abs`・`sign`・`pow`・`floor_div`・`mod`・`is_even`・`is_odd` | int・bigint | なし | 今の言語で書ける | — |
| `Sliceable` | 候補 | `reverse`・`slice`・`take`・`drop`・`take_end`・`drop_end`・`chunk`・`remove_at` | list・string・bytes | `string.chunk`・`string.remove_at` | 今の言語で書ける | — |
| `Searchable` | 候補 | `contains`・`starts_with`・`ends_with`・`index_of`・`split`・`repeat` | string・bytes | なし | 今の言語で書ける | — |
| `Collection` | 候補 | `new`・`from_list`・`to_list`・`contains`・`any`・`all`・`count`・`filter`・`fold`・`find` | list・set・deque・heap・bytes | `bytes.contains（名前の衝突）` | 関連型（要素の型）、型引数を通した、受け手のない呼び出し（`T.parse(s)`・`T.nan()`） | R-L0-31（× 言語待ち） |
| `Indexed` | 候補 | `get`・`first`・`last` | list・deque | なし | 関連型（要素の型） | — |
| `Stack` | 候補 | `push`・`pop` | list・heap | なし | 関連型（要素の型） | — |
| `Convert[T]` | 候補 | `to_T`・`to_T_checked`・`to_T_saturating`・`to_T_wrapping` | int・int8・int16・int32・int64・uint8・uint16・uint32・uint64・float・float32・float64・bigint | なし | 型引数つきの protocol | R-L0-32・R-L0-34 |

注:

- `Ord`: 順序のある型。`<mod>.compare` は `Ord` の `compare` そのもの。
- `Integer`: `int` と サイズつきの整数 8 型。
- `FloatingPoint`: `to_bits`・`from_bits` は形が同じなので候補として足しています。
- `Sized`: `clear` は 7 つとも同じ形で持つので、候補として足しています。
- `HttpMessage`: リデザイン後の一覧で、すでに `[M: HttpMessage]` の総称関数です。
- `Parse`: `parse(s: String) -> Result[Self, String]`。
- `Repr`: `"${x}"` の関数の形。`bytes.to_string` は UTF-8 の読み取りで役割が違うので外しています。
- `Arithmetic`: 整数の算術。サイズつきの整数は演算子だけで、これらの関数を持ちません。`float.abs` は `float` だけの関数に数えています。
- `Searchable`: 部分の列で探す族。`list.contains` は要素を受け取るので Collection 側です。
- `Collection`: 要素の型（`bytes` は `Int`）を関連型で持ちます。`map` は要素が `(K, V)` で callback の形が違うので外しています。`bytes.contains` は部分の列を受け取る（`Searchable`）ので、要素を受け取る `Collection` の `contains` と名前がぶつかります。
- `Convert[T]`: 数の型どうしの変換の族（347 関数）。変換先 `T` を型引数にとる protocol 1 つで書けます。どのセルがあるかは R-L0-32・R-L0-34 の行列のとおりです。

## 決めてある抜け

| 関数 | 理由 |
|---|---|
| `list.new` | リテラル `[]` で作る |
| `list.from_list` | すでに `List` |
| `list.to_list` | すでに `List` |
| `heap.fold` | R-heap-05（中の並び順が外から見える） |
| `heap.find` | R-heap-05（中の並び順が外から見える） |

## 型ごとの準拠

| 型 | 準拠する protocol | 関数の数 | その型だけの関数 |
|---|---|---|---|
| `Int` | Ord・Integer・Parse・Repr・Radix・Arithmetic・Convert[T] | 77 | 24 |
| `Int8` | Ord・Integer・Parse・Repr・Radix・Convert[T] | 33 | 0 |
| `Int16` | Ord・Integer・Parse・Repr・Radix・Convert[T] | 36 | 0 |
| `Int32` | Ord・Integer・Parse・Repr・Radix・Convert[T] | 40 | 0 |
| `Int64` | Ord・Integer・Parse・Repr・Radix・Convert[T] | 45 | 0 |
| `UInt8` | Ord・Integer・Parse・Repr・Radix・Convert[T] | 24 | 0 |
| `UInt16` | Ord・Integer・Parse・Repr・Radix・Convert[T] | 30 | 0 |
| `UInt32` | Ord・Integer・Parse・Repr・Radix・Convert[T] | 37 | 0 |
| `UInt64` | Ord・Integer・Parse・Repr・Radix・Convert[T] | 49 | 0 |
| `Float` | Ord・FloatingPoint・Parse・Repr・Convert[T] | 54 | 8 |
| `Float32` | Ord・FloatingPoint・Parse・Repr・Convert[T] | 45 | 0 |
| `Float64` | Ord・FloatingPoint・Parse・Repr・Convert[T] | 46 | 0 |
| `BigInt` | Ord・Parse・Repr・Radix・Arithmetic・Convert[T] | 23 | 3 |
| `Bool` | Ord・Parse・Repr | 2 | 0 |
| `String` | Ord・Sized・Sliceable・Searchable | 49 | 33 |
| `Bytes` | Ord・Sized・Sliceable・Searchable・Collection | 141 | 114 |
| `List[A]` | Sized・Sliceable・Collection・Indexed・Stack | 70 | 47 |
| `Map[K, V]` | Sized | 22 | 19 |
| `Set[A]` | Sized・Collection | 22 | 9 |
| `Deque[A]` | Sized・Collection・Indexed | 21 | 5 |
| `Heap[A]` | Sized・Collection・Stack | 16 | 3 |
| `Option[A]` | — | 13 | 13 |
| `Result[A, E]` | — | 14 | 14 |
| `Duration` | Ord・TimeAmount | 7 | 0 |
| `Compute` | Ord・TimeAmount | 7 | 0 |
| `Uuid` | Ord・Parse・Repr | 7 | 4 |
| `Url` | Parse・Repr | 9 | 7 |
| `Value` | — | 29 | 29 |
| `Matrix` | — | 25 | 25 |
| `Regex` | Parse | 15 | 14 |

## protocol にしない共通の名前

名前が同じでも、protocol にできない組と、役割が違うので protocol にしない組です。

| 関数 | 型 | 理由 |
|---|---|---|
| `map`・`flat_map` | list・option・result・deque・map・set | 戻り値の入れ物が引数の入れ物で決まります（`List[B]`・`Option[B]`）。Almide の protocol は入れ物そのもの（`F[_]`）を引数にとれません。（要るもの: 型の入れ物を引数にとる型（`F[_]`）） |
| `split`・`replace`・`replace_first` | string・regex | 形は同じに見えますが、`string` では Self が対象の文字列、`regex` では Self がパターンで、役割が違います。 |
| `merge` | map・value | `value.merge` は JSON の object どうしの合成で、`map.merge` とは対象が違います。 |

## その型だけの関数

- `Int`（24）: `band`、`bit_reverse`、`bit_width`、`bnot`、`bor`、`bshl`、`bshr`、`bshr_logical`、`bxor`、`byte_swap`、`count_leading_zeros`、`count_ones`、`count_trailing_zeros`、`log2_ceil`、`log2_floor`、`next_power_of_two`、`prev_power_of_two`、`rotate_left`、`rotate_right`、`to_hex`、`wrap`、`wrap_add`、`wrap_mul`、`wrap_sub`
- `Float`（8）: `abs`、`ceil`、`floor`、`pow`、`round`、`signum`、`to_fixed`、`trunc`
- `BigInt`（3）: `clamp`、`max`、`min`
- `String`（33）: `byte_len`、`byte_slice`、`capitalize`、`chars`、`count`、`first`、`from_codepoint`、`from_utf16`、`get`、`is_alpha`、`is_alphanumeric`、`is_digit`、`is_lower`、`is_upper`、`is_whitespace`、`last`、`last_index_of`、`lines`、`pad_end`、`pad_start`、`push`、`replace`、`replace_first`、`split_once`、`strip_prefix`、`strip_suffix`、`to_codepoint`、`to_lower`、`to_upper`、`to_utf16`、`trim`、`trim_end`、`trim_start`
- `Bytes`（114）: `append_bool`、`append_bytes`、`append_f16_be`、`append_f16_le`、`append_f32_be`、`append_f32_le`、`append_f64_be`、`append_f64_le`、`append_i16_be`、`append_i16_le`、`append_i32_be`、`append_i32_le`、`append_i64_be`、`append_i64_le`、`append_i8`、`append_u16_be`、`append_u16_le`、`append_u32_be`、`append_u32_le`、`append_u8`、`copy_from`、`copy_within`、`fill`、`first`、`from_string`、`get`、`get_bool`、`get_f16_be`、`get_f16_be_array`、`get_f16_le`、`get_f16_le_array`、`get_f32_be`、`get_f32_be_array`、`get_f32_le`、`get_f32_le_array`、`get_f64_be`、`get_f64_be_array`、`get_f64_le`、`get_f64_le_array`、`get_i16_be`、`get_i16_be_array`、`get_i16_le`、`get_i16_le_array`、`get_i32_be`、`get_i32_be_array`、`get_i32_le`、`get_i32_le_array`、`get_i64_be`、`get_i64_be_array`、`get_i64_le`、`get_i64_le_array`、`get_i8`、`get_u16_be`、`get_u16_be_array`、`get_u16_le`、`get_u16_le_array`、`get_u32_be`、`get_u32_be_array`、`get_u32_le`、`get_u32_le_array`、`get_u8`、`insert`、`is_eof`、`is_valid_utf8`、`last`、`map`、`next_bool`、`next_bytes`、`next_f16_be`、`next_f16_le`、`next_f32_be`、`next_f32_le`、`next_f64_be`、`next_f64_le`、`next_i16_be`、`next_i16_le`、`next_i32_be`、`next_i32_le`、`next_i64_be`、`next_i64_le`、`next_i8`、`next_u16_be`、`next_u16_le`、`next_u32_be`、`next_u32_le`、`next_u8`、`pad_end`、`pad_start`、`pop`、`push`、`put_bool`、`put_f16_be`、`put_f16_le`、`put_f32_be`、`put_f32_le`、`put_f64_be`、`put_f64_le`、`put_i16_be`、`put_i16_le`、`put_i32_be`、`put_i32_le`、`put_i64_be`、`put_i64_le`、`put_i8`、`put_u16_be`、`put_u16_le`、`put_u32_be`、`put_u32_le`、`put_u8`、`set`、`skip`、`to_string`、`to_string_lossy`、`xor`
- `List[A]`（47）: `binary_search`、`binary_search_by`、`binary_search_with`、`dedup`、`drop_while`、`enumerate`、`filter_map`、`find_index`、`find_map`、`flat_map`、`flatten`、`group_by`、`index_of`、`insert`、`intersperse`、`iterate`、`join`、`map`、`max`、`max_by`、`max_with`、`min`、`min_by`、`min_with`、`partition`、`product`、`range`、`range_step`、`reduce`、`repeat`、`scan`、`set`、`sort`、`sort_by`、`sort_with`、`split_at`、`sum`、`swap`、`take_while`、`unique`、`unique_by`、`unzip`、`update`、`window`、`with_capacity`、`zip`、`zip_with`
- `Map[K, V]`（19）: `all`、`any`、`contains`、`count`、`filter`、`find`、`fold`、`from_list`、`get`、`keys`、`map`、`merge`、`new`、`remove`、`set`、`to_list`、`update`、`upsert`、`values`
- `Set[A]`（9）: `difference`、`insert`、`intersection`、`is_disjoint`、`is_subset`、`map`、`remove`、`symmetric_difference`、`union`
- `Deque[A]`（5）: `map`、`pop_back`、`pop_front`、`push_back`、`push_front`
- `Heap[A]`（3）: `new_by`、`new_with`、`peek`
- `Option[A]`（13）: `collect`、`collect_map`、`filter`、`flat_map`、`flatten`、`is_none`、`is_some`、`map`、`or_else`、`to_list`、`to_result`、`unwrap_or_else`、`zip`
- `Result[A, E]`（14）: `filter`、`flat_map`、`flatten`、`is_err`、`is_ok`、`map`、`map_err`、`or_else`、`partition`、`to_err_option`、`to_list`、`to_option`、`unwrap_or_else`、`zip`
- `Uuid`（4）: `from_bytes`、`to_bytes`、`v4`、`v7`
- `Url`（7）: `build_query`、`decode_component`、`decode_form`、`encode_component`、`encode_form`、`parse_query`、`resolve`
- `Value`（29）: `array`、`as_array`、`as_bool`、`as_float`、`as_int`、`as_object`、`as_string`、`bool`、`float`、`get`、`get_array`、`get_bool`、`get_float`、`get_int`、`get_object`、`get_path`、`get_string`、`index`、`int`、`key`、`merge`、`null`、`object`、`omit`、`pick`、`remove_path`、`root`、`set_path`、`string`
- `Matrix`（25）: `broadcast_add_row`、`cols`、`concat_cols`、`concat_rows`、`dot_row`、`from_bytes_f16_le`、`from_bytes_f32_le`、`from_bytes_f64_le`、`from_lists`、`gather_rows`、`get`、`map`、`ones`、`rows`、`scale`、`shape`、`slice_rows`、`split_cols_even`、`to_bytes_f16_le`、`to_bytes_f32_le`、`to_bytes_f64_le`、`to_lists`、`transpose`、`zeros`、`zip_with`
- `Regex`（14）: `captures`、`captures_all`、`escape`、`find`、`find_all`、`index_of`、`is_full_match`、`is_match`、`new`、`replace`、`replace_first`、`replace_first_with`、`replace_with`、`split`
