# bytes

Binary data manipulation. `import bytes`.

`Bytes` is a contiguous, mutable-in-place byte buffer with a length prefix.
Most operations are O(1) or O(n).

For source scanners, convert once with `bytes.from_string(source)` and use
`bytes.get(buffer, offset) ?? -1` while scanning. This uses compact `u8`
storage on native; `bytes.to_list` of it gives a `List[Int]` (`i64`
elements) instead. Neither conversion is a zero-copy borrowed view. To extract a
UTF-8 token by its byte offsets, use `string.byte_slice(source, start, end)`.

## Naming convention

`<t>` is one of `u8 | i8 | u16 | i16 | u32 | i32 | i64 | f16 | f32 | f64`; every
width except `u8`/`i8` comes in `_le` and `_be`. Almide `Int` is i64 and `Float`
is f64; smaller widths are sign- or zero-extended on read and truncated on
write, and `f16`/`f32` round to nearest even.

- `get_<t>(b, i) -> T?` — read one value at byte offset `i`; `none` when the value does not fit inside the buffer.
- `get_<t>_array(b, i, count) -> List[T]?` — `count` consecutive values; `none` if any is cut off.
- `next_<t>(b, i) -> (T?, Int)` — a cursor read: the value and the offset just past it (unchanged on `none`).
- `put_<t>(b, i, x)` — overwrite at offset `i` in place; a write outside the buffer aborts.
- `append_<t>(b, x)` — grow the buffer by one value.
- `get_bool` / `next_bool` / `put_bool` / `append_bool` use one byte.

The writers — `put_*`, `append_*`, `push`, `pop`, `fill`, `clear`,
`copy_from`, `copy_within` — change their receiver in place, and the
receiver is a `mut` parameter: pass a `var` (or a `mut` parameter of your own).
A `let` binding, a plain parameter or a temporary is E032.

```almd check
fn stamp(mut b: Bytes, v: Int) -> Unit = bytes.put_u8(b, 0, v)  // a helper that fills its caller's buffer

fn main() -> Unit = {
  var buf = bytes.zeros(4)   // `var`, not `let`
  bytes.append_u8(buf, 7)
  stamp(buf, 9)
}
```

## Construction & inspection

| Signature | Purpose |
|---|---|
| `bytes.new() -> Bytes` | An empty buffer |
| `bytes.zeros(n: Int) -> Bytes` | `n` zeroed bytes |
| `bytes.from_list(xs: List[Int]) -> Bytes` | From a list of byte values |
| `bytes.from_string(s: String) -> Bytes` | Copy UTF-8 into a compact byte buffer (one byte per element) |
| `bytes.to_list(b) -> List[Int]` | Materialise as a list |
| `bytes.len(b) -> Int` | Length |
| `bytes.is_empty(b) -> Bool` | Length == 0 |
| `bytes.is_eof(b, i) -> Bool` | `i` is at or past the end |
| `bytes.get(b, i) -> Int?` | Single byte |
| `bytes.compare(a, b) -> Int` | Lexicographic order |

## Slicing & combining

| Signature | Purpose |
|---|---|
| `bytes.slice(b, start, end) -> Bytes` | Half-open slice |
| `a + b` | Concatenate (Bytes `+`, like String and List) |
| `bytes.repeat(b, n) -> Bytes` | Repeat `n` times (bounded; an oversized result aborts) |
| `bytes.chunk(b, n) -> List[Bytes]` | Pieces of `n` bytes; `n <= 0` aborts |
| `bytes.pad_start(b, n, byte)` / `bytes.pad_end(b, n, byte)` | Pad to length `n` |
| `bytes.set(b, i, x) -> Bytes` | A copy with one byte replaced |
| `bytes.push(b, x)` / `bytes.pop(b)` | Append / remove the last byte (mutate) |
| `bytes.append_bytes(b, other)` | Append a whole buffer (mutates) |
| `bytes.clear(b)` | Truncate to length 0 (mutates) |

## Typed reads and writes

| Family | Shape | Out of range |
|---|---|---|
| `bytes.get_u16_le(b, i)` … | `T?` | `none` |
| `bytes.get_i32_le_array(b, i, count)` … | `List[T]?` | `none` |
| `bytes.next_u32_be(b, i)` … | `(T?, Int)` | `(none, i)` |
| `bytes.put_f64_le(b, i, x)` … | `Unit`, in place | aborts |
| `bytes.append_i64_be(b, x)` … | `Unit`, grows | — |

Every cell of type × byte order × operation exists; `almide docs-gen --check`
fails if one is missing. The signature index below lists each one.

## Strings inside a buffer

| Shape | Composition |
|---|---|
| `len` bytes at `pos` as text | `bytes.to_string_lossy(bytes.slice(b, pos, pos + len))` |
| a u32-length-prefixed string at `pos` | `bytes.next_u32_le(b, pos)` (or `_be`), then `bytes.next_bytes(b, p, n)` |
| append a u32-length-prefixed string | `bytes.append_u32_be(b, n)`, then `bytes.append_bytes(b, bytes.from_string(s))` |

## Pointer interop (deprecated — use `ffi`)

Deprecated since dialect epoch 15 in favour of the [`ffi`](ffi.md) module, and
outside the memory-safety guarantee: each call is E100 in a file without
`import ffi` (#4043) and under `--profile critical` (#4008).

| Signature | Purpose |
|---|---|
| `bytes.as_ptr(b) -> RawPtr` | Read-only pointer to data region |
| `bytes.as_mut_ptr(b) -> RawPtr` | Mutable pointer to data region |
| `bytes.from_raw_ptr(ptr, len) -> Bytes` | Wrap a foreign buffer |
| `bytes.copy_to_ptr(b, ptr, cap)` | Copy buffer into a foreign address |
| `bytes.data_ptr(b)` | Address of the data (after the length prefix) |

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (183 functions)

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

// Mutable address of the data; null if empty.
// @since 0.11.0 or earlier
bytes.as_mut_ptr(b: Bytes) -> RawPtr   (deprecated — use ffi.address)

// Address of the data; null if empty.
// @since 0.11.0 or earlier
bytes.as_ptr(b: Bytes) -> RawPtr   (deprecated — use ffi.address)

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

// Writes up to cap bytes to ptr; count, 0 if null.
// @since 0.11.0 or earlier
bytes.copy_to_ptr(b: Bytes, ptr: RawPtr, cap: Int) -> Int   (deprecated — use ffi.write)

// Address of the data as an Int; target-specific.
// @since 0.13.2 or earlier
bytes.data_ptr(b: Bytes) -> Int   (deprecated — use ffi.address)

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

// Copy of len bytes at ptr; empty if null or len <= 0.
// @since 0.11.0 or earlier
bytes.from_raw_ptr(ptr: RawPtr, len: Int) -> Bytes   (deprecated — use ffi.read)

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

<!-- END GENERATED SIGNATURE INDEX -->
