# bytes

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](bytes.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](bytes.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](bytes.zh-CN.md)



> 原文：[bytes.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/bytes.md)。読者向けの非規範的な固定スナップショットです。自動生成・自動同期の対象ではありません。署名・型・`@since` とコード（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例・性能・安全性・ターゲット間の一致は、この翻訳作業では独立実行・検証していません。

> 計算量・定時間比較・FFI 境界の記述は原文の主張であり、独立した性能測定や外部コードの安全性保証ではありません。定時間比較でも長さの不一致は即座に false で、実行時間は長さに依存します。原文の全組み合わせ網羅という記述や範囲表記も、ここで追加検証したとは扱いません。

バイナリデータの操作。自動インポートされ、`import bytes` は任意です。

`Bytes` は長さの接頭部を持つ、連続したその場で変更可能なバイトバッファです。ほとんどの操作は O(1) または O(n) です。

ソースのスキャナーでは `bytes.from_string(source)` で一度変換し、走査中は `bytes.get(buffer, offset) ?? -1` を使います。ネイティブではコンパクトな `u8` 記憶域を使います。一方、これに `bytes.to_list` を適用すると `List[Int]`（`i64` 要素）になります。どちらの変換もゼロコピーの借用ビューではありません。バイト位置で UTF-8 トークンを取り出すには `string.byte_slice(source, start, end)` を使います。

## 命名規則

`<t>` は `u8 | i8 | u16 | i16 | u32 | i32 | i64 | f16 | f32 | f64` のいずれかです。`u8`/`i8` 以外の各幅には `_le` と `_be` があります。Almide の `Int` は i64、`Float` は f64 です。より小さい幅は読み取り時に符号拡張またはゼロ拡張し、書き込み時は切り詰めます。`f16`/`f32` は最近接偶数に丸めます。

- `get_<t>(b, i) -> T?` — バイト位置 `i` の値を1つ読みます。値がバッファ内に収まらなければ `none`。
- `get_<t>_array(b, i, count) -> List[T]?` — `count` 個の連続した値。どれかが途切れれば `none`。
- `next_<t>(b, i) -> (T?, Int)` — カーソル読み取り。値とその直後の位置を返します（`none` のとき位置は変わりません）。
- `put_<t>(b, i, x)` — 位置 `i` をその場で上書き。バッファ外への書き込みは中断します。
- `append_<t>(b, x)` — 値1つ分バッファを伸ばします。
- `get_bool` / `next_bool` / `put_bool` / `append_bool` は1バイトを使います。

書き込み操作 `put_*`、`append_*`、`push`、`pop`、`fill`、`clear`、`copy_from`、`copy_within` はレシーバーをその場で変更します。レシーバーは `mut` パラメーターなので `var`（または自身の `mut` パラメーター）を渡してください。`let` 束縛、通常のパラメーター、一時値は E032 です。

```almd check
fn stamp(mut b: Bytes, v: Int) -> Unit = bytes.put_u8(b, 0, v)  // a helper that fills its caller's buffer

fn main() -> Unit = {
  var buf = bytes.zeros(4)   // `var`, not `let`
  bytes.append_u8(buf, 7)
  stamp(buf, 9)
}
```

## 構築と調査

| 署名 | 目的 |
|---|---|
| `bytes.new() -> Bytes` | 空のバッファ |
| `bytes.zeros(n: Int) -> Bytes` | `n` 個のゼロバイト |
| `bytes.from_list(xs: List[Int]) -> Bytes` | バイト値のリストから構築 |
| `bytes.from_string(s: String) -> Bytes` | UTF-8 をコンパクトなバイトバッファへコピー（要素あたり1バイト） |
| `bytes.to_list(b) -> List[Int]` | リストとして実体化 |
| `bytes.len(b) -> Int` | 長さ |
| `bytes.is_empty(b) -> Bool` | 長さ == 0 |
| `bytes.is_eof(b, i) -> Bool` | `i` が負、または末尾以降 |
| `bytes.get(b, i) -> Int?` | 単一バイト |
| `bytes.compare(a, b) -> Int` | 辞書順 |

## 切り出しと結合

| 署名 | 目的 |
|---|---|
| `bytes.slice(b, start, end) -> Bytes` | 半開区間の切り出し |
| `a + b` | 連結（String、List と同様の Bytes の `+`） |
| `bytes.repeat(b, n) -> Bytes` | `n` 回反復（上限あり。結果が大きすぎると中断） |
| `bytes.chunk(b, n) -> List[Bytes]` | `n` バイトずつ。`n <= 0` は中断 |
| `bytes.pad_start(b, n, byte)` / `bytes.pad_end(b, n, byte)` | 長さ `n` まで埋める |
| `bytes.set(b, i, x) -> Bytes` | 1バイトを置き換えたコピー |
| `bytes.push(b, x)` / `bytes.pop(b)` | 末尾バイトの追加／削除（変更する） |
| `bytes.append_bytes(b, other)` | バッファ全体を追加（変更する） |
| `bytes.clear(b)` | 長さ0に切り詰め（変更する） |

## 型付き読み書き

| 系列 | 形 | 範囲外 |
|---|---|---|
| `bytes.get_u16_le(b, i)` … | `T?` | `none` |
| `bytes.get_i32_le_array(b, i, count)` … | `List[T]?` | `none` |
| `bytes.next_u32_be(b, i)` … | `(T?, Int)` | `(none, i)` |
| `bytes.put_f64_le(b, i, x)` … | `Unit`、その場で変更 | 中断 |
| `bytes.append_i64_be(b, x)` … | `Unit`、伸長 | — |

型 × バイト順 × 操作の全組み合わせが存在し、どれかが欠ければ `almide docs-gen --check` が失敗します。下記の署名索引に各項目があります。

## バッファ内の文字列

| 形 | 組み合わせ |
|---|---|
| `pos` の `len` バイトをテキストとして扱う | `bytes.to_string_lossy(bytes.slice(b, pos, pos + len))` |
| `pos` の u32 長さ接頭部付き文字列 | `bytes.next_u32_le(b, pos)`（または `_be`）、次に `bytes.next_bytes(b, p, n)` |
| u32 長さ接頭部付き文字列を追加 | `bytes.append_u32_be(b, n)`、次に `bytes.append_bytes(b, bytes.from_string(s))` |

## ポインターとの相互運用

`bytes` の生ポインター関数（`as_ptr`、`as_mut_ptr`、`from_raw_ptr`、`copy_to_ptr`、`data_ptr`）は方言エポック15で削除されました（ADR-0031）。`Bytes` 値は境界検査付きの [`ffi`](../../../../stdlib/ffi.md) Region として外部コードへ渡します。読み取り専用ビューは `ffi.region(b)`、外部コードまたは `ffi.write` が埋められるバッファは `ffi.alloc(n)!` です。


## 原文固定スナップショットの API 索引（178関数）

