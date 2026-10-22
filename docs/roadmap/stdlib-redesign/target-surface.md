<!-- description: Stdlib redesign — the standard library as it stands once every adopted proposal lands: every public function with its post-redesign signature, per module, and what was removed -->
# リデザイン後の標準ライブラリ（設計）

> Last updated: 2026-10-22
> 元の記録: [README.md](README.md)（案の索引）・各 L2 ページの L3 関数表

採用した案（○）をすべて適用したあとの、標準ライブラリの公開関数の一覧です。各 L2 ページの L3 関数表から機械的に組み立てています。表と食い違うときは、L3 の表と L2 の案が正です。

- 保留の案（R-L0-20・R-L0-21・zlib の出力の一致）は適用していません。いまの形のままです。
- シグネチャは `fn モジュール.名前(引数) -> 戻り型` の形にそろえています。`mut` はその場で書き換える引数、`T?` は `Option[T]` です（R-L0-24 の正規の綴り）。`effect fn` の戻り型は、docs/stdlib と同じく成功したときの型で書きます（失敗は `String` の err で、呼び出し側は `!` で伝搬します）。effect でない関数が失敗を返すときは `Result[T, String]` と書きます。
- 「変化」の列: 空欄は据え置き、新規は追加、改名は旧名つき、意味変更は何が変わるかを書きます。撤去した関数は、各モジュールの最後に移行先と一緒に載せます。
- 移行の扱いはすべて R-L0-14 のとおりです（epoch 15 で `@deprecated`、epoch 16 で撤去、`almide fix` かヒントで移行）。

## 一覧に出ない共通の約束

シグネチャだけでは読めない、全モジュールに共通する約束です。根拠は各ページと契約台帳（docs/contracts）にあります。

| 約束 | 中身 | 根拠 |
|---|---|---|
| 文字列の位置 | `string` の位置・長さ（`len`・`get`・`slice`・`index_of`）と `regex.index_of` は、どれもコードポイント単位です。バイト単位は `byte_` で始まる関数（`byte_len`・`byte_slice`）だけです。 | L2-string、L2-regex |
| `Float` の順序 | `float.compare`（`float32`・`float64` も）と、`list.sort`・`binary_search`・`heap` の既定の順序は、IEEE 754 の totalOrder です（`-0.0 < 0.0`、NaN は正の無限大より大きい）。`<` 演算子だけは NaN との比較が false です。 | L2-float 完全性の規則 4 |
| `Map`・`Set` の走査順 | 入れた順です（同じキーを上書きしても位置は変わりません）。native と wasm で同じです。 | 契約台帳（Map・Set の挿入順） |
| 数学関数の値 | `math` の超越関数は、libm に任せず Almide で書いた同じ実装を両 target で通し、ビット単位で同じ値を返します。NaN は 1 つの決まった値です。 | ADR-0016、L2-math |
| `_checked` の変換 | 結果が元の値と正確に等しいときだけ some です。範囲外、小数部の切り捨て、精度の損失はどれも none です（`float.to_int32_checked(1.5)` も `int.to_float32_checked(16777217)` も none）。 | R-L0-28 |
| `next_` の none | 値が none のときは、受け取った状態（読み位置・乱数の状態）をそのまま返します。 | R-L0-27 |
| 文字列の本文 | `http.body`・`req_body` は、不正な UTF-8 を置換文字にして読みます。正確なバイトは `body_bytes`・`req_body_bytes` です。 | R-http-11 |
| `heap` の向き | 既定（`heap.new`・`from_list`）は最小が先です。最大を先にするときは `new_with` か `new_by` で順序を渡します。`heap.to_list` は `pop` を繰り返したのと同じ順で、同じ順位は入れた順です。 | L2-heap、R-L1-02 |
| `Matrix` の演算子 | `+`・`-`・`/`・単項 `-` は要素ごと、`*` は行列の積です（要素ごとの積は `matrix.zip_with`）。形が合わなければ abort です。 | R-matrix-14、L2-matrix |
| `min_value` | 整数の `min_value` は最も小さい値です。浮動小数点の `min_value` は Rust の `MIN` と同じ「最も小さい有限の値」（`-max_value()`）で、Java・Kotlin の「最小の正の値」ではありません。 | L2-float 完全性の規則 2 |
| 整数の端 | `int.bshl`・`bshr` のシフト量は 64 で割った余りです（`bshl(1, 64)` は 1）。`int.abs(int.min_value())` は `min_value` のままです（折り返し）。無印の `float.to_<整数>` は端に張り付き、NaN は 0 です。整数どうしの無印 `to_<型>` は折り返します（R-L0-08）。どれも両 target で同じです。 | 実測 `0.68.0 dev 437b8f127`、R-L0-08 |
| `map.update` | キーが無ければ何もせず、同じ map を返します。無いときに作るなら `upsert` です。 | 実測 |
| 改行 | `io.print`・`io.eprint` は改行を付けません。付けるのは組み込みの `println`・`eprintln` です。 | L2-io |
| 範囲のはみ出し | 位置や長さが範囲を超えたら丸めます（`list.slice(xs, -3, n)` の負の開始は 0 になります）。添字 1 つを書き換える関数と、形を前提にする関数は abort です。負の位置を abort にする案は R-L0-29 で保留しています。 | R-L0-12、R-L0-29 |
| `zlib` の圧縮 | 圧縮の出力は native と wasm で違います（往復だけを約束します）。そのため圧縮の関数は effect のままです。出力の一致は保留です。 | R-zlib-03（保留） |

## 全体の数

| いまの関数 | リデザイン後 | 据え置き | 新規 | 改名 | 意味変更 | 撤去 |
|---:|---:|---:|---:|---:|---:|---:|
| 1001 | 1143 | 659 | 340 | 81 | 63 | 198 |

固定幅の数（`int8`〜`uint64`、`float32`・`float64`）は型ごとに 1 関数と数えます。

## モジュールごとの数

