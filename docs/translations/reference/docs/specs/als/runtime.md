<a id="als--実行時規範runtime"></a>

# ALS — Runtime Requirements

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](runtime.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](runtime.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](runtime.zh-CN.md)

> Non-normative reader snapshot of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/runtime.md). Normative status and requirements below belong to the original, not this copy. No automatic synchronization; historical implementation, conformance and safety claims have not been independently revalidated for this translation. Code and rule identifiers are preserved.

> Last updated: 2026-10-26

Observable requirements for program execution (error termination, string-interpolation display forms and concurrent combinators). References follow the same method as the beginning of [strings.md](../../../../../specs/als/strings.md).

<a id="als-r1-effect-main-のエラー終了形"></a>

## ALS-R1 Error termination of effect-main

When `effect fn main` ends with Err, it writes one line `Error: <メッセージ>` to stderr (メッセージ means message) and terminates with **exit code 1**. Ok termination exits 0. Panic-class failures, including the abort in ALS-T6, are also unified under the same `Error:` prefix and exit 1.
Contracts: C-035.

> **Scope clarification (non-normative)**: do not extend the above into a termination convention for every arbitrary process failure. See C-219 for explicit `panic`, ALS-E30 for holes and similar cases, and ALS-T30 for distinguishing a child's raw panic / trap. Each records its own heading and exit status.

<a id="als-r2-補間の表示形"></a>

## ALS-R2 Interpolation display forms

The display form of string interpolation `"${v}"` is specified by type:

- **Containers** (List/Map/Set/tuple/Option/Result): Almide literal forms (`[1, 2]`, `("a", 1)`, `some(3)`, etc.). Nesting recursively follows the same form.
- **Records/variants**: `TypeName { field: v, … }`, with fields in **declaration order**. Anonymous records use lexicographic field-name order. Recursive and generic ADTs follow the same rule for each instantiation.
- **Bare Float**: Display drops `.0` from integral values (`3`), whereas `float.to_string` retains it (`3.0`). This distinction between the two forms is normative.
Contracts: C-008, C-009, C-010, C-011.

<a id="als-r3-fan-並行コンビネータの決定性"></a>

## ALS-R3 Determinism of fan concurrent combinators

**Accepted forms** (syntax-element direction: `ExprKind::Fan` / `FanBounded` / `FanRace` / `FanRaceMap` / `FanSettle` / `FanTimeout`): list form `fan.map(xs, (x) =>
…)`, block forms `fan.settle { … }` / `fan.min(clock) { … }`. Every form **requires an effect fn context**. The mapper of `fan.map` **must return Result** (bare values are rejected at check time, with guidance toward `(x) => ok(…)`; any/settle thunks auto-wrap, but map mappers do not). `fan.settle { a; b }` returns a **tuple of Results**, with as many elements as thunks (ALS-DT4).

The results of `fan.map` and `fan.settle` are **deterministic in list order**: the first applicable result from evaluation starting at the head of the argument list, not the first to finish. Errors surface in the unified abort form of ALS-R1.

The observations of `fan.map` and `fan.settle` (stdout, stderr, exit code and return value) must equal those of **sequential evaluation of every element, one at a time in list order**. The execution mechanism (sequential, threads or asynchronous subtasks) must not change these observations (C-004). Even after an element returns Err, `fan.map` **evaluates all remaining elements**, and if there are multiple Errs, returns the **Err with the smallest index**: the same rule as block form `fan { }` (C-005, C-199, `spec/wasm_cross/fan_map_err_runs_every_element.almd`). A trap in element k is observed after elements 0..k-1 have finished and their output has appeared in list order; abort follows element k's output up to the trap, and no output from elements after k appears (C-200, `spec/wasm_cross/fan_trap_waits_for_elements_below.almd`).

The old names `fan.race` and `fan.timeout` were removed in 0.42.0 / 0.29.0, then **returned in 0.47.0 with deterministic semantics**: race selects the lexicographic minimum of (spend, index) (ALS-DT3, C-205, including mapper form `fan.min(clock, xs, f)`); timeout is a wall-clock deadline cooperatively checked at charge sites (ALS-DT5, C-208). `fan.bounded(c) { … }` is a deterministic budget (ALS-DT2, C-204), without a mapper form. This return SUPERSEDED the old tombstone decisions (C-004/C-006). The current spelling for minimum spend is `fan.min` (dialect epoch 14). `fan.race` was its name at the return described above, not the current call name.
Contracts: C-004, C-005, C-006.