```
// Appends x rounded to f32, BE.
// @since 0.13.4 or earlier
bytes.append_f32_be(mut b: Bytes, x: Float) -> Unit

// Appends x rounded to f32, LE.
// @since 0.13.4 or earlier
bytes.append_f32_le(mut b: Bytes, x: Float) -> Unit

// Appends x as 8-byte f64, BE.
// @since 0.13.4 or earlier
bytes.append_f64_be(mut b: Bytes, x: Float) -> Unit

// Appends x as 8-byte f64, LE.
// @since 0.13.4 or earlier
bytes.append_f64_le(mut b: Bytes, x: Float) -> Unit

// Appends x's low 16 bits, BE.
// @since 0.13.4 or earlier
bytes.append_i16_be(mut b: Bytes, x: Int) -> Unit

// Appends x's low 16 bits, LE.
// @since 0.13.4 or earlier
bytes.append_i16_le(mut b: Bytes, x: Int) -> Unit

// Appends x's low 32 bits, BE.
// @since 0.13.4 or earlier
bytes.append_i32_be(mut b: Bytes, x: Int) -> Unit

// Appends x's low 32 bits, LE.
// @since 0.13.4 or earlier
bytes.append_i32_le(mut b: Bytes, x: Int) -> Unit

// Appends x as 8 bytes, BE.
// @since 0.13.4 or earlier
bytes.append_i64_be(mut b: Bytes, x: Int) -> Unit

// Appends x as 8 bytes, LE.
// @since 0.13.4 or earlier
bytes.append_i64_le(mut b: Bytes, x: Int) -> Unit

// Appends x's low 16 bits, BE.
// @since 0.13.4 or earlier
bytes.append_u16_be(mut b: Bytes, x: Int) -> Unit

// Appends x's low 16 bits, LE.
// @since 0.13.4 or earlier
bytes.append_u16_le(mut b: Bytes, x: Int) -> Unit

// Appends x's low 32 bits, BE.
// @since 0.13.4 or earlier
bytes.append_u32_be(mut b: Bytes, x: Int) -> Unit

// Appends x's low 32 bits, LE.
// @since 0.13.4 or earlier
bytes.append_u32_le(mut b: Bytes, x: Int) -> Unit

// Appends x's low 8 bits; 256 becomes 0.
// @since 0.13.4 or earlier
bytes.append_u8(mut b: Bytes, x: Int) -> Unit

// Truncates b to length 0 in place.
// @since 0.9.8 or earlier
bytes.clear(mut b: Bytes) -> Unit

// Lexicographic -1, 0 or 1; a prefix sorts first.
// @since unreleased
bytes.compare(a: Bytes, b: Bytes) -> Int

// True if needle occurs in b, or needle is empty.
// @since 0.13.4 or earlier
bytes.contains(b: Bytes, needle: Bytes) -> Bool

// Copies len bytes of src into b; clamped to fit.
// @since 0.13.4 or earlier
bytes.copy_from(mut b: Bytes, src: Bytes, dst_off: Int, src_off: Int, len: Int) -> Unit

// True if b ends with suffix, or suffix is empty.
// @since 0.13.4 or earlier
bytes.ends_with(b: Bytes, suffix: Bytes) -> Bool

// True if pos >= len or pos is negative.
// @since unreleased
bytes.is_eof(b: Bytes, pos: Int) -> Bool

// Sets every byte to x's low 8 bits in place.
// @since 0.13.4 or earlier
bytes.fill(mut b: Bytes, x: Int) -> Unit

// xs as bytes, low 8 bits each; -1 becomes 255.
// @since 0.9.3 or earlier
bytes.from_list(xs: List[Int]) -> Bytes

// UTF-8 encoding of s.
// @since 0.9.8 or earlier
bytes.from_string(s: String) -> Bytes

// Byte at index i, or none when out of range.
// @since 0.9.3 or earlier
bytes.get(b: Bytes, i: Int) -> Option[Int]

// First offset of needle, or none; some(0) if empty.
// @since 0.13.4 or earlier
bytes.index_of(b: Bytes, needle: Bytes) -> Option[Int]

// Copy with x's low byte at i, clamped to 0..len.
// @since 0.13.4 or earlier
bytes.insert(b: Bytes, i: Int, x: Int) -> Bytes

// True when b holds no bytes.
// @since 0.9.3 or earlier
bytes.is_empty(b: Bytes) -> Bool

// Whether b is valid UTF-8; true when empty.
// @since 0.13.4 or earlier
bytes.is_valid_utf8(b: Bytes) -> Bool

// Byte count; 0 for an empty buffer.
// @since 0.9.3 or earlier
bytes.len(b: Bytes) -> Int

// f applied per byte; results cut to low 8 bits.
// @since unreleased
bytes.map(b: Bytes, f: (Int) -> Int) -> Bytes

// n zero bytes; empty if n <= 0.
// @since unreleased
bytes.zeros(n: Int) -> Bytes

// Left-padded with fill to length n; as-is if longer.
// @since unreleased
bytes.pad_start(b: Bytes, n: Int, fill: Int) -> Bytes

// Right-padded with fill to length n; as-is if longer.
// @since unreleased
bytes.pad_end(b: Bytes, n: Int, fill: Int) -> Bytes

// Appends x's low 8 bits in place.
// @since 0.9.8 or earlier
bytes.push(mut b: Bytes, x: Int) -> Unit

// Copies src_start..src_end to dst; no-op if it won't fit.
// @since 0.22.0 or earlier
bytes.copy_within(mut b: Bytes, src_start: Int, src_end: Int, dst: Int) -> Unit

// Copy without the byte at pos; as-is if out of range.
// @since 0.13.4 or earlier
bytes.remove_at(b: Bytes, i: Int) -> Bytes

// b concatenated n times; empty if n <= 0.
// @since 0.9.3 or earlier
bytes.repeat(b: Bytes, n: Int) -> Bytes

// Copy with the byte order reversed.
// @since 0.13.4 or earlier
bytes.reverse(b: Bytes) -> Bytes

// Copy with byte i set to x; as-is if out of range.
// @since 0.9.3 or earlier
bytes.set(b: Bytes, i: Int, x: Int) -> Bytes

// pos + n, capped at len.
// @since 0.13.4 or earlier
bytes.skip(b: Bytes, pos: Int, n: Int) -> Int

// Copy of start..end, each clamped to 0..len; empty if start >= end.
// @since 0.9.3 or earlier
bytes.slice(b: Bytes, start: Int, end: Int) -> Bytes

// Pieces between sep matches; [b] for an empty sep.
// @since 0.13.4 or earlier
bytes.split(b: Bytes, sep: Bytes) -> List[Bytes]

// True if b begins with prefix, or prefix is empty.
// @since 0.13.4 or earlier
bytes.starts_with(b: Bytes, prefix: Bytes) -> Bool

// True if a and b hold the same bytes, in time that depends only on their
// lengths: no early exit on the first differing byte. Use it to compare a
// MAC, token or password hash; `==` returns at the first difference. A length
// mismatch is false at once (lengths are public).
// @since unreleased
bytes.is_equal_constant_time(a: Bytes, b: Bytes) -> Bool

// Each byte as an Int in 0..255.
// @since 0.9.3 or earlier
bytes.to_list(b: Bytes) -> List[Int]

// UTF-8 decode; err "invalid UTF-8: ..." if bad.
// @since 0.13.4 or earlier
bytes.to_string(b: Bytes) -> Result[String, String]

// UTF-8 decode; invalid sequences become U+FFFD.
// @since 0.13.4 or earlier
bytes.to_string_lossy(b: Bytes) -> String

// Appends 1 for true, 0 for false.
// @since unreleased
bytes.append_bool(mut b: Bytes, x: Bool) -> Unit

// Bytewise XOR, as long as the shorter input.
// @since 0.13.4 or earlier
bytes.xor(a: Bytes, b: Bytes) -> Bytes

// The byte at pos as 0..255; none outside 0..len. The same value as get.
// @since unreleased
bytes.get_u8(b: Bytes, pos: Int) -> Option[Int]

// (get_u8(b, pos), pos + 1); the position stays at pos when nothing is read.
// @since unreleased
bytes.next_u8(b: Bytes, pos: Int) -> (Option[Int], Int)

// Writes x's low 8 bits at pos in place; pos outside 0..len aborts.
// @since unreleased
bytes.put_u8(mut b: Bytes, pos: Int, x: Int) -> Unit

// The byte at pos as a signed -128..127; none outside 0..len.
// @since unreleased
bytes.get_i8(b: Bytes, pos: Int) -> Option[Int]

// (get_i8(b, pos), pos + 1); the position stays at pos when nothing is read.
// @since unreleased
bytes.next_i8(b: Bytes, pos: Int) -> (Option[Int], Int)

// Writes x's low 8 bits at pos in place; pos outside 0..len aborts.
// @since unreleased
bytes.put_i8(mut b: Bytes, pos: Int, x: Int) -> Unit

// Appends x's low 8 bits in place; -1 becomes 255.
// @since unreleased
bytes.append_i8(mut b: Bytes, x: Int) -> Unit

// The byte at pos != 0; none outside 0..len.
// @since unreleased
bytes.get_bool(b: Bytes, pos: Int) -> Option[Bool]

// (get_bool(b, pos), pos + 1); the position stays at pos when nothing is read.
// @since unreleased
bytes.next_bool(b: Bytes, pos: Int) -> (Option[Bool], Int)

// Writes 1 for true, 0 for false at pos in place; pos outside 0..len aborts.
// @since unreleased
bytes.put_bool(mut b: Bytes, pos: Int, x: Bool) -> Unit

// The u16 LE at pos; none unless 2 bytes fit.
// @since unreleased
bytes.get_u16_le(b: Bytes, pos: Int) -> Option[Int]

// (get_u16_le(b, pos), pos + 2); the position stays at pos when nothing is read.
// @since unreleased
bytes.next_u16_le(b: Bytes, pos: Int) -> (Option[Int], Int)

// count u16 LE from pos; none unless all fit or if count < 0.
// @since unreleased
bytes.get_u16_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]

// Writes x as u16 LE at pos in place; a window outside 0..len aborts.
// @since unreleased
bytes.put_u16_le(mut b: Bytes, pos: Int, x: Int) -> Unit

// The u16 BE at pos; none unless 2 bytes fit.
// @since unreleased
bytes.get_u16_be(b: Bytes, pos: Int) -> Option[Int]

// (get_u16_be(b, pos), pos + 2); the position stays at pos when nothing is read.
// @since unreleased
bytes.next_u16_be(b: Bytes, pos: Int) -> (Option[Int], Int)

// count u16 BE from pos; none unless all fit or if count < 0.
// @since unreleased
bytes.get_u16_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]

// Writes x as u16 BE at pos in place; a window outside 0..len aborts.
// @since unreleased
bytes.put_u16_be(mut b: Bytes, pos: Int, x: Int) -> Unit

// The i16 LE at pos; none unless 2 bytes fit.
// @since unreleased
bytes.get_i16_le(b: Bytes, pos: Int) -> Option[Int]

// (get_i16_le(b, pos), pos + 2); the position stays at pos when nothing is read.
// @since unreleased
bytes.next_i16_le(b: Bytes, pos: Int) -> (Option[Int], Int)

// count i16 LE from pos; none unless all fit or if count < 0.
// @since unreleased
bytes.get_i16_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]

// Writes x as i16 LE at pos in place; a window outside 0..len aborts.
// @since unreleased
bytes.put_i16_le(mut b: Bytes, pos: Int, x: Int) -> Unit

// The i16 BE at pos; none unless 2 bytes fit.
// @since unreleased
bytes.get_i16_be(b: Bytes, pos: Int) -> Option[Int]

// (get_i16_be(b, pos), pos + 2); the position stays at pos when nothing is read.
// @since unreleased
bytes.next_i16_be(b: Bytes, pos: Int) -> (Option[Int], Int)

// count i16 BE from pos; none unless all fit or if count < 0.
// @since unreleased
bytes.get_i16_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]

// Writes x as i16 BE at pos in place; a window outside 0..len aborts.
// @since unreleased
bytes.put_i16_be(mut b: Bytes, pos: Int, x: Int) -> Unit

// The u32 LE at pos; none unless 4 bytes fit.
// @since unreleased
bytes.get_u32_le(b: Bytes, pos: Int) -> Option[Int]

// (get_u32_le(b, pos), pos + 4); the position stays at pos when nothing is read.
// @since unreleased
bytes.next_u32_le(b: Bytes, pos: Int) -> (Option[Int], Int)

// count u32 LE from pos; none unless all fit or if count < 0.
// @since unreleased
bytes.get_u32_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]

// Writes x as u32 LE at pos in place; a window outside 0..len aborts.
// @since unreleased
bytes.put_u32_le(mut b: Bytes, pos: Int, x: Int) -> Unit

// The u32 BE at pos; none unless 4 bytes fit.
// @since unreleased
bytes.get_u32_be(b: Bytes, pos: Int) -> Option[Int]

// (get_u32_be(b, pos), pos + 4); the position stays at pos when nothing is read.
// @since unreleased
bytes.next_u32_be(b: Bytes, pos: Int) -> (Option[Int], Int)

// count u32 BE from pos; none unless all fit or if count < 0.
// @since unreleased
bytes.get_u32_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]

// Writes x as u32 BE at pos in place; a window outside 0..len aborts.
// @since unreleased
bytes.put_u32_be(mut b: Bytes, pos: Int, x: Int) -> Unit

// The i32 LE at pos; none unless 4 bytes fit.
// @since unreleased
bytes.get_i32_le(b: Bytes, pos: Int) -> Option[Int]

// (get_i32_le(b, pos), pos + 4); the position stays at pos when nothing is read.
// @since unreleased
bytes.next_i32_le(b: Bytes, pos: Int) -> (Option[Int], Int)

// count i32 LE from pos; none unless all fit or if count < 0.
// @since unreleased
bytes.get_i32_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]

// Writes x as i32 LE at pos in place; a window outside 0..len aborts.
// @since unreleased
bytes.put_i32_le(mut b: Bytes, pos: Int, x: Int) -> Unit

// The i32 BE at pos; none unless 4 bytes fit.
// @since unreleased
bytes.get_i32_be(b: Bytes, pos: Int) -> Option[Int]

// (get_i32_be(b, pos), pos + 4); the position stays at pos when nothing is read.
// @since unreleased
bytes.next_i32_be(b: Bytes, pos: Int) -> (Option[Int], Int)

// count i32 BE from pos; none unless all fit or if count < 0.
// @since unreleased
bytes.get_i32_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]

// Writes x as i32 BE at pos in place; a window outside 0..len aborts.
// @since unreleased
bytes.put_i32_be(mut b: Bytes, pos: Int, x: Int) -> Unit

// The i64 LE at pos; none unless 8 bytes fit.
// @since unreleased
bytes.get_i64_le(b: Bytes, pos: Int) -> Option[Int]

// (get_i64_le(b, pos), pos + 8); the position stays at pos when nothing is read.
// @since unreleased
bytes.next_i64_le(b: Bytes, pos: Int) -> (Option[Int], Int)

// count i64 LE from pos; none unless all fit or if count < 0.
// @since unreleased
bytes.get_i64_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]

// Writes x as i64 LE at pos in place; a window outside 0..len aborts.
// @since unreleased
bytes.put_i64_le(mut b: Bytes, pos: Int, x: Int) -> Unit

// The i64 BE at pos; none unless 8 bytes fit.
// @since unreleased
bytes.get_i64_be(b: Bytes, pos: Int) -> Option[Int]

// (get_i64_be(b, pos), pos + 8); the position stays at pos when nothing is read.
// @since unreleased
bytes.next_i64_be(b: Bytes, pos: Int) -> (Option[Int], Int)

// count i64 BE from pos; none unless all fit or if count < 0.
// @since unreleased
bytes.get_i64_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]

// Writes x as i64 BE at pos in place; a window outside 0..len aborts.
// @since unreleased
bytes.put_i64_be(mut b: Bytes, pos: Int, x: Int) -> Unit

// The f32 LE at pos; none unless 4 bytes fit.
// @since unreleased
bytes.get_f32_le(b: Bytes, pos: Int) -> Option[Float]

// (get_f32_le(b, pos), pos + 4); the position stays at pos when nothing is read.
// @since unreleased
bytes.next_f32_le(b: Bytes, pos: Int) -> (Option[Float], Int)

// count f32 LE from pos; none unless all fit or if count < 0.
// @since unreleased
bytes.get_f32_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]

// Writes x as f32 LE at pos in place; a window outside 0..len aborts.
// @since unreleased
bytes.put_f32_le(mut b: Bytes, pos: Int, x: Float) -> Unit

// The f32 BE at pos; none unless 4 bytes fit.
// @since unreleased
bytes.get_f32_be(b: Bytes, pos: Int) -> Option[Float]

// (get_f32_be(b, pos), pos + 4); the position stays at pos when nothing is read.
// @since unreleased
bytes.next_f32_be(b: Bytes, pos: Int) -> (Option[Float], Int)

// count f32 BE from pos; none unless all fit or if count < 0.
// @since unreleased
bytes.get_f32_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]

// Writes x as f32 BE at pos in place; a window outside 0..len aborts.
// @since unreleased
bytes.put_f32_be(mut b: Bytes, pos: Int, x: Float) -> Unit

// The f64 LE at pos; none unless 8 bytes fit.
// @since unreleased
bytes.get_f64_le(b: Bytes, pos: Int) -> Option[Float]

// (get_f64_le(b, pos), pos + 8); the position stays at pos when nothing is read.
// @since unreleased
bytes.next_f64_le(b: Bytes, pos: Int) -> (Option[Float], Int)

// count f64 LE from pos; none unless all fit or if count < 0.
// @since unreleased
bytes.get_f64_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]

// Writes x as f64 LE at pos in place; a window outside 0..len aborts.
// @since unreleased
bytes.put_f64_le(mut b: Bytes, pos: Int, x: Float) -> Unit

// The f64 BE at pos; none unless 8 bytes fit.
// @since unreleased
bytes.get_f64_be(b: Bytes, pos: Int) -> Option[Float]

// (get_f64_be(b, pos), pos + 8); the position stays at pos when nothing is read.
// @since unreleased
bytes.next_f64_be(b: Bytes, pos: Int) -> (Option[Float], Int)

// count f64 BE from pos; none unless all fit or if count < 0.
// @since unreleased
bytes.get_f64_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]

// Writes x as f64 BE at pos in place; a window outside 0..len aborts.
// @since unreleased
bytes.put_f64_be(mut b: Bytes, pos: Int, x: Float) -> Unit

// The f16 LE at pos as a Float; none unless 2 bytes fit.
// @since unreleased
bytes.get_f16_le(b: Bytes, pos: Int) -> Option[Float]

// (get_f16_le(b, pos), pos + 2); the position stays at pos when nothing is read.
// @since unreleased
bytes.next_f16_le(b: Bytes, pos: Int) -> (Option[Float], Int)

// count f16 LE from pos; none unless all fit or if count < 0.
// @since unreleased
bytes.get_f16_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]

// The f16 BE at pos as a Float; none unless 2 bytes fit.
// @since unreleased
bytes.get_f16_be(b: Bytes, pos: Int) -> Option[Float]

// (get_f16_be(b, pos), pos + 2); the position stays at pos when nothing is read.
// @since unreleased
bytes.next_f16_be(b: Bytes, pos: Int) -> (Option[Float], Int)

// count f16 BE from pos; none unless all fit or if count < 0.
// @since unreleased
bytes.get_f16_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]

// Writes x rounded to f16 LE at pos in place; a window outside 0..len aborts.
// @since unreleased
bytes.put_f16_le(mut b: Bytes, pos: Int, x: Float) -> Unit

// Writes x rounded to f16 BE at pos in place; a window outside 0..len aborts.
// @since unreleased
bytes.put_f16_be(mut b: Bytes, pos: Int, x: Float) -> Unit

// Appends x rounded to f16 (nearest even; overflow is inf), LE.
// @since unreleased
bytes.append_f16_le(mut b: Bytes, x: Float) -> Unit

// Appends x rounded to f16 (nearest even; overflow is inf), BE.
// @since unreleased
bytes.append_f16_be(mut b: Bytes, x: Float) -> Unit

// (the n bytes at pos, pos + n); (none, pos) unless they fit or if n < 0.
// @since unreleased
bytes.next_bytes(b: Bytes, pos: Int, n: Int) -> (Option[Bytes], Int)

// Appends src's bytes to b in place.
// @since unreleased
bytes.append_bytes(mut b: Bytes, src: Bytes) -> Unit

// An empty byte buffer.
// @since 0.9.3 or earlier
bytes.new() -> Bytes

// n-byte pieces in order, the last may be short; n <= 0 aborts.
// @since unreleased
bytes.chunk(b: Bytes, n: Int) -> List[Bytes]

// Removes and returns the last byte; none if b is empty.
// @since unreleased
bytes.pop(mut b: Bytes) -> Option[Int]

// The first n bytes; all of b past its length; empty if n < 0.
// @since unreleased
bytes.take(b: Bytes, n: Int) -> Bytes

// b without its first n bytes; empty past its length; all of b if n < 0.
// @since unreleased
bytes.drop(b: Bytes, n: Int) -> Bytes

// The last n bytes; all of b past its length; empty if n < 0.
// @since unreleased
bytes.take_end(b: Bytes, n: Int) -> Bytes

// b without its last n bytes; empty past its length; all of b if n < 0.
// @since unreleased
bytes.drop_end(b: Bytes, n: Int) -> Bytes

// (first n bytes, the rest); n past the end gives (b, empty), n < 0 gives (empty, b).
// @since unreleased
bytes.split_at(b: Bytes, n: Int) -> (Bytes, Bytes)

// The first byte, or none if b is empty.
// @since unreleased
bytes.first(b: Bytes) -> Option[Int]

// The last byte, or none if b is empty.
// @since unreleased
bytes.last(b: Bytes) -> Option[Int]

// The bytes for which f is true.
// @since unreleased
bytes.filter(b: Bytes, f: (Int) -> Bool) -> Bytes

// f folded over the bytes, left to right.
// @since unreleased
bytes.fold(b: Bytes, init: B, f: (B, Int) -> B) -> B

// True when f holds for some byte; false for empty b.
// @since unreleased
bytes.any(b: Bytes, f: (Int) -> Bool) -> Bool

// True when f holds for every byte; true for empty b.
// @since unreleased
bytes.all(b: Bytes, f: (Int) -> Bool) -> Bool

// How many bytes satisfy f.
// @since unreleased
bytes.count(b: Bytes, f: (Int) -> Bool) -> Int

// The first byte satisfying f, or none.
// @since unreleased
bytes.find(b: Bytes, f: (Int) -> Bool) -> Option[Int]

// Index of the first byte satisfying f, or none.
// @since unreleased
bytes.find_index(b: Bytes, f: (Int) -> Bool) -> Option[Int]

// (index, byte) pairs from 0.
// @since unreleased
bytes.enumerate(b: Bytes) -> List[(Int, Int)]

// Bytes paired with ys; stops at the shorter.
// @since unreleased
bytes.zip(b: Bytes, ys: List[B]) -> List[(Int, B)]

// f over bytes paired with ys; stops at the shorter.
// @since unreleased
bytes.zip_with(b: Bytes, ys: List[B], f: (Int, B) -> C) -> List[C]

// The some results of f over the bytes, in order.
// @since unreleased
bytes.filter_map(b: Bytes, f: (Int) -> Option[B]) -> List[B]

// The first some result of f over the bytes, or none.
// @since unreleased
bytes.find_map(b: Bytes, f: (Int) -> Option[B]) -> Option[B]

// f folded over the bytes from the first; none if b is empty.
// @since unreleased
bytes.reduce(b: Bytes, f: (Int, Int) -> Int) -> Option[Int]

// The running fold over the bytes, init excluded.
// @since unreleased
bytes.scan(b: Bytes, init: B, f: (B, Int) -> B) -> List[B]

// (bytes where f is true, the rest), each in order.
// @since unreleased
bytes.partition(b: Bytes, f: (Int) -> Bool) -> (List[Int], List[Int])

// Bytes grouped by f's key, each group in order.
// @since unreleased
bytes.group_by(b: Bytes, f: (Int) -> B) -> Map[B, List[Int]]

// The bytes in ascending order.
// @since unreleased
bytes.sort(b: Bytes) -> List[Int]

// The bytes ordered by f's key; equal keys keep their order.
// @since unreleased
bytes.sort_by(b: Bytes, f: (Int) -> B) -> List[Int]

// The bytes ordered by cmp (negative = first argument first); stable.
// @since unreleased
bytes.sort_with(b: Bytes, cmp: (Int, Int) -> Int) -> List[Int]

// The smallest byte; none if empty.
// @since unreleased
bytes.min(b: Bytes) -> Option[Int]

// The largest byte; none if empty.
// @since unreleased
bytes.max(b: Bytes) -> Option[Int]

// The first byte with the smallest key; none if empty.
// @since unreleased
bytes.min_by(b: Bytes, f: (Int) -> K) -> Option[Int]

// The last byte with the largest key; none if empty.
// @since unreleased
bytes.max_by(b: Bytes, f: (Int) -> K) -> Option[Int]

// The first smallest byte by cmp; none if empty.
// @since unreleased
bytes.min_with(b: Bytes, cmp: (Int, Int) -> Int) -> Option[Int]

// The last largest byte by cmp; none if empty.
// @since unreleased
bytes.max_with(b: Bytes, cmp: (Int, Int) -> Int) -> Option[Int]

// The sum of the bytes; 0 for empty b.
// @since unreleased
bytes.sum(b: Bytes) -> Int

// The product of the bytes, wrapping; 1 for empty b.
// @since unreleased
bytes.product(b: Bytes) -> Int

// The longest prefix whose bytes all satisfy f.
// @since unreleased
bytes.take_while(b: Bytes, f: (Int) -> Bool) -> Bytes

// b after the longest prefix whose bytes all satisfy f.
// @since unreleased
bytes.drop_while(b: Bytes, f: (Int) -> Bool) -> Bytes

// Every run of n consecutive bytes; [] if n > len; n <= 0 aborts.
// @since unreleased
bytes.window(b: Bytes, n: Int) -> List[Bytes]

// b with each run of equal adjacent bytes collapsed to one.
// @since unreleased
bytes.dedup(b: Bytes) -> Bytes

// The bytes of b with sep between each pair.
// @since unreleased
bytes.intersperse(b: Bytes, sep: Int) -> Bytes

// The first occurrence of each byte value, in order.
// @since unreleased
bytes.unique(b: Bytes) -> Bytes

// The first byte of each key f gives, in order.
// @since unreleased
bytes.unique_by(b: Bytes, f: (Int) -> K) -> Bytes

// Byte index of the last needle, or none; an empty needle is found at len.
// @since unreleased
bytes.last_index_of(b: Bytes, needle: Bytes) -> Option[Int]

// b with every non-overlapping from replaced by to, left to right.
// @since unreleased
bytes.replace(b: Bytes, from: Bytes, to: Bytes) -> Bytes

// b with the first from replaced by to.
// @since unreleased
bytes.replace_first(b: Bytes, from: Bytes, to: Bytes) -> Bytes

// b without prefix, or none when b does not start with it.
// @since unreleased
bytes.strip_prefix(b: Bytes, prefix: Bytes) -> Option[Bytes]

// b without suffix, or none when b does not end with it.
// @since unreleased
bytes.strip_suffix(b: Bytes, suffix: Bytes) -> Option[Bytes]

// Non-overlapping occurrences of needle; len + 1 for an empty needle.
// @since unreleased
bytes.count_matches(b: Bytes, needle: Bytes) -> Int
```

