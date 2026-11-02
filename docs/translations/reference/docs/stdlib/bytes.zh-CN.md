# bytes

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](bytes.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](bytes.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](bytes.zh-CN.md)



> 原文：[bytes.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/bytes.md)。这是面向读者的非规范性固定快照，不参与自动生成或自动同步。签名、类型、`@since` 及代码（包括注释）保持原样，API 说明译文附在末尾。本次翻译未独立运行或验证示例、性能、安全性或跨目标一致性。

> 复杂度、定时间比较和 FFI 边界的说明来自原文，不是独立性能测量或外部代码安全保证。定时间比较在长度不同时也会立即返回 false，运行时间仍依赖长度。原文关于组合覆盖和范围写法的描述不表示本次另行验证过。

二进制数据操作。自动导入；`import bytes` 可省略。

`Bytes` 是带长度前缀、连续且可原地修改的字节缓冲区。多数操作为 O(1) 或 O(n)。

扫描源码时，用 `bytes.from_string(source)` 转换一次，再在扫描过程中使用 `bytes.get(buffer, offset) ?? -1`。原生目标使用紧凑的 `u8` 存储；将其交给 `bytes.to_list` 则得到 `List[Int]`（`i64` 元素）。两种转换都不是零拷贝借用视图。要按字节偏移提取 UTF-8 词元，请使用 `string.byte_slice(source, start, end)`。

<a id="naming-convention"></a>

## 命名约定

`<t>` 是 `u8 | i8 | u16 | i16 | u32 | i32 | i64 | f16 | f32 | f64` 之一；除 `u8`/`i8` 外，每种宽度都有 `_le` 和 `_be`。Almide `Int` 是 i64，`Float` 是 f64；较小宽度在读取时符号扩展或零扩展，在写入时截断，`f16`/`f32` 舍入到最近偶数。

- `get_<t>(b, i) -> T?`：读取字节偏移 `i` 处的一个值；若值不能完整容纳于缓冲区，则为 `none`。
- `get_<t>_array(b, i, count) -> List[T]?`：`count` 个连续值；任一个不完整都为 `none`。
- `next_<t>(b, i) -> (T?, Int)`：游标读取，返回值及其后的位置（为 `none` 时位置不变）。
- `put_<t>(b, i, x)`：在偏移 `i` 原地覆盖；写到缓冲区外会中止。
- `append_<t>(b, x)`：将缓冲区增长一个值。
- `get_bool` / `next_bool` / `put_bool` / `append_bool` 使用一个字节。

写入操作 `put_*`、`append_*`、`push`、`pop`、`fill`、`clear`、`copy_from`、`copy_within` 原地修改接收者。接收者是 `mut` 参数，因此请传入 `var`（或你自己的 `mut` 参数）。`let` 绑定、普通参数或临时值会产生 E032。

```almd check
fn stamp(mut b: Bytes, v: Int) -> Unit = bytes.put_u8(b, 0, v)  // a helper that fills its caller's buffer

fn main() -> Unit = {
  var buf = bytes.zeros(4)   // `var`, not `let`
  bytes.append_u8(buf, 7)
  stamp(buf, 9)
}
```

<a id="construction--inspection"></a>

## 构造与检查

| 签名 | 用途 |
|---|---|
| `bytes.new() -> Bytes` | 空缓冲区 |
| `bytes.zeros(n: Int) -> Bytes` | `n` 个零字节 |
| `bytes.from_list(xs: List[Int]) -> Bytes` | 从字节值列表构造 |
| `bytes.from_string(s: String) -> Bytes` | 将 UTF-8 复制到紧凑字节缓冲区（每元素一字节） |
| `bytes.to_list(b) -> List[Int]` | 实体化为列表 |
| `bytes.len(b) -> Int` | 长度 |
| `bytes.is_empty(b) -> Bool` | 长度 == 0 |
| `bytes.is_eof(b, i) -> Bool` | `i` 为负或位于末尾及之后 |
| `bytes.get(b, i) -> Int?` | 单个字节 |
| `bytes.compare(a, b) -> Int` | 字典序 |