| モジュール | リデザイン後 | 新規 | 改名 | 意味変更 | 撤去 | 記録 |
|---|---:|---:|---:|---:|---:|---|
| [`args`](#args) | 3 | 1 | 0 | 0 | 4 | [L2-args.md](L2-args.md) |
| [`base64`](#base64) | 4 | 0 | 0 | 2 | 0 | [L2-base64.md](L2-base64.md) |
| [`bigint`](#bigint)（新しいモジュール） | 23 | 23 | 0 | 0 | 0 | [L2-bigint.md](L2-bigint.md) |
| [`bool`](#bool) | 1 | 1 | 0 | 0 | 0 | [L2-bool.md](L2-bool.md) |
| [`bytes`](#bytes) | 139 | 26 | 71 | 6 | 35 | [L2-bytes.md](L2-bytes.md) |
| [`compute`](#compute) | 7 | 1 | 0 | 0 | 0 | [L2-compute.md](L2-compute.md) |
| [`datetime`](#datetime) | 18 | 1 | 0 | 1 | 5 | [L2-datetime.md](L2-datetime.md) |
| [`deque`](#deque)（新しいモジュール） | 21 | 21 | 0 | 0 | 0 | [L2-deque.md](L2-deque.md) |
| [`duration`](#duration) | 7 | 1 | 0 | 0 | 0 | [L2-duration.md](L2-duration.md) |
| [`env`](#env) | 8 | 2 | 0 | 0 | 3 | [L2-env.md](L2-env.md) |
| `error`（モジュールごと撤去） | 0 | 0 | 0 | 0 | 3 | [L2-error.md](L2-error.md) |
| [`ffi`](#ffi)（新しいモジュール） | 4 | 4 | 0 | 0 | 0 | [L2-ffi.md](L2-ffi.md) |
| [`float`](#float) | 45 | 11 | 1 | 0 | 12 | [L2-float.md](L2-float.md) |
| [`float32`](#float32) | 30 | 20 | 0 | 0 | 0 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`float64`](#float64) | 31 | 21 | 0 | 0 | 0 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`fs`](#fs) | 30 | 1 | 2 | 3 | 5 | [L2-fs.md](L2-fs.md) |
| [`hash`](#hash) | 11 | 8 | 0 | 2 | 1 | [L2-hash.md](L2-hash.md) |
| [`heap`](#heap)（新しいモジュール） | 16 | 16 | 0 | 0 | 0 | [L2-heap.md](L2-heap.md) |
| [`hex`](#hex) | 3 | 0 | 0 | 0 | 0 | [L2-hex.md](L2-hex.md) |
| [`html`](#html) | 5 | 0 | 0 | 0 | 0 | [L2-html.md](L2-html.md) |
| [`http`](#http) | 39 | 6 | 2 | 7 | 15 | [L2-http.md](L2-http.md) |
| [`int`](#int) | 70 | 13 | 1 | 2 | 15 | [L2-int.md](L2-int.md) |
| [`int16`](#int16) | 28 | 6 | 0 | 0 | 0 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`int32`](#int32) | 31 | 7 | 0 | 0 | 0 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`int64`](#int64) | 35 | 9 | 0 | 0 | 0 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`int8`](#int8) | 26 | 6 | 0 | 0 | 0 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`io`](#io) | 7 | 3 | 0 | 2 | 4 | [L2-io.md](L2-io.md) |
| [`json`](#json) | 3 | 0 | 0 | 1 | 12 | [L2-json.md](L2-json.md) |
| [`list`](#list) | 70 | 9 | 0 | 12 | 6 | [L2-list.md](L2-list.md) |
| [`map`](#map) | 22 | 0 | 1 | 0 | 3 | [L2-map.md](L2-map.md) |
| [`math`](#math) | 27 | 12 | 0 | 0 | 8 | [L2-math.md](L2-math.md) |
| [`matrix`](#matrix) | 25 | 3 | 0 | 7 | 24 | [L2-matrix.md](L2-matrix.md) |
| [`net`](#net) | 22 | 9 | 0 | 1 | 13 | [L2-net.md](L2-net.md) |
| [`option`](#option) | 13 | 0 | 0 | 0 | 1 | [L2-option.md](L2-option.md) |
| [`path`](#path) | 10 | 3 | 0 | 0 | 3 | [L2-path.md](L2-path.md) |
| [`process`](#process) | 13 | 8 | 0 | 2 | 12 | [L2-process.md](L2-process.md) |
| [`random`](#random) | 10 | 6 | 0 | 0 | 0 | [L2-random.md](L2-random.md) |
| [`regex`](#regex) | 15 | 7 | 1 | 7 | 0 | [L2-regex.md](L2-regex.md) |
| [`result`](#result) | 14 | 0 | 0 | 0 | 1 | [L2-result.md](L2-result.md) |
| [`set`](#set) | 22 | 3 | 0 | 0 | 0 | [L2-set.md](L2-set.md) |
| [`string`](#string) | 49 | 4 | 1 | 2 | 5 | [L2-string.md](L2-string.md) |
| [`testing`](#testing) | 10 | 0 | 0 | 2 | 0 | [L2-testing.md](L2-testing.md) |
| [`uint16`](#uint16) | 24 | 6 | 0 | 0 | 0 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`uint32`](#uint32) | 29 | 7 | 0 | 0 | 0 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`uint64`](#uint64) | 38 | 12 | 0 | 0 | 0 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`uint8`](#uint8) | 20 | 6 | 0 | 0 | 0 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`unix`](#unix)（新しいモジュール） | 12 | 12 | 0 | 0 | 0 | [L2-unix.md](L2-unix.md) |
| [`url`](#url) | 9 | 4 | 0 | 1 | 1 | [L2-url.md](L2-url.md) |
| [`uuid`](#uuid)（新しいモジュール） | 7 | 7 | 0 | 0 | 0 | [L2-uuid.md](L2-uuid.md) |
| [`value`](#value) | 29 | 14 | 1 | 0 | 5 | [L2-value.md](L2-value.md) |
| [`zlib`](#zlib) | 8 | 0 | 0 | 3 | 2 | [L2-zlib.md](L2-zlib.md) |

## args

記録: [L2-args.md](L2-args.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `has_flag` | `effect fn args.has_flag(name: String) -> Bool` | 新規 | R-args-01 |
| `option` | `effect fn args.option(name: String) -> String?` |  |  |
| `positional` | `effect fn args.positional() -> List[String]` |  |  |

撤去（4）:

| 関数 | 代わり | 案 |
|---|---|---|
| `flag` | `has_flag` | R-args-01 |
| `option_or` | `option(n)! ?? d` | R-args-02 |
| `positional_at` | `list.get(args.positional()!, i)` | R-args-03 |
| `raw` | `env.args` | R-args-02 |

## base64

記録: [L2-base64.md](L2-base64.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `decode` | `fn base64.decode(s: String) -> Result[Bytes, String]` | 意味変更: `+/` だけを受け取る | R-base64-01 |
| `decode_url` | `fn base64.decode_url(s: String) -> Result[Bytes, String]` | 意味変更: `-_` だけを受け取る | R-base64-01 |
| `encode` | `fn base64.encode(b: Bytes) -> String` |  |  |
| `encode_url` | `fn base64.encode_url(b: Bytes) -> String` |  |  |

## bigint

新しいモジュールです（L1 で追加）。
記録: [L2-bigint.md](L2-bigint.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `abs` | `fn bigint.abs(a: BigInt) -> BigInt` | 新規 | R-bigint-01 |
| `clamp` | `fn bigint.clamp(n: BigInt, lo: BigInt, hi: BigInt) -> BigInt` | 新規 | R-bigint-05 |
| `compare` | `fn bigint.compare(a: BigInt, b: BigInt) -> Int` | 新規 | R-bigint-01 |
| `floor_div` | `fn bigint.floor_div(a: BigInt, b: BigInt) -> BigInt` | 新規 | R-bigint-01（R-int-03 と同じ綴り） |
| `is_even` | `fn bigint.is_even(a: BigInt) -> Bool` | 新規 | R-bigint-01（`int.is_even` と同じ綴り） |
| `is_odd` | `fn bigint.is_odd(a: BigInt) -> Bool` | 新規 | R-bigint-01 |
| `max` | `fn bigint.max(a: BigInt, b: BigInt) -> BigInt` | 新規 | R-bigint-01 |
| `min` | `fn bigint.min(a: BigInt, b: BigInt) -> BigInt` | 新規 | R-bigint-01 |
| `mod` | `fn bigint.mod(a: BigInt, b: BigInt) -> BigInt` | 新規 | R-bigint-01（R-int-03 と同じ綴り） |
| `parse` | `fn bigint.parse(s: String) -> Result[BigInt, String]` | 新規 | R-bigint-01 |
| `parse_radix` | `fn bigint.parse_radix(s: String, radix: Int) -> Result[BigInt, String]` | 新規 | R-bigint-01 |
| `pow` | `fn bigint.pow(base: BigInt, exp: Int) -> BigInt` | 新規 | R-bigint-01 |
| `sign` | `fn bigint.sign(a: BigInt) -> Int` | 新規 | R-bigint-01 |
| `to_float` | `fn bigint.to_float(b: BigInt) -> Float` | 新規 | R-bigint-01 |
| `to_float_checked` | `fn bigint.to_float_checked(b: BigInt) -> Float?` | 新規 | R-bigint-01 |
| `to_int` | `fn bigint.to_int(b: BigInt) -> Int` | 新規 | R-bigint-01 |
| `to_int_checked` | `fn bigint.to_int_checked(b: BigInt) -> Int?` | 新規 | R-bigint-01 |
| `to_int_saturating` | `fn bigint.to_int_saturating(b: BigInt) -> Int` | 新規 | R-bigint-01 |
| `to_string` | `fn bigint.to_string(b: BigInt) -> String` | 新規 | R-bigint-01 |
| `to_string_radix` | `fn bigint.to_string_radix(b: BigInt, radix: Int) -> String` | 新規 | R-bigint-01 |
| `to_uint64` | `fn bigint.to_uint64(b: BigInt) -> UInt64` | 新規 | R-bigint-06 |
| `to_uint64_checked` | `fn bigint.to_uint64_checked(b: BigInt) -> UInt64?` | 新規 | R-bigint-06 |
| `to_uint64_saturating` | `fn bigint.to_uint64_saturating(b: BigInt) -> UInt64` | 新規 | R-bigint-06 |

## bool

記録: [L2-bool.md](L2-bool.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `parse` | `fn bool.parse(s: String) -> Result[Bool, String]` | 新規 | R-bool-01 |

## bytes

記録: [L2-bytes.md](L2-bytes.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `all` | `fn bytes.all(b: Bytes, f: (Int) -> Bool) -> Bool` | 新規 | R-bytes-18 |
| `any` | `fn bytes.any(b: Bytes, f: (Int) -> Bool) -> Bool` | 新規 | R-bytes-18 |
| `append_bool` | `fn bytes.append_bool(mut b: Bytes, val: Bool) -> Unit` | 改名（旧 `write_bool`） | R-bytes-03 |
| `append_bytes` | `fn bytes.append_bytes(mut b: Bytes, src: Bytes) -> Unit` | 新規 | R-bytes-05 |
| `append_f16_be` | `fn bytes.append_f16_be(mut b: Bytes, val: Float) -> Unit` | 新規 | R-bytes-03 |
| `append_f16_le` | `fn bytes.append_f16_le(mut b: Bytes, val: Float) -> Unit` | 新規 | R-bytes-03 |
| `append_f32_be` | `fn bytes.append_f32_be(mut b: Bytes, val: Float) -> Unit` |  |  |
| `append_f32_le` | `fn bytes.append_f32_le(mut b: Bytes, val: Float) -> Unit` |  |  |
| `append_f64_be` | `fn bytes.append_f64_be(mut b: Bytes, val: Float) -> Unit` |  |  |
| `append_f64_le` | `fn bytes.append_f64_le(mut b: Bytes, val: Float) -> Unit` |  |  |
| `append_i16_be` | `fn bytes.append_i16_be(mut b: Bytes, val: Int) -> Unit` |  |  |
| `append_i16_le` | `fn bytes.append_i16_le(mut b: Bytes, val: Int) -> Unit` |  |  |
| `append_i32_be` | `fn bytes.append_i32_be(mut b: Bytes, val: Int) -> Unit` |  |  |
| `append_i32_le` | `fn bytes.append_i32_le(mut b: Bytes, val: Int) -> Unit` |  |  |
| `append_i64_be` | `fn bytes.append_i64_be(mut b: Bytes, val: Int) -> Unit` |  |  |
| `append_i64_le` | `fn bytes.append_i64_le(mut b: Bytes, val: Int) -> Unit` |  |  |
| `append_i8` | `fn bytes.append_i8(mut b: Bytes, val: Int) -> Unit` | 新規 | R-bytes-15 |
| `append_u16_be` | `fn bytes.append_u16_be(mut b: Bytes, val: Int) -> Unit` |  |  |
| `append_u16_le` | `fn bytes.append_u16_le(mut b: Bytes, val: Int) -> Unit` |  |  |
| `append_u32_be` | `fn bytes.append_u32_be(mut b: Bytes, val: Int) -> Unit` |  |  |
| `append_u32_le` | `fn bytes.append_u32_le(mut b: Bytes, val: Int) -> Unit` |  |  |
| `chunk` | `fn bytes.chunk(b: Bytes, size: Int) -> List[Bytes]` | 改名（旧 `chunks`） | R-bytes-08 |
| `clear` | `fn bytes.clear(mut b: Bytes) -> Unit` |  |  |
| `compare` | `fn bytes.compare(a: Bytes, b: Bytes) -> Int` | 改名（旧 `cmp`） | R-bytes-08 |
| `contains` | `fn bytes.contains(b: Bytes, pattern: Bytes) -> Bool` |  |  |
| `copy_from` | `fn bytes.copy_from(mut dst: Bytes, src: Bytes, dst_off: Int, src_off: Int, len: Int) -> Unit` |  |  |
| `copy_within` | `fn bytes.copy_within(mut b: Bytes, src_start: Int, src_end: Int, dst: Int) -> Unit` |  |  |
| `count` | `fn bytes.count(b: Bytes, f: (Int) -> Bool) -> Int` | 新規 | R-bytes-18 |
| `drop` | `fn bytes.drop(b: Bytes, n: Int) -> Bytes` | 新規 | R-bytes-08 |
| `drop_end` | `fn bytes.drop_end(b: Bytes, n: Int) -> Bytes` | 新規 | R-bytes-08 |
| `ends_with` | `fn bytes.ends_with(b: Bytes, suffix: Bytes) -> Bool` |  |  |
| `fill` | `fn bytes.fill(mut b: Bytes, val: Int) -> Unit` |  |  |
| `filter` | `fn bytes.filter(b: Bytes, f: (Int) -> Bool) -> Bytes` | 新規 | R-bytes-18 |
| `find` | `fn bytes.find(b: Bytes, f: (Int) -> Bool) -> Int?` | 新規 | R-bytes-18 |
| `first` | `fn bytes.first(b: Bytes) -> Int?` | 新規 | R-bytes-20 |
| `fold` | `fn bytes.fold(b: Bytes, init: B, f: (B, Int) -> B) -> B` | 新規 | R-bytes-18 |
| `from_list` | `fn bytes.from_list(xs: List[Int]) -> Bytes` |  |  |
| `from_string` | `fn bytes.from_string(s: String) -> Bytes` |  | R-bytes-10 |
| `get` | `fn bytes.get(b: Bytes, i: Int) -> Int?` |  | R-bytes-09 |
| `get_bool` | `fn bytes.get_bool(b: Bytes, pos: Int) -> Bool?` | 改名（旧 `read_bool`）。範囲外は false ではなく none | R-bytes-09 |
| `get_f16_be` | `fn bytes.get_f16_be(b: Bytes, pos: Int) -> Float?` | 新規 | R-bytes-03 |
| `get_f16_be_array` | `fn bytes.get_f16_be_array(b: Bytes, pos: Int, count: Int) -> List[Float]?` | 新規 | R-bytes-03 |
| `get_f16_le` | `fn bytes.get_f16_le(b: Bytes, pos: Int) -> Float?` | 改名（旧 `read_f16_le`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_f16_le_array` | `fn bytes.get_f16_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]?` | 改名（旧 `read_f16_le_array`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_f32_be` | `fn bytes.get_f32_be(b: Bytes, pos: Int) -> Float?` | 改名（旧 `read_f32_be`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_f32_be_array` | `fn bytes.get_f32_be_array(b: Bytes, pos: Int, count: Int) -> List[Float]?` | 改名（旧 `read_f32_be_array`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_f32_le` | `fn bytes.get_f32_le(b: Bytes, pos: Int) -> Float?` | 改名（旧 `read_f32_le`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_f32_le_array` | `fn bytes.get_f32_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]?` | 改名（旧 `read_f32_le_array`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_f64_be` | `fn bytes.get_f64_be(b: Bytes, pos: Int) -> Float?` | 改名（旧 `read_f64_be`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_f64_be_array` | `fn bytes.get_f64_be_array(b: Bytes, pos: Int, count: Int) -> List[Float]?` | 改名（旧 `read_f64_be_array`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_f64_le` | `fn bytes.get_f64_le(b: Bytes, pos: Int) -> Float?` | 改名（旧 `read_f64_le`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_f64_le_array` | `fn bytes.get_f64_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]?` | 改名（旧 `read_f64_le_array`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_i16_be` | `fn bytes.get_i16_be(b: Bytes, pos: Int) -> Int?` | 改名（旧 `read_i16_be`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_i16_be_array` | `fn bytes.get_i16_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | 改名（旧 `read_i16_be_array`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_i16_le` | `fn bytes.get_i16_le(b: Bytes, pos: Int) -> Int?` | 改名（旧 `read_i16_le`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_i16_le_array` | `fn bytes.get_i16_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | 改名（旧 `read_i16_le_array`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_i32_be` | `fn bytes.get_i32_be(b: Bytes, pos: Int) -> Int?` | 改名（旧 `read_i32_be`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_i32_be_array` | `fn bytes.get_i32_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | 改名（旧 `read_i32_be_array`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_i32_le` | `fn bytes.get_i32_le(b: Bytes, pos: Int) -> Int?` | 改名（旧 `read_i32_le`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_i32_le_array` | `fn bytes.get_i32_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | 改名（旧 `read_i32_le_array`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_i64_be` | `fn bytes.get_i64_be(b: Bytes, pos: Int) -> Int?` | 改名（旧 `read_i64_be`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_i64_be_array` | `fn bytes.get_i64_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | 改名（旧 `read_i64_be_array`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_i64_le` | `fn bytes.get_i64_le(b: Bytes, pos: Int) -> Int?` | 改名（旧 `read_i64_le`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_i64_le_array` | `fn bytes.get_i64_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | 改名（旧 `read_i64_le_array`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_i8` | `fn bytes.get_i8(b: Bytes, pos: Int) -> Int?` | 新規 | R-bytes-15 |
| `get_u16_be` | `fn bytes.get_u16_be(b: Bytes, pos: Int) -> Int?` | 改名（旧 `read_u16_be`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_u16_be_array` | `fn bytes.get_u16_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | 改名（旧 `read_u16_be_array`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_u16_le` | `fn bytes.get_u16_le(b: Bytes, pos: Int) -> Int?` | 改名（旧 `read_u16_le`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_u16_le_array` | `fn bytes.get_u16_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | 改名（旧 `read_u16_le_array`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_u32_be` | `fn bytes.get_u32_be(b: Bytes, pos: Int) -> Int?` | 改名（旧 `read_u32_be`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_u32_be_array` | `fn bytes.get_u32_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | 改名（旧 `read_u32_be_array`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_u32_le` | `fn bytes.get_u32_le(b: Bytes, pos: Int) -> Int?` | 改名（旧 `read_u32_le`）。範囲外は 0 ではなく none | R-bytes-09 |
| `get_u32_le_array` | `fn bytes.get_u32_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | 改名（旧 `read_u32_le_array`）。範囲外は 0 ではなく none | R-bytes-09 |
| `index_of` | `fn bytes.index_of(b: Bytes, pattern: Bytes) -> Int?` |  |  |
| `insert` | `fn bytes.insert(b: Bytes, pos: Int, val: Int) -> Bytes` | 意味変更: `0..=len` の外は abort | R-bytes-11 |
| `is_empty` | `fn bytes.is_empty(b: Bytes) -> Bool` |  |  |
| `is_eof` | `fn bytes.is_eof(b: Bytes, pos: Int) -> Bool` | 改名（旧 `eof`） | R-bytes-07 |
| `is_valid_utf8` | `fn bytes.is_valid_utf8(b: Bytes) -> Bool` |  |  |
| `last` | `fn bytes.last(b: Bytes) -> Int?` | 新規 | R-bytes-20 |
| `len` | `fn bytes.len(b: Bytes) -> Int` |  |  |
| `map` | `fn bytes.map(b: Bytes, f: (Int) -> Int) -> Bytes` | 改名（旧 `map_each`） | R-bytes-08 |
| `new` | `fn bytes.new(len: Int) -> Bytes` |  |  |
| `next_bool` | `fn bytes.next_bool(b: Bytes, pos: Int) -> (Bool?, Int)` | 改名（旧 `read_bool_at`）。`(値, 次の位置)` を返す | R-bytes-13 |
| `next_bytes` | `fn bytes.next_bytes(b: Bytes, pos: Int, n: Int) -> (Bytes?, Int)` | 改名（旧 `take_at`）。`(値, 次の位置)` を返す | R-bytes-13 |
| `next_f16_be` | `fn bytes.next_f16_be(b: Bytes, pos: Int) -> (Float?, Int)` | 新規 | R-bytes-03、R-bytes-13 |
| `next_f16_le` | `fn bytes.next_f16_le(b: Bytes, pos: Int) -> (Float?, Int)` | 改名（旧 `read_f16_le_at`）。`(値, 次の位置)` を返す | R-bytes-13 |
| `next_f32_be` | `fn bytes.next_f32_be(b: Bytes, pos: Int) -> (Float?, Int)` | 改名（旧 `read_f32_be_at`）。`(値, 次の位置)` を返す | R-bytes-13 |
| `next_f32_le` | `fn bytes.next_f32_le(b: Bytes, pos: Int) -> (Float?, Int)` | 改名（旧 `read_f32_le_at`）。`(値, 次の位置)` を返す | R-bytes-13 |
| `next_f64_be` | `fn bytes.next_f64_be(b: Bytes, pos: Int) -> (Float?, Int)` | 改名（旧 `read_f64_be_at`）。`(値, 次の位置)` を返す | R-bytes-13 |
| `next_f64_le` | `fn bytes.next_f64_le(b: Bytes, pos: Int) -> (Float?, Int)` | 改名（旧 `read_f64_le_at`）。`(値, 次の位置)` を返す | R-bytes-13 |
| `next_i16_be` | `fn bytes.next_i16_be(b: Bytes, pos: Int) -> (Int?, Int)` | 改名（旧 `read_i16_be_at`）。`(値, 次の位置)` を返す | R-bytes-13 |
| `next_i16_le` | `fn bytes.next_i16_le(b: Bytes, pos: Int) -> (Int?, Int)` | 改名（旧 `read_i16_le_at`）。`(値, 次の位置)` を返す | R-bytes-13 |
| `next_i32_be` | `fn bytes.next_i32_be(b: Bytes, pos: Int) -> (Int?, Int)` | 改名（旧 `read_i32_be_at`）。`(値, 次の位置)` を返す | R-bytes-13 |
| `next_i32_le` | `fn bytes.next_i32_le(b: Bytes, pos: Int) -> (Int?, Int)` | 改名（旧 `read_i32_le_at`）。`(値, 次の位置)` を返す | R-bytes-13 |
| `next_i64_be` | `fn bytes.next_i64_be(b: Bytes, pos: Int) -> (Int?, Int)` | 改名（旧 `read_i64_be_at`）。`(値, 次の位置)` を返す | R-bytes-13 |
| `next_i64_le` | `fn bytes.next_i64_le(b: Bytes, pos: Int) -> (Int?, Int)` | 改名（旧 `read_i64_le_at`）。`(値, 次の位置)` を返す | R-bytes-13 |
| `next_i8` | `fn bytes.next_i8(b: Bytes, pos: Int) -> (Int?, Int)` | 新規 | R-bytes-15、R-bytes-13 |
| `next_u16_be` | `fn bytes.next_u16_be(b: Bytes, pos: Int) -> (Int?, Int)` | 改名（旧 `read_u16_be_at`）。`(値, 次の位置)` を返す | R-bytes-13 |
| `next_u16_le` | `fn bytes.next_u16_le(b: Bytes, pos: Int) -> (Int?, Int)` | 改名（旧 `read_u16_le_at`）。`(値, 次の位置)` を返す | R-bytes-13 |
| `next_u32_be` | `fn bytes.next_u32_be(b: Bytes, pos: Int) -> (Int?, Int)` | 改名（旧 `read_u32_be_at`）。`(値, 次の位置)` を返す | R-bytes-13 |
| `next_u32_le` | `fn bytes.next_u32_le(b: Bytes, pos: Int) -> (Int?, Int)` | 改名（旧 `read_u32_le_at`）。`(値, 次の位置)` を返す | R-bytes-13 |
| `next_u8` | `fn bytes.next_u8(b: Bytes, pos: Int) -> (Int?, Int)` | 改名（旧 `read_u8_at`）。`(値, 次の位置)` を返す | R-bytes-13 |
| `pad_end` | `fn bytes.pad_end(b: Bytes, target_len: Int, val: Int) -> Bytes` | 改名（旧 `pad_right`） | R-bytes-08 |
| `pad_start` | `fn bytes.pad_start(b: Bytes, target_len: Int, val: Int) -> Bytes` | 改名（旧 `pad_left`） | R-bytes-08 |
| `pop` | `fn bytes.pop(mut b: Bytes) -> Int?` | 新規 | R-bytes-20 |
| `push` | `fn bytes.push(mut b: Bytes, val: Int) -> Unit` |  | R-bytes-02 |
| `put_bool` | `fn bytes.put_bool(mut b: Bytes, pos: Int, val: Bool) -> Unit` | 新規 | R-bytes-03、R-bytes-11 |
| `put_f16_be` | `fn bytes.put_f16_be(mut b: Bytes, pos: Int, val: Float) -> Unit` | 新規 | R-bytes-03 |
| `put_f16_le` | `fn bytes.put_f16_le(mut b: Bytes, pos: Int, val: Float) -> Unit` | 新規 | R-bytes-03 |
| `put_f32_be` | `fn bytes.put_f32_be(mut b: Bytes, pos: Int, val: Float) -> Unit` | 改名（旧 `set_f32_be`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_f32_le` | `fn bytes.put_f32_le(mut b: Bytes, pos: Int, val: Float) -> Unit` | 改名（旧 `set_f32_le`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_f64_be` | `fn bytes.put_f64_be(mut b: Bytes, pos: Int, val: Float) -> Unit` | 改名（旧 `set_f64_be`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_f64_le` | `fn bytes.put_f64_le(mut b: Bytes, pos: Int, val: Float) -> Unit` | 改名（旧 `set_f64_le`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_i16_be` | `fn bytes.put_i16_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | 改名（旧 `set_i16_be`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_i16_le` | `fn bytes.put_i16_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | 改名（旧 `set_i16_le`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_i32_be` | `fn bytes.put_i32_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | 改名（旧 `set_i32_be`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_i32_le` | `fn bytes.put_i32_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | 改名（旧 `set_i32_le`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_i64_be` | `fn bytes.put_i64_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | 改名（旧 `set_i64_be`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_i64_le` | `fn bytes.put_i64_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | 改名（旧 `set_i64_le`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_i8` | `fn bytes.put_i8(mut b: Bytes, pos: Int, val: Int) -> Unit` | 新規 | R-bytes-15 |
| `put_u16_be` | `fn bytes.put_u16_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | 改名（旧 `set_u16_be`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_u16_le` | `fn bytes.put_u16_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | 改名（旧 `set_u16_le`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_u32_be` | `fn bytes.put_u32_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | 改名（旧 `set_u32_be`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_u32_le` | `fn bytes.put_u32_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | 改名（旧 `set_u32_le`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_u8` | `fn bytes.put_u8(mut b: Bytes, pos: Int, val: Int) -> Unit` | 改名（旧 `set_u8`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `remove_at` | `fn bytes.remove_at(b: Bytes, pos: Int) -> Bytes` | 意味変更: 範囲外は abort | R-bytes-11 |
| `repeat` | `fn bytes.repeat(b: Bytes, n: Int) -> Bytes` |  |  |
| `reverse` | `fn bytes.reverse(b: Bytes) -> Bytes` |  |  |
| `set` | `fn bytes.set(b: Bytes, i: Int, val: Int) -> Bytes` | 意味変更: 範囲外は abort | R-bytes-11 |
| `skip` | `fn bytes.skip(b: Bytes, pos: Int, n: Int) -> Int` | 意味変更: `n < 0` は abort | R-bytes-13、R-bytes-21 |
| `slice` | `fn bytes.slice(b: Bytes, start: Int, end: Int) -> Bytes` | 意味変更: 負の開始位置を 0 に丸める | R-bytes-12 |
| `split` | `fn bytes.split(b: Bytes, sep: Bytes) -> List[Bytes]` |  |  |
| `starts_with` | `fn bytes.starts_with(b: Bytes, prefix: Bytes) -> Bool` |  |  |
| `take` | `fn bytes.take(b: Bytes, n: Int) -> Bytes` | 新規 | R-bytes-08 |
| `take_end` | `fn bytes.take_end(b: Bytes, n: Int) -> Bytes` | 新規 | R-bytes-08 |
| `to_list` | `fn bytes.to_list(b: Bytes) -> List[Int]` |  |  |
| `to_string` | `fn bytes.to_string(b: Bytes) -> Result[String, String]` |  | R-bytes-10 |
| `to_string_lossy` | `fn bytes.to_string_lossy(b: Bytes) -> String` |  | R-bytes-10 |
| `xor` | `fn bytes.xor(a: Bytes, b: Bytes) -> Bytes` | 意味変更: 長さが違うと abort | R-bytes-23 |

撤去（35）:

| 関数 | 代わり | 案 |
|---|---|---|
| `append_u8` | `push` | R-bytes-02 |
| `as_mut_ptr` | `ffi.address` | R-bytes-06 |
| `as_ptr` | `ffi.address` | R-bytes-06 |
| `concat` | `+` | R-bytes-16 |
| `copy_to_ptr` | `ffi.write` | R-bytes-06 |
| `data_ptr` | `ffi.address` | R-bytes-06 |
| `from_raw_ptr` | `ffi.read` | R-bytes-06 |
| `get_or` | `get(b, i) ?? d` | R-bytes-07 |
| `heap_restore` | 内部の名前にする | R-bytes-14 |
| `heap_save` | 内部の名前にする | R-bytes-14 |
| `lines` | `string.lines(bytes.to_string(b)!)` か `bytes.split` | R-bytes-17 |
| `read_float32` | 接尾辞の系統 `get_`/`append_`/`put_` | R-bytes-04 |
| `read_int32` | 接尾辞の系統 `get_`/`append_`/`put_` | R-bytes-04 |
| `read_length_prefixed_strings_le` | `get_u32_le` と `slice` の組み合わせ | R-bytes-05 |
| `read_string_at` | `slice` ＋ `to_string_lossy` | R-bytes-05 |
| `read_string_be` | `get_u32_be` と `slice` の組み合わせ | R-bytes-05 |
| `read_string_be_at` | `next_u32_be` と `next_bytes` の組み合わせ | R-bytes-05 |
| `read_u8` | `get`（同じ役割。範囲外は none） | R-bytes-09 |
| `read_uint16` | 接尾辞の系統 `get_`/`append_`/`put_` | R-bytes-04 |
| `read_uint32` | 接尾辞の系統 `get_`/`append_`/`put_` | R-bytes-04 |
| `set_at` | `put_u8`（範囲外は abort） | R-bytes-02、R-bytes-11 |
| `set_float32` | 接尾辞の系統 `get_`/`append_`/`put_` | R-bytes-04 |
| `set_int32` | 接尾辞の系統 `get_`/`append_`/`put_` | R-bytes-04 |
| `set_uint16` | 接尾辞の系統 `get_`/`append_`/`put_` | R-bytes-04 |
| `set_uint32` | 接尾辞の系統 `get_`/`append_`/`put_` | R-bytes-04 |
| `skip_length_prefixed_le` | `get_u32_le` と `slice` の組み合わせ | R-bytes-05 |
| `write_f64_be` | `append_f64_be` | R-bytes-03 |
| `write_float32` | 接尾辞の系統 `get_`/`append_`/`put_` | R-bytes-04 |
| `write_i64_be` | `append_i64_be` | R-bytes-03 |
| `write_int32` | 接尾辞の系統 `get_`/`append_`/`put_` | R-bytes-04 |
| `write_string_be` | `append_u32_be` ＋ `append_bytes` | R-bytes-05 |
| `write_u32_be` | `append_u32_be` | R-bytes-03 |
| `write_u8` | `push` | R-bytes-02 |
| `write_uint16` | 接尾辞の系統 `get_`/`append_`/`put_` | R-bytes-04 |
| `write_uint32` | 接尾辞の系統 `get_`/`append_`/`put_` | R-bytes-04 |

## compute

記録: [L2-compute.md](L2-compute.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `compare` | `fn compute.compare(a: Compute, b: Compute) -> Int` | 新規 | R-compute-01 |
| `h` | `fn compute.h(n: Int) -> Compute` |  |  |
| `min` | `fn compute.min(n: Int) -> Compute` |  |  |
| `ms` | `fn compute.ms(n: Int) -> Compute` |  |  |
| `ns` | `fn compute.ns(n: Int) -> Compute` |  |  |
| `s` | `fn compute.s(n: Int) -> Compute` |  |  |
| `us` | `fn compute.us(n: Int) -> Compute` |  |  |

## datetime

記録: [L2-datetime.md](L2-datetime.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `add_days` | `fn datetime.add_days(ts: Int, n: Int) -> Int` |  |  |
| `add_hours` | `fn datetime.add_hours(ts: Int, n: Int) -> Int` |  |  |
| `add_minutes` | `fn datetime.add_minutes(ts: Int, n: Int) -> Int` |  |  |
| `add_seconds` | `fn datetime.add_seconds(ts: Int, n: Int) -> Int` |  |  |
| `day` | `fn datetime.day(ts: Int) -> Int` |  |  |
| `format` | `fn datetime.format(ts: Int, pattern: String) -> String` |  | R-datetime-05 |
| `from_parts` | `fn datetime.from_parts(y: Int, m: Int, d: Int, h: Int, min: Int, s: Int) -> Result[Int, String]` | 意味変更: 範囲外は err。`-> Result[Int, String]` | R-datetime-07 |
| `hour` | `fn datetime.hour(ts: Int) -> Int` |  |  |
| `minute` | `fn datetime.minute(ts: Int) -> Int` |  |  |
| `monotonic_ns` | `effect fn datetime.monotonic_ns() -> Int` |  |  |
| `month` | `fn datetime.month(ts: Int) -> Int` |  |  |
| `now` | `effect fn datetime.now() -> Int` |  | R-datetime-01（`env.unix_timestamp` の残る側） |
| `now_ms` | `effect fn datetime.now_ms() -> Int` | 新規 | R-datetime-01 |
| `parse_iso` | `fn datetime.parse_iso(s: String) -> Result[Int, String]` |  |  |
| `second` | `fn datetime.second(ts: Int) -> Int` |  |  |
| `to_iso` | `fn datetime.to_iso(ts: Int) -> String` |  |  |
| `weekday` | `fn datetime.weekday(ts: Int) -> String` |  | R-datetime-04 |
| `year` | `fn datetime.year(ts: Int) -> Int` |  |  |

撤去（5）:

| 関数 | 代わり | 案 |
|---|---|---|
| `diff_seconds` | `a - b` | R-datetime-03（`a - b`） |
| `from_unix` | 引数そのもの | R-datetime-03（引数そのもの） |
| `is_after` | `a > b` | R-datetime-03（`a > b`） |
| `is_before` | `a < b` | R-datetime-03（`a < b`） |
| `to_unix` | 引数そのもの | R-datetime-03（引数そのもの） |

## deque

新しいモジュールです（L1 で追加）。
記録: [L2-deque.md](L2-deque.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `all` | `fn deque.all[A](d: Deque[A], f: (A) -> Bool) -> Bool` | 新規 | R-deque-02 |
| `any` | `fn deque.any[A](d: Deque[A], f: (A) -> Bool) -> Bool` | 新規 | R-deque-02 |
| `clear` | `fn deque.clear[A](mut d: Deque[A]) -> Unit` | 新規 | R-deque-02、R-L0-04 |
| `contains` | `fn deque.contains[A](d: Deque[A], x: A) -> Bool` | 新規 | R-deque-02 |
| `count` | `fn deque.count[A](d: Deque[A], f: (A) -> Bool) -> Int` | 新規 | R-deque-02 |
| `filter` | `fn deque.filter[A](d: Deque[A], f: (A) -> Bool) -> Deque[A]` | 新規 | R-deque-02 |
| `find` | `fn deque.find[A](d: Deque[A], f: (A) -> Bool) -> A?` | 新規 | R-deque-02 |
| `first` | `fn deque.first[A](d: Deque[A]) -> A?` | 新規 | R-deque-01 |
| `fold` | `fn deque.fold[A, B](d: Deque[A], init: B, f: (B, A) -> B) -> B` | 新規 | R-deque-02 |
| `from_list` | `fn deque.from_list[A](xs: List[A]) -> Deque[A]` | 新規 | R-deque-01 |
| `get` | `fn deque.get[A](d: Deque[A], i: Int) -> A?` | 新規 | R-deque-01 |
| `is_empty` | `fn deque.is_empty[A](d: Deque[A]) -> Bool` | 新規 | R-deque-01 |
| `last` | `fn deque.last[A](d: Deque[A]) -> A?` | 新規 | R-deque-01 |
| `len` | `fn deque.len[A](d: Deque[A]) -> Int` | 新規 | R-deque-01 |
| `map` | `fn deque.map[A, B](d: Deque[A], f: (A) -> B) -> Deque[B]` | 新規 | R-deque-02 |
| `new` | `fn deque.new[A]() -> Deque[A]` | 新規 | R-deque-01 |
| `pop_back` | `fn deque.pop_back[A](mut d: Deque[A]) -> A?` | 新規 | R-deque-01 |
| `pop_front` | `fn deque.pop_front[A](mut d: Deque[A]) -> A?` | 新規 | R-deque-01 |
| `push_back` | `fn deque.push_back[A](mut d: Deque[A], x: A) -> Unit` | 新規 | R-deque-01 |
| `push_front` | `fn deque.push_front[A](mut d: Deque[A], x: A) -> Unit` | 新規 | R-deque-01 |
| `to_list` | `fn deque.to_list[A](d: Deque[A]) -> List[A]` | 新規 | R-deque-01 |

## duration

記録: [L2-duration.md](L2-duration.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `compare` | `fn duration.compare(a: Duration, b: Duration) -> Int` | 新規 | R-duration-01 |
| `h` | `fn duration.h(n: Int) -> Duration` |  |  |
| `min` | `fn duration.min(n: Int) -> Duration` |  |  |
| `ms` | `fn duration.ms(n: Int) -> Duration` |  |  |
| `ns` | `fn duration.ns(n: Int) -> Duration` |  |  |
| `s` | `fn duration.s(n: Int) -> Duration` |  |  |
| `us` | `fn duration.us(n: Int) -> Duration` |  |  |

## env

記録: [L2-env.md](L2-env.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `args` | `effect fn env.args() -> List[String]` |  | R-env-01（ここに集める） |
| `cwd` | `effect fn env.cwd() -> String` |  |  |
| `get` | `effect fn env.get(name: String) -> String?` |  | R-env-01、R-env-03 |
| `os` | `fn env.os() -> String` |  |  |
| `remove` | `effect fn env.remove(name: String) -> Unit` | 新規 | R-env-02 |
| `set` | `effect fn env.set(name: String, value: String) -> Unit` |  |  |
| `sleep_ms` | `effect fn env.sleep_ms(ms: Int) -> Unit` |  | R-env-01 |
| `vars` | `effect fn env.vars() -> Map[String, String]` | 新規 | R-env-02 |

撤去（3）:

| 関数 | 代わり | 案 |
|---|---|---|
| `millis` | `datetime.now_ms` へ移動 | R-env-01 |
| `temp_dir` | `fs.temp_dir` | R-env-01 |
| `unix_timestamp` | `datetime.now` | R-env-01 |

## ffi

新しいモジュールです（L1 で追加）。
記録: [L2-ffi.md](L2-ffi.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `address` | `fn ffi.address(b: Bytes) -> RawPtr` | 新規 | R-ffi-01 |
| `read` | `effect fn ffi.read(ptr: RawPtr, len: Int) -> Bytes` | 新規 | R-ffi-01 |
| `to_int` | `fn ffi.to_int(p: RawPtr) -> Int` | 新規 | R-ffi-01 |
| `write` | `effect fn ffi.write(ptr: RawPtr, b: Bytes, cap: Int) -> Int` | 新規 | R-ffi-01、R-ffi-03 |

## float

記録: [L2-float.md](L2-float.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `abs` | `fn float.abs(n: Float) -> Float` |  |  |
| `ceil` | `fn float.ceil(n: Float) -> Float` |  |  |
| `clamp` | `fn float.clamp(n: Float, lo: Float, hi: Float) -> Float` |  |  |
| `compare` | `fn float.compare(a: Float, b: Float) -> Int` | 新規 | R-sized-02 |
| `epsilon` | `fn float.epsilon() -> Float` | 新規 | R-float-03 |
| `floor` | `fn float.floor(n: Float) -> Float` |  |  |
| `from_bits` | `fn float.from_bits(bits: Int) -> Float` | 新規 | R-float-02 |
| `infinity` | `fn float.infinity() -> Float` | 新規 | R-float-03 |
| `is_finite` | `fn float.is_finite(n: Float) -> Bool` | 新規 | R-float-04 |
| `is_infinite` | `fn float.is_infinite(n: Float) -> Bool` |  |  |
| `is_nan` | `fn float.is_nan(n: Float) -> Bool` |  |  |
| `max` | `fn float.max(a: Float, b: Float) -> Float` |  | R-math-01（`math.fmax` の残る側） |
| `max_value` | `fn float.max_value() -> Float` | 新規 | R-float-03 |
| `min` | `fn float.min(a: Float, b: Float) -> Float` |  | R-math-01（`math.fmin` の残る側） |
| `min_value` | `fn float.min_value() -> Float` | 新規 | R-float-03 |
| `nan` | `fn float.nan() -> Float` | 新規 | R-float-03 |
| `parse` | `fn float.parse(s: String) -> Result[Float, String]` |  |  |
| `pow` | `fn float.pow(base: Float, exp: Float) -> Float` | 新規 | R-math-01 |
| `round` | `fn float.round(n: Float) -> Float` |  |  |
| `signum` | `fn float.signum(n: Float) -> Float` | 改名（旧 `sign`）。動きは変えない | R-float-05 |
| `to_bits` | `fn float.to_bits(f: Float) -> Int` |  | R-float-02（`from_bits` と対にする） |
| `to_fixed` | `fn float.to_fixed(n: Float, decimals: Int) -> String` |  |  |
| `to_float32` | `fn float.to_float32(n: Float) -> Float32` |  | R-sized-01（行列のセル） |
| `to_float32_checked` | `fn float.to_float32_checked(n: Float) -> Float32?` |  | R-sized-01（行列のセル） |
| `to_float64` | `fn float.to_float64(n: Float) -> Float64` |  | R-sized-01（行列のセル） |
| `to_int` | `fn float.to_int(n: Float) -> Int` |  | R-sized-01（行列のセル） |
| `to_int16` | `fn float.to_int16(n: Float) -> Int16` |  | R-sized-01（行列のセル） |
| `to_int16_checked` | `fn float.to_int16_checked(n: Float) -> Int16?` |  | R-sized-01（行列のセル） |
| `to_int32` | `fn float.to_int32(n: Float) -> Int32` |  | R-sized-01（行列のセル） |
| `to_int32_checked` | `fn float.to_int32_checked(n: Float) -> Int32?` |  | R-sized-01（行列のセル） |
| `to_int64` | `fn float.to_int64(n: Float) -> Int64` |  | R-sized-01（行列のセル） |
| `to_int64_checked` | `fn float.to_int64_checked(n: Float) -> Int64?` |  | R-sized-01（行列のセル） |
| `to_int8` | `fn float.to_int8(n: Float) -> Int8` |  | R-sized-01（行列のセル） |
| `to_int8_checked` | `fn float.to_int8_checked(n: Float) -> Int8?` |  | R-sized-01（行列のセル） |
| `to_int_checked` | `fn float.to_int_checked(x: Float) -> Int?` | 新規 | R-sized-01 |
| `to_string` | `fn float.to_string(n: Float) -> String` |  |  |
| `to_uint16` | `fn float.to_uint16(n: Float) -> UInt16` |  | R-sized-01（行列のセル） |
| `to_uint16_checked` | `fn float.to_uint16_checked(n: Float) -> UInt16?` |  | R-sized-01（行列のセル） |
| `to_uint32` | `fn float.to_uint32(n: Float) -> UInt32` |  | R-sized-01（行列のセル） |
| `to_uint32_checked` | `fn float.to_uint32_checked(n: Float) -> UInt32?` |  | R-sized-01（行列のセル） |
| `to_uint64` | `fn float.to_uint64(n: Float) -> UInt64` |  | R-sized-01（行列のセル） |
| `to_uint64_checked` | `fn float.to_uint64_checked(n: Float) -> UInt64?` |  | R-sized-01（行列のセル） |
| `to_uint8` | `fn float.to_uint8(n: Float) -> UInt8` |  | R-sized-01（行列のセル） |
| `to_uint8_checked` | `fn float.to_uint8_checked(n: Float) -> UInt8?` |  | R-sized-01（行列のセル） |
| `trunc` | `fn float.trunc(n: Float) -> Float` | 新規 | R-float-04 |

撤去（12）:

| 関数 | 代わり | 案 |
|---|---|---|
| `from_float32` | `float32.to_float` | R-float-01（`float32.to_float`） |
| `from_float64` | `float64.to_float` | R-float-01（`float64.to_float`） |
| `from_int` | `int.to_float` と同じ | R-float-01（`int.to_float` と同じ。R-L0-07 の 3） |
| `sqrt` | `math.sqrt` | R-math-03 |
| `to_int16_saturating` | `float.to_int16` が同じ飽和をする | R-sized-04（`float.to_int16` が同じ飽和をする。1 回の呼び出しだけの双子） |
| `to_int32_saturating` | `float.to_int32` が同じ飽和をする | R-sized-04（`float.to_int32` が同じ飽和をする。1 回の呼び出しだけの双子） |
| `to_int64_saturating` | `float.to_int64` が同じ飽和をする | R-sized-04（`float.to_int64` が同じ飽和をする。1 回の呼び出しだけの双子） |
| `to_int8_saturating` | `float.to_int8` が同じ飽和をする | R-sized-04（`float.to_int8` が同じ飽和をする。1 回の呼び出しだけの双子） |
| `to_uint16_saturating` | `float.to_uint16` が同じ飽和をする | R-sized-04（`float.to_uint16` が同じ飽和をする。1 回の呼び出しだけの双子） |
| `to_uint32_saturating` | `float.to_uint32` が同じ飽和をする | R-sized-04（`float.to_uint32` が同じ飽和をする。1 回の呼び出しだけの双子） |
| `to_uint64_saturating` | `float.to_uint64` が同じ飽和をする | R-sized-04（`float.to_uint64` が同じ飽和をする。1 回の呼び出しだけの双子） |
| `to_uint8_saturating` | `float.to_uint8` が同じ飽和をする | R-sized-04（`float.to_uint8` が同じ飽和をする。1 回の呼び出しだけの双子） |

## float32

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `compare` | `fn float32.compare(a: Float32, b: Float32) -> Int` | 新規 | R-sized-02 |
| `epsilon` | `fn float32.epsilon() -> Float32` | 新規 | R-sized-03 |
| `from_bits` | `fn float32.from_bits(bits: Int) -> Float32` | 新規 | R-sized-03 |
| `infinity` | `fn float32.infinity() -> Float32` | 新規 | R-sized-03 |
| `max_value` | `fn float32.max_value() -> Float32` | 新規 | R-sized-03 |
| `min_value` | `fn float32.min_value() -> Float32` | 新規 | R-sized-03 |
| `nan` | `fn float32.nan() -> Float32` | 新規 | R-sized-03 |
| `parse` | `fn float32.parse(s: String) -> Result[Float32, String]` | 新規 | R-sized-02 |
| `to_bits` | `fn float32.to_bits(x: Float32) -> Int` | 新規 | R-sized-03 |
| `to_float` | `fn float32.to_float(x: Float32) -> Float` | 新規 | R-sized-01 |
| `to_float64` | `fn float32.to_float64(x: Float32) -> Float64` |  | R-sized-01（行列のセル） |
| `to_int` | `fn float32.to_int(x: Float32) -> Int` | 新規 | R-sized-01 |
| `to_int16` | `fn float32.to_int16(x: Float32) -> Int16` |  | R-sized-01（行列のセル） |
| `to_int16_checked` | `fn float32.to_int16_checked(x: Float32) -> Int16?` | 新規 | R-sized-01 |
| `to_int32` | `fn float32.to_int32(x: Float32) -> Int32` |  | R-sized-01（行列のセル） |
| `to_int32_checked` | `fn float32.to_int32_checked(x: Float32) -> Int32?` | 新規 | R-sized-01 |
| `to_int64` | `fn float32.to_int64(x: Float32) -> Int64` |  | R-sized-01（行列のセル） |
| `to_int64_checked` | `fn float32.to_int64_checked(x: Float32) -> Int64?` | 新規 | R-sized-01 |
| `to_int8` | `fn float32.to_int8(x: Float32) -> Int8` |  | R-sized-01（行列のセル） |
| `to_int8_checked` | `fn float32.to_int8_checked(x: Float32) -> Int8?` | 新規 | R-sized-01 |
| `to_int_checked` | `fn float32.to_int_checked(x: Float32) -> Int?` | 新規 | R-sized-01 |
| `to_string` | `fn float32.to_string(x: Float32) -> String` |  |  |
| `to_uint16` | `fn float32.to_uint16(x: Float32) -> UInt16` |  | R-sized-01（行列のセル） |
| `to_uint16_checked` | `fn float32.to_uint16_checked(x: Float32) -> UInt16?` | 新規 | R-sized-01 |
| `to_uint32` | `fn float32.to_uint32(x: Float32) -> UInt32` |  | R-sized-01（行列のセル） |
| `to_uint32_checked` | `fn float32.to_uint32_checked(x: Float32) -> UInt32?` | 新規 | R-sized-01 |
| `to_uint64` | `fn float32.to_uint64(x: Float32) -> UInt64` |  | R-sized-01（行列のセル） |
| `to_uint64_checked` | `fn float32.to_uint64_checked(x: Float32) -> UInt64?` | 新規 | R-sized-01 |
| `to_uint8` | `fn float32.to_uint8(x: Float32) -> UInt8` |  | R-sized-01（行列のセル） |
| `to_uint8_checked` | `fn float32.to_uint8_checked(x: Float32) -> UInt8?` | 新規 | R-sized-01 |

## float64

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `compare` | `fn float64.compare(a: Float64, b: Float64) -> Int` | 新規 | R-sized-02 |
| `epsilon` | `fn float64.epsilon() -> Float64` | 新規 | R-sized-03 |
| `from_bits` | `fn float64.from_bits(bits: Int) -> Float64` | 新規 | R-sized-03 |
| `infinity` | `fn float64.infinity() -> Float64` | 新規 | R-sized-03 |
| `max_value` | `fn float64.max_value() -> Float64` | 新規 | R-sized-03 |
| `min_value` | `fn float64.min_value() -> Float64` | 新規 | R-sized-03 |
| `nan` | `fn float64.nan() -> Float64` | 新規 | R-sized-03 |
| `parse` | `fn float64.parse(s: String) -> Result[Float64, String]` | 新規 | R-sized-02 |
| `to_bits` | `fn float64.to_bits(x: Float64) -> Int` | 新規 | R-sized-03 |
| `to_float` | `fn float64.to_float(x: Float64) -> Float` | 新規 | R-sized-01 |
| `to_float32` | `fn float64.to_float32(x: Float64) -> Float32` |  | R-sized-01（行列のセル） |
| `to_float32_checked` | `fn float64.to_float32_checked(x: Float64) -> Float32?` | 新規 | R-sized-01 |
| `to_int` | `fn float64.to_int(x: Float64) -> Int` | 新規 | R-sized-01 |
| `to_int16` | `fn float64.to_int16(x: Float64) -> Int16` |  | R-sized-01（行列のセル） |
| `to_int16_checked` | `fn float64.to_int16_checked(x: Float64) -> Int16?` | 新規 | R-sized-01 |
| `to_int32` | `fn float64.to_int32(x: Float64) -> Int32` |  | R-sized-01（行列のセル） |
| `to_int32_checked` | `fn float64.to_int32_checked(x: Float64) -> Int32?` | 新規 | R-sized-01 |
| `to_int64` | `fn float64.to_int64(x: Float64) -> Int64` |  | R-sized-01（行列のセル） |
| `to_int64_checked` | `fn float64.to_int64_checked(x: Float64) -> Int64?` | 新規 | R-sized-01 |
| `to_int8` | `fn float64.to_int8(x: Float64) -> Int8` |  | R-sized-01（行列のセル） |
| `to_int8_checked` | `fn float64.to_int8_checked(x: Float64) -> Int8?` | 新規 | R-sized-01 |
| `to_int_checked` | `fn float64.to_int_checked(x: Float64) -> Int?` | 新規 | R-sized-01 |
| `to_string` | `fn float64.to_string(x: Float64) -> String` |  |  |
| `to_uint16` | `fn float64.to_uint16(x: Float64) -> UInt16` |  | R-sized-01（行列のセル） |
| `to_uint16_checked` | `fn float64.to_uint16_checked(x: Float64) -> UInt16?` | 新規 | R-sized-01 |
| `to_uint32` | `fn float64.to_uint32(x: Float64) -> UInt32` |  | R-sized-01（行列のセル） |
| `to_uint32_checked` | `fn float64.to_uint32_checked(x: Float64) -> UInt32?` | 新規 | R-sized-01 |
| `to_uint64` | `fn float64.to_uint64(x: Float64) -> UInt64` |  | R-sized-01（行列のセル） |
| `to_uint64_checked` | `fn float64.to_uint64_checked(x: Float64) -> UInt64?` | 新規 | R-sized-01 |
| `to_uint8` | `fn float64.to_uint8(x: Float64) -> UInt8` |  | R-sized-01（行列のセル） |
| `to_uint8_checked` | `fn float64.to_uint8_checked(x: Float64) -> UInt8?` | 新規 | R-sized-01 |

## fs

記録: [L2-fs.md](L2-fs.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `append_bytes` | `effect fn fs.append_bytes(path: String, data: Bytes) -> Unit` | 新規 | R-fs-05 |
| `append_text` | `effect fn fs.append_text(path: String, content: String) -> Unit` | 改名（旧 `append`） | R-fs-04、R-fs-05 |
| `copy` | `effect fn fs.copy(src: String, dst: String) -> Unit` |  |  |
| `create_temp_dir` | `effect fn fs.create_temp_dir(prefix: String) -> String` |  |  |
| `create_temp_file` | `effect fn fs.create_temp_file(prefix: String) -> String` |  |  |
| `exists` | `effect fn fs.exists(path: String) -> Bool` |  |  |
| `fold_lines` | `effect fn fs.fold_lines[A](path: String, init: A, f: (A, String) -> A) -> A` |  |  |
| `fold_lines_chunked` | `effect fn fs.fold_lines_chunked[A](path: String, workers: Int, init: A, f: (A, String) -> A) -> List[A]` |  |  |
| `fold_lines_range` | `effect fn fs.fold_lines_range[A](path: String, start: Int, end: Int, init: A, f: (A, String) -> A) -> A` |  |  |
| `for_each_line` | `effect fn fs.for_each_line(path: String, f: (String) -> Unit) -> Unit` |  |  |
| `glob` | `effect fn fs.glob(pattern: String) -> List[String]` |  |  |
| `is_dir` | `effect fn fs.is_dir(path: String) -> Bool` |  |  |
| `is_file` | `effect fn fs.is_file(path: String) -> Bool` |  |  |
| `is_symlink` | `effect fn fs.is_symlink(path: String) -> Bool` |  |  |
| `list_dir` | `effect fn fs.list_dir(path: String) -> List[String]` |  |  |
| `mkdir_p` | `effect fn fs.mkdir_p(path: String) -> Unit` |  | R-fs-04 |
| `read_bytes` | `effect fn fs.read_bytes(path: String) -> Bytes` | 意味変更: `-> Bytes` にする。唯一のバイト読み | R-fs-01 |
| `read_bytes_if_exists` | `effect fn fs.read_bytes_if_exists(path: String) -> Bytes?` | 意味変更: `-> Bytes?` にする | R-fs-01・R-fs-03 |
| `read_lines` | `effect fn fs.read_lines(path: String) -> List[String]` |  |  |
| `read_lines_if_exists` | `effect fn fs.read_lines_if_exists(path: String) -> List[String]?` |  | R-fs-03 |
| `read_text` | `effect fn fs.read_text(path: String) -> String` |  |  |
| `read_text_if_exists` | `effect fn fs.read_text_if_exists(path: String) -> String?` |  | R-fs-03 |
| `remove` | `effect fn fs.remove(path: String) -> Unit` |  |  |
| `remove_all` | `effect fn fs.remove_all(path: String) -> Unit` |  |  |
| `rename` | `effect fn fs.rename(src: String, dst: String) -> Unit` |  |  |
| `stat` | `effect fn fs.stat(path: String) -> FileStat` |  | R-fs-02 |
| `temp_dir` | `effect fn fs.temp_dir() -> String` |  | L2-env R-env-01 |
| `walk` | `effect fn fs.walk(dir: String) -> List[String]` |  |  |
| `write_bytes` | `effect fn fs.write_bytes(path: String, data: Bytes) -> Unit` | 意味変更: `data: Bytes` にする | R-fs-01 |
| `write_text` | `effect fn fs.write_text(path: String, content: String) -> Unit` | 改名（旧 `write`） | R-fs-05 |

撤去（5）:

| 関数 | 代わり | 案 |
|---|---|---|
| `file_size` | `stat(p)!.size` | R-fs-02 |
| `modified_at` | `stat(p)!.modified` | R-fs-02 |
| `read_bytes_raw` | `read_bytes` | R-fs-01 |
| `read_bytes_raw_if_exists` | `read_bytes_if_exists` | R-fs-01・R-fs-03 |
| `write_bytes_raw` | `write_bytes` | R-fs-01 |

## hash

記録: [L2-hash.md](L2-hash.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `adler32` | `fn hash.adler32(b: Bytes) -> Int` | 新規 | R-hash-02 |
| `crc32` | `fn hash.crc32(b: Bytes) -> Int` | 新規 | R-hash-02 |
| `fnv1a32` | `fn hash.fnv1a32(data: Bytes) -> Int` | 意味変更: 引数を `Bytes` に。`fnv1a32(bytes.from_string(s))` | R-hash-01 |
| `md5` | `fn hash.md5(b: Bytes) -> Bytes` | 新規 | R-hash-03 |
| `md5_hex` | `fn hash.md5_hex(b: Bytes) -> String` | 新規 | R-hash-03、R-hash-05 |
| `sha1` | `fn hash.sha1(b: Bytes) -> Bytes` | 新規 | R-hash-03 |
| `sha1_hex` | `fn hash.sha1_hex(b: Bytes) -> String` | 新規 | R-hash-03、R-hash-05 |
| `sha256` | `fn hash.sha256(b: Bytes) -> Bytes` |  |  |
| `sha256_hex` | `fn hash.sha256_hex(b: Bytes) -> String` | 意味変更: `Bytes` を受け取る | R-hash-05 |
| `sha512` | `fn hash.sha512(b: Bytes) -> Bytes` | 新規 | R-hash-03 |
| `sha512_hex` | `fn hash.sha512_hex(b: Bytes) -> String` | 新規 | R-hash-03、R-hash-05 |

撤去（1）:

| 関数 | 代わり | 案 |
|---|---|---|
| `fnv1a32_bytes` | `fnv1a32` | R-hash-01 |

## heap

新しいモジュールです（L1 で追加）。
記録: [L2-heap.md](L2-heap.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `all` | `fn heap.all[A](h: Heap[A], f: (A) -> Bool) -> Bool` | 新規 | R-heap-02 |
| `any` | `fn heap.any[A](h: Heap[A], f: (A) -> Bool) -> Bool` | 新規 | R-heap-02 |
| `clear` | `fn heap.clear[A](mut h: Heap[A]) -> Unit` | 新規 | R-heap-02、R-L0-04 |
| `contains` | `fn heap.contains[A](h: Heap[A], x: A) -> Bool` | 新規 | R-heap-02 |
| `count` | `fn heap.count[A](h: Heap[A], f: (A) -> Bool) -> Int` | 新規 | R-heap-02 |
| `filter` | `fn heap.filter[A](h: Heap[A], f: (A) -> Bool) -> Heap[A]` | 新規 | R-heap-02 |
| `from_list` | `fn heap.from_list[A](xs: List[A]) -> Heap[A]` | 新規 | R-heap-01 |
| `is_empty` | `fn heap.is_empty[A](h: Heap[A]) -> Bool` | 新規 | R-heap-01 |
| `len` | `fn heap.len[A](h: Heap[A]) -> Int` | 新規 | R-heap-01 |
| `new` | `fn heap.new[A]() -> Heap[A]` | 新規 | R-heap-01 |
| `new_by` | `fn heap.new_by[A, K](key: (A) -> K) -> Heap[A]` | 新規 | R-heap-01 |
| `new_with` | `fn heap.new_with[A](cmp: (A, A) -> Int) -> Heap[A]` | 新規 | R-heap-01 |
| `peek` | `fn heap.peek[A](h: Heap[A]) -> A?` | 新規 | R-heap-01 |
| `pop` | `fn heap.pop[A](mut h: Heap[A]) -> A?` | 新規 | R-heap-01 |
| `push` | `fn heap.push[A](mut h: Heap[A], x: A) -> Unit` | 新規 | R-heap-01 |
| `to_list` | `fn heap.to_list[A](h: Heap[A]) -> List[A]` | 新規 | R-heap-01 |

## hex

記録: [L2-hex.md](L2-hex.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `decode` | `fn hex.decode(s: String) -> Result[Bytes, String]` |  | R-hex-01 |
| `encode` | `fn hex.encode(b: Bytes) -> String` |  | R-hex-01 |
| `encode_upper` | `fn hex.encode_upper(b: Bytes) -> String` |  | R-hex-01 |

## html

記録: [L2-html.md](L2-html.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `concat` | `fn html.concat(a: SafeHtml, b: SafeHtml) -> SafeHtml` |  | R-html-01 |
| `empty` | `fn html.empty() -> SafeHtml` |  | R-html-01 |
| `escape` | `fn html.escape(s: String) -> SafeHtml` |  | R-html-01 |
| `raw` | `fn html.raw(s: String) -> SafeHtml` |  | R-html-01 |
| `to_string` | `fn html.to_string(h: SafeHtml) -> String` |  | R-html-01 |

## http

記録: [L2-http.md](L2-http.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `body` | `fn http.body(resp: HttpResponse) -> String` |  |  |
| `body_bytes` | `fn http.body_bytes(resp: HttpResponse) -> Bytes` | 新規 | R-http-01 |
| `cancel` | `effect fn http.cancel(c: HttpCall) -> Unit` |  | R-http-01 |
| `decode_json` | `fn http.decode_json[T](req: HttpRequest, decode: (Value) -> Result[T, String]) -> Result[T, HttpResponse]` |  |  |
| `get` | `effect fn http.get(url: String) -> String` |  | R-http-01 |
| `get_header` | `fn http.get_header(resp: HttpResponse, key: String) -> String?` |  |  |
| `get_response` | `effect fn http.get_response(url: String) -> HttpResponse` |  | R-http-01 |
| `header_values` | `fn http.header_values(resp: HttpResponse, key: String) -> List[String]` |  |  |
| `headers` | `fn http.headers(resp: HttpResponse) -> List[(String, String)]` | 意味変更: `List[(String, String)]` を返す | R-http-09 |
| `json` | `fn http.json(status: Int, body: String) -> HttpResponse` |  |  |
| `mount` | `fn http.mount(prefix: String, sub: (HttpRequest) -> Result[HttpResponse, String]) -> HttpRoute` |  |  |
| `new_request` | `fn http.new_request(method: String, target: String, body: String, headers: List[(String, String)]) -> HttpRequest` | 意味変更: `headers` を `List[(String, String)]` に | R-http-09 |
| `param` | `fn http.param(req: HttpRequest, name: String) -> String?` |  |  |
| `poll` | `effect fn http.poll(c: HttpCall) -> Result[HttpResponse, String]?` |  | R-http-01 |
| `query_params` | `fn http.query_params(req: HttpRequest) -> List[(String, String)]` | 意味変更: `List[(String, String)]` を返す | R-http-10 |
| `read_new` | `effect fn http.read_new(c: HttpCall) -> String` |  | R-http-01 |
| `redirect` | `fn http.redirect(url: String) -> HttpResponse` |  |  |
| `req_body` | `fn http.req_body(req: HttpRequest) -> String` |  |  |
| `req_body_bytes` | `fn http.req_body_bytes(req: HttpRequest) -> Bytes` | 新規 | R-http-11 |
| `req_header` | `fn http.req_header(req: HttpRequest, key: String) -> String?` |  | R-http-03 |
| `req_header_values` | `fn http.req_header_values(req: HttpRequest, key: String) -> List[String]` | 新規 | R-http-03 |
| `req_headers` | `fn http.req_headers(req: HttpRequest) -> List[(String, String)]` | 新規 | R-http-03、R-http-09 |
| `req_method` | `fn http.req_method(req: HttpRequest) -> String` |  |  |
| `req_path` | `fn http.req_path(req: HttpRequest) -> String` |  |  |
| `request` | `effect fn http.request(method: String, url: String, body: String, headers: List[(String, String)]) -> String` | 意味変更: `headers` を `List[(String, String)]` に | R-http-09 |
| `request_response` | `effect fn http.request_response(method: String, url: String, body: String, headers: List[(String, String)]) -> HttpResponse` | 意味変更: `headers` を `List[(String, String)]` に | R-http-09 |
| `response` | `fn http.response(status: Int, body: String) -> HttpResponse` |  |  |
| `route` | `fn http.route(pattern: String, handler: (HttpRequest) -> Result[HttpResponse, String]) -> HttpRoute` |  |  |
| `router` | `fn http.router(routes: List[HttpRoute]) -> Result[(HttpRequest) -> Result[HttpResponse, String], String]` |  |  |
| `serve` | `effect fn http.serve(port: Int, f: effect (HttpRequest) -> HttpResponse) -> Unit` |  |  |
| `set_body` | `fn http.set_body(resp: HttpResponse, body: String) -> HttpResponse` | 新規 | R-http-06 |
| `set_body_bytes` | `fn http.set_body_bytes(resp: HttpResponse, body: Bytes) -> HttpResponse` | 新規 | R-http-11 |
| `set_header` | `fn http.set_header(resp: HttpResponse, key: String, value: String) -> HttpResponse` |  | R-http-02 |
| `set_status` | `fn http.set_status(resp: HttpResponse, code: Int) -> HttpResponse` | 改名（旧 `status`）。`set_status` | R-http-02 |
| `start` | `effect fn http.start(method: String, url: String, body: String, headers: List[(String, String)], limits: HttpLimits) -> HttpCall` | 意味変更: `headers` を `List[(String, String)]` に | R-http-09 |
| `status` | `fn http.status(resp: HttpResponse) -> Int` | 改名（旧 `status_code`）。epoch 17。`0` の番兵もなくす | R-http-04、R-http-08 |
| `wait` | `effect fn http.wait(c: HttpCall) -> HttpResponse` |  | R-http-01 |
| `with_headers` | `fn http.with_headers(status: Int, body: String, headers: List[(String, String)]) -> HttpResponse` | 意味変更: `headers` を `List[(String, String)]` に | R-http-09 |
| `wrap` | `fn http.wrap(handler: HttpHandler, middleware: List[HttpMiddleware]) -> HttpHandler` |  |  |

撤去（15）:

| 関数 | 代わり | 案 |
|---|---|---|
| `delete` | `request("DELETE", …)` | R-http-01 |
| `delete_response` | `request_response("DELETE", …)` | R-http-01 |
| `get_bytes` | `body_bytes(get_response(u)!)` | R-http-01 |
| `get_status` | `get_response` | R-http-01 |
| `patch` | `request("PATCH", …)` | R-http-01 |
| `patch_response` | `request_response("PATCH", …)` | R-http-01 |
| `post` | `request("POST", …)` | R-http-01 |
| `post_response` | `request_response("POST", …)` | R-http-01 |
| `put` | `request("PUT", …)` | R-http-01 |
| `put_response` | `request_response("PUT", …)` | R-http-01 |
| `request_bytes` | `body_bytes(request_response(…)!)` | R-http-01 |
| `request_status` | `request_response` | R-http-01 |
| `request_stream` | ハンドル | R-http-01 |
| `request_stream_with_limits` | ハンドル | R-http-01 |
| `url_decode` | `url.decode_form(s) ?? s` | R-http-04 |

## int

記録: [L2-int.md](L2-int.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `abs` | `fn int.abs(n: Int) -> Int` |  | R-math-01（`math.abs` の残る側） |
| `band` | `fn int.band(a: Int, b: Int) -> Int` |  | R-int-05 ×（改名しない） |
| `bit_reverse` | `fn int.bit_reverse(n: Int) -> Int` |  |  |
| `bit_width` | `fn int.bit_width(n: Int) -> Int` |  |  |
| `bnot` | `fn int.bnot(a: Int) -> Int` |  | R-int-05 × |
| `bor` | `fn int.bor(a: Int, b: Int) -> Int` |  | R-int-05 × |
| `bshl` | `fn int.bshl(a: Int, n: Int) -> Int` |  | R-int-05 × |
| `bshr` | `fn int.bshr(a: Int, n: Int) -> Int` |  | R-int-05 × |
| `bshr_logical` | `fn int.bshr_logical(a: Int, n: Int) -> Int` | 新規 | R-int-06 |
| `bxor` | `fn int.bxor(a: Int, b: Int) -> Int` |  | R-int-05 × |
| `byte_swap` | `fn int.byte_swap(n: Int) -> Int` |  |  |
| `clamp` | `fn int.clamp(n: Int, lo: Int, hi: Int) -> Int` |  |  |
| `compare` | `fn int.compare(a: Int, b: Int) -> Int` | 新規 | R-sized-02 |
| `count_leading_zeros` | `fn int.count_leading_zeros(n: Int) -> Int` |  |  |
| `count_ones` | `fn int.count_ones(n: Int) -> Int` | 改名（旧 `pop_count`） | R-int-07 |
| `count_trailing_zeros` | `fn int.count_trailing_zeros(n: Int) -> Int` |  |  |
| `floor_div` | `fn int.floor_div(a: Int, b: Int) -> Int` | 新規 | R-int-03 |
| `is_even` | `fn int.is_even(n: Int) -> Bool` |  |  |
| `is_odd` | `fn int.is_odd(n: Int) -> Bool` |  |  |
| `log2_ceil` | `fn int.log2_ceil(n: Int) -> Int` |  |  |
| `log2_floor` | `fn int.log2_floor(n: Int) -> Int` |  |  |
| `max` | `fn int.max(a: Int, b: Int) -> Int` |  | R-math-01（`math.max` の残る側） |
| `max_value` | `fn int.max_value() -> Int` |  |  |
| `min` | `fn int.min(a: Int, b: Int) -> Int` |  | R-math-01（`math.min` の残る側） |
| `min_value` | `fn int.min_value() -> Int` |  |  |
| `mod` | `fn int.mod(a: Int, b: Int) -> Int` | 新規 | R-int-03 |
| `next_power_of_two` | `fn int.next_power_of_two(n: Int) -> Int` |  |  |
| `parse` | `fn int.parse(s: String) -> Result[Int, String]` |  |  |
| `parse_radix` | `fn int.parse_radix(s: String, radix: Int) -> Result[Int, String]` | 新規 | R-int-04 |
| `pow` | `fn int.pow(base: Int, exp: Int) -> Int` | 新規 | R-math-01 |
| `prev_power_of_two` | `fn int.prev_power_of_two(n: Int) -> Int` |  |  |
| `rotate_left` | `fn int.rotate_left(a: Int, n: Int, bits: Int) -> Int` |  |  |
| `rotate_right` | `fn int.rotate_right(a: Int, n: Int, bits: Int) -> Int` |  |  |
| `sign` | `fn int.sign(n: Int) -> Int` | 新規 | R-math-01 |
| `to_bigint` | `fn int.to_bigint(n: Int) -> BigInt` | 新規 | R-bigint-01（`int` に置く。L2-int の表にも載せる） |
| `to_float` | `fn int.to_float(n: Int) -> Float` |  |  |
| `to_float32` | `fn int.to_float32(n: Int) -> Float32` |  | R-sized-01（行列のセル） |
| `to_float32_checked` | `fn int.to_float32_checked(n: Int) -> Float32?` |  | R-sized-01（行列のセル） |
| `to_float64` | `fn int.to_float64(n: Int) -> Float64` |  | R-sized-01（行列のセル） |
| `to_float64_checked` | `fn int.to_float64_checked(x: Int) -> Float64?` | 新規 | R-sized-01 |
| `to_float_checked` | `fn int.to_float_checked(x: Int) -> Float?` | 新規 | R-sized-01 |
| `to_hex` | `fn int.to_hex(n: Int) -> String` |  | R-int-04（64 bit の並びの 16 進。符号付きの基数変換は `to_string_radix`） |
| `to_int16` | `fn int.to_int16(n: Int) -> Int16` |  | R-sized-01（行列のセル） |
| `to_int16_checked` | `fn int.to_int16_checked(n: Int) -> Int16?` |  | R-sized-01（行列のセル） |
| `to_int16_saturating` | `fn int.to_int16_saturating(n: Int) -> Int16` |  | R-sized-01（行列のセル） |
| `to_int32` | `fn int.to_int32(n: Int) -> Int32` |  | R-sized-01（行列のセル） |
| `to_int32_checked` | `fn int.to_int32_checked(n: Int) -> Int32?` |  | R-sized-01（行列のセル） |
| `to_int32_saturating` | `fn int.to_int32_saturating(n: Int) -> Int32` |  | R-sized-01（行列のセル） |
| `to_int64` | `fn int.to_int64(n: Int) -> Int64` |  | R-sized-01（行列のセル） |
| `to_int8` | `fn int.to_int8(n: Int) -> Int8` |  | R-sized-01（行列のセル） |
| `to_int8_checked` | `fn int.to_int8_checked(n: Int) -> Int8?` |  | R-sized-01（行列のセル） |
| `to_int8_saturating` | `fn int.to_int8_saturating(n: Int) -> Int8` |  | R-sized-01（行列のセル） |
| `to_string` | `fn int.to_string(n: Int) -> String` |  |  |
| `to_string_radix` | `fn int.to_string_radix(n: Int, radix: Int) -> String` | 新規 | R-int-04 |
| `to_uint16` | `fn int.to_uint16(n: Int) -> UInt16` |  | R-sized-01（行列のセル） |
| `to_uint16_checked` | `fn int.to_uint16_checked(n: Int) -> UInt16?` |  | R-sized-01（行列のセル） |
| `to_uint16_saturating` | `fn int.to_uint16_saturating(n: Int) -> UInt16` |  | R-sized-01（行列のセル） |
| `to_uint32` | `fn int.to_uint32(n: Int) -> UInt32` |  | R-sized-01（行列のセル） |
| `to_uint32_checked` | `fn int.to_uint32_checked(n: Int) -> UInt32?` |  | R-sized-01（行列のセル） |
| `to_uint32_saturating` | `fn int.to_uint32_saturating(n: Int) -> UInt32` |  | R-sized-01（行列のセル） |
| `to_uint64` | `fn int.to_uint64(n: Int) -> UInt64` |  | R-sized-01（行列のセル） |
| `to_uint64_checked` | `fn int.to_uint64_checked(n: Int) -> UInt64?` |  | R-sized-01（行列のセル） |
| `to_uint64_saturating` | `fn int.to_uint64_saturating(n: Int) -> UInt64` |  | R-sized-01（行列のセル） |
| `to_uint8` | `fn int.to_uint8(n: Int) -> UInt8` |  | R-sized-01（行列のセル） |
| `to_uint8_checked` | `fn int.to_uint8_checked(n: Int) -> UInt8?` |  | R-sized-01（行列のセル） |
| `to_uint8_saturating` | `fn int.to_uint8_saturating(n: Int) -> UInt8` |  | R-sized-01（行列のセル） |
| `wrap` | `fn int.wrap(n: Int, bits: Int) -> Int` | 新規 | R-int-02 |
| `wrap_add` | `fn int.wrap_add(a: Int, b: Int, bits: Int) -> Int` | 意味変更: `bits <= 0` は abort | R-int-02（`bits <= 0` を abort にそろえる。値は変えない） |
| `wrap_mul` | `fn int.wrap_mul(a: Int, b: Int, bits: Int) -> Int` | 意味変更: `bits <= 0` は abort | R-int-02（同上） |
| `wrap_sub` | `fn int.wrap_sub(a: Int, b: Int, bits: Int) -> Int` | 新規 | R-int-02 |

撤去（15）:

| 関数 | 代わり | 案 |
|---|---|---|
| `bits_to_f32` | `float32.from_bits` → `float32.to_float` | R-float-02（`float32.from_bits` → `float32.to_float`） |
| `bits_to_float` | `float.from_bits` | R-float-02（`float.from_bits`） |
| `from_hex` | `int.parse_radix(s, 16)` に一本化 | R-int-04（`int.parse_radix(s, 16)` に一本化） |
| `from_int16` | `int16.to_int` に移す | R-sized-01（`int16.to_int` に移す。R-L0-07） |
| `from_int32` | `int32.to_int` に移す | R-sized-01（`int32.to_int` に移す。R-L0-07） |
| `from_int64` | `int64.to_int` に移す | R-sized-01（`int64.to_int` に移す。R-L0-07） |
| `from_int8` | `int8.to_int` に移す | R-sized-01（`int8.to_int` に移す。R-L0-07） |
| `from_uint16` | `uint16.to_int` に移す | R-sized-01（`uint16.to_int` に移す。R-L0-07） |
| `from_uint32` | `uint32.to_int` に移す | R-sized-01（`uint32.to_int` に移す。R-L0-07） |
| `from_uint64` | `uint64.to_int` に移す | R-sized-01（`uint64.to_int` に移す。R-L0-07） |
| `from_uint64_checked` | `uint64.to_int_checked` | R-sized-01（`uint64.to_int_checked`） |
| `from_uint64_saturating` | `uint64.to_int_saturating` | R-sized-01（`uint64.to_int_saturating`） |
| `from_uint8` | `uint8.to_int` に移す | R-sized-01（`uint8.to_int` に移す。R-L0-07） |
| `to_u32` | `int.wrap(x, 32)` | R-int-02（`int.wrap(x, 32)`。#3851 E） |
| `to_u8` | `int.wrap(x, 8)` | R-int-02（`int.wrap(x, 8)`。#3851 E） |

## int16

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `compare` | `fn int16.compare(a: Int16, b: Int16) -> Int` | 新規 | R-sized-02 |
| `max_value` | `fn int16.max_value() -> Int16` |  |  |
| `min_value` | `fn int16.min_value() -> Int16` |  |  |
| `parse` | `fn int16.parse(s: String) -> Result[Int16, String]` | 新規 | R-sized-02 |
| `parse_radix` | `fn int16.parse_radix(s: String, radix: Int) -> Result[Int16, String]` | 新規 | R-sized-06 |
| `to_float` | `fn int16.to_float(x: Int16) -> Float` | 新規 | R-sized-01 |
| `to_float32` | `fn int16.to_float32(x: Int16) -> Float32` |  | R-sized-01（行列のセル） |
| `to_float64` | `fn int16.to_float64(x: Int16) -> Float64` |  | R-sized-01（行列のセル） |
| `to_int` | `fn int16.to_int(x: Int16) -> Int` | 新規 | R-sized-01 |
| `to_int32` | `fn int16.to_int32(x: Int16) -> Int32` |  | R-sized-01（行列のセル） |
| `to_int64` | `fn int16.to_int64(x: Int16) -> Int64` |  | R-sized-01（行列のセル） |
| `to_int8` | `fn int16.to_int8(x: Int16) -> Int8` |  | R-sized-01（行列のセル） |
| `to_int8_checked` | `fn int16.to_int8_checked(x: Int16) -> Int8?` |  | R-sized-01（行列のセル） |
| `to_int8_saturating` | `fn int16.to_int8_saturating(x: Int16) -> Int8` |  | R-sized-01（行列のセル） |
| `to_string` | `fn int16.to_string(x: Int16) -> String` |  |  |
| `to_string_radix` | `fn int16.to_string_radix(x: Int16, radix: Int) -> String` | 新規 | R-sized-06 |
| `to_uint16` | `fn int16.to_uint16(x: Int16) -> UInt16` |  | R-sized-01（行列のセル） |
| `to_uint16_checked` | `fn int16.to_uint16_checked(x: Int16) -> UInt16?` |  | R-sized-01（行列のセル） |
| `to_uint16_saturating` | `fn int16.to_uint16_saturating(x: Int16) -> UInt16` |  | R-sized-01（行列のセル） |
| `to_uint32` | `fn int16.to_uint32(x: Int16) -> UInt32` |  | R-sized-01（行列のセル） |
| `to_uint32_checked` | `fn int16.to_uint32_checked(x: Int16) -> UInt32?` |  | R-sized-01（行列のセル） |
| `to_uint32_saturating` | `fn int16.to_uint32_saturating(x: Int16) -> UInt32` |  | R-sized-01（行列のセル） |
| `to_uint64` | `fn int16.to_uint64(x: Int16) -> UInt64` |  | R-sized-01（行列のセル） |
| `to_uint64_checked` | `fn int16.to_uint64_checked(x: Int16) -> UInt64?` |  | R-sized-01（行列のセル） |
| `to_uint64_saturating` | `fn int16.to_uint64_saturating(x: Int16) -> UInt64` |  | R-sized-01（行列のセル） |
| `to_uint8` | `fn int16.to_uint8(x: Int16) -> UInt8` |  | R-sized-01（行列のセル） |
| `to_uint8_checked` | `fn int16.to_uint8_checked(x: Int16) -> UInt8?` |  | R-sized-01（行列のセル） |
| `to_uint8_saturating` | `fn int16.to_uint8_saturating(x: Int16) -> UInt8` |  | R-sized-01（行列のセル） |

## int32

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `compare` | `fn int32.compare(a: Int32, b: Int32) -> Int` | 新規 | R-sized-02 |
| `max_value` | `fn int32.max_value() -> Int32` |  |  |
| `min_value` | `fn int32.min_value() -> Int32` |  |  |
| `parse` | `fn int32.parse(s: String) -> Result[Int32, String]` | 新規 | R-sized-02 |
| `parse_radix` | `fn int32.parse_radix(s: String, radix: Int) -> Result[Int32, String]` | 新規 | R-sized-06 |
| `to_float` | `fn int32.to_float(x: Int32) -> Float` | 新規 | R-sized-01 |
| `to_float32` | `fn int32.to_float32(x: Int32) -> Float32` |  | R-sized-01（行列のセル） |
| `to_float32_checked` | `fn int32.to_float32_checked(x: Int32) -> Float32?` | 新規 | R-sized-01 |
| `to_float64` | `fn int32.to_float64(x: Int32) -> Float64` |  | R-sized-01（行列のセル） |
| `to_int` | `fn int32.to_int(x: Int32) -> Int` | 新規 | R-sized-01 |
| `to_int16` | `fn int32.to_int16(x: Int32) -> Int16` |  | R-sized-01（行列のセル） |
| `to_int16_checked` | `fn int32.to_int16_checked(x: Int32) -> Int16?` |  | R-sized-01（行列のセル） |
| `to_int16_saturating` | `fn int32.to_int16_saturating(x: Int32) -> Int16` |  | R-sized-01（行列のセル） |
| `to_int64` | `fn int32.to_int64(x: Int32) -> Int64` |  | R-sized-01（行列のセル） |
| `to_int8` | `fn int32.to_int8(x: Int32) -> Int8` |  | R-sized-01（行列のセル） |
| `to_int8_checked` | `fn int32.to_int8_checked(x: Int32) -> Int8?` |  | R-sized-01（行列のセル） |
| `to_int8_saturating` | `fn int32.to_int8_saturating(x: Int32) -> Int8` |  | R-sized-01（行列のセル） |
| `to_string` | `fn int32.to_string(x: Int32) -> String` |  |  |
| `to_string_radix` | `fn int32.to_string_radix(x: Int32, radix: Int) -> String` | 新規 | R-sized-06 |
| `to_uint16` | `fn int32.to_uint16(x: Int32) -> UInt16` |  | R-sized-01（行列のセル） |
| `to_uint16_checked` | `fn int32.to_uint16_checked(x: Int32) -> UInt16?` |  | R-sized-01（行列のセル） |
| `to_uint16_saturating` | `fn int32.to_uint16_saturating(x: Int32) -> UInt16` |  | R-sized-01（行列のセル） |
| `to_uint32` | `fn int32.to_uint32(x: Int32) -> UInt32` |  | R-sized-01（行列のセル） |
| `to_uint32_checked` | `fn int32.to_uint32_checked(x: Int32) -> UInt32?` |  | R-sized-01（行列のセル） |
| `to_uint32_saturating` | `fn int32.to_uint32_saturating(x: Int32) -> UInt32` |  | R-sized-01（行列のセル） |
| `to_uint64` | `fn int32.to_uint64(x: Int32) -> UInt64` |  | R-sized-01（行列のセル） |
| `to_uint64_checked` | `fn int32.to_uint64_checked(x: Int32) -> UInt64?` |  | R-sized-01（行列のセル） |
| `to_uint64_saturating` | `fn int32.to_uint64_saturating(x: Int32) -> UInt64` |  | R-sized-01（行列のセル） |
| `to_uint8` | `fn int32.to_uint8(x: Int32) -> UInt8` |  | R-sized-01（行列のセル） |
| `to_uint8_checked` | `fn int32.to_uint8_checked(x: Int32) -> UInt8?` |  | R-sized-01（行列のセル） |
| `to_uint8_saturating` | `fn int32.to_uint8_saturating(x: Int32) -> UInt8` |  | R-sized-01（行列のセル） |

## int64

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `compare` | `fn int64.compare(a: Int64, b: Int64) -> Int` | 新規 | R-sized-02 |
| `max_value` | `fn int64.max_value() -> Int64` |  |  |
| `min_value` | `fn int64.min_value() -> Int64` |  |  |
| `parse` | `fn int64.parse(s: String) -> Result[Int64, String]` | 新規 | R-sized-02 |
| `parse_radix` | `fn int64.parse_radix(s: String, radix: Int) -> Result[Int64, String]` | 新規 | R-sized-06 |
| `to_float` | `fn int64.to_float(x: Int64) -> Float` | 新規 | R-sized-01 |
| `to_float32` | `fn int64.to_float32(x: Int64) -> Float32` |  | R-sized-01（行列のセル） |
| `to_float32_checked` | `fn int64.to_float32_checked(x: Int64) -> Float32?` | 新規 | R-sized-01 |
| `to_float64` | `fn int64.to_float64(x: Int64) -> Float64` |  | R-sized-01（行列のセル） |
| `to_float64_checked` | `fn int64.to_float64_checked(x: Int64) -> Float64?` | 新規 | R-sized-01 |
| `to_float_checked` | `fn int64.to_float_checked(x: Int64) -> Float?` | 新規 | R-sized-01 |
| `to_int` | `fn int64.to_int(x: Int64) -> Int` | 新規 | R-sized-01 |
| `to_int16` | `fn int64.to_int16(x: Int64) -> Int16` |  | R-sized-01（行列のセル） |
| `to_int16_checked` | `fn int64.to_int16_checked(x: Int64) -> Int16?` |  | R-sized-01（行列のセル） |
| `to_int16_saturating` | `fn int64.to_int16_saturating(x: Int64) -> Int16` |  | R-sized-01（行列のセル） |
| `to_int32` | `fn int64.to_int32(x: Int64) -> Int32` |  | R-sized-01（行列のセル） |
| `to_int32_checked` | `fn int64.to_int32_checked(x: Int64) -> Int32?` |  | R-sized-01（行列のセル） |
| `to_int32_saturating` | `fn int64.to_int32_saturating(x: Int64) -> Int32` |  | R-sized-01（行列のセル） |
| `to_int8` | `fn int64.to_int8(x: Int64) -> Int8` |  | R-sized-01（行列のセル） |
| `to_int8_checked` | `fn int64.to_int8_checked(x: Int64) -> Int8?` |  | R-sized-01（行列のセル） |
| `to_int8_saturating` | `fn int64.to_int8_saturating(x: Int64) -> Int8` |  | R-sized-01（行列のセル） |
| `to_string` | `fn int64.to_string(x: Int64) -> String` |  |  |
| `to_string_radix` | `fn int64.to_string_radix(x: Int64, radix: Int) -> String` | 新規 | R-sized-06 |
| `to_uint16` | `fn int64.to_uint16(x: Int64) -> UInt16` |  | R-sized-01（行列のセル） |
| `to_uint16_checked` | `fn int64.to_uint16_checked(x: Int64) -> UInt16?` |  | R-sized-01（行列のセル） |
| `to_uint16_saturating` | `fn int64.to_uint16_saturating(x: Int64) -> UInt16` |  | R-sized-01（行列のセル） |
| `to_uint32` | `fn int64.to_uint32(x: Int64) -> UInt32` |  | R-sized-01（行列のセル） |
| `to_uint32_checked` | `fn int64.to_uint32_checked(x: Int64) -> UInt32?` |  | R-sized-01（行列のセル） |
| `to_uint32_saturating` | `fn int64.to_uint32_saturating(x: Int64) -> UInt32` |  | R-sized-01（行列のセル） |
| `to_uint64` | `fn int64.to_uint64(x: Int64) -> UInt64` |  | R-sized-01（行列のセル） |
| `to_uint64_checked` | `fn int64.to_uint64_checked(x: Int64) -> UInt64?` |  | R-sized-01（行列のセル） |
| `to_uint64_saturating` | `fn int64.to_uint64_saturating(x: Int64) -> UInt64` |  | R-sized-01（行列のセル） |
| `to_uint8` | `fn int64.to_uint8(x: Int64) -> UInt8` |  | R-sized-01（行列のセル） |
| `to_uint8_checked` | `fn int64.to_uint8_checked(x: Int64) -> UInt8?` |  | R-sized-01（行列のセル） |
| `to_uint8_saturating` | `fn int64.to_uint8_saturating(x: Int64) -> UInt8` |  | R-sized-01（行列のセル） |

## int8

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `compare` | `fn int8.compare(a: Int8, b: Int8) -> Int` | 新規 | R-sized-02 |
| `max_value` | `fn int8.max_value() -> Int8` |  |  |
| `min_value` | `fn int8.min_value() -> Int8` |  |  |
| `parse` | `fn int8.parse(s: String) -> Result[Int8, String]` | 新規 | R-sized-02 |
| `parse_radix` | `fn int8.parse_radix(s: String, radix: Int) -> Result[Int8, String]` | 新規 | R-sized-06 |
| `to_float` | `fn int8.to_float(x: Int8) -> Float` | 新規 | R-sized-01 |
| `to_float32` | `fn int8.to_float32(x: Int8) -> Float32` |  | R-sized-01（行列のセル） |
| `to_float64` | `fn int8.to_float64(x: Int8) -> Float64` |  | R-sized-01（行列のセル） |
| `to_int` | `fn int8.to_int(x: Int8) -> Int` | 新規 | R-sized-01 |
| `to_int16` | `fn int8.to_int16(x: Int8) -> Int16` |  | R-sized-01（行列のセル） |
| `to_int32` | `fn int8.to_int32(x: Int8) -> Int32` |  | R-sized-01（行列のセル） |
| `to_int64` | `fn int8.to_int64(x: Int8) -> Int64` |  | R-sized-01（行列のセル） |
| `to_string` | `fn int8.to_string(x: Int8) -> String` |  |  |
| `to_string_radix` | `fn int8.to_string_radix(x: Int8, radix: Int) -> String` | 新規 | R-sized-06 |
| `to_uint16` | `fn int8.to_uint16(x: Int8) -> UInt16` |  | R-sized-01（行列のセル） |
| `to_uint16_checked` | `fn int8.to_uint16_checked(x: Int8) -> UInt16?` |  | R-sized-01（行列のセル） |
| `to_uint16_saturating` | `fn int8.to_uint16_saturating(x: Int8) -> UInt16` |  | R-sized-01（行列のセル） |
| `to_uint32` | `fn int8.to_uint32(x: Int8) -> UInt32` |  | R-sized-01（行列のセル） |
| `to_uint32_checked` | `fn int8.to_uint32_checked(x: Int8) -> UInt32?` |  | R-sized-01（行列のセル） |
| `to_uint32_saturating` | `fn int8.to_uint32_saturating(x: Int8) -> UInt32` |  | R-sized-01（行列のセル） |
| `to_uint64` | `fn int8.to_uint64(x: Int8) -> UInt64` |  | R-sized-01（行列のセル） |
| `to_uint64_checked` | `fn int8.to_uint64_checked(x: Int8) -> UInt64?` |  | R-sized-01（行列のセル） |
| `to_uint64_saturating` | `fn int8.to_uint64_saturating(x: Int8) -> UInt64` |  | R-sized-01（行列のセル） |
| `to_uint8` | `fn int8.to_uint8(x: Int8) -> UInt8` |  | R-sized-01（行列のセル） |
| `to_uint8_checked` | `fn int8.to_uint8_checked(x: Int8) -> UInt8?` |  | R-sized-01（行列のセル） |
| `to_uint8_saturating` | `fn int8.to_uint8_saturating(x: Int8) -> UInt8` |  | R-sized-01（行列のセル） |

## io

記録: [L2-io.md](L2-io.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `eprint` | `effect fn io.eprint(s: String) -> Unit` | 新規 | R-io-03 |
| `print` | `effect fn io.print(s: String) -> Unit` |  |  |
| `read_all` | `effect fn io.read_all() -> String` |  |  |
| `read_all_bytes` | `effect fn io.read_all_bytes() -> Bytes` | 新規 | R-io-04 |
| `read_bytes` | `effect fn io.read_bytes(n: Int) -> Bytes` | 新規 | R-io-01 |
| `read_line` | `effect fn io.read_line() -> String?` | 意味変更: `-> String?` にする。唯一の 1 行読み | R-io-01 |
| `write_bytes` | `effect fn io.write_bytes(data: Bytes) -> Unit` | 意味変更: `data: Bytes` にし、effect fn にする | R-io-01・R-io-02 |

撤去（4）:

| 関数 | 代わり | 案 |
|---|---|---|
| `read_byte` | `read_bytes(1)` | R-io-01 |
| `read_line_opt` | `read_line` | R-io-01 |
| `read_n_bytes` | `read_bytes(n)` | R-io-01 |
| `write` | `write_bytes` | R-io-01・R-io-02 |

## json

記録: [L2-json.md](L2-json.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `parse` | `fn json.parse(text: String) -> Result[Value, String]` | 意味変更: 後ろの余計な文字は err | R-json-02 |
| `stringify` | `fn json.stringify(v: Value) -> String` |  | R-json-03 |
| `stringify_pretty` | `fn json.stringify_pretty(j: Value) -> String` |  | R-json-03 |

撤去（12）:

| 関数 | 代わり | 案 |
|---|---|---|
| `field` | `value.key` | R-json-01 |
| `get_array` | `value.get_array` | R-json-01 |
| `get_bool` | `value.get_bool` | R-json-01 |
| `get_float` | `value.get_float` | R-json-01 |
| `get_int` | `value.get_int` | R-json-01 |
| `get_path` | `value.get_path` | R-json-01 |
| `get_string` | `value.get_string` | R-json-01 |
| `index` | `value.index`（負の添字は範囲外） | R-json-01 |
| `remove_path` | `value.remove_path` | R-json-01 |
| `root` | `value.root` | R-json-01 |
| `set_path` | `value.set_path` | R-json-01 |
| `to_map` | `value.as_object(v) \|> result.map(map.from_list)` | R-json-01 |

## list

記録: [L2-list.md](L2-list.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `all` | `fn list.all(xs: List[A], f: (A) -> Bool) -> Bool` |  |  |
| `any` | `fn list.any(xs: List[A], f: (A) -> Bool) -> Bool` |  |  |
| `binary_search` | `fn list.binary_search(xs: List[A], target: A) -> Int?` | 意味変更: 要素型を順序のある型すべてに広げる | R-list-15 |
| `binary_search_by` | `fn list.binary_search_by(xs: List[A], target: K, key: (A) -> K) -> Int?` | 新規 | R-list-17 |
| `binary_search_with` | `fn list.binary_search_with(xs: List[A], target: A, cmp: (A, A) -> Int) -> Int?` | 新規 | R-list-17 |
| `chunk` | `fn list.chunk(xs: List[A], n: Int) -> List[List[A]]` | 意味変更: `n <= 0` は abort | R-list-10 |
| `clear` | `fn list.clear(mut xs: List[A]) -> Unit` |  | R-list-00 |
| `contains` | `fn list.contains(xs: List[A], x: A) -> Bool` |  |  |
| `count` | `fn list.count(xs: List[A], f: (A) -> Bool) -> Int` |  |  |
| `dedup` | `fn list.dedup(xs: List[A]) -> List[A]` |  |  |
| `drop` | `fn list.drop(xs: List[A], n: Int) -> List[A]` |  | R-L0-12 |
| `drop_end` | `fn list.drop_end(xs: List[A], n: Int) -> List[A]` |  | R-L0-12 |
| `drop_while` | `fn list.drop_while(xs: List[A], f: (A) -> Bool) -> List[A]` |  |  |
| `enumerate` | `fn list.enumerate(xs: List[A]) -> List[(Int, A)]` |  |  |
| `filter` | `fn list.filter(xs: List[A], f: (A) -> Bool) -> List[A]` |  |  |
| `filter_map` | `fn list.filter_map(xs: List[A], f: (A) -> B?) -> List[B]` |  |  |
| `find` | `fn list.find(xs: List[A], f: (A) -> Bool) -> A?` |  |  |
| `find_index` | `fn list.find_index(xs: List[A], f: (A) -> Bool) -> Int?` |  |  |
| `find_map` | `fn list.find_map(xs: List[A], f: (A) -> B?) -> B?` |  |  |
| `first` | `fn list.first(xs: List[A]) -> A?` |  |  |
| `flat_map` | `fn list.flat_map(xs: List[A], f: (A) -> List[B]) -> List[B]` |  |  |
| `flatten` | `fn list.flatten(xss: List[List[T]]) -> List[T]` |  |  |
| `fold` | `fn list.fold(xs: List[A], init: B, f: (B, A) -> B) -> B` |  |  |
| `get` | `fn list.get(xs: List[A], i: Int) -> A?` |  |  |
| `group_by` | `fn list.group_by(xs: List[A], f: (A) -> B) -> Map[B, List[A]]` |  |  |
| `index_of` | `fn list.index_of(xs: List[A], x: A) -> Int?` |  |  |
| `insert` | `fn list.insert(xs: List[A], i: Int, val: A) -> List[A]` | 意味変更: `0..=len` の外は abort | R-list-00、R-list-13 |
| `intersperse` | `fn list.intersperse(xs: List[A], sep: A) -> List[A]` |  |  |
| `is_empty` | `fn list.is_empty(xs: List[A]) -> Bool` |  |  |
| `iterate` | `fn list.iterate(seed: T, f: (T) -> T, n: Int) -> List[T]` |  |  |
| `join` | `fn list.join(xs: List[String], sep: String) -> String` ・ `fn list.join(xs: List[Bytes], sep: Bytes) -> Bytes` | 意味変更: `List[Bytes]` と `Bytes` の区切りも受け取る | R-string-01、R-list-16 |
| `last` | `fn list.last(xs: List[A]) -> A?` |  |  |
| `len` | `fn list.len(xs: List[A]) -> Int` |  | R-L0-11 |
| `map` | `fn list.map(xs: List[A], f: (A) -> B) -> List[B]` |  |  |
| `max` | `fn list.max(xs: List[A]) -> A?` |  |  |
| `max_by` | `fn list.max_by(xs: List[A], f: (A) -> K) -> A?` | 新規 | R-list-06 |
| `max_with` | `fn list.max_with(xs: List[A], cmp: (A, A) -> Int) -> A?` | 新規 | R-list-19 |
| `min` | `fn list.min(xs: List[A]) -> A?` |  |  |
| `min_by` | `fn list.min_by(xs: List[A], f: (A) -> K) -> A?` | 新規 | R-list-06 |
| `min_with` | `fn list.min_with(xs: List[A], cmp: (A, A) -> Int) -> A?` | 新規 | R-list-19 |
| `partition` | `fn list.partition(xs: List[A], f: (A) -> Bool) -> (List[A], List[A])` |  |  |
| `pop` | `fn list.pop(mut xs: List[A]) -> A?` |  | R-list-00 |
| `product` | `fn list.product(xs: List[Int]) -> Int` ・ `fn list.product(xs: List[Float]) -> Float` | 意味変更: `List[Float]` も受け取る | R-list-08 |
| `push` | `fn list.push(mut xs: List[A], x: A) -> Unit` |  | R-list-00 |
| `range` | `fn list.range(start: Int, end: Int) -> List[Int]` |  |  |
| `range_step` | `fn list.range_step(start: Int, end: Int, step: Int) -> List[Int]` | 新規 | R-list-09 |
| `reduce` | `fn list.reduce(xs: List[A], f: (A, A) -> A) -> A?` |  |  |
| `remove_at` | `fn list.remove_at(xs: List[A], i: Int) -> List[A]` | 意味変更: 範囲外は abort | R-list-00、R-list-13 |
| `repeat` | `fn list.repeat(val: A, n: Int) -> List[A]` |  |  |
| `reverse` | `fn list.reverse(xs: List[A]) -> List[A]` |  | R-list-00 |
| `scan` | `fn list.scan(xs: List[A], init: B, f: (B, A) -> B) -> List[B]` |  |  |
| `set` | `fn list.set(xs: List[A], i: Int, val: A) -> List[A]` | 意味変更: 範囲外は abort | R-list-00、R-list-13 |
| `slice` | `fn list.slice(xs: List[A], start: Int, end: Int) -> List[A]` | 意味変更: 負の開始位置を 0 に丸める | R-list-10 |
| `sort` | `fn list.sort(xs: List[A]) -> List[A]` |  | R-list-00 |
| `sort_by` | `fn list.sort_by(xs: List[A], f: (A) -> B) -> List[A]` |  | R-list-06 |
| `sort_with` | `fn list.sort_with(xs: List[A], cmp: (A, A) -> Int) -> List[A]` | 新規 | R-list-06 |
| `split_at` | `fn list.split_at(xs: List[T], n: Int) -> (List[T], List[T])` |  |  |
| `sum` | `fn list.sum(xs: List[Int]) -> Int` ・ `fn list.sum(xs: List[Float]) -> Float` | 意味変更: `List[Float]` も受け取る | R-list-08 |
| `swap` | `fn list.swap(xs: List[A], i: Int, j: Int) -> List[A]` | 意味変更: 範囲外は abort | R-list-00、R-list-13 |
| `take` | `fn list.take(xs: List[A], n: Int) -> List[A]` |  | R-L0-12 |
| `take_end` | `fn list.take_end(xs: List[A], n: Int) -> List[A]` |  | R-L0-12 |
| `take_while` | `fn list.take_while(xs: List[A], f: (A) -> Bool) -> List[A]` |  |  |
| `unique` | `fn list.unique(xs: List[A]) -> List[A]` |  |  |
| `unique_by` | `fn list.unique_by(xs: List[A], f: (A) -> K) -> List[A]` |  |  |
| `unzip` | `fn list.unzip(xs: List[(A, B)]) -> (List[A], List[B])` | 新規 | R-list-07 |
| `update` | `fn list.update(xs: List[A], i: Int, f: (A) -> A) -> List[A]` | 意味変更: 範囲外は abort | R-list-00、R-list-13 |
| `window` | `fn list.window(xs: List[A], n: Int) -> List[List[A]]` | 意味変更: `n <= 0` は abort。`windows` の残る側 | R-list-03、R-list-10 |
| `with_capacity` | `fn list.with_capacity(cap: Int) -> List[A]` |  | R-list-00 |
| `zip` | `fn list.zip(xs: List[A], ys: List[B]) -> List[(A, B)]` |  |  |
| `zip_with` | `fn list.zip_with(xs: List[A], ys: List[B], f: (A, B) -> C) -> List[C]` |  |  |

撤去（6）:

| 関数 | 代わり | 案 |
|---|---|---|
| `bundled_probe` | `n + 1` | R-list-04 |
| `get_or` | `list.get(xs, i) ?? d` | R-list-01 |
| `length` | `list.len` | R-list-02 |
| `shuffle` | `random.shuffle` | R-list-05 |
| `tail` | `list.drop(xs, 1)` | R-list-14 |
| `windows` | `list.window` | R-list-03 |

## map

記録: [L2-map.md](L2-map.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `all` | `fn map.all(m: Map[K, V], f: (K, V) -> Bool) -> Bool` |  | R-map-06 |
| `any` | `fn map.any(m: Map[K, V], f: (K, V) -> Bool) -> Bool` |  | R-map-06 |
| `clear` | `fn map.clear(mut m: Map[K, V]) -> Unit` |  | — (R-L0-04 の動詞) |
| `contains` | `fn map.contains(m: Map[K, V], key: K) -> Bool` |  |  |
| `count` | `fn map.count(m: Map[K, V], f: (K, V) -> Bool) -> Int` |  | R-map-06 |
| `filter` | `fn map.filter(m: Map[K, V], f: (K, V) -> Bool) -> Map[K, V]` |  | R-map-06 |
| `find` | `fn map.find(m: Map[K, V], f: (K, V) -> Bool) -> (K, V)?` |  | R-map-06 |
| `fold` | `fn map.fold(m: Map[K, V], init: A, f: (A, K, V) -> A) -> A` |  | R-map-06 |
| `from_list` | `fn map.from_list(pairs: List[(K, V)]) -> Map[K, V]` |  | — (R-L0-07) |
| `get` | `fn map.get(m: Map[K, V], key: K) -> V?` |  |  |
| `is_empty` | `fn map.is_empty(m: Map[K, V]) -> Bool` |  |  |
| `keys` | `fn map.keys(m: Map[K, V]) -> List[K]` |  |  |
| `len` | `fn map.len(m: Map[K, V]) -> Int` |  | — (R-L0-11) |
| `map` | `fn map.map(m: Map[K, V], f: (V) -> B) -> Map[K, B]` |  | R-map-04 |
| `merge` | `fn map.merge(a: Map[K, V], b: Map[K, V]) -> Map[K, V]` |  |  |
| `new` | `fn map.new() -> Map[K, V]` |  |  |
| `remove` | `fn map.remove(m: Map[K, V], key: K) -> Map[K, V]` |  | R-map-01（置き換え先） |
| `set` | `fn map.set(m: Map[K, V], key: K, value: V) -> Map[K, V]` |  | R-map-01（置き換え先） |
| `to_list` | `fn map.to_list(m: Map[K, V]) -> List[(K, V)]` | 改名（旧 `entries`） | R-map-07 |
| `update` | `fn map.update(m: Map[K, V], key: K, f: (V) -> V) -> Map[K, V]` |  |  |
| `upsert` | `fn map.upsert(m: Map[K, V], key: K, init: V, f: (V) -> V) -> Map[K, V]` |  |  |
| `values` | `fn map.values(m: Map[K, V]) -> List[V]` |  |  |

撤去（3）:

| 関数 | 代わり | 案 |
|---|---|---|
| `delete` | `m = map.remove(m, k)` | R-map-01 |
| `get_or` | `map.get(m, k) ?? d` | R-map-02 |
| `insert` | `m[k] = v`、または `m = map.set(m, k, v)` | R-map-01 |

## math

記録: [L2-math.md](L2-math.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `acos` | `fn math.acos(x: Float) -> Float` | 新規 | R-math-02 |
| `acosh` | `fn math.acosh(x: Float) -> Float` | 新規 | R-math-05 |
| `asin` | `fn math.asin(x: Float) -> Float` | 新規 | R-math-02 |
| `asinh` | `fn math.asinh(x: Float) -> Float` | 新規 | R-math-05 |
| `atan` | `fn math.atan(x: Float) -> Float` |  |  |
| `atan2` | `fn math.atan2(y: Float, x: Float) -> Float` | 新規 | R-math-02 |
| `atanh` | `fn math.atanh(x: Float) -> Float` | 新規 | R-math-05 |
| `cbrt` | `fn math.cbrt(x: Float) -> Float` | 新規 | R-math-02 |
| `choose` | `fn math.choose(n: Int, k: Int) -> Int` |  |  |
| `cos` | `fn math.cos(x: Float) -> Float` |  |  |
| `cosh` | `fn math.cosh(x: Float) -> Float` | 新規 | R-math-02 |
| `e` | `fn math.e() -> Float` |  |  |
| `exp` | `fn math.exp(x: Float) -> Float` |  |  |
| `expm1` | `fn math.expm1(x: Float) -> Float` | 新規 | R-math-04 |
| `factorial` | `fn math.factorial(n: Int) -> Int` |  | —（大きな値は [L2-bigint](L2-bigint.md)） |
| `hypot` | `fn math.hypot(x: Float, y: Float) -> Float` | 新規 | R-math-02 |
| `log` | `fn math.log(x: Float) -> Float` |  |  |
| `log10` | `fn math.log10(x: Float) -> Float` |  |  |
| `log1p` | `fn math.log1p(x: Float) -> Float` | 新規 | R-math-04 |
| `log2` | `fn math.log2(x: Float) -> Float` |  |  |
| `log_gamma` | `fn math.log_gamma(x: Float) -> Float` |  |  |
| `pi` | `fn math.pi() -> Float` |  |  |
| `sin` | `fn math.sin(x: Float) -> Float` |  |  |
| `sinh` | `fn math.sinh(x: Float) -> Float` | 新規 | R-math-02 |
| `sqrt` | `fn math.sqrt(x: Float) -> Float` |  | R-math-03（R-math-01 の見直し） |
| `tan` | `fn math.tan(x: Float) -> Float` |  |  |
| `tanh` | `fn math.tanh(x: Float) -> Float` |  |  |

撤去（8）:

| 関数 | 代わり | 案 |
|---|---|---|
| `abs` | `int.abs` | R-math-01（`int.abs`） |
| `fmax` | `float.max` | R-math-01（`float.max`） |
| `fmin` | `float.min` | R-math-01（`float.min`） |
| `fpow` | `float.pow` | R-math-01（`float.pow`） |
| `max` | `int.max` | R-math-01（`int.max`） |
| `min` | `int.min` | R-math-01（`int.min`） |
| `pow` | `int.pow` | R-math-01（`int.pow`） |
| `sign` | `int.sign` | R-math-01（`int.sign`） |

## matrix

記録: [L2-matrix.md](L2-matrix.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `broadcast_add_row` | `fn matrix.broadcast_add_row(m: Matrix, bias: List[Float]) -> Matrix` | 意味変更: 範囲・形の不一致は abort | R-matrix-05（長さ違いは abort） |
| `cols` | `fn matrix.cols(m: Matrix) -> Int` |  |  |
| `concat_cols` | `fn matrix.concat_cols(matrices: List[Matrix]) -> Matrix` |  |  |
| `concat_rows` | `fn matrix.concat_rows(matrices: List[Matrix]) -> Matrix` | 新規 | R-matrix-11 |
| `dot_row` | `fn matrix.dot_row(m: Matrix, r: Int, vec: List[Float]) -> Float` | 意味変更: 範囲・形の不一致は abort | R-matrix-05（範囲外は abort） |
| `from_bytes_f16_le` | `fn matrix.from_bytes_f16_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix?` | 意味変更: 負の形は abort、データ不足は none | R-matrix-05、R-matrix-15 |
| `from_bytes_f32_le` | `fn matrix.from_bytes_f32_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix?` | 意味変更: 負の形は abort、データ不足は none | R-matrix-05、R-matrix-15 |
| `from_bytes_f64_le` | `fn matrix.from_bytes_f64_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix?` | 意味変更: 負の形は abort、データ不足は none | R-matrix-05、R-matrix-15 |
| `from_lists` | `fn matrix.from_lists(rows: List[List[Float]]) -> Matrix` |  |  |
| `gather_rows` | `fn matrix.gather_rows(m: Matrix, indices: List[Int]) -> Matrix` | 意味変更: 範囲・形の不一致は abort | R-matrix-04・R-matrix-05（範囲外は abort） |
| `get` | `fn matrix.get(m: Matrix, row: Int, col: Int) -> Float?` | 意味変更: `Option[Float]` を返す | R-matrix-08 |
| `map` | `fn matrix.map(m: Matrix, f: (Float) -> Float) -> Matrix` |  |  |
| `ones` | `fn matrix.ones(rows: Int, cols: Int) -> Matrix` |  |  |
| `rows` | `fn matrix.rows(m: Matrix) -> Int` |  |  |
| `scale` | `fn matrix.scale(m: Matrix, s: Float) -> Matrix` |  |  |
| `shape` | `fn matrix.shape(m: Matrix) -> (Int, Int)` |  |  |
| `slice_rows` | `fn matrix.slice_rows(m: Matrix, start: Int, end: Int) -> Matrix` |  | —（R-L0-12 の丸め） |
| `split_cols_even` | `fn matrix.split_cols_even(m: Matrix, n: Int) -> List[Matrix]` |  |  |
| `to_bytes_f16_le` | `fn matrix.to_bytes_f16_le(m: Matrix) -> Bytes` | 新規 | R-matrix-12 |
| `to_bytes_f32_le` | `fn matrix.to_bytes_f32_le(m: Matrix) -> Bytes` |  |  |
| `to_bytes_f64_le` | `fn matrix.to_bytes_f64_le(m: Matrix) -> Bytes` |  |  |
| `to_lists` | `fn matrix.to_lists(m: Matrix) -> List[List[Float]]` |  |  |
| `transpose` | `fn matrix.transpose(m: Matrix) -> Matrix` |  |  |
| `zeros` | `fn matrix.zeros(rows: Int, cols: Int) -> Matrix` |  |  |
| `zip_with` | `fn matrix.zip_with(a: Matrix, b: Matrix, f: (Float, Float) -> Float) -> Matrix` | 新規 | R-matrix-09 |

撤去（24）:

| 関数 | 代わり | 案 |
|---|---|---|
| `add` | `a + b` | R-matrix-14 |
| `append_rows` | `concat_rows([a, b])` | R-matrix-11 |
| `concat_cols_many` | `matrix.concat_cols` | R-matrix-06 |
| `conv1d` | nnlib | R-matrix-01（nnlib） |
| `div` | `a / b` | R-matrix-14 |
| `fma` | 内部の `__fma` | R-matrix-02（内部の `__fma`） |
| `fma3` | 内部の `__fma3` | R-matrix-02（内部の `__fma3`） |
| `linear_f32_row_no_bias` | nnlib | R-matrix-01（nnlib） |
| `linear_row` | nnlib | R-matrix-01（nnlib） |
| `linear_row_no_bias` | nnlib | R-matrix-01（nnlib） |
| `mul` | `a * b` | R-matrix-14 |
| `mul_f32` | `a * b` | R-matrix-03 |
| `mul_f32_scaled` | `matrix.scale(a * b, s)` | R-matrix-03 |
| `mul_f32_t` | `a * matrix.transpose(b)` | R-matrix-03 |
| `mul_f32_t_scaled` | `matrix.scale(a * matrix.transpose(b), s)` | R-matrix-03 |
| `mul_scaled` | 内部の `__mul_scaled` | R-matrix-02（内部の `__mul_scaled`） |
| `neg` | `-m` | R-matrix-14 |
| `ones_f32` | `matrix.ones` | R-matrix-03 |
| `pow` | `matrix.map(m, (x) => float.pow(x, e))` | R-matrix-13 |
| `row_dot` | `matrix.dot_row` | R-matrix-06 |
| `select_rows` | `matrix.gather_rows` | R-matrix-04 |
| `select_rows_f32` | nnlib | R-matrix-01（nnlib） |
| `sub` | `a - b` | R-matrix-14 |
| `zeros_f32` | `matrix.zeros`（f32 のデータは from_bytes_f32_le で読む） | R-matrix-03 |

## net

記録: [L2-net.md](L2-net.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `ip_to_string` | `fn net.ip_to_string(ip: IpAddr) -> String` | 新規 | R-net-03 |
| `is_loopback` | `fn net.is_loopback(ip: IpAddr) -> Bool` | 新規 | R-net-03 |
| `lookup_host` | `effect fn net.lookup_host(host: String) -> List[IpAddr]` | 新規 | R-net-03 |
| `parse_ip` | `fn net.parse_ip(s: String) -> Result[IpAddr, String]` | 新規 | R-net-03 |
| `tcp_accept` | `effect fn net.tcp_accept(listener: Int) -> Int` |  |  |
| `tcp_available` | `effect fn net.tcp_available(stream: Int) -> Int` |  |  |
| `tcp_close` | `effect fn net.tcp_close(stream: Int) -> Unit` |  |  |
| `tcp_close_listener` | `effect fn net.tcp_close_listener(listener: Int) -> Unit` |  |  |
| `tcp_connect` | `effect fn net.tcp_connect(host: String, port: Int) -> Int` |  |  |
| `tcp_is_open` | `effect fn net.tcp_is_open(stream: Int) -> Bool` | 意味変更: effect fn | R-net-04 |
| `tcp_listen` | `effect fn net.tcp_listen(host: String, port: Int) -> Int` |  |  |
| `tcp_local_port` | `effect fn net.tcp_local_port(listener: Int) -> Int` |  |  |
| `tcp_read` | `effect fn net.tcp_read(stream: Int, len: Int) -> Bytes` |  |  |
| `tcp_read_exact` | `effect fn net.tcp_read_exact(stream: Int, len: Int) -> Bytes` |  |  |
| `tcp_read_timeout` | `effect fn net.tcp_read_timeout(stream: Int, len: Int, timeout_ms: Int) -> Bytes` |  |  |
| `tcp_set_timeout` | `effect fn net.tcp_set_timeout(stream: Int, timeout_ms: Int) -> Unit` |  |  |
| `tcp_write` | `effect fn net.tcp_write(stream: Int, data: Bytes) -> Unit` |  |  |
| `udp_bind` | `effect fn net.udp_bind(host: String, port: Int) -> UdpSocket` | 新規 | R-net-02 |
| `udp_close` | `effect fn net.udp_close(sock: UdpSocket) -> Unit` | 新規 | R-net-02 |
| `udp_local_port` | `effect fn net.udp_local_port(sock: UdpSocket) -> Int` | 新規 | R-net-02 |
| `udp_recv_from` | `effect fn net.udp_recv_from(sock: UdpSocket, max: Int) -> UdpPacket` | 新規 | R-net-01、R-net-06 |
| `udp_send_to` | `effect fn net.udp_send_to(sock: UdpSocket, data: Bytes, host: String, port: Int) -> Unit` | 新規 | R-net-02 |

撤去（13）:

| 関数 | 代わり | 案 |
|---|---|---|
| `shm_create` | `unix.shm_create` | R-net-01 |
| `shm_read` | `unix.shm_read` | R-net-01 |
| `shm_resize` | `unix.shm_resize` | R-net-01 |
| `shm_write` | `unix.shm_write` | R-net-01 |
| `unix_accept` | `unix.accept` | R-net-01 |
| `unix_close` | `unix.close` | R-net-01 |
| `unix_connect` | `unix.connect` | R-net-01 |
| `unix_listen` | `unix.listen` | R-net-01 |
| `unix_poll` | `unix.select([fd], t)` | R-net-01 |
| `unix_recv` | `unix.read` | R-net-01 |
| `unix_send` | `unix.write` | R-net-01 |
| `unix_take_fds` | `unix.take_fds` | R-net-01 |
| `unix_wait` | `unix.select` | R-net-01 |

## option

記録: [L2-option.md](L2-option.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `collect` | `fn option.collect(xs: List[T?]) -> List[T]?` |  | R-option-02 |
| `collect_map` | `fn option.collect_map(xs: List[T], f: (T) -> U?) -> List[U]?` |  | R-option-02 |
| `filter` | `fn option.filter(o: A?, f: (A) -> Bool) -> A?` |  | R-option-04 |
| `flat_map` | `fn option.flat_map(o: A?, f: (A) -> B?) -> B?` |  | R-L0-10（`flat_map` の綴りは規則で固定） |
| `flatten` | `fn option.flatten(o: A??) -> A?` |  |  |
| `is_none` | `fn option.is_none(o: A?) -> Bool` |  |  |
| `is_some` | `fn option.is_some(o: A?) -> Bool` |  |  |
| `map` | `fn option.map(o: A?, f: (A) -> B) -> B?` |  |  |
| `or_else` | `fn option.or_else(o: A?, f: () -> A?) -> A?` |  | R-option-03 |
| `to_list` | `fn option.to_list(o: A?) -> List[A]` |  |  |
| `to_result` | `fn option.to_result(o: A?, e: E) -> Result[A, E]` |  | 注: E は多相（probe-error/p.almd で独自の variant E を確認）。ADR-0007 D4 の「E は String 固定」は文面のほうが古い |
| `unwrap_or_else` | `fn option.unwrap_or_else(o: A?, f: () -> A) -> A` |  | `??` の脱糖先（ADR-0005 D1） |
| `zip` | `fn option.zip(a: A?, b: B?) -> (A, B)?` |  | R-option-04 |

撤去（1）:

| 関数 | 代わり | 案 |
|---|---|---|
| `unwrap_or` | `o ?? d` | R-option-01 |

## path

記録: [L2-path.md](L2-path.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `basename` | `fn path.basename(p: String) -> String` |  |  |
| `components` | `fn path.components(p: String) -> List[String]` | 新規 | R-path-01 |
| `dirname` | `fn path.dirname(p: String) -> String` |  |  |
| `extension` | `fn path.extension(p: String) -> String?` |  |  |
| `is_absolute` | `fn path.is_absolute(p: String) -> Bool` |  |  |
| `join` | `fn path.join(base: String, child: String) -> String` |  |  |
| `normalize` | `fn path.normalize(p: String) -> String` |  |  |
| `relative_to` | `fn path.relative_to(p: String, base: String) -> String?` | 新規 | R-path-01 |
| `stem` | `fn path.stem(p: String) -> String` |  |  |
| `with_extension` | `fn path.with_extension(p: String, ext: String) -> String` | 新規 | R-path-03 |

撤去（3）:

| 関数 | 代わり | 案 |
|---|---|---|
| `from_string` | `SafePath` ごと | R-path-02 |
| `to_string` | `SafePath` ごと | R-path-02 |
| `trusted` | `SafePath` ごと | R-path-02 |

## process

記録: [L2-process.md](L2-process.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `child_pid` | `effect fn process.child_pid(c: ProcessChild) -> Int` | 新規 | R-process-06 |
| `exec` | `effect fn process.exec(cmd: String, args: List[String]) -> String` |  | R-process-02 |
| `exit` | `effect fn process.exit(code: Int) -> Never` |  |  |
| `kill` | `effect fn process.kill(pid: Int, signal: Int) -> Unit` |  | R-process-03 |
| `output` | `effect fn process.output(c: ProcessCommand) -> ProcessStatus` | 新規 | R-process-02 |
| `pid` | `effect fn process.pid() -> Int` | 意味変更: effect fn にする | R-process-04 |
| `poll` | `effect fn process.poll(c: ProcessChild) -> Result[ProcessStatus, String]?` | 新規 | R-process-03 |
| `read_new_stderr` | `effect fn process.read_new_stderr(c: ProcessChild) -> String` | 新規 | R-process-03 |
| `read_new_stdout` | `effect fn process.read_new_stdout(c: ProcessChild) -> String` | 新規 | R-process-03 |
| `run` | `effect fn process.run(c: ProcessCommand) -> Int` | 意味変更: `ProcessCommand` を受け取る | R-process-07 |
| `start` | `effect fn process.start(c: ProcessCommand) -> ProcessChild` | 新規 | R-process-03 |
| `stop` | `effect fn process.stop(c: ProcessChild, grace_ms: Int) -> ProcessStatus` | 新規 | R-process-03 |
| `wait` | `effect fn process.wait(c: ProcessChild) -> ProcessStatus` | 新規 | R-process-03 |

撤去（12）:

| 関数 | 代わり | 案 |
|---|---|---|
| `args` | `env.args` | R-process-01 |
| `env` | `env.get` | R-process-01 |
| `exec_attached` | `run` | R-process-02 |
| `exec_in` | `output` | R-process-02 |
| `exec_status` | `output` | R-process-02 |
| `exec_status_timeout` | `output` の `timeout_ms` | R-process-02 |
| `exec_with_stdin` | `output` | R-process-02 |
| `is_alive` | `poll` | R-process-03 |
| `run_in` | `run(ProcessCommand { dir: … })` | R-process-07 |
| `sleep` | `env.sleep_ms` | R-process-01 |
| `spawn` | `start` | R-process-03 |
| `stdin_lines` | `string.lines(io.read_all()!)` | R-process-01 |

## random

記録: [L2-random.md](L2-random.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `bytes` | `effect fn random.bytes(n: Int) -> Bytes` | 新規 | R-random-02 |
| `choice` | `effect fn random.choice(xs: List[T]) -> T?` |  |  |
| `float` | `effect fn random.float() -> Float` |  |  |
| `int` | `effect fn random.int(min: Int, max: Int) -> Int` |  | R-random-04 |
| `next_choice` | `fn random.next_choice[T](rng: Rng, xs: List[T]) -> (T?, Rng)` | 新規 | R-random-01 |
| `next_float` | `fn random.next_float(rng: Rng) -> (Float, Rng)` | 新規 | R-random-01 |
| `next_int` | `fn random.next_int(rng: Rng, min: Int, max: Int) -> (Int, Rng)` | 新規 | R-random-01 |
| `next_shuffle` | `fn random.next_shuffle[T](rng: Rng, xs: List[T]) -> (List[T], Rng)` | 新規 | R-random-01 |
| `seeded` | `fn random.seeded(seed: Int) -> Rng` | 新規 | R-random-01 |
| `shuffle` | `effect fn random.shuffle(xs: List[T]) -> List[T]` |  | R-random-03 |

## regex

記録: [L2-regex.md](L2-regex.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `captures` | `fn regex.captures(re: Regex, s: String) -> List[String?]?` | 意味変更: 第 1 引数を `Regex` に。グループは `String?` | R-regex-08 |
| `captures_all` | `fn regex.captures_all(re: Regex, s: String) -> List[List[String?]]` | 新規 | R-regex-04、R-regex-08 |
| `escape` | `fn regex.escape(s: String) -> String` | 新規 | R-regex-04 |
| `find` | `fn regex.find(re: Regex, s: String) -> String?` | 意味変更: 第 1 引数を `Regex` に | R-regex-02 |
| `find_all` | `fn regex.find_all(re: Regex, s: String) -> List[String]` | 意味変更: 第 1 引数を `Regex` に | R-regex-02 |
| `index_of` | `fn regex.index_of(re: Regex, s: String) -> Int?` | 新規 | R-regex-04 |
| `is_full_match` | `fn regex.is_full_match(re: Regex, s: String) -> Bool` | 改名（旧 `full_match`）。第 1 引数は `Regex` | R-regex-01、R-regex-02 |
| `is_match` | `fn regex.is_match(re: Regex, s: String) -> Bool` | 意味変更: 第 1 引数を `Regex` に | R-regex-02 |
| `new` | `fn regex.new(pat: String) -> Regex` | 新規 | R-regex-02 |
| `parse` | `fn regex.parse(pat: String) -> Result[Regex, String]` | 新規 | R-regex-02 |
| `replace` | `fn regex.replace(re: Regex, s: String, rep: String) -> String` | 意味変更: 第 1 引数を `Regex` に | R-regex-02、R-regex-05（`$1` は解釈しない） |
| `replace_first` | `fn regex.replace_first(re: Regex, s: String, rep: String) -> String` | 意味変更: 第 1 引数を `Regex` に | R-regex-02 |
| `replace_first_with` | `fn regex.replace_first_with(re: Regex, s: String, f: (List[String?]) -> String) -> String` | 新規 | R-regex-07、R-regex-08 |
| `replace_with` | `fn regex.replace_with(re: Regex, s: String, f: (List[String?]) -> String) -> String` | 新規 | R-regex-05、R-regex-08 |
| `split` | `fn regex.split(re: Regex, s: String) -> List[String]` | 意味変更: 第 1 引数を `Regex` に | R-regex-02 |

## result

記録: [L2-result.md](L2-result.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `filter` | `fn result.filter(r: Result[A, E], pred: (A) -> Bool, err_val: E) -> Result[A, E]` |  | R-result-02 |
| `flat_map` | `fn result.flat_map(r: Result[A, E], f: (A) -> Result[B, E]) -> Result[B, E]` |  | R-L0-10 |
| `flatten` | `fn result.flatten(r: Result[Result[A, E], E]) -> Result[A, E]` |  | R-option-04 |
| `is_err` | `fn result.is_err(r: Result[A, E]) -> Bool` |  |  |
| `is_ok` | `fn result.is_ok(r: Result[A, E]) -> Bool` |  |  |
| `map` | `fn result.map(r: Result[A, E], f: (A) -> B) -> Result[B, E]` |  |  |
| `map_err` | `fn result.map_err(r: Result[A, E], f: (E) -> F) -> Result[A, F]` |  | ADR-0004 D2 の正準形 |
| `or_else` | `fn result.or_else(r: Result[A, E], f: (E) -> Result[A, F]) -> Result[A, F]` |  |  |
| `partition` | `fn result.partition(rs: List[Result[T, E]]) -> (List[T], List[E])` |  | R-result-05 |
| `to_err_option` | `fn result.to_err_option(r: Result[A, E]) -> E?` |  | R-result-03 |
| `to_list` | `fn result.to_list(r: Result[A, E]) -> List[A]` |  | R-option-04 |
| `to_option` | `fn result.to_option(r: Result[A, E]) -> A?` |  | `?` の脱糖先 |
| `unwrap_or_else` | `fn result.unwrap_or_else(r: Result[A, E], f: (E) -> A) -> A` |  | `??` の脱糖先（ADR-0005 D1） |
| `zip` | `fn result.zip(a: Result[A, E], b: Result[B, E]) -> Result[(A, B), E]` |  | R-option-04 |

撤去（1）:

| 関数 | 代わり | 案 |
|---|---|---|
| `unwrap_or` | `r ?? d` | R-result-01 |

## set

記録: [L2-set.md](L2-set.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `all` | `fn set.all(s: Set[A], f: (A) -> Bool) -> Bool` |  | R-map-06 |
| `any` | `fn set.any(s: Set[A], f: (A) -> Bool) -> Bool` |  | R-map-06 |
| `clear` | `fn set.clear(mut s: Set[A]) -> Unit` | 新規 | R-L0-04（`clear` の行列） |
| `contains` | `fn set.contains(s: Set[A], value: A) -> Bool` |  |  |
| `count` | `fn set.count(s: Set[A], f: (A) -> Bool) -> Int` | 新規 | R-set-01 |
| `difference` | `fn set.difference(a: Set[A], b: Set[A]) -> Set[A]` |  |  |
| `filter` | `fn set.filter(s: Set[A], f: (A) -> Bool) -> Set[A]` |  | R-map-06 |
| `find` | `fn set.find(s: Set[A], f: (A) -> Bool) -> A?` | 新規 | R-set-01 |
| `fold` | `fn set.fold(s: Set[A], init: B, f: (B, A) -> B) -> B` |  | R-map-06 |
| `from_list` | `fn set.from_list(xs: List[A]) -> Set[A]` |  | — (R-L0-07) |
| `insert` | `fn set.insert(s: Set[A], value: A) -> Set[A]` |  | R-set-04 |
| `intersection` | `fn set.intersection(a: Set[A], b: Set[A]) -> Set[A]` |  |  |
| `is_disjoint` | `fn set.is_disjoint(a: Set[A], b: Set[A]) -> Bool` |  |  |
| `is_empty` | `fn set.is_empty(s: Set[A]) -> Bool` |  |  |
| `is_subset` | `fn set.is_subset(a: Set[A], b: Set[A]) -> Bool` |  | R-set-02 |
| `len` | `fn set.len(s: Set[A]) -> Int` |  | — (R-L0-11) |
| `map` | `fn set.map(s: Set[A], f: (A) -> B) -> Set[B]` |  | R-map-06 |
| `new` | `fn set.new() -> Set[A]` |  |  |
| `remove` | `fn set.remove(s: Set[A], value: A) -> Set[A]` |  |  |
| `symmetric_difference` | `fn set.symmetric_difference(a: Set[A], b: Set[A]) -> Set[A]` |  |  |
| `to_list` | `fn set.to_list(s: Set[A]) -> List[A]` |  |  |
| `union` | `fn set.union(a: Set[A], b: Set[A]) -> Set[A]` |  |  |

## string

記録: [L2-string.md](L2-string.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `byte_len` | `fn string.byte_len(s: String) -> Int` | 新規 | R-string-06 |
| `byte_slice` | `fn string.byte_slice(s: String, start: Int, end: Int) -> String?` |  |  |
| `capitalize` | `fn string.capitalize(s: String) -> String` |  |  |
| `chars` | `fn string.chars(s: String) -> List[String]` |  |  |
| `clear` | `fn string.clear(mut s: String) -> Unit` |  |  |
| `compare` | `fn string.compare(a: String, b: String) -> Int` | 新規 | R-string-05（R-L0-16） |
| `contains` | `fn string.contains(s: String, sub: String) -> Bool` |  |  |
| `count` | `fn string.count(s: String, sub: String) -> Int` |  |  |
| `drop` | `fn string.drop(s: String, n: Int) -> String` |  |  |
| `drop_end` | `fn string.drop_end(s: String, n: Int) -> String` |  |  |
| `ends_with` | `fn string.ends_with(s: String, suffix: String) -> Bool` |  |  |
| `first` | `fn string.first(s: String) -> String?` |  |  |
| `from_codepoint` | `fn string.from_codepoint(n: Int) -> String?` | 意味変更: `Option[String]` を返す | R-string-11 |
| `from_utf16` | `fn string.from_utf16(units: List[Int]) -> Result[String, String]` | 新規 | R-string-07 |
| `get` | `fn string.get(s: String, i: Int) -> String?` |  |  |
| `index_of` | `fn string.index_of(s: String, needle: String) -> Int?` |  |  |
| `is_alpha` | `fn string.is_alpha(s: String) -> Bool` |  |  |
| `is_alphanumeric` | `fn string.is_alphanumeric(s: String) -> Bool` |  |  |
| `is_digit` | `fn string.is_digit(s: String) -> Bool` |  |  |
| `is_empty` | `fn string.is_empty(s: String) -> Bool` |  |  |
| `is_lower` | `fn string.is_lower(s: String) -> Bool` |  |  |
| `is_upper` | `fn string.is_upper(s: String) -> Bool` |  |  |
| `is_whitespace` | `fn string.is_whitespace(s: String) -> Bool` |  |  |
| `last` | `fn string.last(s: String) -> String?` |  |  |
| `last_index_of` | `fn string.last_index_of(s: String, needle: String) -> Int?` |  |  |
| `len` | `fn string.len(s: String) -> Int` |  |  |
| `lines` | `fn string.lines(s: String) -> List[String]` |  |  |
| `pad_end` | `fn string.pad_end(s: String, n: Int, ch: String) -> String` |  |  |
| `pad_start` | `fn string.pad_start(s: String, n: Int, ch: String) -> String` |  |  |
| `push` | `fn string.push(mut s: String, suffix: String) -> Unit` |  |  |
| `repeat` | `fn string.repeat(s: String, n: Int) -> String` |  |  |
| `replace` | `fn string.replace(s: String, from: String, to: String) -> String` |  |  |
| `replace_first` | `fn string.replace_first(s: String, from: String, to: String) -> String` |  |  |
| `reverse` | `fn string.reverse(s: String) -> String` |  |  |
| `slice` | `fn string.slice(s: String, start: Int, end: Int) -> String` | 意味変更: `end` の既定値を外す。2 引数は `drop` | R-string-08 |
| `split` | `fn string.split(s: String, sep: String) -> List[String]` |  |  |
| `split_once` | `fn string.split_once(s: String, sep: String) -> (String, String)?` |  |  |
| `starts_with` | `fn string.starts_with(s: String, prefix: String) -> Bool` |  |  |
| `strip_prefix` | `fn string.strip_prefix(s: String, prefix: String) -> String?` |  |  |
| `strip_suffix` | `fn string.strip_suffix(s: String, suffix: String) -> String?` |  |  |
| `take` | `fn string.take(s: String, n: Int) -> String` |  |  |
| `take_end` | `fn string.take_end(s: String, n: Int) -> String` |  |  |
| `to_codepoint` | `fn string.to_codepoint(s: String) -> Int?` | 改名（旧 `codepoint`） | R-string-12 |
| `to_lower` | `fn string.to_lower(s: String) -> String` |  |  |
| `to_upper` | `fn string.to_upper(s: String) -> String` |  |  |
| `to_utf16` | `fn string.to_utf16(s: String) -> List[Int]` | 新規 | R-string-07 |
| `trim` | `fn string.trim(s: String) -> String` |  |  |
| `trim_end` | `fn string.trim_end(s: String) -> String` |  |  |
| `trim_start` | `fn string.trim_start(s: String) -> String` |  |  |

撤去（5）:

| 関数 | 代わり | 案 |
|---|---|---|
| `from_bytes` | `bytes.from_list(xs) \|> bytes.to_string_lossy` | R-string-02 |
| `join` | `list.join` | R-string-01 |
| `length` | `string.len` | R-string-04 |
| `run_length_encode` | `string.chars` と `list.fold` | R-string-03 |
| `to_bytes` | `bytes.from_string` | R-string-02 |

## testing

記録: [L2-testing.md](L2-testing.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `assert_aborts` | `fn testing.assert_aborts[T](expected: String, f: () -> T) -> Unit` |  |  |
| `assert_approx` | `fn testing.assert_approx(a: Float, b: Float, tolerance: Float) -> Unit` |  |  |
| `assert_contains` | `fn testing.assert_contains(haystack: String, needle: String) -> Unit` |  | R-testing-01 |
| `assert_err` | `fn testing.assert_err[A, B](result: Result[A, B]) -> Unit` |  |  |
| `assert_gt` | `fn testing.assert_gt[T](a: T, b: T) -> Unit` | 意味変更: 順序のある型すべてに広げる | R-testing-02 |
| `assert_lt` | `fn testing.assert_lt[T](a: T, b: T) -> Unit` | 意味変更: 順序のある型すべてに広げる | R-testing-02 |
| `assert_none` | `fn testing.assert_none[A](opt: A?) -> Unit` |  |  |
| `assert_ok` | `fn testing.assert_ok[A, B](result: Result[A, B]) -> Unit` |  |  |
| `assert_snapshot` | `fn testing.assert_snapshot(actual: String, expected: String) -> Unit` |  |  |
| `assert_some` | `fn testing.assert_some[A](opt: A?) -> Unit` |  |  |

## uint16

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `compare` | `fn uint16.compare(a: UInt16, b: UInt16) -> Int` | 新規 | R-sized-02 |
| `max_value` | `fn uint16.max_value() -> UInt16` |  |  |
| `min_value` | `fn uint16.min_value() -> UInt16` |  |  |
| `parse` | `fn uint16.parse(s: String) -> Result[UInt16, String]` | 新規 | R-sized-02 |
| `parse_radix` | `fn uint16.parse_radix(s: String, radix: Int) -> Result[UInt16, String]` | 新規 | R-sized-06 |
| `to_float` | `fn uint16.to_float(x: UInt16) -> Float` | 新規 | R-sized-01 |
| `to_float32` | `fn uint16.to_float32(x: UInt16) -> Float32` |  | R-sized-01（行列のセル） |
| `to_float64` | `fn uint16.to_float64(x: UInt16) -> Float64` |  | R-sized-01（行列のセル） |
| `to_int` | `fn uint16.to_int(x: UInt16) -> Int` | 新規 | R-sized-01 |
| `to_int16` | `fn uint16.to_int16(x: UInt16) -> Int16` |  | R-sized-01（行列のセル） |
| `to_int16_checked` | `fn uint16.to_int16_checked(x: UInt16) -> Int16?` |  | R-sized-01（行列のセル） |
| `to_int16_saturating` | `fn uint16.to_int16_saturating(x: UInt16) -> Int16` |  | R-sized-01（行列のセル） |
| `to_int32` | `fn uint16.to_int32(x: UInt16) -> Int32` |  | R-sized-01（行列のセル） |
| `to_int64` | `fn uint16.to_int64(x: UInt16) -> Int64` |  | R-sized-01（行列のセル） |
| `to_int8` | `fn uint16.to_int8(x: UInt16) -> Int8` |  | R-sized-01（行列のセル） |
| `to_int8_checked` | `fn uint16.to_int8_checked(x: UInt16) -> Int8?` |  | R-sized-01（行列のセル） |
| `to_int8_saturating` | `fn uint16.to_int8_saturating(x: UInt16) -> Int8` |  | R-sized-01（行列のセル） |
| `to_string` | `fn uint16.to_string(x: UInt16) -> String` |  |  |
| `to_string_radix` | `fn uint16.to_string_radix(x: UInt16, radix: Int) -> String` | 新規 | R-sized-06 |
| `to_uint32` | `fn uint16.to_uint32(x: UInt16) -> UInt32` |  | R-sized-01（行列のセル） |
| `to_uint64` | `fn uint16.to_uint64(x: UInt16) -> UInt64` |  | R-sized-01（行列のセル） |
| `to_uint8` | `fn uint16.to_uint8(x: UInt16) -> UInt8` |  | R-sized-01（行列のセル） |
| `to_uint8_checked` | `fn uint16.to_uint8_checked(x: UInt16) -> UInt8?` |  | R-sized-01（行列のセル） |
| `to_uint8_saturating` | `fn uint16.to_uint8_saturating(x: UInt16) -> UInt8` |  | R-sized-01（行列のセル） |

## uint32

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `compare` | `fn uint32.compare(a: UInt32, b: UInt32) -> Int` | 新規 | R-sized-02 |
| `max_value` | `fn uint32.max_value() -> UInt32` |  |  |
| `min_value` | `fn uint32.min_value() -> UInt32` |  |  |
| `parse` | `fn uint32.parse(s: String) -> Result[UInt32, String]` | 新規 | R-sized-02 |
| `parse_radix` | `fn uint32.parse_radix(s: String, radix: Int) -> Result[UInt32, String]` | 新規 | R-sized-06 |
| `to_float` | `fn uint32.to_float(x: UInt32) -> Float` | 新規 | R-sized-01 |
| `to_float32` | `fn uint32.to_float32(x: UInt32) -> Float32` |  | R-sized-01（行列のセル） |
| `to_float32_checked` | `fn uint32.to_float32_checked(x: UInt32) -> Float32?` | 新規 | R-sized-01 |
| `to_float64` | `fn uint32.to_float64(x: UInt32) -> Float64` |  | R-sized-01（行列のセル） |
| `to_int` | `fn uint32.to_int(x: UInt32) -> Int` | 新規 | R-sized-01 |
| `to_int16` | `fn uint32.to_int16(x: UInt32) -> Int16` |  | R-sized-01（行列のセル） |
| `to_int16_checked` | `fn uint32.to_int16_checked(x: UInt32) -> Int16?` |  | R-sized-01（行列のセル） |
| `to_int16_saturating` | `fn uint32.to_int16_saturating(x: UInt32) -> Int16` |  | R-sized-01（行列のセル） |
| `to_int32` | `fn uint32.to_int32(x: UInt32) -> Int32` |  | R-sized-01（行列のセル） |
| `to_int32_checked` | `fn uint32.to_int32_checked(x: UInt32) -> Int32?` |  | R-sized-01（行列のセル） |
| `to_int32_saturating` | `fn uint32.to_int32_saturating(x: UInt32) -> Int32` |  | R-sized-01（行列のセル） |
| `to_int64` | `fn uint32.to_int64(x: UInt32) -> Int64` |  | R-sized-01（行列のセル） |
| `to_int8` | `fn uint32.to_int8(x: UInt32) -> Int8` |  | R-sized-01（行列のセル） |
| `to_int8_checked` | `fn uint32.to_int8_checked(x: UInt32) -> Int8?` |  | R-sized-01（行列のセル） |
| `to_int8_saturating` | `fn uint32.to_int8_saturating(x: UInt32) -> Int8` |  | R-sized-01（行列のセル） |
| `to_string` | `fn uint32.to_string(x: UInt32) -> String` |  |  |
| `to_string_radix` | `fn uint32.to_string_radix(x: UInt32, radix: Int) -> String` | 新規 | R-sized-06 |
| `to_uint16` | `fn uint32.to_uint16(x: UInt32) -> UInt16` |  | R-sized-01（行列のセル） |
| `to_uint16_checked` | `fn uint32.to_uint16_checked(x: UInt32) -> UInt16?` |  | R-sized-01（行列のセル） |
| `to_uint16_saturating` | `fn uint32.to_uint16_saturating(x: UInt32) -> UInt16` |  | R-sized-01（行列のセル） |
| `to_uint64` | `fn uint32.to_uint64(x: UInt32) -> UInt64` |  | R-sized-01（行列のセル） |
| `to_uint8` | `fn uint32.to_uint8(x: UInt32) -> UInt8` |  | R-sized-01（行列のセル） |
| `to_uint8_checked` | `fn uint32.to_uint8_checked(x: UInt32) -> UInt8?` |  | R-sized-01（行列のセル） |
| `to_uint8_saturating` | `fn uint32.to_uint8_saturating(x: UInt32) -> UInt8` |  | R-sized-01（行列のセル） |

## uint64

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `compare` | `fn uint64.compare(a: UInt64, b: UInt64) -> Int` | 新規 | R-sized-02 |
| `max_value` | `fn uint64.max_value() -> UInt64` |  |  |
| `min_value` | `fn uint64.min_value() -> UInt64` |  |  |
| `parse` | `fn uint64.parse(s: String) -> Result[UInt64, String]` | 新規 | R-sized-02 |
| `parse_radix` | `fn uint64.parse_radix(s: String, radix: Int) -> Result[UInt64, String]` | 新規 | R-sized-06 |
| `to_bigint` | `fn uint64.to_bigint(x: UInt64) -> BigInt` | 新規 | R-bigint-06 |
| `to_float` | `fn uint64.to_float(x: UInt64) -> Float` | 新規 | R-sized-01 |
| `to_float32` | `fn uint64.to_float32(x: UInt64) -> Float32` |  | R-sized-01（行列のセル） |
| `to_float32_checked` | `fn uint64.to_float32_checked(x: UInt64) -> Float32?` | 新規 | R-sized-01 |
| `to_float64` | `fn uint64.to_float64(x: UInt64) -> Float64` |  | R-sized-01（行列のセル） |
| `to_float64_checked` | `fn uint64.to_float64_checked(x: UInt64) -> Float64?` | 新規 | R-sized-01 |
| `to_float_checked` | `fn uint64.to_float_checked(x: UInt64) -> Float?` | 新規 | R-sized-01 |
| `to_int` | `fn uint64.to_int(x: UInt64) -> Int` | 新規 | R-sized-01 |
| `to_int16` | `fn uint64.to_int16(x: UInt64) -> Int16` |  | R-sized-01（行列のセル） |
| `to_int16_checked` | `fn uint64.to_int16_checked(x: UInt64) -> Int16?` |  | R-sized-01（行列のセル） |
| `to_int16_saturating` | `fn uint64.to_int16_saturating(x: UInt64) -> Int16` |  | R-sized-01（行列のセル） |
| `to_int32` | `fn uint64.to_int32(x: UInt64) -> Int32` |  | R-sized-01（行列のセル） |
| `to_int32_checked` | `fn uint64.to_int32_checked(x: UInt64) -> Int32?` |  | R-sized-01（行列のセル） |
| `to_int32_saturating` | `fn uint64.to_int32_saturating(x: UInt64) -> Int32` |  | R-sized-01（行列のセル） |
| `to_int64` | `fn uint64.to_int64(x: UInt64) -> Int64` |  | R-sized-01（行列のセル） |
| `to_int64_checked` | `fn uint64.to_int64_checked(x: UInt64) -> Int64?` |  | R-sized-01（行列のセル） |
| `to_int64_saturating` | `fn uint64.to_int64_saturating(x: UInt64) -> Int64` |  | R-sized-01（行列のセル） |
| `to_int8` | `fn uint64.to_int8(x: UInt64) -> Int8` |  | R-sized-01（行列のセル） |
| `to_int8_checked` | `fn uint64.to_int8_checked(x: UInt64) -> Int8?` |  | R-sized-01（行列のセル） |
| `to_int8_saturating` | `fn uint64.to_int8_saturating(x: UInt64) -> Int8` |  | R-sized-01（行列のセル） |
| `to_int_checked` | `fn uint64.to_int_checked(x: UInt64) -> Int?` | 新規 | R-sized-01 |
| `to_int_saturating` | `fn uint64.to_int_saturating(x: UInt64) -> Int` | 新規 | R-sized-01 |
| `to_string` | `fn uint64.to_string(x: UInt64) -> String` |  |  |
| `to_string_radix` | `fn uint64.to_string_radix(x: UInt64, radix: Int) -> String` | 新規 | R-sized-06 |
| `to_uint16` | `fn uint64.to_uint16(x: UInt64) -> UInt16` |  | R-sized-01（行列のセル） |
| `to_uint16_checked` | `fn uint64.to_uint16_checked(x: UInt64) -> UInt16?` |  | R-sized-01（行列のセル） |
| `to_uint16_saturating` | `fn uint64.to_uint16_saturating(x: UInt64) -> UInt16` |  | R-sized-01（行列のセル） |
| `to_uint32` | `fn uint64.to_uint32(x: UInt64) -> UInt32` |  | R-sized-01（行列のセル） |
| `to_uint32_checked` | `fn uint64.to_uint32_checked(x: UInt64) -> UInt32?` |  | R-sized-01（行列のセル） |
| `to_uint32_saturating` | `fn uint64.to_uint32_saturating(x: UInt64) -> UInt32` |  | R-sized-01（行列のセル） |
| `to_uint8` | `fn uint64.to_uint8(x: UInt64) -> UInt8` |  | R-sized-01（行列のセル） |
| `to_uint8_checked` | `fn uint64.to_uint8_checked(x: UInt64) -> UInt8?` |  | R-sized-01（行列のセル） |
| `to_uint8_saturating` | `fn uint64.to_uint8_saturating(x: UInt64) -> UInt8` |  | R-sized-01（行列のセル） |

## uint8

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `compare` | `fn uint8.compare(a: UInt8, b: UInt8) -> Int` | 新規 | R-sized-02 |
| `max_value` | `fn uint8.max_value() -> UInt8` |  |  |
| `min_value` | `fn uint8.min_value() -> UInt8` |  |  |
| `parse` | `fn uint8.parse(s: String) -> Result[UInt8, String]` | 新規 | R-sized-02 |
| `parse_radix` | `fn uint8.parse_radix(s: String, radix: Int) -> Result[UInt8, String]` | 新規 | R-sized-06 |
| `to_float` | `fn uint8.to_float(x: UInt8) -> Float` | 新規 | R-sized-01 |
| `to_float32` | `fn uint8.to_float32(x: UInt8) -> Float32` |  | R-sized-01（行列のセル） |
| `to_float64` | `fn uint8.to_float64(x: UInt8) -> Float64` |  | R-sized-01（行列のセル） |
| `to_int` | `fn uint8.to_int(x: UInt8) -> Int` | 新規 | R-sized-01 |
| `to_int16` | `fn uint8.to_int16(x: UInt8) -> Int16` |  | R-sized-01（行列のセル） |
| `to_int32` | `fn uint8.to_int32(x: UInt8) -> Int32` |  | R-sized-01（行列のセル） |
| `to_int64` | `fn uint8.to_int64(x: UInt8) -> Int64` |  | R-sized-01（行列のセル） |
| `to_int8` | `fn uint8.to_int8(x: UInt8) -> Int8` |  | R-sized-01（行列のセル） |
| `to_int8_checked` | `fn uint8.to_int8_checked(x: UInt8) -> Int8?` |  | R-sized-01（行列のセル） |
| `to_int8_saturating` | `fn uint8.to_int8_saturating(x: UInt8) -> Int8` |  | R-sized-01（行列のセル） |
| `to_string` | `fn uint8.to_string(x: UInt8) -> String` |  |  |
| `to_string_radix` | `fn uint8.to_string_radix(x: UInt8, radix: Int) -> String` | 新規 | R-sized-06 |
| `to_uint16` | `fn uint8.to_uint16(x: UInt8) -> UInt16` |  | R-sized-01（行列のセル） |
| `to_uint32` | `fn uint8.to_uint32(x: UInt8) -> UInt32` |  | R-sized-01（行列のセル） |
| `to_uint64` | `fn uint8.to_uint64(x: UInt8) -> UInt64` |  | R-sized-01（行列のセル） |

## unix

新しいモジュールです（L1 で追加）。
記録: [L2-unix.md](L2-unix.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `accept` | `effect fn unix.accept(listener: Fd) -> Fd` | 新規 | R-unix-01 |
| `close` | `effect fn unix.close(fd: Fd) -> Unit` | 新規 | R-unix-01 |
| `connect` | `effect fn unix.connect(path: String) -> Fd` | 新規 | R-unix-01 |
| `listen` | `effect fn unix.listen(path: String) -> Fd` | 新規 | R-unix-01 |
| `read` | `effect fn unix.read(sock: Fd, max: Int) -> Bytes` | 新規 | R-unix-01、R-unix-03 |
| `select` | `effect fn unix.select(fds: List[Fd], timeout_ms: Int) -> List[Fd]` | 新規 | R-unix-01、R-unix-02 |
| `shm_create` | `effect fn unix.shm_create(size: Int) -> Fd` | 新規 | R-unix-01 |
| `shm_read` | `effect fn unix.shm_read(fd: Fd, offset: Int, len: Int) -> Bytes` | 新規 | R-unix-01 |
| `shm_resize` | `effect fn unix.shm_resize(fd: Fd, size: Int) -> Unit` | 新規 | R-unix-01 |
| `shm_write` | `effect fn unix.shm_write(fd: Fd, offset: Int, data: Bytes) -> Unit` | 新規 | R-unix-01 |
| `take_fds` | `effect fn unix.take_fds(sock: Fd) -> List[Fd]` | 新規 | R-unix-01 |
| `write` | `effect fn unix.write(sock: Fd, data: Bytes, fds: List[Fd]) -> Unit` | 新規 | R-unix-01、R-unix-03 |

## url

記録: [L2-url.md](L2-url.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `build_query` | `fn url.build_query(pairs: List[(String, String)]) -> String` |  | R-url-02 |
| `decode_component` | `fn url.decode_component(s: String) -> Result[String, String]` |  | R-url-01 |
| `decode_form` | `fn url.decode_form(s: String) -> Result[String, String]` | 新規 | R-url-01 |
| `encode_component` | `fn url.encode_component(s: String) -> String` |  | R-url-01 |
| `encode_form` | `fn url.encode_form(s: String) -> String` | 新規 | R-url-01 |
| `parse` | `fn url.parse(s: String) -> Result[Url, String]` | 意味変更: userinfo を受け取り、`Url` に `userinfo` の欄を足す | R-url-05 |
| `parse_query` | `fn url.parse_query(query: String) -> Result[List[(String, String)], String]` | 新規 | R-url-02 |
| `resolve` | `fn url.resolve(base: Url, ref: String) -> Result[Url, String]` | 新規 | R-url-03 |
| `to_string` | `fn url.to_string(u: Url) -> String` |  |  |

撤去（1）:

| 関数 | 代わり | 案 |
|---|---|---|
| `query_pairs` | `parse_query` | R-url-02 |

## uuid

新しいモジュールです（L1 で追加）。
記録: [L2-uuid.md](L2-uuid.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `compare` | `fn uuid.compare(a: Uuid, b: Uuid) -> Int` | 新規 | R-uuid-01 |
| `from_bytes` | `fn uuid.from_bytes(b: Bytes) -> Result[Uuid, String]` | 新規 | R-uuid-01 |
| `parse` | `fn uuid.parse(s: String) -> Result[Uuid, String]` | 新規 | R-uuid-01 |
| `to_bytes` | `fn uuid.to_bytes(u: Uuid) -> Bytes` | 新規 | R-uuid-01 |
| `to_string` | `fn uuid.to_string(u: Uuid) -> String` | 新規 | R-uuid-01 |
| `v4` | `effect fn uuid.v4() -> Uuid` | 新規 | R-uuid-01 |
| `v7` | `effect fn uuid.v7() -> Uuid` | 新規 | R-uuid-01 |

## value

記録: [L2-value.md](L2-value.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `array` | `fn value.array(items: List[Value]) -> Value` |  | R-value-02 |
| `as_array` | `fn value.as_array(v: Value) -> Result[List[Value], String]` |  | R-value-02 |
| `as_bool` | `fn value.as_bool(v: Value) -> Result[Bool, String]` |  | R-value-02 |
| `as_float` | `fn value.as_float(v: Value) -> Result[Float, String]` |  | R-value-02 |
| `as_int` | `fn value.as_int(v: Value) -> Result[Int, String]` |  | R-value-02 |
| `as_object` | `fn value.as_object(v: Value) -> Result[List[(String, Value)], String]` | 新規 | R-value-02 |
| `as_string` | `fn value.as_string(v: Value) -> Result[String, String]` |  | R-value-02 |
| `bool` | `fn value.bool(b: Bool) -> Value` |  | R-value-02 |
| `float` | `fn value.float(f: Float) -> Value` |  | R-value-02 |
| `get` | `fn value.get(v: Value, key: String) -> Value?` | 新規 | R-value-07 |
| `get_array` | `fn value.get_array(v: Value, key: String) -> List[Value]?` | 新規 | R-value-02 |
| `get_bool` | `fn value.get_bool(v: Value, key: String) -> Bool?` | 新規 | R-value-02 |
| `get_float` | `fn value.get_float(v: Value, key: String) -> Float?` | 新規 | R-value-02 |
| `get_int` | `fn value.get_int(v: Value, key: String) -> Int?` | 新規 | R-value-02 |
| `get_object` | `fn value.get_object(v: Value, key: String) -> List[(String, Value)]?` | 新規 | R-value-02 |
| `get_path` | `fn value.get_path(v: Value, p: Path) -> Value?` | 新規 | R-value-03 |
| `get_string` | `fn value.get_string(v: Value, key: String) -> String?` | 新規 | R-value-02 |
| `index` | `fn value.index(p: Path, i: Int) -> Path` | 新規 | R-value-03 |
| `int` | `fn value.int(n: Int) -> Value` |  | R-value-02 |
| `key` | `fn value.key(p: Path, name: String) -> Path` | 新規 | R-value-03 |
| `merge` | `fn value.merge(a: Value, b: Value) -> Value` |  |  |
| `null` | `fn value.null() -> Value` |  |  |
| `object` | `fn value.object(pairs: List[(String, Value)]) -> Value` |  | R-value-02 |
| `omit` | `fn value.omit(v: Value, keys: List[String]) -> Value` |  |  |
| `pick` | `fn value.pick(v: Value, keys: List[String]) -> Value` |  |  |
| `remove_path` | `fn value.remove_path(v: Value, p: Path) -> Value` | 新規 | R-value-03 |
| `root` | `fn value.root() -> Path` | 新規 | R-value-03 |
| `set_path` | `fn value.set_path(v: Value, p: Path, x: Value) -> Result[Value, String]` | 新規 | R-value-03 |
| `string` | `fn value.string(s: String) -> Value` | 改名（旧 `str`） | R-value-08 |

撤去（5）:

| 関数 | 代わり | 案 |
|---|---|---|
| `field` | `value.get`（Option） | R-value-07 |
| `keys` | `as_object` | R-value-04 |
| `stringify` | `json.stringify` | R-value-01 |
| `to_camel_case` | 代わりなし（利用者が書く） | R-value-05 |
| `to_snake_case` | 代わりなし（利用者が書く） | R-value-05 |

## zlib

記録: [L2-zlib.md](L2-zlib.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `compress` | `effect fn zlib.compress(data: Bytes) -> Bytes` |  | R-zlib-03 |
| `compress_level` | `effect fn zlib.compress_level(data: Bytes, level: Int) -> Bytes` |  | R-zlib-03 |
| `decompress` | `fn zlib.decompress(data: Bytes) -> Result[Bytes, String]` | 意味変更: effect を外し、`fn … -> Result[Bytes, String]` にする | R-zlib-02 |
| `deflate` | `effect fn zlib.deflate(data: Bytes) -> Bytes` |  | R-zlib-03 |
| `deflate_level` | `effect fn zlib.deflate_level(data: Bytes, level: Int) -> Bytes` |  | R-zlib-03 |
| `gunzip` | `fn zlib.gunzip(data: Bytes) -> Result[Bytes, String]` | 意味変更: effect を外し、`fn … -> Result[Bytes, String]` にする | R-zlib-02 |
| `gzip` | `effect fn zlib.gzip(data: Bytes) -> Bytes` |  | R-zlib-03 |
| `inflate` | `fn zlib.inflate(data: Bytes) -> Result[Bytes, String]` | 意味変更: effect を外し、`fn … -> Result[Bytes, String]` にする | R-zlib-02 |

撤去（2）:

| 関数 | 代わり | 案 |
|---|---|---|
| `adler32` | `hash.adler32` | R-zlib-01 |
| `crc32` | `hash.crc32` | R-zlib-01 |

## error（モジュールごと撤去）

| 関数 | 代わり | 案 |
|---|---|---|
| `chain` | `"${a}: ${b}"`（文字列の埋め込み） | R-error-01 |
| `context` | `result.map_err(r, (e) => "${msg}: ${e}")` | R-error-01、R-error-02 |
| `message` | `match`、または `result.to_err_option(r) ?? ""` | R-error-01 |
