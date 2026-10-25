# io

The standard streams, and the byte I/O that works over any handle.
`import io`, `effect fn`.

`io` has two layers. The **text shortcuts** — `read_line`, `read_all`,
`print`, `eprint` — read and write the standard streams as `String`. The
**handle functions** — `read`, `read_exact`, `read_to_end`, `write`, `close` —
move `Bytes` over any value that is `Read`, `Write` or `Close`: the standard
streams (`io.stdin()`, `io.stdout()`, `io.stderr()`), a `net.TcpStream`, a
`unix.Fd`. There is one way to read bytes, whatever the source: no module has
its own `tcp_read` or `unix_recv`.

| Handle | Made by | Read | Write | Close |
|---|---|---|---|---|
| `io.Stdin` | `io.stdin()` | yes | | |
| `io.Stdout`, `io.Stderr` | `io.stdout()`, `io.stderr()` | | yes | |
| `net.TcpStream` | `net.tcp_connect`, `net.tcp_accept` | yes | yes | yes |
| `net.TcpListener`, `net.UdpSocket` | `net.tcp_listen`, `net.udp_bind` | | | yes |
| `unix.Fd` | `unix.connect`, `unix.listen`, `unix.accept`, ... | yes | yes | yes |

Passing a handle to a function its type does not support is a type error, so
`io.close(io.stdout()!)` does not compile. A handle cannot be made from an
`Int`: only its own module builds one.

**Calling the stream functions.** `io.stdin()`, `io.stdout()` and
`io.stderr()` are `effect fn`s, and each call yields a `Result`, so write
`io.stdout()!`. Every handle function returns a `Result` too (ADR-0008:
propagation is explicit), so a write is `io.write(io.stdout()!, data)!`.

