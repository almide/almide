# ADR-0027: Effect functions are inspected in a fixed host world — a seeded sandbox, property verdicts for what the host decides, and loopback fixtures for the network

- **Status**: Accepted (D1–D8, 2026-10-17).
- **Date**: 2026-10-17
- **Scope**: how `xtarget-fuzz inspect` applies the five inspection criteria (parity, complexity, alloc, fuzz, diamond) to the stdlib functions whose results depend on the host: fs, env, args, io, process, random, datetime, http and net, the host-address functions in `bytes` and `mem`, and `list.shuffle`.
- **Builds on**: [effect-inspection-harness.md](../roadmap/active/effect-inspection-harness.md) (the 2026-10-12 design, tiers E1–E4). This ADR turns it into decisions for the inspection harness, and fills the gaps that design left open: the legs, how a non-deterministic value is compared, the host-address functions, and how each criterion reads for an effect row.
- **Related**: #3552 (the inspection campaign), #1710 (wasi:http@0.3), #1827 (wasm-unserved functions), [ADR-0023](./0023-wasm-http-over-wasi-http.md), [ADR-0025](./0025-wasm-process-is-a-private-host-capability.md).

## Context

After the 2026-10-16 wave, `proofs/stdlib-inspection.toml` has **150** uninspected rows. Of those, 140 depend on the host:

| Module | Rows | What decides the result |
|---|---|---|
| fs | 34 | the file tree |
| http | 28 | a server |
| net | 26 | a peer, a socket, shared memory |
| process | 17 | child processes, the process's own pid, its exit |
| env | 9 | env vars, argv, cwd, the clock, the OS name |
| args | 6 | argv |
| io | 6 | stdin, stdout |
| bytes | 6 | heap addresses |
| random | 4 | the entropy source |
| datetime | 2 | the clock |
| mem | 2 | the heap mark |

`list.shuffle` (a pure `fn` documented "unseeded, differs per run") is a 141st.

Today the inspection harness (`tools/xtarget-fuzz/src/inspect.rs`) builds each probe program and runs it as a bare process: native as `./bin`, wasm as `wasmtime run`. It has no cwd of its own, no argv, env or stdin, and compares stdout byte for byte. So an effect probe either cannot run on wasm at all (no preopen) or prints values that differ per run (a pid, a timestamp).

The program fuzzer already has half of the machinery: `oracle/sandbox.rs` creates a fresh seeded tree with fixed argv, env and stdin, and `oracle/runner.rs` runs every leg in it and compares a snapshot of the tree afterwards (tier E1). The inspection harness does not use it yet.

## Decision

### D1. The legs

An effect row compares native against the **embedded wasm host** (`almide run --target wasm`, `almide-wasm-run`). It also compares against the **stock runtime** (`wasmtime run`) wherever `proofs/target-availability.toml` says stock serves the function.

The row's `parity` field names the legs it covers. A function the embedded host does not serve is made to run on wasm first and then inspected on both legs (D8).

### D2. A fixed world per run (tier E1: fs, env, args, io, process.args / env / stdin_lines)

Every probe run, on every leg and at every size, gets its own fresh sandbox from `oracle/sandbox.rs`:
- the seeded tree, used as the cwd;
- the fixed argv, env and stdin.

The inspection harness reuses the fuzzer's runner rather than a second copy of it.

Parity compares stdout, stderr, the exit code, **and the tree snapshot after the run**. A write that lands differently is a divergence even when nothing is printed.

Recipes name paths in the tree, so the measured call always takes the success path. That is the rule from #3671: a stamp taken on the err or none path is void.

### D3. What the host decides is compared as a verdict, not as a value (tier E2)

A value the host chooses (a random number, a timestamp, a pid, an address) cannot match across legs or runs. Its probe does not print the value. It prints a **verdict line**: the function's declared properties, computed in the probe itself from already-inspected pure functions.

Both legs must print the same verdicts, and every verdict must be `true`. The comparison stays byte equality, now over the verdicts.

| Function | Properties |
|---|---|
| `random.int(lo, hi)` | lo ≤ r ≤ hi over m draws, and both ends are hit when hi − lo is small |
| `random.float()` | 0 ≤ r < 1 over m draws |
| `random.choice(xs)` | the result is an element of xs; `none` only for `[]` |
| `random.shuffle(xs)`, `list.shuffle(xs)` | a permutation of xs (sorted copies equal), and a frequency check: over m shuffles, each element's count at each position is within 6σ of m/n. With m fixed in the recipe, a correct shuffle fails this about once in 10⁹ runs |
| `datetime.now()`, `env.unix_timestamp()`, `env.millis()` | within the window `[t₀, t₁]` that the harness passes in, read just before and just after the run |
| `datetime.monotonic_ns()` | two reads are non-decreasing |
| `env.sleep_ms(k)`, `process.sleep(k)` | the monotonic gap across the call is ≥ k ms |
| `process.pid()` | > 0, and `process.is_alive(process.pid())` |
| `process.is_alive(p)` | true for a running child, false after it is reaped |
| `env.os()` | the value the leg's contract names for the host it runs on |
| `bytes.as_ptr` / `as_mut_ptr` / `data_ptr` / `copy_to_ptr` / `from_raw_ptr` / `heap_restore` | round trip: the bytes copied out through a pointer and read back are equal, and a pointer is stable while its value is live |
| `mem.save` / `mem.restore` | the allocations made between them are released (the live count returns to the mark) |
| `net.tcp_is_open(c)` | true while connected to the fixture (D5), false after `tcp_close` |