<a id="slicing--combining"></a>

## 切片与组合

| 签名 | 用途 |
|---|---|
| `bytes.slice(b, start, end) -> Bytes` | 半开区间切片 |
| `a + b` | 拼接（Bytes `+`，如同 String 和 List） |
| `bytes.repeat(b, n) -> Bytes` | 重复 `n` 次（有界；结果过大会中止） |
| `bytes.chunk(b, n) -> List[Bytes]` | 每块 `n` 字节；`n <= 0` 中止 |
| `bytes.pad_start(b, n, byte)` / `bytes.pad_end(b, n, byte)` | 填充至长度 `n` |
| `bytes.set(b, i, x) -> Bytes` | 替换一个字节的副本 |
| `bytes.push(b, x)` / `bytes.pop(b)` | 追加／移除末尾字节（修改） |
| `bytes.append_bytes(b, other)` | 追加整个缓冲区（修改） |
| `bytes.clear(b)` | 截断到长度0（修改） |

<a id="typed-reads-and-writes"></a>

## 有类型的读写

| 系列 | 形状 | 越界 |
|---|---|---|
| `bytes.get_u16_le(b, i)` … | `T?` | `none` |
| `bytes.get_i32_le_array(b, i, count)` … | `List[T]?` | `none` |
| `bytes.next_u32_be(b, i)` … | `(T?, Int)` | `(none, i)` |
| `bytes.put_f64_le(b, i, x)` … | `Unit`，原地 | 中止 |
| `bytes.append_i64_be(b, x)` … | `Unit`，增长 | — |

类型 × 字节序 × 操作的每个组合都存在；缺少任何一个，`almide docs-gen --check` 都会失败。下方签名索引逐一列出。

<a id="strings-inside-a-buffer"></a>

## 缓冲区中的字符串

| 形式 | 组合 |
|---|---|
| 将 `pos` 处 `len` 字节作为文本 | `bytes.to_string_lossy(bytes.slice(b, pos, pos + len))` |
| `pos` 处带 u32 长度前缀的字符串 | `bytes.next_u32_le(b, pos)`（或 `_be`），然后 `bytes.next_bytes(b, p, n)` |
| 追加带 u32 长度前缀的字符串 | `bytes.append_u32_be(b, n)`，然后 `bytes.append_bytes(b, bytes.from_string(s))` |

<a id="pointer-interop"></a>

## 指针互操作

`bytes` 的裸指针函数（`as_ptr`、`as_mut_ptr`、`from_raw_ptr`、`copy_to_ptr`、`data_ptr`）已在方言纪元15移除（ADR-0031）。请将 `Bytes` 值作为经过边界检查的 [`ffi`](../../../../stdlib/ffi.md) Region 交给外部代码：`ffi.region(b)` 提供只读视图，`ffi.alloc(n)!` 提供可由外部代码或 `ffi.write` 填写的缓冲区。


<a id="signature-index-178-functions"></a>

## 原文固定快照 API 索引（178个函数）

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

## API 说明译文（按上述签名顺序）

