# io

Standard I/O. import io, effect.

**stdout is one buffer** (#2245). `println`, `io.print`, `io.write` and
`io.write_bytes` all write through the same 64 KiB buffer, so they appear in
program order. When stdout is a terminal the buffer flushes after every
write (each line shows as it happens); when it is a pipe or a file it fills
and flushes in blocks — 50,000 short lines cost the time of a handful of
system calls instead of one each. It is flushed at exit, when the program
panics or `main` returns an error, before a child process runs, before every
read of stdin, and by `io.print`, which always flushes (so `io.print("")` is
an explicit flush). `eprintln` writes to stderr unbuffered, so when stdout is
not a terminal the relative order of the two streams is not preserved — the
bytes on each stream are. The wasm leg writes each call straight to the
host and produces the same bytes in the same order (C-162).

### `io.read_line() -> String`

Read a single line from standard input

```almd check
import io

effect fn main() -> Unit = {
  let name = io.read_line()
  println("Hello, ${name}!")
}
```

### `io.print(s: String) -> Unit`

Print a string to stdout without a trailing newline

```almd run
import io

effect fn main() -> Unit = {
  io.print("Enter name: ")
  println("Alice")
}
```
```output
Enter name: Alice
```

### `io.read_all() -> String`

Read all of standard input as a single string

```almd check
import io

effect fn main() -> Unit = {
  let input = io.read_all()
  println(int.to_string(string.len(input)))
}
```

### `io.write_bytes(data: List[Int]) -> Unit`

Write raw bytes to stdout (no UTF-8 conversion)

```almd run
import io

effect fn main() -> Unit = {
  io.write_bytes([0x50, 0x34, 0x0A])
}
```
```output
P4
```

### `io.write(data: Bytes) -> Unit`

Write a Bytes buffer to stdout (zero-copy, buffered)

```almd run
import io

effect fn main() -> Unit = {
  let buf = bytes.from_string("Hi\n")
  io.write(buf)
}
```
```output
Hi
```

### `io.read_byte() -> Int`

Read a single byte from stdin (returns -1 on EOF).

```almd check
import io

effect fn main() -> Unit = {
  let b = io.read_byte()
  println(if b == -1 then "EOF" else int.to_string(b))
}
```

### `io.read_n_bytes(n: Int) -> List[Int]`

Read N bytes from stdin (may return fewer on EOF).

```almd check
import io

effect fn main() -> Unit = {
  let bytes = io.read_n_bytes(4)
  println("${bytes}")
}
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (7 functions)

```
effect io.read_line() -> String
effect io.print(s: String) -> Unit
effect io.read_all() -> String
io.read_byte() -> Int
io.read_n_bytes(n: Int) -> List[Int]
io.write_bytes(data: List[Int]) -> Unit
io.write(data: Bytes) -> Unit
```

<!-- END GENERATED SIGNATURE INDEX -->