<a id="als-r4-非有限浮動小数の定数表示"></a>

## ALS-R4 Constant display of nonfinite floating-point values

Nonfinite values produced by constant folding (inf / -inf / NaN) are displayed as named constants (`inf`, `-inf`, `NaN`). Bit patterns and forms such as `1e999` are nonconforming.
Contracts: C-012.

<a id="als-r5-プロセス環境"></a>

## ALS-R5 Process environment

`env.args` returns arguments **excluding** argv[0] (the program name), identically on both targets (measured on 0.59.1: two arguments give length 2). `env.get(name)` observes the host process's environment variable and returns some(value) if present or none if absent, with identical bytes on both targets (wasm uses WASI environ + runner environment inheritance). `random.int(a, b)` always remains in [a, b], including under WASI entropy.

`env.secret(name)` returns the same value as `env.get(name)`, wrapped in `Secret[String]`: some(secret.new(v)) exactly when `env.get(name)` is some(v), and none when it is none. The value extracted with `secret.reveal` is byte-identical to `env.get`, including empty values. Every rendering displays the value as `Secret(***)` (C-425); `==` is a constant-time comparison of the extracted strings (C-426). The two targets agree.
Test: `spec/wasm_cross/env_secret.almd`. Contracts: C-438.

`env.os()` and `fs.temp_dir()` are the **only exceptions** to the equivalence rule. Because both report the running host, native returns the actual OS name and temp directory, while the wasm leg returns the WASI sandbox (`wasi` / `/tmp`). Making them identical on the two targets would itself be a defect: a program asking “which platform am I on?” must receive the truth. The exception is not unbounded: both legs are subject to proving the same deterministic invariants (os belongs to the closed set {macos, linux, windows, wasi}; temp_dir is nonempty and absolute on posix hosts). There are currently two exceptions (env.os and fs.temp_dir, C-189). Adding a third function requires revising C-189's statement and fixture.

Launch failure of `process.output` and `process.start` is an err containing the operation and command names. The prefix is `process.output(<quoted cmd>): ` (the same form for `process.start`), or `process.output(<quoted dir>, <quoted cmd>): ` when a working directory `dir` is specified, followed by the host's error description. The command name is double-quoted, escaping embedded quotes, backslashes and control characters. The argument list is not included. A nonexistent executable is a launch failure, not a timeout. When a deadline fires, the err remains `exec timed out after <ms>ms`.
Test: `spec/stdlib/process_timeout_test.almd`

On the wasm target, the embedded host (`almide run --target wasm` and the wasm leg of `almide test`) provides the subprocess family (`process.exec`, `output`, `run`, `kill`, `pid`), children from `process.start`, and their `poll`, `wait`, `stop`, `read_new_stdout`, `read_new_stderr` and `child_pid`, with the same observations as native. Captured stdout and stderr, strings returned by each incremental read, exit codes (-1 for a signal-terminated child), deadline errs and other err strings are byte-identical. Pids are host values and are not compared across legs. A standalone artifact (`almide build --target wasm`) imports `call` from the private interface `almide:process/spawn` only if the program contains subprocess operations. An artifact without these operations has no such import. A runtime that does not define this import rejects the module at load time, before executing `_start`, so the subprocess call cannot return a wrong answer at runtime. Building such a program as a component (`--component`) is rejected with E081.
Tests: `spec/embedded_cross/process_spawn_family.almd`,
`spec/embedded_cross/process_start_family.almd`

