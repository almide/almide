# unix

Unix-domain sockets, the file descriptors they carry, and shared-memory files —
the descriptor-passing floor Wayland, PipeWire and D-Bus are built on.
`import unix`, `effect fn`.

A descriptor is an opaque `unix.Fd`: only this module makes one (`connect`,
`listen`, `accept`, `take_fds`, `shm_create`), so a TCP handle or a bare `Int`
can never reach a call here. An `Fd` is `Read`, `Write` and `Close`, so its
bytes go through [io](./io.md) like every other handle's:

- read with `io.read(fd, max)` — blocks until some bytes arrive, and an empty
  result means the peer closed;
- write with `io.write(fd, data)`, or `unix.write_fds` to pass descriptors
  along with the bytes;
- close with `io.close(fd)` — a socket, a listener, a shared-memory file or a
  received descriptor alike. Closing a socket also closes the descriptors it
  received that were never taken.

Every call returns a `Result` — write `!` or handle the `err` (ADR-0008).
`take_fds` returns a plain list, but as an `effect fn` its call yields a
`Result` too: `unix.take_fds(sock)!`.

**Targets.** Native serves every function on a Unix platform (Linux, macOS,
the BSDs); elsewhere they return `err`. `almide run --target wasm` (the
embedded host) serves the same code through interim host ops (#3620, retired
when ADR-0023 step 7 runs that lane as the WASI 0.3 component). WASI has
neither Unix-domain sockets nor shared memory, so a stock artifact
(`almide build --target wasm`) refuses this module at build time (E081).

## Sockets

### `unix.connect(path: String) -> Result[Fd, String]`

A stream socket connected to the listener at `path`.

### `unix.listen(path: String) -> Result[Fd, String]`

A listener bound at `path`, which must not exist yet.

### `unix.accept(listener: Fd) -> Result[Fd, String]`

Block for the next connection on `listener`; its socket.

### `unix.write_fds(sock: Fd, data: Bytes, fds: List[Fd]) -> Result[Unit, String]`

Send all of `data`, with the descriptors `fds` (at most 28) riding with its
first byte (SCM_RIGHTS). The peer receives them as new descriptors of the same
open files.

### `unix.take_fds(sock: Fd) -> List[Fd]`

The descriptors received on `sock` and not yet taken, oldest first. They queue
per socket as `io.read` receives the bytes they rode with.

### `unix.select(xs: List[Fd], timeout_ms: Int) -> Result[List[Fd], String]`

The descriptors among `xs` that are readable (data, a connection, or the peer's
close) within `timeout_ms`, in the order given — one wait for several sources,
such as a Wayland socket and a compositor's event socket. A negative timeout
waits until one is readable; `0` only checks, without waiting, which is what an
event loop needs to pump without blocking. Empty when none became readable.

```almd check
import io
import unix

effect fn main() -> Unit = {
  let l = unix.listen("/tmp/demo.sock")!
  let a = unix.connect("/tmp/demo.sock")!
  let b = unix.accept(l)!
  io.write(a, bytes.from_string("hello"))!
  let ready = unix.select([b], 1000)!
  println("${list.len(ready)} ready")
  let got = io.read(b, 64)!
  println(bytes.to_string_lossy(got))
  io.close(a)!
  io.close(b)!
  io.close(l)!
}
```

## Shared-memory files

An unnamed file to fill and hand to a peer, which maps it: the pixels of a
Wayland buffer, an audio ring. A write is visible at once to every process that
holds the file, mapped or not.

### `unix.shm_create(size: Int) -> Result[Fd, String]`

A new zero-filled unnamed file of `size` bytes.

### `unix.shm_resize(fd: Fd, size: Int) -> Result[Unit, String]`

Grow or shrink the file to `size` bytes.

### `unix.shm_write(fd: Fd, offset: Int, data: Bytes) -> Result[Unit, String]`

Write `data` into the file at byte `offset`. A negative offset, or a write past
the end of the file, aborts the program: `shm_resize` first.

### `unix.shm_read(fd: Fd, offset: Int, len: Int) -> Result[Bytes, String]`

Up to `len` bytes of the file from `offset`; fewer at its end.

```almd check
import io
import unix

effect fn main() -> Unit = {
  let l = unix.listen("/tmp/demo.sock")!
  let a = unix.connect("/tmp/demo.sock")!
  let b = unix.accept(l)!
  let file = unix.shm_create(4096)!
  unix.shm_write(file, 0, bytes.from_string("pixels"))!
  unix.write_fds(a, bytes.from_string("take this"), [file])!
  let _ = io.read(b, 64)!
  let got = unix.take_fds(b)!
  match list.get(got, 0) {
    some(f) => println(bytes.to_string_lossy(unix.shm_read(f, 0, 6)!)),
    none => println("no descriptor arrived"),
  }
}
```
