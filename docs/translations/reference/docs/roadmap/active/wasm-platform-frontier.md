<!-- description: Historical platform survey with current core/component/embedded route pointers -->
<a id="wasm-platform-frontier--beyond-core-wasm-30"></a>

# WASM Platform Frontier — beyond core Wasm 3.0

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](wasm-platform-frontier.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](wasm-platform-frontier.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](wasm-platform-frontier.zh-CN.md)

> Non-normative historical reader snapshot. The pinned original is authoritative; no automatic synchronization. No independent rerun of measurements, proof gates or current implementation checks. Quoted commands are not executed. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/wasm-platform-frontier.md)
> The 2026-10-31 route boundary explicitly supersedes old Phase 0–1, env.cwd-wall, broad filesystem-preopen and sequential-fan premises below. The June–August standards/runtime survey is not newly checked. Older shared-thread aspirations are retained as historical plans, not adopted current architecture; blanket equivalence and cross-language platform claims remain source assertions, not independent universal proofs.

<a id="current-route-boundary-2026-10-31"></a>

## Current route boundary (2026-10-31)

The Phase 0–1 status below is historical. Almide now has a structural core
Wasm emitter, supported component packaging, an embedded WASI 0.3 HTTP lane,
and embedded filesystem/network/process operations. Availability is still
operation- and route-specific; this does **not** mean every host serves every
operation (the child-handle process family remains a pending Wasm port).

Use [the host overview](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/README.md),
[target availability](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/target-availability.toml),
[build_wasm.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/cli/build_wasm.rs) and
[component_host.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-wasm-run/src/component_host.rs).
`env.cwd` is served on the embedded route; current filesystem scoping and
preopen defaults must not be inferred from the old `--dir=/` sketch below.
The embedded fan implementation uses isolated worker instances, so a blanket
“Wasm fan is sequential” premise is also obsolete.

The following standards/version survey is retained as its June–August
snapshot, not a newly verified external roadmap or deployment recommendation.
Current repository tool pins are in [toolchain-pins.txt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/scripts/toolchain-pins.txt).
The implementation plans below are not a claim that their complete future
architecture has landed.

> **Active scope: Phase 0-1** — document the deterministic profile and design the `almide_host` ABI.
> Introduce Phase 2-4 (component target, 0.3 async, threads) incrementally as standards mature.
> **Exit criteria (Phase 1)**: the same .wasm executes `http.get/post` on both a browser (fetch shim) and wasmtime (host fn).

Tracking core Wasm 3.0 (standardized 2025-09) is **almost complete**. Remaining investment lies outside the core specification: WASI / Component Model and post-3.0 proposals. This roadmap records what is done, what was deliberately rejected, and what to wait for.

<a id="core-wasm-30--現状監査-2026-06-05"></a>

## Core Wasm 3.0 — Current-State Audit (2026-06-05)

<a id="採用済み"></a>

### Adopted

| Feature | Implementation |
|------|------|
| tail calls | almide-mir's `tail_call_indexes` (render_wasm_b.rs) classifies terminal CallFn and emits `return_call` (re-ported in #864, C-178). Self-recursion uses upstream TCO loops. `return_call_indirect` (closure tail) **also ported, 2026-08-03**: the same classification admits CallIndirect (fixture: closure_tail_recursion + instruction gate: tail_call_indirect_test.rs). Pure indirect infinite cycles legitimately decline on creator-side drop, so the depth claim is qualified honestly in the fixture header |
| SIMD (v128) | The old almide-codegen 4× v128 unrolling is not ported to almide-mir; default output has no v128, confirmed in #864. Further coverage: [wasm-optimization-roadmap](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/wasm-optimization-roadmap.md) |
| Strict validation | Assumes wasmtime 45+ / V8 strict validator (StackBalancePass) |

<a id="取る-小さくてミッション直結"></a>

### Adopt (Small and Directly Mission-Relevant)

