<!-- description: Stdlib redesign — the standard library as it stands once every adopted proposal lands: every public function with its post-redesign signature, per module, and what was removed -->
# リデザイン後の標準ライブラリ（設計）

> Last updated: 2026-10-24
> 元の記録: [README.md](README.md)（案の索引）・各 L2 ページの L3 関数表

採用した案（○）をすべて適用したあとの、標準ライブラリの公開関数の一覧です。各 L2 ページの L3 関数表から機械的に組み立てています。表と食い違うときは、L3 の表と L2 の案が正です。

- 保留の案（zlib の出力の一致、`map.map` の callback の形など）は適用していません。いまの形のままです。採らなかった案と保留の案は、理由と一緒に最後の表にまとめています。
- シグネチャは `fn モジュール.名前(引数) -> 戻り型` の形にそろえています。`mut` はその場で書き換える引数、`T?` は `Option[T]` です（R-L0-24 の正規の綴り）。`effect fn` の戻り型は、docs/stdlib と同じく成功したときの型で書きます（失敗は `String` の err で、呼び出し側は `!` で伝搬します）。effect でない関数が失敗を返すときは `Result[T, String]` と書きます。
- 「変化」の列: 空欄は据え置き、新規は追加、改名は旧名つき、意味変更は何が変わるかを書きます。撤去した関数は、各モジュールの最後に移行先と一緒に載せます。
- 移行の扱いはすべて R-L0-14 のとおりです（epoch 15 で `@deprecated`、epoch 16 で撤去、`almide fix` かヒントで移行）。

## 一覧に出ない共通の約束

シグネチャだけでは読めない、全モジュールに共通する約束です。根拠は各ページと契約台帳（docs/contracts）にあります。

| 約束 | 中身 | 根拠 |
|---|---|---|
| 文字列の位置 | `string` の位置・長さ（`len`・`get`・`slice`・`index_of`）と `regex.index_of` は、どれもコードポイント単位です。バイト単位は `byte_` で始まる関数（`byte_len`・`byte_slice`）だけです。 | L2-string、L2-regex |
| `Float` の順序 | `float.compare`（`float32`・`float64` も）と、`list.sort`・`binary_search`・`heap` の既定の順序は、IEEE 754 の totalOrder です（`-0.0 < 0.0`）。NaN は型の幅ごとに 1 つの値（`Float`・`Float64` は `0x7ff8000000000000`、`Float32` は `0x7fc00000`）にそろえるので、NaN は正の無限大より大きく、NaN どうしは等しい、が正確に成り立ちます。そのかわり NaN のビットは往復しません。`to_bits`・`from_bits` と `bytes` の `get_f32_le`・`get_f64_le`・`append_f32_le`・`append_f64_le` などでも、どの NaN もこの値になります。NaN の中身ごと往復させるときは、`get_u32_le`・`get_u64_le` で整数として読み書きします。`<` 演算子だけは NaN との比較が false です。 | R-float-09、R-float-10 |
| `Map`・`Set` の走査順 | 入れた順です（同じキーを上書きしても位置は変わりません）。native と wasm で同じです。 | 契約台帳（Map・Set の挿入順） |
| 数学関数の値 | `math` の超越関数は、libm に任せず Almide で書いた同じ実装を両 target で通し、ビット単位で同じ値を返します。NaN は 1 つの決まった値です。 | ADR-0016、L2-math |
| `_checked` の変換 | 結果が元の値と正確に等しいときだけ some です。範囲外、小数部の切り捨て、精度の損失はどれも none です（`float.to_int32_checked(1.5)` も `int.to_float32_checked(16777217)` も none）。 | R-L0-28 |
| `next_` の none | 値が none のときは、受け取った状態（読み位置・乱数の状態）をそのまま返します。 | R-L0-27 |
| 文字列の本文 | `http.body` は、不正な UTF-8 を置換文字にして読みます（要求でも応答でも）。正確なバイトは `body_bytes` です。 | R-http-11、R-http-12 |
| `heap` の向き | 既定（`heap.new`・`from_list`）は最小が先です。最大を先にするときは `new_with` か `new_by` で順序を渡します。`heap.to_list` は `pop` を繰り返したのと同じ順で、同じ順位は入れた順です。 | L2-heap、R-L1-02 |
| `Matrix` の演算子 | `+`・`-`・`/`・単項 `-` は要素ごと、`*` は行列の積です（要素ごとの積は `matrix.zip_with`）。形が合わなければ abort です。 | R-matrix-14、L2-matrix |
| `min_value` | 整数の `min_value` は最も小さい値です。浮動小数点の `min_value` は Rust の `MIN` と同じ「最も小さい有限の値」（`-max_value()`）で、Java・Kotlin の「最小の正の値」ではありません。 | L2-float 完全性の規則 2 |
| 整数の端 | `int.bshl`・`bshr` のシフト量は 64 で割った余りです（`bshl(1, 64)` は 1）。`int.abs(int.min_value())` は `min_value` のままです（折り返し）。無印の変換 `to_<型>` は、変換先が整数なら、表せない値（範囲外。浮動小数点からは NaN も）で abort します。入力が整数でも浮動小数点でも同じです。小数部は 0 の方向に切り捨てます（`float.to_int(1.5)` は 1）。変換先が浮動小数点なら abort せず、最も近い値に丸めます（範囲外は無限大、NaN は NaN）。端に張り付かせるなら `_saturating`、整数どうしで下位のビットを残して折り返すなら `_wrapping`、正確に表せなければ none にするなら `_checked` です。`int.wrap(x, bits)` は `Int` の中の計算で、型の変換には使いません。 | R-L0-32、R-L0-34、R-L0-35 |
| `map.update` | キーが無ければ abort です（`list.update` の範囲外と同じ）。無いときに作るなら `upsert`、あるか分からないなら `map.contains` で確かめます。 | R-map-08 |
| 改行 | `io.print`・`io.eprint` は改行を付けません。付けるのは組み込みの `println`・`eprintln` です。 | L2-io |
| protocol の階層 | 標準の型は protocol の階層（[protocol-view.md](protocol-view.md)。手本は Swift）に準拠し、モジュールの関数はその protocol の関数そのものです。protocol が要件とする関数は、準拠する型のどれにも同じ名前・同じ形であります。`compare` を持つ型は `min`・`max`・`clamp` を、すべての値の型は `to_string` を、既定の実装で持ちます（この一覧には、型のページに行のある分だけ載せています）。 | R-L0-37、R-L0-38 |
| 範囲外の扱い | 関数の役割で決まります。範囲を取り出す関数（`slice`・`take`・`drop` など）は、長さを超えた分を丸め、負の位置・負の数は abort です。位置の 1 つを読む関数（`get`・`first`・`bytes.get_*`・`next_*`）は none です。位置の 1 つを書く関数（`set`・`update`・`insert`・`remove_at`・`bytes.put_*`）と、形を前提にする関数（`matrix` の演算、`bytes.xor`、`chunk(0)`）は abort です。 | R-L0-33、R-L0-29 |
| `zlib` の圧縮 | 圧縮した出力のバイト列は native と wasm で違います。これは「両 target で同じ結果」の約束がまだ果たせていない箇所で、`effect` を付けているのは目印にすぎず、解決ではありません。同じ実装に替えたときの速さと圧縮率を測ってから決めます。往復（圧縮して戻す）は両 target で同じです。 | R-zlib-03（保留、未達） |
| Codec の入力 | Codec のレコード型 `T` は、`T.decode` のほかに `T.decode_strict` を導出します。`decode` は知らないキーを無視し、`decode_strict` はフィールドを読む前に最初の知らないキーを `unknown key '<k>'` で拒みます。`@skip` のフィールドは読みも書きもせず、`@skip_decode` のフィールドは読まずに書きます（どちらも既定値が必要）。フィールドに置ける属性はこの 2 つと `@builtin`・`@location` だけで、ほかはエラーの E053 です。 | R-L0-48 |

## 全体の数

| いまの関数 | リデザイン後 | 据え置き | 新規 | 改名 | 意味変更 | 撤去 |
|---:|---:|---:|---:|---:|---:|---:|
| 1001 | 1304 | 409 | 720 | 88 | 87 | 417 |

固定幅の数（`int8`〜`uint64`、`float32`・`float64`）は型ごとに 1 関数と数えます。

## 実体の数

この一覧は、同じ protocol の関数を型ごとに 1 行と数えています（計 1,304 行）。[protocol の階層](protocol-view.md)で数え直すと、覚える名前はずっと少なくなります。

| 数え方 | 一覧の行 | 覚える名前 |
|---|---:|---:|
| protocol の要件（protocol ごとに 1 回） | 796 | 166 |
| 値の型だけが持つ関数 | 297 | 297 |
| 値の型を持たないモジュール（`fs`・`http`・`math` など） | 211 | 211 |
| 計 | 1304 | 674 |

## モジュールごとの数