`http.start(http.new_request(method, url, body, headers), limits)` starts a request and immediately returns a call handle (`HttpCall`). Limits are passed per call as `limits = { total_ms,
idle_ms }` (milliseconds; 0 means unlimited). `total_ms` is wall-clock time measured from `start`, including connection, waiting for the first byte, and the entire body. `idle_ms` limits the interval between arriving bytes, including the wait for the first byte. A fired limit terminates the call with an err naming itself: `request timeout: total_ms <n> exceeded` or `request timeout: idle_ms <n> exceeded`. `ALMIDE_HTTP_TIMEOUT_SECS` is the default for calls that take no limits and does not affect calls with limits. `poll` and `read_new` do not wait; `wait` waits for completion. `cancel`, and dropping the last copy of a handle, terminate an ongoing call with err `request cancelled` and close the connection. Subsequent `read_new` returns the empty string, and the server observes the closed connection. Whether a limit fires is a host property (the same discipline as C-214); what is fixed is the err's form when it fires. On the wasm target, the embedded host (`almide run --target wasm`) provides `start`, `poll`, `read_new`, `wait` and `cancel` with the same observations as native, including err strings, connection closure on `cancel`, and cancellation when the last handle copy is dropped. A standalone artifact (`almide build --target wasm`) cannot have this family, and `almide check --target wasm` rejects it with E081. OpenAI and Anthropic streaming clients moved to package almide/llm in dialect epoch 14 (C-270).
Tests: `spec/stdlib/http_call_test.almd`, `spec/embedded_cross/http_call_handle_errs.almd`