## API 説明の訳（上記署名順）

- `bytes.append_f32_be(mut b: Bytes, x: Float) -> Unit`：x を f32 に丸め、BE で追加します。
- `bytes.append_f32_le(mut b: Bytes, x: Float) -> Unit`：x を f32 に丸め、LE で追加します。
- `bytes.append_f64_be(mut b: Bytes, x: Float) -> Unit`：x を 8-byte f64 として BE で追加します。
- `bytes.append_f64_le(mut b: Bytes, x: Float) -> Unit`：x を 8-byte f64 として LE で追加します。
- `bytes.append_i16_be(mut b: Bytes, x: Int) -> Unit`：x の下位16ビットを BE で追加します。
- `bytes.append_i16_le(mut b: Bytes, x: Int) -> Unit`：x の下位16ビットを LE で追加します。
- `bytes.append_i32_be(mut b: Bytes, x: Int) -> Unit`：x の下位32ビットを BE で追加します。
- `bytes.append_i32_le(mut b: Bytes, x: Int) -> Unit`：x の下位32ビットを LE で追加します。
- `bytes.append_i64_be(mut b: Bytes, x: Int) -> Unit`：x を 8 bytes として BE で追加します。
- `bytes.append_i64_le(mut b: Bytes, x: Int) -> Unit`：x を 8 bytes として LE で追加します。
- `bytes.append_u16_be(mut b: Bytes, x: Int) -> Unit`：x の下位16ビットを BE で追加します。
- `bytes.append_u16_le(mut b: Bytes, x: Int) -> Unit`：x の下位16ビットを LE で追加します。
- `bytes.append_u32_be(mut b: Bytes, x: Int) -> Unit`：x の下位32ビットを BE で追加します。
- `bytes.append_u32_le(mut b: Bytes, x: Int) -> Unit`：x の下位32ビットを LE で追加します。
- `bytes.append_u8(mut b: Bytes, x: Int) -> Unit`：x の下位8ビットを追加します。256 は0になります。
- `bytes.clear(mut b: Bytes) -> Unit`：b をその場で長さ0に切り詰めます。
- `bytes.compare(a: Bytes, b: Bytes) -> Int`：辞書順の -1、0、1。接頭部は先に並びます。
- `bytes.contains(b: Bytes, needle: Bytes) -> Bool`：needle が b に現れるか、needle が空なら true。
- `bytes.copy_from(mut b: Bytes, src: Bytes, dst_off: Int, src_off: Int, len: Int) -> Unit`：src の len バイトを b へコピー。収まるように制限します。
- `bytes.ends_with(b: Bytes, suffix: Bytes) -> Bool`：b が suffix で終わるか、suffix が空なら true。
- `bytes.is_eof(b: Bytes, pos: Int) -> Bool`：pos >= len または pos が負なら true。
- `bytes.fill(mut b: Bytes, x: Int) -> Unit`：全バイトを x の下位8ビットにその場で設定します。
- `bytes.from_list(xs: List[Int]) -> Bytes`：xs を各値の下位8ビットからなるバイト列にします。-1 は255。
- `bytes.from_string(s: String) -> Bytes`：s の UTF-8 符号化。
- `bytes.get(b: Bytes, i: Int) -> Option[Int]`：添字 i のバイト。範囲外なら none。
- `bytes.index_of(b: Bytes, needle: Bytes) -> Option[Int]`：needle の最初の位置、または none。空なら some(0)。
- `bytes.insert(b: Bytes, i: Int, x: Int) -> Bytes`：0..len に制限した i に x の下位バイトを挿入したコピー。
- `bytes.is_empty(b: Bytes) -> Bool`：b にバイトがなければ true。
- `bytes.is_valid_utf8(b: Bytes) -> Bool`：b が有効な UTF-8 かどうか。空なら true。
- `bytes.len(b: Bytes) -> Int`：バイト数。空のバッファなら0。
- `bytes.map(b: Bytes, f: (Int) -> Int) -> Bytes`：各バイトに f を適用し、結果を下位8ビットに切り詰めます。
- `bytes.zeros(n: Int) -> Bytes`：n 個のゼロバイト。n <= 0 なら空。
- `bytes.pad_start(b: Bytes, n: Int, fill: Int) -> Bytes`：fill で左を埋め長さ n にします。すでに長ければそのまま。
- `bytes.pad_end(b: Bytes, n: Int, fill: Int) -> Bytes`：fill で右を埋め長さ n にします。すでに長ければそのまま。
- `bytes.push(mut b: Bytes, x: Int) -> Unit`：x の下位8ビットをその場で追加します。
- `bytes.copy_within(mut b: Bytes, src_start: Int, src_end: Int, dst: Int) -> Unit`：src_start..src_end を dst へコピー。収まらなければ何もしません。
- `bytes.remove_at(b: Bytes, i: Int) -> Bytes`：pos のバイトを除いたコピー。範囲外ならそのまま。
- `bytes.repeat(b: Bytes, n: Int) -> Bytes`：b を n 回連結。n <= 0 なら空。
- `bytes.reverse(b: Bytes) -> Bytes`：バイト順を逆にしたコピー。
- `bytes.set(b: Bytes, i: Int, x: Int) -> Bytes`：バイト i を x に設定したコピー。範囲外ならそのまま。
- `bytes.skip(b: Bytes, pos: Int, n: Int) -> Int`：pos + n を len で上限にします。
- `bytes.slice(b: Bytes, start: Int, end: Int) -> Bytes`：start..end のコピー。各境界を0..len に制限し、start >= end なら空。
- `bytes.split(b: Bytes, sep: Bytes) -> List[Bytes]`：sep の一致の間にある部分。sep が空なら [b]。
- `bytes.starts_with(b: Bytes, prefix: Bytes) -> Bool`：b が prefix で始まるか、prefix が空なら true。
- `bytes.is_equal_constant_time(a: Bytes, b: Bytes) -> Bool`：a と b が同じバイト列なら true。時間は長さだけに依存し、最初の異なるバイトで早期終了しません。MAC、トークン、パスワードハッシュの比較に使います。`==` は最初の違いで返ります。長さの不一致は即座に false です（長さは公開情報）。
- `bytes.to_list(b: Bytes) -> List[Int]`：各バイトを0..255の Int として返します。
- `bytes.to_string(b: Bytes) -> Result[String, String]`：UTF-8 デコード。不正なら err "invalid UTF-8: ..."。
- `bytes.to_string_lossy(b: Bytes) -> String`：UTF-8 デコード。不正な列は U+FFFD に置き換えます。
- `bytes.append_bool(mut b: Bytes, x: Bool) -> Unit`：true には1、false には0を追加。
- `bytes.xor(a: Bytes, b: Bytes) -> Bytes`：バイト単位 XOR。短い入力と同じ長さ。
- `bytes.get_u8(b: Bytes, pos: Int) -> Option[Int]`：pos のバイトを0..255で返します。0..len 外なら none。get と同じ値。
- `bytes.next_u8(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_u8(b, pos), pos + 1)。何も読めなければ位置は pos のまま。
- `bytes.put_u8(mut b: Bytes, pos: Int, x: Int) -> Unit`：pos に x の下位8ビットをその場で書き込みます。pos が0..len 外なら中断。
- `bytes.get_i8(b: Bytes, pos: Int) -> Option[Int]`：pos のバイトを符号付き -128..127 で返します。0..len 外なら none。
- `bytes.next_i8(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_i8(b, pos), pos + 1)。何も読めなければ位置は pos のまま。
- `bytes.put_i8(mut b: Bytes, pos: Int, x: Int) -> Unit`：pos に x の下位8ビットをその場で書き込みます。pos が0..len 外なら中断。
- `bytes.append_i8(mut b: Bytes, x: Int) -> Unit`：x の下位8ビットをその場で追加します。-1 は255。
- `bytes.get_bool(b: Bytes, pos: Int) -> Option[Bool]`：pos のバイト != 0。0..len 外なら none。
- `bytes.next_bool(b: Bytes, pos: Int) -> (Option[Bool], Int)`：(get_bool(b, pos), pos + 1)。何も読めなければ位置は pos のまま。
- `bytes.put_bool(mut b: Bytes, pos: Int, x: Bool) -> Unit`：pos に true なら1、false なら0をその場で書き込みます。pos が0..len 外なら中断。
- `bytes.get_u16_le(b: Bytes, pos: Int) -> Option[Int]`：pos の u16 LE。2バイトが収まらなければ none。
- `bytes.next_u16_le(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_u16_le(b, pos), pos + 2)。何も読めなければ位置は pos のまま。
- `bytes.get_u16_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：pos から count 個の u16 LE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_u16_le(mut b: Bytes, pos: Int, x: Int) -> Unit`：x を u16 LE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_u16_be(b: Bytes, pos: Int) -> Option[Int]`：pos の u16 BE。2バイトが収まらなければ none。
- `bytes.next_u16_be(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_u16_be(b, pos), pos + 2)。何も読めなければ位置は pos のまま。
- `bytes.get_u16_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：pos から count 個の u16 BE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_u16_be(mut b: Bytes, pos: Int, x: Int) -> Unit`：x を u16 BE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_i16_le(b: Bytes, pos: Int) -> Option[Int]`：pos の i16 LE。2バイトが収まらなければ none。
- `bytes.next_i16_le(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_i16_le(b, pos), pos + 2)。何も読めなければ位置は pos のまま。
- `bytes.get_i16_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：pos から count 個の i16 LE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_i16_le(mut b: Bytes, pos: Int, x: Int) -> Unit`：x を i16 LE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_i16_be(b: Bytes, pos: Int) -> Option[Int]`：pos の i16 BE。2バイトが収まらなければ none。
- `bytes.next_i16_be(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_i16_be(b, pos), pos + 2)。何も読めなければ位置は pos のまま。
- `bytes.get_i16_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：pos から count 個の i16 BE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_i16_be(mut b: Bytes, pos: Int, x: Int) -> Unit`：x を i16 BE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_u32_le(b: Bytes, pos: Int) -> Option[Int]`：pos の u32 LE。4バイトが収まらなければ none。
- `bytes.next_u32_le(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_u32_le(b, pos), pos + 4)。何も読めなければ位置は pos のまま。
- `bytes.get_u32_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：pos から count 個の u32 LE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_u32_le(mut b: Bytes, pos: Int, x: Int) -> Unit`：x を u32 LE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_u32_be(b: Bytes, pos: Int) -> Option[Int]`：pos の u32 BE。4バイトが収まらなければ none。
- `bytes.next_u32_be(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_u32_be(b, pos), pos + 4)。何も読めなければ位置は pos のまま。
- `bytes.get_u32_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：pos から count 個の u32 BE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_u32_be(mut b: Bytes, pos: Int, x: Int) -> Unit`：x を u32 BE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_i32_le(b: Bytes, pos: Int) -> Option[Int]`：pos の i32 LE。4バイトが収まらなければ none。
- `bytes.next_i32_le(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_i32_le(b, pos), pos + 4)。何も読めなければ位置は pos のまま。
- `bytes.get_i32_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：pos から count 個の i32 LE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_i32_le(mut b: Bytes, pos: Int, x: Int) -> Unit`：x を i32 LE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_i32_be(b: Bytes, pos: Int) -> Option[Int]`：pos の i32 BE。4バイトが収まらなければ none。
- `bytes.next_i32_be(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_i32_be(b, pos), pos + 4)。何も読めなければ位置は pos のまま。
- `bytes.get_i32_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：pos から count 個の i32 BE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_i32_be(mut b: Bytes, pos: Int, x: Int) -> Unit`：x を i32 BE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_i64_le(b: Bytes, pos: Int) -> Option[Int]`：pos の i64 LE。8バイトが収まらなければ none。
- `bytes.next_i64_le(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_i64_le(b, pos), pos + 8)。何も読めなければ位置は pos のまま。
- `bytes.get_i64_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：pos から count 個の i64 LE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_i64_le(mut b: Bytes, pos: Int, x: Int) -> Unit`：x を i64 LE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_i64_be(b: Bytes, pos: Int) -> Option[Int]`：pos の i64 BE。8バイトが収まらなければ none。
- `bytes.next_i64_be(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_i64_be(b, pos), pos + 8)。何も読めなければ位置は pos のまま。
- `bytes.get_i64_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：pos から count 個の i64 BE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_i64_be(mut b: Bytes, pos: Int, x: Int) -> Unit`：x を i64 BE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_f32_le(b: Bytes, pos: Int) -> Option[Float]`：pos の f32 LE。4バイトが収まらなければ none。
- `bytes.next_f32_le(b: Bytes, pos: Int) -> (Option[Float], Int)`：(get_f32_le(b, pos), pos + 4)。何も読めなければ位置は pos のまま。
- `bytes.get_f32_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]`：pos から count 個の f32 LE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_f32_le(mut b: Bytes, pos: Int, x: Float) -> Unit`：x を f32 LE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_f32_be(b: Bytes, pos: Int) -> Option[Float]`：pos の f32 BE。4バイトが収まらなければ none。
- `bytes.next_f32_be(b: Bytes, pos: Int) -> (Option[Float], Int)`：(get_f32_be(b, pos), pos + 4)。何も読めなければ位置は pos のまま。
- `bytes.get_f32_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]`：pos から count 個の f32 BE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_f32_be(mut b: Bytes, pos: Int, x: Float) -> Unit`：x を f32 BE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_f64_le(b: Bytes, pos: Int) -> Option[Float]`：pos の f64 LE。8バイトが収まらなければ none。
- `bytes.next_f64_le(b: Bytes, pos: Int) -> (Option[Float], Int)`：(get_f64_le(b, pos), pos + 8)。何も読めなければ位置は pos のまま。
- `bytes.get_f64_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]`：pos から count 個の f64 LE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_f64_le(mut b: Bytes, pos: Int, x: Float) -> Unit`：x を f64 LE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_f64_be(b: Bytes, pos: Int) -> Option[Float]`：pos の f64 BE。8バイトが収まらなければ none。
- `bytes.next_f64_be(b: Bytes, pos: Int) -> (Option[Float], Int)`：(get_f64_be(b, pos), pos + 8)。何も読めなければ位置は pos のまま。
- `bytes.get_f64_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]`：pos から count 個の f64 BE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_f64_be(mut b: Bytes, pos: Int, x: Float) -> Unit`：x を f64 BE として pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.get_f16_le(b: Bytes, pos: Int) -> Option[Float]`：pos の f16 LE を Float として返します。2バイトが収まらなければ none。
- `bytes.next_f16_le(b: Bytes, pos: Int) -> (Option[Float], Int)`：(get_f16_le(b, pos), pos + 2)。何も読めなければ位置は pos のまま。
- `bytes.get_f16_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]`：pos から count 個の f16 LE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.get_f16_be(b: Bytes, pos: Int) -> Option[Float]`：pos の f16 BE を Float として返します。2バイトが収まらなければ none。
- `bytes.next_f16_be(b: Bytes, pos: Int) -> (Option[Float], Int)`：(get_f16_be(b, pos), pos + 2)。何も読めなければ位置は pos のまま。
- `bytes.get_f16_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]`：pos から count 個の f16 BE。すべてが収まらない場合、または count < 0 なら none。
- `bytes.put_f16_le(mut b: Bytes, pos: Int, x: Float) -> Unit`：x を f16 LE に丸めて pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.put_f16_be(mut b: Bytes, pos: Int, x: Float) -> Unit`：x を f16 BE に丸めて pos にその場で書き込みます。書き込み範囲が0..len の外なら中断します。
- `bytes.append_f16_le(mut b: Bytes, x: Float) -> Unit`：x を f16 に丸めて追加します（最近接偶数、オーバーフローは inf）。LE。
- `bytes.append_f16_be(mut b: Bytes, x: Float) -> Unit`：x を f16 に丸めて追加します（最近接偶数、オーバーフローは inf）。BE。
- `bytes.next_bytes(b: Bytes, pos: Int, n: Int) -> (Option[Bytes], Int)`：(pos の n バイト, pos + n)。収まらない、または n < 0 の場合は (none, pos)。
- `bytes.append_bytes(mut b: Bytes, src: Bytes) -> Unit`：src のバイト列を b にその場で追加。
- `bytes.new() -> Bytes`：空のバイトバッファ。
- `bytes.chunk(b: Bytes, n: Int) -> List[Bytes]`：n バイトずつ順に分割。最後は短くなる場合があります。n <= 0 は中断。
- `bytes.pop(mut b: Bytes) -> Option[Int]`：末尾バイトを削除して返します。b が空なら none。
- `bytes.take(b: Bytes, n: Int) -> Bytes`：先頭 n バイト。長さを超えれば b 全体、n < 0 なら空。
- `bytes.drop(b: Bytes, n: Int) -> Bytes`：b の先頭 n バイトを除いたもの。長さを超えれば空、n < 0 なら b 全体。
- `bytes.take_end(b: Bytes, n: Int) -> Bytes`：末尾 n バイト。長さを超えれば b 全体、n < 0 なら空。
- `bytes.drop_end(b: Bytes, n: Int) -> Bytes`：b の末尾 n バイトを除いたもの。長さを超えれば空、n < 0 なら b 全体。
- `bytes.split_at(b: Bytes, n: Int) -> (Bytes, Bytes)`：(先頭 n バイト、残り)。n が末尾を超えれば (b, 空)、n < 0 なら (空, b)。
- `bytes.first(b: Bytes) -> Option[Int]`：先頭バイト。b が空なら none。
- `bytes.last(b: Bytes) -> Option[Int]`：末尾バイト。b が空なら none。
- `bytes.filter(b: Bytes, f: (Int) -> Bool) -> Bytes`：f が true になるバイト。
- `bytes.fold(b: Bytes, init: B, f: (B, Int) -> B) -> B`：バイト列に f を左から右へ畳み込みます。
- `bytes.any(b: Bytes, f: (Int) -> Bool) -> Bool`：どれかのバイトで f が成り立てば true。b が空なら false。
- `bytes.all(b: Bytes, f: (Int) -> Bool) -> Bool`：すべてのバイトで f が成り立てば true。b が空なら true。
- `bytes.count(b: Bytes, f: (Int) -> Bool) -> Int`：f を満たすバイト数。
- `bytes.find(b: Bytes, f: (Int) -> Bool) -> Option[Int]`：f を満たす最初のバイト、または none。
- `bytes.find_index(b: Bytes, f: (Int) -> Bool) -> Option[Int]`：f を満たす最初のバイトの添字、または none。
- `bytes.enumerate(b: Bytes) -> List[(Int, Int)]`：0から始まる (添字、バイト) の組。
- `bytes.zip(b: Bytes, ys: List[B]) -> List[(Int, B)]`：バイトを ys と組にします。短い方で終了。
- `bytes.zip_with(b: Bytes, ys: List[B], f: (Int, B) -> C) -> List[C]`：ys と組にしたバイトに f を適用。短い方で終了。
- `bytes.filter_map(b: Bytes, f: (Int) -> Option[B]) -> List[B]`：各バイトへの f の結果のうち some を順に並べます。
- `bytes.find_map(b: Bytes, f: (Int) -> Option[B]) -> Option[B]`：各バイトへの f の最初の some 結果、または none。
- `bytes.reduce(b: Bytes, f: (Int, Int) -> Int) -> Option[Int]`：最初のバイトから f で畳み込みます。b が空なら none。
- `bytes.scan(b: Bytes, init: B, f: (B, Int) -> B) -> List[B]`：バイト列の逐次畳み込み。init は含みません。
- `bytes.partition(b: Bytes, f: (Int) -> Bool) -> (List[Int], List[Int])`：(f が true のバイト、それ以外)。それぞれ順序を保持。
- `bytes.group_by(b: Bytes, f: (Int) -> B) -> Map[B, List[Int]]`：f のキーでバイトをグループ化。各グループの順序を保持。
- `bytes.sort(b: Bytes) -> List[Int]`：昇順のバイト列。
- `bytes.sort_by(b: Bytes, f: (Int) -> B) -> List[Int]`：f のキー順のバイト列。同じキーの順序を保持。
- `bytes.sort_with(b: Bytes, cmp: (Int, Int) -> Int) -> List[Int]`：cmp 順のバイト列（負なら第1引数が先）。安定です。
- `bytes.min(b: Bytes) -> Option[Int]`：最小バイト。空なら none。
- `bytes.max(b: Bytes) -> Option[Int]`：最大バイト。空なら none。
- `bytes.min_by(b: Bytes, f: (Int) -> K) -> Option[Int]`：最小キーを持つ最初のバイト。空なら none。
- `bytes.max_by(b: Bytes, f: (Int) -> K) -> Option[Int]`：最大キーを持つ最後のバイト。空なら none。
- `bytes.min_with(b: Bytes, cmp: (Int, Int) -> Int) -> Option[Int]`：cmp で最小となる最初のバイト。空なら none。
- `bytes.max_with(b: Bytes, cmp: (Int, Int) -> Int) -> Option[Int]`：cmp で最大となる最後のバイト。空なら none。
- `bytes.sum(b: Bytes) -> Int`：バイトの和。b が空なら0。
- `bytes.product(b: Bytes) -> Int`：バイトの積。折り返します。b が空なら1。
- `bytes.take_while(b: Bytes, f: (Int) -> Bool) -> Bytes`：すべてのバイトが f を満たす最長の接頭部。
- `bytes.drop_while(b: Bytes, f: (Int) -> Bool) -> Bytes`：すべてのバイトが f を満たす最長の接頭部を除いた b。
- `bytes.window(b: Bytes, n: Int) -> List[Bytes]`：n 個の連続バイトからなるすべての区間。n > len なら []、n <= 0 は中断。
- `bytes.dedup(b: Bytes) -> Bytes`：隣接する同値バイトの各連続を1つにまとめた b。
- `bytes.intersperse(b: Bytes, sep: Int) -> Bytes`：b の各隣接バイト間に sep。
- `bytes.unique(b: Bytes) -> Bytes`：各バイト値の最初の出現を順に保持。
- `bytes.unique_by(b: Bytes, f: (Int) -> K) -> Bytes`：f の各キーに対する最初のバイトを順に保持。
- `bytes.last_index_of(b: Bytes, needle: Bytes) -> Option[Int]`：最後の needle のバイト添字、または none。空の needle は len で見つかります。
- `bytes.replace(b: Bytes, from: Bytes, to: Bytes) -> Bytes`：b の重ならないすべての from を左から右へ to に置換。
- `bytes.replace_first(b: Bytes, from: Bytes, to: Bytes) -> Bytes`：b の最初の from を to に置換。
- `bytes.strip_prefix(b: Bytes, prefix: Bytes) -> Option[Bytes]`：b から prefix を除いたもの。prefix で始まらなければ none。
- `bytes.strip_suffix(b: Bytes, suffix: Bytes) -> Option[Bytes]`：b から suffix を除いたもの。suffix で終わらなければ none。
- `bytes.count_matches(b: Bytes, needle: Bytes) -> Int`：重ならない needle の出現数。needle が空なら len + 1。
