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

Read a single line from standard input. The newline (and a trailing `\r`) is
cut. At end of input it returns `""` — the same value it returns for an empty
line, so it cannot tell the two apart; use `io.read_line_opt` when the loop
has to stop at the end of input.

```almd check
import io

effect fn main() -> Unit = {
  let name = io.read_line()
  println("Hello, ${name}!")
}
```

### `io.read_line_opt() -> String?`

Read a single line from standard input, or `none` at end of input (#2539).
An empty line is `some("")`, so a loop can skip empty lines and still stop
when the input ends (Ctrl+D, or the end of a pipe). Same line cutting as
`io.read_line`, and both read from the same stdin, so they can be mixed.

```almd check
import io

effect fn main() -> Unit = {
  var open = true
  while open {
    match io.read_line_opt() {
      none => {
        open = false
      },
      some("") => (),
      some(line) => println("> ${line}"),
    }
  }
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

## Signature index (8 functions)

```
// Next stdin line, newline cut; "" at EOF too — read_line_opt tells them apart.
// @since 0.5.0 or earlier
effect io.read_line() -> String

// Next stdin line, newline cut; none at EOF, some("") for an empty line.
// @since unreleased
effect io.read_line_opt() -> Option[String]

// Writes s, no newline, then flushes.
// @since 0.5.0 or earlier
effect io.print(s: String) -> Unit

// Rest of stdin as text; empty at EOF.
// @since 0.5.0 or earlier
effect io.read_all() -> String

// Next stdin byte 0..255, or -1 at EOF.
// @since 0.12.1 or earlier
io.read_byte() -> Int

// Up to n stdin bytes; [] if n <= 0.
// @since 0.12.1 or earlier
io.read_n_bytes(n: Int) -> List[Int]

// Low byte of each Int to stdout; unflushed.
// @since 0.9.8 or earlier
io.write_bytes(data: List[Int]) -> Unit

// Raw data to stdout; no newline.
// @since 0.9.8 or earlier
io.write(data: Bytes) -> Unit
```

<!-- END GENERATED SIGNATURE INDEX -->
