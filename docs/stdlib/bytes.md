# bytes

Binary data manipulation. `import bytes`.

`Bytes` is a contiguous, mutable-in-place byte buffer with a length prefix.
Most operations are O(1) or O(n).

For source scanners, convert once with `bytes.from_string(source)` and use
`bytes.get_or(buffer, offset, -1)` while scanning. This uses compact `u8`
storage on native; `string.to_bytes` instead returns `List[Int]` (`i64`
elements). Neither conversion is a zero-copy borrowed view. To extract a
UTF-8 token by its byte offsets, use `string.byte_slice(source, start, end)`.

## Naming convention

- `read_<dtype>_le|be(b, pos)` — read one value at a byte offset (no advance).
- `read_<dtype>_le_array(b, pos, count)` — read `count` consecutive values into a `List[T]`.
- `set_<dtype>_le(b, pos, val)` — overwrite at offset (length unchanged).
- `append_<dtype>_le(b, val)` — append to the end (length grows by sizeof(dtype)).
- `write_<dtype>_be(b, val)` — append big-endian (legacy; prefer `append_*_le`).

`<dtype>` is one of `u8 | u16 | u32 | i32 | i64 | f16 | f32 | f64` (or `bool`/`string` for the BE family). Almide `Int` is i64 and `Float` is f64; smaller widths are sign- or zero-extended on read and truncated on write.

The writers — `set_*`, `append_*`, `write_*`, `push`, `set_at`, `fill`,
`clear`, `copy_from`, `copy_within` — change their receiver in place, and the
receiver is a `mut` parameter: pass a `var` (or a `mut` parameter of your own).
A `let` binding, a plain parameter or a temporary is E032.

```almd check
fn stamp(mut b: Bytes, v: Int) -> Unit = bytes.set_u8(b, 0, v)  // a helper that fills its caller's buffer

fn main() -> Unit = {
  var buf = bytes.new(4)   // `var`, not `let`
  bytes.append_u8(buf, 7)
  stamp(buf, 9)
}
```

## Construction & inspection

| Signature | Purpose |
|---|---|
| `bytes.new(len: Int) -> Bytes` | Allocate `len` zeroed bytes |
| `bytes.from_list(xs: List[Int]) -> Bytes` | From a list of byte values |
| `bytes.from_string(s: String) -> Bytes` | Copy UTF-8 into a compact byte buffer (one byte per element) |
| `bytes.to_list(b) -> List[Int]` | Materialise as a list |
| `bytes.len(b) -> Int` | Length |
| `bytes.is_empty(b) -> Bool` | Length == 0 |
| `bytes.get(b, i) -> Option[Int]` | Single byte |
| `bytes.get_or(b, i, default) -> Int` | Single byte with fallback |

## Slicing & combining

| Signature | Purpose |
|---|---|
| `bytes.slice(b, start, end) -> Bytes` | Half-open slice |
| `bytes.concat(a, b) -> Bytes` | Concatenate |
| `bytes.repeat(b, n) -> Bytes` | Repeat n times |
| `bytes.set(b, i, val) -> Bytes` | Replace one byte |
| `bytes.push(b, val)` | Append one byte (mutates) |
| `bytes.clear(b)` | Truncate to length 0 (mutates) |

## Little-endian readers (single value)

| Signature | Width |
|---|---|
| `bytes.read_u8(b, pos)` | 1 byte |
| `bytes.read_u16_le(b, pos)` | 2 bytes |
| `bytes.read_u32_le(b, pos)` | 4 bytes (zero-extended) |
| `bytes.read_i32_le(b, pos)` | 4 bytes (sign-extended) |
| `bytes.read_i64_le(b, pos)` | 8 bytes |
| `bytes.read_f16_le(b, pos)` | 2 bytes → Float (IEEE-754 half) |
| `bytes.read_f32_le(b, pos)` | 4 bytes → Float (promoted) |
| `bytes.read_f64_le(b, pos)` | 8 bytes → Float |

## Little-endian readers (bulk arrays)

Each returns a `List[T]` — one native call beats `count` Almide-side reads.

| Signature | Element width |
|---|---|
| `bytes.read_i32_le_array(b, pos, count)` | 4 bytes → `List[Int]` |
| `bytes.read_u32_le_array(b, pos, count)` | 4 bytes → `List[Int]` |
| `bytes.read_i64_le_array(b, pos, count)` | 8 bytes → `List[Int]` |
| `bytes.read_f16_le_array(b, pos, count)` | 2 bytes → `List[Float]` |
| `bytes.read_f32_le_array(b, pos, count)` | 4 bytes → `List[Float]` |
| `bytes.read_f64_le_array(b, pos, count)` | 8 bytes → `List[Float]` |

## Little-endian writers

`set_*_le` overwrites at a fixed position; `append_*_le` grows the buffer.

