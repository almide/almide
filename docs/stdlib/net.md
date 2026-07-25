# net

TCP sockets. `import net`, `effect fn`.

Every call except `tcp_is_open` is an `effect fn` — it needs a network
capability, so it can only be called from an `effect fn` and its `Result` is
auto-propagated with `?`. A stream or listener is an opaque `Int` handle.

**Native only.** The wasm leg has no socket floor, so a program using `net`
builds and runs natively but walls on `--target wasm`. `spec/stdlib/net_test.almd`
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

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (12 functions)

```
effect net.tcp_connect(host: String, port: Int) -> Int
effect net.tcp_read(stream: Int, len: Int) -> Bytes
effect net.tcp_write(stream: Int, data: Bytes) -> Unit
effect net.tcp_read_exact(stream: Int, len: Int) -> Bytes
effect net.tcp_close(stream: Int) -> Unit
net.tcp_is_open(stream: Int) -> Bool
effect net.tcp_read_timeout(stream: Int, len: Int, timeout_ms: Int) -> Bytes
effect net.tcp_set_timeout(stream: Int, timeout_ms: Int) -> Unit
effect net.tcp_available(stream: Int) -> Int
effect net.tcp_listen(host: String, port: Int) -> Int
effect net.tcp_accept(listener: Int) -> Int
effect net.tcp_close_listener(listener: Int) -> Unit
```

<!-- END GENERATED SIGNATURE INDEX -->
