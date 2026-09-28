# net

TCP sockets, Unix-domain sockets and the shared-memory files passed over them.
`import net`, `effect fn`.

Every call except `tcp_is_open` is an `effect fn` — it needs a network
capability, so it can only be called from an `effect fn`. A TCP stream or
listener is an opaque `Int` handle. The `unix_*` and `shm_*` functions take and
return raw file descriptors instead, because a descriptor is what crosses to the
peer.

**Native only.** The wasm leg has no socket floor, so a program using `net`
builds and runs natively but walls on `--target wasm`. The `unix_*` and `shm_*`
functions need a Unix platform (Linux, macOS, the BSDs); elsewhere they return
`err`. `spec/stdlib/net_test.almd`
carries a `// wasm:skip` marker for the same reason.

## Client

### `effect net.tcp_connect(host: String, port: Int) -> Int`

Open a connection and return the stream handle.

```almd
effect fn fetch(host: String) -> Result[Bytes, String] = {
  let s = net.tcp_connect(host, 80)
  net.tcp_write(s, bytes.from_string("GET / HTTP/1.0\r\n\r\n"))
  let body = net.tcp_read(s, 4096)
  net.tcp_close(s)
  ok(body)
}
```

### `effect net.tcp_read(stream: Int, len: Int) -> Bytes`

Read UP TO `len` bytes. A short read is normal — the result can be shorter than
requested, and empty at end of stream.

### `effect net.tcp_read_exact(stream: Int, len: Int) -> Bytes`

Read exactly `len` bytes, erroring if the stream ends first. The right call for
a length-prefixed protocol.

### `effect net.tcp_write(stream: Int, data: Bytes) -> Unit`

Write the whole buffer.

### `effect net.tcp_close(stream: Int) -> Unit`

Close the stream. The handle is invalid afterwards.

### `net.tcp_is_open(stream: Int) -> Bool`

Whether the handle is still open. The one non-effect fn here: it reads local
bookkeeping and performs no I/O.

## Timeouts and readiness

### `effect net.tcp_read_timeout(stream: Int, len: Int, timeout_ms: Int) -> Bytes`

`tcp_read` that gives up after `timeout_ms`.

### `effect net.tcp_set_timeout(stream: Int, timeout_ms: Int) -> Unit`

Set the default timeout for subsequent reads on this stream.

### `effect net.tcp_available(stream: Int) -> Int`

Bytes readable without blocking.

## Server

### `effect net.tcp_listen(host: String, port: Int) -> Int`

Bind and listen; returns a listener handle.

### `effect net.tcp_accept(listener: Int) -> Int`

Block until a client connects; returns its stream handle.

### `effect net.tcp_close_listener(listener: Int) -> Unit`

Stop listening.

```almd
effect fn serve() -> Unit = {
  let l = net.tcp_listen("127.0.0.1", 8080)
  let s = net.tcp_accept(l)
  net.tcp_write(s, bytes.from_string("hi\n"))
  net.tcp_close(s)
  net.tcp_close_listener(l)
}
```

## Unix-domain sockets

A stream socket to a peer on the same machine, which can carry open file
descriptors beside its bytes (SCM_RIGHTS). This is the transport of Wayland,
PipeWire and D-Bus.

### `effect net.unix_connect(path: String) -> Int`

Connect to the listener at `path`; the socket's descriptor.

### `effect net.unix_listen(path: String) -> Int` / `effect net.unix_accept(listener: Int) -> Int`

Listen at `path` (which must not exist yet) and take the next connection.

### `effect net.unix_send(sock: Int, data: Bytes, fds: List[Int]) -> Unit`

Send all of `data`, with the descriptors `fds` (at most 28) attached to its
first byte. The peer receives them as new descriptors of the same open files.

### `effect net.unix_recv(sock: Int, max: Int) -> Bytes` / `effect net.unix_take_fds(sock: Int) -> List[Int]`

Receive up to `max` bytes, blocking until some arrive; an empty result means the
peer closed. Descriptors that arrived with them queue per socket, in order, until
`unix_take_fds` takes them.

### `effect net.unix_poll(fd: Int, timeout_ms: Int) -> Bool`

