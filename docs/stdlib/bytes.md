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