| Signature | Effect |
|---|---|
| `bytes.set_u8(b, pos, val)` | Overwrite 1 byte |
| `bytes.set_u16_le(b, pos, val)` | Overwrite 2 bytes |
| `bytes.set_u32_le(b, pos, val)` | Overwrite 4 bytes |
| `bytes.set_i32_le(b, pos, val)` | Overwrite 4 bytes |
| `bytes.set_i64_le(b, pos, val)` | Overwrite 8 bytes |
| `bytes.set_f32_le(b, pos, val)` | Overwrite 4 bytes (demoted from f64) |
| `bytes.set_f64_le(b, pos, val)` | Overwrite 8 bytes |
| `bytes.append_u8(b, val)` | Append 1 byte |
| `bytes.append_u16_le(b, val)` | Append 2 bytes |
| `bytes.append_u32_le(b, val)` | Append 4 bytes |
| `bytes.append_i32_le(b, val)` | Append 4 bytes |
| `bytes.append_i64_le(b, val)` | Append 8 bytes |
| `bytes.append_f32_le(b, val)` | Append 4 bytes (demoted from f64) |
| `bytes.append_f64_le(b, val)` | Append 8 bytes |

## Big-endian (network protocols)

Single-value readers (returns 0 on out-of-bounds — see roadmap for `_at` variants):

| Signature | Width |
|---|---|
| `bytes.read_u32_be(b, pos)` | 4 bytes |
| `bytes.read_i32_be(b, pos)` | 4 bytes (sign-extended) |
| `bytes.read_i64_be(b, pos)` | 8 bytes |
| `bytes.read_f32_be(b, pos)` | 4 bytes → Float |
| `bytes.read_f64_be(b, pos)` | 8 bytes → Float |
| `bytes.read_string_be(b, pos)` | length-prefixed string |
| `bytes.read_bool(b, pos)` | 1-byte bool |

Bulk readers (symmetric to LE):

| Signature | Element width |
|---|---|
| `bytes.read_u32_be_array(b, pos, count)` | 4 bytes → `List[Int]` |
| `bytes.read_i32_be_array(b, pos, count)` | 4 bytes → `List[Int]` |
| `bytes.read_i64_be_array(b, pos, count)` | 8 bytes → `List[Int]` |
| `bytes.read_f32_be_array(b, pos, count)` | 4 bytes → `List[Float]` |
| `bytes.read_f64_be_array(b, pos, count)` | 8 bytes → `List[Float]` |

Appenders (preferred). The `write_*_be` family is the older spelling and remains as an alias.

| Signature | Width |
|---|---|
| `bytes.append_u16_be(b, val)` | 2 bytes |
| `bytes.append_u32_be(b, val)` | 4 bytes |
| `bytes.append_i32_be(b, val)` | 4 bytes |
| `bytes.append_i64_be(b, val)` | 8 bytes |
| `bytes.append_f32_be(b, val)` | 4 bytes (demoted from f64) |
| `bytes.append_f64_be(b, val)` | 8 bytes |
| `bytes.write_string_be(b, s)` | Length-prefixed string |
| `bytes.write_bool(b, val)` | 1-byte bool |

## Higher-level readers

| Signature | Purpose |
|---|---|
| `bytes.read_string_at(b, pos, len)` | UTF-8 substring of `len` bytes |
| `bytes.read_length_prefixed_strings_le(b, pos, count)` | List of `count` length-prefixed strings |
| `bytes.skip_length_prefixed_le(b, pos, count)` | Returns the byte offset past `count` LE-length-prefixed records |

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

## Signature index (209 functions)

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

// New Bytes holding a then b.
// @since 0.9.3 or earlier
bytes.concat(a: Bytes, b: Bytes) -> Bytes

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

// Byte at index i, or default when out of range.
// @since 0.9.3 or earlier
bytes.get_or(b: Bytes, i: Int, default: Int) -> Int

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

// Pieces split at LF; CR kept; a final LF adds no piece.
// @since 0.13.4 or earlier
bytes.lines(b: Bytes) -> List[Bytes]

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

// Sets byte i to val's low 8 bits; no-op out of range.
// @since 0.22.0 or earlier
bytes.set_at(mut b: Bytes, i: Int, val: Int) -> Unit

// Copies src_start..src_end to dst; no-op if it won't fit.
// @since 0.22.0 or earlier
bytes.copy_within(mut b: Bytes, src_start: Int, src_end: Int, dst: Int) -> Unit

// Up to count u32-LE-prefixed strings; stops if cut off.
// @since 0.13.4 or earlier
bytes.read_length_prefixed_strings_le(b: Bytes, pos: Int, count: Int) -> List[String]

// len bytes at pos, lossy UTF-8; empty if out of range.
// @since 0.13.4 or earlier
bytes.read_string_at(b: Bytes, pos: Int, len: Int) -> String

// String after a u32 BE length at pos; empty if cut off.
// @since 0.11.0 or earlier
bytes.read_string_be(b: Bytes, pos: Int) -> String

// Cursor form of read_string_be; (pos, none) if cut off.
// @since 0.15.0 or earlier
bytes.read_string_be_at(b: Bytes, pos: Int) -> (Int, Option[String])