`http.serve(port, handler)` also runs with native semantics in the embedded wasm lane (`almide run --target wasm`). One instance processes all requests of a run, one at a time in acceptance order. main runs once and calls `http.serve`, as on native. The host binds `0.0.0.0:<port>` and passes parsed requests to the guest one at a time. Effects in main before `serve` happen once. The app passed to `serve` is closed over its instance: it does not read main's local variables (almide/almide#2698). Thus no requirements are placed on values computed by main before `serve`, and the number of instances evaluating the app is unobservable. The same code reads requests in both legs: the method and target of the request line, header lines split at the first colon and trimmed (in arrival order), and the Content-Length body (UTF-8, replacing invalid bytes with the replacement character).
Both legs answer every request with the same status code, header collection and body. The header collection consists of response-header (name, value) pairs; names are compared ASCII-case-insensitively. Order does not matter between differently named fields; fields with the same name retain their relative order (RFC 9110 §5.3). Host-managed fields `date`, `connection`, `keep-alive`, `transfer-encoding` and `content-length` are excluded (these are framing; bodies are compared after removing framing). Reason phrases are not normative. HTTP/2 and HTTP/3 have no reason phrases (RFC 9113 §8.3.2, RFC 9114 §4.3.2), and host HTTP libraries write their own (the native core writes `418 OK`, hyper writes `418 I'm a teapot`, measured in the almide/almide#2659 prototype).
`method`, `path`, `body`, `header` (first match, ASCII-case-insensitive), and `query_params` return the same values. For query_params, text after the first `?` is split on `&`, each pair is split at the first `=`, pairs without `=` are discarded, `+` and `%XX` are decoded, and later keys win. A handler's `err(m)` becomes `500` with body `Internal error: <m>` and `Content-Type: text/plain`. In an export host (`wasmtime serve` running a C-375 artifact), the host's own `500` answers handler traps or aborts, and the instance is discarded.
On socket hosts (native and the embedded lane), bind failure aborts execution regardless of call position, writes `Error: bind failed: <os message>` to stderr, and exits 1. Since `http.serve` has a type that does not return err, a caller's `!` does nothing. Before this requirement, the native runtime's err surfaced only when the call was at the end of a function; in other positions, the program continued without a server.
On socket hosts, the lane retains native streaming rules while the server runs. stderr is unbuffered, so every line written to stderr reaches the stream on both legs, and the two records contain the same lines. Order across requests is not normative; host log decoration such as `wasmtime serve`'s `stdout [req_id] :: ` prefix is excluded. stdout flushes on every write if it is a terminal, otherwise on each write ending a line (C-162).
On socket hosts, signals stop the server without losing its output (almide/almide#2692). When the first SIGTERM or SIGINT (Ctrl-C or Ctrl-Break on Windows) arrives while `http.serve` is running, the host stops accepting (closing connections not yet accepted without answering), finishes and answers the in-flight request, flushes stdout and returns from `http.serve`. Subsequent statements therefore run, and the exit code is main's. A second signal, whether during cleanup or after `serve` returns, or cleanup still waiting beyond the request timeout (30 seconds), flushes stdout and exits 1 without answering the in-flight request. The serve shutdown described here does not terminate with 128+signal number. See ALS-R9 / C-350 for general `process.exit` range and preview-1 restrictions.
Out of scope: HTTP framing and connection reuse (host decisions), and other signals. On native Windows, forced shutdown exits 1 without flushing stdout: only the serving thread touches the stdout buffer, until stdout becomes a single process-wide buffer (almide/almide ADR-0020 §5.5).
The standard artifact (`almide build --target wasm`) has no listening socket. Serve-shaped programs become components exporting `wasi:http/handler@0.3.0`, described in the following paragraph (C-375). Other programs reaching `http.serve` are rejected for standard artifacts at both check and build time (E081, both almide/almide#2659).
Tests: `spec/serve_cross/http_serve_replay.almd` (a nonterminating server fixture, not run by the generic runner; an implementation driver starts both legs, replays the same request sequence, compares status codes, header collections and unframed bodies, and stderr as multisets of lines, and starts on an occupied port to compare aborts); `spec/serve_cross/http_serve_shutdown.almd` (another nonterminating server fixture started by the same driver on both legs with stdout redirected to files. One SIGTERM during a request whose handler sleeps causes that request to be answered, every line through the line main writes after `http.serve` to remain in the file, and exit 0. Two SIGTERMs during a stalled request preserve all lines written before shutdown and exit 1).

Building a serve-shaped program with `almide build --target wasm` — the body of `main` is exactly one `http.serve(port, app)` call, preceded only by `let` bindings read exclusively by `port` — produces a component exporting `wasi:http/handler@0.3.0`; `wasmtime serve` loads and serves it without flags. The component imports only interfaces of the service world, not `wasi:filesystem`. `main` does not run and `port` is not evaluated: the host decides the address. The app and top-level `let` bindings are evaluated per instance. Only one handler runs at a time within an instance (backpressure); the host handles concurrent requests by adding instances. The guest applies native limits: a request body over 1 MiB receives `413`, a request line over 8 KiB receives `414`, and the handler is not called. Handler traps or aborts receive the host's `500`, and the host discards the instance. Other accepted requests receive the same status code, header collection and unframed body as native. Comparison uses C-367's HTTP semantics; reason phrases, `date` and framing belong to the host and are not compared as bytes.
Out of scope: host address, acceptance, concurrency, instance reuse, timeouts, shutdown and log decoration (all host configuration).
Test: `spec/serve_cross/http_serve_export.almd` (a nonterminating server fixture, not run by the generic runner. An implementation driver builds it for standard wasm, serves it with `wasmtime serve`, also runs native, replays the same request sequence and compares status codes, header collections and unframed bodies; it sends the artifact an oversized body and an overlong request line, then finally requests `/trap`).

Contracts: C-096, C-112, C-118, C-133, C-189, C-214, C-366, C-367, C-375.

<a id="als-r6-ファイルシステムのパス解決"></a>

## ALS-R6 Filesystem path resolution

At startup, the wasm fs runtime builds a WASI preopen directory table and resolves an absolute path to the longest matching preopen plus the relative remainder, including `./` normalization. Writing then reading the same path reaches the same host file as native std::fs, independently of CWD. Open-error wording follows the native-wording requirements of the ALS-T6 family.

`fs.list_dir(path)` returns **all** directory entries. There is no entry-count limit; host buffer boundaries do not appear in observations (wasm `fd_readdir` is resumable and each pass returns only what fits in the caller's buffer). Both targets exclude `.` and `..`, and normalize order to **byte lexicographic order** (native uses `names.sort()`, wasm uses insertion sort with the same byte comparison). Filesystem readdir order is unobservable. Read failure returns err, not a shortened list.
Test: `spec/wasm_cross/fs_list_dir_multipass.almd`
Contracts: C-042, C-272.

<a id="als-r7-ストリーミング行走査の可謬コールバック"></a>

## ALS-R7 Fallible callbacks in streaming line traversal

ADR-0006's one-bit fallibility polymorphism also applies to streaming line traversal in fs. When a callback body of `fs.fold_lines` / `fs.for_each_line` uses `!`, the checker rewrites the callee to internal carrier `fs.__fallible_fold_lines` / `fs.__fallible_for_each_line`, and the entire call becomes fallible (analogous to `list.map` → `list.__fallible_map`). Carriers are not source spellings: naming one directly in source produces E043.

The normative observation is the **sequence of callback calls**. Callbacks are invoked through the first line that returns err, and **never** for subsequent lines (stop at first err). The callback's err message enters the caller's err channel unchanged. Both properties are byte-identical between native and wasm.

The native leg additionally stops **reading itself** at the failing line (`almide_rt_fs_fold_lines_effect` returns from its BufReader loop). The self-hosted wasm implementation first reads the entire file, as in C-220, so “where the reader stopped” is not a wasm observation. Like RSS, it is native-only and outside the observable promise.

Partitioned traversal (`fold_lines_range` / `fold_lines_chunked`) has **no fallible form**. “The first err” in partitioned traversal would observe thread execution order, leaving no definable cutoff point. The matrix in `tests/fs_streaming_family_gate_test.rs` fixes this deliberate omission.

Tests: `spec/wasm_cross/fs_fallible_stream_callback.almd` (both legs),
`spec/stdlib/fs_streaming_test.almd` (for_each_line cell, native only),
`tests/fs_streaming_family_gate_test.rs` (matrix gate).
Contracts: C-274.

<a id="als-r8-http-レスポンスヘッダの規範"></a>

## ALS-R8 HTTP response-header requirements

The `http` response-construction family (`response` / `json` / `redirect` / `with_headers` / `set_status` / `body` / `set_header` / `header`) consists of **pure data operations that do not touch the network** and runs on both targets. There are three header-name requirements:

- **Field names are case-insensitive** (RFC 9110 §5.1). Folding is **ASCII-only** (field names are `token`; match v0's `eq_ignore_ascii_case`, not Unicode `string.to_lower`). `header` returns the first match, or none if absent.
- **At most one entry per field name.** Writes (`set_header`, `with_headers`) case-insensitively **overwrite the existing entry's value in place**, retaining the originally stored spelling and position, or append if absent. Thus `header(set_header(r, k, v), k') == some(v)` always holds when `k` and `k'` differ only in ASCII case.
- **`with_headers` constructs the passed list using the above overwrite rule** (C-275). Without duplicate names it keeps list order; for duplicate names it puts the last value at the first position and spelling.

  It does **not seed** `Content-Type` on its own. Only `response` (`text/plain`) and `json` (`application/json`) have default Content-Types, since they have no other way to name one. `redirect` has only `Location`.

Tests: `spec/wasm_cross/http_response_headers.almd`,
`spec/stdlib/http_response_test.almd`.

A handler accepts `HttpRequest` and returns `HttpResponse` (`HttpHandler = effect (HttpRequest) -> HttpResponse`); an app composed with a router and middleware is also a handler. `http.new_request(method,
target, body, headers)` (with `headers` of type `List[(String, String)]`) creates a request without a socket. The read family (`method` / `path` / `body` / `header` / `query_params` / `param`) returns the same values for it on both targets. `path` returns target including its query string. Routing gives the same result on both targets:

- A route is `"METHOD /path"` (omitting the method means all methods). `{name}` binds one segment, and a final `{name...}` binds the remaining path, possibly empty. Bound values are percent-decoded, leaving `+` as `+`. Matching uses the path without the query string and does not count empty segments.
- The **most specific** matching route responds, independently of registration order. A is at least as specific as B when every path matching A also matches B; a method-specific route is more specific than a methodless route with the same path.
- `http.router(routes)` rejects, with an `err` naming every problem, a table containing route pairs that can match the same request with neither more specific than the other (including duplicate patterns), nonfinal `{name...}`, doubly bound names, methods other than uppercase alphabetic letters, or paths not starting with `/`. A rejected table does not respond.
- If no route matches the path, return `404 Not Found`. If matching paths exist only for other methods, return `405 Method Not Allowed` with `Allow` (methods sorted and joined with `, `, including HEAD if GET is present). HEAD falls back to a GET route and returns an empty body. A request whose target does not start with `/`, or whose path contains `%` not followed by two hexadecimal digits, receives `400 Bad Request`.
- `http.mount(prefix, sub)` passes the path after prefix and the query to `sub`; names bound by prefix are also readable from `sub`. `http.wrap(h, [a, b])` is `a(b(h))`: the first list element is outermost.
- `http.decode_json[T](req)` reads the body as JSON and converts it with `T.decode_strict` to input type `T` (a record or variant declaring only `: Decode`, C-435). A failing `err` is a `400 Bad Request` response with the reason in its body. Keys outside `T` at any depth, including inside nested input records and variant payloads, produce `400 Bad Request: unknown key '<k>'` with the record path (`, at address`). A `T` that is also `Encode` (`Codec`), or a type containing a `Codec` record as a field or variant payload, receives E110 at check time (C-436). A handler's `err` remains an `err` returned to its caller.

Test: `spec/stdlib/http_router_test.almd`.
Contracts: C-275, C-368, C-436.

<a id="als-r9-プロセス終了コードの値域"></a>

## ALS-R9 Process exit-code range

The code accepted by `process.exit(code)` is the POSIX exit-status range **0..=255**. Native and the embedded host (`almide run --target wasm`) terminate with **the unchanged value** for this range. The purpose is to allow wrappers to pass through a child's 126, 127 or 128+n (#2780). Rust `std::process::exit`, Go `os.Exit`, Python `sys.exit` and Node `process.exit` also pass these values.

A computed argument outside 0..=255 produces a defined domain error: one stderr line `Error: exit code must be in 0..=255`, then **exit 1** (identical bytes on all targets). The languages listed above leave the low 8 bits to the OS, so `exit(256)` reports success (0), and `exit(-1)` reports 255. Because the caller cannot detect this truncation, Almide differs from them here.

| Layer | Values carried | Out-of-range behavior |
|---|---|---|
| POSIX `exit(status)` | Low 8 bits only | Parent observes `256` as `0` (silent truncation) |
| WASI preview-1 `proc_exit` (stock runtime) | `[0, 126)` | Host trap. wasmtime 47 emits `exit with invalid exit status outside of [0..126)` and exits 1; the information that this was an exit request is lost |
| Component model `wasi:cli/exit#exit-with-code(u8)` | 0..255 | Does not trap |

**WASI preview-1 builds cannot deliver 126..=255.** Running a preview-1 artifact emitted by `almide build --target
wasm` on a stock runtime traps on `proc_exit` with those values (#2303). The build therefore rejects that band itself: one stderr line `Error: a WASI preview-1 build cannot exit with a code in
126..=255`, then **exit 1**. This is a diagnosed wall of preview-1 lowering, like `env.cwd` on wasm, not a silent 1 or an unexplained host trap. 0..=125 exits unchanged on every leg.

This wall belongs to preview-1 lowering, not wasm itself. If an artifact carrying a full byte as the exit code (component model `exit-with-code(u8)`) becomes the default, the wall can be removed; removal is a backward-compatible expansion.

When a directly called `process.exit` has an out-of-range integer literal argument, the checker rejects it with **E084** pointing to the argument. This includes parentheses, unary minus and literals in every radix; the same rule applies through module aliases, selective imports and pipes. `-0` is 0; double unary minus is judged as a positive value.

```almide check-fail=E084
import process
effect fn main() -> Unit = process.exit(256)
```

Computed arguments such as variables and arithmetic expressions are not rejected by this literal diagnostic. Nor does it track calls through variables holding function values. Runtime range checking continues to apply. Same-named user functions and local function values are not the stdlib `process.exit` and are outside this diagnostic's scope.

Tests: `spec/wasm_cross/exit_code_out_of_range.almd`,
`spec/wasm_cross/exit_code_upper_bound.almd`,
`spec/wasm_cross/exit_code_passthrough.almd`,
`spec/embedded_cross/exit_status_passthrough.almd`,
`tests/diagnostics/e084-exit-code-domain/broken.almd`,
`tests/diagnostics/e084-exit-code-domain/fixed.almd`,
`tests/exit_literal_check_test.rs`.
Contracts: C-350, C-351.

> Source context: the dated text retains historical fan forms and describes distinct embedded-host, standalone preview-1 and service-component paths. “Only exceptions” in ALS-R5 is retained alongside its separate host-dependent PID, timeout and HTTP exclusions; it must not be generalized into identical behavior on every host. ALS-R3's general error wording does not erase the separately stated Result-tuple behavior of settle. ALS-R8's default Content-Type statements are preserved, not reconciled here with later typed-response rules. None of these statements constitutes an independently verified current runtime or security guarantee.
