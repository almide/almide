# 標準ライブラリ

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)



> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/README.md) の非規範的な読者用索引です。元の生成器の出力先ではなく、自動生成・自動同期されません。固定時点の署名・数・順序を保持し、各モジュールの読者訳にある API 説明を署名で照合して収録しました。そのため、原文索引の先頭一文だけより詳しい説明を含む場合があります。正本は原文です。コード例・実装・安全性を翻訳作業で独立検証したものではありません。各モジュールの原文不整合・ターゲット差・安全上の文脈に関する注記も必ず参照してください。


51 の stdlib モジュールの公開関数、計 1324 関数の署名と説明を掲載します。各モジュールのページに説明全文、例、`@since` リリースがあります。

`mut` パラメーターはその場で書き換えます。呼び出し元は `var` 束縛を渡して変更を受け取ります（`let`、非 `mut` パラメーター、一時値は E032）。ほかのパラメーターはすべて読み取り専用であり、`mut` なしで値を変更する関数は新しい値を返します。

| モジュール | 関数 | 型 |
|---|---:|---:|
| [args](#args) | 3 | 0 |
| [base64](#base64) | 4 | 0 |
| [bigint](#bigint) | 24 | 1 |
| [bool](#bool) | 3 | 0 |
| [bytes](#bytes) | 178 | 0 |
| [compute](#compute) | 7 | 0 |
| [datetime](#datetime) | 18 | 0 |
| [deque](#deque) | 60 | 0 |
| [duration](#duration) | 7 | 0 |
| [env](#env) | 9 | 0 |
| [ffi](#ffi) | 8 | 0 |
| [float](#float) | 27 | 0 |
| [float32](#float32) | 24 | 0 |
| [float64](#float64) | 24 | 0 |
| [fs](#fs) | 37 | 1 |
| [hash](#hash) | 11 | 0 |
| [heap](#heap) | 16 | 0 |
| [hex](#hex) | 3 | 0 |
| [html](#html) | 8 | 0 |
| [http](#http) | 36 | 4 |
| [int](#int) | 47 | 0 |
| [int16](#int16) | 37 | 0 |
| [int32](#int32) | 37 | 0 |
| [int64](#int64) | 37 | 0 |
| [int8](#int8) | 37 | 0 |
| [io](#io) | 12 | 0 |
| [json](#json) | 3 | 0 |
| [list](#list) | 74 | 0 |
| [map](#map) | 37 | 0 |
| [math](#math) | 27 | 0 |
| [matrix](#matrix) | 27 | 0 |
| [net](#net) | 16 | 2 |
| [option](#option) | 14 | 0 |
| [path](#path) | 11 | 0 |
| [process](#process) | 13 | 2 |
| [random](#random) | 10 | 0 |
| [regex](#regex) | 16 | 1 |
| [result](#result) | 15 | 0 |
| [secret](#secret) | 2 | 0 |
| [set](#set) | 42 | 0 |
| [string](#string) | 87 | 0 |
| [testing](#testing) | 10 | 0 |
| [uint16](#uint16) | 35 | 0 |
| [uint32](#uint32) | 35 | 0 |
| [uint64](#uint64) | 35 | 0 |
| [uint8](#uint8) | 35 | 0 |
| [unix](#unix) | 10 | 0 |
| [url](#url) | 9 | 1 |
| [uuid](#uuid) | 7 | 0 |
| [value](#value) | 29 | 0 |
| [zlib](#zlib) | 11 | 0 |

## args

[args.md](args.ja.md)

- `effect args.has_flag(name: String) -> Bool` — `--` より前の --name なら true。1文字名は -name にも一致します。短縮形は自動導出しません。
- `effect args.option(name: String) -> Option[String]` — `--` より前の --name=v または --name v の値。なければ none。
- `effect args.positional(options: List[String]) -> List[String]` — `--` より前の、フラグでも指定オプションの値でもない引数と、`--` より後の全引数。

## base64

[base64.md](base64.ja.md)

- `base64.encode(b: Bytes) -> String` — + / と = パディングを使う標準 base64。
- `base64.decode(s: String) -> Result[Bytes, String]` — パディング付き正準 base64（+ /）からバイト列へ。それ以外は err。
- `base64.encode_url(b: Bytes) -> String` — URL-safe base64（- _）。= パディングを維持。
- `base64.decode_url(s: String) -> Result[Bytes, String]` — パディング付き正準 URL-safe base64（- _）からバイト列へ。それ以外は err。

## bigint

[bigint.md](bigint.ja.md)

- `bigint.parse(s: String) -> Result[BigInt, String]` — 前後空白を除いた十進 s から BigInt。任意の + または - 符号。不正入力や 4300 桁超は err。
- `bigint.parse_radix(s: String, radix: Int) -> Result[BigInt, String]` — 前後空白を除いた基数 radix の s から BigInt。+/- 符号、大小文字可。16/8/2 では 0x/0o/0b 可。不正入力、および二のべき乗でない基数の 4300 桁超は err。radix が 2..36 外なら abort。
- `bigint.to_string(x: BigInt) -> String` — 十進数字。負なら - を付けます。4300 桁超は abort。
- `bigint.to_string_radix(x: BigInt, radix: Int) -> String` — x の radix 進数字、小文字、負なら -。radix が 2..36 外、または二のべき乗でない基数で 4300 桁を超えるテキストになる場合は abort。
- `bigint.compare(a: BigInt, b: BigInt) -> Int` — a が b より小・同じ・大なら -1、0、1。
- `bigint.min(a: BigInt, b: BigInt) -> BigInt` — a と b の小さい方。
- `bigint.max(a: BigInt, b: BigInt) -> BigInt` — a と b の大きい方。
- `bigint.clamp(x: BigInt, lo: BigInt, hi: BigInt) -> BigInt` — x を lo..hi に制限します。lo > hi は abort。
- `bigint.abs(x: BigInt) -> BigInt` — x の大きさ（絶対値）。
- `bigint.sign(x: BigInt) -> Int` — x の符号により -1、0、1。0 には 0。
- `bigint.is_even(x: BigInt) -> Bool` — x が 2 の倍数なら true。
- `bigint.is_odd(x: BigInt) -> Bool` — x が 2 の倍数でなければ true。
- `bigint.floor_div(a: BigInt, b: BigInt) -> BigInt` — -inf 方向に丸めた商。b == 0 は abort。
- `bigint.modulo(a: BigInt, b: BigInt) -> BigInt` — b と同符号の余り。a == floor_div(a, b) * b + modulo(a, b) が成立します。b == 0 は abort。
- `bigint.pow(base: BigInt, exp: Int) -> BigInt` — base の exp 乗。exp < 0、または結果が確実に 30 * 2^28 ビット超となる場合は abort。
- `bigint.bit_length(x: BigInt) -> Int` — x の絶対値を表すためのビット数。0 には 0。
- `bigint.band(a: BigInt, b: BigInt) -> BigInt` — ビット単位 AND。
- `bigint.bor(a: BigInt, b: BigInt) -> BigInt` — ビット単位 OR。
- `bigint.bxor(a: BigInt, b: BigInt) -> BigInt` — ビット単位 XOR。
- `bigint.bnot(x: BigInt) -> BigInt` — ビット単位の補数。-x - 1 に等しい。
- `bigint.bshl(a: BigInt, n: Int) -> BigInt` — a を n ビット左シフト（a * 2^n）。n < 0 は abort。
- `bigint.bshr(a: BigInt, n: Int) -> BigInt` — a を n ビット右シフトし、-inf 方向へ丸めます（符号で充填）。n < 0 は abort。
- `bigint.from(x: S) -> BigInt` — x をゼロ方向へ丸めて BigInt にします。NaN または無限大は abort。
- `bigint.from_checked(x: S) -> Option[BigInt]` — x が整数なら some(x as BigInt)、それ以外（小数、NaN、無限大）は none。

## bool

[bool.md](bool.ja.md)

- `bool.to_string(b: Bool) -> String` — "true" または "false"。bool.parse で読み戻せます。
- `bool.parse(s: String) -> Result[Bool, String]` — "true" なら ok(true)、"false" なら ok(false)、それ以外は err。
- `bool.compare(a: Bool, b: Bool) -> Int` — a < b なら負、等しければ0、a > b なら正。false < true。

## bytes

[bytes.md](bytes.ja.md)

- `bytes.append_f32_be(mut b: Bytes, x: Float) -> Unit` — x を f32 に丸め、BE で追加します。
- `bytes.append_f32_le(mut b: Bytes, x: Float) -> Unit` — x を f32 に丸め、LE で追加します。
- `bytes.append_f64_be(mut b: Bytes, x: Float) -> Unit` — x を 8-byte f64 として BE で追加します。
- `bytes.append_f64_le(mut b: Bytes, x: Float) -> Unit` — x を 8-byte f64 として LE で追加します。
- `bytes.append_i16_be(mut b: Bytes, x: Int) -> Unit` — x の下位16ビットを BE で追加します。
- `bytes.append_i16_le(mut b: Bytes, x: Int) -> Unit` — x の下位16ビットを LE で追加します。
- `bytes.append_i32_be(mut b: Bytes, x: Int) -> Unit` — x の下位32ビットを BE で追加します。
- `bytes.append_i32_le(mut b: Bytes, x: Int) -> Unit` — x の下位32ビットを LE で追加します。
- `bytes.append_i64_be(mut b: Bytes, x: Int) -> Unit` — x を 8 bytes として BE で追加します。
- `bytes.append_i64_le(mut b: Bytes, x: Int) -> Unit` — x を 8 bytes として LE で追加します。
- `bytes.append_u16_be(mut b: Bytes, x: Int) -> Unit` — x の下位16ビットを BE で追加します。
- `bytes.append_u16_le(mut b: Bytes, x: Int) -> Unit` — x の下位16ビットを LE で追加します。
- `bytes.append_u32_be(mut b: Bytes, x: Int) -> Unit` — x の下位32ビットを BE で追加します。
- `bytes.append_u32_le(mut b: Bytes, x: Int) -> Unit` — x の下位32ビットを LE で追加します。
- `bytes.append_u8(mut b: Bytes, x: Int) -> Unit` — x の下位8ビットを追加します。256 は0になります。
- `bytes.clear(mut b: Bytes) -> Unit` — b をその場で長さ0に切り詰めます。
- `bytes.compare(a: Bytes, b: Bytes) -> Int` — 辞書順の -1、0、1。接頭部は先に並びます。
- `bytes.contains(b: Bytes, needle: Bytes) -> Bool` — needle が b に現れるか、needle が空なら true。
- `bytes.copy_from(mut b: Bytes, src: Bytes, dst_off: Int, src_off: Int, len: Int) -> Unit` — src の len バイトを b へコピー。収まるように制限します。
- `bytes.ends_with(b: Bytes, suffix: Bytes) -> Bool` — b が suffix で終わるか、suffix が空なら true。
- `bytes.is_eof(b: Bytes, pos: Int) -> Bool` — pos >= len または pos が負なら true。
- `bytes.fill(mut b: Bytes, x: Int) -> Unit` — 全バイトを x の下位8ビットにその場で設定します。
- `bytes.from_list(xs: List[Int]) -> Bytes` — xs を各値の下位8ビットからなるバイト列にします。-1 は255。
- `bytes.from_string(s: String) -> Bytes` — s の UTF-8 符号化。
- `bytes.get(b: Bytes, i: Int) -> Option[Int]` — 添字 i のバイト。範囲外なら none。
- `bytes.index_of(b: Bytes, needle: Bytes) -> Option[Int]` — needle の最初の位置、または none。空なら some(0)。
- `bytes.insert(b: Bytes, i: Int, x: Int) -> Bytes` — 0..len に制限した i に x の下位バイトを挿入したコピー。
- `bytes.is_empty(b: Bytes) -> Bool` — b にバイトがなければ true。
- `bytes.is_valid_utf8(b: Bytes) -> Bool` — b が有効な UTF-8 かどうか。空なら true。
- `bytes.len(b: Bytes) -> Int` — バイト数。空のバッファなら0。
- `bytes.map(b: Bytes, f: (Int) -> Int) -> Bytes` — 各バイトに f を適用し、結果を下位8ビットに切り詰めます。
- `bytes.zeros(n: Int) -> Bytes` — n 個のゼロバイト。n <= 0 なら空。
- `bytes.pad_start(b: Bytes, n: Int, fill: Int) -> Bytes` — fill で左を埋め長さ n にします。すでに長ければそのまま。
- `bytes.pad_end(b: Bytes, n: Int, fill: Int) -> Bytes` — fill で右を埋め長さ n にします。すでに長ければそのまま。
- `bytes.push(mut b: Bytes, x: Int) -> Unit` — x の下位8ビットをその場で追加します。
- `bytes.copy_within(mut b: Bytes, src_start: Int, src_end: Int, dst: Int) -> Unit` — src_start..src_end を dst へコピー。収まらなければ何もしません。
- `bytes.remove_at(b: Bytes, i: Int) -> Bytes` — pos のバイトを除いたコピー。範囲外ならそのまま。
- `bytes.repeat(b: Bytes, n: Int) -> Bytes` — b を n 回連結。n <= 0 なら空。
- `bytes.reverse(b: Bytes) -> Bytes` — バイト順を逆にしたコピー。
- `bytes.set(b: Bytes, i: Int, x: Int) -> Bytes` — バイト i を x に設定したコピー。範囲外ならそのまま。
- `bytes.skip(b: Bytes, pos: Int, n: Int) -> Int` — pos + n を len で上限にします。
- `bytes.slice(b: Bytes, start: Int, end: Int) -> Bytes` — start..end のコピー。各境界を0..len に制限し、start >= end なら空。
- `bytes.split(b: Bytes, sep: Bytes) -> List[Bytes]` — sep の一致の間にある部分。sep が空なら [b]。
- `bytes.starts_with(b: Bytes, prefix: Bytes) -> Bool` — b が prefix で始まるか、prefix が空なら true。
- `bytes.is_equal_constant_time(a: Bytes, b: Bytes) -> Bool` — a と b が同じバイト列なら true。時間は長さだけに依存し、最初の異なるバイトで早期終了しません。MAC、トークン、パスワードハッシュの比較に使います。`==` は最初の違いで返ります。長さの不一致は即座に false です（長さは公開情報）。
- `bytes.to_list(b: Bytes) -> List[Int]` — 各バイトを0..255の Int として返します。
- `bytes.to_string(b: Bytes) -> Result[String, String]` — UTF-8 デコード。不正なら err "invalid UTF-8: ..."。
- `bytes.to_string_lossy(b: Bytes) -> String` — UTF-8 デコード。不正な列は U+FFFD に置き換えます。
- `bytes.append_bool(mut b: Bytes, x: Bool) -> Unit` — true には1、false には0を追加。
- `bytes.xor(a: Bytes, b: Bytes) -> Bytes` — バイト単位 XOR。短い入力と同じ長さ。
- `bytes.get_u8(b: Bytes, pos: Int) -> Option[Int]` — pos のバイトを0..255で返します。0..len 外なら none。get と同じ値。
- `bytes.next_u8(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_u8(b, pos), pos + 1)。何も読めなければ位置は pos のまま。
- `bytes.put_u8(mut b: Bytes, pos: Int, x: Int) -> Unit` — pos に x の下位8ビットをその場で書き込みます。pos が0..len 外なら中断。
- `bytes.get_i8(b: Bytes, pos: Int) -> Option[Int]` — pos のバイトを符号付き -128..127 で返します。0..len 外なら none。
- `bytes.next_i8(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_i8(b, pos), pos + 1)。何も読めなければ位置は pos のまま。
- `bytes.put_i8(mut b: Bytes, pos: Int, x: Int) -> Unit` — pos に x の下位8ビットをその場で書き込みます。pos が0..len 外なら中断。
- `bytes.append_i8(mut b: Bytes, x: Int) -> Unit` — x の下位8ビットをその場で追加します。-1 は255。
- `bytes.get_bool(b: Bytes, pos: Int) -> Option[Bool]` — pos のバイト != 0。0..len 外なら none。
- `bytes.next_bool(b: Bytes, pos: Int) -> (Option[Bool], Int)` — (get_bool(b, pos), pos + 1)。何も読めなければ位置は pos のまま。
- `bytes.put_bool(mut b: Bytes, pos: Int, x: Bool) -> Unit` — pos に true なら1、false なら0をその場で書き込みます。pos が0..len 外なら中断。
- `bytes.get_u16_le(b: Bytes, pos: Int) -> Option[Int]` — pos の u16 LE。2バイトが収まらなければ none。
- `bytes.next_u16_le(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_u16_le(b, pos), pos + 2)。何も読めなければ位置は pos のまま。
- `bytes.get_u16_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — pos から count 個の u16 LE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_u16_le(mut b: Bytes, pos: Int, x: Int) -> Unit` — x を u16 LE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_u16_be(b: Bytes, pos: Int) -> Option[Int]` — pos の u16 BE。2バイトが収まらなければ none。
- `bytes.next_u16_be(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_u16_be(b, pos), pos + 2)。何も読めなければ位置は pos のまま。
- `bytes.get_u16_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — pos から count 個の u16 BE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_u16_be(mut b: Bytes, pos: Int, x: Int) -> Unit` — x を u16 BE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_i16_le(b: Bytes, pos: Int) -> Option[Int]` — pos の i16 LE。2バイトが収まらなければ none。
- `bytes.next_i16_le(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_i16_le(b, pos), pos + 2)。何も読めなければ位置は pos のまま。
- `bytes.get_i16_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — pos から count 個の i16 LE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_i16_le(mut b: Bytes, pos: Int, x: Int) -> Unit` — x を i16 LE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_i16_be(b: Bytes, pos: Int) -> Option[Int]` — pos の i16 BE。2バイトが収まらなければ none。
- `bytes.next_i16_be(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_i16_be(b, pos), pos + 2)。何も読めなければ位置は pos のまま。
- `bytes.get_i16_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — pos から count 個の i16 BE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_i16_be(mut b: Bytes, pos: Int, x: Int) -> Unit` — x を i16 BE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_u32_le(b: Bytes, pos: Int) -> Option[Int]` — pos の u32 LE。4バイトが収まらなければ none。
- `bytes.next_u32_le(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_u32_le(b, pos), pos + 4)。何も読めなければ位置は pos のまま。
- `bytes.get_u32_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — pos から count 個の u32 LE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_u32_le(mut b: Bytes, pos: Int, x: Int) -> Unit` — x を u32 LE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_u32_be(b: Bytes, pos: Int) -> Option[Int]` — pos の u32 BE。4バイトが収まらなければ none。
- `bytes.next_u32_be(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_u32_be(b, pos), pos + 4)。何も読めなければ位置は pos のまま。
- `bytes.get_u32_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — pos から count 個の u32 BE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_u32_be(mut b: Bytes, pos: Int, x: Int) -> Unit` — x を u32 BE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_i32_le(b: Bytes, pos: Int) -> Option[Int]` — pos の i32 LE。4バイトが収まらなければ none。
- `bytes.next_i32_le(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_i32_le(b, pos), pos + 4)。何も読めなければ位置は pos のまま。
- `bytes.get_i32_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — pos から count 個の i32 LE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_i32_le(mut b: Bytes, pos: Int, x: Int) -> Unit` — x を i32 LE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_i32_be(b: Bytes, pos: Int) -> Option[Int]` — pos の i32 BE。4バイトが収まらなければ none。
- `bytes.next_i32_be(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_i32_be(b, pos), pos + 4)。何も読めなければ位置は pos のまま。
- `bytes.get_i32_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — pos から count 個の i32 BE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_i32_be(mut b: Bytes, pos: Int, x: Int) -> Unit` — x を i32 BE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_i64_le(b: Bytes, pos: Int) -> Option[Int]` — pos の i64 LE。8バイトが収まらなければ none。
- `bytes.next_i64_le(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_i64_le(b, pos), pos + 8)。何も読めなければ位置は pos のまま。
- `bytes.get_i64_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — pos から count 個の i64 LE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_i64_le(mut b: Bytes, pos: Int, x: Int) -> Unit` — x を i64 LE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_i64_be(b: Bytes, pos: Int) -> Option[Int]` — pos の i64 BE。8バイトが収まらなければ none。
- `bytes.next_i64_be(b: Bytes, pos: Int) -> (Option[Int], Int)` — (get_i64_be(b, pos), pos + 8)。何も読めなければ位置は pos のまま。
- `bytes.get_i64_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]` — pos から count 個の i64 BE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_i64_be(mut b: Bytes, pos: Int, x: Int) -> Unit` — x を i64 BE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_f32_le(b: Bytes, pos: Int) -> Option[Float]` — pos の f32 LE。4バイトが収まらなければ none。
- `bytes.next_f32_le(b: Bytes, pos: Int) -> (Option[Float], Int)` — (get_f32_le(b, pos), pos + 4)。何も読めなければ位置は pos のまま。
- `bytes.get_f32_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]` — pos から count 個の f32 LE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_f32_le(mut b: Bytes, pos: Int, x: Float) -> Unit` — x を f32 LE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_f32_be(b: Bytes, pos: Int) -> Option[Float]` — pos の f32 BE。4バイトが収まらなければ none。
- `bytes.next_f32_be(b: Bytes, pos: Int) -> (Option[Float], Int)` — (get_f32_be(b, pos), pos + 4)。何も読めなければ位置は pos のまま。
- `bytes.get_f32_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]` — pos から count 個の f32 BE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_f32_be(mut b: Bytes, pos: Int, x: Float) -> Unit` — x を f32 BE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_f64_le(b: Bytes, pos: Int) -> Option[Float]` — pos の f64 LE。8バイトが収まらなければ none。
- `bytes.next_f64_le(b: Bytes, pos: Int) -> (Option[Float], Int)` — (get_f64_le(b, pos), pos + 8)。何も読めなければ位置は pos のまま。
- `bytes.get_f64_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]` — pos から count 個の f64 LE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_f64_le(mut b: Bytes, pos: Int, x: Float) -> Unit` — x を f64 LE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_f64_be(b: Bytes, pos: Int) -> Option[Float]` — pos の f64 BE。8バイトが収まらなければ none。
- `bytes.next_f64_be(b: Bytes, pos: Int) -> (Option[Float], Int)` — (get_f64_be(b, pos), pos + 8)。何も読めなければ位置は pos のまま。
- `bytes.get_f64_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]` — pos から count 個の f64 BE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_f64_be(mut b: Bytes, pos: Int, x: Float) -> Unit` — x を f64 BE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_f16_le(b: Bytes, pos: Int) -> Option[Float]` — pos の f16 LE を Float として返します。2バイトが収まらなければ none。
- `bytes.next_f16_le(b: Bytes, pos: Int) -> (Option[Float], Int)` — (get_f16_le(b, pos), pos + 2)。何も読めなければ位置は pos のまま。
- `bytes.get_f16_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]` — pos から count 個の f16 LE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.get_f16_be(b: Bytes, pos: Int) -> Option[Float]` — pos の f16 BE を Float として返します。2バイトが収まらなければ none。
- `bytes.next_f16_be(b: Bytes, pos: Int) -> (Option[Float], Int)` — (get_f16_be(b, pos), pos + 2)。何も読めなければ位置は pos のまま。
- `bytes.get_f16_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]` — pos から count 個の f16 BE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_f16_le(mut b: Bytes, pos: Int, x: Float) -> Unit` — x を f16 LE に丸めて pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.put_f16_be(mut b: Bytes, pos: Int, x: Float) -> Unit` — x を f16 BE に丸めて pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.append_f16_le(mut b: Bytes, x: Float) -> Unit` — x を f16 に丸めて追加します（最近接偶数、オーバーフローは inf）。LE。
- `bytes.append_f16_be(mut b: Bytes, x: Float) -> Unit` — x を f16 に丸めて追加します（最近接偶数、オーバーフローは inf）。BE。
- `bytes.next_bytes(b: Bytes, pos: Int, n: Int) -> (Option[Bytes], Int)` — (pos の n バイト, pos + n)。収まらない、または n < 0 の場合は (none, pos)。
- `bytes.append_bytes(mut b: Bytes, src: Bytes) -> Unit` — src のバイト列を b にその場で追加。
- `bytes.new() -> Bytes` — 空のバイトバッファ。
- `bytes.chunk(b: Bytes, n: Int) -> List[Bytes]` — n バイトずつ順に分割。最後は短くなる場合があります。n <= 0 は中断。
- `bytes.pop(mut b: Bytes) -> Option[Int]` — 末尾バイトを削除して返します。b が空なら none。
- `bytes.take(b: Bytes, n: Int) -> Bytes` — 先頭 n バイト。長さを超えれば b 全体、n < 0 なら空。
- `bytes.drop(b: Bytes, n: Int) -> Bytes` — b の先頭 n バイトを除いたもの。長さを超えれば空、n < 0 なら b 全体。
- `bytes.take_end(b: Bytes, n: Int) -> Bytes` — 末尾 n バイト。長さを超えれば b 全体、n < 0 なら空。
- `bytes.drop_end(b: Bytes, n: Int) -> Bytes` — b の末尾 n バイトを除いたもの。長さを超えれば空、n < 0 なら b 全体。
- `bytes.split_at(b: Bytes, n: Int) -> (Bytes, Bytes)` — (先頭 n バイト、残り)。n が末尾を超えれば (b, 空)、n < 0 なら (空, b)。
- `bytes.first(b: Bytes) -> Option[Int]` — 先頭バイト。b が空なら none。
- `bytes.last(b: Bytes) -> Option[Int]` — 末尾バイト。b が空なら none。
- `bytes.filter(b: Bytes, f: (Int) -> Bool) -> Bytes` — f が true になるバイト。
- `bytes.fold(b: Bytes, init: B, f: (B, Int) -> B) -> B` — バイト列に f を左から右へ畳み込みます。
- `bytes.any(b: Bytes, f: (Int) -> Bool) -> Bool` — どれかのバイトで f が成り立てば true。b が空なら false。
- `bytes.all(b: Bytes, f: (Int) -> Bool) -> Bool` — すべてのバイトで f が成り立てば true。b が空なら true。
- `bytes.count(b: Bytes, f: (Int) -> Bool) -> Int` — f を満たすバイト数。
- `bytes.find(b: Bytes, f: (Int) -> Bool) -> Option[Int]` — f を満たす最初のバイト、または none。
- `bytes.find_index(b: Bytes, f: (Int) -> Bool) -> Option[Int]` — f を満たす最初のバイトの添字、または none。
- `bytes.enumerate(b: Bytes) -> List[(Int, Int)]` — 0から始まる (添字、バイト) の組。
- `bytes.zip(b: Bytes, ys: List[B]) -> List[(Int, B)]` — バイトを ys と組にします。短い方で終了。
- `bytes.zip_with(b: Bytes, ys: List[B], f: (Int, B) -> C) -> List[C]` — ys と組にしたバイトに f を適用。短い方で終了。
- `bytes.filter_map(b: Bytes, f: (Int) -> Option[B]) -> List[B]` — 各バイトへの f の結果のうち some を順に並べます。
- `bytes.find_map(b: Bytes, f: (Int) -> Option[B]) -> Option[B]` — 各バイトへの f の最初の some 結果、または none。
- `bytes.reduce(b: Bytes, f: (Int, Int) -> Int) -> Option[Int]` — 最初のバイトから f で畳み込みます。b が空なら none。
- `bytes.scan(b: Bytes, init: B, f: (B, Int) -> B) -> List[B]` — バイト列の逐次畳み込み。init は含みません。
- `bytes.partition(b: Bytes, f: (Int) -> Bool) -> (List[Int], List[Int])` — (f が true のバイト、それ以外)。それぞれ順序を保持。
- `bytes.group_by(b: Bytes, f: (Int) -> B) -> Map[B, List[Int]]` — f のキーでバイトをグループ化。各グループの順序を保持。
- `bytes.sort(b: Bytes) -> List[Int]` — 昇順のバイト列。
- `bytes.sort_by(b: Bytes, f: (Int) -> B) -> List[Int]` — f のキー順のバイト列。同じキーの順序を保持。
- `bytes.sort_with(b: Bytes, cmp: (Int, Int) -> Int) -> List[Int]` — cmp 順のバイト列（負なら第1引数が先）。安定です。
- `bytes.min(b: Bytes) -> Option[Int]` — 最小バイト。空なら none。
- `bytes.max(b: Bytes) -> Option[Int]` — 最大バイト。空なら none。
- `bytes.min_by(b: Bytes, f: (Int) -> K) -> Option[Int]` — 最小キーを持つ最初のバイト。空なら none。
- `bytes.max_by(b: Bytes, f: (Int) -> K) -> Option[Int]` — 最大キーを持つ最後のバイト。空なら none。
- `bytes.min_with(b: Bytes, cmp: (Int, Int) -> Int) -> Option[Int]` — cmp で最小となる最初のバイト。空なら none。
- `bytes.max_with(b: Bytes, cmp: (Int, Int) -> Int) -> Option[Int]` — cmp で最大となる最後のバイト。空なら none。
- `bytes.sum(b: Bytes) -> Int` — バイトの和。b が空なら0。
- `bytes.product(b: Bytes) -> Int` — バイトの積。折り返します。b が空なら1。
- `bytes.take_while(b: Bytes, f: (Int) -> Bool) -> Bytes` — すべてのバイトが f を満たす最長の接頭部。
- `bytes.drop_while(b: Bytes, f: (Int) -> Bool) -> Bytes` — すべてのバイトが f を満たす最長の接頭部を除いた b。
- `bytes.window(b: Bytes, n: Int) -> List[Bytes]` — n 個の連続バイトからなるすべての区間。n > len なら []、n <= 0 は中断。
- `bytes.dedup(b: Bytes) -> Bytes` — 隣接する同値バイトの各連続を1つにまとめた b。
- `bytes.intersperse(b: Bytes, sep: Int) -> Bytes` — b の各隣接バイト間に sep。
- `bytes.unique(b: Bytes) -> Bytes` — 各バイト値の最初の出現を順に保持。
- `bytes.unique_by(b: Bytes, f: (Int) -> K) -> Bytes` — f の各キーに対する最初のバイトを順に保持。
- `bytes.last_index_of(b: Bytes, needle: Bytes) -> Option[Int]` — 最後の needle のバイト添字、または none。空の needle は len で見つかります。
- `bytes.replace(b: Bytes, from: Bytes, to: Bytes) -> Bytes` — b の重ならないすべての from を左から右へ to に置換。
- `bytes.replace_first(b: Bytes, from: Bytes, to: Bytes) -> Bytes` — b の最初の from を to に置換。
- `bytes.strip_prefix(b: Bytes, prefix: Bytes) -> Option[Bytes]` — b から prefix を除いたもの。prefix で始まらなければ none。
- `bytes.strip_suffix(b: Bytes, suffix: Bytes) -> Option[Bytes]` — b から suffix を除いたもの。suffix で終わらなければ none。
- `bytes.count_matches(b: Bytes, needle: Bytes) -> Int` — 重ならない needle の出現数。needle が空なら len + 1。

## compute

[compute.md](compute.ja.md)

- `compute.ns(n: Int) -> Compute` — 決定的な計算時間の n ナノ秒。負の n は abort。
- `compute.us(n: Int) -> Compute` — 決定的な計算時間の n マイクロ秒。負の n は abort。
- `compute.ms(n: Int) -> Compute` — 決定的な計算時間の n ミリ秒。負の n は abort。
- `compute.s(n: Int) -> Compute` — 決定的な計算時間の n 秒。負の n は abort。
- `compute.min(n: Int) -> Compute` — 決定的な計算時間の n 分。負の n は abort。
- `compute.h(n: Int) -> Compute` — 決定的な計算時間の n 時間。負の n は abort。
- `compute.compare(a: Compute, b: Compute) -> Int` — a < b なら負、等しければ 0、a > b なら正。

## datetime

[datetime.md](datetime.ja.md)

- `datetime.add_days(ts: Int, n: Int) -> Int` — ts + n * 86400。夏時間や暦のロジックはありません。
- `datetime.add_hours(ts: Int, n: Int) -> Int` — ts + n * 3600。n は負でも構いません。
- `datetime.add_minutes(ts: Int, n: Int) -> Int` — ts + n * 60。n は負でも構いません。
- `datetime.add_seconds(ts: Int, n: Int) -> Int` — ts + n。n は負でも構いません。
- `datetime.day(ts: Int) -> Int` — 暦上の日1..31（UTC）。
- `datetime.format(ts: Int, pattern: String) -> String` — %Y %m %d %H %M %S を置換します（UTC）。%% エスケープはありません。
- `datetime.from_parts(y: Int, m: Int, d: Int, h: Int, min: Int, s: Int) -> Result[Int, String]` — UTC のエポック秒。parse_iso と同様、範囲外のフィールドは err。
- `datetime.hour(ts: Int) -> Int` — UTC の時0..23。
- `datetime.minute(ts: Int) -> Int` — UTC の分0..59。
- `datetime.month(ts: Int) -> Int` — UTC の月1..12。
- `effect datetime.now() -> Int` — 実時間時計の Unix 秒（単調ではありません）。
- `effect datetime.monotonic_ns() -> Int` — 初回呼び出しからの単調な ns。差分を比較してください。
- `effect datetime.now_ms() -> Int` — 実時間時計の Unix エポックからのミリ秒（単調ではありません）。
- `datetime.parse_iso(s: String) -> Result[Int, String]` — YYYY-MM-DDTHH:MM:SSZ のエポック秒。年0..=9999、小数部分は err。
- `datetime.second(ts: Int) -> Int` — UTC の秒0..59。
- `datetime.to_iso(ts: Int) -> String` — UTC の YYYY-MM-DDTHH:MM:SSZ。
- `datetime.weekday(ts: Int) -> String` — UTC の英語の曜日名。0 は Thursday。
- `datetime.year(ts: Int) -> Int` — UTC の暦年。

## deque

[deque.md](deque.ja.md)

- `deque.new() -> Deque[A]` — 空のキュー。
- `deque.from_list(xs: List[A]) -> Deque[A]` — xs のキュー。先頭から末尾の順です。
- `deque.to_list(d: Deque[A]) -> List[A]` — 先頭から末尾の要素。
- `deque.len(d: Deque[A]) -> Int` — 要素数。空なら0。
- `deque.is_empty(d: Deque[A]) -> Bool` — d に要素がない場合に限り true。
- `deque.push(mut d: Deque[A], x: A) -> Unit` — 末尾に x をその場で追加します。
- `deque.push_front(mut d: Deque[A], x: A) -> Unit` — 先頭に x をその場で追加します。
- `deque.pop(mut d: Deque[A]) -> Option[A]` — 末尾の要素を削除して返します。空なら none。
- `deque.pop_front(mut d: Deque[A]) -> Option[A]` — 先頭の要素を削除して返します。空なら none。
- `deque.first(d: Deque[A]) -> Option[A]` — 先頭の要素。空なら none。
- `deque.last(d: Deque[A]) -> Option[A]` — 末尾の要素。空なら none。
- `deque.get(d: Deque[A], i: Int) -> Option[A]` — 先頭から数えた添字 i の要素。範囲外なら none。
- `deque.contains(d: Deque[A], x: A) -> Bool` — いずれかの要素 == x なら true。
- `deque.clear(mut d: Deque[A]) -> Unit` — d の全要素をその場で削除します。
- `deque.map(d: Deque[A], f: (A) -> B) -> Deque[B]` — 先頭から末尾への f(x) のキュー。f も先頭から末尾の順に実行します。
- `deque.filter(d: Deque[A], f: (A) -> Bool) -> Deque[A]` — f が成り立つ要素。順序は保持します。
- `deque.fold(d: Deque[A], init: B, f: (B, A) -> B) -> B` — f(acc, x) を先頭から末尾へ畳み込みます。
- `deque.any(d: Deque[A], f: (A) -> Bool) -> Bool` — いずれかの要素で f が成り立てば true。空なら false。
- `deque.all(d: Deque[A], f: (A) -> Bool) -> Bool` — すべての要素で f が成り立てば true。空なら true。
- `deque.count(d: Deque[A], f: (A) -> Bool) -> Int` — f が成り立つ要素数。
- `deque.find(d: Deque[A], f: (A) -> Bool) -> Option[A]` — f が成り立つ先頭に最も近い要素、または none。
- `deque.reverse(d: Deque[A]) -> Deque[A]` — 末尾から先頭への要素。
- `deque.take(d: Deque[A], n: Int) -> Deque[A]` — 先頭 n 要素。list.take と同じ。
- `deque.drop(d: Deque[A], n: Int) -> Deque[A]` — 先頭 n 要素を除くすべて。list.drop と同じ。
- `deque.take_end(d: Deque[A], n: Int) -> Deque[A]` — 末尾 n 要素。list.take_end と同じ。
- `deque.drop_end(d: Deque[A], n: Int) -> Deque[A]` — 末尾 n 要素を除くすべて。list.drop_end と同じ。
- `deque.slice(d: Deque[A], start: Int, end: Int) -> Deque[A]` — 添字 start..<end。list.slice と同じ。
- `deque.set(d: Deque[A], i: Int, x: A) -> Deque[A]` — 位置 i = x としたコピー。list.set と同じ。
- `deque.insert(d: Deque[A], i: Int, x: A) -> Deque[A]` — i に x を挿入したコピー。list.insert と同じ。
- `deque.remove_at(d: Deque[A], i: Int) -> Deque[A]` — 位置 i を除いたコピー。list.remove_at と同じ。
- `deque.enumerate(d: Deque[A]) -> List[(Int, A)]` — (添字、要素) の組。添字は先頭の0から。
- `deque.zip(d: Deque[A], ys: List[B]) -> List[(A, B)]` — 位置で ys と組にします。短い方で終了します。
- `deque.zip_with(d: Deque[A], ys: List[B], f: (A, B) -> C) -> List[C]` — (要素、ys の要素) の組に f を適用。短い方で終了します。
- `deque.filter_map(d: Deque[A], f: (A) -> Option[B]) -> List[B]` — f が返す some を先頭から末尾へ並べます。
- `deque.find_map(d: Deque[A], f: (A) -> Option[B]) -> Option[B]` — f が返す最初の some、または none。一致後は f を呼びません。
- `deque.reduce(d: Deque[A], f: (A, A) -> A) -> Option[A]` — 先頭の要素を初期値とした左畳み込み。空なら none。
- `deque.scan(d: Deque[A], init: B, f: (B, A) -> B) -> List[B]` — 先頭から末尾への逐次畳み込み値。init は含みません。
- `deque.partition(d: Deque[A], f: (A) -> Bool) -> (List[A], List[A])` — (f が成り立つ要素、それ以外)。順序は保持します。
- `deque.group_by(d: Deque[A], f: (A) -> B) -> Map[B, List[A]]` — キー f(x) → 要素群。キーは初出順。
- `deque.sort(d: Deque[A]) -> List[A]` — 昇順の要素。Float は IEEE totalOrder。
- `deque.sort_by(d: Deque[A], f: (A) -> B) -> List[A]` — キー f(x) による安定ソート。f は要素ごとに1回実行。
- `deque.sort_with(d: Deque[A], cmp: (A, A) -> Int) -> List[A]` — cmp による安定ソート。負なら a を先に置きます。
- `deque.min(d: Deque[A]) -> Option[A]` — 最小要素。空なら none。
- `deque.max(d: Deque[A]) -> Option[A]` — 最大要素。空なら none。
- `deque.min_by(d: Deque[A], f: (A) -> K) -> Option[A]` — キー f(x) が最小の要素。同点なら先頭に最も近いもの。
- `deque.max_by(d: Deque[A], f: (A) -> K) -> Option[A]` — キー f(x) が最大の要素。同点なら先頭に最も近いもの。
- `deque.min_with(d: Deque[A], cmp: (A, A) -> Int) -> Option[A]` — cmp による最小要素。同点なら先頭に最も近いもの。
- `deque.max_with(d: Deque[A], cmp: (A, A) -> Int) -> Option[A]` — cmp による最大要素。同点なら先頭に最も近いもの。
- `deque.find_index(d: Deque[A], f: (A) -> Bool) -> Option[Int]` — f が成り立つ最初の x の先頭からの添字、または none。
- `deque.take_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]` — f が成り立つ先頭からの最長連続部分。
- `deque.drop_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]` — f が成り立つ先頭からの最長連続部分を除いた残り。
- `deque.chunk(d: Deque[A], n: Int) -> List[Deque[A]]` — 先頭から n 個ずつの連続部分。list.chunk と同じ。
- `deque.window(d: Deque[A], n: Int) -> List[Deque[A]]` — 長さ n のすべての連続部分。list.window と同じ。
- `deque.split_at(d: Deque[A], n: Int) -> (Deque[A], Deque[A])` — (先頭 n 個、残り)。list.split_at と同じ。
- `deque.dedup(d: Deque[A]) -> Deque[A]` — 隣接する同値の連続を1つにまとめます。list.dedup と同じ。
- `deque.intersperse(d: Deque[A], sep: A) -> Deque[A]` — 隣接要素間に sep。
- `deque.unique(d: Deque[A]) -> Deque[A]` — 重複を削除し、先頭に最も近い出現を保持します。
- `deque.unique_by(d: Deque[A], f: (A) -> K) -> Deque[A]` — 異なるキー f(x) ごとの最初の要素。
- `deque.sum(d: Deque[A]) -> A` — 要素の和。空なら0。整数はオーバーフロー時に折り返します。
- `deque.product(d: Deque[A]) -> A` — 要素の積。空なら1。整数はオーバーフロー時に折り返します。

## duration

[duration.md](duration.ja.md)

- `duration.ns(n: Int) -> Duration` — 実時間で n ナノ秒。n が負なら中止。
- `duration.us(n: Int) -> Duration` — 実時間で n マイクロ秒。n が負なら中止。
- `duration.ms(n: Int) -> Duration` — 実時間で n ミリ秒。n が負なら中止。
- `duration.s(n: Int) -> Duration` — 実時間で n 秒。n が負なら中止。
- `duration.min(n: Int) -> Duration` — 実時間で n 分。n が負なら中止。
- `duration.h(n: Int) -> Duration` — 実時間で n 時間。n が負なら中止。
- `duration.compare(a: Duration, b: Duration) -> Int` — a < b なら負、等しければ0、a > b なら正。

## env

[env.md](env.ja.md)

- `effect env.args() -> List[String]` — プログラム引数。argv[0] は除外。
- `effect env.get(name: String) -> Option[String]` — name の値。未設定なら none。
- `effect env.secret(name: String) -> Option[Secret[String]]` — name の値を Secret として返します（`Secret(***)` と表示、定時間 ==）。未設定なら none。トークンやキー用。
- `effect env.set(name: String, value: String) -> Unit` — このプロセスと以後の子プロセスに name を設定。名前が空、`=`/NUL を含む、値が NUL を含む場合は err。
- `effect env.remove(name: String) -> Unit` — このプロセスと以後の子プロセスから name を削除。未設定なら成功。名前が空、`=`/NUL を含む場合は err。
- `effect env.vars() -> Map[String, String]` — このプロセスの全環境変数。名前をキーとし名前順。秘密も通常の値として含むため、認証情報は `env.secret` で名前を指定して読みます。
- `effect env.cwd() -> String` — 絶対パスの現在ディレクトリー。読めない場合は err。
- `effect env.sleep_ms(ms: Int) -> Unit` — ms ミリ秒ブロック。0または負なら待たずに戻ります。
- `env.os() -> String` — ホスト OS：macos、linux、windows、unknown。wasm では "wasi"（C-189）。

## ffi

[ffi.md](ffi.ja.md)

- `ffi.region(b: Bytes) -> Region` — b の読取り専用ビュー。b は少なくともビューが存在する間は生存する。
- `effect ffi.alloc(n: Int) -> Region` — n 個のゼロバイトからなる新しい書込み可能領域。n が負なら err。
- `ffi.len(r: Region) -> Int` — 領域のバイト長。
- `ffi.is_writable(r: Region) -> Bool` — write が領域を変更できるなら true（ffi.alloc と書込み可能な外部領域）。
- `effect ffi.read(r: Region, off: Int, n: Int) -> Bytes` — off から n バイトのコピー。0 <= off かつ off + n <= len(r) でなければ err。
- `effect ffi.write(r: Region, off: Int, b: Bytes) -> Unit` — off に b を書く。読取り専用または b が収まらなければ err。
- `effect ffi.to_bytes(r: Region) -> Bytes` — 領域全体を Bytes としてコピー。
- `ffi.slice(r: Region, off: Int, n: Int) -> Result[Region, String]` — r 内の off から n バイトの読取り専用ビュー。0 <= off かつ off + n <= len(r) でなければ err。

## float

[float.md](float.ja.md)

- `float.to_string(x: Float) -> String` — 往復変換可能な最短の数字列。指数表記はなく、2.0 は .0 を保ちます。
- `float.parse(s: String) -> Result[Float, String]` — s の前後の ASCII 空白を除去して有限 Float にします。不正な文字、inf、NaN、オーバーフローは err。
- `float.to_fixed(x: Float, decimals: Int) -> String` — 固定小数点表記で、中間値は偶数へ丸めます。decimals が0..4096の範囲外なら中断します。
- `float.to_bits(f: Float) -> Int` — IEEE-754 ビットを Int として返します。どの NaN も正規 NaN になります。
- `float.abs(x: Float) -> Float` — 絶対値。-0.0 は0.0、NaN は NaN のまま。
- `float.floor(x: Float) -> Float` — -inf 方向に丸めます。-0.5 は -1.0。
- `float.ceil(x: Float) -> Float` — +inf 方向に丸めます。-0.5 は -0.0。
- `float.round(x: Float) -> Float` — 最も近い整数値。中間値はゼロから離れる方向に丸めます。
- `float.min(a: Float, b: Float) -> Float` — 小さい方。NaN のオペランドは無視します。-0.0 < 0.0。
- `float.max(a: Float, b: Float) -> Float` — 大きい方。NaN のオペランドは無視します。0.0 > -0.0。
- `float.clamp(x: Float, lo: Float, hi: Float) -> Float` — x を lo..hi に制限します。lo > hi または境界が NaN の場合は中断します。
- `float.sign(n: Float) -> Float   (deprecated — use float.signum)` — 符号ビットに応じて1.0または-1.0（0.0 は1.0）。NaN には NaN。
- `float.signum(x: Float) -> Float` — 符号ビットに応じて1.0または-1.0（0.0 は1.0、-0.0 は-1.0）。NaN には NaN。
- `float.is_nan(x: Float) -> Bool` — NaN（自身と等しくない）に対してのみ true。
- `float.is_infinite(x: Float) -> Bool` — inf と -inf に対して true、NaN に対して false。
- `float.trunc(x: Float) -> Float` — 0方向に丸めます。-0.5 は -0.0。
- `float.is_finite(x: Float) -> Bool` — x が inf、-inf、NaN のいずれでもなければ true。
- `float.pow(base: Float, exp: Float) -> Float` — base の exp 乗。負の base と小数の exp の組み合わせでは NaN。
- `float.compare(a: Float, b: Float) -> Int` — IEEE-754 全順序で -1、0、1：-0.0 < 0.0、NaN は inf より上で自身と等しい。
- `float.from_bits(bits: Int) -> Float` — IEEE-754 binary64 のビットが bits である Float。どの NaN も単一の NaN になります。
- `float.infinity() -> Float` — 正の無限大。
- `float.nan() -> Float` — 単一の静かな NaN（ビットは0x7ff8000000000000）。
- `float.epsilon() -> Float` — 1.0 と次の Float の間隔：2^-52。
- `float.max_value() -> Float` — 最大の有限 Float。約1.7976931348623157e308。
- `float.min_value() -> Float` — 最も負の有限 Float（= -max_value()）。最小の正数ではありません。
- `float.from(x: S) -> Float` — x を最も近い Float にします（中間値は偶数へ）。NaN は NaN のまま。
- `float.from_checked(x: S) -> Option[Float]` — Float が x を正確に保持できれば some(x as Float)、それ以外（丸めが生じる値、NaN）は none。

## float32

[float32.md](float32.ja.md)

- `float32.to_string(x: Float32) -> String` — f32 の往復変換可能な最短の数字列。指数表記はなく、0.1 は0.1のまま、2.0 は .0 を保ちます。
- `float32.compare(a: Float32, b: Float32) -> Int` — IEEE 754 全順序で -1、0、1：-0.0 < 0.0、NaN は inf より上で、NaN 同士は等しい。
- `float32.min(a: Float32, b: Float32) -> Float32` — 小さい方。NaN のオペランドは無視します。-0.0 < 0.0。
- `float32.max(a: Float32, b: Float32) -> Float32` — 大きい方。NaN のオペランドは無視します。0.0 > -0.0。
- `float32.clamp(x: Float32, lo: Float32, hi: Float32) -> Float32` — x を lo..hi に制限します。lo > hi または境界が NaN の場合は中断します。
- `float32.is_nan(x: Float32) -> Bool` — NaN の場合だけ true。
- `float32.is_infinite(x: Float32) -> Bool` — inf と -inf に対して true、NaN に対して false。
- `float32.is_finite(x: Float32) -> Bool` — x が inf、-inf、NaN のいずれでもなければ true。
- `float32.abs(x: Float32) -> Float32` — 絶対値。-0.0 は0.0、NaN は NaN のまま。
- `float32.floor(x: Float32) -> Float32` — -inf 方向に丸めます。-0.5 は -1.0。
- `float32.ceil(x: Float32) -> Float32` — +inf 方向に丸めます。-0.5 は -0.0。
- `float32.round(x: Float32) -> Float32` — 最も近い整数値。中間値はゼロから離れる方向に丸めます。
- `float32.trunc(x: Float32) -> Float32` — 0方向に丸めます。-0.5 は -0.0。
- `float32.signum(x: Float32) -> Float32` — 符号ビットに応じて1.0または-1.0（0.0 は1.0、-0.0 は-1.0）。NaN には NaN。
- `float32.to_bits(x: Float32) -> Int` — IEEE-754 binary32 の32ビットを非負の Int として返します。どの NaN も0x7fc00000になります。
- `float32.from_bits(bits: Int) -> Float32` — bits の下位32ビットを binary32 のビットとする Float32。
- `float32.infinity() -> Float32` — 正の無限大。
- `float32.nan() -> Float32` — 単一の静かな NaN（ビットは0x7fc00000）。
- `float32.epsilon() -> Float32` — 1.0 と次の Float32 の間隔：2^-23。
- `float32.max_value() -> Float32` — 最大の有限 Float32。約3.4028235e38。
- `float32.min_value() -> Float32` — 最も負の有限 Float32（= -max_value()）。最小の正数ではありません。
- `float32.parse(s: String) -> Result[Float32, String]` — 前後の空白を除去した s を Float32 にします。構文は float.parse と同じです。不正な文字は err、オーバーフローは inf。
- `float32.from(x: S) -> Float32` — x を最も近い Float32 に1回丸めます（中間値は偶数へ）。NaN は NaN のまま。
- `float32.from_checked(x: S) -> Option[Float32]` — Float32 が x を正確に保持できれば some(x as Float32)、それ以外（丸めが生じる値、NaN）は none。

## float64

[float64.md](float64.ja.md)

- `float64.to_string(x: Float64) -> String` — 通常の10進表記で、指数表記は使いません。2.0 は .0 を保ちます。
- `float64.compare(a: Float64, b: Float64) -> Int` — IEEE 754 全順序で -1、0、1：-0.0 < 0.0、NaN は inf より上で、NaN 同士は等しい。
- `float64.min(a: Float64, b: Float64) -> Float64` — 小さい方。NaN のオペランドは無視します。-0.0 < 0.0。
- `float64.max(a: Float64, b: Float64) -> Float64` — 大きい方。NaN のオペランドは無視します。0.0 > -0.0。
- `float64.clamp(x: Float64, lo: Float64, hi: Float64) -> Float64` — x を lo..hi に制限します。lo > hi または境界が NaN の場合は中断します。
- `float64.is_nan(x: Float64) -> Bool` — NaN の場合だけ true。
- `float64.is_infinite(x: Float64) -> Bool` — inf と -inf に対して true、NaN に対して false。
- `float64.is_finite(x: Float64) -> Bool` — x が inf、-inf、NaN のいずれでもなければ true。
- `float64.abs(x: Float64) -> Float64` — 絶対値。-0.0 は0.0、NaN は NaN のまま。
- `float64.floor(x: Float64) -> Float64` — -inf 方向に丸めます。-0.5 は -1.0。
- `float64.ceil(x: Float64) -> Float64` — +inf 方向に丸めます。-0.5 は -0.0。
- `float64.round(x: Float64) -> Float64` — 最も近い整数値。中間値はゼロから離れる方向に丸めます。
- `float64.trunc(x: Float64) -> Float64` — 0方向に丸めます。-0.5 は -0.0。
- `float64.signum(x: Float64) -> Float64` — 符号ビットに応じて1.0または-1.0（0.0 は1.0、-0.0 は-1.0）。NaN には NaN。
- `float64.to_bits(x: Float64) -> Int` — IEEE-754 binary64 の64ビットを Int として返します。どの NaN も正規 NaN になります。
- `float64.from_bits(bits: Int) -> Float64` — IEEE-754 binary64 のビットが bits である Float64。
- `float64.infinity() -> Float64` — 正の無限大。
- `float64.nan() -> Float64` — 単一の静かな NaN（ビットは0x7ff8000000000000）。
- `float64.epsilon() -> Float64` — 1.0 と次の Float64 の間隔：2^-52。
- `float64.max_value() -> Float64` — 最大の有限 Float64。約1.7976931348623157e308。
- `float64.min_value() -> Float64` — 最も負の有限 Float64（= -max_value()）。最小の正数ではありません。
- `float64.parse(s: String) -> Result[Float64, String]` — s の前後の ASCII 空白を除去して Float64 にします。構文は float.parse と同じです。不正な文字、inf、NaN は err。
- `float64.from(x: S) -> Float64` — x を最も近い Float64 にします（中間値は偶数へ）。NaN は NaN のまま。
- `float64.from_checked(x: S) -> Option[Float64]` — Float64 が x を正確に保持できれば some(x as Float64)、それ以外（丸めが生じる値、NaN）は none。

## fs

[fs.md](fs.ja.md)

- `effect fs.read_text(path: String) -> String` — 通常ファイル全体（<= 1 GiB）を UTF-8 として読む。不在または非 UTF-8 なら err。
- `effect fs.read_bytes(path: String) -> Bytes` — 通常ファイル全体（<= 1 GiB）を Bytes として読む。不在なら err。
- `effect fs.write_text(path: String, content: String) -> Unit` — 上書きまたは作成。親ディレクトリがなければ err。
- `effect fs.write_bytes(path: String, data: Bytes) -> Unit` — data をそのまま書き、ファイルを置き換える。
- `effect fs.append_text(path: String, content: String) -> Unit` — content を追記。不在なら作成。
- `effect fs.append_bytes(path: String, data: Bytes) -> Unit` — data をそのまま追記。不在なら作成。
- `effect fs.mkdir_p(path: String) -> Unit` — ディレクトリと親を作成。既存なら ok。
- `effect fs.exists(path: String) -> Bool` — 不在または宛先のないシンボリックリンクなら false。
- `effect fs.read_lines(path: String) -> List[String]` — <= 1 GiB の通常ファイルの CRLF 対応行。末尾改行は行を増やさない。
- `effect fs.fold_lines(path: String, init: A, f: (A, String) -> A) -> A` — 行をストリーム処理して fold。不在なら err。
- `effect fs.for_each_line(path: String, f: (String) -> Unit) -> Unit` — ストリーム処理で各行に f を呼ぶ。ファイル途中で err になる場合あり。
- `effect fs.fold_lines_range(path: String, start: Int, end: Int, init: A, f: (A, String) -> A) -> A` — バイト [start, end) で始まる行を fold。
- `effect fs.fold_lines_chunked(path: String, workers: Int, init: A, f: (A, String) -> A) -> List[A]` — 並列チャンクの部分結果を順序通りに返す。空なら []。
- `effect fs.read_text_if_exists(path: String) -> Option[String]` — テキスト、不在なら none。非 UTF-8 なら err。
- `effect fs.read_bytes_if_exists(path: String) -> Option[Bytes]` — ファイルの Bytes、不在なら none。
- `effect fs.read_lines_if_exists(path: String) -> Option[List[String]]` — read_lines の結果、不在なら none。
- `effect fs.remove(path: String) -> Unit` — ファイルまたは空ディレクトリを削除。不在なら err。
- `effect fs.list_dir(path: String) -> List[String]` — 整列済みの名前（パスではない）。ディレクトリ以外は err。
- `effect fs.is_dir(path: String) -> Bool` — ディレクトリなら true。シンボリックリンクを辿る。
- `effect fs.is_file(path: String) -> Bool` — 通常ファイルなら true。シンボリックリンクを辿る。
- `effect fs.copy(src: String, dst: String) -> Unit` — src を dst へコピー。dst は上書き。
- `effect fs.rename(src: String, dst: String) -> Unit` — src を dst へ移動。既存 dst を置換。
- `effect fs.walk(dir: String) -> List[String]` — dir 配下の整列済みパス。リンクは列挙するが辿らない。
- `effect fs.remove_all(path: String) -> Unit` — ツリーまたはファイルを削除。不在なら err。
- `effect fs.temp_dir() -> String` — ホストの一時ディレクトリ。場所であり安全な名前ではない。
- `effect fs.stat(path: String) -> FileStat` — サイズ・種類・mtime。不在なら err。
- `effect fs.glob(pattern: String) -> List[String]` — 整列済みの * と ** マッチ。リンクは辿らない。
- `effect fs.create_temp_file(prefix: String) -> String` — ランダム名の新しい非公開ファイル。そのパス。
- `effect fs.create_temp_dir(prefix: String) -> String` — ランダム名の新しい非公開ディレクトリ。そのパス。
- `effect fs.is_symlink(path: String) -> Bool` — 宛先がなくてもシンボリックリンクなら true。
- `effect fs.open_root(dir: String) -> FsRoot` — dir を FsRoot として返す。ディレクトリ以外なら err。
- `effect fs.root_read_text(root: FsRoot, path: String) -> String` — root 内の path を read_text。範囲外へ出るなら err。
- `effect fs.root_read_bytes(root: FsRoot, path: String) -> Bytes` — root 内の path を read_bytes。範囲外へ出るなら err。
- `effect fs.root_write_text(root: FsRoot, path: String, content: String) -> Unit` — root 内の path に content を書く。範囲外へ出るなら err。
- `effect fs.root_list_dir(root: FsRoot, path: String) -> List[String]` — root 内の path を list_dir。範囲外へ出るなら err。
- `effect fs.root_exists(root: FsRoot, path: String) -> Bool` — root 内の path を exists。範囲外へ出るなら err。
- `effect fs.root_remove(root: FsRoot, path: String) -> Unit` — root 内の path を remove。範囲外へ出るなら err。

## hash

[hash.md](hash.ja.md)

- `hash.fnv1a32(b: Bytes) -> Int` — b の 32-bit FNV-1a、0..4294967295。
- `hash.sha256(b: Bytes) -> Bytes` — b の 32-byte SHA-256 digest。MAC ではありません。sha256(key ++ msg) は length extension で偽造可能です。hash.hmac_sha256 / hash.is_valid_hmac_sha256 を使います。
- `hash.sha256_hex(b: Bytes) -> String` — b の SHA-256 を 64 小文字十六進文字で返します。
- `hash.crc32(b: Bytes) -> Int` — CRC-32（IEEE、反射型）を unsigned Int で返します。空の b は 0。
- `hash.adler32(b: Bytes) -> Int` — Adler-32 を unsigned Int で返します。空の b は 1。
- `hash.sha512(b: Bytes) -> Bytes` — b の 64-byte SHA-512 digest。MAC ではありません。sha512(key ++ msg) は length extension で偽造可能です。hash.hmac_sha512 / hash.is_valid_hmac_sha512 を使います。
- `hash.sha512_hex(b: Bytes) -> String` — b の SHA-512 を 128 小文字十六進文字で返します。
- `hash.hmac_sha256(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]` — key の下での b の 32-byte HMAC-SHA256（RFC 2104）、key が先。tag は Secret なので == は定時間です。十六進: hex.encode(secret.reveal(tag))。
- `hash.hmac_sha512(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]` — key の下での b の 64-byte HMAC-SHA512（RFC 2104）、key が先。tag は Secret なので == は定時間です。十六進: hex.encode(secret.reveal(tag))。
- `hash.is_valid_hmac_sha256(key: Secret[Bytes], b: Bytes, tag: Secret[Bytes]) -> Bool` — tag が key の下での b の HMAC-SHA256 なら true。定時間で比較します。十六進 tag: hash.is_valid_hmac_sha256(key, b, secret.new(hex.decode(sig) ?? bytes.from_list([])))。
- `hash.is_valid_hmac_sha512(key: Secret[Bytes], b: Bytes, tag: Secret[Bytes]) -> Bool` — tag が key の下での b の HMAC-SHA512 なら true。定時間で比較します。十六進 tag: hash.is_valid_hmac_sha512(key, b, secret.new(hex.decode(sig) ?? bytes.from_list([])))。

## heap

[heap.md](heap.ja.md)

- `heap.new() -> Heap[A]` — 空のヒープ。最小要素から取り出します。
- `heap.new_by(f: (A) -> K) -> Heap[A]` — キー f(x) 順の空のヒープ。最小キーから取り出します。
- `heap.new_with(cmp: (A, A) -> Int) -> Heap[A]` — cmp 順の空のヒープ。負なら a が先。
- `heap.from_list(xs: List[A]) -> Heap[A]` — xs から自然順のヒープを作ります。等しい要素はリスト順に出ます。
- `heap.push(mut h: Heap[A], x: A) -> Unit` — x をその場で追加します。
- `heap.pop(mut h: Heap[A]) -> Option[A]` — 次に取り出す要素を削除して返します。空なら none。
- `heap.clear(mut h: Heap[A]) -> Unit` — h の全要素をその場で削除します。順序規則は保持します。
- `heap.peek(h: Heap[A]) -> Option[A]` — pop が次に返す要素。空なら none。
- `heap.len(h: Heap[A]) -> Int` — 要素数。空のヒープでは 0。
- `heap.is_empty(h: Heap[A]) -> Bool` — h に要素がない場合に限り true。
- `heap.to_list(h: Heap[A]) -> List[A]` — pop 順の要素。
- `heap.contains(h: Heap[A], x: A) -> Bool` — いずれかの要素が == x なら true。
- `heap.any(h: Heap[A], f: (A) -> Bool) -> Bool` — f が成立する要素があれば true。空なら false。
- `heap.all(h: Heap[A], f: (A) -> Bool) -> Bool` — 全要素で f が成立すれば true。空なら true。
- `heap.count(h: Heap[A], f: (A) -> Bool) -> Int` — f が成立する要素の数。
- `heap.filter(h: Heap[A], f: (A) -> Bool) -> Heap[A]` — f が成立する要素を同じ順序規則のヒープにします。等しい要素は入った順を保持します。

## hex

[hex.md](hex.ja.md)

- `hex.encode(b: Bytes) -> String` — 小文字の十六進表記。1 バイトにつき 2 文字。
- `hex.encode_upper(b: Bytes) -> String` — 大文字の十六進表記。1 バイトにつき 2 文字。
- `hex.decode(s: String) -> Result[Bytes, String]` — 十六進表記（大小文字どちらも）からバイト列へ。奇数長または不正文字なら err。

## html

[html.md](html.ja.md)

- `html.escape(s: String) -> SafeHtml` — s の & < > " ' を HTML 実体参照へ変換。テキスト文脈専用。
- `html.raw(s: String) -> SafeHtml` — エスケープしない s を SafeHtml とする。信頼済み入力専用。
- `html.to_string(h: SafeHtml) -> String` — h 内の HTML テキストをそのまま返す。
- `html.concat(a: SafeHtml, b: SafeHtml) -> SafeHtml` — a の次に b を一つの SafeHtml とする。再エスケープしない。
- `html.empty() -> SafeHtml` — 空文字列の SafeHtml。
- `html.attr(name: String, value: String) -> SafeHtml` — value を二重引用符付き属性用にエスケープした name="value"。URL、イベントハンドラー、不正な名前では中止。
- `html.url_attr(name: String, url: String) -> SafeHtml` — name="url" をエスケープ。url は相対形式または http/https/mailto のみ保持し、それ以外は about:invalid#almide-blocked。
- `html.script_json(v: Value) -> SafeHtml` — json.stringify(v) の < > & U+2028 U+2029 を \u エスケープにし、`<script>` 内で使える形にする。

## http

[http.md](http.ja.md)

- `effect http.serve(port: Int, handler: HttpHandler) -> Unit` — 0.0.0.0:port で継続的に提供する。ハンドラーの err は 500 になる。
- `http.response(status: Int, body: B) -> HttpResponse` — ボディ付きで応答する。その型が Content-Type を決める（String: text/plain; charset=utf-8）。
- `http.json(status: Int, body: String) -> HttpResponse` — Content-Type application/json で応答する。ボディは JSON テキスト。
- `http.with_headers(status: Int, body: B, headers: List[(String, String)]) -> HttpResponse` — ボディと、Content-Type の後に指定順のヘッダーを付けて応答する。同名の繰り返しでは最初の綴りと最後の値を保持する。
- `http.with_body(resp: HttpResponse, body: B) -> HttpResponse` — resp のボディを body に、Content-Type を body の型に置き換える。
- `http.typed(media_type: String, body: Bytes) -> TypedBody` — メディア型 media_type のボディ。リテラルであり、スクリプト実行可能な型や標準ライブラリが所有する型ではない。
- `http.redirect(url: String) -> HttpResponse` — Location が url、空のボディ、Content-Type text/plain; charset=utf-8 を持つ 302。
- `http.set_status(resp: HttpResponse, code: Int) -> HttpResponse` — resp に code を設定する。範囲検査はしない。
- `http.status(resp: HttpResponse) -> Int` — HTTP ステータスコード。ステータス行が不正なら 0。
- `http.header(m: M, name: String) -> Option[String]` — 大文字・小文字を問わずヘッダー name の最初の値。なければ none。
- `http.header_values(m: M, name: String) -> List[String]` — 大文字・小文字を問わずヘッダー name の全値を通信順に返す。なければ []。
- `http.headers(m: M) -> List[(String, String)]` — 各ヘッダーを (name, value) として返す。通信上の綴りと順序、重複を保持する。
- `http.body(m: M) -> String` — ボディのテキスト。取得したボディは非可逆に UTF-8 デコードする。
- `http.body_bytes(m: M) -> Bytes` — ボディのバイト。取得したボディは送信どおり、構築したボディは body(m) の UTF-8 バイト。
- `http.set_header(m: M, name: String, value: String) -> M` — m にヘッダー name を大文字・小文字を問わず設定する。最初の一致をその場で置換し、なければ追加。レスポンスでは Content-Type を拒否する。
- `http.set_body(m: M, body: String) -> M` — m のボディを置換する。レスポンスの Content-Type は text/plain; charset=utf-8 になる。
- `http.set_body_bytes(m: M, body: Bytes) -> M` — m のボディを、非可逆な UTF-8 でデコードした body に置換する。レスポンスの Content-Type は application/octet-stream になる。
- `http.method(req: HttpRequest) -> String` — 送信されたままのリクエストメソッド。例：GET。
- `http.path(req: HttpRequest) -> String` — クエリ文字列を含むリクエストターゲット（送信用に構築したリクエストでは完全 URL）。
- `http.query_params(req: HttpRequest) -> Result[List[(String, String)], String]` — ターゲットの最初の ? 以降に url.parse_query を適用。組は順序を保持。不正なエスケープは err。
- `effect http.get(url: String) -> String` — 404 でもボディを返す。err はトランスポートの失敗のみ。
- `effect http.request(method: String, url: String, body: String, headers: List[(String, String)]) -> String` — 任意のメソッドに対する応答ボディ。どのステータスでも ok。
- `effect http.get_response(url: String) -> HttpResponse` — 404 を含む応答全体。リダイレクトには追従しない。
- `effect http.send(req: HttpRequest) -> HttpResponse` — req をターゲットが示す URL に送る。どのステータスでも応答全体を返し、リダイレクトには追従しない。
- `effect http.start(req: HttpRequest, limits: HttpLimits) -> HttpCall` — req の送信を開始し、ハンドルを返して直ちに戻る。
- `effect http.poll(c: HttpCall) -> Option[Result[HttpResponse, String]]` — ブロックしない。実行中は none、その後は結果。
- `effect http.read_new(c: HttpCall) -> String` — 前回の read_new 以降のボディテキスト。ブロックしない。
- `effect http.wait(c: HttpCall) -> HttpResponse` — 呼び出し終了までブロックする。どのステータスでも ok。
- `effect http.cancel(c: HttpCall) -> Unit` — 接続を閉じる。呼び出しは err で終了する。
- `http.new_request(method: String, target: String, body: String, headers: List[(String, String)]) -> HttpRequest` — コード内で構築するリクエスト。ターゲットは ?query を保持する（send / start では完全 URL）。
- `http.param(req: HttpRequest, name: String) -> Option[String]` — ルーターがバインドした、デコード済みパスパラメーター。未バインドなら none。
- `http.route(pattern: String, handler: HttpHandler) -> HttpRoute` — "GET /users/{id}" からルートを作る。メソッドなしなら任意。
- `http.mount(prefix: String, sub: HttpHandler) -> HttpRoute` — プレフィックス配下のサブアプリ。/api/x は /x として届く。
- `http.router(xs: List[HttpRoute]) -> Result[HttpHandler, String]` — ルートテーブルをハンドラーにする。ルート競合は err。
- `http.wrap(handler: HttpHandler, middleware: List[HttpMiddleware]) -> HttpHandler` — ミドルウェアを適用する。先頭が最も外側。
- `http.decode_json(req: HttpRequest) -> Result[T, HttpResponse]` — JSON ボディを入力型 T に厳密にデコードする。err は 400 レスポンス。

## int

[int.md](int.ja.md)

- `int.to_string(x: Int) -> String` — 10進数字。負の場合は - を付けます。
- `int.to_hex(x: Int) -> String` — 小文字の16進数。0x は付きません。-1 は f が16桁になります。
- `int.parse(s: String) -> Result[Int, String]` — s の前後の ASCII スペース/タブ/LF/FF/CR を除去して10進 Int にします。不正な文字やオーバーフローは err です。
- `int.abs(x: Int) -> Int` — x の大きさ。min_value は折り返して自身になります。
- `int.min(a: Int, b: Int) -> Int` — a と b の小さい方。
- `int.max(a: Int, b: Int) -> Int` — a と b の大きい方。
- `int.clamp(x: Int, lo: Int, hi: Int) -> Int` — x を lo..hi に制限します。lo > hi のとき中断します。
- `int.is_even(x: Int) -> Bool` — x % 2 == 0 のとき true。負数でも正確です。
- `int.is_odd(x: Int) -> Bool` — x % 2 != 0 のとき true。-3 は奇数です。
- `int.band(a: Int, b: Int) -> Int` — 64ビットすべてのビット単位 AND。
- `int.bor(a: Int, b: Int) -> Int` — 64ビットすべてのビット単位 OR。
- `int.bxor(a: Int, b: Int) -> Int` — 64ビットすべてのビット単位 XOR。
- `int.bnot(x: Int) -> Int` — ビット反転。-x - 1 と等価です。
- `int.bshl(a: Int, n: Int) -> Int` — a を n ビット左シフトします。n は64を法とします。
- `int.bshr(a: Int, n: Int) -> Int` — 算術（符号埋め）右シフト。n は64を法とします。
- `int.count_leading_zeros(x: Int) -> Int` — 64ビット語の先頭のゼロビット数。0 の場合は64。
- `int.count_trailing_zeros(x: Int) -> Int` — 末尾のゼロビット数。0 の場合は64。
- `int.pop_count(n: Int) -> Int   (deprecated — use int.count_ones)` — 立っているビット数。-1 の場合は64。
- `int.count_ones(x: Int) -> Int` — 立っているビット数。-1 の場合は64。
- `int.bit_reverse(x: Int) -> Int` — 64ビットの順序を反転します。1 は min_value になります。
- `int.byte_swap(x: Int) -> Int` — 8バイトの順序を反転します（エンディアン反転）。
- `int.bit_width(n: Int) -> Int   (deprecated — use int.bit_length)` — n を格納するのに必要なビット数。0 は0、負数は64。
- `int.bit_length(x: Int) -> Int` — x を格納するのに必要なビット数。0 は0、負数は64。
- `int.log2_floor(x: Int) -> Int` — log2(x) の床。x <= 0 の場合は -1。
- `int.log2_ceil(x: Int) -> Int` — log2(x) の天井。x <= 1 の場合は0。
- `int.next_power_of_two(x: Int) -> Int` — x 以上の最小の2の累乗。x <= 1 なら1。2^62 を超えると min_value。
- `int.prev_power_of_two(x: Int) -> Int` — x 以下の最大の2の累乗。x <= 0 なら0。
- `int.wrap_add(a: Int, b: Int) -> Int` — a + b を64ビットに折り返します。
- `int.wrap_mul(a: Int, b: Int) -> Int` — a * b を64ビットに折り返します。
- `int.rotate_right(a: Int, n: Int) -> Int` — a の64ビットを右に n 回転します。n は64を法とします。
- `int.rotate_left(a: Int, n: Int) -> Int` — a の64ビットを左に n 回転します。n は64を法とします。
- `int.min_value() -> Int` — 最小の Int：-2^63。
- `int.max_value() -> Int` — 最大の Int：2^63-1。
- `int.compare(a: Int, b: Int) -> Int` — a が b より小さい、等しい、大きい順に -1、0、1。
- `int.sign(x: Int) -> Int` — x の符号に応じて -1、0、1。0 に対しては0。
- `int.pow(base: Int, exp: Int) -> Int` — base の exp 乗。* と同様に64ビットに折り返します。exp < 0 は中断します。
- `int.floor_div(a: Int, b: Int) -> Int` — 商を -inf 方向に丸めます。/ と同様、b == 0 と min_value / -1 は中断します。
- `int.modulo(a: Int, b: Int) -> Int` — b と同じ符号の剰余で、a == floor_div(a, b) * b + modulo(a, b) を満たします。b == 0 は中断します。
- `int.wrap(x: Int, bits: Int) -> Int` — x mod 2^bits。bits < 64 なら非負です。bits <= 0 は中断し、bits >= 64 は x を保ちます。
- `int.wrap_sub(a: Int, b: Int) -> Int` — a - b を64ビットに折り返します。
- `int.bshr_logical(a: Int, n: Int) -> Int` — 64ビットをゼロ埋めして右シフトします。n は64を法とします。
- `int.to_string_radix(x: Int, radix: Int) -> String` — x を radix 進の符号付き小文字数字で表します。16進の -255 は "-ff"。radix が2..36の範囲外なら中断します。
- `int.parse_radix(s: String, radix: Int) -> Result[Int, String]` — 前後の空白を除去した s を radix 進の Int にします。+ または - の符号、大文字・小文字、16・8・2進での0x・0o・0bを許可します。不正な文字やオーバーフローは err。radix が2..36の範囲外なら中断します。
- `int.from(x: S) -> Int` — x をゼロ方向に丸めて Int にします。NaN、無限大、Int の範囲外の値は中断します。
- `int.from_checked(x: S) -> Option[Int]` — Int が x を正確に保持できれば some(x as Int)、それ以外（小数、NaN、範囲外）は none。
- `int.from_saturating(x: S) -> Int` — x をゼロ方向に丸めて Int にし、Int の範囲に制限します。NaN は0。
- `int.from_wrapping(x: S) -> Int` — 整数 x の下位64ビットを Int として扱います（2の補数）。

## int16

[int16.md](int16.ja.md)

- `int16.to_string(x: Int16) -> String` — 10進数字。負の場合は - を付けます。
- `int16.min_value() -> Int16` — 最小の Int16：-32768。
- `int16.max_value() -> Int16` — 最大の Int16：32767。
- `int16.compare(a: Int16, b: Int16) -> Int` — a が b より小さい、等しい、大きい順に -1、0、1。
- `int16.min(a: Int16, b: Int16) -> Int16` — a と b の小さい方。
- `int16.max(a: Int16, b: Int16) -> Int16` — a と b の大きい方。
- `int16.clamp(x: Int16, lo: Int16, hi: Int16) -> Int16` — x を lo..hi に制限します。lo > hi のとき中断します。
- `int16.floor_div(a: Int16, b: Int16) -> Int16` — 商を -inf 方向に丸めます。/ と同様、b == 0 と min_value / -1 は中断します。
- `int16.modulo(a: Int16, b: Int16) -> Int16` — b と同じ符号の剰余で、a == floor_div(a, b) * b + modulo(a, b) を満たします。b == 0 は中断します。
- `int16.pow(base: Int16, exp: Int) -> Int16` — base の exp 乗。* と同様に16ビットに折り返します。exp < 0 は中断します。
- `int16.is_even(x: Int16) -> Bool` — x が2の倍数なら true。
- `int16.is_odd(x: Int16) -> Bool` — x が2の倍数でなければ true。
- `int16.abs(x: Int16) -> Int16` — x の大きさ。min_value は折り返して自身になります。
- `int16.sign(x: Int16) -> Int` — x の符号に応じて -1、0、1。
- `int16.band(a: Int16, b: Int16) -> Int16` — ビット単位 AND。
- `int16.bor(a: Int16, b: Int16) -> Int16` — ビット単位 OR。
- `int16.bxor(a: Int16, b: Int16) -> Int16` — ビット単位 XOR。
- `int16.bnot(x: Int16) -> Int16` — 16ビットすべてをビット反転します。
- `int16.bshl(a: Int16, n: Int) -> Int16` — a を n ビット左シフトします。n は16を法とします。
- `int16.bshr(a: Int16, n: Int) -> Int16` — 符号埋めの右シフト。n は16を法とします。
- `int16.count_leading_zeros(x: Int16) -> Int` — 16ビット内で、最上位の立っているビットより上のゼロビット数。0 の場合は16。
- `int16.count_trailing_zeros(x: Int16) -> Int` — 最下位の立っているビットより下のゼロビット数。0 の場合は16。
- `int16.count_ones(x: Int16) -> Int` — 16ビット中の立っているビット数。
- `int16.byte_swap(x: Int16) -> Int16` — 2バイトの順序を反転します（エンディアン反転）。
- `int16.wrap_add(a: Int16, b: Int16) -> Int16` — a + b を16ビットに折り返します。
- `int16.wrap_sub(a: Int16, b: Int16) -> Int16` — a - b を16ビットに折り返します。
- `int16.wrap_mul(a: Int16, b: Int16) -> Int16` — a * b を16ビットに折り返します。
- `int16.rotate_left(a: Int16, n: Int) -> Int16` — a の16ビットを左に n 回転します。n は16を法とします。
- `int16.rotate_right(a: Int16, n: Int) -> Int16` — a の16ビットを右に n 回転します。n は16を法とします。
- `int16.bit_length(x: Int16) -> Int` — x を格納するのに必要なビット数。0 は0、負数は16。
- `int16.parse(s: String) -> Result[Int16, String]` — s の前後の ASCII 空白を除去して10進 Int16 にします。不正な文字や -32768..32767 の範囲外は err。
- `int16.to_string_radix(x: Int16, radix: Int) -> String` — x を radix 進の小文字数字で表し、負数には - を付けます。radix が2..36の範囲外なら中断します。
- `int16.parse_radix(s: String, radix: Int) -> Result[Int16, String]` — 前後の空白を除去した s を radix 進の Int16 にします。構文は int.parse_radix と同じです。不正な文字や -32768..32767 の範囲外は err。
- `int16.from(x: S) -> Int16` — x をゼロ方向に丸めて Int16 にします。NaN、無限大、Int16 の範囲外の値は中断します。
- `int16.from_checked(x: S) -> Option[Int16]` — Int16 が x を正確に保持できれば some(x as Int16)、それ以外（小数、NaN、範囲外）は none。
- `int16.from_saturating(x: S) -> Int16` — x をゼロ方向に丸めて Int16 にし、Int16 の範囲に制限します。NaN は0。
- `int16.from_wrapping(x: S) -> Int16` — 整数 x の下位16ビットを Int16 として扱います（2の補数）。

## int32

[int32.md](int32.ja.md)

- `int32.to_string(x: Int32) -> String` — 10進数字。負の場合は - を付けます。
- `int32.min_value() -> Int32` — 最小の Int32：-2147483648。
- `int32.max_value() -> Int32` — 最大の Int32：2147483647。
- `int32.compare(a: Int32, b: Int32) -> Int` — a が b より小さい、等しい、大きい順に -1、0、1。
- `int32.min(a: Int32, b: Int32) -> Int32` — a と b の小さい方。
- `int32.max(a: Int32, b: Int32) -> Int32` — a と b の大きい方。
- `int32.clamp(x: Int32, lo: Int32, hi: Int32) -> Int32` — x を lo..hi に制限します。lo > hi のとき中断します。
- `int32.floor_div(a: Int32, b: Int32) -> Int32` — 商を -inf 方向に丸めます。/ と同様、b == 0 と min_value / -1 は中断します。
- `int32.modulo(a: Int32, b: Int32) -> Int32` — b と同じ符号の剰余で、a == floor_div(a, b) * b + modulo(a, b) を満たします。b == 0 は中断します。
- `int32.pow(base: Int32, exp: Int) -> Int32` — base の exp 乗。* と同様に32ビットに折り返します。exp < 0 は中断します。
- `int32.is_even(x: Int32) -> Bool` — x が2の倍数なら true。
- `int32.is_odd(x: Int32) -> Bool` — x が2の倍数でなければ true。
- `int32.abs(x: Int32) -> Int32` — x の大きさ。min_value は折り返して自身になります。
- `int32.sign(x: Int32) -> Int` — x の符号に応じて -1、0、1。
- `int32.band(a: Int32, b: Int32) -> Int32` — ビット単位 AND。
- `int32.bor(a: Int32, b: Int32) -> Int32` — ビット単位 OR。
- `int32.bxor(a: Int32, b: Int32) -> Int32` — ビット単位 XOR。
- `int32.bnot(x: Int32) -> Int32` — 32ビットすべてをビット反転します。
- `int32.bshl(a: Int32, n: Int) -> Int32` — a を n ビット左シフトします。n は32を法とします。
- `int32.bshr(a: Int32, n: Int) -> Int32` — 符号埋めの右シフト。n は32を法とします。
- `int32.count_leading_zeros(x: Int32) -> Int` — 32ビット内で、最上位の立っているビットより上のゼロビット数。0 の場合は32。
- `int32.count_trailing_zeros(x: Int32) -> Int` — 最下位の立っているビットより下のゼロビット数。0 の場合は32。
- `int32.count_ones(x: Int32) -> Int` — 32ビット中の立っているビット数。
- `int32.byte_swap(x: Int32) -> Int32` — 4バイトの順序を反転します（エンディアン反転）。
- `int32.wrap_add(a: Int32, b: Int32) -> Int32` — a + b を32ビットに折り返します。
- `int32.wrap_sub(a: Int32, b: Int32) -> Int32` — a - b を32ビットに折り返します。
- `int32.wrap_mul(a: Int32, b: Int32) -> Int32` — a * b を32ビットに折り返します。
- `int32.rotate_left(a: Int32, n: Int) -> Int32` — a の32ビットを左に n 回転します。n は32を法とします。
- `int32.rotate_right(a: Int32, n: Int) -> Int32` — a の32ビットを右に n 回転します。n は32を法とします。
- `int32.bit_length(x: Int32) -> Int` — x を格納するのに必要なビット数。0 は0、負数は32。
- `int32.parse(s: String) -> Result[Int32, String]` — s の前後の ASCII 空白を除去して10進 Int32 にします。不正な文字や -2147483648..2147483647 の範囲外は err。
- `int32.to_string_radix(x: Int32, radix: Int) -> String` — x を radix 進の小文字数字で表し、負数には - を付けます。radix が2..36の範囲外なら中断します。
- `int32.parse_radix(s: String, radix: Int) -> Result[Int32, String]` — 前後の空白を除去した s を radix 進の Int32 にします。構文は int.parse_radix と同じです。不正な文字や -2147483648..2147483647 の範囲外は err。
- `int32.from(x: S) -> Int32` — x をゼロ方向に丸めて Int32 にします。NaN、無限大、Int32 の範囲外の値は中断します。
- `int32.from_checked(x: S) -> Option[Int32]` — Int32 が x を正確に保持できれば some(x as Int32)、それ以外（小数、NaN、範囲外）は none。
- `int32.from_saturating(x: S) -> Int32` — x をゼロ方向に丸めて Int32 にし、Int32 の範囲に制限します。NaN は0。
- `int32.from_wrapping(x: S) -> Int32` — 整数 x の下位32ビットを Int32 として扱います（2の補数）。

## int64

[int64.md](int64.ja.md)

- `int64.to_string(x: Int64) -> String` — 10 進数字。負なら - を付ける。
- `int64.min_value() -> Int64` — 最小の Int64：-2^63。
- `int64.max_value() -> Int64` — 最大の Int64：2^63-1。
- `int64.compare(a: Int64, b: Int64) -> Int` — a が b より小さい、等しい、大きい順に -1、0、1。
- `int64.min(a: Int64, b: Int64) -> Int64` — a と b の小さい方。
- `int64.max(a: Int64, b: Int64) -> Int64` — a と b の大きい方。
- `int64.clamp(x: Int64, lo: Int64, hi: Int64) -> Int64` — x を lo..hi に収める。lo > hi なら abort。
- `int64.floor_div(a: Int64, b: Int64) -> Int64` — 商を -inf 方向へ丸める。/ と同様、b == 0 と min_value / -1 は abort。
- `int64.modulo(a: Int64, b: Int64) -> Int64` — b と同じ符号の剰余。a == floor_div(a, b) * b + modulo(a, b) を満たす。b == 0 なら abort。
- `int64.pow(base: Int64, exp: Int) -> Int64` — base の exp 乗。* と同様に 64 ビットへ折り返す。exp < 0 なら abort。
- `int64.is_even(x: Int64) -> Bool` — x が 2 の倍数なら true。
- `int64.is_odd(x: Int64) -> Bool` — x が 2 の倍数でなければ true。
- `int64.abs(x: Int64) -> Int64` — x の大きさ。min_value は自身へ折り返す。
- `int64.sign(x: Int64) -> Int` — x の符号に応じ -1、0、1。
- `int64.band(a: Int64, b: Int64) -> Int64` — ビット単位 AND。
- `int64.bor(a: Int64, b: Int64) -> Int64` — ビット単位 OR。
- `int64.bxor(a: Int64, b: Int64) -> Int64` — ビット単位 XOR。
- `int64.bnot(x: Int64) -> Int64` — 64 ビットすべてをビット反転。
- `int64.bshl(a: Int64, n: Int) -> Int64` — a を n ビット左シフト。n は 64 を法とする。
- `int64.bshr(a: Int64, n: Int) -> Int64` — 符号埋め右シフト。n は 64 を法とする。
- `int64.count_leading_zeros(x: Int64) -> Int` — 64 ビット中の最上位の立ったビットより上のゼロ数。0 なら 64。
- `int64.count_trailing_zeros(x: Int64) -> Int` — 最下位の立ったビットより下のゼロ数。0 なら 64。
- `int64.count_ones(x: Int64) -> Int` — 64 ビット中の立っているビット数。
- `int64.byte_swap(x: Int64) -> Int64` — 8 バイトを逆順にする（エンディアン反転）。
- `int64.wrap_add(a: Int64, b: Int64) -> Int64` — a + b を 64 ビットへ折り返す。
- `int64.wrap_sub(a: Int64, b: Int64) -> Int64` — a - b を 64 ビットへ折り返す。
- `int64.wrap_mul(a: Int64, b: Int64) -> Int64` — a * b を 64 ビットへ折り返す。
- `int64.rotate_left(a: Int64, n: Int) -> Int64` — a の 64 ビットを n だけ左回転。n は 64 を法とする。
- `int64.rotate_right(a: Int64, n: Int) -> Int64` — a の 64 ビットを n だけ右回転。n は 64 を法とする。
- `int64.bit_length(x: Int64) -> Int` — x を保持するのに必要なビット数。0 なら 0、負なら 64。
- `int64.parse(s: String) -> Result[Int64, String]` — 前後の ASCII 空白を除いた s から 10 進 Int64 を解析。不正内容や overflow なら err。
- `int64.to_string_radix(x: Int64, radix: Int) -> String` — x を radix 進の小文字数字で表す。負なら - を付ける。radix が 2..36 外なら abort。
- `int64.parse_radix(s: String, radix: Int) -> Result[Int64, String]` — 前後の空白を除いた s を radix 進の Int64 として解析。文法は int.parse_radix。不正内容や overflow なら err。
- `int64.from(x: S) -> Int64` — x をゼロ方向に丸め Int64 にする。NaN、無限大、Int64 範囲外なら abort。
- `int64.from_checked(x: S) -> Option[Int64]` — Int64 が x を正確に保持できれば some(x as Int64)、それ以外（小数部分、NaN、範囲外）は none。
- `int64.from_saturating(x: S) -> Int64` — x をゼロ方向に丸め Int64 にし、Int64 の範囲に収める。NaN は 0。
- `int64.from_wrapping(x: S) -> Int64` — 整数 x の下位 64 ビットを Int64 とする（2 の補数）。

## int8

[int8.md](int8.ja.md)

- `int8.to_string(x: Int8) -> String` — 10進数字。負の場合は - を付けます。
- `int8.min_value() -> Int8` — 最小の Int8：-128。
- `int8.max_value() -> Int8` — 最大の Int8：127。
- `int8.compare(a: Int8, b: Int8) -> Int` — a が b より小さい、等しい、大きい順に -1、0、1。
- `int8.min(a: Int8, b: Int8) -> Int8` — a と b の小さい方。
- `int8.max(a: Int8, b: Int8) -> Int8` — a と b の大きい方。
- `int8.clamp(x: Int8, lo: Int8, hi: Int8) -> Int8` — x を lo..hi に制限します。lo > hi のとき中断します。
- `int8.floor_div(a: Int8, b: Int8) -> Int8` — 商を -inf 方向に丸めます。/ と同様、b == 0 と min_value / -1 は中断します。
- `int8.modulo(a: Int8, b: Int8) -> Int8` — b と同じ符号の剰余で、a == floor_div(a, b) * b + modulo(a, b) を満たします。b == 0 は中断します。
- `int8.pow(base: Int8, exp: Int) -> Int8` — base の exp 乗。* と同様に8ビットに折り返します。exp < 0 は中断します。
- `int8.is_even(x: Int8) -> Bool` — x が2の倍数なら true。
- `int8.is_odd(x: Int8) -> Bool` — x が2の倍数でなければ true。
- `int8.abs(x: Int8) -> Int8` — x の大きさ。min_value は折り返して自身になります。
- `int8.sign(x: Int8) -> Int` — x の符号に応じて -1、0、1。
- `int8.band(a: Int8, b: Int8) -> Int8` — ビット単位 AND。
- `int8.bor(a: Int8, b: Int8) -> Int8` — ビット単位 OR。
- `int8.bxor(a: Int8, b: Int8) -> Int8` — ビット単位 XOR。
- `int8.bnot(x: Int8) -> Int8` — 8ビットすべてをビット反転します。
- `int8.bshl(a: Int8, n: Int) -> Int8` — a を n ビット左シフトします。n は8を法とします。
- `int8.bshr(a: Int8, n: Int) -> Int8` — 符号埋めの右シフト。n は8を法とします。
- `int8.count_leading_zeros(x: Int8) -> Int` — 8ビット内で、最上位の立っているビットより上のゼロビット数。0 の場合は8。
- `int8.count_trailing_zeros(x: Int8) -> Int` — 最下位の立っているビットより下のゼロビット数。0 の場合は8。
- `int8.count_ones(x: Int8) -> Int` — 8ビット中の立っているビット数。
- `int8.byte_swap(x: Int8) -> Int8` — バイト順を反転します。1バイトなので x 自身です。
- `int8.wrap_add(a: Int8, b: Int8) -> Int8` — a + b を8ビットに折り返します。
- `int8.wrap_sub(a: Int8, b: Int8) -> Int8` — a - b を8ビットに折り返します。
- `int8.wrap_mul(a: Int8, b: Int8) -> Int8` — a * b を8ビットに折り返します。
- `int8.rotate_left(a: Int8, n: Int) -> Int8` — a の8ビットを左に n 回転します。n は8を法とします。
- `int8.rotate_right(a: Int8, n: Int) -> Int8` — a の8ビットを右に n 回転します。n は8を法とします。
- `int8.bit_length(x: Int8) -> Int` — x を格納するのに必要なビット数。0 は0、負数は8。
- `int8.parse(s: String) -> Result[Int8, String]` — s の前後の ASCII 空白を除去して10進 Int8 にします。不正な文字や -128..127 の範囲外は err。
- `int8.to_string_radix(x: Int8, radix: Int) -> String` — x を radix 進の小文字数字で表し、負数には - を付けます。radix が2..36の範囲外なら中断します。
- `int8.parse_radix(s: String, radix: Int) -> Result[Int8, String]` — 前後の空白を除去した s を radix 進の Int8 にします。構文は int.parse_radix と同じです。不正な文字や -128..127 の範囲外は err。
- `int8.from(x: S) -> Int8` — x をゼロ方向に丸めて Int8 にします。NaN、無限大、Int8 の範囲外の値は中断します。
- `int8.from_checked(x: S) -> Option[Int8]` — Int8 が x を正確に保持できれば some(x as Int8)、それ以外（小数、NaN、範囲外）は none。
- `int8.from_saturating(x: S) -> Int8` — x をゼロ方向に丸めて Int8 にし、Int8 の範囲に制限します。NaN は0。
- `int8.from_wrapping(x: S) -> Int8` — 整数 x の下位8ビットを Int8 として扱います（2の補数）。

## io

[io.md](io.ja.md)

- `effect io.read_line() -> Option[String]` — stdin の次の行。改行を除去、不正UTF-8は U+FFFD。EOFは none、空行は some("")。
- `effect io.read_all() -> String` — stdin の残りをテキストとして取得。不正UTF-8は U+FFFD、EOFは空。
- `effect io.print(s: String) -> Unit` — s を stdout に改行なしで書き、その後フラッシュ。
- `effect io.eprint(s: String) -> Unit` — s を stderr に改行なしで書く。
- `effect io.stdin() -> Stdin` — 標準入力。io.read / read_exact / read_to_end 用。
- `effect io.stdout() -> Stdout` — 標準出力。io.write 用。
- `effect io.stderr() -> Stderr` — 標準エラー。io.write 用。
- `effect io.read(h: H, max: Int) -> Bytes` — h から最大 max バイト。h 終了後は空。max < 0 なら err。
- `effect io.read_exact(h: H, n: Int) -> Bytes` — h から正確に n バイト。先に h が終了したら届いた数を示す err。
- `effect io.read_to_end(h: H) -> Bytes` — h 終了まで全バイト。
- `effect io.write(h: H, data: Bytes) -> Unit` — data 全体を h に書く。
- `effect io.close(h: H) -> Unit` — h を閉じる。2回閉じても成功。

## json

[json.md](json.ja.md)

- `json.parse(text: String) -> Result[Value, String]` — 厳密な JSON Value または err。末尾の空白以外のテキストと1000を超える入れ子はエラー。
- `json.stringify(v: Value) -> String` — コンパクトな JSON。Float 1.0 は1、NaN/inf は null と表示。
- `json.stringify_pretty(v: Value) -> String` — 2スペースインデントの JSON。[] と {} はインライン、NaN/inf は null。

## list

[list.md](list.ja.md)

- `list.len(xs: List[A]) -> Int` — 要素数。空のリストでは 0。
- `list.length(xs: List[A]) -> Int   (deprecated — use list.len)` — list.len の別名。
- `list.get(xs: List[A], i: Int) -> Option[A]` — インデックス i の要素。範囲外なら none。
- `list.set(xs: List[A], i: Int, x: A) -> List[A]` — スロット i = x としたコピー。i が範囲外なら何もしない。
- `list.swap(xs: List[A], i: Int, j: Int) -> List[A]` — i と j を交換したコピー。どちらかが範囲外なら何もしない。
- `list.sort(xs: List[A]) -> List[A]` — 昇順のコピー。Float は IEEE totalOrder に従う。
- `list.reverse(xs: List[A]) -> List[A]` — 要素を逆順にする。
- `list.contains(xs: List[A], x: A) -> Bool` — いずれかの要素が == x なら true。NaN は決して一致しない。
- `list.enumerate(xs: List[A]) -> List[(Int, A)]` — （インデックス、要素）の組。インデックスは 0 から。
- `list.zip(xs: List[A], ys: List[B]) -> List[(A, B)]` — 位置ごとの組。短い方のリストの終わりで止まる。
- `list.flatten(xs: List[List[A]]) -> List[A]` — 内側のリストを順に連結する。
- `list.take(xs: List[A], n: Int) -> List[A]` — 先頭の n 要素。n >= len なら全体、n < 0 なら []。
- `list.drop(xs: List[A], n: Int) -> List[A]` — 先頭の n 要素を除いた全体。n >= len なら []、n < 0 なら全体。
- `list.unique(xs: List[A]) -> List[A]` — 重複を除去し、最初の出現を残す。
- `list.index_of(xs: List[A], x: A) -> Option[Int]` — 最初の x のインデックス。なければ none。
- `list.last(xs: List[A]) -> Option[A]` — 最後の要素。空なら none。
- `list.chunk(xs: List[A], n: Int) -> List[List[A]]` — n 要素ずつの連なり。最後は短い場合がある。n <= 0 なら実行を中断する。
- `list.sum(xs: List[A]) -> A` — 任意の数値の和（R-L0-42）。空なら 0。整数はオーバーフロー時にラップする。
- `list.product(xs: List[A]) -> A` — 任意の数値の積（R-L0-42）。空なら 1。整数はオーバーフロー時にラップする。
- `list.first(xs: List[A]) -> Option[A]` — 先頭の要素。空なら none。
- `list.is_empty(xs: List[A]) -> Bool` — xs に要素がない場合に限り true。
- `list.min(xs: List[A]) -> Option[A]` — 最小の要素。空なら none。
- `list.max(xs: List[A]) -> Option[A]` — 最大の要素。空なら none。
- `list.join(xs: List[T], sep: T) -> T` — 各部分を sep で結合（R-L0-42）。String または Bytes。[] なら空。
- `list.map(xs: List[A], f: (A) -> B) -> List[B]` — 各要素に順に f を適用する。
- `list.filter(xs: List[A], f: (A) -> Bool) -> List[A]` — f が true となる要素。順序は保持する。
- `list.find(xs: List[A], f: (A) -> Bool) -> Option[A]` — f が成り立つ最初の要素。なければ none。
- `list.any(xs: List[A], f: (A) -> Bool) -> Bool` — いずれかの x で f が成り立つなら true。空なら false。
- `list.all(xs: List[A], f: (A) -> Bool) -> Bool` — すべての x で f が成り立つなら true。空なら true。
- `list.count(xs: List[A], f: (A) -> Bool) -> Int` — f が成り立つ要素の数。
- `list.flat_map(xs: List[A], f: (A) -> List[B]) -> List[B]` — f(x) のリストを順に連結する。
- `list.filter_map(xs: List[A], f: (A) -> Option[B]) -> List[B]` — f(x) の some 値を順に返す。
- `list.fold(xs: List[A], init: B, f: (B, A) -> B) -> B` — init からの左畳み込み。空なら init。
- `list.sort_by(xs: List[A], f: (A) -> B) -> List[A]` — キー f(x) による安定ソート。f は x ごとに 1 回実行される。
- `list.take_while(xs: List[A], f: (A) -> Bool) -> List[A]` — f が成り立つ最長の接頭部分。
- `list.drop_while(xs: List[A], f: (A) -> Bool) -> List[A]` — f が成り立つ最長の接頭部分を除いた残り。
- `list.partition(xs: List[A], f: (A) -> Bool) -> (List[A], List[A])` — （f が成り立つ要素、それ以外）。順序は保持する。
- `list.reduce(xs: List[A], f: (A, A) -> A) -> Option[A]` — xs[0] を初期値とする左畳み込み。空なら none。
- `list.group_by(xs: List[A], f: (A) -> B) -> Map[B, List[A]]` — キー f(x) → 要素。キーは最初に現れた順。
- `list.find_index(xs: List[A], f: (A) -> Bool) -> Option[Int]` — f が成り立つ最初の x のインデックス。なければ none。
- `list.update(xs: List[A], i: Int, f: (A) -> A) -> List[A]` — スロット i = f(old) としたコピー。i が範囲外なら何もしない。
- `list.scan(xs: List[A], init: B, f: (B, A) -> B) -> List[B]` — 畳み込みの途中の値。init は含めない。
- `list.zip_with(xs: List[A], ys: List[B], f: (A, B) -> C) -> List[C]` — 組に f を適用する。短い方のリストの終わりで止まる。
- `list.unique_by(xs: List[A], f: (A) -> K) -> List[A]` — 異なるキー f(x) ごとに最初の要素。
- `list.range(start: Int, end: Int) -> List[Int]` — start から end の手前まで。end <= start なら []。
- `list.slice(xs: List[A], start: Int, end: Int) -> List[A]` — インデックス start..<end。それぞれを 0..len に収める。start >= end なら []。
- `list.insert(xs: List[A], i: Int, x: A) -> List[A]` — i の位置に x を置いたコピー。i >= len または i < 0 なら末尾に追加。
- `list.remove_at(xs: List[A], i: Int) -> List[A]` — スロット i を除いたコピー。i が範囲外なら何もしない。
- `list.repeat(x: A, n: Int) -> List[A]` — x を n 個並べる。n <= 0 なら []。
- `list.intersperse(xs: List[A], sep: A) -> List[A]` — 隣接する要素の間に sep を置く。
- `list.dedup(xs: List[A]) -> List[A]` — 隣接して等しい連なりをまとめる。全体での重複除去ではない。
- `list.take_end(xs: List[A], n: Int) -> List[A]` — 末尾の n 要素。n >= len なら全体、n < 0 なら []。
- `list.drop_end(xs: List[A], n: Int) -> List[A]` — 末尾の n 要素を除いた全体。n >= len なら []、n < 0 なら全体。
- `list.window(xs: List[A], n: Int) -> List[List[A]]` — 長さ n のすべての連続部分。n > len なら []。n <= 0 なら実行を中断する。
- `list.binary_search(xs: List[A], target: A) -> Option[Int]` — 昇順（list.sort の順序）にソートされた xs 内の target のインデックス。なければ none。
- `list.push(mut xs: List[A], x: A) -> Unit` — xs の末尾に x をインプレースで追加する。
- `list.with_capacity(cap: Int) -> List[A]` — 空のリスト。cap は事前割り当てのヒントにすぎない。
- `list.pop(mut xs: List[A]) -> Option[A]` — 最後の要素を削除して返す。空なら none。
- `list.clear(mut xs: List[A]) -> Unit` — xs の全要素をインプレースで削除する。
- `list.split_at(xs: List[A], n: Int) -> (List[A], List[A])` — （先頭 n 要素、先頭 n 要素を除いた残り）。n が末尾を超えるなら (xs, [])、n < 0 なら ([], xs)。
- `list.iterate(seed: A, f: (A) -> A, n: Int) -> List[A]` — 長さ n の [seed, f(seed), ...]。n <= 0 なら []。
- `list.find_map(xs: List[A], f: (A) -> Option[B]) -> Option[B]` — f が返す最初の some。なければ none。見つかった後は f を呼ばない。
- `list.new() -> List[A]` — 空のリスト。[] と同じ値。
- `list.from_list(xs: List[A]) -> List[A]` — xs 自体。リストからの Container コンストラクター。
- `list.to_list(xs: List[A]) -> List[A]` — xs 自体。リストとしての Container ビュー。
- `list.sort_with(xs: List[A], cmp: (A, A) -> Int) -> List[A]` — cmp による安定ソート。負なら a を先にし、0 なら入力順を維持する。
- `list.min_by(xs: List[A], f: (A) -> K) -> Option[A]` — キー f(x) が最小の要素。同点なら最初のもの。f は x ごとに 1 回実行される。
- `list.max_by(xs: List[A], f: (A) -> K) -> Option[A]` — キー f(x) が最大の要素。同点なら最初のもの。f は x ごとに 1 回実行される。
- `list.min_with(xs: List[A], cmp: (A, A) -> Int) -> Option[A]` — cmp による最小の要素。同点なら最初のもの。空なら none。
- `list.max_with(xs: List[A], cmp: (A, A) -> Int) -> Option[A]` — cmp による最大の要素。同点なら最初のもの。空なら none。
- `list.binary_search_by(xs: List[A], target: K, f: (A) -> K) -> Option[Int]` — キー順にソートされた xs のキー f(x) における target のインデックス。なければ none。
- `list.binary_search_with(xs: List[A], target: A, cmp: (A, A) -> Int) -> Option[Int]` — cmp でソートされた xs 内の target のインデックス。なければ none。cmp(x, target) が呼ばれる。
- `list.unzip(xs: List[(A, B)]) -> (List[A], List[B])` — 組のリストを（第 1 要素群、第 2 要素群）にする。zip の逆。
- `list.range_step(start: Int, end: Int, step: Int) -> List[Int]` — start, start + step, ... と end の手前まで。step < 0 なら減少する。step = 0 なら実行を中断する。

## map

[map.md](map.ja.md)

- `map.new() -> Map[K, V]` — 空の Map[K, V]。
- `map.get(m: Map[K, V], key: K) -> Option[V]` — key に対応する値。なければ none。
- `map.set(m: Map[K, V], key: K, value: V) -> Map[K, V]` — key → value としたコピー。既存キーは上書きし、位置を維持します。
- `map.contains(m: Map[K, V], key: K) -> Bool` — key があれば true。
- `map.remove(m: Map[K, V], key: K) -> Map[K, V]` — key を除いたコピー。なければ変更なし。
- `map.keys(m: Map[K, V]) -> List[K]` — 全キーを挿入順で返します。
- `map.values(m: Map[K, V]) -> List[V]` — 全値をキーの挿入順で返します。
- `map.len(m: Map[K, V]) -> Int` — entry 数。空の map では 0。
- `map.to_list(m: Map[K, V]) -> List[(K, V)]` — (key, value) の組を挿入順で返します。
- `map.merge(a: Map[K, V], b: Map[K, V]) -> Map[K, V]` — a に b の entry を追加。共通キーでは b が優先。
- `map.is_empty(m: Map[K, V]) -> Bool` — m に entry がない場合に限り true。
- `map.from_list(xs: List[(K, V)]) -> Map[K, V]` — pairs xs から map を作ります。重複キーは最後の値を採用します。
- `map.map(m: Map[K, V], f: (V) -> B) -> Map[K, B]` — キーと順序はそのまま、値を f(v) に置換します。
- `map.filter(m: Map[K, V], f: ((K, V)) -> Bool) -> Map[K, V]` — f((k, v)) が成立する entry。順序を保持します。
- `map.fold(m: Map[K, V], init: A, f: (A, (K, V)) -> A) -> A` — 挿入順で f(acc, (k, v)) を fold します。
- `map.any(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool` — f((k, v)) が成立するものがあれば true。空なら false。
- `map.all(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool` — 全 f((k, v)) が成立すれば true。空なら true。
- `map.count(m: Map[K, V], f: ((K, V)) -> Bool) -> Int` — f((k, v)) が成立する entry 数。
- `map.find(m: Map[K, V], f: ((K, V)) -> Bool) -> Option[(K, V)]` — f が成立する最初の (k, v)。なければ none。
- `map.update(m: Map[K, V], key: K, f: (V) -> V) -> Map[K, V]` — key の値を f(v) にしたコピー。キーがなければ何もしません。
- `map.upsert(m: Map[K, V], key: K, init: V, f: (V) -> V) -> Map[K, V]` — 存在すれば key → f(old)、なければ init を末尾に追加します。
- `map.clear(mut m: Map[K, V]) -> Unit` — m の全 entry をその場で削除します。
- `map.enumerate(m: Map[K, V]) -> List[(Int, (K, V))]` — (index, (key, value)) の組を挿入順で返します。index は 0 から。
- `map.zip(m: Map[K, V], ys: List[B]) -> List[((K, V), B)]` — entry と ys を位置ごとに組にします。短い方で終了します。
- `map.zip_with(m: Map[K, V], ys: List[B], f: ((K, V), B) -> C) -> List[C]` — (entry, ys の要素) の組に f を適用します。短い方で終了します。
- `map.filter_map(m: Map[K, V], f: ((K, V)) -> Option[B]) -> List[B]` — entry に f を適用して得た some を挿入順で返します。
- `map.find_map(m: Map[K, V], f: ((K, V)) -> Option[B]) -> Option[B]` — entry に f を適用して得た最初の some。なければ none。一致後は f を呼びません。
- `map.reduce(m: Map[K, V], f: ((K, V), (K, V)) -> (K, V)) -> Option[(K, V)]` — 最初の entry を初期値とする entry の左 fold。空なら none。
- `map.scan(m: Map[K, V], init: B, f: (B, (K, V)) -> B) -> List[B]` — entry の fold の途中値。init は含みません。
- `map.partition(m: Map[K, V], f: ((K, V)) -> Bool) -> (List[(K, V)], List[(K, V)])` — (f が成立する entry、それ以外)。挿入順を保持します。
- `map.group_by(m: Map[K, V], f: ((K, V)) -> B) -> Map[B, List[(K, V)]]` — キー f(entry) → entry 群。キーは最初に現れた順序。
- `map.sort_by(m: Map[K, V], f: ((K, V)) -> B) -> List[(K, V)]` — キー f(entry) で entry を安定ソートします。f は entry ごとに一回実行します。
- `map.sort_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> List[(K, V)]` — cmp で entry を安定ソートします。負なら a が先。
- `map.min_by(m: Map[K, V], f: ((K, V)) -> U) -> Option[(K, V)]` — f(entry) が最小の entry。同値なら最初のもの。
- `map.max_by(m: Map[K, V], f: ((K, V)) -> U) -> Option[(K, V)]` — f(entry) が最大の entry。同値なら最初のもの。
- `map.min_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> Option[(K, V)]` — cmp で最小となる entry。同値なら最初のもの。
- `map.max_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> Option[(K, V)]` — cmp で最大となる entry。同値なら最初のもの。

## math

[math.md](math.ja.md)

- `math.atan(x: Float) -> Float` — ラジアンの逆正接。-pi/2..pi/2 内。
- `math.choose(n: Int, k: Int) -> Int` — 二項係数 n over k。k < 0 または k > n は 0。overflow は wrap。
- `math.cos(x: Float) -> Float` — x ラジアンの余弦。無限大の x は NaN。
- `math.e() -> Float` — ネイピア数 2.718281828459045。
- `math.exp(x: Float) -> Float` — e の x 乗。約 709.78 超で inf に overflow。
- `math.factorial(n: Int) -> Int` — 1..n の積。n <= 0 は 1。20 超で wrap、66 以降は 0。
- `math.log(x: Float) -> Float` — 自然対数。0 は -inf、負の x は NaN。
- `math.log10(x: Float) -> Float` — 底 10 の対数。0 は -inf、負の x は NaN。
- `math.log2(x: Float) -> Float` — 底 2 の対数。0 は -inf、負の x は NaN。
- `math.log_gamma(x: Float) -> Float` — 実数全域の ln|Gamma(x)|。極では inf。
- `math.pi() -> Float` — 円周率 3.141592653589793。
- `math.sin(x: Float) -> Float` — x ラジアンの正弦。無限大の x は NaN。
- `math.sqrt(x: Float) -> Float` — 平方根。0 未満は NaN、-0.0 は -0.0 のまま。
- `math.tan(x: Float) -> Float` — x ラジアンの正接。無限大の x は NaN。
- `math.tanh(x: Float) -> Float` — -1..1 の双曲線正接。inf は 1.0。
- `math.asin(x: Float) -> Float` — ラジアンの逆正弦。-pi/2..pi/2 内。-1..1 外は NaN。
- `math.acos(x: Float) -> Float` — ラジアンの逆余弦。0..pi 内。-1..1 外は NaN。
- `math.atan2(y: Float, x: Float) -> Float` — 点 (x, y) の角度をラジアンで返します。-pi..pi 内。
- `math.expm1(x: Float) -> Float` — e の x 乗から 1 を引いた値。0 付近で精度を保ち、約 -38 未満は -1.0。
- `math.log1p(x: Float) -> Float` — 1 + x の自然対数。0 付近で精度を保ち、-1 は -inf、それ未満は NaN。
- `math.sinh(x: Float) -> Float` — 双曲線正弦。約 710 を超えると +-inf に overflow。
- `math.cosh(x: Float) -> Float` — 双曲線余弦。少なくとも 1.0。約 710 を超えると inf に overflow。
- `math.hypot(a: Float, b: Float) -> Float` — 途中で overflow や underflow を起こさず sqrt(a*a + b*b) を求めます。
- `math.cbrt(x: Float) -> Float` — 実立方根。負の x には負の根。
- `math.asinh(x: Float) -> Float` — 逆双曲線正弦。
- `math.acosh(x: Float) -> Float` — 逆双曲線余弦。少なくとも 0.0。1.0 未満は NaN。
- `math.atanh(x: Float) -> Float` — 逆双曲線正接。+-1 は +-inf、-1..1 外は NaN。

## matrix

[matrix.md](matrix.ja.md)

- `matrix.zeros(rows: Int, cols: Int) -> Matrix` — rows × cols の0.0。負数は0に制限。
- `matrix.ones(rows: Int, cols: Int) -> Matrix` — rows × cols の1.0。負数は0に制限。
- `matrix.shape(m: Matrix) -> (Int, Int)` — (rows, cols)。行がなければ (0, 0)。
- `matrix.transpose(m: Matrix) -> Matrix` — cols × rows のコピー。r × 0 は0 × 0になります。
- `matrix.from_lists(xs: List[List[Float]]) -> Matrix` — 等長の行 xs からなる行列。[] は0 × 0。
- `matrix.to_lists(m: Matrix) -> List[List[Float]]` — 行をリストにします。r × 0 は r 個の空行。
- `matrix.get(m: Matrix, row: Int, col: Int) -> Option[Float]` — (row, col) の要素。範囲外なら none。
- `matrix.rows(m: Matrix) -> Int` — 行数。空の行列なら0。
- `matrix.cols(m: Matrix) -> Int` — 行0の幅。行がなければ0。
- `matrix.scale(m: Matrix, s: Float) -> Matrix` — 全要素を s 倍。
- `matrix.map(m: Matrix, f: (Float) -> Float) -> Matrix` — 全要素に f を適用。
- `matrix.from_bytes_f32_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]` — b の offset から f32 LE の rows × cols。b が短ければ none。
- `matrix.to_bytes_f64_le(m: Matrix) -> Bytes` — 行優先の f64 LE。要素あたり8バイト。
- `matrix.to_bytes_f32_le(m: Matrix) -> Bytes` — 行優先の f32 LE。各4バイト（丸め）。
- `matrix.to_bytes_f16_le(m: Matrix) -> Bytes` — 行優先の f16 LE。各2バイト（最近接偶数丸め、オーバーフローは inf）。
- `matrix.from_bytes_f16_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]` — b の offset から f16 LE の rows × cols。b が短ければ none。
- `matrix.from_bytes_f64_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]` — b の offset から f64 LE の rows × cols。b が短ければ none。
- `matrix.broadcast_add_row(m: Matrix, bias: List[Float]) -> Matrix` — 各行に bias を加え、列数を bias に合わせて切り詰めます。
- `matrix.split_cols_even(m: Matrix, n: Int) -> List[Matrix]` — 幅 cols/n の n 個の切片。余りは捨てます。n = 0 なら []。
- `matrix.concat_cols(xs: List[Matrix]) -> Matrix` — 横並びに結合。[] は0 × 0。
- `matrix.concat_cols_many(matrices: List[Matrix]) -> Matrix   (deprecated — use matrix.concat_cols)` — matrix.concat_cols の別名。
- `matrix.slice_rows(m: Matrix, start: Int, end: Int) -> Matrix` — 行 [start, end)。各境界を0..行数に制限。
- `matrix.gather_rows(m: Matrix, indices: List[Int]) -> Matrix` — indices の行。範囲外ならゼロ行。
- `matrix.dot_row(m: Matrix, r: Int, vec: List[Float]) -> Float` — 行 r と vec の内積。r が範囲外なら0.0。
- `matrix.row_dot(m: Matrix, r: Int, vec: List[Float]) -> Float   (deprecated — use matrix.dot_row)` — matrix.dot_row の別名。
- `matrix.zip_with(a: Matrix, b: Matrix, f: (Float, Float) -> Float) -> Matrix` — 同位置の各要素対に f を適用。形状不一致は中断。
- `matrix.concat_rows(xs: List[Matrix]) -> Matrix` — 各行列の行を上から下へ並べます。空の要素は飛ばし、幅不一致は中断。

## net

[net.md](net.ja.md)

- `effect net.tcp_connect(host: String, port: Int) -> TcpStream` — host:port に接続したストリーム。拒否なら err。
- `effect net.tcp_is_open(stream: TcpStream) -> Bool` — io.close 後 false。相手のクローズは見えない。
- `effect net.tcp_read_timeout(stream: TcpStream, len: Int, timeout_ms: Int) -> Bytes` — timeout_ms（> 0）以内に最大 len バイト。その後は元のタイムアウトに戻る。
- `effect net.tcp_set_timeout(stream: TcpStream, timeout_ms: Int) -> Unit` — 読書きタイムアウトを設定。<= 0 で解除。
- `effect net.tcp_available(stream: TcpStream) -> Int` — 現在読めるバイト数、最大 65536。なければ 0。
- `effect net.tcp_listen(host: String, port: Int) -> TcpListener` — host:port にバインドしたリスナー。使用中なら err。0 は OS が選ぶ。
- `effect net.tcp_local_port(listener: TcpListener) -> Int` — リスナーのバインド先ポート。0 で待受けた場合は OS が選んだ値。
- `effect net.tcp_accept(listener: TcpListener) -> TcpStream` — 次のクライアントを待ってブロックし、そのストリームを返す。
- `effect net.udp_bind(host: String, port: Int) -> UdpSocket` — host:port にバインドした UDP ソケット。0 は OS が選ぶ。
- `effect net.udp_local_port(sock: UdpSocket) -> Int` — sock のバインド先ポート。
- `effect net.udp_send_to(sock: UdpSocket, data: Bytes, host: String, port: Int) -> Unit` — data を 1 データグラムとして host:port に送る。
- `effect net.udp_recv_from(sock: UdpSocket, max: Int) -> UdpPacket` — 次のデータグラムを待ち、最大 max バイトと送信元を返す。
- `effect net.lookup_host(host: String) -> List[IpAddr]` — host の解決先アドレスを各 1 回。なければ err。
- `net.parse_ip(s: String) -> Result[IpAddr, String]` — s が表す IP。ドット区切り IPv4 または IPv6（`::` と IPv4 末尾可）。
- `net.ip_to_string(ip: IpAddr) -> String` — ip のテキスト。ドット区切り IPv4 または RFC 5952 短縮 IPv6。
- `net.is_loopback(ip: IpAddr) -> Bool` — 127.0.0.0/8 と ::1 なら true。

## option

[option.md](option.ja.md)

- `option.map(o: Option[A], f: (A) -> B) -> Option[B]` — some(v) には some(f(v))、none は none のまま。
- `option.flat_map(o: Option[A], f: (A) -> Option[B]) -> Option[B]` — some(v) には f(v)、none は none のまま。
- `option.flatten(o: Option[Option[A]]) -> Option[A]` — some(inner) の内側の option。none には none。
- `option.unwrap_or(o: Option[A], default: A) -> A   (deprecated — use ??)` — some(v) には v、それ以外は default（常に評価）。
- `option.unwrap_or_else(o: Option[A], f: () -> A) -> A` — some(v) には v、それ以外は f()。f は none の場合だけ実行します。
- `option.is_some(o: Option[A]) -> Bool` — some(_) には true、none には false。
- `option.is_none(o: Option[A]) -> Bool` — none には true、some(_) には false。
- `option.to_result(o: Option[A], e: E) -> Result[A, E]` — some(v) には ok(v)、none には err(e)。
- `option.filter(o: Option[A], f: (A) -> Bool) -> Option[A]` — some(v) で f(v) が成り立つなら o、それ以外は none。
- `option.zip(a: Option[A], b: Option[B]) -> Option[(A, B)]` — 両方が some なら some((x, y))、それ以外は none。
- `option.or_else(o: Option[A], f: () -> Option[A]) -> Option[A]` — some なら o、それ以外は f()。f は none の場合だけ実行します。
- `option.to_list(o: Option[A]) -> List[A]` — some(v) には [v]、none には []。
- `option.collect(xs: List[Option[A]]) -> Option[List[A]]` — none の要素がなければ some(values)、それ以外は none。
- `option.collect_map(xs: List[A], f: (A) -> Option[B]) -> Option[List[B]]` — すべての f(x) を含む some、または none。f はすべての x に対して実行します。

## path

[path.md](path.ja.md)

- `path.join(base: String, child: String) -> String` — base/child。絶対パスの child は base を置き換える。
- `path.join_within(base: String, child: String) -> Result[String, String]` — base/child。child が base 外へ出られる場合は err。
- `path.dirname(path: String) -> String` — 最後のセグメントを除いた path。スラッシュがなければ ""。
- `path.basename(path: String) -> String` — 最後の空でないセグメント。/ または空パスなら ""。
- `path.extension(path: String) -> Option[String]` — ベース名の最後のドットの後。ドットなし、またはドットファイルなら none。
- `path.is_absolute(path: String) -> Bool` — path が / から始まると true。
- `path.stem(path: String) -> String` — 最後の拡張子を除いたベース名。ドットファイルはそのまま。
- `path.normalize(path: String) -> String` — . と .. を字句的に解決。"" は "." になる。
- `path.components(path: String) -> List[String]` — normalize(path) のセグメント。絶対パスなら先頭は "/"。"" または "." なら ["."]。
- `path.relative_to(path: String, base: String) -> Option[String]` — base に対する path の字句的な相対パス。等しければ "."、base 配下でなければ none。
- `path.with_extension(path: String, ext: String) -> String` — path の最後の拡張子を ext に置き換える。"" なら削除。ファイル名がなければ変更なし。

## process

[process.md](process.ja.md)

- `effect process.exec(cmd: String, args: List[String]) -> String` — 終了 0 なら stdout、それ以外は stderr テキストを含む err。
- `effect process.output(c: ProcessCommand) -> ProcessStatus` — c を最後まで実行。どの終了コードも ok。起動不能または期限超過なら err。
- `effect process.run(c: ProcessCommand) -> Int` — c をこの端末上で実行（c.stdin 未設定なら stdin も）。終了コードを返し、シグナル終了なら -1。
- `effect process.start(c: ProcessCommand) -> ProcessChild` — c を独自のプロセスグループで起動して直ちに返る。出力を取得する。
- `effect process.poll(c: ProcessChild) -> Option[Result[ProcessStatus, String]]` — ブロックしない。実行中は none、その後は結果（期限超過なら err）。
- `effect process.wait(c: ProcessChild) -> ProcessStatus` — c 終了までブロックし、その結果を返す。
- `effect process.stop(c: ProcessChild, grace_ms: Int) -> ProcessStatus` — c のグループへ SIGTERM、grace_ms 待機後 SIGKILL。終了後の再呼出しは同じ結果。
- `effect process.read_new_stdout(c: ProcessChild) -> String` — 前回読取り以降 c が stdout に書いた内容。ブロックしない。
- `effect process.read_new_stderr(c: ProcessChild) -> String` — 前回読取り以降 c が stderr に書いた内容。ブロックしない。
- `effect process.child_pid(c: ProcessChild) -> Int` — c の OS プロセス ID。process.kill 用。
- `effect process.kill(pid: Int, signal: Int) -> Unit` — kill(2) で pid（> 0）に signal（0..=31）を送る。err は理由を示す。
- `effect process.exit(code: Int) -> Never` — 0..=255 の code で終了。それ以外は abort、終了 1。
- `effect process.pid() -> Int` — このプログラムの OS プロセス ID。

## random

[random.md](random.ja.md)

- `effect random.int(min: Int, max: Int) -> Int` — [min, max] のランダムな Int。max <= min なら min。秘密には使わず random.bytes を使います。
- `effect random.float() -> Float` — [0, 1) のランダムな Float。秘密には使わず random.bytes を使います。
- `effect random.choice(xs: List[A]) -> Option[A]` — xs のランダムな要素。空なら none。秘密には使わず random.bytes を使います。
- `effect random.shuffle(xs: List[A]) -> List[A]` — xs のランダム順列を新しいリストで返します。秘密には使わず random.bytes を使います。
- `effect random.bytes(n: Int) -> Bytes` — OS の暗号学的乱数源（CSPRNG）から n バイト。n < 0 は abort。
- `random.seeded(seed: Int) -> Rng` — seed により乱数列が固定される generator。
- `random.next_int(rng: Rng, min: Int, max: Int) -> (Int, Rng)` — [min, max]（両端を含む）の Int と次の状態。max <= min なら min を返し、状態は変更しません。
- `random.next_float(rng: Rng) -> (Float, Rng)` — 上位 53 ビットからの [0, 1) の Float と次の状態。
- `random.next_choice(rng: Rng, xs: List[A]) -> (Option[A], Rng)` — 一様に選んだ要素と次の状態。xs が空なら none で状態変更なし。
- `random.next_shuffle(rng: Rng, xs: List[A]) -> (List[A], Rng)` — xs の一様な順列（Fisher-Yates）と次の状態。

## regex

[regex.md](regex.ja.md)

- `regex.new(pattern: String) -> Regex` — `pattern` の Regex。拒否時は `Error: invalid regex: <reason>` で abort。
- `regex.compile(pattern: String) -> Result[Regex, String]` — `pattern` の Regex、拒否時は err(reason)。
- `regex.is_match(re: Regex, s: String) -> Bool` — re が s のどこかに一致する。
- `regex.is_full_match(re: Regex, s: String) -> Bool` — re が s 全体に一致する。
- `regex.full_match(re: Regex, s: String) -> Bool   (deprecated — use regex.is_full_match)` — re が s 全体に一致する（regex.is_full_match へ改名）。
- `regex.find(re: Regex, s: String) -> Option[String]` — s 内の re の最左マッチ、なければ none。
- `regex.find_all(re: Regex, s: String) -> List[String]` — 重ならない全マッチを順に返す。
- `regex.index_of(re: Regex, s: String) -> Option[Int]` — 最左マッチ開始位置のコードポイント添字、なければ none。
- `regex.replace(re: Regex, s: String, rep: String) -> String` — 全マッチをリテラル rep で置換（`$1` は解釈しない）。
- `regex.replace_first(re: Regex, s: String, rep: String) -> String` — 最初のマッチをリテラル rep で置換。
- `regex.replace_with(re: Regex, s: String, f: (List[Option[String]]) -> String) -> String` — 全マッチを f([whole, group1, ...]) で置換。
- `regex.replace_first_with(re: Regex, s: String, f: (List[Option[String]]) -> String) -> String` — 最初のマッチを f([whole, group1, ...]) で置換。
- `regex.split(re: Regex, s: String) -> List[String]` — マッチ間のテキスト。なければ [s]。
- `regex.captures(re: Regex, s: String) -> Option[List[Option[String]]]` — 最初のマッチの [whole, group1, ...]、なければ none。参加しないグループは none。
- `regex.captures_all(re: Regex, s: String) -> List[List[Option[String]]]` — 各マッチの [whole, group1, ...] を順に返す。
- `regex.escape(s: String) -> String` — s の正規表現メタ文字をすべてエスケープし、自身に一致させる。

## result

[result.md](result.ja.md)

- `result.map(r: Result[A, E], f: (A) -> B) -> Result[B, E]` — ok(v) には ok(f(v))、err はそのまま通します。
- `result.map_err(r: Result[A, E], f: (E) -> F) -> Result[A, F]` — err(e) には err(f(e))、ok はそのまま通します。
- `result.flat_map(r: Result[A, E], f: (A) -> Result[B, E]) -> Result[B, E]` — ok(v) には f(v)、err はそのまま通します。
- `result.unwrap_or(r: Result[A, E], default: A) -> A   (deprecated — use ??)` — ok(v) には v、それ以外は default（常に評価）。
- `result.unwrap_or_else(r: Result[A, E], f: (E) -> A) -> A` — ok(v) には v、それ以外は f(e)。f は err の場合だけ実行します。
- `result.is_ok(r: Result[A, E]) -> Bool` — ok(_) には true、err(_) には false。
- `result.is_err(r: Result[A, E]) -> Bool` — err(_) には true、ok(_) には false。
- `result.to_option(r: Result[A, E]) -> Option[A]` — ok(v) には some(v)、err には none。e は破棄します。
- `result.to_err_option(r: Result[A, E]) -> Option[E]` — err(e) には some(e)、ok には none。
- `result.partition(xs: List[Result[A, E]]) -> (List[A], List[E])` — (ok の値、err の値)。各リストは元のリスト順です。
- `result.flatten(r: Result[Result[A, E], E]) -> Result[A, E]` — ok(inner) の内側の result。外側の err は保持します。
- `result.to_list(r: Result[A, E]) -> List[A]` — ok(v) には [v]、err には []。
- `result.zip(a: Result[A, E], b: Result[B, E]) -> Result[(A, B), E]` — 両方が ok なら ok((x, y))、それ以外は最初の err。
- `result.or_else(r: Result[A, E], f: (E) -> Result[A, F]) -> Result[A, F]` — ok なら r、それ以外は f(e)。f は err の場合だけ実行します。
- `result.filter(r: Result[A, E], f: (A) -> Bool, err_val: E) -> Result[A, E]` — ok に対して f が成り立たなければ err(err_val)。err は保持します。

## secret

[secret.md](secret.ja.md)

- `secret.new(x: A) -> Secret[A]` — x を包み、すべてのレンダラが `Secret(***)` と表示するようにする。
- `secret.reveal(s: Secret[A]) -> A` — s が包む値。

## set

[set.md](set.ja.md)

- `set.new() -> Set[A]` — 空の Set[A]。
- `set.from_list(xs: List[A]) -> Set[A]` — xs の重複しない要素を、最初に現れた順で返します。
- `set.insert(s: Set[A], x: A) -> Set[A]` — x を末尾に追加したコピー。すでにあれば変更なし。
- `set.remove(s: Set[A], x: A) -> Set[A]` — x を除いたコピー。なければ変更なし。
- `set.contains(s: Set[A], x: A) -> Bool` — x が要素なら true。
- `set.len(s: Set[A]) -> Int` — 要素数。空の set では 0。
- `set.is_empty(s: Set[A]) -> Bool` — s に要素がない場合に限り true。
- `set.to_list(s: Set[A]) -> List[A]` — 要素を挿入順で返します。
- `set.union(a: Set[A], b: Set[A]) -> Set[A]` — a の要素、その後に b の新しい要素。
- `set.intersection(a: Set[A], b: Set[A]) -> Set[A]` — a の要素のうち b にもあるもの。a の順序。
- `set.difference(a: Set[A], b: Set[A]) -> Set[A]` — a の要素のうち b にないもの。a の順序。
- `set.symmetric_difference(a: Set[A], b: Set[A]) -> Set[A]` — ちょうど一方だけにある要素。a のみの要素、その後に b のみの要素。
- `set.is_subset(a: Set[A], b: Set[A]) -> Bool` — a の全要素が b にあれば true。a が空でも true。
- `set.is_disjoint(a: Set[A], b: Set[A]) -> Bool` — a と b が要素を共有しなければ true。
- `set.filter(s: Set[A], f: (A) -> Bool) -> Set[A]` — f が成立する要素。順序を保持します。
- `set.map(s: Set[A], f: (A) -> B) -> Set[B]` — f(x) の set。f の結果が衝突すると要素数が減ります。
- `set.fold(s: Set[A], init: B, f: (B, A) -> B) -> B` — 挿入順で f(acc, x) を fold します。
- `set.any(s: Set[A], f: (A) -> Bool) -> Bool` — f が成立する要素があれば true。空なら false。
- `set.all(s: Set[A], f: (A) -> Bool) -> Bool` — 全要素で f が成立すれば true。空なら true。
- `set.count(s: Set[A], f: (A) -> Bool) -> Int` — f が成立する要素数。
- `set.find(s: Set[A], f: (A) -> Bool) -> Option[A]` — 挿入順で最初に f が成立する要素。なければ none。
- `set.clear(mut s: Set[A]) -> Unit` — s の全要素をその場で削除します。
- `set.enumerate(s: Set[A]) -> List[(Int, A)]` — (index, member) の組を挿入順で返します。index は 0 から。
- `set.zip(s: Set[A], ys: List[B]) -> List[(A, B)]` — 要素と ys を位置ごとに組にします。短い方で終了します。
- `set.zip_with(s: Set[A], ys: List[B], f: (A, B) -> C) -> List[C]` — (member, ys の要素) の組に f を適用します。短い方で終了します。
- `set.filter_map(s: Set[A], f: (A) -> Option[B]) -> List[B]` — f が返す some を挿入順で返します。
- `set.find_map(s: Set[A], f: (A) -> Option[B]) -> Option[B]` — f が返す最初の some。なければ none。一致後は f を呼びません。
- `set.reduce(s: Set[A], f: (A, A) -> A) -> Option[A]` — 最初の要素を初期値とする左 fold。空なら none。
- `set.scan(s: Set[A], init: B, f: (B, A) -> B) -> List[B]` — 挿入順の fold の途中値。init は含みません。
- `set.partition(s: Set[A], f: (A) -> Bool) -> (List[A], List[A])` — (f が成立する要素、それ以外)。挿入順を保持します。
- `set.group_by(s: Set[A], f: (A) -> B) -> Map[B, List[A]]` — キー f(x) → 要素群。キーは最初に現れた順序。
- `set.sort(s: Set[A]) -> List[A]` — 要素を昇順にします。Float は IEEE totalOrder。
- `set.sort_by(s: Set[A], f: (A) -> B) -> List[A]` — キー f(x) で要素を安定ソートします。f は要素ごとに一回実行します。
- `set.sort_with(s: Set[A], cmp: (A, A) -> Int) -> List[A]` — cmp で要素を安定ソートします。負なら a が先。
- `set.min(s: Set[A]) -> Option[A]` — 最小要素。空なら none。
- `set.max(s: Set[A]) -> Option[A]` — 最大要素。空なら none。
- `set.min_by(s: Set[A], f: (A) -> K) -> Option[A]` — キー f(x) が最小の要素。同値なら最初のもの。
- `set.max_by(s: Set[A], f: (A) -> K) -> Option[A]` — キー f(x) が最大の要素。同値なら最初のもの。
- `set.min_with(s: Set[A], cmp: (A, A) -> Int) -> Option[A]` — cmp で最小となる要素。同値なら最初のもの。
- `set.max_with(s: Set[A], cmp: (A, A) -> Int) -> Option[A]` — cmp で最大となる要素。同値なら最初のもの。
- `set.sum(s: Set[A]) -> A` — 要素の和。空なら 0。整数は overflow で wrap します。
- `set.product(s: Set[A]) -> A` — 要素の積。空なら 1。整数は overflow で wrap します。

## string

[string.md](string.ja.md)

- `string.trim(s: String) -> String` — s の先頭/末尾の空白を除く。
- `string.split(s: String, sep: String) -> List[String]` — sep の前後の断片。空の断片も保持する。
- `string.split_once(s: String, sep: String) -> Option[(String, String)]` — `sep` の最初の出現で分割して `(before, after)` を返す。`sep` がない場合に限り none。
- `string.len(s: String) -> Int` — コードポイント数（バイト数ではない）。
- `string.length(s: String) -> Int   (deprecated — use string.len)` — string.len の別名。
- `string.contains(s: String, needle: String) -> Bool` — s 内に needle があるか。needle が空なら true。
- `string.is_equal_constant_time(a: String, b: String) -> Bool` — a と b が同じ UTF-8 バイトを持つなら true。時間は長さだけに依存し、最初の相違バイトで早期終了しない。トークンや署名ヘッダーの比較に使う。`==` は最初の相違で返る。長さが異なれば即座に false（長さは公開情報）。
- `string.starts_with(s: String, prefix: String) -> Bool` — s が prefix で始まるか。"" は常に一致。
- `string.ends_with(s: String, suffix: String) -> Bool` — s が suffix で終わるか。"" は常に一致。
- `string.slice(s: String, start: Int, end: Int) -> String` — コードポイント区間 [start, end)。s の範囲に収める。
- `string.byte_slice(s: String, start: Int, end: Int) -> Option[String]` — バイト区間 [start, end)。不正または文字の途中なら none。
- `string.pad_start(s: String, n: Int, fill: String) -> String` — fill で左を埋め n 文字にする。切り詰めない。
- `string.capitalize(s: String) -> String` — 最初の文字を to_upper と同じ方法で大文字化し、残りはそのまま。
- `string.to_upper(s: String) -> String` — Unicode 大文字化。非 ASCII を ASCII へ変換しない。同一性比較用ではない。
- `string.to_lower(s: String) -> String` — Unicode 小文字化。非 ASCII を ASCII へ変換しない。同一性比較用ではない。
- `string.replace(s: String, from: String, to: String) -> String` — すべての from を to に置き換える。
- `string.get(s: String, i: Int) -> Option[String]` — i のコードポイント。範囲外なら none。
- `string.lines(s: String) -> List[String]` — \n または \r\n で分割し、行末は除く。
- `string.to_list(s: String) -> List[String]` — 各コードポイントを一文字の文字列にする。
- `string.index_of(s: String, needle: String) -> Option[Int]` — 最初の needle のコードポイントインデックス。なければ none。
- `string.repeat(s: String, n: Int) -> String` — s を n 回繰り返す。n <= 0 なら空。
- `string.is_digit(s: String) -> Bool` — すべて ASCII 0-9 か。s が空なら false。
- `string.is_alpha(s: String) -> Bool` — すべて Unicode の字母か。s が空なら false。
- `string.is_alphanumeric(s: String) -> Bool` — すべて字母/数字か。s が空なら false。
- `string.is_whitespace(s: String) -> Bool` — すべて空白か。s が空なら true。
- `string.is_upper(s: String) -> Bool` — 字母があり、すべて大文字。その他は無視。
- `string.is_lower(s: String) -> Bool` — 字母があり、すべて小文字。その他は無視。
- `string.to_codepoint(s: String) -> Option[Int]` — s がちょうど一コードポイントならスカラー値。それ以外は none。
- `string.from_codepoint(n: Int) -> Option[String]` — n の一文字文字列。n が Unicode スカラー値でなければ none。
- `string.pad_end(s: String, n: Int, fill: String) -> String` — fill で右を埋め n 文字にする。切り詰めない。
- `string.trim_start(s: String) -> String` — s の先頭空白を除く。
- `string.trim_end(s: String) -> String` — s の末尾空白を除く。
- `string.count_matches(s: String, needle: String) -> Int` — 重なり合わない needle の出現数。needle が空なら len+1。
- `string.count(s: String, f: (String) -> Bool) -> Int` — f が true になる文字の数。
- `string.is_empty(s: String) -> Bool` — "" だけで true。" " は空でない。
- `string.reverse(s: String) -> String` — コードポイントを逆順にする。書記素を考慮しない。
- `string.strip_prefix(s: String, prefix: String) -> Option[String]` — s から prefix を除く。なければ none。
- `string.strip_suffix(s: String, suffix: String) -> Option[String]` — s から suffix を除く。なければ none。
- `string.replace_first(s: String, from: String, to: String) -> String` — from があれば最初の出現を to へ置換。
- `string.last_index_of(s: String, needle: String) -> Option[Int]` — 最後の needle のコードポイントインデックス。なければ none。
- `string.first(s: String) -> Option[String]` — 先頭コードポイント。空なら none。
- `string.last(s: String) -> Option[String]` — 末尾コードポイント。空なら none。
- `string.take(s: String, n: Int) -> String` — 先頭 n コードポイント。n < 0 なら ""。
- `string.take_end(s: String, n: Int) -> String` — 末尾 n コードポイント。n < 0 なら ""。
- `string.drop(s: String, n: Int) -> String` — s の先頭 n コードポイントを除く。n < 0 なら s。
- `string.drop_end(s: String, n: Int) -> String` — s の末尾 n コードポイントを除く。n < 0 なら s。
- `string.push(mut s: String, x: String) -> Unit` — s に x をその場で追加する。
- `string.pop(mut s: String) -> Option[String]` — 最後の文字を除去して返す。s が空なら none。
- `string.clear(mut s: String) -> Unit` — s をその場で空にする。
- `string.compare(a: String, b: String) -> Int` — コードポイント順で a が b より前なら負、等しければ 0、後なら正。
- `string.byte_len(s: String) -> Int` — UTF-8 バイト数（string.len はコードポイント数）。
- `string.to_utf16(s: String) -> List[Int]` — UTF-16 コード単位。U+FFFF を超えるコードポイントはサロゲートペア。
- `string.from_utf16(xs: List[Int]) -> Result[String, String]` — UTF-16 コード単位から文字列を作る。対にならないサロゲート、または 0..65535 外の単位なら err。
- `string.filter(s: String, f: (String) -> Bool) -> String` — s のうち f が true になる文字。
- `string.fold(s: String, init: B, f: (B, String) -> B) -> B` — s の文字に左から右へ f を fold する。
- `string.insert(s: String, i: Int, x: String) -> String` — i 番目の文字の前に x を挿入した s。i が 0..=len 外なら abort。
- `string.remove_at(s: String, i: Int) -> String` — i 番目の文字を除いた s。i が 0..len 外なら abort。
- `string.new() -> String` — 空文字列。
- `string.from_list(xs: List[String]) -> String` — xs の文字列を間に何も挟まず結合する。
- `string.enumerate(s: String) -> List[(Int, String)]` — 0 からの (index, character) の組。
- `string.zip(s: String, ys: List[B]) -> List[(String, B)]` — 文字を ys と組にする。短い方で終了。
- `string.zip_with(s: String, ys: List[B], f: (String, B) -> C) -> List[C]` — 文字と ys の組に f を適用する。短い方で終了。
- `string.filter_map(s: String, f: (String) -> Option[B]) -> List[B]` — 各文字に f を適用した some の結果を順に返す。
- `string.find_map(s: String, f: (String) -> Option[B]) -> Option[B]` — 各文字に f を適用した最初の some の結果。なければ none。
- `string.reduce(s: String, f: (String, String) -> String) -> Option[String]` — 最初の文字を起点に f で fold。s が空なら none。
- `string.scan(s: String, init: B, f: (B, String) -> B) -> List[B]` — 文字を順に fold した途中結果。init は含まない。
- `string.partition(s: String, f: (String) -> Bool) -> (List[String], List[String])` — (f が true の文字、それ以外)。それぞれ順序を保持。
- `string.group_by(s: String, f: (String) -> B) -> Map[B, List[String]]` — f のキーで文字をグループ化し、各群の順序を保持。
- `string.sort(s: String) -> List[String]` — コードポイント順の文字。
- `string.sort_by(s: String, f: (String) -> B) -> List[String]` — f のキー順の文字。同じキーは順序を保持。
- `string.sort_with(s: String, cmp: (String, String) -> Int) -> List[String]` — cmp 順の文字（負なら第一引数が先）。安定ソート。
- `string.min(s: String) -> Option[String]` — コードポイント順で最小の文字。空なら none。
- `string.max(s: String) -> Option[String]` — コードポイント順で最大の文字。空なら none。
- `string.min_by(s: String, f: (String) -> K) -> Option[String]` — 最小のキーを持つ最初の文字。空なら none。
- `string.max_by(s: String, f: (String) -> K) -> Option[String]` — 最大のキーを持つ最後の文字。空なら none。
- `string.min_with(s: String, cmp: (String, String) -> Int) -> Option[String]` — cmp による最小の最初の文字。空なら none。
- `string.max_with(s: String, cmp: (String, String) -> Int) -> Option[String]` — cmp による最大の最後の文字。空なら none。
- `string.find_index(s: String, f: (String) -> Bool) -> Option[Int]` — f が true になる最初の文字のインデックス。なければ none。
- `string.take_while(s: String, f: (String) -> Bool) -> String` — すべての文字が f を満たす最長の接頭部分。
- `string.drop_while(s: String, f: (String) -> Bool) -> String` — すべての文字が f を満たす最長の接頭部分を除いた s。
- `string.chunk(s: String, n: Int) -> List[String]` — n 文字ずつの連続部分。最後は短くてもよい。n <= 0 なら abort。
- `string.window(s: String, n: Int) -> List[String]` — 連続する n 文字のすべての区間。n > len なら []。n <= 0 なら abort。
- `string.split_at(s: String, n: Int) -> (String, String)` — (先頭 n 文字、残り)。n が末尾を超えると (s, "")、n < 0 なら ("", s)。
- `string.dedup(s: String) -> String` — 隣接する同じ文字の各連続を一つにまとめた s。
- `string.intersperse(s: String, sep: String) -> String` — s の各隣接文字間に sep を挟む。
- `string.unique(s: String) -> String` — 各文字の最初の出現を順に返す。
- `string.unique_by(s: String, f: (String) -> K) -> String` — f が与える各キーについて最初の文字を順に返す。

## testing

[testing.md](testing.ja.md)

- `testing.assert_aborts(expected: String, f: () -> A) -> Unit` — 子で単独実行した f が expected を含むテキストで中止しなければ失敗。
- `testing.assert_contains(haystack: String, needle: String) -> Unit` — haystack に needle が含まれなければ失敗。空文字列 "" は合格。
- `testing.assert_approx(a: Float, b: Float, tolerance: Float) -> Unit` — abs(a - b) < tolerance（厳密な不等号）でなければ失敗。
- `testing.assert_gt(a: A, b: A) -> Unit` — a > b でなければ失敗。等しい場合も失敗。
- `testing.assert_lt(a: A, b: A) -> Unit` — a < b でなければ失敗。等しい場合も失敗。
- `testing.assert_some(o: Option[A]) -> Unit` — o が none の場合に失敗。
- `testing.assert_ok(r: Result[A, B]) -> Unit` — r が err の場合に失敗。
- `testing.assert_none(o: Option[A]) -> Unit` — o が some の場合に失敗。
- `testing.assert_err(r: Result[A, B]) -> Unit` — r が ok の場合に失敗。
- `testing.assert_snapshot(actual: String, expected: String) -> Unit` — actual == expected でなければ失敗。--update-snapshots で書き換え。

## uint16

[uint16.md](uint16.ja.md)

- `uint16.to_string(x: UInt16) -> String` — 10 進数字。符号は付かない。
- `uint16.min_value() -> UInt16` — 最小の UInt16：0。
- `uint16.max_value() -> UInt16` — 最大の UInt16：65535。
- `uint16.compare(a: UInt16, b: UInt16) -> Int` — a が b より小さい、等しい、大きい順に -1、0、1。
- `uint16.min(a: UInt16, b: UInt16) -> UInt16` — a と b の小さい方。
- `uint16.max(a: UInt16, b: UInt16) -> UInt16` — a と b の大きい方。
- `uint16.clamp(x: UInt16, lo: UInt16, hi: UInt16) -> UInt16` — x を lo..hi に収める。lo > hi なら abort。
- `uint16.floor_div(a: UInt16, b: UInt16) -> UInt16` — 商。符号なし型の a / b と同じ。b == 0 なら abort。
- `uint16.modulo(a: UInt16, b: UInt16) -> UInt16` — 剰余。符号なし型の a % b と同じ。b == 0 なら abort。
- `uint16.pow(base: UInt16, exp: Int) -> UInt16` — base の exp 乗。* と同様に 16 ビットへ折り返す。exp < 0 なら abort。
- `uint16.is_even(x: UInt16) -> Bool` — x が 2 の倍数なら true。
- `uint16.is_odd(x: UInt16) -> Bool` — x が 2 の倍数でなければ true。
- `uint16.band(a: UInt16, b: UInt16) -> UInt16` — ビット単位 AND。
- `uint16.bor(a: UInt16, b: UInt16) -> UInt16` — ビット単位 OR。
- `uint16.bxor(a: UInt16, b: UInt16) -> UInt16` — ビット単位 XOR。
- `uint16.bnot(x: UInt16) -> UInt16` — 16 ビットすべてをビット反転。
- `uint16.bshl(a: UInt16, n: Int) -> UInt16` — a を n ビット左シフト。n は 16 を法とする。
- `uint16.bshr(a: UInt16, n: Int) -> UInt16` — ゼロ埋め右シフト。n は 16 を法とする。
- `uint16.count_leading_zeros(x: UInt16) -> Int` — 16 ビット中の最上位の立ったビットより上のゼロ数。0 なら 16。
- `uint16.count_trailing_zeros(x: UInt16) -> Int` — 最下位の立ったビットより下のゼロ数。0 なら 16。
- `uint16.count_ones(x: UInt16) -> Int` — 16 ビット中の立っているビット数。
- `uint16.byte_swap(x: UInt16) -> UInt16` — 2 バイトを逆順にする（エンディアン反転）。
- `uint16.wrap_add(a: UInt16, b: UInt16) -> UInt16` — a + b を 16 ビットへ折り返す。
- `uint16.wrap_sub(a: UInt16, b: UInt16) -> UInt16` — a - b を 16 ビットへ折り返す。
- `uint16.wrap_mul(a: UInt16, b: UInt16) -> UInt16` — a * b を 16 ビットへ折り返す。
- `uint16.rotate_left(a: UInt16, n: Int) -> UInt16` — a の 16 ビットを n だけ左回転。n は 16 を法とする。
- `uint16.rotate_right(a: UInt16, n: Int) -> UInt16` — a の 16 ビットを n だけ右回転。n は 16 を法とする。
- `uint16.bit_length(x: UInt16) -> Int` — x を保持するのに必要なビット数。0 なら 0。
- `uint16.parse(s: String) -> Result[UInt16, String]` — 前後の ASCII 空白を除いた s から 10 進 UInt16 を解析。不正内容、- 符号、65535 超なら err。
- `uint16.to_string_radix(x: UInt16, radix: Int) -> String` — x を radix 進の小文字数字で表す。符号なし。radix が 2..36 外なら abort。
- `uint16.parse_radix(s: String, radix: Int) -> Result[UInt16, String]` — 前後の空白を除いた s を radix 進の UInt16 として解析。文法は int.parse_radix。不正内容、- 符号、65535 超なら err。
- `uint16.from(x: S) -> UInt16` — x をゼロ方向に丸め UInt16 にする。NaN、無限大、UInt16 範囲外なら abort。
- `uint16.from_checked(x: S) -> Option[UInt16]` — UInt16 が x を正確に保持できれば some(x as UInt16)、それ以外（小数部分、NaN、範囲外）は none。
- `uint16.from_saturating(x: S) -> UInt16` — x をゼロ方向に丸め UInt16 にし、UInt16 の範囲に収める。NaN は 0。
- `uint16.from_wrapping(x: S) -> UInt16` — 整数 x の下位 16 ビットを UInt16 とする（2 の補数）。

## uint32

[uint32.md](uint32.ja.md)

- `uint32.to_string(x: UInt32) -> String` — 10 進数字。符号は付かない。
- `uint32.min_value() -> UInt32` — 最小の UInt32：0。
- `uint32.max_value() -> UInt32` — 最大の UInt32：4294967295。
- `uint32.compare(a: UInt32, b: UInt32) -> Int` — a が b より小さい、等しい、大きい順に -1、0、1。
- `uint32.min(a: UInt32, b: UInt32) -> UInt32` — a と b の小さい方。
- `uint32.max(a: UInt32, b: UInt32) -> UInt32` — a と b の大きい方。
- `uint32.clamp(x: UInt32, lo: UInt32, hi: UInt32) -> UInt32` — x を lo..hi に収める。lo > hi なら abort。
- `uint32.floor_div(a: UInt32, b: UInt32) -> UInt32` — 商。符号なし型の a / b と同じ。b == 0 なら abort。
- `uint32.modulo(a: UInt32, b: UInt32) -> UInt32` — 剰余。符号なし型の a % b と同じ。b == 0 なら abort。
- `uint32.pow(base: UInt32, exp: Int) -> UInt32` — base の exp 乗。* と同様に 32 ビットへ折り返す。exp < 0 なら abort。
- `uint32.is_even(x: UInt32) -> Bool` — x が 2 の倍数なら true。
- `uint32.is_odd(x: UInt32) -> Bool` — x が 2 の倍数でなければ true。
- `uint32.band(a: UInt32, b: UInt32) -> UInt32` — ビット単位 AND。
- `uint32.bor(a: UInt32, b: UInt32) -> UInt32` — ビット単位 OR。
- `uint32.bxor(a: UInt32, b: UInt32) -> UInt32` — ビット単位 XOR。
- `uint32.bnot(x: UInt32) -> UInt32` — 32 ビットすべてをビット反転。
- `uint32.bshl(a: UInt32, n: Int) -> UInt32` — a を n ビット左シフト。n は 32 を法とする。
- `uint32.bshr(a: UInt32, n: Int) -> UInt32` — ゼロ埋め右シフト。n は 32 を法とする。
- `uint32.count_leading_zeros(x: UInt32) -> Int` — 32 ビット中の最上位の立ったビットより上のゼロ数。0 なら 32。
- `uint32.count_trailing_zeros(x: UInt32) -> Int` — 最下位の立ったビットより下のゼロ数。0 なら 32。
- `uint32.count_ones(x: UInt32) -> Int` — 32 ビット中の立っているビット数。
- `uint32.byte_swap(x: UInt32) -> UInt32` — 4 バイトを逆順にする（エンディアン反転）。
- `uint32.wrap_add(a: UInt32, b: UInt32) -> UInt32` — a + b を 32 ビットへ折り返す。
- `uint32.wrap_sub(a: UInt32, b: UInt32) -> UInt32` — a - b を 32 ビットへ折り返す。
- `uint32.wrap_mul(a: UInt32, b: UInt32) -> UInt32` — a * b を 32 ビットへ折り返す。
- `uint32.rotate_left(a: UInt32, n: Int) -> UInt32` — a の 32 ビットを n だけ左回転。n は 32 を法とする。
- `uint32.rotate_right(a: UInt32, n: Int) -> UInt32` — a の 32 ビットを n だけ右回転。n は 32 を法とする。
- `uint32.bit_length(x: UInt32) -> Int` — x を保持するのに必要なビット数。0 なら 0。
- `uint32.parse(s: String) -> Result[UInt32, String]` — 前後の ASCII 空白を除いた s から 10 進 UInt32 を解析。不正内容、- 符号、4294967295 超なら err。
- `uint32.to_string_radix(x: UInt32, radix: Int) -> String` — x を radix 進の小文字数字で表す。符号なし。radix が 2..36 外なら abort。
- `uint32.parse_radix(s: String, radix: Int) -> Result[UInt32, String]` — 前後の空白を除いた s を radix 進の UInt32 として解析。文法は int.parse_radix。不正内容、- 符号、4294967295 超なら err。
- `uint32.from(x: S) -> UInt32` — x をゼロ方向に丸め UInt32 にする。NaN、無限大、UInt32 範囲外なら abort。
- `uint32.from_checked(x: S) -> Option[UInt32]` — UInt32 が x を正確に保持できれば some(x as UInt32)、それ以外（小数部分、NaN、範囲外）は none。
- `uint32.from_saturating(x: S) -> UInt32` — x をゼロ方向に丸め UInt32 にし、UInt32 の範囲に収める。NaN は 0。
- `uint32.from_wrapping(x: S) -> UInt32` — 整数 x の下位 32 ビットを UInt32 とする（2 の補数）。

## uint64

[uint64.md](uint64.ja.md)

- `uint64.to_string(x: UInt64) -> String` — 符号なしの 10 進数字。2^64-1 まで正確。
- `uint64.min_value() -> UInt64` — 最小の UInt64：0。
- `uint64.max_value() -> UInt64` — 最大の UInt64：2^64-1。
- `uint64.compare(a: UInt64, b: UInt64) -> Int` — a が b より小さい、等しい、大きい順に -1、0、1。
- `uint64.min(a: UInt64, b: UInt64) -> UInt64` — a と b の小さい方。
- `uint64.max(a: UInt64, b: UInt64) -> UInt64` — a と b の大きい方。
- `uint64.clamp(x: UInt64, lo: UInt64, hi: UInt64) -> UInt64` — x を lo..hi に収める。lo > hi なら abort。
- `uint64.floor_div(a: UInt64, b: UInt64) -> UInt64` — 商。符号なし型の a / b と同じ。b == 0 なら abort。
- `uint64.modulo(a: UInt64, b: UInt64) -> UInt64` — 剰余。符号なし型の a % b と同じ。b == 0 なら abort。
- `uint64.pow(base: UInt64, exp: Int) -> UInt64` — base の exp 乗。* と同様に 64 ビットへ折り返す。exp < 0 なら abort。
- `uint64.is_even(x: UInt64) -> Bool` — x が 2 の倍数なら true。
- `uint64.is_odd(x: UInt64) -> Bool` — x が 2 の倍数でなければ true。
- `uint64.band(a: UInt64, b: UInt64) -> UInt64` — ビット単位 AND。
- `uint64.bor(a: UInt64, b: UInt64) -> UInt64` — ビット単位 OR。
- `uint64.bxor(a: UInt64, b: UInt64) -> UInt64` — ビット単位 XOR。
- `uint64.bnot(x: UInt64) -> UInt64` — 64 ビットすべてをビット反転。
- `uint64.bshl(a: UInt64, n: Int) -> UInt64` — a を n ビット左シフト。n は 64 を法とする。
- `uint64.bshr(a: UInt64, n: Int) -> UInt64` — ゼロ埋め右シフト。n は 64 を法とする。
- `uint64.count_leading_zeros(x: UInt64) -> Int` — 64 ビット中の最上位の立ったビットより上のゼロ数。0 なら 64。
- `uint64.count_trailing_zeros(x: UInt64) -> Int` — 最下位の立ったビットより下のゼロ数。0 なら 64。
- `uint64.count_ones(x: UInt64) -> Int` — 64 ビット中の立っているビット数。
- `uint64.byte_swap(x: UInt64) -> UInt64` — 8 バイトを逆順にする（エンディアン反転）。
- `uint64.wrap_add(a: UInt64, b: UInt64) -> UInt64` — a + b を 64 ビットへ折り返す。
- `uint64.wrap_sub(a: UInt64, b: UInt64) -> UInt64` — a - b を 64 ビットへ折り返す。
- `uint64.wrap_mul(a: UInt64, b: UInt64) -> UInt64` — a * b を 64 ビットへ折り返す。
- `uint64.rotate_left(a: UInt64, n: Int) -> UInt64` — a の 64 ビットを n だけ左回転。n は 64 を法とする。
- `uint64.rotate_right(a: UInt64, n: Int) -> UInt64` — a の 64 ビットを n だけ右回転。n は 64 を法とする。
- `uint64.bit_length(x: UInt64) -> Int` — x を保持するのに必要なビット数。0 なら 0。
- `uint64.parse(s: String) -> Result[UInt64, String]` — 前後の ASCII 空白を除いた s から 10 進 UInt64 を解析。不正内容、- 符号、2^64-1 超なら err。
- `uint64.to_string_radix(x: UInt64, radix: Int) -> String` — x を radix 進の小文字数字で表す。符号なし。2^64-1 まで正確。radix が 2..36 外なら abort。
- `uint64.parse_radix(s: String, radix: Int) -> Result[UInt64, String]` — 前後の空白を除いた s を radix 進の UInt64 として解析。文法は int.parse_radix。不正内容、- 符号、2^64-1 超なら err。
- `uint64.from(x: S) -> UInt64` — x をゼロ方向に丸め UInt64 にする。NaN、無限大、UInt64 範囲外なら abort。
- `uint64.from_checked(x: S) -> Option[UInt64]` — UInt64 が x を正確に保持できれば some(x as UInt64)、それ以外（小数部分、NaN、範囲外）は none。
- `uint64.from_saturating(x: S) -> UInt64` — x をゼロ方向に丸め UInt64 にし、UInt64 の範囲に収める。NaN は 0。
- `uint64.from_wrapping(x: S) -> UInt64` — 整数 x の下位 64 ビットを UInt64 とする（2 の補数）。

## uint8

[uint8.md](uint8.ja.md)

- `uint8.to_string(x: UInt8) -> String` — 10 進数字。符号は付かない。
- `uint8.min_value() -> UInt8` — 最小の UInt8：0。
- `uint8.max_value() -> UInt8` — 最大の UInt8：255。
- `uint8.compare(a: UInt8, b: UInt8) -> Int` — a が b より小さい、等しい、大きい順に -1、0、1。
- `uint8.min(a: UInt8, b: UInt8) -> UInt8` — a と b の小さい方。
- `uint8.max(a: UInt8, b: UInt8) -> UInt8` — a と b の大きい方。
- `uint8.clamp(x: UInt8, lo: UInt8, hi: UInt8) -> UInt8` — x を lo..hi に収める。lo > hi なら abort。
- `uint8.floor_div(a: UInt8, b: UInt8) -> UInt8` — 商。符号なし型の a / b と同じ。b == 0 なら abort。
- `uint8.modulo(a: UInt8, b: UInt8) -> UInt8` — 剰余。符号なし型の a % b と同じ。b == 0 なら abort。
- `uint8.pow(base: UInt8, exp: Int) -> UInt8` — base の exp 乗。* と同様に 8 ビットへ折り返す。exp < 0 なら abort。
- `uint8.is_even(x: UInt8) -> Bool` — x が 2 の倍数なら true。
- `uint8.is_odd(x: UInt8) -> Bool` — x が 2 の倍数でなければ true。
- `uint8.band(a: UInt8, b: UInt8) -> UInt8` — ビット単位 AND。
- `uint8.bor(a: UInt8, b: UInt8) -> UInt8` — ビット単位 OR。
- `uint8.bxor(a: UInt8, b: UInt8) -> UInt8` — ビット単位 XOR。
- `uint8.bnot(x: UInt8) -> UInt8` — 8 ビットすべてをビット反転。
- `uint8.bshl(a: UInt8, n: Int) -> UInt8` — a を n ビット左シフト。n は 8 を法とする。
- `uint8.bshr(a: UInt8, n: Int) -> UInt8` — ゼロ埋め右シフト。n は 8 を法とする。
- `uint8.count_leading_zeros(x: UInt8) -> Int` — 8 ビット中の最上位の立ったビットより上のゼロ数。0 なら 8。
- `uint8.count_trailing_zeros(x: UInt8) -> Int` — 最下位の立ったビットより下のゼロ数。0 なら 8。
- `uint8.count_ones(x: UInt8) -> Int` — 8 ビット中の立っているビット数。
- `uint8.byte_swap(x: UInt8) -> UInt8` — バイト順を反転。1 バイトなので x 自身。
- `uint8.wrap_add(a: UInt8, b: UInt8) -> UInt8` — a + b を 8 ビットへ折り返す。
- `uint8.wrap_sub(a: UInt8, b: UInt8) -> UInt8` — a - b を 8 ビットへ折り返す。
- `uint8.wrap_mul(a: UInt8, b: UInt8) -> UInt8` — a * b を 8 ビットへ折り返す。
- `uint8.rotate_left(a: UInt8, n: Int) -> UInt8` — a の 8 ビットを n だけ左回転。n は 8 を法とする。
- `uint8.rotate_right(a: UInt8, n: Int) -> UInt8` — a の 8 ビットを n だけ右回転。n は 8 を法とする。
- `uint8.bit_length(x: UInt8) -> Int` — x を保持するのに必要なビット数。0 なら 0。
- `uint8.parse(s: String) -> Result[UInt8, String]` — 前後の ASCII 空白を除いた s から 10 進 UInt8 を解析。不正内容、- 符号、255 超なら err。
- `uint8.to_string_radix(x: UInt8, radix: Int) -> String` — x を radix 進の小文字数字で表す。符号なし。radix が 2..36 外なら abort。
- `uint8.parse_radix(s: String, radix: Int) -> Result[UInt8, String]` — 前後の空白を除いた s を radix 進の UInt8 として解析。文法は int.parse_radix。不正内容、- 符号、255 超なら err。
- `uint8.from(x: S) -> UInt8` — x をゼロ方向に丸め UInt8 にする。NaN、無限大、UInt8 範囲外なら abort。
- `uint8.from_checked(x: S) -> Option[UInt8]` — UInt8 が x を正確に保持できれば some(x as UInt8)、それ以外（小数部分、NaN、範囲外）は none。
- `uint8.from_saturating(x: S) -> UInt8` — x をゼロ方向に丸め UInt8 にし、UInt8 の範囲に収める。NaN は 0。
- `uint8.from_wrapping(x: S) -> UInt8` — 整数 x の下位 8 ビットを UInt8 とする（2 の補数）。

## unix

[unix.md](unix.ja.md)

- `effect unix.connect(path: String) -> Fd` — path のリスナーに接続したストリームソケット。
- `effect unix.listen(path: String) -> Fd` — path にバインドしたリスナー。path はまだ存在していてはいけません。
- `effect unix.accept(listener: Fd) -> Fd` — listener の次の接続をブロックして待ちます。
- `effect unix.write_fds(sock: Fd, data: Bytes, fds: List[Fd]) -> Unit` — data 全体を送り、最初のバイトに記述子 fds を載せます。
- `effect unix.take_fds(sock: Fd) -> List[Fd]` — sock で受信済みでまだ取り出していない記述子。古い順です。
- `effect unix.select(xs: List[Fd], timeout_ms: Int) -> List[Fd]` — xs のうち timeout_ms 以内に読み取り可能な記述子を順に返します。< 0 はどれかを待ち、0 は確認のみ。
- `effect unix.shm_create(size: Int) -> Fd` — size バイトの新しいゼロ埋め無名ファイル。
- `effect unix.shm_resize(fd: Fd, size: Int) -> Unit` — ファイル fd を size バイトに拡大または縮小します。
- `effect unix.shm_write(fd: Fd, offset: Int, data: Bytes) -> Unit` — fd の offset に data を書き込みます。負の offset や末尾を超える書き込みは中断します（先に shm_resize）。
- `effect unix.shm_read(fd: Fd, offset: Int, len: Int) -> Bytes` — fd の offset から最大 len バイト。末尾ではより少なくなります。

## url

[url.md](url.ja.md)

- `url.encode_component(s: String) -> String` — A-Za-z0-9-._~ 以外を UTF-8 の %XX エスケープにする。
- `url.decode_component(s: String) -> Result[String, String]` — s をパーセントデコード。不正エスケープ/UTF-8は err。+ は保持。
- `url.parse(s: String) -> Result[Url, String]` — s から Url。WHATWG と同様に整形。不正なスキーム/ホスト/ポート、バックスラッシュ/制御文字、userinfo、IPv6 は err。
- `url.to_string(u: Url) -> String` — u の URL テキスト。空の query/fragment は省略。
- `url.build_query(xs: List[(String, String)]) -> String` — k=v を & で結合。両側をパーセントエンコード。
- `url.encode_form(s: String) -> String` — encode_component と同様だが、スペースは + になる。
- `url.decode_form(s: String) -> Result[String, String]` — + をスペースにし、その後パーセントデコード。不正エスケープ/UTF-8は err。
- `url.parse_query(query: String) -> Result[List[(String, String)], String]` — デコード済み (k, v) ペア。値のないキーには ""。不正エスケープは err。
- `url.resolve(base: Url, ref: String) -> Result[Url, String]` — ref を parse と同様に整形し base に対して解決（RFC 3986 5.2）。結果が URL でなければ err。

## uuid

[uuid.md](uuid.ja.md)

- `effect uuid.v4() -> Uuid` — ランダムな（version 4）Uuid。OS CSPRNG からの 122 ビット。
- `effect uuid.v7() -> Uuid` — 時刻順の（version 7）Uuid。Unix ms、カウンタ、ランダムビット。各値は直前より大きくなります。
- `uuid.parse(s: String) -> Result[Uuid, String]` — s が表す Uuid。8-4-4-4-12 の十六進数字、大小文字可。任意で {…} 内または urn:uuid: の後に置けます。
- `uuid.to_string(u: Uuid) -> String` — u を 8-4-4-4-12 の小文字十六進数字にします。
- `uuid.from_bytes(b: Bytes) -> Result[Uuid, String]` — 正確に 16 バイトの b の Uuid。それ以外の長さは err。
- `uuid.to_bytes(u: Uuid) -> Bytes` — u の 16 バイト。
- `uuid.compare(a: Uuid, b: Uuid) -> Int` — a が b より前・同じ・後なら、バイトごとの比較で負・ゼロ・正。

## value

[value.md](value.ja.md)

- `value.array(xs: List[Value]) -> Value` — xs を Array Value に包みます。
- `value.as_array(v: Value) -> Result[List[Value], String]` — Array の要素。v が別種なら err。
- `value.as_bool(v: Value) -> Result[Bool, String]` — Bool payload。それ以外は err（truthiness なし）。
- `value.as_float(v: Value) -> Result[Float, String]` — Float payload（Int は拡張）。それ以外は err。
- `value.as_int(v: Value) -> Result[Int, String]` — Int payload。Float は err、切捨て変換しません。
- `value.as_string(v: Value) -> Result[String, String]` — Str payload。それ以外は err（型強制変換なし）。
- `value.bool(b: Bool) -> Value` — Bool b を保持する Value。
- `value.float(f: Float) -> Value` — Float f を保持する Value。
- `value.int(n: Int) -> Value` — Int n を保持する Value。
- `value.merge(a: Value, b: Value) -> Value` — 浅い merge。b のキーが優先。どちらか一方でも object でなければ b。
- `value.null() -> Value` — JSON null Value。
- `value.object(xs: List[(String, Value)]) -> Value` — pairs xs を Object に包みます。重複キーは最初の位置と最後の値を保持。
- `value.omit(v: Value, keys: List[String]) -> Value` — v から列挙したキーを除きます。object 以外はそのまま。
- `value.pick(v: Value, keys: List[String]) -> Value` — v の列挙したキーだけ。object でなければ {}。
- `value.string(s: String) -> Value` — String s を保持する Value。
- `value.as_object(v: Value) -> Result[List[(String, Value)], String]` — 順序通りの Object の組。v が別種なら err。
- `value.get(v: Value, key: String) -> Option[Value]` — key の Value。存在しないか v が Object でなければ none。
- `value.get_string(v: Value, key: String) -> Option[String]` — key の String。存在しないか別種なら none。
- `value.get_int(v: Value, key: String) -> Option[Int]` — key の Int。存在しないか Int でなければ none（Float の切捨て変換はしません）。
- `value.get_float(v: Value, key: String) -> Option[Float]` — key の Float（Int は拡張）。存在しないか別種なら none。
- `value.get_bool(v: Value, key: String) -> Option[Bool]` — key の Bool。存在しないか別種なら none。
- `value.get_array(v: Value, key: String) -> Option[List[Value]]` — key の Array 要素。存在しないか別種なら none。
- `value.get_object(v: Value, key: String) -> Option[List[(String, Value)]]` — key の Object の組。存在しないか別種なら none。
- `value.root() -> Path` — 空の path。値全体。
- `value.key(p: Path, name: String) -> Path` — p を object のキー name で延長します。
- `value.index(p: Path, i: Int) -> Path` — p を array の index i で延長します。負の i は範囲外。
- `value.get_path(v: Value, p: Path) -> Option[Value]` — p の Value。どこかの step が見つからなければ none。
- `value.set_path(v: Value, p: Path, x: Value) -> Result[Value, String]` — p に x を設定した v。不足キーは作成、index が見つからなければ v はそのまま。結果の入れ子が 1000 を超えるなら err。
- `value.remove_path(v: Value, p: Path) -> Value` — v から p の entry を除きます。不在なら変更なし。root なら null。

## zlib

[zlib.md](zlib.ja.md)

- `effect zlib.compress(b: Bytes) -> Bytes` — b の zlib 枠付き DEFLATE。バイト列はターゲットにより異なります。
- `effect zlib.compress_level(b: Bytes, level: Int) -> Bytes` — compress と同様。level は 0..9 に制限し、wasm では使いません。
- `zlib.decompress(b: Bytes) -> Result[Bytes, String]` — 展開した zlib stream。入力破損、末尾の追加バイト、268435456 バイトを超える出力は err。
- `zlib.decompress_max(b: Bytes, max_len: Int) -> Result[Bytes, String]` — decompress と同様、出力上限は max_len バイト。max_len <= 0 なら abort。
- `effect zlib.deflate(b: Bytes) -> Bytes` — header なしの raw DEFLATE。バイト列はターゲットにより異なります。
- `effect zlib.deflate_level(b: Bytes, level: Int) -> Bytes` — deflate と同様。level は 0..9 に制限し、wasm では使いません。
- `zlib.inflate(b: Bytes) -> Result[Bytes, String]` — raw DEFLATE の decode。stream 破損、末尾の追加バイト、268435456 バイトを超える出力は err。
- `zlib.inflate_max(b: Bytes, max_len: Int) -> Result[Bytes, String]` — inflate と同様、出力上限は max_len バイト。max_len <= 0 なら abort。
- `effect zlib.gzip(b: Bytes) -> Bytes` — b の RFC 1952 gzip。バイト列はターゲットにより異なります。
- `zlib.gunzip(b: Bytes) -> Result[Bytes, String]` — gzip member を decode。不正 header、CRC、空入力、268435456 バイトを超える出力は err。
- `zlib.gunzip_max(b: Bytes, max_len: Int) -> Result[Bytes, String]` — gunzip と同様、出力上限は max_len バイト（全 member 合計）。max_len <= 0 なら abort。