| モジュール | リデザイン後 | 新規 | 改名 | 意味変更 | 撤去 | 記録 |
|---|---:|---:|---:|---:|---:|---|
| [`args`](#args) | 3 | 1 | 0 | 0 | 4 | [L2-args.md](L2-args.md) |
| [`base64`](#base64) | 4 | 0 | 0 | 2 | 0 | [L2-base64.md](L2-base64.md) |
| [`bigint`](#bigint)（新しいモジュール） | 24 | 24 | 0 | 0 | 0 | [L2-bigint.md](L2-bigint.md) |
| [`bool`](#bool) | 3 | 3 | 0 | 0 | 0 | [L2-bool.md](L2-bool.md) |
| [`bytes`](#bytes) | 178 | 64 | 72 | 6 | 34 | [L2-bytes.md](L2-bytes.md) |
| [`compute`](#compute) | 7 | 1 | 0 | 0 | 0 | [L2-compute.md](L2-compute.md) |
| [`datetime`](#datetime) | 18 | 1 | 0 | 1 | 5 | [L2-datetime.md](L2-datetime.md) |
| [`deque`](#deque)（新しいモジュール） | 60 | 60 | 0 | 0 | 0 | [L2-deque.md](L2-deque.md) |
| [`duration`](#duration) | 7 | 1 | 0 | 0 | 0 | [L2-duration.md](L2-duration.md) |
| [`env`](#env) | 8 | 2 | 0 | 0 | 3 | [L2-env.md](L2-env.md) |
| `error`（モジュールごと撤去） | 0 | 0 | 0 | 0 | 3 | [L2-error.md](L2-error.md) |
| [`ffi`](#ffi)（新しいモジュール） | 4 | 4 | 0 | 0 | 0 | [L2-ffi.md](L2-ffi.md) |
| [`float`](#float) | 26 | 12 | 1 | 0 | 32 | [L2-float.md](L2-float.md) |
| [`float32`](#float32) | 24 | 23 | 0 | 0 | 9 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`float64`](#float64) | 24 | 23 | 0 | 0 | 9 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`fs`](#fs) | 30 | 1 | 2 | 4 | 5 | [L2-fs.md](L2-fs.md) |
| [`hash`](#hash) | 19 | 16 | 0 | 2 | 1 | [L2-hash.md](L2-hash.md) |
| [`heap`](#heap)（新しいモジュール） | 16 | 16 | 0 | 0 | 0 | [L2-heap.md](L2-heap.md) |
| [`hex`](#hex) | 3 | 0 | 0 | 0 | 0 | [L2-hex.md](L2-hex.md) |
| [`html`](#html) | 5 | 0 | 0 | 0 | 0 | [L2-html.md](L2-html.md) |
| [`http`](#http) | 34 | 4 | 5 | 9 | 18 | [L2-http.md](L2-http.md) |
| [`int`](#int) | 45 | 14 | 2 | 4 | 41 | [L2-int.md](L2-int.md) |
| [`int16`](#int16) | 37 | 34 | 0 | 0 | 19 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`int32`](#int32) | 37 | 34 | 0 | 0 | 21 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`int64`](#int64) | 37 | 34 | 0 | 0 | 23 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`int8`](#int8) | 37 | 34 | 0 | 0 | 17 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`io`](#io) | 12 | 8 | 0 | 2 | 4 | [L2-io.md](L2-io.md) |
| [`json`](#json) | 3 | 0 | 0 | 1 | 12 | [L2-json.md](L2-json.md) |
| [`list`](#list) | 73 | 12 | 0 | 16 | 6 | [L2-list.md](L2-list.md) |
| [`map`](#map) | 37 | 15 | 1 | 7 | 3 | [L2-map.md](L2-map.md) |
| [`math`](#math) | 27 | 12 | 0 | 0 | 8 | [L2-math.md](L2-math.md) |
| [`matrix`](#matrix) | 25 | 3 | 0 | 8 | 24 | [L2-matrix.md](L2-matrix.md) |
| [`net`](#net) | 16 | 8 | 0 | 8 | 18 | [L2-net.md](L2-net.md) |
| [`option`](#option) | 13 | 0 | 0 | 0 | 1 | [L2-option.md](L2-option.md) |
| [`path`](#path) | 10 | 3 | 0 | 0 | 3 | [L2-path.md](L2-path.md) |
| [`process`](#process) | 13 | 8 | 0 | 2 | 12 | [L2-process.md](L2-process.md) |
| [`random`](#random) | 10 | 6 | 0 | 0 | 0 | [L2-random.md](L2-random.md) |
| [`regex`](#regex) | 15 | 7 | 1 | 7 | 0 | [L2-regex.md](L2-regex.md) |
| [`result`](#result) | 14 | 0 | 0 | 0 | 1 | [L2-result.md](L2-result.md) |
| [`secret`](#secret) | 2 | 2 | 0 | 0 | 0 | [L2-secret.md](L2-secret.md) |
| [`set`](#set) | 42 | 23 | 0 | 0 | 0 | [L2-set.md](L2-set.md) |
| [`string`](#string) | 86 | 41 | 3 | 2 | 5 | [L2-string.md](L2-string.md) |
| [`testing`](#testing) | 10 | 0 | 0 | 2 | 0 | [L2-testing.md](L2-testing.md) |
| [`uint16`](#uint16) | 35 | 32 | 0 | 0 | 15 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`uint32`](#uint32) | 35 | 32 | 0 | 0 | 19 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`uint64`](#uint64) | 35 | 32 | 0 | 0 | 23 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`uint8`](#uint8) | 35 | 32 | 0 | 0 | 11 | [L2-sized-numbers.md](L2-sized-numbers.md) |
| [`unix`](#unix)（新しいモジュール） | 10 | 10 | 0 | 0 | 0 | [L2-unix.md](L2-unix.md) |
| [`url`](#url) | 9 | 4 | 0 | 1 | 1 | [L2-url.md](L2-url.md) |
| [`uuid`](#uuid)（新しいモジュール） | 7 | 7 | 0 | 0 | 0 | [L2-uuid.md](L2-uuid.md) |
| [`value`](#value) | 29 | 14 | 1 | 0 | 5 | [L2-value.md](L2-value.md) |
| [`zlib`](#zlib) | 11 | 3 | 0 | 3 | 2 | [L2-zlib.md](L2-zlib.md) |

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
| `abs` | `fn bigint.abs(x: BigInt) -> BigInt` | 新規 | R-bigint-01 |
| `band` | `fn bigint.band(a: BigInt, b: BigInt) -> BigInt` | 新規 | R-bigint-07 |
| `bit_length` | `fn bigint.bit_length(x: BigInt) -> Int` | 新規 | R-int-11 |
| `bnot` | `fn bigint.bnot(x: BigInt) -> BigInt` | 新規 | R-bigint-07 |
| `bor` | `fn bigint.bor(a: BigInt, b: BigInt) -> BigInt` | 新規 | R-bigint-07 |
| `bshl` | `fn bigint.bshl(a: BigInt, n: Int) -> BigInt` | 新規 | R-bigint-07 |
| `bshr` | `fn bigint.bshr(a: BigInt, n: Int) -> BigInt` | 新規 | R-bigint-07 |
| `bxor` | `fn bigint.bxor(a: BigInt, b: BigInt) -> BigInt` | 新規 | R-bigint-07 |
| `clamp` | `fn bigint.clamp(x: BigInt, lo: BigInt, hi: BigInt) -> BigInt` | 新規 | R-bigint-05 |
| `compare` | `fn bigint.compare(a: BigInt, b: BigInt) -> Int` | 新規 | R-bigint-01 |
| `floor_div` | `fn bigint.floor_div(a: BigInt, b: BigInt) -> BigInt` | 新規 | R-bigint-01（R-int-03 と同じ綴り） |
| `from` | `fn bigint.from[S: Numeric](x: S) -> BigInt` | 新規 | R-L0-40 |
| `from_checked` | `fn bigint.from_checked[S: Numeric](x: S) -> BigInt?` | 新規 | R-L0-40 |
| `is_even` | `fn bigint.is_even(x: BigInt) -> Bool` | 新規 | R-bigint-01（`int.is_even` と同じ綴り） |
| `is_odd` | `fn bigint.is_odd(x: BigInt) -> Bool` | 新規 | R-bigint-01 |
| `max` | `fn bigint.max(a: BigInt, b: BigInt) -> BigInt` | 新規 | R-bigint-01 |
| `min` | `fn bigint.min(a: BigInt, b: BigInt) -> BigInt` | 新規 | R-bigint-01 |
| `modulo` | `fn bigint.modulo(a: BigInt, b: BigInt) -> BigInt` | 新規 | R-bigint-01（R-int-03 と同じ綴り） |
| `parse` | `fn bigint.parse(s: String) -> Result[BigInt, String]` | 新規 | R-bigint-01 |
| `parse_radix` | `fn bigint.parse_radix(s: String, radix: Int) -> Result[BigInt, String]` | 新規 | R-bigint-01 |
| `pow` | `fn bigint.pow(base: BigInt, exp: Int) -> BigInt` | 新規 | R-bigint-01 |
| `sign` | `fn bigint.sign(x: BigInt) -> Int` | 新規 | R-bigint-01 |
| `to_string` | `fn bigint.to_string(x: BigInt) -> String` | 新規 | R-bigint-01 |
| `to_string_radix` | `fn bigint.to_string_radix(x: BigInt, radix: Int) -> String` | 新規 | R-bigint-01 |

## bool

記録: [L2-bool.md](L2-bool.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `compare` | `fn bool.compare(a: Bool, b: Bool) -> Int` | 新規 | R-bool-02 |
| `parse` | `fn bool.parse(s: String) -> Result[Bool, String]` | 新規 | R-bool-01 |
| `to_string` | `fn bool.to_string(b: Bool) -> String` | 新規 | R-bool-03 |

## bytes

記録: [L2-bytes.md](L2-bytes.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `all` | `fn bytes.all(b: Bytes, f: (Int) -> Bool) -> Bool` | 新規 | R-bytes-18 |
| `any` | `fn bytes.any(b: Bytes, f: (Int) -> Bool) -> Bool` | 新規 | R-bytes-18 |
| `append_bool` | `fn bytes.append_bool(mut b: Bytes, x: Bool) -> Unit` | 改名（旧 `write_bool`） | R-bytes-03 |
| `append_bytes` | `fn bytes.append_bytes(mut b: Bytes, src: Bytes) -> Unit` | 新規 | R-bytes-05 |
| `append_f16_be` | `fn bytes.append_f16_be(mut b: Bytes, x: Float) -> Unit` | 新規 | R-bytes-03 |
| `append_f16_le` | `fn bytes.append_f16_le(mut b: Bytes, x: Float) -> Unit` | 新規 | R-bytes-03 |
| `append_f32_be` | `fn bytes.append_f32_be(mut b: Bytes, x: Float) -> Unit` |  |  |
| `append_f32_le` | `fn bytes.append_f32_le(mut b: Bytes, x: Float) -> Unit` |  |  |
| `append_f64_be` | `fn bytes.append_f64_be(mut b: Bytes, x: Float) -> Unit` |  |  |
| `append_f64_le` | `fn bytes.append_f64_le(mut b: Bytes, x: Float) -> Unit` |  |  |
| `append_i16_be` | `fn bytes.append_i16_be(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_i16_le` | `fn bytes.append_i16_le(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_i32_be` | `fn bytes.append_i32_be(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_i32_le` | `fn bytes.append_i32_le(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_i64_be` | `fn bytes.append_i64_be(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_i64_le` | `fn bytes.append_i64_le(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_i8` | `fn bytes.append_i8(mut b: Bytes, x: Int) -> Unit` | 新規 | R-bytes-15 |
| `append_u16_be` | `fn bytes.append_u16_be(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_u16_le` | `fn bytes.append_u16_le(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_u32_be` | `fn bytes.append_u32_be(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_u32_le` | `fn bytes.append_u32_le(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_u8` | `fn bytes.append_u8(mut b: Bytes, x: Int) -> Unit` |  | R-bytes-25 |
| `chunk` | `fn bytes.chunk(b: Bytes, n: Int) -> List[Bytes]` | 改名（旧 `chunks`） | R-bytes-08 |
| `clear` | `fn bytes.clear(mut b: Bytes) -> Unit` |  |  |
| `compare` | `fn bytes.compare(a: Bytes, b: Bytes) -> Int` | 改名（旧 `cmp`） | R-bytes-08 |
| `contains` | `fn bytes.contains(b: Bytes, needle: Bytes) -> Bool` |  |  |
| `copy_from` | `fn bytes.copy_from(mut b: Bytes, src: Bytes, dst_off: Int, src_off: Int, len: Int) -> Unit` |  |  |
| `copy_within` | `fn bytes.copy_within(mut b: Bytes, src_start: Int, src_end: Int, dst: Int) -> Unit` |  |  |
| `count` | `fn bytes.count(b: Bytes, f: (Int) -> Bool) -> Int` | 新規 | R-bytes-18 |
| `count_matches` | `fn bytes.count_matches(b: Bytes, needle: Bytes) -> Int` | 新規 | R-bytes-27 |
| `dedup` | `fn bytes.dedup(b: Bytes) -> Bytes` | 新規 | R-L0-42 |
| `drop` | `fn bytes.drop(b: Bytes, n: Int) -> Bytes` | 新規。負の数は abort。R-L0-29 | R-bytes-08 |
| `drop_end` | `fn bytes.drop_end(b: Bytes, n: Int) -> Bytes` | 新規。負の数は abort。R-L0-29 | R-bytes-08 |
| `drop_while` | `fn bytes.drop_while(b: Bytes, f: (Int) -> Bool) -> Bytes` | 新規 | R-L0-42 |
| `ends_with` | `fn bytes.ends_with(b: Bytes, suffix: Bytes) -> Bool` |  |  |
| `enumerate` | `fn bytes.enumerate(b: Bytes) -> List[(Int, Int)]` | 新規 | R-L0-42 |
| `fill` | `fn bytes.fill(mut b: Bytes, x: Int) -> Unit` |  |  |
| `filter` | `fn bytes.filter(b: Bytes, f: (Int) -> Bool) -> Bytes` | 新規 | R-bytes-18 |
| `filter_map` | `fn bytes.filter_map(b: Bytes, f: (Int) -> B?) -> List[B]` | 新規 | R-L0-42 |
| `find` | `fn bytes.find(b: Bytes, f: (Int) -> Bool) -> Int?` | 新規 | R-bytes-18 |
| `find_index` | `fn bytes.find_index(b: Bytes, f: (Int) -> Bool) -> Int?` | 新規 | R-L0-42 |
| `find_map` | `fn bytes.find_map(b: Bytes, f: (Int) -> B?) -> B?` | 新規 | R-L0-42 |
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
| `get_u8` | `fn bytes.get_u8(b: Bytes, pos: Int) -> Int?` | 新規 | R-bytes-25 |
| `group_by` | `fn bytes.group_by(b: Bytes, f: (Int) -> B) -> Map[B, List[Int]]` | 新規 | R-L0-42 |
| `index_of` | `fn bytes.index_of(b: Bytes, needle: Bytes) -> Int?` |  |  |
| `insert` | `fn bytes.insert(b: Bytes, i: Int, x: Int) -> Bytes` | 意味変更: `0..=len` の外は abort | R-bytes-11 |
| `intersperse` | `fn bytes.intersperse(b: Bytes, sep: Int) -> Bytes` | 新規 | R-L0-42 |
| `is_empty` | `fn bytes.is_empty(b: Bytes) -> Bool` |  |  |
| `is_equal_constant_time` | `fn bytes.is_equal_constant_time(a: Bytes, b: Bytes) -> Bool` | 新規 | R-bytes-29 |
| `is_eof` | `fn bytes.is_eof(b: Bytes, pos: Int) -> Bool` | 改名（旧 `eof`） | R-bytes-07 |
| `is_valid_utf8` | `fn bytes.is_valid_utf8(b: Bytes) -> Bool` |  |  |
| `last` | `fn bytes.last(b: Bytes) -> Int?` | 新規 | R-bytes-20 |
| `last_index_of` | `fn bytes.last_index_of(b: Bytes, needle: Bytes) -> Int?` | 新規 | R-bytes-27 |
| `len` | `fn bytes.len(b: Bytes) -> Int` |  |  |
| `map` | `fn bytes.map(b: Bytes, f: (Int) -> Int) -> Bytes` | 改名（旧 `map_each`） | R-bytes-08 |
| `max` | `fn bytes.max(b: Bytes) -> Int?` | 新規 | R-L0-42 |
| `max_by` | `fn bytes.max_by(b: Bytes, f: (Int) -> K) -> Int?` | 新規 | R-L0-42 |
| `max_with` | `fn bytes.max_with(b: Bytes, cmp: (Int, Int) -> Int) -> Int?` | 新規 | R-L0-42 |
| `min` | `fn bytes.min(b: Bytes) -> Int?` | 新規 | R-L0-42 |
| `min_by` | `fn bytes.min_by(b: Bytes, f: (Int) -> K) -> Int?` | 新規 | R-L0-42 |
| `min_with` | `fn bytes.min_with(b: Bytes, cmp: (Int, Int) -> Int) -> Int?` | 新規 | R-L0-42 |
| `new` | `fn bytes.new() -> Bytes` | 新規 | R-bytes-28 |
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
| `pad_end` | `fn bytes.pad_end(b: Bytes, n: Int, fill: Int) -> Bytes` | 改名（旧 `pad_right`） | R-bytes-08 |
| `pad_start` | `fn bytes.pad_start(b: Bytes, n: Int, fill: Int) -> Bytes` | 改名（旧 `pad_left`） | R-bytes-08 |
| `partition` | `fn bytes.partition(b: Bytes, f: (Int) -> Bool) -> (List[Int], List[Int])` | 新規 | R-L0-42 |
| `pop` | `fn bytes.pop(mut b: Bytes) -> Int?` | 新規 | R-bytes-20 |
| `product` | `fn bytes.product(b: Bytes) -> Int` | 新規 | R-L0-42 |
| `push` | `fn bytes.push(mut b: Bytes, x: Int) -> Unit` |  | R-bytes-02 |
| `put_bool` | `fn bytes.put_bool(mut b: Bytes, pos: Int, x: Bool) -> Unit` | 新規。範囲外は abort | R-bytes-03、R-bytes-11 |
| `put_f16_be` | `fn bytes.put_f16_be(mut b: Bytes, pos: Int, x: Float) -> Unit` | 新規 | R-bytes-03 |
| `put_f16_le` | `fn bytes.put_f16_le(mut b: Bytes, pos: Int, x: Float) -> Unit` | 新規 | R-bytes-03 |
| `put_f32_be` | `fn bytes.put_f32_be(mut b: Bytes, pos: Int, x: Float) -> Unit` | 改名（旧 `set_f32_be`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_f32_le` | `fn bytes.put_f32_le(mut b: Bytes, pos: Int, x: Float) -> Unit` | 改名（旧 `set_f32_le`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_f64_be` | `fn bytes.put_f64_be(mut b: Bytes, pos: Int, x: Float) -> Unit` | 改名（旧 `set_f64_be`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_f64_le` | `fn bytes.put_f64_le(mut b: Bytes, pos: Int, x: Float) -> Unit` | 改名（旧 `set_f64_le`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_i16_be` | `fn bytes.put_i16_be(mut b: Bytes, pos: Int, x: Int) -> Unit` | 改名（旧 `set_i16_be`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_i16_le` | `fn bytes.put_i16_le(mut b: Bytes, pos: Int, x: Int) -> Unit` | 改名（旧 `set_i16_le`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_i32_be` | `fn bytes.put_i32_be(mut b: Bytes, pos: Int, x: Int) -> Unit` | 改名（旧 `set_i32_be`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_i32_le` | `fn bytes.put_i32_le(mut b: Bytes, pos: Int, x: Int) -> Unit` | 改名（旧 `set_i32_le`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_i64_be` | `fn bytes.put_i64_be(mut b: Bytes, pos: Int, x: Int) -> Unit` | 改名（旧 `set_i64_be`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_i64_le` | `fn bytes.put_i64_le(mut b: Bytes, pos: Int, x: Int) -> Unit` | 改名（旧 `set_i64_le`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_i8` | `fn bytes.put_i8(mut b: Bytes, pos: Int, x: Int) -> Unit` | 新規 | R-bytes-15 |
| `put_u16_be` | `fn bytes.put_u16_be(mut b: Bytes, pos: Int, x: Int) -> Unit` | 改名（旧 `set_u16_be`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_u16_le` | `fn bytes.put_u16_le(mut b: Bytes, pos: Int, x: Int) -> Unit` | 改名（旧 `set_u16_le`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_u32_be` | `fn bytes.put_u32_be(mut b: Bytes, pos: Int, x: Int) -> Unit` | 改名（旧 `set_u32_be`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_u32_le` | `fn bytes.put_u32_le(mut b: Bytes, pos: Int, x: Int) -> Unit` | 改名（旧 `set_u32_le`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `put_u8` | `fn bytes.put_u8(mut b: Bytes, pos: Int, x: Int) -> Unit` | 改名（旧 `set_u8`）。範囲外は abort | R-bytes-01、R-bytes-11 |
| `reduce` | `fn bytes.reduce(b: Bytes, f: (Int, Int) -> Int) -> Int?` | 新規 | R-L0-42 |
| `remove_at` | `fn bytes.remove_at(b: Bytes, i: Int) -> Bytes` | 意味変更: 範囲外は abort | R-bytes-11 |
| `repeat` | `fn bytes.repeat(b: Bytes, n: Int) -> Bytes` |  |  |
| `replace` | `fn bytes.replace(b: Bytes, from: Bytes, to: Bytes) -> Bytes` | 新規 | R-bytes-27 |
| `replace_first` | `fn bytes.replace_first(b: Bytes, from: Bytes, to: Bytes) -> Bytes` | 新規 | R-bytes-27 |
| `reverse` | `fn bytes.reverse(b: Bytes) -> Bytes` |  |  |
| `scan` | `fn bytes.scan(b: Bytes, init: B, f: (B, Int) -> B) -> List[B]` | 新規 | R-L0-42 |
| `set` | `fn bytes.set(b: Bytes, i: Int, x: Int) -> Bytes` | 意味変更: 範囲外は abort | R-bytes-11 |
| `skip` | `fn bytes.skip(b: Bytes, pos: Int, n: Int) -> Int` | 意味変更: `n < 0` は abort | R-bytes-13、R-bytes-21 |
| `slice` | `fn bytes.slice(b: Bytes, start: Int, end: Int) -> Bytes` | 意味変更: 負の位置は abort。R-L0-29 | R-bytes-12、R-L0-29 |
| `sort` | `fn bytes.sort(b: Bytes) -> List[Int]` | 新規 | R-L0-42 |
| `sort_by` | `fn bytes.sort_by(b: Bytes, f: (Int) -> B) -> List[Int]` | 新規 | R-L0-42 |
| `sort_with` | `fn bytes.sort_with(b: Bytes, cmp: (Int, Int) -> Int) -> List[Int]` | 新規 | R-L0-42 |
| `split` | `fn bytes.split(b: Bytes, sep: Bytes) -> List[Bytes]` |  |  |
| `split_at` | `fn bytes.split_at(b: Bytes, n: Int) -> (Bytes, Bytes)` | 新規 | R-L0-42 |
| `starts_with` | `fn bytes.starts_with(b: Bytes, prefix: Bytes) -> Bool` |  |  |
| `strip_prefix` | `fn bytes.strip_prefix(b: Bytes, prefix: Bytes) -> Bytes?` | 新規 | R-bytes-27 |
| `strip_suffix` | `fn bytes.strip_suffix(b: Bytes, suffix: Bytes) -> Bytes?` | 新規 | R-bytes-27 |
| `sum` | `fn bytes.sum(b: Bytes) -> Int` | 新規 | R-L0-42 |
| `take` | `fn bytes.take(b: Bytes, n: Int) -> Bytes` | 新規。負の数は abort。R-L0-29 | R-bytes-08 |
| `take_end` | `fn bytes.take_end(b: Bytes, n: Int) -> Bytes` | 新規。負の数は abort。R-L0-29 | R-bytes-08 |
| `take_while` | `fn bytes.take_while(b: Bytes, f: (Int) -> Bool) -> Bytes` | 新規 | R-L0-42 |
| `to_list` | `fn bytes.to_list(b: Bytes) -> List[Int]` |  |  |
| `to_string` | `fn bytes.to_string(b: Bytes) -> Result[String, String]` |  | R-bytes-10 |
| `to_string_lossy` | `fn bytes.to_string_lossy(b: Bytes) -> String` |  | R-bytes-10 |
| `unique` | `fn bytes.unique(b: Bytes) -> Bytes` | 新規 | R-L0-42 |
| `unique_by` | `fn bytes.unique_by(b: Bytes, f: (Int) -> K) -> Bytes` | 新規 | R-L0-42 |
| `window` | `fn bytes.window(b: Bytes, n: Int) -> List[Bytes]` | 新規 | R-L0-42 |
| `xor` | `fn bytes.xor(a: Bytes, b: Bytes) -> Bytes` | 意味変更: 長さが違うと abort | R-bytes-23 |
| `zeros` | `fn bytes.zeros(n: Int) -> Bytes` | 改名（旧 `new`） | R-bytes-28 |
| `zip` | `fn bytes.zip(b: Bytes, ys: List[B]) -> List[(Int, B)]` | 新規 | R-L0-42 |
| `zip_with` | `fn bytes.zip_with(b: Bytes, ys: List[B], f: (Int, B) -> C) -> List[C]` | 新規 | R-L0-42 |

撤去（34）:

| 関数 | 代わり | 案 |
|---|---|---|
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
| `chunk` | `fn deque.chunk(d: Deque[A], n: Int) -> List[Deque[A]]` | 新規 | R-L0-42 |
| `clear` | `fn deque.clear[A](mut d: Deque[A]) -> Unit` | 新規 | R-deque-02、R-L0-04 |
| `contains` | `fn deque.contains[A](d: Deque[A], x: A) -> Bool` | 新規 | R-deque-02 |
| `count` | `fn deque.count[A](d: Deque[A], f: (A) -> Bool) -> Int` | 新規 | R-deque-02 |
| `dedup` | `fn deque.dedup(d: Deque[A]) -> Deque[A]` | 新規 | R-L0-42 |
| `drop` | `fn deque.drop[A](d: Deque[A], n: Int) -> Deque[A]` | 新規 | R-deque-04 |
| `drop_end` | `fn deque.drop_end[A](d: Deque[A], n: Int) -> Deque[A]` | 新規 | R-deque-04 |
| `drop_while` | `fn deque.drop_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]` | 新規 | R-L0-42 |
| `enumerate` | `fn deque.enumerate(d: Deque[A]) -> List[(Int, A)]` | 新規 | R-L0-42 |
| `filter` | `fn deque.filter[A](d: Deque[A], f: (A) -> Bool) -> Deque[A]` | 新規 | R-deque-02 |
| `filter_map` | `fn deque.filter_map(d: Deque[A], f: (A) -> B?) -> List[B]` | 新規 | R-L0-42 |
| `find` | `fn deque.find[A](d: Deque[A], f: (A) -> Bool) -> A?` | 新規 | R-deque-02 |
| `find_index` | `fn deque.find_index(d: Deque[A], f: (A) -> Bool) -> Int?` | 新規 | R-L0-42 |
| `find_map` | `fn deque.find_map(d: Deque[A], f: (A) -> B?) -> B?` | 新規 | R-L0-42 |
| `first` | `fn deque.first[A](d: Deque[A]) -> A?` | 新規 | R-deque-01 |
| `fold` | `fn deque.fold[A, B](d: Deque[A], init: B, f: (B, A) -> B) -> B` | 新規 | R-deque-02 |
| `from_list` | `fn deque.from_list[A](xs: List[A]) -> Deque[A]` | 新規 | R-deque-01 |
| `get` | `fn deque.get[A](d: Deque[A], i: Int) -> A?` | 新規 | R-deque-01 |
| `group_by` | `fn deque.group_by(d: Deque[A], f: (A) -> B) -> Map[B, List[A]]` | 新規 | R-L0-42 |
| `insert` | `fn deque.insert[A](d: Deque[A], i: Int, x: A) -> Deque[A]` | 新規 | R-deque-04 |
| `intersperse` | `fn deque.intersperse(d: Deque[A], sep: A) -> Deque[A]` | 新規 | R-L0-42 |
| `is_empty` | `fn deque.is_empty[A](d: Deque[A]) -> Bool` | 新規 | R-deque-01 |
| `last` | `fn deque.last[A](d: Deque[A]) -> A?` | 新規 | R-deque-01 |
| `len` | `fn deque.len[A](d: Deque[A]) -> Int` | 新規 | R-deque-01 |
| `map` | `fn deque.map[A, B](d: Deque[A], f: (A) -> B) -> Deque[B]` | 新規 | R-deque-02 |
| `max` | `fn deque.max(d: Deque[A]) -> A?` | 新規 | R-L0-42 |
| `max_by` | `fn deque.max_by(d: Deque[A], f: (A) -> K) -> A?` | 新規 | R-L0-42 |
| `max_with` | `fn deque.max_with(d: Deque[A], cmp: (A, A) -> Int) -> A?` | 新規 | R-L0-42 |
| `min` | `fn deque.min(d: Deque[A]) -> A?` | 新規 | R-L0-42 |
| `min_by` | `fn deque.min_by(d: Deque[A], f: (A) -> K) -> A?` | 新規 | R-L0-42 |
| `min_with` | `fn deque.min_with(d: Deque[A], cmp: (A, A) -> Int) -> A?` | 新規 | R-L0-42 |
| `new` | `fn deque.new[A]() -> Deque[A]` | 新規 | R-deque-01 |
| `partition` | `fn deque.partition(d: Deque[A], f: (A) -> Bool) -> (List[A], List[A])` | 新規 | R-L0-42 |
| `pop` | `fn deque.pop[A](mut d: Deque[A]) -> A?` | 新規 | R-deque-01、R-deque-03 |
| `pop_front` | `fn deque.pop_front[A](mut d: Deque[A]) -> A?` | 新規 | R-deque-01 |
| `product` | `fn deque.product[A: Numeric](d: Deque[A]) -> A` | 新規 | R-L0-42 |
| `push` | `fn deque.push[A](mut d: Deque[A], x: A) -> Unit` | 新規 | R-deque-01、R-deque-03 |
| `push_front` | `fn deque.push_front[A](mut d: Deque[A], x: A) -> Unit` | 新規 | R-deque-01 |
| `reduce` | `fn deque.reduce(d: Deque[A], f: (A, A) -> A) -> A?` | 新規 | R-L0-42 |
| `remove_at` | `fn deque.remove_at[A](d: Deque[A], i: Int) -> Deque[A]` | 新規 | R-deque-04 |
| `reverse` | `fn deque.reverse[A](d: Deque[A]) -> Deque[A]` | 新規 | R-deque-04 |
| `scan` | `fn deque.scan(d: Deque[A], init: B, f: (B, A) -> B) -> List[B]` | 新規 | R-L0-42 |
| `set` | `fn deque.set[A](d: Deque[A], i: Int, x: A) -> Deque[A]` | 新規 | R-deque-04 |
| `slice` | `fn deque.slice[A](d: Deque[A], start: Int, end: Int) -> Deque[A]` | 新規 | R-deque-04 |
| `sort` | `fn deque.sort(d: Deque[A]) -> List[A]` | 新規 | R-L0-42 |
| `sort_by` | `fn deque.sort_by(d: Deque[A], f: (A) -> B) -> List[A]` | 新規 | R-L0-42 |
| `sort_with` | `fn deque.sort_with(d: Deque[A], cmp: (A, A) -> Int) -> List[A]` | 新規 | R-L0-42 |
| `split_at` | `fn deque.split_at(d: Deque[A], n: Int) -> (Deque[A], Deque[A])` | 新規 | R-L0-42 |
| `sum` | `fn deque.sum[A: Numeric](d: Deque[A]) -> A` | 新規 | R-L0-42 |
| `take` | `fn deque.take[A](d: Deque[A], n: Int) -> Deque[A]` | 新規 | R-deque-04 |
| `take_end` | `fn deque.take_end[A](d: Deque[A], n: Int) -> Deque[A]` | 新規 | R-deque-04 |
| `take_while` | `fn deque.take_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]` | 新規 | R-L0-42 |
| `to_list` | `fn deque.to_list[A](d: Deque[A]) -> List[A]` | 新規 | R-deque-01 |
| `unique` | `fn deque.unique(d: Deque[A]) -> Deque[A]` | 新規 | R-L0-42 |
| `unique_by` | `fn deque.unique_by(d: Deque[A], f: (A) -> K) -> Deque[A]` | 新規 | R-L0-42 |
| `window` | `fn deque.window(d: Deque[A], n: Int) -> List[Deque[A]]` | 新規 | R-L0-42 |
| `zip` | `fn deque.zip(d: Deque[A], ys: List[B]) -> List[(A, B)]` | 新規 | R-L0-42 |
| `zip_with` | `fn deque.zip_with(d: Deque[A], ys: List[B], f: (A, B) -> C) -> List[C]` | 新規 | R-L0-42 |

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
| `address` | `fn ffi.address(b: Bytes) -> RawPtr` | 新規。`bytes.as_ptr`・`as_mut_ptr` から | R-ffi-01 |
| `read` | `effect fn ffi.read(ptr: RawPtr, len: Int) -> Bytes` | 新規。`bytes.from_raw_ptr` から | R-ffi-01 |
| `to_int` | `fn ffi.to_int(p: RawPtr) -> Int` | 新規。`bytes.data_ptr` の役割 | R-ffi-01 |
| `write` | `effect fn ffi.write(ptr: RawPtr, b: Bytes, cap: Int) -> Int` | 新規。`bytes.copy_to_ptr` から。書き込む先を先に | R-ffi-01、R-ffi-03 |

## float

記録: [L2-float.md](L2-float.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `abs` | `fn float.abs(x: Float) -> Float` |  |  |
| `ceil` | `fn float.ceil(x: Float) -> Float` |  |  |
| `clamp` | `fn float.clamp(x: Float, lo: Float, hi: Float) -> Float` |  |  |
| `compare` | `fn float.compare(a: Float, b: Float) -> Int` | 新規 | R-sized-02 |
| `epsilon` | `fn float.epsilon() -> Float` | 新規 | R-float-03 |
| `floor` | `fn float.floor(x: Float) -> Float` |  |  |
| `from` | `fn float.from[S: Numeric](x: S) -> Float` | 新規 | R-L0-40 |
| `from_bits` | `fn float.from_bits(bits: Int) -> Float` | 新規 | R-float-02 |
| `from_checked` | `fn float.from_checked[S: Numeric](x: S) -> Float?` | 新規 | R-L0-40 |
| `infinity` | `fn float.infinity() -> Float` | 新規 | R-float-03 |
| `is_finite` | `fn float.is_finite(x: Float) -> Bool` | 新規 | R-float-04 |
| `is_infinite` | `fn float.is_infinite(x: Float) -> Bool` |  |  |
| `is_nan` | `fn float.is_nan(x: Float) -> Bool` |  |  |
| `max` | `fn float.max(a: Float, b: Float) -> Float` |  | R-math-01（`math.fmax` の残る側） |
| `max_value` | `fn float.max_value() -> Float` | 新規 | R-float-03 |
| `min` | `fn float.min(a: Float, b: Float) -> Float` |  | R-math-01（`math.fmin` の残る側） |
| `min_value` | `fn float.min_value() -> Float` | 新規 | R-float-03 |
| `nan` | `fn float.nan() -> Float` | 新規 | R-float-03 |
| `parse` | `fn float.parse(s: String) -> Result[Float, String]` |  |  |
| `pow` | `fn float.pow(base: Float, exp: Float) -> Float` | 新規 | R-math-01 |
| `round` | `fn float.round(x: Float) -> Float` |  |  |
| `signum` | `fn float.signum(x: Float) -> Float` | 改名（旧 `sign`）。動きは変えない | R-float-05 |
| `to_bits` | `fn float.to_bits(f: Float) -> Int` |  | R-float-02（`from_bits` と対にする） |
| `to_fixed` | `fn float.to_fixed(x: Float, decimals: Int) -> String` |  |  |
| `to_string` | `fn float.to_string(x: Float) -> String` |  |  |
| `trunc` | `fn float.trunc(x: Float) -> Float` | 新規 | R-float-04 |

撤去（32）:

| 関数 | 代わり | 案 |
|---|---|---|
| `from_float32` | `float32.to_float` | R-float-01（`float32.to_float`） |
| `from_float64` | `float64.to_float` | R-float-01（`float64.to_float`） |
| `from_int` | `int.to_float` と同じ | R-float-01（`int.to_float` と同じ。R-L0-07 の 3） |
| `sqrt` | `math.sqrt` | R-math-03 |
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float32_checked` | `float32.from_checked` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int` | `int.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int16_checked` | `int16.from_checked` | R-L0-40 |
| `to_int16_saturating` | `int16.from_saturating` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int32_checked` | `int32.from_checked` | R-L0-40 |
| `to_int32_saturating` | `int32.from_saturating` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int64_checked` | `int64.from_checked` | R-L0-40 |
| `to_int64_saturating` | `int64.from_saturating` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_int8_checked` | `int8.from_checked` | R-L0-40 |
| `to_int8_saturating` | `int8.from_saturating` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint16_checked` | `uint16.from_checked` | R-L0-40 |
| `to_uint16_saturating` | `uint16.from_saturating` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint32_checked` | `uint32.from_checked` | R-L0-40 |
| `to_uint32_saturating` | `uint32.from_saturating` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint64_checked` | `uint64.from_checked` | R-L0-40 |
| `to_uint64_saturating` | `uint64.from_saturating` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |
| `to_uint8_checked` | `uint8.from_checked` | R-L0-40 |
| `to_uint8_saturating` | `uint8.from_saturating` | R-L0-40 |

## float32

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `abs` | `fn float32.abs(x: Float32) -> Float32` | 新規 | R-sized-11 |
| `ceil` | `fn float32.ceil(x: Float32) -> Float32` | 新規 | R-sized-11 |
| `clamp` | `fn float32.clamp(x: Float32, lo: Float32, hi: Float32) -> Float32` | 新規 | R-sized-08 |
| `compare` | `fn float32.compare(a: Float32, b: Float32) -> Int` | 新規 | R-sized-02 |
| `epsilon` | `fn float32.epsilon() -> Float32` | 新規 | R-sized-03 |
| `floor` | `fn float32.floor(x: Float32) -> Float32` | 新規 | R-sized-11 |
| `from` | `fn float32.from[S: Numeric](x: S) -> Float32` | 新規 | R-L0-40 |
| `from_bits` | `fn float32.from_bits(bits: Int) -> Float32` | 新規 | R-sized-03 |
| `from_checked` | `fn float32.from_checked[S: Numeric](x: S) -> Float32?` | 新規 | R-L0-40 |
| `infinity` | `fn float32.infinity() -> Float32` | 新規 | R-sized-03 |
| `is_finite` | `fn float32.is_finite(x: Float32) -> Bool` | 新規 | R-sized-08 |
| `is_infinite` | `fn float32.is_infinite(x: Float32) -> Bool` | 新規 | R-sized-08 |
| `is_nan` | `fn float32.is_nan(x: Float32) -> Bool` | 新規 | R-sized-08 |
| `max` | `fn float32.max(a: Float32, b: Float32) -> Float32` | 新規 | R-sized-08 |
| `max_value` | `fn float32.max_value() -> Float32` | 新規 | R-sized-03 |
| `min` | `fn float32.min(a: Float32, b: Float32) -> Float32` | 新規 | R-sized-08 |
| `min_value` | `fn float32.min_value() -> Float32` | 新規 | R-sized-03 |
| `nan` | `fn float32.nan() -> Float32` | 新規 | R-sized-03 |
| `parse` | `fn float32.parse(s: String) -> Result[Float32, String]` | 新規 | R-sized-02 |
| `round` | `fn float32.round(x: Float32) -> Float32` | 新規 | R-sized-11 |
| `signum` | `fn float32.signum(x: Float32) -> Float32` | 新規 | R-sized-11 |
| `to_bits` | `fn float32.to_bits(x: Float32) -> Int` | 新規 | R-sized-03 |
| `to_string` | `fn float32.to_string(x: Float32) -> String` |  |  |
| `trunc` | `fn float32.trunc(x: Float32) -> Float32` | 新規 | R-sized-11 |

撤去（9）:

| 関数 | 代わり | 案 |
|---|---|---|
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |

## float64

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `abs` | `fn float64.abs(x: Float64) -> Float64` | 新規 | R-sized-11 |
| `ceil` | `fn float64.ceil(x: Float64) -> Float64` | 新規 | R-sized-11 |
| `clamp` | `fn float64.clamp(x: Float64, lo: Float64, hi: Float64) -> Float64` | 新規 | R-sized-08 |
| `compare` | `fn float64.compare(a: Float64, b: Float64) -> Int` | 新規 | R-sized-02 |
| `epsilon` | `fn float64.epsilon() -> Float64` | 新規 | R-sized-03 |
| `floor` | `fn float64.floor(x: Float64) -> Float64` | 新規 | R-sized-11 |
| `from` | `fn float64.from[S: Numeric](x: S) -> Float64` | 新規 | R-L0-40 |
| `from_bits` | `fn float64.from_bits(bits: Int) -> Float64` | 新規 | R-sized-03 |
| `from_checked` | `fn float64.from_checked[S: Numeric](x: S) -> Float64?` | 新規 | R-L0-40 |
| `infinity` | `fn float64.infinity() -> Float64` | 新規 | R-sized-03 |
| `is_finite` | `fn float64.is_finite(x: Float64) -> Bool` | 新規 | R-sized-08 |
| `is_infinite` | `fn float64.is_infinite(x: Float64) -> Bool` | 新規 | R-sized-08 |
| `is_nan` | `fn float64.is_nan(x: Float64) -> Bool` | 新規 | R-sized-08 |
| `max` | `fn float64.max(a: Float64, b: Float64) -> Float64` | 新規 | R-sized-08 |
| `max_value` | `fn float64.max_value() -> Float64` | 新規 | R-sized-03 |
| `min` | `fn float64.min(a: Float64, b: Float64) -> Float64` | 新規 | R-sized-08 |
| `min_value` | `fn float64.min_value() -> Float64` | 新規 | R-sized-03 |
| `nan` | `fn float64.nan() -> Float64` | 新規 | R-sized-03 |
| `parse` | `fn float64.parse(s: String) -> Result[Float64, String]` | 新規 | R-sized-02 |
| `round` | `fn float64.round(x: Float64) -> Float64` | 新規 | R-sized-11 |
| `signum` | `fn float64.signum(x: Float64) -> Float64` | 新規 | R-sized-11 |
| `to_bits` | `fn float64.to_bits(x: Float64) -> Int` | 新規 | R-sized-03 |
| `to_string` | `fn float64.to_string(x: Float64) -> String` |  |  |
| `trunc` | `fn float64.trunc(x: Float64) -> Float64` | 新規 | R-sized-11 |

撤去（9）:

| 関数 | 代わり | 案 |
|---|---|---|
| `to_float32` | `float32.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |

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
| `fold_lines_range` | `effect fn fs.fold_lines_range[A](path: String, start: Int, end: Int, init: A, f: (A, String) -> A) -> A` | 意味変更: 負の位置で abort | R-L0-29、R-L0-46 |
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
| `fnv1a32` | `fn hash.fnv1a32(b: Bytes) -> Int` | 意味変更: 引数を `Bytes` に。`fnv1a32(bytes.from_string(s))` | R-hash-01 |
| `hmac_sha256` | `fn hash.hmac_sha256(key: Bytes, b: Bytes) -> Bytes` | 新規 | R-hash-06 |
| `hmac_sha256_hex` | `fn hash.hmac_sha256_hex(key: Bytes, b: Bytes) -> String` | 新規 | R-hash-06 |
| `hmac_sha512` | `fn hash.hmac_sha512(key: Bytes, b: Bytes) -> Bytes` | 新規 | R-hash-06 |
| `hmac_sha512_hex` | `fn hash.hmac_sha512_hex(key: Bytes, b: Bytes) -> String` | 新規 | R-hash-06 |
| `is_valid_hmac_sha256` | `fn hash.is_valid_hmac_sha256(key: Bytes, b: Bytes, tag: Bytes) -> Bool` | 新規 | R-hash-06 |
| `is_valid_hmac_sha256_hex` | `fn hash.is_valid_hmac_sha256_hex(key: Bytes, b: Bytes, tag: String) -> Bool` | 新規 | R-hash-06 |
| `is_valid_hmac_sha512` | `fn hash.is_valid_hmac_sha512(key: Bytes, b: Bytes, tag: Bytes) -> Bool` | 新規 | R-hash-06 |
| `is_valid_hmac_sha512_hex` | `fn hash.is_valid_hmac_sha512_hex(key: Bytes, b: Bytes, tag: String) -> Bool` | 新規 | R-hash-06 |
| `md5` | `fn hash.md5(b: Bytes) -> Bytes` | 新規 | R-hash-03 |
| `md5_hex` | `fn hash.md5_hex(b: Bytes) -> String` | 新規。`Bytes` を受け取る | R-hash-03、R-hash-05 |
| `sha1` | `fn hash.sha1(b: Bytes) -> Bytes` | 新規 | R-hash-03 |
| `sha1_hex` | `fn hash.sha1_hex(b: Bytes) -> String` | 新規。`Bytes` を受け取る | R-hash-03、R-hash-05 |
| `sha256` | `fn hash.sha256(b: Bytes) -> Bytes` |  |  |
| `sha256_hex` | `fn hash.sha256_hex(b: Bytes) -> String` | 意味変更: `Bytes` を受け取る | R-hash-05 |
| `sha512` | `fn hash.sha512(b: Bytes) -> Bytes` | 新規 | R-hash-03 |
| `sha512_hex` | `fn hash.sha512_hex(b: Bytes) -> String` | 新規。`Bytes` を受け取る | R-hash-03、R-hash-05 |

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
| `new_by` | `fn heap.new_by[A, K](f: (A) -> K) -> Heap[A]` | 新規 | R-heap-01 |
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
| `body` | `fn http.body[M: HttpMessage](m: M) -> String` | 意味変更: `HttpRequest` も受け取る | R-http-12 |
| `body_bytes` | `fn http.body_bytes[M: HttpMessage](m: M) -> Bytes` | 新規。`HttpRequest` も受け取る | R-http-12 |
| `cancel` | `effect fn http.cancel(c: HttpCall) -> Unit` |  | R-http-01 |
| `decode_json` | `fn http.decode_json[A](req: HttpRequest, decode: (Value) -> Result[A, String]) -> Result[A, HttpResponse]` |  |  |
| `get` | `effect fn http.get(url: String) -> String` |  | R-http-01 |
| `get_response` | `effect fn http.get_response(url: String) -> HttpResponse` |  | R-http-01 |
| `header` | `fn http.header[M: HttpMessage](m: M, name: String) -> String?` | 改名（旧 `get_header`）。`HttpRequest` も受け取る | R-http-12 |
| `header_values` | `fn http.header_values[M: HttpMessage](m: M, name: String) -> List[String]` | 意味変更: `HttpRequest` も受け取る | R-http-12 |
| `headers` | `fn http.headers[M: HttpMessage](m: M) -> List[(String, String)]` | 意味変更: `HttpRequest` も受け取る | R-http-12 |
| `json` | `fn http.json(status: Int, body: String) -> HttpResponse` |  |  |
| `method` | `fn http.method(req: HttpRequest) -> String` | 改名（旧 `req_method`） | R-http-14 |
| `mount` | `fn http.mount(prefix: String, sub: (HttpRequest) -> Result[HttpResponse, String]) -> HttpRoute` |  |  |
| `new_request` | `fn http.new_request(method: String, target: String, body: String, headers: List[(String, String)]) -> HttpRequest` | 意味変更: `headers` を `List[(String, String)]` に | R-http-09 |
| `param` | `fn http.param(req: HttpRequest, name: String) -> String?` |  |  |
| `path` | `fn http.path(req: HttpRequest) -> String` | 改名（旧 `req_path`）。クエリを含むことを文書に明記 | R-http-14 |
| `poll` | `effect fn http.poll(c: HttpCall) -> Result[HttpResponse, String]?` |  | R-http-01 |
| `query_params` | `fn http.query_params(req: HttpRequest) -> Result[List[(String, String)], String]` | 意味変更: `url.parse_query` と同じ一つの復号器（不正なエスケープは err、#4117、C-404） | R-http-10 |
| `read_new` | `effect fn http.read_new(c: HttpCall) -> String` |  | R-http-01 |
| `redirect` | `fn http.redirect(url: String) -> HttpResponse` |  |  |
| `request` | `effect fn http.request(method: String, url: String, body: String, headers: List[(String, String)]) -> String` | 意味変更: `headers` を `List[(String, String)]` に | R-http-09 |
| `response` | `fn http.response(status: Int, body: String) -> HttpResponse` |  |  |
| `route` | `fn http.route(pattern: String, handler: HttpHandler) -> HttpRoute` |  |  |
| `router` | `fn http.router(xs: List[HttpRoute]) -> Result[HttpHandler, String]` |  |  |
| `send` | `effect fn http.send(req: HttpRequest) -> HttpResponse` | 新規 | R-http-13 |
| `serve` | `effect fn http.serve(port: Int, handler: HttpHandler) -> Unit` |  |  |
| `set_body` | `fn http.set_body[M: HttpMessage](m: M, body: String) -> M` | 新規。`HttpRequest` も受け取る | R-http-12 |
| `set_body_bytes` | `fn http.set_body_bytes[M: HttpMessage](m: M, body: Bytes) -> M` | 新規。`HttpRequest` も受け取る | R-http-12 |
| `set_header` | `fn http.set_header[M: HttpMessage](m: M, name: String, value: String) -> M` | 意味変更: `HttpRequest` も受け取る | R-http-12 |
| `set_status` | `fn http.set_status(resp: HttpResponse, code: Int) -> HttpResponse` | 改名（旧 `status`）。`set_status` | R-http-02 |
| `start` | `effect fn http.start(req: HttpRequest, limits: HttpLimits) -> HttpCall` | 意味変更: `start(req: HttpRequest, limits: HttpLimits)` にする | R-http-09、R-http-14 |
| `status` | `fn http.status(resp: HttpResponse) -> Int` | 改名（旧 `status_code`）。epoch 17。`0` の番兵もなくす | R-http-04、R-http-08 |
| `wait` | `effect fn http.wait(c: HttpCall) -> HttpResponse` |  | R-http-01 |
| `with_headers` | `fn http.with_headers(status: Int, body: String, headers: List[(String, String)]) -> HttpResponse` | 意味変更: `headers` を `List[(String, String)]` に | R-http-09 |
| `wrap` | `fn http.wrap(handler: HttpHandler, middleware: List[HttpMiddleware]) -> HttpHandler` |  |  |

撤去（18）:

| 関数 | 代わり | 案 |
|---|---|---|
| `delete` | `request("DELETE", …)` | R-http-01 |
| `delete_response` | `send(new_request("DELETE", …))` | R-http-01 |
| `get_bytes` | `body_bytes(get_response(u)!)` | R-http-01 |
| `get_status` | `get_response` | R-http-01 |
| `patch` | `request("PATCH", …)` | R-http-01 |
| `patch_response` | `send(new_request("PATCH", …))` | R-http-01 |
| `post` | `request("POST", …)` | R-http-01 |
| `post_response` | `send(new_request("POST", …))` | R-http-01 |
| `put` | `request("PUT", …)` | R-http-01 |
| `put_response` | `send(new_request("PUT", …))` | R-http-01 |
| `req_body` | `http.body` | R-http-12 |
| `req_header` | `http.header` | R-http-12 |
| `request_bytes` | `body_bytes(send(new_request(…))!)` | R-http-01 |
| `request_response` | `send(new_request(method, url, body, headers))` | R-http-14 |
| `request_status` | `send` | R-http-01 |
| `request_stream` | ハンドル | R-http-01 |
| `request_stream_with_limits` | ハンドル | R-http-01 |
| `url_decode` | `url.decode_form(s) ?? s` | R-http-04 |

## int

記録: [L2-int.md](L2-int.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `abs` | `fn int.abs(x: Int) -> Int` |  | R-math-01（`math.abs` の残る側） |
| `band` | `fn int.band(a: Int, b: Int) -> Int` |  | R-int-05 ×（改名しない） |
| `bit_length` | `fn int.bit_length(x: Int) -> Int` | 改名（旧 `bit_width`） | R-int-11 |
| `bit_reverse` | `fn int.bit_reverse(x: Int) -> Int` |  |  |
| `bnot` | `fn int.bnot(x: Int) -> Int` |  | R-int-05 × |
| `bor` | `fn int.bor(a: Int, b: Int) -> Int` |  | R-int-05 × |
| `bshl` | `fn int.bshl(a: Int, n: Int) -> Int` |  | R-int-05 × |
| `bshr` | `fn int.bshr(a: Int, n: Int) -> Int` |  | R-int-05 × |
| `bshr_logical` | `fn int.bshr_logical(a: Int, n: Int) -> Int` | 新規 | R-int-06 |
| `bxor` | `fn int.bxor(a: Int, b: Int) -> Int` |  | R-int-05 × |
| `byte_swap` | `fn int.byte_swap(x: Int) -> Int` |  |  |
| `clamp` | `fn int.clamp(x: Int, lo: Int, hi: Int) -> Int` |  |  |
| `compare` | `fn int.compare(a: Int, b: Int) -> Int` | 新規 | R-sized-02 |
| `count_leading_zeros` | `fn int.count_leading_zeros(x: Int) -> Int` |  |  |
| `count_ones` | `fn int.count_ones(x: Int) -> Int` | 改名（旧 `pop_count`） | R-int-07 |
| `count_trailing_zeros` | `fn int.count_trailing_zeros(x: Int) -> Int` |  |  |
| `floor_div` | `fn int.floor_div(a: Int, b: Int) -> Int` | 新規 | R-int-03 |
| `from` | `fn int.from[S: Numeric](x: S) -> Int` | 新規 | R-L0-40 |
| `from_checked` | `fn int.from_checked[S: Numeric](x: S) -> Int?` | 新規 | R-L0-40 |
| `from_saturating` | `fn int.from_saturating[S: Numeric](x: S) -> Int` | 新規 | R-L0-40 |
| `from_wrapping` | `fn int.from_wrapping[S: Integer](x: S) -> Int` | 新規 | R-L0-40 |
| `is_even` | `fn int.is_even(x: Int) -> Bool` |  |  |
| `is_odd` | `fn int.is_odd(x: Int) -> Bool` |  |  |
| `log2_ceil` | `fn int.log2_ceil(x: Int) -> Int` |  |  |
| `log2_floor` | `fn int.log2_floor(x: Int) -> Int` |  |  |
| `max` | `fn int.max(a: Int, b: Int) -> Int` |  | R-math-01（`math.max` の残る側） |
| `max_value` | `fn int.max_value() -> Int` |  |  |
| `min` | `fn int.min(a: Int, b: Int) -> Int` |  | R-math-01（`math.min` の残る側） |
| `min_value` | `fn int.min_value() -> Int` |  |  |
| `modulo` | `fn int.modulo(a: Int, b: Int) -> Int` | 新規 | R-int-03 |
| `next_power_of_two` | `fn int.next_power_of_two(x: Int) -> Int` |  |  |
| `parse` | `fn int.parse(s: String) -> Result[Int, String]` |  |  |
| `parse_radix` | `fn int.parse_radix(s: String, radix: Int) -> Result[Int, String]` | 新規 | R-int-04 |
| `pow` | `fn int.pow(base: Int, exp: Int) -> Int` | 新規 | R-math-01 |
| `prev_power_of_two` | `fn int.prev_power_of_two(x: Int) -> Int` |  |  |
| `rotate_left` | `fn int.rotate_left(a: Int, n: Int) -> Int` | 意味変更: `bits` を外し 64 bit で回す | R-int-12 |
| `rotate_right` | `fn int.rotate_right(a: Int, n: Int) -> Int` | 意味変更: `bits` を外し 64 bit で回す | R-int-12 |
| `sign` | `fn int.sign(x: Int) -> Int` | 新規 | R-math-01 |
| `to_hex` | `fn int.to_hex(x: Int) -> String` |  | R-int-04（64 bit の並びの 16 進。符号付きの基数変換は `to_string_radix`） |
| `to_string` | `fn int.to_string(x: Int) -> String` |  |  |
| `to_string_radix` | `fn int.to_string_radix(x: Int, radix: Int) -> String` | 新規 | R-int-04 |
| `wrap` | `fn int.wrap(x: Int, bits: Int) -> Int` | 新規 | R-int-02 |
| `wrap_add` | `fn int.wrap_add(a: Int, b: Int) -> Int` | 意味変更: `bits` を外し 64 bit で折り返す。N bit は `int.wrap` と組む | R-int-12 |
| `wrap_mul` | `fn int.wrap_mul(a: Int, b: Int) -> Int` | 意味変更: `bits` を外し 64 bit で折り返す。N bit は `int.wrap` と組む | R-int-12 |
| `wrap_sub` | `fn int.wrap_sub(a: Int, b: Int) -> Int` | 新規 | R-int-02、R-int-12 |

撤去（41）:

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
| `to_float` | `float.from` | R-L0-40 |
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float32_checked` | `float32.from_checked` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int16_checked` | `int16.from_checked` | R-L0-40 |
| `to_int16_saturating` | `int16.from_saturating` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int32_checked` | `int32.from_checked` | R-L0-40 |
| `to_int32_saturating` | `int32.from_saturating` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_int8_checked` | `int8.from_checked` | R-L0-40 |
| `to_int8_saturating` | `int8.from_saturating` | R-L0-40 |
| `to_u32` | `int.wrap(x, 32)` | R-int-02（`int.wrap(x, 32)`。#3851 E） |
| `to_u8` | `int.wrap(x, 8)` | R-int-02（`int.wrap(x, 8)`。#3851 E） |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint16_checked` | `uint16.from_checked` | R-L0-40 |
| `to_uint16_saturating` | `uint16.from_saturating` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint32_checked` | `uint32.from_checked` | R-L0-40 |
| `to_uint32_saturating` | `uint32.from_saturating` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint64_checked` | `uint64.from_checked` | R-L0-40 |
| `to_uint64_saturating` | `uint64.from_saturating` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |
| `to_uint8_checked` | `uint8.from_checked` | R-L0-40 |
| `to_uint8_saturating` | `uint8.from_saturating` | R-L0-40 |

## int16

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `abs` | `fn int16.abs(x: Int16) -> Int16` | 新規 | R-sized-10 |
| `band` | `fn int16.band(a: Int16, b: Int16) -> Int16` | 新規 | R-sized-10 |
| `bit_length` | `fn int16.bit_length(x: Int16) -> Int` | 新規 | R-int-11 |
| `bnot` | `fn int16.bnot(x: Int16) -> Int16` | 新規 | R-sized-10 |
| `bor` | `fn int16.bor(a: Int16, b: Int16) -> Int16` | 新規 | R-sized-10 |
| `bshl` | `fn int16.bshl(a: Int16, n: Int) -> Int16` | 新規 | R-sized-10 |
| `bshr` | `fn int16.bshr(a: Int16, n: Int) -> Int16` | 新規 | R-sized-10 |
| `bxor` | `fn int16.bxor(a: Int16, b: Int16) -> Int16` | 新規 | R-sized-10 |
| `byte_swap` | `fn int16.byte_swap(x: Int16) -> Int16` | 新規 | R-sized-10 |
| `clamp` | `fn int16.clamp(x: Int16, lo: Int16, hi: Int16) -> Int16` | 新規 | R-sized-08 |
| `compare` | `fn int16.compare(a: Int16, b: Int16) -> Int` | 新規 | R-sized-02 |
| `count_leading_zeros` | `fn int16.count_leading_zeros(x: Int16) -> Int` | 新規 | R-sized-10 |
| `count_ones` | `fn int16.count_ones(x: Int16) -> Int` | 新規 | R-sized-10 |
| `count_trailing_zeros` | `fn int16.count_trailing_zeros(x: Int16) -> Int` | 新規 | R-sized-10 |
| `floor_div` | `fn int16.floor_div(a: Int16, b: Int16) -> Int16` | 新規 | R-sized-10 |
| `from` | `fn int16.from[S: Numeric](x: S) -> Int16` | 新規 | R-L0-40 |
| `from_checked` | `fn int16.from_checked[S: Numeric](x: S) -> Int16?` | 新規 | R-L0-40 |
| `from_saturating` | `fn int16.from_saturating[S: Numeric](x: S) -> Int16` | 新規 | R-L0-40 |
| `from_wrapping` | `fn int16.from_wrapping[S: Integer](x: S) -> Int16` | 新規 | R-L0-40 |
| `is_even` | `fn int16.is_even(x: Int16) -> Bool` | 新規 | R-sized-10 |
| `is_odd` | `fn int16.is_odd(x: Int16) -> Bool` | 新規 | R-sized-10 |
| `max` | `fn int16.max(a: Int16, b: Int16) -> Int16` | 新規 | R-sized-08 |
| `max_value` | `fn int16.max_value() -> Int16` |  |  |
| `min` | `fn int16.min(a: Int16, b: Int16) -> Int16` | 新規 | R-sized-08 |
| `min_value` | `fn int16.min_value() -> Int16` |  |  |
| `modulo` | `fn int16.modulo(a: Int16, b: Int16) -> Int16` | 新規 | R-sized-10 |
| `parse` | `fn int16.parse(s: String) -> Result[Int16, String]` | 新規 | R-sized-02 |
| `parse_radix` | `fn int16.parse_radix(s: String, radix: Int) -> Result[Int16, String]` | 新規 | R-sized-06 |
| `pow` | `fn int16.pow(base: Int16, exp: Int) -> Int16` | 新規 | R-sized-10 |
| `rotate_left` | `fn int16.rotate_left(a: Int16, n: Int) -> Int16` | 新規 | R-int-12 |
| `rotate_right` | `fn int16.rotate_right(a: Int16, n: Int) -> Int16` | 新規 | R-int-12 |
| `sign` | `fn int16.sign(x: Int16) -> Int` | 新規 | R-sized-10 |
| `to_string` | `fn int16.to_string(x: Int16) -> String` |  |  |
| `to_string_radix` | `fn int16.to_string_radix(x: Int16, radix: Int) -> String` | 新規 | R-sized-06 |
| `wrap_add` | `fn int16.wrap_add(a: Int16, b: Int16) -> Int16` | 新規 | R-int-12 |
| `wrap_mul` | `fn int16.wrap_mul(a: Int16, b: Int16) -> Int16` | 新規 | R-int-12 |
| `wrap_sub` | `fn int16.wrap_sub(a: Int16, b: Int16) -> Int16` | 新規 | R-int-12 |

撤去（19）:

| 関数 | 代わり | 案 |
|---|---|---|
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_int8_checked` | `int8.from_checked` | R-L0-40 |
| `to_int8_saturating` | `int8.from_saturating` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint16_checked` | `uint16.from_checked` | R-L0-40 |
| `to_uint16_saturating` | `uint16.from_saturating` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint32_checked` | `uint32.from_checked` | R-L0-40 |
| `to_uint32_saturating` | `uint32.from_saturating` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint64_checked` | `uint64.from_checked` | R-L0-40 |
| `to_uint64_saturating` | `uint64.from_saturating` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |
| `to_uint8_checked` | `uint8.from_checked` | R-L0-40 |
| `to_uint8_saturating` | `uint8.from_saturating` | R-L0-40 |

## int32

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `abs` | `fn int32.abs(x: Int32) -> Int32` | 新規 | R-sized-10 |
| `band` | `fn int32.band(a: Int32, b: Int32) -> Int32` | 新規 | R-sized-10 |
| `bit_length` | `fn int32.bit_length(x: Int32) -> Int` | 新規 | R-int-11 |
| `bnot` | `fn int32.bnot(x: Int32) -> Int32` | 新規 | R-sized-10 |
| `bor` | `fn int32.bor(a: Int32, b: Int32) -> Int32` | 新規 | R-sized-10 |
| `bshl` | `fn int32.bshl(a: Int32, n: Int) -> Int32` | 新規 | R-sized-10 |
| `bshr` | `fn int32.bshr(a: Int32, n: Int) -> Int32` | 新規 | R-sized-10 |
| `bxor` | `fn int32.bxor(a: Int32, b: Int32) -> Int32` | 新規 | R-sized-10 |
| `byte_swap` | `fn int32.byte_swap(x: Int32) -> Int32` | 新規 | R-sized-10 |
| `clamp` | `fn int32.clamp(x: Int32, lo: Int32, hi: Int32) -> Int32` | 新規 | R-sized-08 |
| `compare` | `fn int32.compare(a: Int32, b: Int32) -> Int` | 新規 | R-sized-02 |
| `count_leading_zeros` | `fn int32.count_leading_zeros(x: Int32) -> Int` | 新規 | R-sized-10 |
| `count_ones` | `fn int32.count_ones(x: Int32) -> Int` | 新規 | R-sized-10 |
| `count_trailing_zeros` | `fn int32.count_trailing_zeros(x: Int32) -> Int` | 新規 | R-sized-10 |
| `floor_div` | `fn int32.floor_div(a: Int32, b: Int32) -> Int32` | 新規 | R-sized-10 |
| `from` | `fn int32.from[S: Numeric](x: S) -> Int32` | 新規 | R-L0-40 |
| `from_checked` | `fn int32.from_checked[S: Numeric](x: S) -> Int32?` | 新規 | R-L0-40 |
| `from_saturating` | `fn int32.from_saturating[S: Numeric](x: S) -> Int32` | 新規 | R-L0-40 |
| `from_wrapping` | `fn int32.from_wrapping[S: Integer](x: S) -> Int32` | 新規 | R-L0-40 |
| `is_even` | `fn int32.is_even(x: Int32) -> Bool` | 新規 | R-sized-10 |
| `is_odd` | `fn int32.is_odd(x: Int32) -> Bool` | 新規 | R-sized-10 |
| `max` | `fn int32.max(a: Int32, b: Int32) -> Int32` | 新規 | R-sized-08 |
| `max_value` | `fn int32.max_value() -> Int32` |  |  |
| `min` | `fn int32.min(a: Int32, b: Int32) -> Int32` | 新規 | R-sized-08 |
| `min_value` | `fn int32.min_value() -> Int32` |  |  |
| `modulo` | `fn int32.modulo(a: Int32, b: Int32) -> Int32` | 新規 | R-sized-10 |
| `parse` | `fn int32.parse(s: String) -> Result[Int32, String]` | 新規 | R-sized-02 |
| `parse_radix` | `fn int32.parse_radix(s: String, radix: Int) -> Result[Int32, String]` | 新規 | R-sized-06 |
| `pow` | `fn int32.pow(base: Int32, exp: Int) -> Int32` | 新規 | R-sized-10 |
| `rotate_left` | `fn int32.rotate_left(a: Int32, n: Int) -> Int32` | 新規 | R-int-12 |
| `rotate_right` | `fn int32.rotate_right(a: Int32, n: Int) -> Int32` | 新規 | R-int-12 |
| `sign` | `fn int32.sign(x: Int32) -> Int` | 新規 | R-sized-10 |
| `to_string` | `fn int32.to_string(x: Int32) -> String` |  |  |
| `to_string_radix` | `fn int32.to_string_radix(x: Int32, radix: Int) -> String` | 新規 | R-sized-06 |
| `wrap_add` | `fn int32.wrap_add(a: Int32, b: Int32) -> Int32` | 新規 | R-int-12 |
| `wrap_mul` | `fn int32.wrap_mul(a: Int32, b: Int32) -> Int32` | 新規 | R-int-12 |
| `wrap_sub` | `fn int32.wrap_sub(a: Int32, b: Int32) -> Int32` | 新規 | R-int-12 |

撤去（21）:

| 関数 | 代わり | 案 |
|---|---|---|
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int16_checked` | `int16.from_checked` | R-L0-40 |
| `to_int16_saturating` | `int16.from_saturating` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_int8_checked` | `int8.from_checked` | R-L0-40 |
| `to_int8_saturating` | `int8.from_saturating` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint16_checked` | `uint16.from_checked` | R-L0-40 |
| `to_uint16_saturating` | `uint16.from_saturating` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint32_checked` | `uint32.from_checked` | R-L0-40 |
| `to_uint32_saturating` | `uint32.from_saturating` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint64_checked` | `uint64.from_checked` | R-L0-40 |
| `to_uint64_saturating` | `uint64.from_saturating` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |
| `to_uint8_checked` | `uint8.from_checked` | R-L0-40 |
| `to_uint8_saturating` | `uint8.from_saturating` | R-L0-40 |

## int64

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `abs` | `fn int64.abs(x: Int64) -> Int64` | 新規 | R-sized-10 |
| `band` | `fn int64.band(a: Int64, b: Int64) -> Int64` | 新規 | R-sized-10 |
| `bit_length` | `fn int64.bit_length(x: Int64) -> Int` | 新規 | R-int-11 |
| `bnot` | `fn int64.bnot(x: Int64) -> Int64` | 新規 | R-sized-10 |
| `bor` | `fn int64.bor(a: Int64, b: Int64) -> Int64` | 新規 | R-sized-10 |
| `bshl` | `fn int64.bshl(a: Int64, n: Int) -> Int64` | 新規 | R-sized-10 |
| `bshr` | `fn int64.bshr(a: Int64, n: Int) -> Int64` | 新規 | R-sized-10 |
| `bxor` | `fn int64.bxor(a: Int64, b: Int64) -> Int64` | 新規 | R-sized-10 |
| `byte_swap` | `fn int64.byte_swap(x: Int64) -> Int64` | 新規 | R-sized-10 |
| `clamp` | `fn int64.clamp(x: Int64, lo: Int64, hi: Int64) -> Int64` | 新規 | R-sized-08 |
| `compare` | `fn int64.compare(a: Int64, b: Int64) -> Int` | 新規 | R-sized-02 |
| `count_leading_zeros` | `fn int64.count_leading_zeros(x: Int64) -> Int` | 新規 | R-sized-10 |
| `count_ones` | `fn int64.count_ones(x: Int64) -> Int` | 新規 | R-sized-10 |
| `count_trailing_zeros` | `fn int64.count_trailing_zeros(x: Int64) -> Int` | 新規 | R-sized-10 |
| `floor_div` | `fn int64.floor_div(a: Int64, b: Int64) -> Int64` | 新規 | R-sized-10 |
| `from` | `fn int64.from[S: Numeric](x: S) -> Int64` | 新規 | R-L0-40 |
| `from_checked` | `fn int64.from_checked[S: Numeric](x: S) -> Int64?` | 新規 | R-L0-40 |
| `from_saturating` | `fn int64.from_saturating[S: Numeric](x: S) -> Int64` | 新規 | R-L0-40 |
| `from_wrapping` | `fn int64.from_wrapping[S: Integer](x: S) -> Int64` | 新規 | R-L0-40 |
| `is_even` | `fn int64.is_even(x: Int64) -> Bool` | 新規 | R-sized-10 |
| `is_odd` | `fn int64.is_odd(x: Int64) -> Bool` | 新規 | R-sized-10 |
| `max` | `fn int64.max(a: Int64, b: Int64) -> Int64` | 新規 | R-sized-08 |
| `max_value` | `fn int64.max_value() -> Int64` |  |  |
| `min` | `fn int64.min(a: Int64, b: Int64) -> Int64` | 新規 | R-sized-08 |
| `min_value` | `fn int64.min_value() -> Int64` |  |  |
| `modulo` | `fn int64.modulo(a: Int64, b: Int64) -> Int64` | 新規 | R-sized-10 |
| `parse` | `fn int64.parse(s: String) -> Result[Int64, String]` | 新規 | R-sized-02 |
| `parse_radix` | `fn int64.parse_radix(s: String, radix: Int) -> Result[Int64, String]` | 新規 | R-sized-06 |
| `pow` | `fn int64.pow(base: Int64, exp: Int) -> Int64` | 新規 | R-sized-10 |
| `rotate_left` | `fn int64.rotate_left(a: Int64, n: Int) -> Int64` | 新規 | R-int-12 |
| `rotate_right` | `fn int64.rotate_right(a: Int64, n: Int) -> Int64` | 新規 | R-int-12 |
| `sign` | `fn int64.sign(x: Int64) -> Int` | 新規 | R-sized-10 |
| `to_string` | `fn int64.to_string(x: Int64) -> String` |  |  |
| `to_string_radix` | `fn int64.to_string_radix(x: Int64, radix: Int) -> String` | 新規 | R-sized-06 |
| `wrap_add` | `fn int64.wrap_add(a: Int64, b: Int64) -> Int64` | 新規 | R-int-12 |
| `wrap_mul` | `fn int64.wrap_mul(a: Int64, b: Int64) -> Int64` | 新規 | R-int-12 |
| `wrap_sub` | `fn int64.wrap_sub(a: Int64, b: Int64) -> Int64` | 新規 | R-int-12 |

撤去（23）:

| 関数 | 代わり | 案 |
|---|---|---|
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int16_checked` | `int16.from_checked` | R-L0-40 |
| `to_int16_saturating` | `int16.from_saturating` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int32_checked` | `int32.from_checked` | R-L0-40 |
| `to_int32_saturating` | `int32.from_saturating` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_int8_checked` | `int8.from_checked` | R-L0-40 |
| `to_int8_saturating` | `int8.from_saturating` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint16_checked` | `uint16.from_checked` | R-L0-40 |
| `to_uint16_saturating` | `uint16.from_saturating` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint32_checked` | `uint32.from_checked` | R-L0-40 |
| `to_uint32_saturating` | `uint32.from_saturating` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint64_checked` | `uint64.from_checked` | R-L0-40 |
| `to_uint64_saturating` | `uint64.from_saturating` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |
| `to_uint8_checked` | `uint8.from_checked` | R-L0-40 |
| `to_uint8_saturating` | `uint8.from_saturating` | R-L0-40 |

## int8

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `abs` | `fn int8.abs(x: Int8) -> Int8` | 新規 | R-sized-10 |
| `band` | `fn int8.band(a: Int8, b: Int8) -> Int8` | 新規 | R-sized-10 |
| `bit_length` | `fn int8.bit_length(x: Int8) -> Int` | 新規 | R-int-11 |
| `bnot` | `fn int8.bnot(x: Int8) -> Int8` | 新規 | R-sized-10 |
| `bor` | `fn int8.bor(a: Int8, b: Int8) -> Int8` | 新規 | R-sized-10 |
| `bshl` | `fn int8.bshl(a: Int8, n: Int) -> Int8` | 新規 | R-sized-10 |
| `bshr` | `fn int8.bshr(a: Int8, n: Int) -> Int8` | 新規 | R-sized-10 |
| `bxor` | `fn int8.bxor(a: Int8, b: Int8) -> Int8` | 新規 | R-sized-10 |
| `byte_swap` | `fn int8.byte_swap(x: Int8) -> Int8` | 新規 | R-sized-10 |
| `clamp` | `fn int8.clamp(x: Int8, lo: Int8, hi: Int8) -> Int8` | 新規 | R-sized-08 |
| `compare` | `fn int8.compare(a: Int8, b: Int8) -> Int` | 新規 | R-sized-02 |
| `count_leading_zeros` | `fn int8.count_leading_zeros(x: Int8) -> Int` | 新規 | R-sized-10 |
| `count_ones` | `fn int8.count_ones(x: Int8) -> Int` | 新規 | R-sized-10 |
| `count_trailing_zeros` | `fn int8.count_trailing_zeros(x: Int8) -> Int` | 新規 | R-sized-10 |
| `floor_div` | `fn int8.floor_div(a: Int8, b: Int8) -> Int8` | 新規 | R-sized-10 |
| `from` | `fn int8.from[S: Numeric](x: S) -> Int8` | 新規 | R-L0-40 |
| `from_checked` | `fn int8.from_checked[S: Numeric](x: S) -> Int8?` | 新規 | R-L0-40 |
| `from_saturating` | `fn int8.from_saturating[S: Numeric](x: S) -> Int8` | 新規 | R-L0-40 |
| `from_wrapping` | `fn int8.from_wrapping[S: Integer](x: S) -> Int8` | 新規 | R-L0-40 |
| `is_even` | `fn int8.is_even(x: Int8) -> Bool` | 新規 | R-sized-10 |
| `is_odd` | `fn int8.is_odd(x: Int8) -> Bool` | 新規 | R-sized-10 |
| `max` | `fn int8.max(a: Int8, b: Int8) -> Int8` | 新規 | R-sized-08 |
| `max_value` | `fn int8.max_value() -> Int8` |  |  |
| `min` | `fn int8.min(a: Int8, b: Int8) -> Int8` | 新規 | R-sized-08 |
| `min_value` | `fn int8.min_value() -> Int8` |  |  |
| `modulo` | `fn int8.modulo(a: Int8, b: Int8) -> Int8` | 新規 | R-sized-10 |
| `parse` | `fn int8.parse(s: String) -> Result[Int8, String]` | 新規 | R-sized-02 |
| `parse_radix` | `fn int8.parse_radix(s: String, radix: Int) -> Result[Int8, String]` | 新規 | R-sized-06 |
| `pow` | `fn int8.pow(base: Int8, exp: Int) -> Int8` | 新規 | R-sized-10 |
| `rotate_left` | `fn int8.rotate_left(a: Int8, n: Int) -> Int8` | 新規 | R-int-12 |
| `rotate_right` | `fn int8.rotate_right(a: Int8, n: Int) -> Int8` | 新規 | R-int-12 |
| `sign` | `fn int8.sign(x: Int8) -> Int` | 新規 | R-sized-10 |
| `to_string` | `fn int8.to_string(x: Int8) -> String` |  |  |
| `to_string_radix` | `fn int8.to_string_radix(x: Int8, radix: Int) -> String` | 新規 | R-sized-06 |
| `wrap_add` | `fn int8.wrap_add(a: Int8, b: Int8) -> Int8` | 新規 | R-int-12 |
| `wrap_mul` | `fn int8.wrap_mul(a: Int8, b: Int8) -> Int8` | 新規 | R-int-12 |
| `wrap_sub` | `fn int8.wrap_sub(a: Int8, b: Int8) -> Int8` | 新規 | R-int-12 |

撤去（17）:

| 関数 | 代わり | 案 |
|---|---|---|
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint16_checked` | `uint16.from_checked` | R-L0-40 |
| `to_uint16_saturating` | `uint16.from_saturating` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint32_checked` | `uint32.from_checked` | R-L0-40 |
| `to_uint32_saturating` | `uint32.from_saturating` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint64_checked` | `uint64.from_checked` | R-L0-40 |
| `to_uint64_saturating` | `uint64.from_saturating` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |
| `to_uint8_checked` | `uint8.from_checked` | R-L0-40 |
| `to_uint8_saturating` | `uint8.from_saturating` | R-L0-40 |

## io

記録: [L2-io.md](L2-io.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `close` | `effect fn io.close[H: Close](h: H) -> Unit` | 新規 | R-L0-44 |
| `eprint` | `effect fn io.eprint(s: String) -> Unit` | 新規 | R-io-03 |
| `print` | `effect fn io.print(s: String) -> Unit` |  |  |
| `read` | `effect fn io.read[H: Read](h: H, max: Int) -> Bytes` | 新規 | R-L0-44 |
| `read_all` | `effect fn io.read_all() -> String` |  |  |
| `read_exact` | `effect fn io.read_exact[H: Read](h: H, n: Int) -> Bytes` | 新規 | R-L0-44 |
| `read_line` | `effect fn io.read_line() -> String?` | 意味変更: `-> String?` にする。唯一の 1 行読み | R-io-01 |
| `read_to_end` | `effect fn io.read_to_end[H: Read](h: H) -> Bytes` | 新規 | R-L0-44 |
| `stderr` | `effect fn io.stderr() -> Stderr` | 新規 | R-io-05 |
| `stdin` | `effect fn io.stdin() -> Stdin` | 新規 | R-io-05 |
| `stdout` | `effect fn io.stdout() -> Stdout` | 新規 | R-io-05 |
| `write` | `effect fn io.write[H: Write](h: H, data: Bytes) -> Unit` | 意味変更: `Write` の総称関数 `write(h, data)` にする。古い 1 引数の呼び出しは型エラー | R-L0-44・R-io-05 |

撤去（4）:

| 関数 | 代わり | 案 |
|---|---|---|
| `read_byte` | `io.read(io.stdin(), 1)` | R-io-01・R-io-05 |
| `read_line_opt` | `read_line` | R-io-01 |
| `read_n_bytes` | `io.read(io.stdin(), n)` | R-io-01・R-io-05 |
| `write_bytes` | `io.write(io.stdout(), data)` | R-io-01・R-io-02・R-io-05 |

## json

記録: [L2-json.md](L2-json.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `parse` | `fn json.parse(text: String) -> Result[Value, String]` | 意味変更: 後ろの余計な文字は err。入れ子が 1000 段を超えたら err | R-json-02、R-json-04 |
| `stringify` | `fn json.stringify(v: Value) -> String` |  | R-json-03 |
| `stringify_pretty` | `fn json.stringify_pretty(v: Value) -> String` |  | R-json-03 |

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
| `binary_search_by` | `fn list.binary_search_by(xs: List[A], target: K, f: (A) -> K) -> Int?` | 新規 | R-list-17 |
| `binary_search_with` | `fn list.binary_search_with(xs: List[A], target: A, cmp: (A, A) -> Int) -> Int?` | 新規 | R-list-17 |
| `chunk` | `fn list.chunk(xs: List[A], n: Int) -> List[List[A]]` | 意味変更: `n <= 0` は abort | R-list-10 |
| `clear` | `fn list.clear(mut xs: List[A]) -> Unit` |  | R-list-00 |
| `contains` | `fn list.contains(xs: List[A], x: A) -> Bool` |  |  |
| `count` | `fn list.count(xs: List[A], f: (A) -> Bool) -> Int` |  |  |
| `dedup` | `fn list.dedup(xs: List[A]) -> List[A]` |  |  |
| `drop` | `fn list.drop(xs: List[A], n: Int) -> List[A]` | 意味変更: 負の数は abort。R-L0-29 | R-L0-12、R-L0-29 |
| `drop_end` | `fn list.drop_end(xs: List[A], n: Int) -> List[A]` | 意味変更: 負の数は abort。R-L0-29 | R-L0-12、R-L0-29 |
| `drop_while` | `fn list.drop_while(xs: List[A], f: (A) -> Bool) -> List[A]` |  |  |
| `enumerate` | `fn list.enumerate(xs: List[A]) -> List[(Int, A)]` |  |  |
| `filter` | `fn list.filter(xs: List[A], f: (A) -> Bool) -> List[A]` |  |  |
| `filter_map` | `fn list.filter_map(xs: List[A], f: (A) -> B?) -> List[B]` |  |  |
| `find` | `fn list.find(xs: List[A], f: (A) -> Bool) -> A?` |  |  |
| `find_index` | `fn list.find_index(xs: List[A], f: (A) -> Bool) -> Int?` |  |  |
| `find_map` | `fn list.find_map(xs: List[A], f: (A) -> B?) -> B?` |  |  |
| `first` | `fn list.first(xs: List[A]) -> A?` |  |  |
| `flat_map` | `fn list.flat_map(xs: List[A], f: (A) -> List[B]) -> List[B]` |  |  |
| `flatten` | `fn list.flatten(xs: List[List[A]]) -> List[A]` |  |  |
| `fold` | `fn list.fold(xs: List[A], init: B, f: (B, A) -> B) -> B` |  |  |
| `from_list` | `fn list.from_list(xs: List[A]) -> List[A]` | 新規 | R-list-20 |
| `get` | `fn list.get(xs: List[A], i: Int) -> A?` |  |  |
| `group_by` | `fn list.group_by(xs: List[A], f: (A) -> B) -> Map[B, List[A]]` |  |  |
| `index_of` | `fn list.index_of(xs: List[A], x: A) -> Int?` |  |  |
| `insert` | `fn list.insert(xs: List[A], i: Int, x: A) -> List[A]` | 意味変更: `0..=len` の外は abort | R-list-00、R-list-13 |
| `intersperse` | `fn list.intersperse(xs: List[A], sep: A) -> List[A]` |  |  |
| `is_empty` | `fn list.is_empty(xs: List[A]) -> Bool` |  |  |
| `iterate` | `fn list.iterate(seed: A, f: (A) -> A, n: Int) -> List[A]` |  |  |
| `join` | `fn list.join[T: Text](xs: List[T], sep: T) -> T` | 意味変更: `String`・`Bytes` の 2 つの形を、`Text` の総称関数 1 つに | R-L0-42 |
| `last` | `fn list.last(xs: List[A]) -> A?` |  |  |
| `len` | `fn list.len(xs: List[A]) -> Int` |  | R-L0-11 |
| `map` | `fn list.map(xs: List[A], f: (A) -> B) -> List[B]` |  |  |
| `max` | `fn list.max(xs: List[A]) -> A?` |  |  |
| `max_by` | `fn list.max_by(xs: List[A], f: (A) -> K) -> A?` | 新規 | R-list-06 |
| `max_with` | `fn list.max_with(xs: List[A], cmp: (A, A) -> Int) -> A?` | 新規 | R-list-19 |
| `min` | `fn list.min(xs: List[A]) -> A?` |  |  |
| `min_by` | `fn list.min_by(xs: List[A], f: (A) -> K) -> A?` | 新規 | R-list-06 |
| `min_with` | `fn list.min_with(xs: List[A], cmp: (A, A) -> Int) -> A?` | 新規 | R-list-19 |
| `new` | `fn list.new() -> List[A]` | 新規 | R-list-20 |
| `partition` | `fn list.partition(xs: List[A], f: (A) -> Bool) -> (List[A], List[A])` |  |  |
| `pop` | `fn list.pop(mut xs: List[A]) -> A?` |  | R-list-00 |
| `product` | `fn list.product[A: Numeric](xs: List[A]) -> A` | 意味変更: `Int`・`Float` の 2 つの形を、数の型すべてを受ける 1 つの総称関数に | R-L0-42 |
| `push` | `fn list.push(mut xs: List[A], x: A) -> Unit` |  | R-list-00 |
| `range` | `fn list.range(start: Int, end: Int) -> List[Int]` |  |  |
| `range_step` | `fn list.range_step(start: Int, end: Int, step: Int) -> List[Int]` | 新規 | R-list-09 |
| `reduce` | `fn list.reduce(xs: List[A], f: (A, A) -> A) -> A?` |  |  |
| `remove_at` | `fn list.remove_at(xs: List[A], i: Int) -> List[A]` | 意味変更: 範囲外は abort | R-list-00、R-list-13 |
| `repeat` | `fn list.repeat(x: A, n: Int) -> List[A]` |  |  |
| `reverse` | `fn list.reverse(xs: List[A]) -> List[A]` |  | R-list-00 |
| `scan` | `fn list.scan(xs: List[A], init: B, f: (B, A) -> B) -> List[B]` |  |  |
| `set` | `fn list.set(xs: List[A], i: Int, x: A) -> List[A]` | 意味変更: 範囲外は abort | R-list-00、R-list-13 |
| `slice` | `fn list.slice(xs: List[A], start: Int, end: Int) -> List[A]` | 意味変更: 負の位置は abort。R-L0-29 | R-list-10、R-L0-29 |
| `sort` | `fn list.sort(xs: List[A]) -> List[A]` |  | R-list-00 |
| `sort_by` | `fn list.sort_by(xs: List[A], f: (A) -> B) -> List[A]` |  | R-list-06 |
| `sort_with` | `fn list.sort_with(xs: List[A], cmp: (A, A) -> Int) -> List[A]` | 新規 | R-list-06 |
| `split_at` | `fn list.split_at(xs: List[A], n: Int) -> (List[A], List[A])` |  |  |
| `sum` | `fn list.sum[A: Numeric](xs: List[A]) -> A` | 意味変更: `Int`・`Float` の 2 つの形を、数の型すべてを受ける 1 つの総称関数に | R-L0-42 |
| `swap` | `fn list.swap(xs: List[A], i: Int, j: Int) -> List[A]` | 意味変更: 範囲外は abort | R-list-00、R-list-13 |
| `take` | `fn list.take(xs: List[A], n: Int) -> List[A]` | 意味変更: 負の数は abort。R-L0-29 | R-L0-12、R-L0-29 |
| `take_end` | `fn list.take_end(xs: List[A], n: Int) -> List[A]` | 意味変更: 負の数は abort。R-L0-29 | R-L0-12、R-L0-29 |
| `take_while` | `fn list.take_while(xs: List[A], f: (A) -> Bool) -> List[A]` |  |  |
| `to_list` | `fn list.to_list(xs: List[A]) -> List[A]` | 新規 | R-list-20 |
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
| `all` | `fn map.all(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool` | 意味変更: callback は組 1 つ。`(k, v) =>` はそのまま書ける | R-map-09 |
| `any` | `fn map.any(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool` | 意味変更: callback は組 1 つ。`(k, v) =>` はそのまま書ける | R-map-09 |
| `clear` | `fn map.clear(mut m: Map[K, V]) -> Unit` |  | — (R-L0-04 の動詞) |
| `contains` | `fn map.contains(m: Map[K, V], key: K) -> Bool` |  |  |
| `count` | `fn map.count(m: Map[K, V], f: ((K, V)) -> Bool) -> Int` | 意味変更: callback は組 1 つ。`(k, v) =>` はそのまま書ける | R-map-09 |
| `enumerate` | `fn map.enumerate(m: Map[K, V]) -> List[(Int, (K, V))]` | 新規 | R-L0-42 |
| `filter` | `fn map.filter(m: Map[K, V], f: ((K, V)) -> Bool) -> Map[K, V]` | 意味変更: callback は組 1 つ。`(k, v) =>` はそのまま書ける | R-map-09 |
| `filter_map` | `fn map.filter_map(m: Map[K, V], f: ((K, V)) -> B?) -> List[B]` | 新規 | R-L0-42 |
| `find` | `fn map.find(m: Map[K, V], f: ((K, V)) -> Bool) -> (K, V)?` | 意味変更: callback は組 1 つ。`(k, v) =>` はそのまま書ける | R-map-09 |
| `find_map` | `fn map.find_map(m: Map[K, V], f: ((K, V)) -> B?) -> B?` | 新規 | R-L0-42 |
| `fold` | `fn map.fold(m: Map[K, V], init: A, f: (A, (K, V)) -> A) -> A` | 意味変更: callback の 2 つめは組。`almide fix` が書き換える | R-map-09 |
| `from_list` | `fn map.from_list(xs: List[(K, V)]) -> Map[K, V]` |  | — (R-L0-07) |
| `get` | `fn map.get(m: Map[K, V], key: K) -> V?` |  |  |
| `group_by` | `fn map.group_by(m: Map[K, V], f: ((K, V)) -> B) -> Map[B, List[(K, V)]]` | 新規 | R-L0-42 |
| `is_empty` | `fn map.is_empty(m: Map[K, V]) -> Bool` |  |  |
| `keys` | `fn map.keys(m: Map[K, V]) -> List[K]` |  |  |
| `len` | `fn map.len(m: Map[K, V]) -> Int` |  | — (R-L0-11) |
| `map` | `fn map.map(m: Map[K, V], f: (V) -> B) -> Map[K, B]` |  | R-map-04 |
| `max_by` | `fn map.max_by(m: Map[K, V], f: ((K, V)) -> U) -> (K, V)?` | 新規 | R-L0-42 |
| `max_with` | `fn map.max_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> (K, V)?` | 新規 | R-L0-42 |
| `merge` | `fn map.merge(a: Map[K, V], b: Map[K, V]) -> Map[K, V]` |  |  |
| `min_by` | `fn map.min_by(m: Map[K, V], f: ((K, V)) -> U) -> (K, V)?` | 新規 | R-L0-42 |
| `min_with` | `fn map.min_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> (K, V)?` | 新規 | R-L0-42 |
| `new` | `fn map.new() -> Map[K, V]` |  |  |
| `partition` | `fn map.partition(m: Map[K, V], f: ((K, V)) -> Bool) -> (List[(K, V)], List[(K, V)])` | 新規 | R-L0-42 |
| `reduce` | `fn map.reduce(m: Map[K, V], f: ((K, V), (K, V)) -> (K, V)) -> (K, V)?` | 新規 | R-L0-42 |
| `remove` | `fn map.remove(m: Map[K, V], key: K) -> Map[K, V]` |  | R-map-01（置き換え先） |
| `scan` | `fn map.scan(m: Map[K, V], init: B, f: (B, (K, V)) -> B) -> List[B]` | 新規 | R-L0-42 |
| `set` | `fn map.set(m: Map[K, V], key: K, value: V) -> Map[K, V]` |  | R-map-01（置き換え先） |
| `sort_by` | `fn map.sort_by(m: Map[K, V], f: ((K, V)) -> B) -> List[(K, V)]` | 新規 | R-L0-42 |
| `sort_with` | `fn map.sort_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> List[(K, V)]` | 新規 | R-L0-42 |
| `to_list` | `fn map.to_list(m: Map[K, V]) -> List[(K, V)]` | 改名（旧 `entries`） | R-map-07 |
| `update` | `fn map.update(m: Map[K, V], key: K, f: (V) -> V) -> Map[K, V]` | 意味変更: キーが無ければ abort | R-map-08 |
| `upsert` | `fn map.upsert(m: Map[K, V], key: K, init: V, f: (V) -> V) -> Map[K, V]` |  |  |
| `values` | `fn map.values(m: Map[K, V]) -> List[V]` |  |  |
| `zip` | `fn map.zip(m: Map[K, V], ys: List[B]) -> List[((K, V), B)]` | 新規 | R-L0-42 |
| `zip_with` | `fn map.zip_with(m: Map[K, V], ys: List[B], f: ((K, V), B) -> C) -> List[C]` | 新規 | R-L0-42 |

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
| `hypot` | `fn math.hypot(a: Float, b: Float) -> Float` | 新規 | R-math-02 |
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
| `concat_cols` | `fn matrix.concat_cols(xs: List[Matrix]) -> Matrix` |  |  |
| `concat_rows` | `fn matrix.concat_rows(xs: List[Matrix]) -> Matrix` | 新規 | R-matrix-11 |
| `dot_row` | `fn matrix.dot_row(m: Matrix, r: Int, vec: List[Float]) -> Float` | 意味変更: 範囲・形の不一致は abort | R-matrix-05（範囲外は abort） |
| `from_bytes_f16_le` | `fn matrix.from_bytes_f16_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix?` | 意味変更: 負の形は abort、データ不足は none | R-matrix-05、R-matrix-15 |
| `from_bytes_f32_le` | `fn matrix.from_bytes_f32_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix?` | 意味変更: 負の形は abort、データ不足は none | R-matrix-05、R-matrix-15 |
| `from_bytes_f64_le` | `fn matrix.from_bytes_f64_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix?` | 意味変更: 負の形は abort、データ不足は none | R-matrix-05、R-matrix-15 |
| `from_lists` | `fn matrix.from_lists(xs: List[List[Float]]) -> Matrix` |  |  |
| `gather_rows` | `fn matrix.gather_rows(m: Matrix, indices: List[Int]) -> Matrix` | 意味変更: 範囲・形の不一致は abort | R-matrix-04・R-matrix-05（範囲外は abort） |
| `get` | `fn matrix.get(m: Matrix, row: Int, col: Int) -> Float?` | 意味変更: `Option[Float]` を返す | R-matrix-08 |
| `map` | `fn matrix.map(m: Matrix, f: (Float) -> Float) -> Matrix` |  |  |
| `ones` | `fn matrix.ones(rows: Int, cols: Int) -> Matrix` |  |  |
| `rows` | `fn matrix.rows(m: Matrix) -> Int` |  |  |
| `scale` | `fn matrix.scale(m: Matrix, s: Float) -> Matrix` |  |  |
| `shape` | `fn matrix.shape(m: Matrix) -> (Int, Int)` |  |  |
| `slice_rows` | `fn matrix.slice_rows(m: Matrix, start: Int, end: Int) -> Matrix` | 意味変更: 負の位置で abort。正の側は丸める | R-L0-29、R-L0-46 |
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
| `tcp_accept` | `effect fn net.tcp_accept(listener: TcpListener) -> TcpStream` | 意味変更: 取っ手を `TcpStream`・`TcpListener` にする | R-net-05 |
| `tcp_available` | `effect fn net.tcp_available(stream: TcpStream) -> Int` | 意味変更: 取っ手を `TcpStream`・`TcpListener` にする | R-net-05 |
| `tcp_connect` | `effect fn net.tcp_connect(host: String, port: Int) -> TcpStream` | 意味変更: 取っ手を `TcpStream`・`TcpListener` にする | R-net-05 |
| `tcp_is_open` | `effect fn net.tcp_is_open(stream: TcpStream) -> Bool` | 意味変更: effect fn にし、`stream: TcpStream` にする | R-net-04、R-net-05 |
| `tcp_listen` | `effect fn net.tcp_listen(host: String, port: Int) -> TcpListener` | 意味変更: 取っ手を `TcpStream`・`TcpListener` にする | R-net-05 |
| `tcp_local_port` | `effect fn net.tcp_local_port(listener: TcpListener) -> Int` | 意味変更: 取っ手を `TcpStream`・`TcpListener` にする | R-net-05 |
| `tcp_read_timeout` | `effect fn net.tcp_read_timeout(stream: TcpStream, len: Int, timeout_ms: Int) -> Bytes` | 意味変更: 取っ手を `TcpStream`・`TcpListener` にする | R-net-05 |
| `tcp_set_timeout` | `effect fn net.tcp_set_timeout(stream: TcpStream, timeout_ms: Int) -> Unit` | 意味変更: 取っ手を `TcpStream`・`TcpListener` にする | R-net-05 |
| `udp_bind` | `effect fn net.udp_bind(host: String, port: Int) -> UdpSocket` | 新規 | R-net-02 |
| `udp_local_port` | `effect fn net.udp_local_port(sock: UdpSocket) -> Int` | 新規 | R-net-02 |
| `udp_recv_from` | `effect fn net.udp_recv_from(sock: UdpSocket, max: Int) -> UdpPacket` | 新規。`UdpPacket` を返す | R-net-01、R-net-06 |
| `udp_send_to` | `effect fn net.udp_send_to(sock: UdpSocket, data: Bytes, host: String, port: Int) -> Unit` | 新規 | R-net-02 |

撤去（18）:

| 関数 | 代わり | 案 |
|---|---|---|
| `shm_create` | `unix.shm_create` | R-net-01 |
| `shm_read` | `unix.shm_read` | R-net-01 |
| `shm_resize` | `unix.shm_resize` | R-net-01 |
| `shm_write` | `unix.shm_write` | R-net-01 |
| `tcp_close` | `io.close` | R-net-07 |
| `tcp_close_listener` | `io.close` | R-net-07 |
| `tcp_read` | `io.read` | R-net-07 |
| `tcp_read_exact` | `io.read_exact` | R-net-07 |
| `tcp_write` | `io.write` | R-net-07 |
| `unix_accept` | `unix.accept` | R-net-01 |
| `unix_close` | `io.close` | R-net-01 |
| `unix_connect` | `unix.connect` | R-net-01 |
| `unix_listen` | `unix.listen` | R-net-01 |
| `unix_poll` | `unix.select([fd], t)` | R-net-01 |
| `unix_recv` | `io.read` | R-net-01 |
| `unix_send` | `unix.write_fds` | R-net-01 |
| `unix_take_fds` | `unix.take_fds` | R-net-01 |
| `unix_wait` | `unix.select` | R-net-01 |

## option

記録: [L2-option.md](L2-option.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `collect` | `fn option.collect(xs: List[A?]) -> List[A]?` |  | R-option-02 |
| `collect_map` | `fn option.collect_map(xs: List[A], f: (A) -> B?) -> List[B]?` |  | R-option-02 |
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
| `basename` | `fn path.basename(path: String) -> String` |  |  |
| `components` | `fn path.components(path: String) -> List[String]` | 新規 | R-path-01 |
| `dirname` | `fn path.dirname(path: String) -> String` |  |  |
| `extension` | `fn path.extension(path: String) -> String?` |  |  |
| `is_absolute` | `fn path.is_absolute(path: String) -> Bool` |  |  |
| `join` | `fn path.join(base: String, child: String) -> String` |  |  |
| `normalize` | `fn path.normalize(path: String) -> String` |  |  |
| `relative_to` | `fn path.relative_to(path: String, base: String) -> String?` | 新規 | R-path-01 |
| `stem` | `fn path.stem(path: String) -> String` |  |  |
| `with_extension` | `fn path.with_extension(path: String, ext: String) -> String` | 新規 | R-path-03 |

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
| `choice` | `effect fn random.choice(xs: List[A]) -> A?` |  |  |
| `float` | `effect fn random.float() -> Float` |  |  |
| `int` | `effect fn random.int(min: Int, max: Int) -> Int` |  | R-random-04 |
| `next_choice` | `fn random.next_choice[A](rng: Rng, xs: List[A]) -> (A?, Rng)` | 新規 | R-random-01 |
| `next_float` | `fn random.next_float(rng: Rng) -> (Float, Rng)` | 新規 | R-random-01 |
| `next_int` | `fn random.next_int(rng: Rng, min: Int, max: Int) -> (Int, Rng)` | 新規 | R-random-01 |
| `next_shuffle` | `fn random.next_shuffle[A](rng: Rng, xs: List[A]) -> (List[A], Rng)` | 新規 | R-random-01 |
| `seeded` | `fn random.seeded(seed: Int) -> Rng` | 新規 | R-random-01 |
| `shuffle` | `effect fn random.shuffle(xs: List[A]) -> List[A]` |  | R-random-03 |

## regex

記録: [L2-regex.md](L2-regex.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `captures` | `fn regex.captures(re: Regex, s: String) -> List[String?]?` | 意味変更: 第 1 引数を `Regex` に。グループは `String?` | R-regex-08 |
| `captures_all` | `fn regex.captures_all(re: Regex, s: String) -> List[List[String?]]` | 新規 | R-regex-04、R-regex-08 |
| `compile` | `fn regex.compile(pattern: String) -> Result[Regex, String]` | 新規 | R-regex-02 |
| `escape` | `fn regex.escape(s: String) -> String` | 新規 | R-regex-04 |
| `find` | `fn regex.find(re: Regex, s: String) -> String?` | 意味変更: 第 1 引数を `Regex` に | R-regex-02 |
| `find_all` | `fn regex.find_all(re: Regex, s: String) -> List[String]` | 意味変更: 第 1 引数を `Regex` に | R-regex-02 |
| `index_of` | `fn regex.index_of(re: Regex, s: String) -> Int?` | 新規 | R-regex-04 |
| `is_full_match` | `fn regex.is_full_match(re: Regex, s: String) -> Bool` | 改名（旧 `full_match`）。第 1 引数は `Regex` | R-regex-01、R-regex-02 |
| `is_match` | `fn regex.is_match(re: Regex, s: String) -> Bool` | 意味変更: 第 1 引数を `Regex` に | R-regex-02 |
| `new` | `fn regex.new(pattern: String) -> Regex` | 新規 | R-regex-02 |
| `replace` | `fn regex.replace(re: Regex, s: String, rep: String) -> String` | 意味変更: 第 1 引数を `Regex` に | R-regex-02、R-regex-05（`$1` は解釈しない） |
| `replace_first` | `fn regex.replace_first(re: Regex, s: String, rep: String) -> String` | 意味変更: 第 1 引数を `Regex` に | R-regex-02 |
| `replace_first_with` | `fn regex.replace_first_with(re: Regex, s: String, f: (List[String?]) -> String) -> String` | 新規 | R-regex-07、R-regex-08 |
| `replace_with` | `fn regex.replace_with(re: Regex, s: String, f: (List[String?]) -> String) -> String` | 新規 | R-regex-05、R-regex-08 |
| `split` | `fn regex.split(re: Regex, s: String) -> List[String]` | 意味変更: 第 1 引数を `Regex` に | R-regex-02 |

## result

記録: [L2-result.md](L2-result.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `filter` | `fn result.filter(r: Result[A, E], f: (A) -> Bool, err_val: E) -> Result[A, E]` |  | R-result-02 |
| `flat_map` | `fn result.flat_map(r: Result[A, E], f: (A) -> Result[B, E]) -> Result[B, E]` |  | R-L0-10 |
| `flatten` | `fn result.flatten(r: Result[Result[A, E], E]) -> Result[A, E]` |  | R-option-04 |
| `is_err` | `fn result.is_err(r: Result[A, E]) -> Bool` |  |  |
| `is_ok` | `fn result.is_ok(r: Result[A, E]) -> Bool` |  |  |
| `map` | `fn result.map(r: Result[A, E], f: (A) -> B) -> Result[B, E]` |  |  |
| `map_err` | `fn result.map_err(r: Result[A, E], f: (E) -> F) -> Result[A, F]` |  | ADR-0004 D2 の正準形 |
| `or_else` | `fn result.or_else(r: Result[A, E], f: (E) -> Result[A, F]) -> Result[A, F]` |  |  |
| `partition` | `fn result.partition(xs: List[Result[A, E]]) -> (List[A], List[E])` |  | R-result-05 |
| `to_err_option` | `fn result.to_err_option(r: Result[A, E]) -> E?` |  | R-result-03 |
| `to_list` | `fn result.to_list(r: Result[A, E]) -> List[A]` |  | R-option-04 |
| `to_option` | `fn result.to_option(r: Result[A, E]) -> A?` |  | `?` の脱糖先 |
| `unwrap_or_else` | `fn result.unwrap_or_else(r: Result[A, E], f: (E) -> A) -> A` |  | `??` の脱糖先（ADR-0005 D1） |
| `zip` | `fn result.zip(a: Result[A, E], b: Result[B, E]) -> Result[(A, B), E]` |  | R-option-04 |

撤去（1）:

| 関数 | 代わり | 案 |
|---|---|---|
| `unwrap_or` | `r ?? d` | R-result-01 |

## secret

記録: [L2-secret.md](L2-secret.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `new` | `fn secret.new[A](x: A) -> Secret[A]` | 新規 | R-secret-01 |
| `reveal` | `fn secret.reveal[A](s: Secret[A]) -> A` | 新規 | R-secret-01 |

## set

記録: [L2-set.md](L2-set.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `all` | `fn set.all(s: Set[A], f: (A) -> Bool) -> Bool` |  | R-map-06 |
| `any` | `fn set.any(s: Set[A], f: (A) -> Bool) -> Bool` |  | R-map-06 |
| `clear` | `fn set.clear(mut s: Set[A]) -> Unit` | 新規 | R-L0-04（`clear` の行列） |
| `contains` | `fn set.contains(s: Set[A], x: A) -> Bool` |  |  |
| `count` | `fn set.count(s: Set[A], f: (A) -> Bool) -> Int` | 新規 | R-set-01 |
| `difference` | `fn set.difference(a: Set[A], b: Set[A]) -> Set[A]` |  |  |
| `enumerate` | `fn set.enumerate(s: Set[A]) -> List[(Int, A)]` | 新規 | R-L0-42 |
| `filter` | `fn set.filter(s: Set[A], f: (A) -> Bool) -> Set[A]` |  | R-map-06 |
| `filter_map` | `fn set.filter_map(s: Set[A], f: (A) -> B?) -> List[B]` | 新規 | R-L0-42 |
| `find` | `fn set.find(s: Set[A], f: (A) -> Bool) -> A?` | 新規 | R-set-01 |
| `find_map` | `fn set.find_map(s: Set[A], f: (A) -> B?) -> B?` | 新規 | R-L0-42 |
| `fold` | `fn set.fold(s: Set[A], init: B, f: (B, A) -> B) -> B` |  | R-map-06 |
| `from_list` | `fn set.from_list(xs: List[A]) -> Set[A]` |  | — (R-L0-07) |
| `group_by` | `fn set.group_by(s: Set[A], f: (A) -> B) -> Map[B, List[A]]` | 新規 | R-L0-42 |
| `insert` | `fn set.insert(s: Set[A], x: A) -> Set[A]` |  | R-set-04 |
| `intersection` | `fn set.intersection(a: Set[A], b: Set[A]) -> Set[A]` |  |  |
| `is_disjoint` | `fn set.is_disjoint(a: Set[A], b: Set[A]) -> Bool` |  |  |
| `is_empty` | `fn set.is_empty(s: Set[A]) -> Bool` |  |  |
| `is_subset` | `fn set.is_subset(a: Set[A], b: Set[A]) -> Bool` |  | R-set-02 |
| `len` | `fn set.len(s: Set[A]) -> Int` |  | — (R-L0-11) |
| `map` | `fn set.map(s: Set[A], f: (A) -> B) -> Set[B]` |  | R-map-06 |
| `max` | `fn set.max(s: Set[A]) -> A?` | 新規 | R-L0-42 |
| `max_by` | `fn set.max_by(s: Set[A], f: (A) -> K) -> A?` | 新規 | R-L0-42 |
| `max_with` | `fn set.max_with(s: Set[A], cmp: (A, A) -> Int) -> A?` | 新規 | R-L0-42 |
| `min` | `fn set.min(s: Set[A]) -> A?` | 新規 | R-L0-42 |
| `min_by` | `fn set.min_by(s: Set[A], f: (A) -> K) -> A?` | 新規 | R-L0-42 |
| `min_with` | `fn set.min_with(s: Set[A], cmp: (A, A) -> Int) -> A?` | 新規 | R-L0-42 |
| `new` | `fn set.new() -> Set[A]` |  |  |
| `partition` | `fn set.partition(s: Set[A], f: (A) -> Bool) -> (List[A], List[A])` | 新規 | R-L0-42 |
| `product` | `fn set.product[A: Numeric](s: Set[A]) -> A` | 新規 | R-L0-42 |
| `reduce` | `fn set.reduce(s: Set[A], f: (A, A) -> A) -> A?` | 新規 | R-L0-42 |
| `remove` | `fn set.remove(s: Set[A], x: A) -> Set[A]` |  |  |
| `scan` | `fn set.scan(s: Set[A], init: B, f: (B, A) -> B) -> List[B]` | 新規 | R-L0-42 |
| `sort` | `fn set.sort(s: Set[A]) -> List[A]` | 新規 | R-L0-42 |
| `sort_by` | `fn set.sort_by(s: Set[A], f: (A) -> B) -> List[A]` | 新規 | R-L0-42 |
| `sort_with` | `fn set.sort_with(s: Set[A], cmp: (A, A) -> Int) -> List[A]` | 新規 | R-L0-42 |
| `sum` | `fn set.sum[A: Numeric](s: Set[A]) -> A` | 新規 | R-L0-42 |
| `symmetric_difference` | `fn set.symmetric_difference(a: Set[A], b: Set[A]) -> Set[A]` |  |  |
| `to_list` | `fn set.to_list(s: Set[A]) -> List[A]` |  |  |
| `union` | `fn set.union(a: Set[A], b: Set[A]) -> Set[A]` |  |  |
| `zip` | `fn set.zip(s: Set[A], ys: List[B]) -> List[(A, B)]` | 新規 | R-L0-42 |
| `zip_with` | `fn set.zip_with(s: Set[A], ys: List[B], f: (A, B) -> C) -> List[C]` | 新規 | R-L0-42 |

## string

記録: [L2-string.md](L2-string.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `byte_len` | `fn string.byte_len(s: String) -> Int` | 新規 | R-string-06 |
| `byte_slice` | `fn string.byte_slice(s: String, start: Int, end: Int) -> String?` |  |  |
| `capitalize` | `fn string.capitalize(s: String) -> String` |  |  |
| `chunk` | `fn string.chunk(s: String, n: Int) -> List[String]` | 新規 | R-L0-42 |
| `clear` | `fn string.clear(mut s: String) -> Unit` |  |  |
| `compare` | `fn string.compare(a: String, b: String) -> Int` | 新規 | R-string-05（R-L0-16） |
| `contains` | `fn string.contains(s: String, needle: String) -> Bool` |  |  |
| `count` | `fn string.count(s: String, f: (String) -> Bool) -> Int` | 新規 | R-string-18、R-L0-42 |
| `count_matches` | `fn string.count_matches(s: String, needle: String) -> Int` | 改名（旧 `count`） | R-string-18 |
| `dedup` | `fn string.dedup(s: String) -> String` | 新規 | R-L0-42 |
| `drop` | `fn string.drop(s: String, n: Int) -> String` |  |  |
| `drop_end` | `fn string.drop_end(s: String, n: Int) -> String` |  |  |
| `drop_while` | `fn string.drop_while(s: String, f: (String) -> Bool) -> String` | 新規 | R-L0-42 |
| `ends_with` | `fn string.ends_with(s: String, suffix: String) -> Bool` |  |  |
| `enumerate` | `fn string.enumerate(s: String) -> List[(Int, String)]` | 新規 | R-L0-42 |
| `filter` | `fn string.filter(s: String, f: (String) -> Bool) -> String` | 新規 | R-string-16 |
| `filter_map` | `fn string.filter_map(s: String, f: (String) -> B?) -> List[B]` | 新規 | R-L0-42 |
| `find_index` | `fn string.find_index(s: String, f: (String) -> Bool) -> Int?` | 新規 | R-L0-42 |
| `find_map` | `fn string.find_map(s: String, f: (String) -> B?) -> B?` | 新規 | R-L0-42 |
| `first` | `fn string.first(s: String) -> String?` |  |  |
| `fold` | `fn string.fold(s: String, init: B, f: (B, String) -> B) -> B` | 新規 | R-string-16 |
| `from_codepoint` | `fn string.from_codepoint(n: Int) -> String?` | 意味変更: `Option[String]` を返す | R-string-11 |
| `from_list` | `fn string.from_list(xs: List[String]) -> String` | 新規 | R-string-17 |
| `from_utf16` | `fn string.from_utf16(xs: List[Int]) -> Result[String, String]` | 新規 | R-string-07 |
| `get` | `fn string.get(s: String, i: Int) -> String?` |  |  |
| `group_by` | `fn string.group_by(s: String, f: (String) -> B) -> Map[B, List[String]]` | 新規 | R-L0-42 |
| `index_of` | `fn string.index_of(s: String, needle: String) -> Int?` |  |  |
| `insert` | `fn string.insert(s: String, i: Int, x: String) -> String` | 新規 | R-string-16 |
| `intersperse` | `fn string.intersperse(s: String, sep: String) -> String` | 新規 | R-L0-42 |
| `is_alpha` | `fn string.is_alpha(s: String) -> Bool` |  |  |
| `is_alphanumeric` | `fn string.is_alphanumeric(s: String) -> Bool` |  |  |
| `is_digit` | `fn string.is_digit(s: String) -> Bool` |  |  |
| `is_empty` | `fn string.is_empty(s: String) -> Bool` |  |  |
| `is_equal_constant_time` | `fn string.is_equal_constant_time(a: String, b: String) -> Bool` | 新規 | R-string-19 |
| `is_lower` | `fn string.is_lower(s: String) -> Bool` |  |  |
| `is_upper` | `fn string.is_upper(s: String) -> Bool` |  |  |
| `is_whitespace` | `fn string.is_whitespace(s: String) -> Bool` |  |  |
| `last` | `fn string.last(s: String) -> String?` |  |  |
| `last_index_of` | `fn string.last_index_of(s: String, needle: String) -> Int?` |  |  |
| `len` | `fn string.len(s: String) -> Int` |  |  |
| `lines` | `fn string.lines(s: String) -> List[String]` |  |  |
| `max` | `fn string.max(s: String) -> String?` | 新規 | R-L0-42 |
| `max_by` | `fn string.max_by(s: String, f: (String) -> K) -> String?` | 新規 | R-L0-42 |
| `max_with` | `fn string.max_with(s: String, cmp: (String, String) -> Int) -> String?` | 新規 | R-L0-42 |
| `min` | `fn string.min(s: String) -> String?` | 新規 | R-L0-42 |
| `min_by` | `fn string.min_by(s: String, f: (String) -> K) -> String?` | 新規 | R-L0-42 |
| `min_with` | `fn string.min_with(s: String, cmp: (String, String) -> Int) -> String?` | 新規 | R-L0-42 |
| `new` | `fn string.new() -> String` | 新規 | R-string-17 |
| `pad_end` | `fn string.pad_end(s: String, n: Int, fill: String) -> String` |  |  |
| `pad_start` | `fn string.pad_start(s: String, n: Int, fill: String) -> String` |  |  |
| `partition` | `fn string.partition(s: String, f: (String) -> Bool) -> (List[String], List[String])` | 新規 | R-L0-42 |
| `pop` | `fn string.pop(mut s: String) -> String?` | 新規 | R-string-16 |
| `push` | `fn string.push(mut s: String, x: String) -> Unit` |  |  |
| `reduce` | `fn string.reduce(s: String, f: (String, String) -> String) -> String?` | 新規 | R-L0-42 |
| `remove_at` | `fn string.remove_at(s: String, i: Int) -> String` | 新規 | R-string-16 |
| `repeat` | `fn string.repeat(s: String, n: Int) -> String` |  |  |
| `replace` | `fn string.replace(s: String, from: String, to: String) -> String` |  |  |
| `replace_first` | `fn string.replace_first(s: String, from: String, to: String) -> String` |  |  |
| `reverse` | `fn string.reverse(s: String) -> String` |  |  |
| `scan` | `fn string.scan(s: String, init: B, f: (B, String) -> B) -> List[B]` | 新規 | R-L0-42 |
| `slice` | `fn string.slice(s: String, start: Int, end: Int) -> String` | 意味変更: `end` の既定値を外す。2 引数は `drop` | R-string-15 |
| `sort` | `fn string.sort(s: String) -> List[String]` | 新規 | R-L0-42 |
| `sort_by` | `fn string.sort_by(s: String, f: (String) -> B) -> List[String]` | 新規 | R-L0-42 |
| `sort_with` | `fn string.sort_with(s: String, cmp: (String, String) -> Int) -> List[String]` | 新規 | R-L0-42 |
| `split` | `fn string.split(s: String, sep: String) -> List[String]` |  |  |
| `split_at` | `fn string.split_at(s: String, n: Int) -> (String, String)` | 新規 | R-L0-42 |
| `split_once` | `fn string.split_once(s: String, sep: String) -> (String, String)?` |  |  |
| `starts_with` | `fn string.starts_with(s: String, prefix: String) -> Bool` |  |  |
| `strip_prefix` | `fn string.strip_prefix(s: String, prefix: String) -> String?` |  |  |
| `strip_suffix` | `fn string.strip_suffix(s: String, suffix: String) -> String?` |  |  |
| `take` | `fn string.take(s: String, n: Int) -> String` |  |  |
| `take_end` | `fn string.take_end(s: String, n: Int) -> String` |  |  |
| `take_while` | `fn string.take_while(s: String, f: (String) -> Bool) -> String` | 新規 | R-L0-42 |
| `to_codepoint` | `fn string.to_codepoint(s: String) -> Int?` | 改名（旧 `codepoint`）。1 コードポイントちょうどでなければ none | R-string-12、R-string-14 |
| `to_list` | `fn string.to_list(s: String) -> List[String]` | 改名（旧 `chars`） | R-string-17 |
| `to_lower` | `fn string.to_lower(s: String) -> String` |  |  |
| `to_upper` | `fn string.to_upper(s: String) -> String` |  |  |
| `to_utf16` | `fn string.to_utf16(s: String) -> List[Int]` | 新規 | R-string-07 |
| `trim` | `fn string.trim(s: String) -> String` |  |  |
| `trim_end` | `fn string.trim_end(s: String) -> String` |  |  |
| `trim_start` | `fn string.trim_start(s: String) -> String` |  |  |
| `unique` | `fn string.unique(s: String) -> String` | 新規 | R-L0-42 |
| `unique_by` | `fn string.unique_by(s: String, f: (String) -> K) -> String` | 新規 | R-L0-42 |
| `window` | `fn string.window(s: String, n: Int) -> List[String]` | 新規 | R-L0-42 |
| `zip` | `fn string.zip(s: String, ys: List[B]) -> List[(String, B)]` | 新規 | R-L0-42 |
| `zip_with` | `fn string.zip_with(s: String, ys: List[B], f: (String, B) -> C) -> List[C]` | 新規 | R-L0-42 |

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
| `assert_aborts` | `fn testing.assert_aborts[A](expected: String, f: () -> A) -> Unit` |  |  |
| `assert_approx` | `fn testing.assert_approx(a: Float, b: Float, tolerance: Float) -> Unit` |  |  |
| `assert_contains` | `fn testing.assert_contains(haystack: String, needle: String) -> Unit` |  | R-testing-01 |
| `assert_err` | `fn testing.assert_err[A, B](r: Result[A, B]) -> Unit` |  |  |
| `assert_gt` | `fn testing.assert_gt[A](a: A, b: A) -> Unit` | 意味変更: 順序のある型すべてに広げる | R-testing-02 |
| `assert_lt` | `fn testing.assert_lt[A](a: A, b: A) -> Unit` | 意味変更: 順序のある型すべてに広げる | R-testing-02 |
| `assert_none` | `fn testing.assert_none[A](o: A?) -> Unit` |  |  |
| `assert_ok` | `fn testing.assert_ok[A, B](r: Result[A, B]) -> Unit` |  |  |
| `assert_snapshot` | `fn testing.assert_snapshot(actual: String, expected: String) -> Unit` |  |  |
| `assert_some` | `fn testing.assert_some[A](o: A?) -> Unit` |  |  |

## uint16

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `band` | `fn uint16.band(a: UInt16, b: UInt16) -> UInt16` | 新規 | R-sized-10 |
| `bit_length` | `fn uint16.bit_length(x: UInt16) -> Int` | 新規 | R-int-11 |
| `bnot` | `fn uint16.bnot(x: UInt16) -> UInt16` | 新規 | R-sized-10 |
| `bor` | `fn uint16.bor(a: UInt16, b: UInt16) -> UInt16` | 新規 | R-sized-10 |
| `bshl` | `fn uint16.bshl(a: UInt16, n: Int) -> UInt16` | 新規 | R-sized-10 |
| `bshr` | `fn uint16.bshr(a: UInt16, n: Int) -> UInt16` | 新規 | R-sized-10 |
| `bxor` | `fn uint16.bxor(a: UInt16, b: UInt16) -> UInt16` | 新規 | R-sized-10 |
| `byte_swap` | `fn uint16.byte_swap(x: UInt16) -> UInt16` | 新規 | R-sized-10 |
| `clamp` | `fn uint16.clamp(x: UInt16, lo: UInt16, hi: UInt16) -> UInt16` | 新規 | R-sized-08 |
| `compare` | `fn uint16.compare(a: UInt16, b: UInt16) -> Int` | 新規 | R-sized-02 |
| `count_leading_zeros` | `fn uint16.count_leading_zeros(x: UInt16) -> Int` | 新規 | R-sized-10 |
| `count_ones` | `fn uint16.count_ones(x: UInt16) -> Int` | 新規 | R-sized-10 |
| `count_trailing_zeros` | `fn uint16.count_trailing_zeros(x: UInt16) -> Int` | 新規 | R-sized-10 |
| `floor_div` | `fn uint16.floor_div(a: UInt16, b: UInt16) -> UInt16` | 新規 | R-sized-10 |
| `from` | `fn uint16.from[S: Numeric](x: S) -> UInt16` | 新規 | R-L0-40 |
| `from_checked` | `fn uint16.from_checked[S: Numeric](x: S) -> UInt16?` | 新規 | R-L0-40 |
| `from_saturating` | `fn uint16.from_saturating[S: Numeric](x: S) -> UInt16` | 新規 | R-L0-40 |
| `from_wrapping` | `fn uint16.from_wrapping[S: Integer](x: S) -> UInt16` | 新規 | R-L0-40 |
| `is_even` | `fn uint16.is_even(x: UInt16) -> Bool` | 新規 | R-sized-10 |
| `is_odd` | `fn uint16.is_odd(x: UInt16) -> Bool` | 新規 | R-sized-10 |
| `max` | `fn uint16.max(a: UInt16, b: UInt16) -> UInt16` | 新規 | R-sized-08 |
| `max_value` | `fn uint16.max_value() -> UInt16` |  |  |
| `min` | `fn uint16.min(a: UInt16, b: UInt16) -> UInt16` | 新規 | R-sized-08 |
| `min_value` | `fn uint16.min_value() -> UInt16` |  |  |
| `modulo` | `fn uint16.modulo(a: UInt16, b: UInt16) -> UInt16` | 新規 | R-sized-10 |
| `parse` | `fn uint16.parse(s: String) -> Result[UInt16, String]` | 新規 | R-sized-02 |
| `parse_radix` | `fn uint16.parse_radix(s: String, radix: Int) -> Result[UInt16, String]` | 新規 | R-sized-06 |
| `pow` | `fn uint16.pow(base: UInt16, exp: Int) -> UInt16` | 新規 | R-sized-10 |
| `rotate_left` | `fn uint16.rotate_left(a: UInt16, n: Int) -> UInt16` | 新規 | R-int-12 |
| `rotate_right` | `fn uint16.rotate_right(a: UInt16, n: Int) -> UInt16` | 新規 | R-int-12 |
| `to_string` | `fn uint16.to_string(x: UInt16) -> String` |  |  |
| `to_string_radix` | `fn uint16.to_string_radix(x: UInt16, radix: Int) -> String` | 新規 | R-sized-06 |
| `wrap_add` | `fn uint16.wrap_add(a: UInt16, b: UInt16) -> UInt16` | 新規 | R-int-12 |
| `wrap_mul` | `fn uint16.wrap_mul(a: UInt16, b: UInt16) -> UInt16` | 新規 | R-int-12 |
| `wrap_sub` | `fn uint16.wrap_sub(a: UInt16, b: UInt16) -> UInt16` | 新規 | R-int-12 |

撤去（15）:

| 関数 | 代わり | 案 |
|---|---|---|
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int16_checked` | `int16.from_checked` | R-L0-40 |
| `to_int16_saturating` | `int16.from_saturating` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_int8_checked` | `int8.from_checked` | R-L0-40 |
| `to_int8_saturating` | `int8.from_saturating` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |
| `to_uint8_checked` | `uint8.from_checked` | R-L0-40 |
| `to_uint8_saturating` | `uint8.from_saturating` | R-L0-40 |

## uint32

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `band` | `fn uint32.band(a: UInt32, b: UInt32) -> UInt32` | 新規 | R-sized-10 |
| `bit_length` | `fn uint32.bit_length(x: UInt32) -> Int` | 新規 | R-int-11 |
| `bnot` | `fn uint32.bnot(x: UInt32) -> UInt32` | 新規 | R-sized-10 |
| `bor` | `fn uint32.bor(a: UInt32, b: UInt32) -> UInt32` | 新規 | R-sized-10 |
| `bshl` | `fn uint32.bshl(a: UInt32, n: Int) -> UInt32` | 新規 | R-sized-10 |
| `bshr` | `fn uint32.bshr(a: UInt32, n: Int) -> UInt32` | 新規 | R-sized-10 |
| `bxor` | `fn uint32.bxor(a: UInt32, b: UInt32) -> UInt32` | 新規 | R-sized-10 |
| `byte_swap` | `fn uint32.byte_swap(x: UInt32) -> UInt32` | 新規 | R-sized-10 |
| `clamp` | `fn uint32.clamp(x: UInt32, lo: UInt32, hi: UInt32) -> UInt32` | 新規 | R-sized-08 |
| `compare` | `fn uint32.compare(a: UInt32, b: UInt32) -> Int` | 新規 | R-sized-02 |
| `count_leading_zeros` | `fn uint32.count_leading_zeros(x: UInt32) -> Int` | 新規 | R-sized-10 |
| `count_ones` | `fn uint32.count_ones(x: UInt32) -> Int` | 新規 | R-sized-10 |
| `count_trailing_zeros` | `fn uint32.count_trailing_zeros(x: UInt32) -> Int` | 新規 | R-sized-10 |
| `floor_div` | `fn uint32.floor_div(a: UInt32, b: UInt32) -> UInt32` | 新規 | R-sized-10 |
| `from` | `fn uint32.from[S: Numeric](x: S) -> UInt32` | 新規 | R-L0-40 |
| `from_checked` | `fn uint32.from_checked[S: Numeric](x: S) -> UInt32?` | 新規 | R-L0-40 |
| `from_saturating` | `fn uint32.from_saturating[S: Numeric](x: S) -> UInt32` | 新規 | R-L0-40 |
| `from_wrapping` | `fn uint32.from_wrapping[S: Integer](x: S) -> UInt32` | 新規 | R-L0-40 |
| `is_even` | `fn uint32.is_even(x: UInt32) -> Bool` | 新規 | R-sized-10 |
| `is_odd` | `fn uint32.is_odd(x: UInt32) -> Bool` | 新規 | R-sized-10 |
| `max` | `fn uint32.max(a: UInt32, b: UInt32) -> UInt32` | 新規 | R-sized-08 |
| `max_value` | `fn uint32.max_value() -> UInt32` |  |  |
| `min` | `fn uint32.min(a: UInt32, b: UInt32) -> UInt32` | 新規 | R-sized-08 |
| `min_value` | `fn uint32.min_value() -> UInt32` |  |  |
| `modulo` | `fn uint32.modulo(a: UInt32, b: UInt32) -> UInt32` | 新規 | R-sized-10 |
| `parse` | `fn uint32.parse(s: String) -> Result[UInt32, String]` | 新規 | R-sized-02 |
| `parse_radix` | `fn uint32.parse_radix(s: String, radix: Int) -> Result[UInt32, String]` | 新規 | R-sized-06 |
| `pow` | `fn uint32.pow(base: UInt32, exp: Int) -> UInt32` | 新規 | R-sized-10 |
| `rotate_left` | `fn uint32.rotate_left(a: UInt32, n: Int) -> UInt32` | 新規 | R-int-12 |
| `rotate_right` | `fn uint32.rotate_right(a: UInt32, n: Int) -> UInt32` | 新規 | R-int-12 |
| `to_string` | `fn uint32.to_string(x: UInt32) -> String` |  |  |
| `to_string_radix` | `fn uint32.to_string_radix(x: UInt32, radix: Int) -> String` | 新規 | R-sized-06 |
| `wrap_add` | `fn uint32.wrap_add(a: UInt32, b: UInt32) -> UInt32` | 新規 | R-int-12 |
| `wrap_mul` | `fn uint32.wrap_mul(a: UInt32, b: UInt32) -> UInt32` | 新規 | R-int-12 |
| `wrap_sub` | `fn uint32.wrap_sub(a: UInt32, b: UInt32) -> UInt32` | 新規 | R-int-12 |

撤去（19）:

| 関数 | 代わり | 案 |
|---|---|---|
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int16_checked` | `int16.from_checked` | R-L0-40 |
| `to_int16_saturating` | `int16.from_saturating` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int32_checked` | `int32.from_checked` | R-L0-40 |
| `to_int32_saturating` | `int32.from_saturating` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_int8_checked` | `int8.from_checked` | R-L0-40 |
| `to_int8_saturating` | `int8.from_saturating` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint16_checked` | `uint16.from_checked` | R-L0-40 |
| `to_uint16_saturating` | `uint16.from_saturating` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |
| `to_uint8_checked` | `uint8.from_checked` | R-L0-40 |
| `to_uint8_saturating` | `uint8.from_saturating` | R-L0-40 |

## uint64

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `band` | `fn uint64.band(a: UInt64, b: UInt64) -> UInt64` | 新規 | R-sized-10 |
| `bit_length` | `fn uint64.bit_length(x: UInt64) -> Int` | 新規 | R-int-11 |
| `bnot` | `fn uint64.bnot(x: UInt64) -> UInt64` | 新規 | R-sized-10 |
| `bor` | `fn uint64.bor(a: UInt64, b: UInt64) -> UInt64` | 新規 | R-sized-10 |
| `bshl` | `fn uint64.bshl(a: UInt64, n: Int) -> UInt64` | 新規 | R-sized-10 |
| `bshr` | `fn uint64.bshr(a: UInt64, n: Int) -> UInt64` | 新規 | R-sized-10 |
| `bxor` | `fn uint64.bxor(a: UInt64, b: UInt64) -> UInt64` | 新規 | R-sized-10 |
| `byte_swap` | `fn uint64.byte_swap(x: UInt64) -> UInt64` | 新規 | R-sized-10 |
| `clamp` | `fn uint64.clamp(x: UInt64, lo: UInt64, hi: UInt64) -> UInt64` | 新規 | R-sized-08 |
| `compare` | `fn uint64.compare(a: UInt64, b: UInt64) -> Int` | 新規 | R-sized-02 |
| `count_leading_zeros` | `fn uint64.count_leading_zeros(x: UInt64) -> Int` | 新規 | R-sized-10 |
| `count_ones` | `fn uint64.count_ones(x: UInt64) -> Int` | 新規 | R-sized-10 |
| `count_trailing_zeros` | `fn uint64.count_trailing_zeros(x: UInt64) -> Int` | 新規 | R-sized-10 |
| `floor_div` | `fn uint64.floor_div(a: UInt64, b: UInt64) -> UInt64` | 新規 | R-sized-10 |
| `from` | `fn uint64.from[S: Numeric](x: S) -> UInt64` | 新規 | R-L0-40 |
| `from_checked` | `fn uint64.from_checked[S: Numeric](x: S) -> UInt64?` | 新規 | R-L0-40 |
| `from_saturating` | `fn uint64.from_saturating[S: Numeric](x: S) -> UInt64` | 新規 | R-L0-40 |
| `from_wrapping` | `fn uint64.from_wrapping[S: Integer](x: S) -> UInt64` | 新規 | R-L0-40 |
| `is_even` | `fn uint64.is_even(x: UInt64) -> Bool` | 新規 | R-sized-10 |
| `is_odd` | `fn uint64.is_odd(x: UInt64) -> Bool` | 新規 | R-sized-10 |
| `max` | `fn uint64.max(a: UInt64, b: UInt64) -> UInt64` | 新規 | R-sized-08 |
| `max_value` | `fn uint64.max_value() -> UInt64` |  |  |
| `min` | `fn uint64.min(a: UInt64, b: UInt64) -> UInt64` | 新規 | R-sized-08 |
| `min_value` | `fn uint64.min_value() -> UInt64` |  |  |
| `modulo` | `fn uint64.modulo(a: UInt64, b: UInt64) -> UInt64` | 新規 | R-sized-10 |
| `parse` | `fn uint64.parse(s: String) -> Result[UInt64, String]` | 新規 | R-sized-02 |
| `parse_radix` | `fn uint64.parse_radix(s: String, radix: Int) -> Result[UInt64, String]` | 新規 | R-sized-06 |
| `pow` | `fn uint64.pow(base: UInt64, exp: Int) -> UInt64` | 新規 | R-sized-10 |
| `rotate_left` | `fn uint64.rotate_left(a: UInt64, n: Int) -> UInt64` | 新規 | R-int-12 |
| `rotate_right` | `fn uint64.rotate_right(a: UInt64, n: Int) -> UInt64` | 新規 | R-int-12 |
| `to_string` | `fn uint64.to_string(x: UInt64) -> String` |  |  |
| `to_string_radix` | `fn uint64.to_string_radix(x: UInt64, radix: Int) -> String` | 新規 | R-sized-06 |
| `wrap_add` | `fn uint64.wrap_add(a: UInt64, b: UInt64) -> UInt64` | 新規 | R-int-12 |
| `wrap_mul` | `fn uint64.wrap_mul(a: UInt64, b: UInt64) -> UInt64` | 新規 | R-int-12 |
| `wrap_sub` | `fn uint64.wrap_sub(a: UInt64, b: UInt64) -> UInt64` | 新規 | R-int-12 |

撤去（23）:

| 関数 | 代わり | 案 |
|---|---|---|
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int16_checked` | `int16.from_checked` | R-L0-40 |
| `to_int16_saturating` | `int16.from_saturating` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int32_checked` | `int32.from_checked` | R-L0-40 |
| `to_int32_saturating` | `int32.from_saturating` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int64_checked` | `int64.from_checked` | R-L0-40 |
| `to_int64_saturating` | `int64.from_saturating` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_int8_checked` | `int8.from_checked` | R-L0-40 |
| `to_int8_saturating` | `int8.from_saturating` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint16_checked` | `uint16.from_checked` | R-L0-40 |
| `to_uint16_saturating` | `uint16.from_saturating` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint32_checked` | `uint32.from_checked` | R-L0-40 |
| `to_uint32_saturating` | `uint32.from_saturating` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |
| `to_uint8_checked` | `uint8.from_checked` | R-L0-40 |
| `to_uint8_saturating` | `uint8.from_saturating` | R-L0-40 |

## uint8

記録: [L2-sized-numbers.md](L2-sized-numbers.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `band` | `fn uint8.band(a: UInt8, b: UInt8) -> UInt8` | 新規 | R-sized-10 |
| `bit_length` | `fn uint8.bit_length(x: UInt8) -> Int` | 新規 | R-int-11 |
| `bnot` | `fn uint8.bnot(x: UInt8) -> UInt8` | 新規 | R-sized-10 |
| `bor` | `fn uint8.bor(a: UInt8, b: UInt8) -> UInt8` | 新規 | R-sized-10 |
| `bshl` | `fn uint8.bshl(a: UInt8, n: Int) -> UInt8` | 新規 | R-sized-10 |
| `bshr` | `fn uint8.bshr(a: UInt8, n: Int) -> UInt8` | 新規 | R-sized-10 |
| `bxor` | `fn uint8.bxor(a: UInt8, b: UInt8) -> UInt8` | 新規 | R-sized-10 |
| `byte_swap` | `fn uint8.byte_swap(x: UInt8) -> UInt8` | 新規 | R-sized-10 |
| `clamp` | `fn uint8.clamp(x: UInt8, lo: UInt8, hi: UInt8) -> UInt8` | 新規 | R-sized-08 |
| `compare` | `fn uint8.compare(a: UInt8, b: UInt8) -> Int` | 新規 | R-sized-02 |
| `count_leading_zeros` | `fn uint8.count_leading_zeros(x: UInt8) -> Int` | 新規 | R-sized-10 |
| `count_ones` | `fn uint8.count_ones(x: UInt8) -> Int` | 新規 | R-sized-10 |
| `count_trailing_zeros` | `fn uint8.count_trailing_zeros(x: UInt8) -> Int` | 新規 | R-sized-10 |
| `floor_div` | `fn uint8.floor_div(a: UInt8, b: UInt8) -> UInt8` | 新規 | R-sized-10 |
| `from` | `fn uint8.from[S: Numeric](x: S) -> UInt8` | 新規 | R-L0-40 |
| `from_checked` | `fn uint8.from_checked[S: Numeric](x: S) -> UInt8?` | 新規 | R-L0-40 |
| `from_saturating` | `fn uint8.from_saturating[S: Numeric](x: S) -> UInt8` | 新規 | R-L0-40 |
| `from_wrapping` | `fn uint8.from_wrapping[S: Integer](x: S) -> UInt8` | 新規 | R-L0-40 |
| `is_even` | `fn uint8.is_even(x: UInt8) -> Bool` | 新規 | R-sized-10 |
| `is_odd` | `fn uint8.is_odd(x: UInt8) -> Bool` | 新規 | R-sized-10 |
| `max` | `fn uint8.max(a: UInt8, b: UInt8) -> UInt8` | 新規 | R-sized-08 |
| `max_value` | `fn uint8.max_value() -> UInt8` |  |  |
| `min` | `fn uint8.min(a: UInt8, b: UInt8) -> UInt8` | 新規 | R-sized-08 |
| `min_value` | `fn uint8.min_value() -> UInt8` |  |  |
| `modulo` | `fn uint8.modulo(a: UInt8, b: UInt8) -> UInt8` | 新規 | R-sized-10 |
| `parse` | `fn uint8.parse(s: String) -> Result[UInt8, String]` | 新規 | R-sized-02 |
| `parse_radix` | `fn uint8.parse_radix(s: String, radix: Int) -> Result[UInt8, String]` | 新規 | R-sized-06 |
| `pow` | `fn uint8.pow(base: UInt8, exp: Int) -> UInt8` | 新規 | R-sized-10 |
| `rotate_left` | `fn uint8.rotate_left(a: UInt8, n: Int) -> UInt8` | 新規 | R-int-12 |
| `rotate_right` | `fn uint8.rotate_right(a: UInt8, n: Int) -> UInt8` | 新規 | R-int-12 |
| `to_string` | `fn uint8.to_string(x: UInt8) -> String` |  |  |
| `to_string_radix` | `fn uint8.to_string_radix(x: UInt8, radix: Int) -> String` | 新規 | R-sized-06 |
| `wrap_add` | `fn uint8.wrap_add(a: UInt8, b: UInt8) -> UInt8` | 新規 | R-int-12 |
| `wrap_mul` | `fn uint8.wrap_mul(a: UInt8, b: UInt8) -> UInt8` | 新規 | R-int-12 |
| `wrap_sub` | `fn uint8.wrap_sub(a: UInt8, b: UInt8) -> UInt8` | 新規 | R-int-12 |

撤去（11）:

| 関数 | 代わり | 案 |
|---|---|---|
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_int8_checked` | `int8.from_checked` | R-L0-40 |
| `to_int8_saturating` | `int8.from_saturating` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |

## unix

新しいモジュールです（L1 で追加）。
記録: [L2-unix.md](L2-unix.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `accept` | `effect fn unix.accept(listener: Fd) -> Fd` | 新規。移動 | R-unix-01 |
| `connect` | `effect fn unix.connect(path: String) -> Fd` | 新規。`net.unix_connect` から移動 | R-unix-01 |
| `listen` | `effect fn unix.listen(path: String) -> Fd` | 新規。移動 | R-unix-01 |
| `select` | `effect fn unix.select(xs: List[Fd], timeout_ms: Int) -> List[Fd]` | 新規。移動。`unix_poll` を吸収 | R-unix-01、R-unix-02 |
| `shm_create` | `effect fn unix.shm_create(size: Int) -> Fd` | 新規。移動 | R-unix-01 |
| `shm_read` | `effect fn unix.shm_read(fd: Fd, offset: Int, len: Int) -> Bytes` | 新規。移動 | R-unix-01 |
| `shm_resize` | `effect fn unix.shm_resize(fd: Fd, size: Int) -> Unit` | 新規。移動 | R-unix-01 |
| `shm_write` | `effect fn unix.shm_write(fd: Fd, offset: Int, data: Bytes) -> Unit` | 新規。移動 | R-unix-01 |
| `take_fds` | `effect fn unix.take_fds(sock: Fd) -> List[Fd]` | 新規。移動 | R-unix-01 |
| `write_fds` | `effect fn unix.write_fds(sock: Fd, data: Bytes, fds: List[Fd]) -> Unit` | 新規。移動 | R-unix-01、R-unix-04 |

## url

記録: [L2-url.md](L2-url.md)

| 関数 | シグネチャ（リデザイン後） | 変化 | 案 |
|---|---|---|---|
| `build_query` | `fn url.build_query(xs: List[(String, String)]) -> String` |  | R-url-02 |
| `decode_component` | `fn url.decode_component(s: String) -> Result[String, String]` |  | R-url-01 |
| `decode_form` | `fn url.decode_form(s: String) -> Result[String, String]` | 新規。`http.url_decode` の置き換え先 | R-url-01 |
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
| `array` | `fn value.array(xs: List[Value]) -> Value` |  | R-value-02 |
| `as_array` | `fn value.as_array(v: Value) -> Result[List[Value], String]` |  | R-value-02 |
| `as_bool` | `fn value.as_bool(v: Value) -> Result[Bool, String]` |  | R-value-02 |
| `as_float` | `fn value.as_float(v: Value) -> Result[Float, String]` |  | R-value-02 |
| `as_int` | `fn value.as_int(v: Value) -> Result[Int, String]` |  | R-value-02 |
| `as_object` | `fn value.as_object(v: Value) -> Result[List[(String, Value)], String]` | 新規 | R-value-02 |
| `as_string` | `fn value.as_string(v: Value) -> Result[String, String]` |  | R-value-02 |
| `bool` | `fn value.bool(b: Bool) -> Value` |  | R-value-02 |
| `float` | `fn value.float(f: Float) -> Value` |  | R-value-02 |
| `get` | `fn value.get(v: Value, key: String) -> Value?` | 新規 | R-value-07 |
| `get_array` | `fn value.get_array(v: Value, key: String) -> List[Value]?` | 新規。`json.get_array` から移す | R-value-02 |
| `get_bool` | `fn value.get_bool(v: Value, key: String) -> Bool?` | 新規。`json.get_bool` から移す | R-value-02 |
| `get_float` | `fn value.get_float(v: Value, key: String) -> Float?` | 新規。`json.get_float` から移す | R-value-02 |
| `get_int` | `fn value.get_int(v: Value, key: String) -> Int?` | 新規。`json.get_int` から移す | R-value-02 |
| `get_object` | `fn value.get_object(v: Value, key: String) -> List[(String, Value)]?` | 新規 | R-value-02 |
| `get_path` | `fn value.get_path(v: Value, p: Path) -> Value?` | 新規。`json.get_path` から移す | R-value-03 |
| `get_string` | `fn value.get_string(v: Value, key: String) -> String?` | 新規。`json.get_string` から移す | R-value-02 |
| `index` | `fn value.index(p: Path, i: Int) -> Path` | 新規。`json.index` から移す。負は範囲外 | R-value-03 |
| `int` | `fn value.int(n: Int) -> Value` |  | R-value-02 |
| `key` | `fn value.key(p: Path, name: String) -> Path` | 新規。`json.field` から移す | R-value-03 |
| `merge` | `fn value.merge(a: Value, b: Value) -> Value` |  |  |
| `null` | `fn value.null() -> Value` |  |  |
| `object` | `fn value.object(xs: List[(String, Value)]) -> Value` |  | R-value-02 |
| `omit` | `fn value.omit(v: Value, keys: List[String]) -> Value` |  |  |
| `pick` | `fn value.pick(v: Value, keys: List[String]) -> Value` |  |  |
| `remove_path` | `fn value.remove_path(v: Value, p: Path) -> Value` | 新規。`json.remove_path` から移す | R-value-03 |
| `root` | `fn value.root() -> Path` | 新規。`json.root` から移す | R-value-03 |
| `set_path` | `fn value.set_path(v: Value, p: Path, x: Value) -> Result[Value, String]` | 新規。`json.set_path` から移す | R-value-03 |
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
| `compress` | `effect fn zlib.compress(b: Bytes) -> Bytes` |  | R-zlib-03 |
| `compress_level` | `effect fn zlib.compress_level(b: Bytes, level: Int) -> Bytes` |  | R-zlib-03 |
| `decompress` | `fn zlib.decompress(b: Bytes) -> Result[Bytes, String]` | 意味変更: effect を外し、`fn … -> Result[Bytes, String]` にする。出力が 256 MiB を超えたら err | R-zlib-02、R-zlib-04 |
| `decompress_max` | `fn zlib.decompress_max(b: Bytes, max_len: Int) -> Result[Bytes, String]` | 新規 | R-zlib-04 |
| `deflate` | `effect fn zlib.deflate(b: Bytes) -> Bytes` |  | R-zlib-03 |
| `deflate_level` | `effect fn zlib.deflate_level(b: Bytes, level: Int) -> Bytes` |  | R-zlib-03 |
| `gunzip` | `fn zlib.gunzip(b: Bytes) -> Result[Bytes, String]` | 意味変更: effect を外し、`fn … -> Result[Bytes, String]` にする。出力が 256 MiB を超えたら err | R-zlib-02、R-zlib-04 |
| `gunzip_max` | `fn zlib.gunzip_max(b: Bytes, max_len: Int) -> Result[Bytes, String]` | 新規 | R-zlib-04 |
| `gzip` | `effect fn zlib.gzip(b: Bytes) -> Bytes` |  | R-zlib-03 |
| `inflate` | `fn zlib.inflate(b: Bytes) -> Result[Bytes, String]` | 意味変更: effect を外し、`fn … -> Result[Bytes, String]` にする。出力が 256 MiB を超えたら err | R-zlib-02、R-zlib-04 |
| `inflate_max` | `fn zlib.inflate_max(b: Bytes, max_len: Int) -> Result[Bytes, String]` | 新規 | R-zlib-04 |

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


## 採らなかった案・保留の案と理由

レビューで同じ指摘を繰り返さないよう、いまも有効な × と保留の案（54 件）を理由の最初の 1〜2 文と一緒に載せます。全文は各ページの案にあります。

| 案 | 内容 | 判定 | 理由 |
|---|---|---|---|
| [R-L0-18](L0-conventions.md#r-l0-18-型を変換する演算子-as-は足さない---不採用) | 型を変換する演算子 `as` は足さない | × 不採用 | 数の変換は R-L0-08 の名前つき関数があり、R-L0-01（1 つの役割に綴りは 1 つ）に反します。ほかの 3 つの用途は、Almide の型の作りでは要らないか、目的に反します。 |
| [R-L1-20](L1-categories.md#r-l1-20-固定幅の数のモジュールint8uint64float32float64---統合しない) | 固定幅の数のモジュール（`int8`〜`uint64`、`float32`、`float64`） | × 統合しない | (a) 型ごとにモジュールを持つ言語は 0 です。しかし多数派は、型にメソッドを付けています。 |
| [R-L1-21](L1-categories.md#r-l1-21-compute-と-duration---統合しないadr-0001) | `compute` と `duration` | × 統合しない（ADR-0001） | ADR-0001 が、決定的な計算時間（`Compute`）と実時間（`Duration`）を、互いに変換できない別の型と決めています。 |
| [R-L1-23](L1-categories.md#r-l1-23-hex-と-base64-を別のモジュールのまま残す---まとめない) | `hex` と `base64` を別のモジュールのまま残す | × まとめない | 5 言語は 1 か所にまとめていますが、まとめたモジュールの名前が揃っていません（Elixir は `Base`、Deno は `@std/encoding`、Python は `base64` の中の `b16encode`）。 |
| [R-L1-29](L1-categories.md#r-l1-29-html-と-testing---動かさない) | `html` と `testing` | × 動かさない | `html` は 5 関数です。「エスケープ済みの HTML を別の型にする」設計（SafeHtml）は、Go の `html/template` と同じ考えで、取り違えを型で防ぎます。 |
| [R-base64-02](L2-base64.md#r-base64-02--を付けない符号化を足す---不採用) | `=` を付けない符号化を足す | × 不採用 | 5 言語が持ちますが、どれも引数か設定（`padding: false`、`omitPadding`）で、別の名前ではありません。 |
| [R-base64-03](L2-base64.md#r-base64-03-hex-とモジュールをまとめる---不採用r-l1-23) | hex とモジュールをまとめる | × 不採用（R-L1-23） | R-L1-23 のとおりです。まとめたモジュールの名前に多数派がありません。 |
| [R-bigint-02](L2-bigint.md#r-bigint-02-123n-のような-bigint-のリテラルを足す---不採用) | `123n` のような BigInt のリテラルを足す | × 不採用 | 足しません。`bigint.parse("123…")!` と `int.to_bigint(123)` で書きます。 |
| [R-bigint-03](L2-bigint.md#r-bigint-03-int-と-bigint-を混ぜた演算を自動で広げる---不採用) | `Int` と `BigInt` を混ぜた演算を自動で広げる | × 不採用 | しません。`a + b` で片方が `Int`、片方が `BigInt` なら型エラーにし、診断のヒントで `int.to_bigint(a)` を示します。 |
| [R-bytes-21](L2-bytes.md#r-bytes-21-bytesskip-を-next_-の形にする---不採用) | `bytes.skip` を `next_` の形にする | × 不採用 | `skip` は値を読まずに位置だけを進めるので、`(値, 次の状態)` の値がありません。`(Unit, Int)` を返す形は、読む側が捨てる値を増やすだけです。 |
| [R-bytes-24](L2-bytes.md#r-bytes-24-bytesset-と-put_u8-を-1-つにする---不採用) | `bytes.set` と `put_u8` を 1 つにする | × 不採用 | `set(b, i, v) -> Bytes` はコピーを返す関数、`put_u8(mut b, pos, v)` はその場で書き換える関数で、R-L0-04（コピーが既定、その場書き換えは決まった動詞）の対です。 |
| [R-compute-02](L2-compute.md#r-compute-02-duration-と統合するまたは読み出し関数を足す---不採用) | `duration` と統合する、または読み出し関数を足す | × 不採用 | どちらもしません。ADR-0001 の決定（2 つの時計は互いに変換できない別の型、読み出しは S7 で省略）と、L1 R-L1-21 のとおりです。 |
| [R-datetime-02](L2-datetime.md#r-datetime-02-時刻専用の型instant-などを作る---不採用) | 時刻専用の型（`Instant` など）を作る | × 不採用 | R-L0-00。R-L0-14 の 4 により、旧い書き方が型エラーになる変更なら同じ名前のまま型を変えられるようになりました。 |
| [R-datetime-04](L2-datetime.md#r-datetime-04-weekday-を数を返す関数にする---不採用) | `weekday` を数を返す関数にする | × 不採用 | R-L0-00。戻り型を `Int` に変えると旧い書き方（`== "Monday"`）は型エラーになるので、R-L0-14 の 4 では許される変更です。 |
| [R-datetime-05](L2-datetime.md#r-datetime-05-format-の--を--にする---不採用) | `format` の `%%` を `%` にする | × 不採用 | R-L0-14 の 4（同じ名前のまま、旧い書き方が型の検査を通って別の結果になる変更はしない。`%%` の出力を変えると、まさにこの形になる）。 |
| [R-duration-02](L2-duration.md#r-duration-02-読み出し関数小数の構築子日の単位を足す---不採用) | 読み出し関数・小数の構築子・日の単位を足す | × 不採用 | 足しません。ADR-0001 S7 の台帳のとおりで、2026-10-16 の裁定（読み出し関数を足す案の撤回）とも同じです。 |
| [R-duration-04](L2-duration.md#r-duration-04-min分を-minutes-などの完全な綴りにする---不採用) | `min`（分）を `minutes` などの完全な綴りにする | × 不採用 | 6 つの単位の綴りは ADR-0001 S2 が決めています。`duration.min(a, b)` と最小値のつもりで書くと引数の数が合わず E004 になるので、黙って違う値にはなりません。 |
| [R-error-02](L2-error.md#r-error-02-errorcontext-を-resultcontext-として残す---不採用) | `error.context` を `result.context` として残す | × 不採用 | R-L0-01。ADR-0004 Alternatives 7 が `result.context` をこの理由で却下済みです（#1104 は却下でクローズ）。 |
| [R-float-06](L2-float.md#r-float-06-floatminmax-の-nan-の扱いを-listminmax-とそろえる--保留) | `float.min`/`max` の NaN の扱いを `list.min`/`max` とそろえる | 保留 | NaN を伝える言語（Go・JS・Kotlin・OCaml）と無視する言語（Rust・Swift）のどちらも 5 言語に届かず、契約 C-049・C-306 で決めた動きを変える話です。 |
| [R-float-08](L2-float.md#r-float-08-float-の床方向の剰余-floatmod-を足す---不採用) | Float の床方向の剰余 `float.mod` を足す | × 不採用 | 床方向の剰余を Float で持つのは Python の `%`、Kotlin の `mod`、Gleam の `float.modulo` の 3 言語で、5 言語の基準に届きません。 |
| [R-fs-04](L2-fs.md#r-fs-04-59-以上が持つが-almide-に無い役割---不採用いまは足さない) | 5/9 以上が持つが Almide に無い役割 | × 不採用（いまは足さない） | どの役割も、いまの関数で足りるか（`mkdir` は `mkdir_p`、ハンドルは行を流す族と `append`）、WASI に経路がないか（chmod）、要望がまだありません（役割ごとの理由は下の各行）。 |
| [R-heap-04](L2-heap.md#r-heap-04-new_minnew_max-に分ける---不採用) | `new_min`・`new_max` に分ける | × 不採用 | 既定は「最小が先」で、Python・Go・OCaml・Kotlin と同じです（R-L1-02）。最大を先にしたいときは `heap.new_with((a, b) => int.compare(b, a))`（差の `b - a` は `Int` の端で溢れるので使いません） か `new_by` で書きます。 |
| [R-hex-02](L2-hex.md#r-hex-02-decode_upper-を足す---不採用) | `decode_upper` を足す | × 不採用 | 大文字・小文字を区別して拒む復号を標準の既定にしているのは Python の `b16decode` だけです(F)。規則 2 のとおり、書き方の違いは `decode` が受け取ります。 |
| [R-html-02](L2-html.md#r-html-02-unescape-を足す---不採用) | `unescape` を足す | × 不採用 | 標準に持つのは Go・Python・Deno の 3 言語で、L1 の基準（5 言語以上）に届きません。HTML を読む処理は package（HTML の構文解析器）の関心事です。 |
| [R-html-03](L2-html.md#r-html-03-safehtml-の列をまとめる-htmljoin-を足す---不採用) | `SafeHtml` の列をまとめる `html.join` を足す | × 不採用 | エスケープの型を持つ言語が 2 つしかなく、5 言語の基準に届きません。`list.fold(xs, html.empty(), html.concat)` で書けます。 |
| [R-int-05](L2-int.md#r-int-05-bandborbxor-などを多数派の綴りに改名する---不採用改名しないr-int-10-で確かめた) | `band`/`bor`/`bxor` などを多数派の綴りに改名する | × 不採用（改名しない。R-int-10 で確かめた） | R-L0-00。関数名でビット演算を書く 4 言語の綴りは割れています（Gleam `bitwise_and`、Elixir `band`、OCaml `logand`、Kotlin `and`、すべて (F)）。 |
| [R-list-12](L2-list.md#r-list-12-多数派に届かない役割は足さない---不採用) | 多数派に届かない役割は足さない | × 不採用 | L1 の「カテゴリを足す基準」と同じ 5/9 の線、R-L0-01。 |
| [R-list-18](L2-list.md#r-list-18-listrepeat-を列全体の繰り返しにする---不採用) | `list.repeat` を列全体の繰り返しにする | × 不採用 | `string.repeat` と `bytes.repeat` は列全体を繰り返し、`list.repeat(v, n)` は 1 つの値を n 個並べます。 |
| [R-map-04](L2-map.md#r-map-04-mapmap-を-map_values-に改名するまたはコールバックを-k-v-にする--保留六巡目に--から変更dojo-で測る) | `map.map` を `map_values` に改名する、またはコールバックを `(K, V)` にする | 保留（六巡目に × から変更。Dojo で測る） | 以前の理由は「間違えても型エラーになる」でしたが、R-L0-36 のとおり、それだけでは不採用の理由になりません。候補は 3 つ（いまの `map.map` の `(V)`、`map.map` を `(K, V)` にする、`map_values` に改名する）で、他言語も割れています（Swift・Kotlin `mapValues`、OCaml `map`（値）と `mapi`（キーと値）、Gleam `map_values`（キーと値））。 |
| [R-map-05](L2-map.md#r-map-05-取り出して消す-pop合わせ方つきの-merge_withキーを変換する-map_keys-を足す---不採用) | 取り出して消す `pop`、合わせ方つきの `merge_with`、キーを変換する `map_keys` を足す | × 不採用 | R-L0-04（`pop` はその場で末尾から取る動詞）、L1 の追加基準（5/9 以上）、R-L0-00。 |
| [R-option-03](L2-option.md#r-option-03-抜けている役割の関数を足さない---不採用) | 抜けている役割の関数を足さない | × 不採用 | R-L0-00、R-L0-01、R-L0-06。 |
| [R-random-04](L2-random.md#r-random-04-randomint-の-max--min-を-abort-にする---不採用) | `random.int` の `max < min` を abort にする | × 不採用 | 変えません。`max <= min` で `min` を返す規則は、失敗を隠す値ではなく、範囲が 1 点に潰れたときの決まった答えで、R-L0-06 の 4（失敗しうる構築・変換は番兵を返さない）の対象でもありません。 |
| [R-random-06](L2-random.md#r-random-06-randomintmin-max-の上端を含むことを変える---不採用文書だけ) | `random.int(min, max)` の上端を含むことを変える | × 不採用（文書だけ） | 上端を含まない言語が多い（Go・Kotlin・OCaml・Gleam・Rust）ものの、同じ名前で意味を変えることは R-L0-14.4 が許しません。 |
| [R-regex-03](L2-regex.md#r-regex-03-パイプの取り違えを宣言の印で止める---不採用r-regex-02-の型で止まるため) | パイプの取り違えを、宣言の印で止める | × 不採用（R-regex-02 の型で止まるため） | 印を付けても、取り違えた呼び出しがコンパイルエラーになる点は型で分けた場合と同じで、生存率は変わりません。そのために新しい言語の仕組みを足す理由がありません。 |
| [R-regex-06](L2-regex.md#r-regex-06-名前つきグループを足さない---不採用) | 名前つきグループを足さない | × 不採用 | `(?<name>...)` と、名前で取り出す API は足しません。 |
| [R-regex-09](L2-regex.md#r-regex-09-対象の文字列を第-1-引数にする---不採用) | 対象の文字列を第 1 引数にする | × 不採用 | `regex` のモジュールの主語は `Regex` で、R-L0-02・R-L0-03（関数は主語の型のモジュールに置き、主語を先頭にする）のとおりです。 |
| [R-result-02](L2-result.md#r-result-02-resultfilter-の-3-引数を据え置く---不採用撤去も改名もしない) | `result.filter` の 3 引数を据え置く | × 不採用（撤去も改名もしない） | R-L0-00、R-L0-14 の 4（意味の付け替え禁止）。 |
| [R-result-03](L2-result.md#r-result-03-resultto_err_option-を改名しない---不採用) | `result.to_err_option` を改名しない | × 不採用 | R-L0-00、R-L0-07。 |
| [R-result-04](L2-result.md#r-result-04-抜けている役割の関数を足さない---不採用) | 抜けている役割の関数を足さない | × 不採用 | R-L0-00、R-L0-01、R-L0-06。 |
| [R-set-02](L2-set.md#r-set-02-setis_superset-を足す---不採用) | `set.is_superset` を足す | × 不採用 | R-L0-01（引数を入れ替えただけの同じ判定に綴りを 2 つ置かない）、L1 の追加基準（5/9 以上）。 |
| [R-set-03](L2-set.md#r-set-03-setpartition-を足す---不採用) | `set.partition` を足す | × 不採用 | L1 の追加基準（5/9 以上）、完全性の規則 1（family の 7 つに `partition` は入っていません）。 |
| [R-set-04](L2-set.md#r-set-04-setinsert-を-setadd-に改名する---不採用) | `set.insert` を `set.add` に改名する | × 不採用 | R-L0-00（多数派がなければ動かさない）、R-L0-04（`insert` はコピー側の動詞として全モジュールで揃えます）。 |
| [R-sized-04](L2-sized-numbers.md#r-sized-04-floatto_整数_saturating-8-関数を撤去する---取り消しr-l0-32) | `float.to_<整数>_saturating` 8 関数を撤去する | × 取り消し（R-L0-32） | R-L0-32 で無印の `float.to_int8` などが範囲外で abort になるので、`_saturating` は同義語でなくなり、いまの飽和の動きを持つ唯一のセルになります。 |
| [R-string-08](L2-string.md#r-string-08-stringsplit_whitespace-を足す---不採用) | `string.split_whitespace` を足す | × 不採用 | 専用の関数を持つのは 4/9（Rust (F)、Go `Fields` (F)、Python `split()` (F)、Elixir `split/1` (F)）で、L1 の「5 言語以上」に届きません。 |
| [R-string-10](L2-string.md#r-string-10-指定文字の-trimsplitnコードポイントのリスト化を足す---不採用) | 指定文字の trim、`splitn`、コードポイントのリスト化を足す | × 不採用 | R-L0-00、L1 の判定基準。 |
| [R-string-13](L2-string.md#r-string-13-stringcount-を述語で数える形にする---不採用) | `string.count` を述語で数える形にする | × 不採用 | 文字列の「数える」は、部分文字列の出現数が多数派です（Rust `matches().count()`・Go `strings.Count`・Python `str.count`・Elixir `String.count`…）。 |
| [R-url-04](L2-url.md#r-url-04-url-の欄を-option-にする---不採用) | `Url` の欄を Option にする | × 不採用 | `query` と `fragment` の「無い」と「空」を区別したい場面は少なく、欄の型を変えると、すべての `u.query` の読みが壊れます（R-L0-14 の 4）。 |
| [R-zlib-03](L2-zlib.md#r-zlib-03-圧縮の出力を両-target-で同じにしeffect-を外す--保留) | 圧縮の出力を両 target で同じにし、`effect` を外す | 保留 | native を host の zlib から self-host の符号化器に替えると、速さと圧縮率が落ちる見込みです。 |
| [R-env-04](L2-env.md#r-env-04-待ち時間タイムアウトの引数を-duration-にする---不採用) | 待ち時間・タイムアウトの引数を `Duration` にする | × 不採用 | 9 言語で待ち時間を時間の型で受け取るのは Go（`time.Duration`）・Rust（`Duration`）・Swift（`Duration`）・Kotlin（`delay` の `Duration` 版）の 4 つで、5 言語の基準に届きません。 |
| [R-heap-05](L2-heap.md#r-heap-05-heapfoldheapfind-を足す---不採用) | `heap.fold`・`heap.find` を足す | × 不採用 | 中の並び（配列の順）で回すと、その並びが外に漏れ、実装を変えると結果が変わります。取り出し順（小さい順）で回すと、名前から想像できない並べ替えが隠れます。 |
| [R-result-06](L2-result.md#r-result-06-resultcollect-を戻す---不採用) | `result.collect` を戻す | × 不採用 | ADR-0007 D1 で撤去した関数です。失敗しうる要素の列は、`list.map` の callback に `!` を書く形（最初の err で止まる。 |
| [R-int-08](L2-int.md#r-int-08-intto_hex-を-to_string_radixx-16-にまとめる---不採用) | `int.to_hex` を `to_string_radix(x, 16)` にまとめる | × 不採用 | 2 つは負の数で結果が違い、同義語ではありません。`int.to_hex(-1)` は 64 bit の並びの 16 進（`"ffffffffffffffff"`）、`int.to_string_radix(-1, 16)` は符号つき（`"-1"`）です。 |
| [R-int-09](L2-int.md#r-int-09-intsign-と-floatsignum-の綴りを-1-つにする---不採用) | `int.sign` と `float.signum` の綴りを 1 つにする | × 不採用 | 0 の扱いが違うので、同じ名前にすると R-float-05 で避けた誤りが戻ります。`int.sign(0)`・`bigint.sign(0)` は 0、`float.signum(0.0)` は 1.0（Rust の `signum` と同じ）です。 |
| [R-bytes-26](L2-bytes.md#r-bytes-26-bytesget_u8-と-getappend_u8-と-push-を-1-つにする---不採用役割の違いを明記) | `bytes.get_u8` と `get`、`append_u8` と `push` を 1 つにする | × 不採用（役割の違いを明記） | 2 つは結果が同じですが、属する族が違います。`get`・`push` は入れ物の族（`list.get`・`deque.push` と同じ形）、`get_u8`・`append_u8` は符号化の族（`get_u16_le`・`append_u16_le` … の 1 バイトのセル）です。 |

## 後の案で判定を変えた案

× や保留にしたあと、後の案で採用や置き換えに変えたもの（11 件）です。いま有効なのは右の列の案です。上の表には載せません。

| 案 | 内容 | もとの判定 | いま有効な案 |
|---|---|---|---|
| [R-L0-20](L0-conventions.md#r-l0-20-critical-プロファイルで黙って折り返す-to_型-を拒む---取り消し-r-l0-32-で解消) | critical プロファイルで、黙って折り返す `to_<型>` を拒む | × 取り消し（→ R-L0-32 で解消） | R-L0-32 |
| [R-L0-21](L0-conventions.md#r-l0-21-無印の-to_型-をはみ出したら失敗する版に変える--保留--r-l0-32r-l0-34-で解消) | 無印の `to_<型>` を、はみ出したら失敗する版に変える | 保留 → R-L0-32・R-L0-34 で解消 | R-L0-32・R-L0-34 |
| [R-hash-04](L2-hash.md#r-hash-04-_hex-のセルの入力を-bytes-に変える---不採用-r-hash-05-で採用に変更) | `_hex` のセルの入力を Bytes に変える | × 不採用（→ R-hash-05 で採用に変更） | R-hash-05 |
| [R-http-07](L2-http.md#r-http-07-応答のヘッダの-get_header-と要求の-req_header-の不揃い---不採用規則を書く-r-http-12-で解消) | 応答のヘッダの `get_header` と要求の `req_header` の不揃い | × 不採用（規則を書く。→ R-http-12 で解消） | R-http-12 |
| [R-io-04](L2-io.md#r-io-04-標準入力をバイト列で全部読む-ioread_all_bytes-を足す---取り消し十三巡目ioread_to_endiostdin-で書く--r-io-05) | 標準入力をバイト列で全部読む `io.read_all_bytes` を足す | × 取り消し（十三巡目。`io.read_to_end(io.stdin())` で書く → R-io-05） | R-io-05 |
| [R-map-03](L2-map.md#r-map-03-mapto_list-を足す---不採用-r-map-07-で採用に変更) | `map.to_list` を足す | × 不採用（→ R-map-07 で採用に変更） | R-map-07 |
| [R-matrix-07](L2-matrix.md#r-matrix-07-mul-を-matmul-に改名し要素ごとの積を足す---不採用-r-matrix-14-で--に置き換え) | `mul` を `matmul` に改名し、要素ごとの積を足す | × 不採用（→ R-matrix-14 で `*` に置き換え） | R-matrix-14 |
| [R-sized-05](L2-sized-numbers.md#r-sized-05-固定幅のモジュールをまとめる算術関数を足す---不採用算術の部分は--r-sized-10-で見直し) | 固定幅のモジュールをまとめる・算術関数を足す | × 不採用（算術の部分は → R-sized-10 で見直し） | R-sized-10 |
| [R-sized-07](L2-sized-numbers.md#r-sized-07-float32float64-に-is_nan-などの判定を足す---不採用-r-sized-08-で見直し) | `float32`・`float64` に `is_nan` などの判定を足す | × 不採用（→ R-sized-08 で見直し） | R-sized-08 |
| [R-value-06](L2-value.md#r-value-06-構築子-str-を-string-に改名する---不採用-r-value-08-で採用に変更) | 構築子 `str` を `string` に改名する | × 不採用（→ R-value-08 で採用に変更） | R-value-08 |
| [R-L0-41](L0-conventions.md#r-l0-41-ビット演算を演算子にし-を関数の合成から外す---取り消し十二巡目記号の意味を入れ替えないため--r-l0-43) | ビット演算を演算子にし、`>>` を関数の合成から外す | × 取り消し（十二巡目。記号の意味を入れ替えないため → R-L0-43） | R-L0-43 |