**stdout is one buffer** (#2245). `println`, `io.print` and
`io.write(io.stdout()!, ...)` all write through the same 64 KiB buffer, so they
appear in program order. The buffer is line-buffered (#3417): a write that ends
a line flushes it, on a terminal, a pipe or a file alike, so a long-running
program (a watcher, a server) shows each line as it prints it. When stdout is a
terminal every write flushes. It is also flushed at exit, when the program
panics or `main` returns an error, before a child process runs, before every
read of stdin, and by `io.print`, which always flushes (so `io.print("")` is
an explicit flush). `eprintln`, `io.eprint` and writes to `io.stderr()` go to
stderr unbuffered, after flushing stdout. When stdout is not a terminal the
relative order of the two streams is not preserved — the bytes on each stream
are. The wasm leg writes each call straight to the host and produces the same
bytes in the same order (C-162).

**Targets.** Native and wasm serve the standard streams and the handle
functions alike. A handle from another module works where that module's
functions work: see [net](./net.md) and [unix](./unix.md).

## Text shortcuts

### `io.read_line() -> String?`

The next line of standard input, or `none` at end of input. The newline (and a
trailing `\r`) is cut. An empty line is `some("")`, so a loop can skip empty
lines and still stop when the input ends (Ctrl+D, or the end of a pipe).

The line's bytes are decoded as UTF-8; each invalid sequence becomes one
`U+FFFD` (`�`), the same on every target (#4142). A read error ends the
input, like end of file.

```almd check
import io

effect fn main() -> Unit = {
  var open = true
  while open {
    match io.read_line() {
      none => {
        open = false
      },
      some("") => (),
      some(line) => println("> ${line}"),
    }
  }
}
```

When one line is all the program wants, `??` gives the end of input a value:

```almd check
import io

effect fn main() -> Unit = {
  let name = io.read_line() ?? "stranger"
  println("Hello, ${name}!")
}
```

### `io.read_all() -> String`

All of the rest of standard input as one string; empty at end of input. The
bytes are decoded as UTF-8; each invalid sequence becomes one `U+FFFD` (`�`),
the same on every target (#4142). A read error ends the input: the bytes read
before it are kept. For the bytes undecoded, use
`io.read_to_end(io.stdin()!)`.

```almd check
import io

effect fn main() -> Unit = {
  let input = io.read_all()
  println("${list.len(string.lines(input))} line(s)")
}
```

### `io.print(s: String) -> Unit`

Write `s` to stdout without a trailing newline, then flush.

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

### `io.eprint(s: String) -> Unit`

Write `s` to stderr without a trailing newline — `eprintln` without the
newline, for a prompt or a progress line on stderr.

```almd check
import io

effect fn main() -> Unit = {
  io.eprint("working... ")
  eprintln("done")
}
```

## The stream handles

### `io.stdin() -> Stdin` / `io.stdout() -> Stdout` / `io.stderr() -> Stderr`

The standard streams as handles, for the functions below. Each call needs `!`.
They share the streams the text shortcuts use: `io.read_line` and
`io.read(io.stdin()!, n)` read the same input, and `println` and
`io.write(io.stdout()!, b)` the same buffer.

## Bytes over any handle

### `io.read(h, max: Int) -> Result[Bytes, String]`

Up to `max` bytes from `h`, blocking until some arrive. A short read is normal:
the result has what arrived, fewer than `max` bytes. It is empty once `h` has
ended (end of input, or the peer closed). A negative `max` is an `err`.

```almd check
import io

effect fn main() -> Unit = {
  let head = io.read(io.stdin()!, 4)!
  println("${bytes.len(head)} byte(s)")
}
```

### `io.read_exact(h, n: Int) -> Result[Bytes, String]`

Exactly `n` bytes from `h`, reading as many times as it takes. If `h` ends
first the result is an `err` naming how many bytes arrived
(`io.read_exact: the stream ended after 3 of 8 bytes`). The right call for a
length-prefixed protocol or a fixed-size header.

```almd check
import io

effect fn main() -> Unit = {
  let header = io.read_exact(io.stdin()!, 8)!
  println("header: ${bytes.len(header)} bytes")
}
```

### `io.read_to_end(h) -> Result[Bytes, String]`

Every byte until `h` ends.

```almd check
import io

effect fn main() -> Unit = {
  let input = io.read_to_end(io.stdin()!)!
  println("${bytes.len(input)} bytes in")
}
```

### `io.write(h, data: Bytes) -> Result[Unit, String]`

Write all of `data` to `h`. On stdout the bytes go into the shared buffer
unchanged — no UTF-8 conversion, no newline.

```almd run
import io

effect fn main() -> Unit = {
  io.write(io.stdout()!, bytes.from_string("Hi\n"))!
}
```
```output
Hi
```

### `io.close(h) -> Result[Unit, String]`

Close `h`; closing it twice is ok. Only handles that own a resource are
`Close` — sockets and descriptors, not the standard streams.

```almd check
import io
import net

effect fn main() -> Unit = {
  let s = net.tcp_connect("example.com", 80)!
  io.write(s, bytes.from_string("HEAD / HTTP/1.0\r\n\r\n"))!
  let reply = io.read(s, 4096)!
  io.close(s)!
  println("${bytes.len(reply)} bytes")
}
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (8 functions)

```
// Next stdin line, newline cut, bad UTF-8 as U+FFFD; "" at EOF too — read_line_opt tells them apart.
// @since 0.5.0 or earlier
effect io.read_line() -> String

// Next stdin line, newline cut; none at EOF, some("") for an empty line.
// @since unreleased
effect io.read_line_opt() -> Option[String]

// Writes s, no newline, then flushes.
// @since 0.5.0 or earlier
effect io.print(s: String) -> Unit

// Rest of stdin as text, bad UTF-8 as U+FFFD; empty at EOF.
// @since 0.5.0 or earlier
effect io.read_all() -> String

// Next stdin byte 0..255, or -1 at EOF.
// @since 0.12.1 or earlier
effect io.read_byte() -> Int

// Up to n stdin bytes; [] if n <= 0.
// @since 0.12.1 or earlier
effect io.read_n_bytes(n: Int) -> List[Int]

// Low byte of each Int to stdout; unflushed.
// @since 0.9.8 or earlier
effect io.write_bytes(data: List[Int]) -> Unit

// Raw data to stdout; no newline.
// @since 0.9.8 or earlier
effect io.write(data: Bytes) -> Unit
```

<!-- END GENERATED SIGNATURE INDEX -->
