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
| `bytes.set_f32_le(b, pos, val)` | Overwrite 4 bytes |
| `bytes.set_u16_le(b, pos, val)` | Overwrite 2 bytes |
| `bytes.append_u8(b, val)` | Append 1 byte |
| `bytes.append_u16_le(b, val)` | Append 2 bytes |
| `bytes.append_u32_le(b, val)` | Append 4 bytes |
| `bytes.append_i32_le(b, val)` | Append 4 bytes |
| `bytes.append_i64_le(b, val)` | Append 8 bytes |
| `bytes.append_f32_le(b, val)` | Append 4 bytes (demoted from f64) |
| `bytes.append_f64_le(b, val)` | Append 8 bytes |

## Big-endian (legacy / network protocols)

| Signature | Purpose |
|---|---|
| `bytes.read_u32_be(b, pos)` | u32 BE |
| `bytes.read_i64_be(b, pos)` | i64 BE |
| `bytes.read_f64_be(b, pos)` | f64 BE |
| `bytes.read_string_be(b, pos)` | length-prefixed BE string |
| `bytes.read_bool(b, pos)` | 1-byte bool |
| `bytes.write_u8(b, val)` | Append u8 |
| `bytes.write_u32_be(b, val)` | Append u32 BE |
| `bytes.write_i64_be(b, val)` | Append i64 BE |
| `bytes.write_f64_be(b, val)` | Append f64 BE |
| `bytes.write_string_be(b, s)` | Length-prefixed BE string |
| `bytes.write_bool(b, val)` | Append 1-byte bool |

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
