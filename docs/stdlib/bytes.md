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

## Pointer interop (advanced)

For zero-copy interop with native code (e.g. when calling Rust runtime directly).

| Signature | Purpose |
|---|---|
| `bytes.as_ptr(b) -> RawPtr` | Read-only pointer to data region |
| `bytes.as_mut_ptr(b) -> RawPtr` | Mutable pointer to data region |
| `bytes.from_raw_ptr(ptr, len) -> Bytes` | Wrap a foreign buffer |
| `bytes.copy_to_ptr(b, ptr, cap)` | Copy buffer into a foreign address |
| `bytes.data_ptr(b)` | Address of the data (after the length prefix) |

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (148 functions)

```
// Appends val rounded to f32, BE.
bytes.append_f32_be(b: Bytes, val: Float) -> Unit

// Appends val rounded to f32, LE.
bytes.append_f32_le(b: Bytes, val: Float) -> Unit

// Appends val as 8-byte f64, BE.
bytes.append_f64_be(b: Bytes, val: Float) -> Unit

// Appends val as 8-byte f64, LE.
bytes.append_f64_le(b: Bytes, val: Float) -> Unit

// Appends val's low 16 bits, BE.
bytes.append_i16_be(b: Bytes, val: Int) -> Unit

// Appends val's low 16 bits, LE.
bytes.append_i16_le(b: Bytes, val: Int) -> Unit

// Appends val's low 32 bits, BE.
bytes.append_i32_be(b: Bytes, val: Int) -> Unit

// Appends val's low 32 bits, LE.
bytes.append_i32_le(b: Bytes, val: Int) -> Unit

// Appends val as 8 bytes, BE.
bytes.append_i64_be(b: Bytes, val: Int) -> Unit

// Appends val as 8 bytes, LE.
bytes.append_i64_le(b: Bytes, val: Int) -> Unit

// Appends val's low 16 bits, BE.
bytes.append_u16_be(b: Bytes, val: Int) -> Unit

// Appends val's low 16 bits, LE.
bytes.append_u16_le(b: Bytes, val: Int) -> Unit

// Appends val's low 32 bits, BE.
bytes.append_u32_be(b: Bytes, val: Int) -> Unit

// Appends val's low 32 bits, LE.
bytes.append_u32_le(b: Bytes, val: Int) -> Unit

// Appends val's low 8 bits; 256 becomes 0.
bytes.append_u8(b: Bytes, val: Int) -> Unit

// Mutable address of the data; null if empty.
bytes.as_mut_ptr(b: Bytes) -> RawPtr

// Address of the data; null if empty.
bytes.as_ptr(b: Bytes) -> RawPtr

// size-byte pieces, last may be short; [] if size <= 0.
bytes.chunks(b: Bytes, size: Int) -> List[Bytes]

// Truncates b to length 0 in place.
bytes.clear(b: Bytes) -> Unit

// Lexicographic -1, 0 or 1; a prefix sorts first.
bytes.cmp(a: Bytes, b: Bytes) -> Int

// New Bytes holding a then b.
bytes.concat(a: Bytes, b: Bytes) -> Bytes

// True if pattern occurs in b, or pattern is empty.
bytes.contains(b: Bytes, pattern: Bytes) -> Bool

// Copies len bytes of src into dst; clamped to fit.
bytes.copy_from(dst: Bytes, src: Bytes, dst_off: Int, src_off: Int, len: Int) -> Unit

// Writes up to cap bytes to ptr; count, 0 if null.
bytes.copy_to_ptr(b: Bytes, ptr: RawPtr, cap: Int) -> Int

// Address of the data as an Int; target-specific.
bytes.data_ptr(b: Bytes) -> Int

// True if b ends with suffix, or suffix is empty.
bytes.ends_with(b: Bytes, suffix: Bytes) -> Bool

// True if pos >= len or pos is negative.
bytes.eof(b: Bytes, pos: Int) -> Bool

// Sets every byte to val's low 8 bits in place.
bytes.fill(b: Bytes, val: Int) -> Unit

// xs as bytes, low 8 bits each; -1 becomes 255.
bytes.from_list(xs: List[Int]) -> Bytes

// Copy of len bytes at ptr; empty if null or len <= 0.
bytes.from_raw_ptr(ptr: RawPtr, len: Int) -> Bytes

// UTF-8 encoding of s.
bytes.from_string(s: String) -> Bytes

// Byte at index i, or none when out of range.
bytes.get(b: Bytes, i: Int) -> Option[Int]

// Byte at index i, or default when out of range.
bytes.get_or(b: Bytes, i: Int, default: Int) -> Int

// First offset of pattern, or none; some(0) if empty.
bytes.index_of(b: Bytes, pattern: Bytes) -> Option[Int]

// Copy with val's low byte at pos, clamped to 0..len.
bytes.insert(b: Bytes, pos: Int, val: Int) -> Bytes

// True when b holds no bytes.
bytes.is_empty(b: Bytes) -> Bool

// Whether b is valid UTF-8; true when empty.
bytes.is_valid_utf8(b: Bytes) -> Bool

// Byte count; 0 for an empty buffer.
bytes.len(b: Bytes) -> Int

// Pieces split at LF; CR kept; a final LF adds no piece.
bytes.lines(b: Bytes) -> List[Bytes]

// f applied per byte; results cut to low 8 bits.
bytes.map_each(b: Bytes, f: (Int) -> Int) -> Bytes

// len zero bytes; empty if len <= 0.
bytes.new(len: Int) -> Bytes

// Left-padded with val to target_len; as-is if longer.
bytes.pad_left(b: Bytes, target_len: Int, val: Int) -> Bytes

// Right-padded with val to target_len; as-is if longer.
bytes.pad_right(b: Bytes, target_len: Int, val: Int) -> Bytes

// Appends val's low 8 bits in place.
bytes.push(b: Bytes, val: Int) -> Unit

// Sets byte i to val's low 8 bits; no-op out of range.
bytes.set_at(b: Bytes, i: Int, val: Int) -> Unit

// Copies src_start..src_end to dst; no-op if it won't fit.
bytes.copy_within(b: Bytes, src_start: Int, src_end: Int, dst: Int) -> Unit

// Byte at pos != 0; false if out of range.
bytes.read_bool(b: Bytes, pos: Int) -> Bool

// (pos+1, byte != 0); (pos, none) at EOF.
bytes.read_bool_at(b: Bytes, pos: Int) -> (Int, Option[Bool])

// f16 LE at pos; 0.0 if out of range.
bytes.read_f16_le(b: Bytes, pos: Int) -> Float

// (pos+2, f16 LE); (pos, none) at EOF.
bytes.read_f16_le_at(b: Bytes, pos: Int) -> (Int, Option[Float])

// count f16 LE from pos; 0.0 past the end.
bytes.read_f16_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]

// f32 BE at pos; 0.0 if out of range.
bytes.read_f32_be(b: Bytes, pos: Int) -> Float

// count f32 BE from pos; 0.0 past the end.
bytes.read_f32_be_array(b: Bytes, pos: Int, count: Int) -> List[Float]

// (pos+4, f32 BE); (pos, none) at EOF.
bytes.read_f32_be_at(b: Bytes, pos: Int) -> (Int, Option[Float])

// f32 LE at pos; 0.0 if out of range.
bytes.read_f32_le(b: Bytes, pos: Int) -> Float

// count f32 LE from pos; 0.0 past the end.
bytes.read_f32_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]

// (pos+4, f32 LE); (pos, none) at EOF.
bytes.read_f32_le_at(b: Bytes, pos: Int) -> (Int, Option[Float])

// f64 BE at pos; 0.0 if out of range.
bytes.read_f64_be(b: Bytes, pos: Int) -> Float

// count f64 BE from pos; 0.0 past the end.
bytes.read_f64_be_array(b: Bytes, pos: Int, count: Int) -> List[Float]

// (pos+8, f64 BE); (pos, none) at EOF.
bytes.read_f64_be_at(b: Bytes, pos: Int) -> (Int, Option[Float])

// f64 LE at pos; 0.0 if out of range.
bytes.read_f64_le(b: Bytes, pos: Int) -> Float

// count f64 LE from pos; 0.0 past the end.
bytes.read_f64_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]

// (pos+8, f64 LE); (pos, none) at EOF.
bytes.read_f64_le_at(b: Bytes, pos: Int) -> (Int, Option[Float])

// i16 BE at pos; 0 if out of range.
bytes.read_i16_be(b: Bytes, pos: Int) -> Int

// (pos+2, i16 BE); (pos, none) at EOF.
bytes.read_i16_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])

// count i16 BE from pos; 0 past the end.
bytes.read_i16_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]

// i16 LE at pos; 0 if out of range.
bytes.read_i16_le(b: Bytes, pos: Int) -> Int

// (pos+2, i16 LE); (pos, none) at EOF.
bytes.read_i16_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])

// count i16 LE from pos; 0 past the end.
bytes.read_i16_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]

// i32 BE at pos; 0 if out of range.
bytes.read_i32_be(b: Bytes, pos: Int) -> Int

// count i32 BE from pos; 0 past the end.
bytes.read_i32_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]

// (pos+4, i32 BE); (pos, none) at EOF.
bytes.read_i32_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])

// i32 LE at pos; 0 if out of range.
bytes.read_i32_le(b: Bytes, pos: Int) -> Int

// count i32 LE from pos; 0 past the end.
bytes.read_i32_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]

// (pos+4, i32 LE); (pos, none) at EOF.
bytes.read_i32_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])

// i64 BE at pos; 0 if out of range.
bytes.read_i64_be(b: Bytes, pos: Int) -> Int

// count i64 BE from pos; 0 past the end.
bytes.read_i64_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]

// (pos+8, i64 BE); (pos, none) at EOF.
bytes.read_i64_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])

// i64 LE at pos; 0 if out of range.
bytes.read_i64_le(b: Bytes, pos: Int) -> Int

// count i64 LE from pos; 0 past the end.
bytes.read_i64_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]

// (pos+8, i64 LE); (pos, none) at EOF.
bytes.read_i64_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])

// Up to count u32-LE-prefixed strings; stops if cut off.
bytes.read_length_prefixed_strings_le(b: Bytes, pos: Int, count: Int) -> List[String]

// len bytes at pos, lossy UTF-8; empty if out of range.
bytes.read_string_at(b: Bytes, pos: Int, len: Int) -> String

// String after a u32 BE length at pos; empty if cut off.
bytes.read_string_be(b: Bytes, pos: Int) -> String

// Cursor form of read_string_be; (pos, none) if cut off.
bytes.read_string_be_at(b: Bytes, pos: Int) -> (Int, Option[String])

// u16 BE at pos; 0 if out of range.
bytes.read_u16_be(b: Bytes, pos: Int) -> Int

// (pos+2, u16 BE); (pos, none) at EOF.
bytes.read_u16_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])

// count u16 BE from pos; 0 past the end.
bytes.read_u16_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]

// u16 LE at pos; 0 if out of range.
bytes.read_u16_le(b: Bytes, pos: Int) -> Int

// count u16 LE from pos; 0 past the end.
bytes.read_u16_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]

// (pos+2, u16 LE); (pos, none) at EOF.
bytes.read_u16_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])

// u32 BE at pos; 0 if out of range.
bytes.read_u32_be(b: Bytes, pos: Int) -> Int

// count u32 BE from pos; 0 past the end.
bytes.read_u32_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]

// (pos+4, u32 BE); (pos, none) at EOF.
bytes.read_u32_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])

// u32 LE at pos; 0 if out of range.
bytes.read_u32_le(b: Bytes, pos: Int) -> Int

// count u32 LE from pos; 0 past the end.
bytes.read_u32_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]

// (pos+4, u32 LE); (pos, none) at EOF.
bytes.read_u32_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])

// Byte at pos; 0 if out of range.
bytes.read_u8(b: Bytes, pos: Int) -> Int

// (pos+1, byte); (pos, none) at EOF.
bytes.read_u8_at(b: Bytes, pos: Int) -> (Int, Option[Int])

// Copy without the byte at pos; as-is if out of range.
bytes.remove_at(b: Bytes, pos: Int) -> Bytes

// b concatenated n times; empty if n <= 0.
bytes.repeat(b: Bytes, n: Int) -> Bytes

// Copy with the byte order reversed.
bytes.reverse(b: Bytes) -> Bytes

// Copy with byte i set to val; as-is if out of range.
bytes.set(b: Bytes, i: Int, val: Int) -> Bytes

// Writes f32 BE at pos; no-op out of range.
bytes.set_f32_be(b: Bytes, pos: Int, val: Float) -> Unit

// Writes f32 LE at pos; no-op out of range.
bytes.set_f32_le(b: Bytes, pos: Int, val: Float) -> Unit

// Writes f64 BE at pos; no-op out of range.
bytes.set_f64_be(b: Bytes, pos: Int, val: Float) -> Unit

// Writes f64 LE at pos; no-op out of range.
bytes.set_f64_le(b: Bytes, pos: Int, val: Float) -> Unit

// Writes i16 BE at pos; no-op out of range.
bytes.set_i16_be(b: Bytes, pos: Int, val: Int) -> Unit

// Writes i16 LE at pos; no-op out of range.
bytes.set_i16_le(b: Bytes, pos: Int, val: Int) -> Unit

// Writes i32 BE at pos; no-op out of range.
bytes.set_i32_be(b: Bytes, pos: Int, val: Int) -> Unit

// Writes i32 LE at pos; no-op out of range.
bytes.set_i32_le(b: Bytes, pos: Int, val: Int) -> Unit

// Writes i64 BE at pos; no-op out of range.
bytes.set_i64_be(b: Bytes, pos: Int, val: Int) -> Unit

// Writes i64 LE at pos; no-op out of range.
bytes.set_i64_le(b: Bytes, pos: Int, val: Int) -> Unit

// Writes u16 BE at pos; no-op out of range.
bytes.set_u16_be(b: Bytes, pos: Int, val: Int) -> Unit

// Writes u16 LE at pos; no-op out of range.
bytes.set_u16_le(b: Bytes, pos: Int, val: Int) -> Unit

// Writes u32 BE at pos; no-op out of range.
bytes.set_u32_be(b: Bytes, pos: Int, val: Int) -> Unit

// Writes u32 LE at pos; no-op out of range.
bytes.set_u32_le(b: Bytes, pos: Int, val: Int) -> Unit

// Writes val's low byte at pos; no-op out of range.
bytes.set_u8(b: Bytes, pos: Int, val: Int) -> Unit

// pos + n, capped at len.
bytes.skip(b: Bytes, pos: Int, n: Int) -> Int

// Offset past count u32-LE-prefixed records.
bytes.skip_length_prefixed_le(b: Bytes, pos: Int, count: Int) -> Int

// Copy of start..end capped at len; empty if start >= end.
bytes.slice(b: Bytes, start: Int, end: Int) -> Bytes

// Pieces between sep matches; [b] for an empty sep.
bytes.split(b: Bytes, sep: Bytes) -> List[Bytes]

// True if b begins with prefix, or prefix is empty.
bytes.starts_with(b: Bytes, prefix: Bytes) -> Bool

// (pos+n, n-byte copy); (pos, none) if out of range.
bytes.take_at(b: Bytes, pos: Int, n: Int) -> (Int, Option[Bytes])

// Each byte as an Int in 0..255.
bytes.to_list(b: Bytes) -> List[Int]

// UTF-8 decode; err "invalid UTF-8: ..." if bad.
bytes.to_string(b: Bytes) -> Result[String, String]

// UTF-8 decode; invalid sequences become U+FFFD.
bytes.to_string_lossy(b: Bytes) -> String

// Appends 1 for true, 0 for false.
bytes.write_bool(b: Bytes, val: Bool) -> Unit

// Appends val as 8-byte f64, BE.
bytes.write_f64_be(b: Bytes, val: Float) -> Unit

// Appends val as 8 bytes, BE.
bytes.write_i64_be(b: Bytes, val: Int) -> Unit

// Appends s after a u32 BE byte-length prefix.
bytes.write_string_be(b: Bytes, s: String) -> Unit

// Appends val's low 32 bits, BE.
bytes.write_u32_be(b: Bytes, val: Int) -> Unit

// Appends val's low 8 bits; 256 becomes 0.
bytes.write_u8(b: Bytes, val: Int) -> Unit

// Bytewise XOR, as long as the shorter input.
bytes.xor(a: Bytes, b: Bytes) -> Bytes

// Opaque arena checkpoint; currently always 0.
bytes.heap_save() -> Int

// Restores a heap_save checkpoint; currently a no-op.
bytes.heap_restore(checkpoint: Int) -> Unit

// UInt16 at offset; 0 if out of range.
bytes.read_uint16(b: Bytes, offset: Int, endian: Endian) -> UInt16

// UInt32 at offset; 0 if out of range.
bytes.read_uint32(b: Bytes, offset: Int, endian: Endian) -> UInt32

// Int32 at offset; 0 if out of range.
bytes.read_int32(b: Bytes, offset: Int, endian: Endian) -> Int32

// Float32 at offset; 0 if out of range.
bytes.read_float32(b: Bytes, offset: Int, endian: Endian) -> Float32

// Appends value as 2 bytes in endian order.
bytes.write_uint16(b: Bytes, value: UInt16, endian: Endian) -> Unit

// Appends value as 4 bytes in endian order.
bytes.write_uint32(b: Bytes, value: UInt32, endian: Endian) -> Unit

// Appends value as 4 bytes in endian order.
bytes.write_int32(b: Bytes, value: Int32, endian: Endian) -> Unit

// Appends value as 4 bytes in endian order.
bytes.write_float32(b: Bytes, value: Float32, endian: Endian) -> Unit

// Writes value at offset; no-op out of range.
bytes.set_uint16(b: Bytes, offset: Int, value: UInt16, endian: Endian) -> Unit

// Writes value at offset; no-op out of range.
bytes.set_uint32(b: Bytes, offset: Int, value: UInt32, endian: Endian) -> Unit

// Writes value at offset; no-op out of range.
bytes.set_int32(b: Bytes, offset: Int, value: Int32, endian: Endian) -> Unit

// Writes value at offset; no-op out of range.
bytes.set_float32(b: Bytes, offset: Int, value: Float32, endian: Endian) -> Unit
```

## Type index (1 types)

```
type bytes.Endian = LittleEndian | BigEndian
```

<!-- END GENERATED SIGNATURE INDEX -->
