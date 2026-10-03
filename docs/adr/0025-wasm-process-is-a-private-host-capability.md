# ADR-0025: Wasm `process` is a private host capability — `almide:process/spawn`, shipped only when the op set names it, refused at load by a stock host, bounded by `[permissions] proc`

- **Status**: Accepted. The owner ruled **A** on 2026-10-01 (#2589): on wasm,
  `process` is available through a private `almide:process/spawn` WIT import.
- **Date**: 2026-10-03
- **Scope**: the subprocess family of `process` on `--target wasm`
  (`exec`, `exec_in`, `exec_with_stdin`, `exec_status`, `exec_status_timeout`,
  `exec_attached`, `spawn`, `kill`, `is_alive`, `pid`); the embedded host
  (`almide run --target wasm`, the wasm leg of `almide test`); the p1 build
  artifact (`almide build --target wasm`); `[permissions] proc` in
  `almide.toml`; `proofs/target-availability.toml`.
- **Exception to**: [ADR-0023](./0023-wasm-http-over-wasi-http.md) §2.6
  ("Nobody who targets stock runtimes ships a private host ABI") and §3.1
  point 1 ("No new op is served by a host"). This record is the one place that
  exception is made, and §3 bounds it.
- **Related**: #2589 (this decision), #2587 (the handle-shaped process API),
  #1423 (target availability), #1628 (component model), #2588
  (`wasi:http/service`, the same "the host fills the world" shape).

## 1. Context

### 1.1 Two models of `process` lived side by side

- `proofs/target-availability.toml` classed the subprocess family
  **native-only-forever**: "stock WASI (0.2 and 0.3) has no subprocess API"
  (#1423 bucket C, 2026-08-31). Every such fn walled on the structural,
  stock-p1 and embedded legs, and `spec/stdlib/process_exec_status_test.almd`,
  `process_ext_test.almd` and `process_timeout_test.almd` sat in
  `proofs/wasm-fallback-baseline.txt` as HOST_CAPABILITY.
- `docs/wasm/capability-system.md` (2026-07-31) treated `Proc` as a capability
  a host grants, and named an `almide_host_proc` import in an `almide_host`
  module. That import never existed: `git grep almide_host_proc` found it in
  the document only.

### 1.2 What the platform offers (2026)

- WASI 0.3.0 (2026-06-11) standardises cli, clocks, filesystem, http, random
  and sockets. There is no subprocess interface, and the WASI proposal list
  has no candidate. Starting a child leaves the sandbox, so a standard one is
  unlikely.
- WASIX (Wasmer) has a non-standard `proc_spawn`; wasmtime does not run it.
- A wasm runtime resolves every import before the first instruction runs.
  A module that imports what the host does not define is refused at
  instantiation. So "this environment cannot start a child" can be a load-time
  verdict instead of a run-time surprise.

### 1.3 The goal behind the ruling

Every program that can build for wasm should. Of the 11 files the wasm leg of
`almide test` could not pass, 3 were these subprocess files.

## 2. Decision

### 2.1 One interface, written in WIT

`crates/almide-wasm-run/wit/process/spawn.wit`:

```wit
package almide:process;

interface spawn {
  enum op { exec, exec-in, exec-with-stdin, exec-status, exec-status-timeout,
            exec-attached, spawn, kill, is-alive, pid }
  call: func(op: op, a: string, b: string) -> result<string, string>;
}
```

Every operation takes two text operands and answers ok(text) or err(text).
The argument list travels as decimal CHAR-length cells (`<chars>\n<payload>`,
the http_framed cell format), and an operation that needs a third operand puts
it in the first cell. The WIT comments define each operand and answer;
the WIT test in `crates/almide-wasm-run/src/tests/host_process_test.rs` pins
the case order and the `call` shape against the host's op numbers.

The case index is the host op minus 80: ops 80..=89 at the boundary between
the emitted code and its host (`crates/almide-wasm/src/fs_meta.rs`,
`PROC_LEAVES`). The guest half is Almide
(`stdlib/process_wasm.almd`, linked on the wasm legs through the self-host
registry); it builds the cells and decodes `exec_status`'s answer
`<code>\n<stdout chars>\n<stdout><stderr>` into `ProcessStatus`.

### 2.2 Who serves it

- **The embedded host** serves ops 80..=89 to the raw module through
  `almide.fs_call`, like every other embedded service, and also defines the
  import `almide:process/spawn.call` itself on its linker
  (`almide_wasm_run::link_spawn_import`). Both run
  `crates/almide-rt-core/src/process_core.rs`, which
  `runtime/rs/src/process.rs` `include!`s verbatim. Native and the embedded
  host therefore produce the captured text, the exit code and every err string
  with one piece of code. The only native-only part is the `waitpid` probe that
  names a child stopped on the terminal (#2540). It needs `unsafe`, which the
  workspace forbids, so the core takes the poll as a parameter. On the
  embedded host such a child runs into the deadline instead.
- **The p1 artifact** (`almide build --target wasm`). When the op set names a
  process op, `to_wasi` adds the import `almide:process/spawn.call`. This is
  the canonical-ABI lowering of `call`:
  `(op, a_ptr, a_len, b_ptr, b_len, retptr)`, with the answer's bytes placed
  through an exported `cabi_realloc`. It also adds a forwarder behind
  `fs_call` that points the staging pair at the answer
  (`crates/almide-wasi/src/proc_service.rs`). A program that names no process
  op gets none of this. The selection is the one the environ pair gets from
  op 26 and the one `wants_http` makes for the p3 world.
- **A component build** (`--component`, p2 or p3) is refused at build time
  with E081 naming `almide:process/spawn`. No component world declares the
  interface yet (§5).

### 2.3 What a stock host does

It refuses the module at load, before `_start` runs. Measured on 2026-10-03
with wasmtime 47.0.2 on the artifact of a three-line `exec_status` program:

```
$ almide build p.almd --target wasm -o p.wasm
note: p.wasm imports almide:process/spawn (process.*, ADR-0025): a stock WASI runtime refuses it at load; it runs on a host that implements that import — `almide run p.almd --target wasm` is one
Built p.wasm (6644 bytes, structural leg, …)
$ wasmtime run p.wasm
Error: failed to run main module `p.wasm`
Caused by:
    0: failed to instantiate "p.wasm"
    1: unknown import: `almide:process/spawn::call` has not been defined
$ almide run p.almd --target wasm
start
code=0 out=hi
```

The build says so, the runtime refuses at load, and the program never gives a
wrong answer at run time.

### 2.4 `[permissions] proc`

`almide.toml` may list the commands the subprocess family may start:

```toml
[permissions]
proc = ["git", "cargo"]
```

- **Statically** (`cli::check_proc_allowlist`, run by `run`, `build` and
  `test` on both targets over the linked IR, and by `almide check` over its
  pre-link IR, which sees the direct calls): every call that starts a child
  must name its command as a string literal on the list. A literal outside the list is refused with
  `process.exec("cargo") (line N): `cargo` is not in [permissions] proc`. A
  command computed at run time, or a spawning fn passed as a value, cannot be
  checked and is refused too. A program that compiles therefore never starts
  a command outside the list, on any target.
- **At run time on the embedded host** (`almide_wasm_run::set_proc_allowlist`):
  the same list bounds every operation that starts a child. A command outside
  it is answered with an err naming the command before anything runs. This
  binds an artifact the static gate did not see.
- Without the key nothing is bounded, as before. `proc = []` allows no command.

`Proc` is not yet its own effect in the checker. `pass_effect_inference` maps
`process` to `Env`, so `allow = ["Env"]` is what admits a process call today.
The `proc` list narrows within that grant. Splitting `Proc` out of `Env`
changes which existing manifests compile and is not part of this decision.

### 2.5 The availability ledger

`proofs/target-availability.toml` gains a class, **host-capability**: the fn
works only where the host grants a capability stock WASI does not define. It
runs on native and on the embedded host, and its stock artifact builds but
carries a private `almide:*` import that a stock runtime refuses at load. The
availability probe reads the import off the artifact, so the stock-p1 wall is
measured, not declared. The ten subprocess rows move to this class with the
stock-p1 leg only. `process.env` and `process.sleep` lose their rows: they
lower over host ops 26 and 36, which every host serves.

## 3. The exception, and its bounds

ADR-0023 §2.6 rejects private host ABIs because a stock runtime would run an
artifact the toolchain built for a host it does not have. Its reasoning holds
where a standard exists to lower to: HTTP has `wasi:http`. Subprocesses have
no standard and no proposal, so there is nothing to lower to. Refusing the
whole family on wasm leaves programs unbuildable that a host could run. This
ADR admits the private interface on four conditions. Removing any one of them
removes the exception.

1. **Selected by the op set.** The import ships only in an artifact whose
   emitted op set names a process op. A process-free program's artifact is
   byte-identical to its artifact before this decision; the size and
   allocation ledgers are the evidence.
2. **Loud at load.** A host without the import refuses the module before it
   runs (§2.3). It never fails silently or with a wrong answer.
3. **Bounded by the manifest.** `[permissions] proc` holds statically and on
   the embedded host (§2.4).
4. **One closed interface.** The exception is `almide:process/spawn` and
   nothing else. Another private `almide:*` interface needs its own record. A
   new operation in this one is a new `enum op` case, landed with its WIT test,
   its host arm and its cross fixture in one change.

## 4. Evidence

- `spec/stdlib/process_exec_status_test.almd`, `process_ext_test.almd` and
  `process_timeout_test.almd` pass the wasm leg of `almide test` on the
  embedded host and leave `proofs/wasm-fallback-baseline.txt`.
- `spec/embedded_cross/process_spawn_family.almd` exercises every operation:
  success, a non-zero status, a spawn failure, a fired deadline, a chatty
  child, `spawn` / `is_alive` / `kill`, and non-ASCII output through the CHAR
  cells. `almide run` and `almide run --target wasm` print it byte for byte
  identically (`tests/embedded_cross_test.rs`).
- `crates/almide-wasm-run/src/tests/host_process_test.rs`: the canonical
  import lands an answer through `cabi_realloc`; a `to_wasi` artifact with
  op 83 carries the import and runs to the right output on a host that defines
  it plus two preview-1 calls; a host without it refuses the module; an op set
  without a process op ships no import; and the WIT case order matches the
  host ops.
- `tests/proc_allowlist_test.rs`: the static gate on `check`, native `run`,
  wasm `run` and wasm `build`, for a literal off the list, a computed command
  and a spawning fn passed as a value.

## 5. Alternatives

- **Keep native-only-forever.** This leaves three spec files and every program
  that shells out unbuildable for wasm, even though a host could serve them.
  The ruling rejected it.
- **WASIX `proc_spawn`.** It is another vendor's private ABI with its own
  semantics, and wasmtime does not run it. Adopting it would add a second
  private ABI without removing the first.
- **Refuse at build time (E081) instead of at load.** This is the old
  behaviour. It is loud, but it makes the artifact impossible for a host that
  could serve it, and the ruling asks for load-time refusal on stock hosts.
  E081 stays for component builds, where no artifact shape exists yet.
- **A component world import now.** The p2/p3 shims place every host-produced
  value through a reservation-disciplined `cabi_realloc` (#2119). That works
  because the shim chooses the bound before the call. A subprocess answer has
  no bound the guest can choose. Carrying `almide:process/spawn` into a
  component needs a streaming answer (`stream<u8>`, the #2587 handle shape)
  first. Until then the component route refuses with E081.
- **A resource-shaped interface now** (`resource child { wait, stdout, stop }`,
  the #2589 sketch). The current surface is synchronous, and #2587 decides the
  handle API and its `Exit` type. `call` with an `enum op` covers today's
  surface. The handle can arrive as new cases or a new `resource` in the same
  package without breaking `call`.

## 6. Consequences

Gains:

- Every subprocess fn builds and runs on the embedded wasm host with native's
  observables, and the wasm fallback list loses three files.
- Process-free artifacts do not change.
- The capability is visible in the artifact: its import list says whether it
  can start a child, and a host that will not grant it refuses at load.

Costs:

- The embedded host serves ten new ops, which ADR-0023 §3.1 point 1 otherwise
  forbids (§3 bounds it).
- A process program's p1 artifact runs only on a host that implements
  `almide:process/spawn`. Stock runtimes refuse it, by design.
- `cabi_realloc` in the p1 artifact bumps the heap frontier and never frees.
  Each answer costs its length for the life of the instance.
- A child stopped on the terminal is named on native and runs into the
  deadline on the embedded host (§2.2).

## 7. Falsifiers

This decision is withdrawn or revised if any of these is observed:

1. A WASI standard (or a ratified proposal) defines a subprocess interface.
   Then the family lowers to it, and this private interface is retired by the
   ADR that adopts it.
2. A stock runtime instantiates an artifact that carries
   `almide:process/spawn` without defining it, or runs it to a wrong answer
   instead of refusing. The load-time guarantee is then false.
3. A program whose op set names no process op ships the import, or its
   artifact bytes change because of this mechanism. The selection is then
   broken (the size ratchets measure this).
4. `spec/embedded_cross/process_spawn_family.almd` diverges between native
   and the embedded host. The shared-core claim is then false.
5. A program built under `[permissions] proc` starts a command outside the
   list on any target.

## 8. References

- WASI 0.3: https://wasi.dev/releases/wasi-p3,
  https://bytecodealliance.org/articles/WASI-0.3
- WASI proposals: https://github.com/WebAssembly/WASI/blob/main/docs/Proposals.md
- Component Model on wasmtime:
  https://component-model.bytecodealliance.org/running-components/wasmtime.html
- Canonical ABI (flattening, `cabi_realloc`, `MAX_FLAT_RESULTS`):
  https://github.com/WebAssembly/component-model/blob/main/design/mvp/CanonicalABI.md
- Deno `--allow-run`: https://docs.deno.com/runtime/fundamentals/security/
