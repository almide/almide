<!-- description: Interim embedded host ops 104..=116 for net's Unix-domain sockets and shared-memory files, and their retirement at ADR-0023 step 7 -->
# net unix sockets and shm on the embedded wasm lane (interim ops 104..=116)

Issue [#3620](https://github.com/almide/almide/issues/3620), route 4 ("unix / fd passing / shm").
A sibling of the net/tcp design (TCP steps 1 and 2: shared `net_core`, ops 91..=103), and
built in the same shape. Owner ruling 2026-10-17: ○.

## Decision

The 13 functions `net.unix_connect`, `unix_listen`, `unix_accept`, `unix_send`, `unix_recv`,
`unix_take_fds`, `unix_poll`, `unix_wait`, `unix_close`, `shm_create`, `shm_resize`,
`shm_write` and `shm_read` get **interim embedded host ops**, exactly as TCP did:

| leg | before | after |
|---|---|---|
| native | `runtime/rs/src/net.rs`, raw descriptors | the shared core, one table per thread |
| embedded (`almide run --target wasm`) | E081 | ops 104..=116 over the same core, one table per run |
| structural (`ALMIDE_WASM_SKIP_STOCK_AUDIT`) | wall | lowers the `stdlib/net_wasm.almd` bodies |
| stock-p1 (`almide build --target wasm`) | E081 | E081, unchanged |

The stock artifact stays E081 because **WASI has neither Unix-domain sockets nor shared memory**:
no preview (1, 0.2, 0.3) defines them, so no stock runtime can serve these calls.
`proofs/target-availability.toml` keeps each row's `stock-p1` leg only, with that reason.

## One implementation

`crates/almide-rt-core/src/net_unix_core.rs` holds the whole family. Native `include!`s it
(`runtime/rs/src/net.rs`, thin `almide_rt_net_unix_*` / `almide_rt_net_shm_*` wrappers), and
`almide-wasm-run` links it (`net_unix_host.rs`). Bytes, descriptors received and every err text
therefore agree by construction; `tests/net_unix_embedded_cross_test.rs` observes it.

**`unsafe`.** Four syscalls have no safe std form on stable Rust: `sendmsg` / `recvmsg` with
SCM_RIGHTS, `poll`, and `fcntl(F_SETFD, FD_CLOEXEC)`. The workspace forbids `unsafe`, and
`almide-wasm-run` takes that lint, so the embedded host could not carry a copy. Two copies would
also be two behaviours. Instead:

- `almide-rt-core` has its own lints: `unsafe_code = "deny"` (not `forbid`).
- Exactly one module, `src/net_unix_sys.rs`, is `#[allow(unsafe_code)]`. Every block carries a
  SAFETY line, and every raw descriptor it receives is borrowed from an `OwnedFd` the table holds
  for the whole call.
- `crates/almide-rt-core/tests/net_unix_core.rs` fails if `unsafe` appears in any other file of
  the crate, or a block in that module lacks its SAFETY line.

The rest of the core (connect, listen, accept, the file reads and writes, close) is safe std
code. Adding a crate such as `rustix` would have removed the `unsafe`, but it would also have
added a crates.io dependency to every generated native project that uses `net`, which then takes
the cargo path instead of the prebuilt runtime.

## Descriptors

The guest receives **raw OS descriptor numbers**, as native returns them. The numbers differ
between a native process and the embedded host's process, so fixtures print descriptor
properties, never the numbers.

The core keeps a table of the descriptors the program holds: the ones it opened (`unix_listen`,
`unix_connect`, `unix_accept`, `shm_create`) and the ones that arrived on a socket
(`unix_recv`), until `unix_close`. A call naming any other number is refused before any system
call:

- negative or above `i32::MAX`: `<fn>: not a file descriptor: <n>` (native's text before this
  change);
- otherwise: `<fn>: descriptor <n> is not open in this program`.

On the embedded host this is what keeps a guest from reaching, or closing, the host's own
descriptors (wasmtime's, the host's stdio). Native answers the same, so this is a **native
behaviour change**: before it, `net.unix_close(0)` closed the process's stdin and
`net.unix_poll(2, 0)` polled stderr. Closing a socket now also closes the descriptors it
received that were never taken (native leaked them).

## Operand encoding (ops 104..=116)

Two operands per op, as for TCP: `a` is text, `b` is text or raw bytes. Answers are ok / err.

| op | function | a | b | ok answer |
|---|---|---|---|---|
| 104 | `unix_connect` | path | — | descriptor |
| 105 | `unix_listen` | path | — | descriptor |
| 106 | `unix_accept` | listener | — | descriptor |
| 107 | `unix_send` | socket, then each descriptor to send, one per line | raw bytes | — |
| 108 | `unix_recv` | socket | max | raw bytes |
| 109 | `unix_take_fds` | socket | — | descriptors, one per line |
| 110 | `unix_poll` | descriptor | ms | `1` / `0` |
| 111 | `unix_wait` | descriptors, one per line | ms | ready descriptors, one per line |
| 112 | `unix_close` | descriptor | — | — |
| 113 | `shm_create` | size | — | descriptor |
| 114 | `shm_resize` | descriptor | size | — |
| 115 | `shm_write` | `<fd>\n<offset>` | raw bytes | — |
| 116 | `shm_read` | descriptor | `<offset>\n<len>` | raw bytes |

The numbers follow the tcp family (91..=103). No other table uses 104..=116: the p1 op set
(`almide-wasi` `P1_SERVED_OPS`, `FS_SERVICE_OPS`), http 43..=65, serve 70..=72, 73 / 74,
process 80..=90 and `component_availability::operation_name` were checked.

## Retirement commitment

**Ops 104..=116 retire when ADR-0023 step 7 runs the embedded lane as the p3 component.** The
same step retires the tcp family's ops 91..=103, which `wasi:sockets` then serves. WASI has no
counterpart for this family, so at step 7 it is served through Almide's own interface in the
component world (#3620's plan), or it is unavailable on wasm until that interface exists. In
neither case do these core-module ops survive step 7. The commitment is also written at the op
tables: `crates/almide-wasm-run/src/net_unix_host.rs` and `crates/almide-wasm/src/net_call.rs`.

## Evidence

- `tests/net_unix_embedded_cross_test.rs`: `tests/net_unix/net_unix_loopback.almd` (every
  function on its success path, descriptors passed both ways) and `net_unix_errors.almd` (every
  function's error paths) print the same lines on native and on `almide run --target wasm`, and
  each reaches its closing `done` line on both legs. A third test pins the error texts on native.
- `crates/almide-rt-core/tests/net_unix_core.rs`: the table's refusals, the close of untaken
  descriptors, and the `unsafe` sanction.
- `tests/net_unix_test.rs` (native, unchanged) still passes.

## Contracts

No contract covers this family yet. C-377..C-379 are the tcp contracts and name only
`net.tcp_*`. A new contract id lands in the als judge first, so the fixtures stay under
`tests/net_unix/` until then, and move to `spec/embedded_cross/` with their `// @contract:`
headers once the judge holds the ids. Two statements, proposed for the judge:

1. **Agreement.** ALS-R5 (#3620): the `net.unix_*` / `net.shm_*` family (`unix_connect`,
   `unix_listen`, `unix_accept`, `unix_send`, `unix_recv`, `unix_take_fds`, `unix_poll`,
   `unix_wait`, `unix_close`, `shm_create`, `shm_resize`, `shm_write`, `shm_read`) answers
   identically on the native leg and the EMBEDDED wasm lane (`almide run --target wasm`) of a
   Unix host. The bytes received agree. The descriptors received agree in count and send order,
   each a new descriptor of the same open file (a write through one is read through the other).
   Readiness from `unix_poll` / `unix_wait` agrees (data, a pending connection or the peer's
   close; `unix_wait` answers the ready descriptors in the order given). `shm_read` answers the
   same bytes (zero-filled on create and growth, fewer at the file's end). Descriptors are raw OS
   numbers, which may differ between the legs; their properties (non-negative, distinct while
   open) agree. The stock artifact (`almide build --target wasm`) refuses every one at build
   time (E081), because WASI has neither Unix-domain sockets nor shared memory.
2. **Errors.** ALS-R5 (#3620): every failure of a `net.unix_*` / `net.shm_*` call is an `err`,
   never a trap, with the same text on native and the embedded wasm lane. A call acts only on
   descriptors the program holds (returned by `unix_listen`, `unix_connect`, `unix_accept` or
   `shm_create`, or received by `unix_recv`, until `unix_close`) and refuses any other before
   any system call: `<fn>: not a file descriptor: <n>` for n < 0 or n > 2^31-1, otherwise
   `<fn>: descriptor <n> is not open in this program`, a standard stream and a closed
   descriptor included. The argument domains have Almide's own texts, checked after the
   descriptor: `shm_create: negative size <n>`, `shm_resize: negative size <n>`,
   `shm_write: negative offset <n>`, `shm_read: negative offset or length (<o>, <n>)`,
   `unix_send: <k> descriptors in one message (at most 28)`. An OS refusal reads
   `<fn>: <the platform's error text>`, with the path for `unix_connect(<path>)` and
   `unix_listen(<path>)`, and both legs read it from the same host. `unix_take_fds` never
   fails: a socket with nothing received, or one not held, answers the empty list. Closing a
   socket closes the descriptors it received that were never taken.