- `bytes.append_f32_be(mut b: Bytes, x: Float) -> Unit`：将 x 舍入为 f32，以 BE 追加。
- `bytes.append_f32_le(mut b: Bytes, x: Float) -> Unit`：将 x 舍入为 f32，以 LE 追加。
- `bytes.append_f64_be(mut b: Bytes, x: Float) -> Unit`：将 x 作为 8-byte f64，以 BE 追加。
- `bytes.append_f64_le(mut b: Bytes, x: Float) -> Unit`：将 x 作为 8-byte f64，以 LE 追加。
- `bytes.append_i16_be(mut b: Bytes, x: Int) -> Unit`：以 BE 追加 x 的低16位。
- `bytes.append_i16_le(mut b: Bytes, x: Int) -> Unit`：以 LE 追加 x 的低16位。
- `bytes.append_i32_be(mut b: Bytes, x: Int) -> Unit`：以 BE 追加 x 的低32位。
- `bytes.append_i32_le(mut b: Bytes, x: Int) -> Unit`：以 LE 追加 x 的低32位。
- `bytes.append_i64_be(mut b: Bytes, x: Int) -> Unit`：将 x 作为 8 bytes，以 BE 追加。
- `bytes.append_i64_le(mut b: Bytes, x: Int) -> Unit`：将 x 作为 8 bytes，以 LE 追加。
- `bytes.append_u16_be(mut b: Bytes, x: Int) -> Unit`：以 BE 追加 x 的低16位。
- `bytes.append_u16_le(mut b: Bytes, x: Int) -> Unit`：以 LE 追加 x 的低16位。
- `bytes.append_u32_be(mut b: Bytes, x: Int) -> Unit`：以 BE 追加 x 的低32位。
- `bytes.append_u32_le(mut b: Bytes, x: Int) -> Unit`：以 LE 追加 x 的低32位。
- `bytes.append_u8(mut b: Bytes, x: Int) -> Unit`：追加 x 的低8位；256变为0。
- `bytes.clear(mut b: Bytes) -> Unit`：将 b 原地截断到长度0。
- `bytes.compare(a: Bytes, b: Bytes) -> Int`：按字典序返回 -1、0或1；前缀排在前面。
- `bytes.contains(b: Bytes, needle: Bytes) -> Bool`：needle 出现在 b 中或 needle 为空时为 true。
- `bytes.copy_from(mut b: Bytes, src: Bytes, dst_off: Int, src_off: Int, len: Int) -> Unit`：将 src 的 len 字节复制到 b，限幅以适应空间。
- `bytes.ends_with(b: Bytes, suffix: Bytes) -> Bool`：b 以 suffix 结尾或 suffix 为空时为 true。
- `bytes.is_eof(b: Bytes, pos: Int) -> Bool`：pos >= len 或 pos 为负时为 true。
- `bytes.fill(mut b: Bytes, x: Int) -> Unit`：原地将每个字节设为 x 的低8位。
- `bytes.from_list(xs: List[Int]) -> Bytes`：将 xs 转为字节，每项取低8位；-1变为255。
- `bytes.from_string(s: String) -> Bytes`：s 的 UTF-8 编码。
- `bytes.get(b: Bytes, i: Int) -> Option[Int]`：索引 i 处的字节，越界为 none。
- `bytes.index_of(b: Bytes, needle: Bytes) -> Option[Int]`：needle 的首次偏移，或 none；为空时为 some(0)。
- `bytes.insert(b: Bytes, i: Int, x: Int) -> Bytes`：在限制至0..len 的 i 处插入 x 低字节的副本。
- `bytes.is_empty(b: Bytes) -> Bool`：b 不含字节时为 true。
- `bytes.is_valid_utf8(b: Bytes) -> Bool`：b 是否为有效 UTF-8；为空时为 true。
- `bytes.len(b: Bytes) -> Int`：字节数；空缓冲区为0。
- `bytes.map(b: Bytes, f: (Int) -> Int) -> Bytes`：逐字节应用 f，结果截断为低8位。
- `bytes.zeros(n: Int) -> Bytes`：n 个零字节；n <= 0 时为空。
- `bytes.pad_start(b: Bytes, n: Int, fill: Int) -> Bytes`：左侧用 fill 填充到长度 n；已更长则保持原样。
- `bytes.pad_end(b: Bytes, n: Int, fill: Int) -> Bytes`：右侧用 fill 填充到长度 n；已更长则保持原样。
- `bytes.push(mut b: Bytes, x: Int) -> Unit`：原地追加 x 的低8位。
- `bytes.copy_within(mut b: Bytes, src_start: Int, src_end: Int, dst: Int) -> Unit`：将 src_start..src_end 复制到 dst；容纳不下则不操作。
- `bytes.remove_at(b: Bytes, i: Int) -> Bytes`：去掉 pos 处字节的副本；越界则保持原样。
- `bytes.repeat(b: Bytes, n: Int) -> Bytes`：将 b 拼接 n 次；n <= 0 时为空。
- `bytes.reverse(b: Bytes) -> Bytes`：字节顺序反转的副本。
- `bytes.set(b: Bytes, i: Int, x: Int) -> Bytes`：将字节 i 设为 x 的副本；越界则保持原样。
- `bytes.skip(b: Bytes, pos: Int, n: Int) -> Int`：pos + n，上限为 len。
- `bytes.slice(b: Bytes, start: Int, end: Int) -> Bytes`：start..end 的副本，各边界限制在0..len；start >= end 时为空。
- `bytes.split(b: Bytes, sep: Bytes) -> List[Bytes]`：sep 匹配之间的片段；sep 为空时为 [b]。
- `bytes.starts_with(b: Bytes, prefix: Bytes) -> Bool`：b 以 prefix 开始或 prefix 为空时为 true。
- `bytes.is_equal_constant_time(a: Bytes, b: Bytes) -> Bool`：a 与 b 包含相同字节时为 true，耗时仅依赖长度，不会在首个不同字节处提前退出。用于比较 MAC、令牌或密码哈希；`==` 会在首个差异处返回。长度不同时立即为 false（长度是公开的）。
- `bytes.to_list(b: Bytes) -> List[Int]`：每个字节作为0..255的 Int。
- `bytes.to_string(b: Bytes) -> Result[String, String]`：UTF-8 解码；无效时返回 err "invalid UTF-8: ..."。
- `bytes.to_string_lossy(b: Bytes) -> String`：UTF-8 解码；无效序列变为 U+FFFD。
- `bytes.append_bool(mut b: Bytes, x: Bool) -> Unit`：true 追加1，false 追加0。
- `bytes.xor(a: Bytes, b: Bytes) -> Bytes`：逐字节 XOR，长度取较短输入。
- `bytes.get_u8(b: Bytes, pos: Int) -> Option[Int]`：以0..255返回 pos 处字节；超出0..len 为 none，与 get 的值相同。
- `bytes.next_u8(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_u8(b, pos), pos + 1)；未读取任何内容时位置保持 pos。
- `bytes.put_u8(mut b: Bytes, pos: Int, x: Int) -> Unit`：在 pos 原地写入 x 的低8位；pos 超出0..len 时中止。
- `bytes.get_i8(b: Bytes, pos: Int) -> Option[Int]`：将 pos 处字节作为有符号 -128..127 返回；超出0..len 为 none。
- `bytes.next_i8(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_i8(b, pos), pos + 1)；未读取任何内容时位置保持 pos。
- `bytes.put_i8(mut b: Bytes, pos: Int, x: Int) -> Unit`：在 pos 原地写入 x 的低8位；pos 超出0..len 时中止。
- `bytes.append_i8(mut b: Bytes, x: Int) -> Unit`：原地追加 x 的低8位；-1变为255。
- `bytes.get_bool(b: Bytes, pos: Int) -> Option[Bool]`：pos 处字节 != 0；超出0..len 为 none。
- `bytes.next_bool(b: Bytes, pos: Int) -> (Option[Bool], Int)`：(get_bool(b, pos), pos + 1)；未读取任何内容时位置保持 pos。
- `bytes.put_bool(mut b: Bytes, pos: Int, x: Bool) -> Unit`：在 pos 原地写入 true 对应的1或 false 对应的0；pos 超出0..len 时中止。
- `bytes.get_u16_le(b: Bytes, pos: Int) -> Option[Int]`：pos 处的 u16 LE；容纳不下2字节时为 none。
- `bytes.next_u16_le(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_u16_le(b, pos), pos + 2)；未读取任何内容时位置保持 pos。
- `bytes.get_u16_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：从 pos 起 count 个 u16 LE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_u16_le(mut b: Bytes, pos: Int, x: Int) -> Unit`：将 x 作为 u16 LE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_u16_be(b: Bytes, pos: Int) -> Option[Int]`：pos 处的 u16 BE；容纳不下2字节时为 none。
- `bytes.next_u16_be(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_u16_be(b, pos), pos + 2)；未读取任何内容时位置保持 pos。
- `bytes.get_u16_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：从 pos 起 count 个 u16 BE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_u16_be(mut b: Bytes, pos: Int, x: Int) -> Unit`：将 x 作为 u16 BE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_i16_le(b: Bytes, pos: Int) -> Option[Int]`：pos 处的 i16 LE；容纳不下2字节时为 none。
- `bytes.next_i16_le(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_i16_le(b, pos), pos + 2)；未读取任何内容时位置保持 pos。
- `bytes.get_i16_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：从 pos 起 count 个 i16 LE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_i16_le(mut b: Bytes, pos: Int, x: Int) -> Unit`：将 x 作为 i16 LE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_i16_be(b: Bytes, pos: Int) -> Option[Int]`：pos 处的 i16 BE；容纳不下2字节时为 none。
- `bytes.next_i16_be(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_i16_be(b, pos), pos + 2)；未读取任何内容时位置保持 pos。
- `bytes.get_i16_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：从 pos 起 count 个 i16 BE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_i16_be(mut b: Bytes, pos: Int, x: Int) -> Unit`：将 x 作为 i16 BE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_u32_le(b: Bytes, pos: Int) -> Option[Int]`：pos 处的 u32 LE；容纳不下4字节时为 none。
- `bytes.next_u32_le(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_u32_le(b, pos), pos + 4)；未读取任何内容时位置保持 pos。
- `bytes.get_u32_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：从 pos 起 count 个 u32 LE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_u32_le(mut b: Bytes, pos: Int, x: Int) -> Unit`：将 x 作为 u32 LE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_u32_be(b: Bytes, pos: Int) -> Option[Int]`：pos 处的 u32 BE；容纳不下4字节时为 none。
- `bytes.next_u32_be(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_u32_be(b, pos), pos + 4)；未读取任何内容时位置保持 pos。
- `bytes.get_u32_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：从 pos 起 count 个 u32 BE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_u32_be(mut b: Bytes, pos: Int, x: Int) -> Unit`：将 x 作为 u32 BE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_i32_le(b: Bytes, pos: Int) -> Option[Int]`：pos 处的 i32 LE；容纳不下4字节时为 none。
- `bytes.next_i32_le(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_i32_le(b, pos), pos + 4)；未读取任何内容时位置保持 pos。
- `bytes.get_i32_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：从 pos 起 count 个 i32 LE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_i32_le(mut b: Bytes, pos: Int, x: Int) -> Unit`：将 x 作为 i32 LE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_i32_be(b: Bytes, pos: Int) -> Option[Int]`：pos 处的 i32 BE；容纳不下4字节时为 none。
- `bytes.next_i32_be(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_i32_be(b, pos), pos + 4)；未读取任何内容时位置保持 pos。
- `bytes.get_i32_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：从 pos 起 count 个 i32 BE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_i32_be(mut b: Bytes, pos: Int, x: Int) -> Unit`：将 x 作为 i32 BE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_i64_le(b: Bytes, pos: Int) -> Option[Int]`：pos 处的 i64 LE；容纳不下8字节时为 none。
- `bytes.next_i64_le(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_i64_le(b, pos), pos + 8)；未读取任何内容时位置保持 pos。
- `bytes.get_i64_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：从 pos 起 count 个 i64 LE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_i64_le(mut b: Bytes, pos: Int, x: Int) -> Unit`：将 x 作为 i64 LE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_i64_be(b: Bytes, pos: Int) -> Option[Int]`：pos 处的 i64 BE；容纳不下8字节时为 none。
- `bytes.next_i64_be(b: Bytes, pos: Int) -> (Option[Int], Int)`：(get_i64_be(b, pos), pos + 8)；未读取任何内容时位置保持 pos。
- `bytes.get_i64_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Int]]`：从 pos 起 count 个 i64 BE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_i64_be(mut b: Bytes, pos: Int, x: Int) -> Unit`：将 x 作为 i64 BE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_f32_le(b: Bytes, pos: Int) -> Option[Float]`：pos 处的 f32 LE；容纳不下4字节时为 none。
- `bytes.next_f32_le(b: Bytes, pos: Int) -> (Option[Float], Int)`：(get_f32_le(b, pos), pos + 4)；未读取任何内容时位置保持 pos。
- `bytes.get_f32_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]`：从 pos 起 count 个 f32 LE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_f32_le(mut b: Bytes, pos: Int, x: Float) -> Unit`：将 x 作为 f32 LE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_f32_be(b: Bytes, pos: Int) -> Option[Float]`：pos 处的 f32 BE；容纳不下4字节时为 none。
- `bytes.next_f32_be(b: Bytes, pos: Int) -> (Option[Float], Int)`：(get_f32_be(b, pos), pos + 4)；未读取任何内容时位置保持 pos。
- `bytes.get_f32_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]`：从 pos 起 count 个 f32 BE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_f32_be(mut b: Bytes, pos: Int, x: Float) -> Unit`：将 x 作为 f32 BE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_f64_le(b: Bytes, pos: Int) -> Option[Float]`：pos 处的 f64 LE；容纳不下8字节时为 none。
- `bytes.next_f64_le(b: Bytes, pos: Int) -> (Option[Float], Int)`：(get_f64_le(b, pos), pos + 8)；未读取任何内容时位置保持 pos。
- `bytes.get_f64_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]`：从 pos 起 count 个 f64 LE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_f64_le(mut b: Bytes, pos: Int, x: Float) -> Unit`：将 x 作为 f64 LE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_f64_be(b: Bytes, pos: Int) -> Option[Float]`：pos 处的 f64 BE；容纳不下8字节时为 none。
- `bytes.next_f64_be(b: Bytes, pos: Int) -> (Option[Float], Int)`：(get_f64_be(b, pos), pos + 8)；未读取任何内容时位置保持 pos。
- `bytes.get_f64_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]`：从 pos 起 count 个 f64 BE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_f64_be(mut b: Bytes, pos: Int, x: Float) -> Unit`：将 x 作为 f64 BE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.get_f16_le(b: Bytes, pos: Int) -> Option[Float]`：pos 处的 f16 LE，作为 Float 返回；容纳不下2字节时为 none。
- `bytes.next_f16_le(b: Bytes, pos: Int) -> (Option[Float], Int)`：(get_f16_le(b, pos), pos + 2)；未读取任何内容时位置保持 pos。
- `bytes.get_f16_le_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]`：从 pos 起 count 个 f16 LE；无法全部容纳或 count < 0 时为 none。
- `bytes.get_f16_be(b: Bytes, pos: Int) -> Option[Float]`：pos 处的 f16 BE，作为 Float 返回；容纳不下2字节时为 none。
- `bytes.next_f16_be(b: Bytes, pos: Int) -> (Option[Float], Int)`：(get_f16_be(b, pos), pos + 2)；未读取任何内容时位置保持 pos。
- `bytes.get_f16_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]`：从 pos 起 count 个 f16 BE；无法全部容纳或 count < 0 时为 none。
- `bytes.put_f16_le(mut b: Bytes, pos: Int, x: Float) -> Unit`：将 x 舍入为 f16 LE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.put_f16_be(mut b: Bytes, pos: Int, x: Float) -> Unit`：将 x 舍入为 f16 BE 在 pos 原地写入；写入窗口超出0..len 时中止。
- `bytes.append_f16_le(mut b: Bytes, x: Float) -> Unit`：追加舍入为 f16 的 x（最近偶数；溢出为 inf），LE。
- `bytes.append_f16_be(mut b: Bytes, x: Float) -> Unit`：追加舍入为 f16 的 x（最近偶数；溢出为 inf），BE。
- `bytes.next_bytes(b: Bytes, pos: Int, n: Int) -> (Option[Bytes], Int)`：(pos 处的 n 字节, pos + n)；容纳不下或 n < 0 时为 (none, pos)。
- `bytes.append_bytes(mut b: Bytes, src: Bytes) -> Unit`：将 src 的字节原地追加到 b。
- `bytes.new() -> Bytes`：空字节缓冲区。
- `bytes.chunk(b: Bytes, n: Int) -> List[Bytes]`：按顺序分成 n 字节块，最后一块可能较短；n <= 0 时中止。
- `bytes.pop(mut b: Bytes) -> Option[Int]`：移除并返回末尾字节；b 为空则为 none。
- `bytes.take(b: Bytes, n: Int) -> Bytes`：前 n 个字节；超过长度取整个 b，n < 0 时为空。
- `bytes.drop(b: Bytes, n: Int) -> Bytes`：去掉 b 的前 n 个字节；超过长度则为空，n < 0 时为整个 b。
- `bytes.take_end(b: Bytes, n: Int) -> Bytes`：最后 n 个字节；超过长度取整个 b，n < 0 时为空。
- `bytes.drop_end(b: Bytes, n: Int) -> Bytes`：去掉 b 的最后 n 个字节；超过长度则为空，n < 0 时为整个 b。
- `bytes.split_at(b: Bytes, n: Int) -> (Bytes, Bytes)`：(前 n 字节, 其余)；n 超过末尾为 (b, 空)，n < 0 为 (空, b)。
- `bytes.first(b: Bytes) -> Option[Int]`：首字节，b 为空则为 none。
- `bytes.last(b: Bytes) -> Option[Int]`：末字节，b 为空则为 none。
- `bytes.filter(b: Bytes, f: (Int) -> Bool) -> Bytes`：f 为 true 的字节。
- `bytes.fold(b: Bytes, init: B, f: (B, Int) -> B) -> B`：对字节从左到右折叠 f。
- `bytes.any(b: Bytes, f: (Int) -> Bool) -> Bool`：某个字节满足 f 时为 true；b 为空为 false。
- `bytes.all(b: Bytes, f: (Int) -> Bool) -> Bool`：每个字节都满足 f 时为 true；b 为空为 true。
- `bytes.count(b: Bytes, f: (Int) -> Bool) -> Int`：满足 f 的字节数量。
- `bytes.find(b: Bytes, f: (Int) -> Bool) -> Option[Int]`：满足 f 的首个字节，或 none。
- `bytes.find_index(b: Bytes, f: (Int) -> Bool) -> Option[Int]`：满足 f 的首个字节的索引，或 none。
- `bytes.enumerate(b: Bytes) -> List[(Int, Int)]`：从0开始的(索引, 字节)对。
- `bytes.zip(b: Bytes, ys: List[B]) -> List[(Int, B)]`：将字节与 ys 配对，在较短者末尾停止。
- `bytes.zip_with(b: Bytes, ys: List[B], f: (Int, B) -> C) -> List[C]`：对与 ys 配对的字节应用 f，在较短者末尾停止。
- `bytes.filter_map(b: Bytes, f: (Int) -> Option[B]) -> List[B]`：按顺序收集对字节应用 f 所得的 some 结果。
- `bytes.find_map(b: Bytes, f: (Int) -> Option[B]) -> Option[B]`：对字节应用 f 所得的首个 some 结果，或 none。
- `bytes.reduce(b: Bytes, f: (Int, Int) -> Int) -> Option[Int]`：从首字节开始用 f 折叠；b 为空则为 none。
- `bytes.scan(b: Bytes, init: B, f: (B, Int) -> B) -> List[B]`：字节的逐步折叠值，不含 init。
- `bytes.partition(b: Bytes, f: (Int) -> Bool) -> (List[Int], List[Int])`：(f 为 true 的字节, 其余)，各自保持顺序。
- `bytes.group_by(b: Bytes, f: (Int) -> B) -> Map[B, List[Int]]`：按 f 的键对字节分组，各组保持顺序。
- `bytes.sort(b: Bytes) -> List[Int]`：升序排列的字节。
- `bytes.sort_by(b: Bytes, f: (Int) -> B) -> List[Int]`：按 f 的键排序的字节，相同键保持顺序。
- `bytes.sort_with(b: Bytes, cmp: (Int, Int) -> Int) -> List[Int]`：按 cmp 排序的字节（负数表示第一个参数在前），稳定。
- `bytes.min(b: Bytes) -> Option[Int]`：最小字节；空时为 none。
- `bytes.max(b: Bytes) -> Option[Int]`：最大字节；空时为 none。
- `bytes.min_by(b: Bytes, f: (Int) -> K) -> Option[Int]`：具有最小键的首个字节；空时为 none。
- `bytes.max_by(b: Bytes, f: (Int) -> K) -> Option[Int]`：具有最大键的最后一个字节；空时为 none。
- `bytes.min_with(b: Bytes, cmp: (Int, Int) -> Int) -> Option[Int]`：按 cmp 最小的首个字节；空时为 none。
- `bytes.max_with(b: Bytes, cmp: (Int, Int) -> Int) -> Option[Int]`：按 cmp 最大的最后一个字节；空时为 none。
- `bytes.sum(b: Bytes) -> Int`：字节之和；b 为空为0。
- `bytes.product(b: Bytes) -> Int`：字节之积，回绕；b 为空为1。
- `bytes.take_while(b: Bytes, f: (Int) -> Bool) -> Bytes`：所有字节均满足 f 的最长前缀。
- `bytes.drop_while(b: Bytes, f: (Int) -> Bool) -> Bytes`：去掉所有字节均满足 f 的最长前缀后的 b。
- `bytes.window(b: Bytes, n: Int) -> List[Bytes]`：每段 n 个连续字节；n > len 为 []，n <= 0 时中止。
- `bytes.dedup(b: Bytes) -> Bytes`：将 b 中每段相邻相等字节合并为一个。
- `bytes.intersperse(b: Bytes, sep: Int) -> Bytes`：在 b 的每对相邻字节间插入 sep。
- `bytes.unique(b: Bytes) -> Bytes`：按顺序保留每个字节值的首次出现。
- `bytes.unique_by(b: Bytes, f: (Int) -> K) -> Bytes`：按顺序保留 f 给出的每个键的首个字节。
- `bytes.last_index_of(b: Bytes, needle: Bytes) -> Option[Int]`：最后一个 needle 的字节索引，或 none；空 needle 在 len 处找到。
- `bytes.replace(b: Bytes, from: Bytes, to: Bytes) -> Bytes`：从左到右将 b 中所有不重叠的 from 替换为 to。
- `bytes.replace_first(b: Bytes, from: Bytes, to: Bytes) -> Bytes`：将 b 中第一个 from 替换为 to。
- `bytes.strip_prefix(b: Bytes, prefix: Bytes) -> Option[Bytes]`：去掉 b 的 prefix；不以它开头则为 none。
- `bytes.strip_suffix(b: Bytes, suffix: Bytes) -> Option[Bytes]`：去掉 b 的 suffix；不以它结尾则为 none。
- `bytes.count_matches(b: Bytes, needle: Bytes) -> Int`：needle 不重叠的出现次数；空 needle 为 len + 1。
