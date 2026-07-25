# bytes

Binary data manipulation. `import bytes`.

`Bytes` is a contiguous, mutable-in-place byte buffer with a length prefix.
Most operations are O(1) or O(n).

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
| `bytes.from_string(s: String) -> Bytes` | UTF-8 view of a string (zero-copy) |
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
bytes.append_f32_be(b: Bytes, val: Float) -> Unit
bytes.append_f32_le(b: Bytes, val: Float) -> Unit
bytes.append_f64_be(b: Bytes, val: Float) -> Unit
bytes.append_f64_le(b: Bytes, val: Float) -> Unit
bytes.append_i16_be(b: Bytes, val: Int) -> Unit
bytes.append_i16_le(b: Bytes, val: Int) -> Unit
bytes.append_i32_be(b: Bytes, val: Int) -> Unit
bytes.append_i32_le(b: Bytes, val: Int) -> Unit
bytes.append_i64_be(b: Bytes, val: Int) -> Unit
bytes.append_i64_le(b: Bytes, val: Int) -> Unit
bytes.append_u16_be(b: Bytes, val: Int) -> Unit
bytes.append_u16_le(b: Bytes, val: Int) -> Unit
bytes.append_u32_be(b: Bytes, val: Int) -> Unit
bytes.append_u32_le(b: Bytes, val: Int) -> Unit
bytes.append_u8(b: Bytes, val: Int) -> Unit
bytes.as_mut_ptr(b: Bytes) -> ?
bytes.as_ptr(b: Bytes) -> ?
bytes.chunks(b: Bytes, size: Int) -> List[Bytes]
bytes.clear(b: Bytes) -> Unit
bytes.cmp(a: Bytes, b: Bytes) -> Int
bytes.concat(a: Bytes, b: Bytes) -> Bytes
bytes.contains(b: Bytes, pattern: Bytes) -> Bool
bytes.copy_from(dst: Bytes, src: Bytes, dst_off: Int, src_off: Int, len: Int) -> Unit
bytes.copy_to_ptr(b: Bytes, ptr: ?, cap: Int) -> Int
bytes.data_ptr(b: Bytes) -> Int
bytes.ends_with(b: Bytes, suffix: Bytes) -> Bool
bytes.eof(b: Bytes, pos: Int) -> Bool
bytes.fill(b: Bytes, val: Int) -> Unit
bytes.from_list(xs: List[Int]) -> Bytes
bytes.from_raw_ptr(ptr: ?, len: Int) -> Bytes
bytes.from_string(s: String) -> Bytes
bytes.get(b: Bytes, i: Int) -> Option[Int]
bytes.get_or(b: Bytes, i: Int, default: Int) -> Int
bytes.index_of(b: Bytes, pattern: Bytes) -> Option[Int]
bytes.insert(b: Bytes, pos: Int, val: Int) -> Bytes
bytes.is_empty(b: Bytes) -> Bool
bytes.is_valid_utf8(b: Bytes) -> Bool
bytes.len(b: Bytes) -> Int
bytes.lines(b: Bytes) -> List[Bytes]
bytes.map_each(b: Bytes, f: (Int) -> Int) -> Bytes
bytes.new(len: Int) -> Bytes
bytes.pad_left(b: Bytes, target_len: Int, val: Int) -> Bytes
bytes.pad_right(b: Bytes, target_len: Int, val: Int) -> Bytes
bytes.push(b: Bytes, val: Int) -> Unit
bytes.set_at(b: Bytes, i: Int, val: Int) -> Unit
bytes.copy_within(b: Bytes, src_start: Int, src_end: Int, dst: Int) -> Unit
bytes.read_bool(b: Bytes, pos: Int) -> Bool
bytes.read_bool_at(b: Bytes, pos: Int) -> ()
bytes.read_f16_le(b: Bytes, pos: Int) -> Float
bytes.read_f16_le_at(b: Bytes, pos: Int) -> ()
bytes.read_f16_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]
bytes.read_f32_be(b: Bytes, pos: Int) -> Float
bytes.read_f32_be_array(b: Bytes, pos: Int, count: Int) -> List[Float]
bytes.read_f32_be_at(b: Bytes, pos: Int) -> ()
bytes.read_f32_le(b: Bytes, pos: Int) -> Float
bytes.read_f32_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]
bytes.read_f32_le_at(b: Bytes, pos: Int) -> ()
bytes.read_f64_be(b: Bytes, pos: Int) -> Float
bytes.read_f64_be_array(b: Bytes, pos: Int, count: Int) -> List[Float]
bytes.read_f64_be_at(b: Bytes, pos: Int) -> ()
bytes.read_f64_le(b: Bytes, pos: Int) -> Float
bytes.read_f64_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]
bytes.read_f64_le_at(b: Bytes, pos: Int) -> ()
bytes.read_i16_be(b: Bytes, pos: Int) -> Int
bytes.read_i16_be_at(b: Bytes, pos: Int) -> ()
bytes.read_i16_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]
bytes.read_i16_le(b: Bytes, pos: Int) -> Int
bytes.read_i16_le_at(b: Bytes, pos: Int) -> ()
bytes.read_i16_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]
bytes.read_i32_be(b: Bytes, pos: Int) -> Int
bytes.read_i32_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]
bytes.read_i32_be_at(b: Bytes, pos: Int) -> ()
bytes.read_i32_le(b: Bytes, pos: Int) -> Int
bytes.read_i32_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]
bytes.read_i32_le_at(b: Bytes, pos: Int) -> ()
bytes.read_i64_be(b: Bytes, pos: Int) -> Int
bytes.read_i64_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]
bytes.read_i64_be_at(b: Bytes, pos: Int) -> ()
bytes.read_i64_le(b: Bytes, pos: Int) -> Int
bytes.read_i64_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]
bytes.read_i64_le_at(b: Bytes, pos: Int) -> ()
bytes.read_length_prefixed_strings_le(b: Bytes, pos: Int, count: Int) -> List[String]
bytes.read_string_at(b: Bytes, pos: Int, len: Int) -> String
bytes.read_string_be(b: Bytes, pos: Int) -> String
bytes.read_string_be_at(b: Bytes, pos: Int) -> ()
bytes.read_u16_be(b: Bytes, pos: Int) -> Int
bytes.read_u16_be_at(b: Bytes, pos: Int) -> ()
bytes.read_u16_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]
bytes.read_u16_le(b: Bytes, pos: Int) -> Int
bytes.read_u16_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]
bytes.read_u16_le_at(b: Bytes, pos: Int) -> ()
bytes.read_u32_be(b: Bytes, pos: Int) -> Int
bytes.read_u32_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]
bytes.read_u32_be_at(b: Bytes, pos: Int) -> ()
bytes.read_u32_le(b: Bytes, pos: Int) -> Int
bytes.read_u32_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]
bytes.read_u32_le_at(b: Bytes, pos: Int) -> ()
bytes.read_u8(b: Bytes, pos: Int) -> Int
bytes.read_u8_at(b: Bytes, pos: Int) -> ()
bytes.remove_at(b: Bytes, pos: Int) -> Bytes
bytes.repeat(b: Bytes, n: Int) -> Bytes
bytes.reverse(b: Bytes) -> Bytes
bytes.set(b: Bytes, i: Int, val: Int) -> Bytes
bytes.set_f32_be(b: Bytes, pos: Int, val: Float) -> Unit
bytes.set_f32_le(b: Bytes, pos: Int, val: Float) -> Unit
bytes.set_f64_be(b: Bytes, pos: Int, val: Float) -> Unit
bytes.set_f64_le(b: Bytes, pos: Int, val: Float) -> Unit
bytes.set_i16_be(b: Bytes, pos: Int, val: Int) -> Unit
bytes.set_i16_le(b: Bytes, pos: Int, val: Int) -> Unit
bytes.set_i32_be(b: Bytes, pos: Int, val: Int) -> Unit
bytes.set_i32_le(b: Bytes, pos: Int, val: Int) -> Unit
bytes.set_i64_be(b: Bytes, pos: Int, val: Int) -> Unit
bytes.set_i64_le(b: Bytes, pos: Int, val: Int) -> Unit
bytes.set_u16_be(b: Bytes, pos: Int, val: Int) -> Unit
bytes.set_u16_le(b: Bytes, pos: Int, val: Int) -> Unit
bytes.set_u32_be(b: Bytes, pos: Int, val: Int) -> Unit
bytes.set_u32_le(b: Bytes, pos: Int, val: Int) -> Unit
bytes.set_u8(b: Bytes, pos: Int, val: Int) -> Unit
bytes.skip(b: Bytes, pos: Int, n: Int) -> Int
bytes.skip_length_prefixed_le(b: Bytes, pos: Int, count: Int) -> Int
bytes.slice(b: Bytes, start: Int, end: Int) -> Bytes
bytes.split(b: Bytes, sep: Bytes) -> List[Bytes]
bytes.starts_with(b: Bytes, prefix: Bytes) -> Bool
bytes.take_at(b: Bytes, pos: Int, n: Int) -> ()
bytes.to_list(b: Bytes) -> List[Int]
bytes.to_string(b: Bytes) -> Result[String, String]
bytes.to_string_lossy(b: Bytes) -> String
bytes.write_bool(b: Bytes, val: Bool) -> Unit
bytes.write_f64_be(b: Bytes, val: Float) -> Unit
bytes.write_i64_be(b: Bytes, val: Int) -> Unit
bytes.write_string_be(b: Bytes, s: String) -> Unit
bytes.write_u32_be(b: Bytes, val: Int) -> Unit
bytes.write_u8(b: Bytes, val: Int) -> Unit
bytes.xor(a: Bytes, b: Bytes) -> Bytes
bytes.heap_save() -> Int
bytes.heap_restore(checkpoint: Int) -> Unit
bytes.read_uint16(b: Bytes, offset: Int, endian: Endian) -> UInt16
bytes.read_uint32(b: Bytes, offset: Int, endian: Endian) -> UInt32
bytes.read_int32(b: Bytes, offset: Int, endian: Endian) -> Int32
bytes.read_float32(b: Bytes, offset: Int, endian: Endian) -> Float32
bytes.write_uint16(b: Bytes, value: UInt16, endian: Endian) -> Unit
bytes.write_uint32(b: Bytes, value: UInt32, endian: Endian) -> Unit
bytes.write_int32(b: Bytes, value: Int32, endian: Endian) -> Unit
bytes.write_float32(b: Bytes, value: Float32, endian: Endian) -> Unit
bytes.set_uint16(b: Bytes, offset: Int, value: UInt16, endian: Endian) -> Unit
bytes.set_uint32(b: Bytes, offset: Int, value: UInt32, endian: Endian) -> Unit
bytes.set_int32(b: Bytes, offset: Int, value: Int32, endian: Endian) -> Unit
bytes.set_float32(b: Bytes, offset: Int, value: Float32, endian: Endian) -> Unit
```

<!-- END GENERATED SIGNATURE INDEX -->
