# bytes

Binary data. import bytes.

### `bytes.len(b: Bytes) -> Int`

Return the number of bytes.

```almd
bytes.len(b) // => 3
```

### `bytes.get(b: Bytes, i: Int) -> Option[Int]`

Get a byte value (0-255) at an index.

```almd
bytes.get(b, 0) // => some(72)
```

### `bytes.get_or(b: Bytes, i: Int, default: Int) -> Int`

Get a byte value at an index, or a default if out of bounds.

```almd
bytes.get_or(b, 0, 0) // => 72
```

### `bytes.set(b: Bytes, i: Int, val: Int) -> Bytes`

Set a byte value at an index (mutates in place for var bindings).

```almd
bytes.set(b, 0, 255)
```

### `bytes.slice(b: Bytes, start: Int, end: Int) -> Bytes`

Extract a sub-range of bytes [start, end).

```almd
bytes.slice(b, 1, 3)
```

### `bytes.from_list(xs: List[Int]) -> Bytes`

Create Bytes from a list of integers (each masked to 0-255).

```almd
bytes.from_list([72, 105]) // => <bytes>
```

### `bytes.to_list(b: Bytes) -> List[Int]`

Convert Bytes to a list of integers (0-255).

```almd
bytes.to_list(b) // => [72, 105]
```

### `bytes.is_empty(b: Bytes) -> Bool`

Check if byte buffer is empty.

```almd
bytes.is_empty?([]) // => true
```

### `bytes.concat(a: Bytes, b: Bytes) -> Bytes`

Concatenate two byte buffers.

```almd
bytes.concat(a, b)
```

### `bytes.repeat(b: Bytes, n: Int) -> Bytes`

Repeat a byte buffer n times.

```almd
bytes.repeat(b, 3)
```

### `bytes.new(len: Int) -> Bytes`

Create a zero-filled byte buffer of given length.

```almd
bytes.new(1024) // => <1024 zero bytes>
```

### `bytes.push(b: Bytes, val: Int) -> Unit`

Append a single byte (0-255) to the buffer (mutates in place).

```almd
bytes.push(buf, 65)
```

### `bytes.clear(b: Bytes) -> Unit`

Remove all bytes from the buffer (mutates in place, keeps capacity).

```almd
bytes.clear(buf)
```

### `bytes.from_string(s: String) -> Bytes`

Convert a string to its UTF-8 byte representation.

```almd
bytes.from_string("Hello") // => <72, 101, 108, 108, 111>
```

### `bytes.write_i64_be(b: Bytes, val: Int) -> Unit`

Append an Int as 8 bytes big-endian.

```almd
bytes.write_i64_be(buf, 42)
```

### `bytes.write_f64_be(b: Bytes, val: Float) -> Unit`

Append a Float as 8 bytes big-endian.

```almd
bytes.write_f64_be(buf, 3.14)
```

### `bytes.write_u32_be(b: Bytes, val: Int) -> Unit`

Append an Int as 4 bytes big-endian (u32).

```almd
bytes.write_u32_be(buf, 256)
```

### `bytes.write_u8(b: Bytes, val: Int) -> Unit`

Append a single byte.

```almd
bytes.write_u8(buf, 0xFF)
```

### `bytes.write_string_be(b: Bytes, s: String) -> Unit`

Append a length-prefixed UTF-8 string (u32 length + bytes).

```almd
bytes.write_string_be(buf, "hello")
```

### `bytes.write_bool(b: Bytes, val: Bool) -> Unit`

Append a boolean as 1 byte (0 or 1).

```almd
bytes.write_bool(buf, true)
```

### `bytes.read_i64_be(b: Bytes, pos: Int) -> Int`

Read an Int from 8 bytes big-endian at position.

```almd
bytes.read_i64_be(buf, 0) // => 42
```

### `bytes.read_f64_be(b: Bytes, pos: Int) -> Float`

Read a Float from 8 bytes big-endian at position.

```almd
bytes.read_f64_be(buf, 0) // => 3.14
```

### `bytes.read_u32_be(b: Bytes, pos: Int) -> Int`

Read an Int from 4 bytes big-endian (u32) at position.

```almd
bytes.read_u32_be(buf, 0) // => 256
```

### `bytes.read_u8(b: Bytes, pos: Int) -> Int`

Read a single byte at position.

```almd
bytes.read_u8(buf, 0) // => 255
```

### `bytes.read_bool(b: Bytes, pos: Int) -> Bool`

Read a boolean from 1 byte at position.

```almd
bytes.read_bool(buf, 0) // => true
```

### `bytes.read_string_be(b: Bytes, pos: Int) -> String`

Read a length-prefixed UTF-8 string at position.

```almd
bytes.read_string_be(buf, 0) // => "hello"
```

### `bytes.as_ptr(b: Bytes) -> RawPtr`

Get raw pointer to buffer data (for C FFI).

```almd
bytes.as_ptr(buf)
```

### `bytes.as_mut_ptr(b: Bytes) -> RawPtr`

Get mutable raw pointer to buffer data (for C FFI).

```almd
bytes.as_mut_ptr(buf)
```

### `bytes.from_raw_ptr(ptr: RawPtr, len: Int) -> Bytes`

Create Bytes from a raw pointer and length (unsafe, for C FFI bridge callee).

```almd
bytes.from_raw_ptr(ptr, 64)
```

### `bytes.copy_to_ptr(b: Bytes, ptr: RawPtr, cap: Int) -> Int`

Copy Bytes to a raw pointer. Returns bytes written.

```almd
bytes.copy_to_ptr(buf, ptr, 1024)
```