Whether `fd` has something to read (data, a connection, or the peer's close)
within `timeout_ms` — `0` checks without waiting, a negative timeout waits as long
as it takes. This is what an event loop needs to pump without blocking.

### `effect net.unix_close(fd: Int) -> Unit`

Close a socket, a listener, a shared-memory file or a received descriptor.

## Shared-memory files

An unnamed file to fill and hand to a peer, which maps it: the pixels of a
Wayland buffer, an audio ring.

### `effect net.shm_create(size: Int) -> Int` / `effect net.shm_resize(fd: Int, size: Int) -> Unit`

Create one of `size` zero bytes, and change its size.

### `effect net.shm_write(fd: Int, offset: Int, data: Bytes) -> Unit` / `effect net.shm_read(fd: Int, offset: Int, len: Int) -> Bytes`

Write or read at a byte offset. A write is visible at once to every process that
holds the file, mapped or not.

```almd
effect fn share() -> Unit = {
  let l = net.unix_listen("/tmp/demo.sock")
  let a = net.unix_connect("/tmp/demo.sock")
  let b = net.unix_accept(l)
  let file = net.shm_create(4096)
  net.shm_write(file, 0, bytes.from_string("pixels"))
  net.unix_send(a, bytes.from_string("take this"), [file])
  let _ = net.unix_recv(b, 64)
  let got = net.unix_take_fds(b)
  println(bytes.to_string_lossy(net.shm_read(got[0], 0, 6)))  // pixels
}
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (24 functions)

```
// Connected stream handle; err if refused.
// @since 0.20.0 or earlier
effect net.tcp_connect(host: String, port: Int) -> Int

// Up to len bytes; empty once the peer closed.
// @since 0.20.0 or earlier
effect net.tcp_read(stream: Int, len: Int) -> Bytes

// Sends all of data, flushed; err if closed.
// @since 0.20.0 or earlier
effect net.tcp_write(stream: Int, data: Bytes) -> Unit

// Exactly len bytes; err on early EOF.
// @since 0.20.0 or earlier
effect net.tcp_read_exact(stream: Int, len: Int) -> Bytes

// Shuts down the stream; twice is ok.
// @since 0.20.0 or earlier
effect net.tcp_close(stream: Int) -> Unit

// False after tcp_close; a peer close is not seen.
// @since 0.20.0 or earlier
net.tcp_is_open(stream: Int) -> Bool

// Up to len bytes; err on timeout or 0 ms.
// @since 0.20.0 or earlier
effect net.tcp_read_timeout(stream: Int, len: Int, timeout_ms: Int) -> Bytes

// Sets read/write timeout; <= 0 clears it.
// @since 0.20.0 or earlier
effect net.tcp_set_timeout(stream: Int, timeout_ms: Int) -> Unit

// Bytes readable now, capped at 65536; 0 if none.
// @since 0.20.0 or earlier
effect net.tcp_available(stream: Int) -> Int

// Listener bound to host:port; err if in use.
// @since 0.20.0 or earlier
effect net.tcp_listen(host: String, port: Int) -> Int

// Blocks for the next client; its stream handle.
// @since 0.20.0 or earlier
effect net.tcp_accept(listener: Int) -> Int

// Stops listening; closing twice is ok.
// @since 0.20.0 or earlier
effect net.tcp_close_listener(listener: Int) -> Unit

// Stream socket connected to the listener at path; its descriptor.
// @since unreleased
effect net.unix_connect(path: String) -> Int

// Listener bound at path, which must not exist yet.
// @since unreleased
effect net.unix_listen(path: String) -> Int

// Blocks for the next connection; its descriptor.
// @since unreleased
effect net.unix_accept(listener: Int) -> Int

// Sends all of data, the descriptors fds riding with its first byte.
// @since unreleased
effect net.unix_send(sock: Int, data: Bytes, fds: List[Int]) -> Unit

// Up to max bytes, blocking; empty once the peer closed.
// @since unreleased
effect net.unix_recv(sock: Int, max: Int) -> Bytes

// Descriptors received on sock and not yet taken, oldest first.
// @since unreleased
effect net.unix_take_fds(sock: Int) -> List[Int]

// True if fd is readable within timeout_ms; < 0 waits forever.
// @since unreleased
effect net.unix_poll(fd: Int, timeout_ms: Int) -> Bool

// Closes a socket, listener or received descriptor.
// @since unreleased
effect net.unix_close(fd: Int) -> Unit

// New zero-filled unnamed file of size bytes; its descriptor.
// @since unreleased
effect net.shm_create(size: Int) -> Int

// Grows or shrinks the file fd to size bytes.
// @since unreleased
effect net.shm_resize(fd: Int, size: Int) -> Unit

// Writes data into fd at byte offset.
// @since unreleased
effect net.shm_write(fd: Int, offset: Int, data: Bytes) -> Unit

// Up to len bytes of fd from offset; fewer at its end.
// @since unreleased
effect net.shm_read(fd: Int, offset: Int, len: Int) -> Bytes
```

## Type index (2 types)

```
// Opaque Int handle to a TCP connection.
// @since 0.20.0 or earlier
type net.TcpStream = Int

// Opaque Int handle to a listening socket.
// @since 0.20.0 or earlier
type net.TcpListener = Int
```

<!-- END GENERATED SIGNATURE INDEX -->