- **Document deterministic-profile compliance** — **done, 2026-08-03, C-210**. Audit conclusion: raw NaN bits reach observations through only two surfaces, `float.to_bits` and the bytes float-write family (Float is not hashable; to_string fixes "NaN"). Both normalize at the observation boundary to canonical NaN (f64 0x7FF8000000000000 / f32 0x7FC00000), blocking x86 sign-set NaN, engine payload-propagation differences and payload smuggling through from_bits. Stronger than the profile: **identical across both targets and all host architectures**. Two nan_canonical_* fixtures plus an instruction-subset gate, deterministic_profile_test.rs, mechanically check no relaxed SIMD / atomics / shared. Byproduct: the audit found same-name/different-signature self-host linking collision, an invalid-wasm-as-Ok escape (#1068); linker now exact-match merges or walls on collision. Scope: **execution determinism of emitted programs**. The compiler's own output determinism, emitter = pure fn of (IR, target), belongs to [determinism-belt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/determinism-belt.md).
- Extended const expressions: flexibility in global initialization; tiny.

<a id="検討-実測で問題になってから"></a>

### Consider (Once Measurements Show a Problem)

- **Exception handling (`try_table`/`throw`)** could propagate effect-fn errors without tag checks. Blocker: **interaction between Perceus RC and unwinding**; skipped RC decrements during unwinding leak, requiring landing-pad-like cleanup. Results remain first-class values, so value representation stays and only the propagation fast path gains a second encoding. Low-to-medium ROI.

<a id="取らない-理由付き"></a>

### Do Not Adopt (With Reasons)

| Feature | Reason |
|------|------|
| GC | Dual linear↔wasm-gc backends pay builtin-parity tax. Own RC/COW management underpins the byte-identical gate |
| relaxed SIMD | The specification permits implementation-dependent results, directly conflicting with equivalence guarantees. Expand 2.0 fixed v128 coverage instead |
| typed function references (`call_ref`) | Ref types cannot be stored in linear memory; structurally unusable while closures are linear-memory structs + table indices. Meaningful only if GC is adopted |
| memory64 | Adds bounds-check cost for use cases satisfied by 4GB |
| multiple memories | A single memory is intentional for iOS Safari compatibility, per the Memory-section comment in `emit_wasm/mod.rs` |

<a id="wasm-レッグの-fs-パス契約2026-08-03-決定--他言語実装との整合を確認済み"></a>

## Wasm-Leg Filesystem Path Contract (2026-08-03 Decision, Checked Against Other Implementations)

WASI p1 has **no cwd**: no chdir/getcwd syscall; paths are relative to preopen fds. Each language reinvents a userspace fiction; implementations were checked for this decision:

| Language | Cwd on wasip1 |
|------|----------------|
| Go | Userspace variable: `$PWD` (guest absolutization), otherwise **`preopens[0].name`**. Emulates Chdir/Getwd (syscall/fs_wasip1.go) |
| Rust | `env::current_dir()` **gives up** with unsupported error; relative paths rely on matching preopen prefixes |
| wasi-libc (C/Python) | Userspace chdir emulation (`__wasilibc_cwd`), prefix-match resolution |

**Almide contract**: confirmed #874 originally matched Go; completed with Windows support.

1. Guest relative-path resolution uses an env-driven cwd fiction: **`ALMIDE_CWD` (launcher pin, priority) → `PWD` (Go-compatible fallback)**. The launcher pin removes Go's known PWD-staleness issue, e.g. Node execFileSync.
2. The launcher (`almide run/test`) pins cwd in spelling visible to the guest: **unix** = host absolute path, directly guest-valid under `--dir=/`, matching Go's PWD value; **Windows** = `.`, the guest name of the `--dir=.` preopen, equivalent to Go's `preopens[0].name` fallback.
3. Temp is `$TMPDIR ?? "/tmp"`, C-189, Go/Python convention. Windows launcher maps host temp to guest `/tmp` and injects `TMPDIR=/tmp`.
4. Host absolute-path transparency is **unix-host-only** (`--dir=/`). No language passes unix absolute paths on Windows hosts; relative paths + `/tmp` are portable spellings, as taught by CHEATSHEET.
5. **Do not introduce chdir**: Go-like mutable userspace Chdir state conflicts with determinism. `env.cwd` remains a diagnosed wall on wasm.
6. WASI 0.2/0.3 wasi:filesystem uses capability handles without ambient cwd; this preopen contract remains forward-compatible.

Implementation: `wasmtime_fs_args` (src/cli/run.rs) + ALMIDE_CWD/PWD scanning in the WAT shim, render_wasm_fs_wat.rs. Validation: ci.yml `Test WASM (Windows host)`, #1066.

<a id="フロンティア--30-の外側"></a>

## Frontier — Outside 3.0

<a id="1-wasi-0203--component-model--httpasync-の標準-abi"></a>

### 1. WASI 0.2/0.3 + Component Model — Standard HTTP/Async ABI

The wasm target currently supports only WASI preview 1: fs/clock/random/stdio/proc_exit. No sockets, HTTP client or process launch; `calls_http.rs` contains only pure `http.response`/`http.json` builders.

Standards status, rechecked 2026-08:
- WASI 0.2 stable. Wasmtime hosts `wasi-http`; browser/Node use jco.
- **WASI 0.3.0 officially released 2026-06-11, ratified that month and stable**. RC first appeared in Spin v3.5 in 2025-11; official support is wasmtime 43+. Native async at canonical-ABI level, `async func` / `stream<T>` / `future<T>`; `wasi:io` absorbed into Canonical ABI and removed. Subsequent 0.3.x follows a release train. WASI 1.0 still targets late 2026.
- Browsers do not speak WASI directly; either 0.2 or 0.3 needs jco or a shim.
- Repo CI pins wasmtime 47.0.3, updated 2026-08 from 42.0.1, satisfying both 0.3 host 43+ and strict-validation 45+ prerequisites.

Unlocked: `wasi-http` + native async streams enable SSE streaming to LLM APIs. Write an agent loop in Almide and deploy one component to wasmtime / Spin / wasmCloud / edge hosts. Host-side outgoing-request filtering, capability-scoped network, aligns directly with Layer 2/3 of [effect-system-capability](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effect-system-capability.md).

Cost: canonical-ABI lifting/lowering — UTF-8 string-boundary conversions, list/record/variant and resource handles — is major work even synchronously. The 0.3 async ABI is at the forefront of Rust itself's 2026 goals. Wasmtime's p1→p2 adapter maps only existing p1 APIs, so **the adapter does not give HTTP**.

Staged introduction:
1. **Phase 1, now: custom `almide_host` import ABI**. Provide `http.get/post`, etc. as host imports: browser fetch shim, server wasmtime host fn. Compiler only adds imports; no canonical ABI needed.
2. **Phase 2: p1→p2 adapter**. Componentize existing modules, fs-only, almost free.
3. **Phase 3: emit 0.2 synchronous canonical ABI**. Direct wasi-http connection.
4. **Phase 4: 0.3 async**. When WASI 1.0 is in sight; fold custom ABI into a thin compatibility layer and retire it.

<a id="2-stack-switching-post-30-proposal--wasm-上の-async-実行モデル"></a>

### 2. Stack Switching (Post-3.0 Proposal) — Async Execution Model on Wasm

Foundation for coroutines/async. Track proposal phases; converge with Phase 4's async canonical ABI.

<a id="3-shared-everything-threads-post-30-proposal--fan-の-wasm-側本格化"></a>

### 3. Shared-Everything-Threads (Post-3.0 Proposal) — Full Wasm-Side `fan.*`

Wasm's single-thread restriction was the root of cross-target `fan.*` semantic differences; fan.timeout was removed from the language in 0.29.0 because of it. Real shared-memory threads would allow wasm fan to mirror native. Track proposal phases.

<a id="追跡すべきバージョン番号"></a>

## Versions to Track

“Wasm 4.0” does not exist and is not planned. Track:
- **WASI 0.3.x → 1.0**, release train, 1.0 targeting late 2026.
- **Proposal phases of stack switching / shared-everything-threads**.

<a id="references"></a>

## References

- [Wasm 3.0 announcement](https://webassembly.org/news/2025-09-17-wasm-3.0/)
- [WASI roadmap](https://wasi.dev/roadmap)
- [WebAssembly proposals tracking](https://github.com/WebAssembly/proposals)
- [wasmtime-wasi-http](https://docs.wasmtime.dev/api/wasmtime_wasi_http/index.html)