The `parity` field of such a row starts `PASS (properties: …)`, so no reader takes it for value equality.

### D4. Processes run a fixed command set (tier E3)

The probes spawn only:
- `/bin/echo`;
- `/bin/cat` fed from stdin;
- `/usr/bin/true`;
- `/bin/sh -c 'exit 3'` for a failing status;
- `/bin/sleep` for the timeout forms.

The harness reaps every child. `process.exit(c)` is judged on the exit code alone.

### D5. The network is a loopback fixture (tier E4)

For each run the harness starts a fixture on 127.0.0.1 and a Unix socket in the sandbox, and stops it after the run. The fixture provides:
- an HTTP server that echoes method, path, headers and body, with canned status codes, a chunked response and an SSE stream;
- a TCP echo server;
- a Unix echo server.

The program reads the address from a fixed env var. The `anthropic_` / `openai_` streaming calls reach the SSE endpoint through their base URL. No external host is contacted and no API key is used.

### D6. How each criterion reads for an effect row

- **Complexity.** The size parameter is the data that flows through the call: file length, line count, payload bytes, list length. A function whose work is a fixed host operation (`exists`, `mkdir_p`, `tcp_close`) is declared order 1. The counted meter (`instructions:u` plus bytes moved) measures the program's own work; kernel time is outside it, and the stamp says so.
- **Alloc.** Unchanged: the call leaves nothing live. The exception is a resource handle it returns, which the probe releases before the count.
- **Fuzz.** The generator draws these functions with arguments from the world: tree paths, the fixture address, `lo..hi` ranges. A generated program that reaches an E2 value prints that value's verdict, not the value.
- **Diamond.** Each path runs in its own fresh sandbox (or fixture), the same on all three paths.

### D7. The table is one place

The tier of each function and the properties in D3 live in one table in `tools/xtarget-fuzz`. A test refuses an effect row of the ledger that the table does not classify. The same rule applies to pure functions, where nothing may be special-cased by name outside the declaration.

### D8. Nothing is stamped native-only

A function no wasm leg serves today is ported first, then inspected on both legs. This covers the http streaming family, `http.*_response`, `net.unix_*`, `net.shm_*`, the quantised matrix kernels, and `testing.assert_throws` inside a test block. The native-only stamp ruled on 2026-10-12 (plan B) is not used for "uninspected = 0" (owner, 2026-10-17: 「未対応は当然wasmでも動くようにした上で両方検品です」).

## Alternatives considered

- **Mock the effects in the stdlib for inspection.** Rejected (as in the 2026-10-12 design): the inspection would judge the mock, not the legs' real host bindings, which are where cross-target bugs live.
- **A seed switch (`ALMIDE_RANDOM_SEED`) so random runs repeat byte for byte.** Rejected. It adds a hidden runtime mode that ordinary programs could come to depend on. Two legs with the same seed would also have to share one PRNG, which constrains the runtime for the sake of the harness. The D3 verdicts check what the API promises (bounds, permutation, uniformity) without changing the runtime.
- **Record and replay host calls.** Rejected: it judges the recording. Kept in reserve for E4 only if the loopback fixture proves flaky.
- **Mark the host-address functions "not inspectable".** Not needed: D3 gives each of the 14 an observable property. If one turns out to have none, it comes back as a separate question.

## Compatibility

- Tool-only: `tools/xtarget-fuzz` and the inspection scripts. No language, stdlib or codegen change.
- Defects the harness finds are fixed test-first, as in every inspection wave.

## Order of work

1. E1 in the inspection harness (sandbox runs, tree snapshot in parity); stamp fs, env, args and io.
2. The E2 verdict probes; stamp random, the clocks, the shuffles, and the address and heap-mark functions.
3. E3; stamp process.
4. E4 fixtures; stamp the http and net functions the embedded host serves.

## Open

1. ~~Adopting D1–D7 as a whole.~~ Accepted 2026-10-17 (○).
2. ~~The 24 rows the embedded host does not serve.~~ Decided 2026-10-17: port them, then inspect both legs (D8).
3. ~~`testing.assert_throws` inside a test block.~~ Decided by the same ruling (D8): make it run on wasm, then inspect both legs.