// Byte at pos; 0 if out of range.
// @since 0.11.0 or earlier
bytes.read_u8(b: Bytes, pos: Int) -> Int

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

// Offset past count u32-LE-prefixed records.
// @since 0.13.4 or earlier
bytes.skip_length_prefixed_le(b: Bytes, pos: Int, count: Int) -> Int

// Copy of start..end capped at len; empty if start >= end.
// @since 0.9.3 or earlier
bytes.slice(b: Bytes, start: Int, end: Int) -> Bytes

// Pieces between sep matches; [b] for an empty sep.
// @since 0.13.4 or earlier
bytes.split(b: Bytes, sep: Bytes) -> List[Bytes]

// True if b begins with prefix, or prefix is empty.
// @since 0.13.4 or earlier
bytes.starts_with(b: Bytes, prefix: Bytes) -> Bool

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

// Appends val as 8-byte f64, BE.
// @since 0.11.0 or earlier
bytes.write_f64_be(mut b: Bytes, val: Float) -> Unit

// Appends val as 8 bytes, BE.
// @since 0.11.0 or earlier
bytes.write_i64_be(mut b: Bytes, val: Int) -> Unit

// Appends s after a u32 BE byte-length prefix.
// @since 0.11.0 or earlier
bytes.write_string_be(mut b: Bytes, s: String) -> Unit

// Appends val's low 32 bits, BE.
// @since 0.11.0 or earlier
bytes.write_u32_be(mut b: Bytes, val: Int) -> Unit

// Appends val's low 8 bits; 256 becomes 0.
// @since 0.11.0 or earlier
bytes.write_u8(mut b: Bytes, val: Int) -> Unit

// Bytewise XOR, as long as the shorter input.
// @since 0.13.4 or earlier
bytes.xor(a: Bytes, b: Bytes) -> Bytes

// UInt16 at offset; 0 if out of range.
// @since 0.15.0 or earlier
bytes.read_uint16(b: Bytes, offset: Int, endian: Endian) -> UInt16

// UInt32 at offset; 0 if out of range.
// @since 0.15.0 or earlier
bytes.read_uint32(b: Bytes, offset: Int, endian: Endian) -> UInt32

// Int32 at offset; 0 if out of range.
// @since 0.15.0 or earlier
bytes.read_int32(b: Bytes, offset: Int, endian: Endian) -> Int32

// Float32 at offset; 0 if out of range.
// @since 0.15.0 or earlier
bytes.read_float32(b: Bytes, offset: Int, endian: Endian) -> Float32

// Appends value as 2 bytes in endian order.
// @since 0.15.0 or earlier
bytes.write_uint16(mut b: Bytes, value: UInt16, endian: Endian) -> Unit

// Appends value as 4 bytes in endian order.
// @since 0.15.0 or earlier
bytes.write_uint32(mut b: Bytes, value: UInt32, endian: Endian) -> Unit

// Appends value as 4 bytes in endian order.
// @since 0.15.0 or earlier
bytes.write_int32(mut b: Bytes, value: Int32, endian: Endian) -> Unit

// Appends value as 4 bytes in endian order.
// @since 0.15.0 or earlier
bytes.write_float32(mut b: Bytes, value: Float32, endian: Endian) -> Unit

// Writes value at offset; no-op out of range.
// @since 0.15.0 or earlier
bytes.set_uint16(mut b: Bytes, offset: Int, value: UInt16, endian: Endian) -> Unit

// Writes value at offset; no-op out of range.
// @since 0.15.0 or earlier
bytes.set_uint32(mut b: Bytes, offset: Int, value: UInt32, endian: Endian) -> Unit

// Writes value at offset; no-op out of range.
// @since 0.15.0 or earlier
bytes.set_int32(mut b: Bytes, offset: Int, value: Int32, endian: Endian) -> Unit

// Writes value at offset; no-op out of range.
// @since 0.15.0 or earlier
bytes.set_float32(mut b: Bytes, offset: Int, value: Float32, endian: Endian) -> Unit

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

// The first n bytes; all of b past its length; n < 0 aborts.
// @since unreleased
bytes.take(b: Bytes, n: Int) -> Bytes

// b without its first n bytes; empty past its length; n < 0 aborts.
// @since unreleased
bytes.drop(b: Bytes, n: Int) -> Bytes

// The last n bytes; all of b past its length; n < 0 aborts.
// @since unreleased
bytes.take_end(b: Bytes, n: Int) -> Bytes

// b without its last n bytes; empty past its length; n < 0 aborts.
// @since unreleased
bytes.drop_end(b: Bytes, n: Int) -> Bytes

// (first n bytes, the rest); n past the end gives (b, empty); n < 0 aborts.
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

## Type index (1 types)

```
// Byte order: LittleEndian or BigEndian.
// @since 0.15.0 or earlier
type bytes.Endian = LittleEndian | BigEndian
```

<!-- END GENERATED SIGNATURE INDEX -->
