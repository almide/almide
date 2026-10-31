<!-- description: Proposed embedded WASI agent runtime; size and cross-host targets require hardware evidence -->
# Porta Embedded — Sub-10KB Almide IoT Agents on WASI Hosts

**Priority:** after `porta` reaches v1.0 and graphics stack stabilizes
**Prerequisites:** WASI Preview 2 sockets binding in Almide stdlib, `wasi-tls` proposal landing, size-optimized WASM emit pass
**Design target:** reuse an Almide agent across desktop and MCU hosts that implement the same required interfaces and semantics. “WASI-compliant” alone does not establish that compatibility.
**Proposed differentiator:** unchanged guest deployment across selected desktop/MCU hosts. This has not been demonstrated here; no market-wide exclusivity is established.

> **Unmeasured target, not a result:** guest ≤ 10 KB and host + guest ≤ 110 KB.
> A total must include the chosen TLS/network stacks, assets and platform glue;
> flash size and peak RAM are separate measurements.

## External baseline checked on 2026-10-31

[Porta at `123bef31`](https://github.com/almide/porta/blob/123bef319c99610dd94fb73c28f6443c5a03fc73/README.md)
describes a native process sandbox (macOS Seatbelt; Linux Landlock/seccomp/
namespaces), as well as a separate Wasm mode. It is not solely a desktop
Wasmtime host. Extending either mode to an MCU is work, not an inherited
security guarantee.

The 3.8 KB Obsid figure below comes from the
[2026-04-11 draft strategy](https://github.com/almide-graphics/obsid/blob/7e31406ee983f43d21cb95ddcb63861bed6c6648/docs/strategy.md),
which describes a browser WebGL engine using a thin JS bridge. It is historical,
not a measurement of a complete native/embedded graphics runtime. This review
did not build the external projects or measure hardware; the resume conditions
remain unmet/unverified rather than automatically satisfied.

---

## Thesis

Almide has a real path to a 10 KB IoT agent, but only if it stops trying to *be* an RTOS and instead *runs inside* a WASI-compliant host that provides TLS, TCP/IP, scheduling, and crypto as imports. The proposal delegates TLS ciphers, certificate parsing and TCP state to the host. The original 64–85 KB engine estimates do not establish the size of a host including these services. Almide owns the application layer, MQTT framing, business rules, and capability policy — all the parts where the language's LLM-authoring and compile-time specialization actually matter.

This is the `porta` philosophy extended to MCUs: *capability-controlled WASM guest, service-providing host, portable agent binary.*

---

## Why Not a Bare-Metal RTOS Clone

The original planning estimates below motivated delegating system services. They are not mathematical lower bounds: code size depends on feature subsets, hardware offload, toolchain and what is included in the measurement.

| Subsystem in original estimate | Illustrative size (not remeasured) | Rationale |
|---|---|---|
| AES-128-GCM (software) | ~1.5 KB | S-box + Galois field multiply |
| SHA-256 (software) | ~0.8 KB | Round constants + mixing function |
| X25519 | ~2 KB | Multi-precision integer arithmetic |
| X.509 DER parser | ~5 KB | ASN.1 decoder + OID table |
| TCP state machine | ~5 KB | Retransmit + congestion + sequence |
| MQTT 3.1.1 QoS 2 | ~3 KB | 4-way handshake state tracking |

The original ~20–25 KB estimate is not a proven software-only floor. Options to investigate include:

1. **Hardware offload** of crypto and TCP (STM32 CryptoCell, ESP32 hardware AES, NXP MAC+IP). Possible but ties the agent to a specific MCU family.
2. **Delegation to a WASI host** that already implements the heavy stacks. Portability and equivalent enforcement must be demonstrated on the chosen hosts.

Path 2 is the one this roadmap commits to.

---

## Where Almide's Size Advantage Comes From

The historical Obsid/browser figure does not establish the old 5×–8× Rust comparison or its transfer to embedded systems. Candidate size mechanisms to measure on the actual guest include:

- **No libc.** Almide's WASM emit doesn't pull crt0, printf, or format-parsing machinery.
- **Whole-program DCE.** Function-level dead-code elimination runs by default; nothing stays in the binary because it *might* be called.
- **Monomorphization.** A TLS library written once in Almide specializes to exactly the ciphersuite in use; alternate code paths vanish at compile time.
- **Compile-time const propagation.** Far finer-grained than C's `#ifdef`; configuration choices become direct substitutions.
- **No vtable layer.** `effect fn` rewrites to plain Result early-return, no dynamic dispatch cost.

Wasm/Thumb-2 density and optimization benefits require same-program, same-feature measurements; no fixed ratio or guaranteed offset is established here.

---

## Architecture

```
┌──────────────────────────────────────────┐
│  Almide IoT agent (< 10 KB WASM)          │  business logic, MQTT framing
├──────────────────────────────────────────┤
│  Almide stdlib (0–4 KB)                   │  bytes, string, math
├──────────────────────────────────────────┤
│  WASI Preview 2 imports                   │  sockets, clocks, random, tls
├──────────────────────────────────────────┤
│  Tiny WASI host (separate binary)         │  WAMR / wasm3 + lwIP + mbedTLS
├──────────────────────────────────────────┤
│  MCU hardware                             │  ESP32-S3 / STM32U5 / RP2350
└──────────────────────────────────────────┘
```

This is a proposed interface layout, not a runnable current binding recipe. Preview 2/component ABI support, adapters, host operations and TCP/TLS semantics must be validated together before claiming the same artifact runs on both hosts.

### Original runtime estimates (not a current compatibility or footprint matrix)

| Host runtime | Size | Notes |
|---|---|---|
| `wasm3` | ~64 KB | Interpreter candidate; full host footprint unverified |
| `wasm-micro-runtime` (WAMR) | ~85 KB+ | Interp + AOT, Bytecode Alliance, broader WASI coverage |
| wasmtime | ~10 MB | Desktop candidate; Porta also has a native process mode |
| custom Almide-authored runtime | — | Deferred; see §"Scope Out" |

WAMR and wasm3 remain candidates pending version-pinned interface, flash/RAM and hardware tests; this document does not select a deployment runtime.

---

## Original desktop/MCU comparison (Porta architecture update above)

| Axis | `porta` (existing) | `porta-embedded` |
|---|---|---|
| Target | Desktop / server | MCU / IoT device |
| Host runtime | wasmtime (~10 MB) | WAMR / wasm3 (~64–85 KB) |
| OS layer | macOS / Linux + `sandbox-exec` | Bare metal, no RTOS or thin RTOS |
| Use case | AI agent sandbox, Claude Code control | Sensor / actuator / telemetry agent |
| Capabilities | FS / Network / Exec | GPIO / I²C / SPI / Network |
| Manifest | `porta.toml` + `manifest.json` | Same format, shared parser |

A shared manifest and capability model are design goals. Equivalent enforcement and unchanged-artifact portability need positive and negative tests on each selected host; neither follows from sharing a schema.

---

## Phases

### Phase 0 — WASI Preview 2 Bindings (2–3 weeks)

- Add `wasi-sockets-tcp`, `wasi-clocks`, `wasi-random` bindings to Almide stdlib.
- Run a plain-TCP echo client in Almide on desktop wasmtime.
- Goal: proof that Almide ↔ WASI Preview 2 plumbing works end-to-end.

### Phase 1 — MQTT Publish Agent PoC (3–4 weeks)

- Implement MQTT 3.1.1 QoS 0 publisher in pure Almide (no TLS yet).
- Target: < 8 KB WASM guest.
- Publish to a public broker from desktop wasmtime, measure message rate and binary size.
- Compare against an equivalent C + lwIP implementation.

### Phase 2 — TLS Integration (blocked on `wasi-tls`)

- When the `wasi-tls` proposal stabilizes, add Almide stdlib bindings.
- Layer MQTT over TLS using host-side TLS.
- Target: < 10 KB WASM guest with the same agent logic.

### Phase 3 — MCU Host and First Hardware Run (1–2 months)

- Benchmark WAMR vs wasm3 for size, RAM footprint, and WASI Preview 2 op coverage.
- Bring up the chosen runtime on one of: ESP32-S3, STM32U5, RP2350.
- Record message rate, power draw, peak RAM.

### Phase 4 — `porta-embedded` Public Release (2–3 months)

- Split `porta-embedded` out of `porta` (separate repo or subdirectory, TBD).
- Unified `porta.toml` schema covering both flavors.
- Example agents (sensor publisher, actuator subscriber, telemetry relay).
- First tagged `v0.1.0`.

### Phase 5 — Vendor HAL Wrappers (ongoing)

- Thin adapters for ESP-IDF, STM32Cube, nRF Connect SDK.
- Host-side WAMR integration templates.
- Keep the developer experience aligned with desktop `porta`.

---

## Differentiator Axes

The graphics stack strategy doc and `ml-inference.md` use the same five axes to evaluate each new package. `porta-embedded` maps as follows:

| Axis | Standing |
|---|---|
| Binary size | Target: guest < 10 KB, host + guest < 110 KB; not measured here. |
| LLM authoring | Hypothesis to compare with C using controlled editing tasks. |
| Zero-copy | Copies and ABI serialization depend on the selected host boundary; measure them. |
| Cross-host | Desired same-artifact behavior on explicitly tested desktop/MCU hosts. |
| Capability security | Proposed MCU host enforcement; not implied by desktop controls. |

---

## Scope Out

- **Bare-metal RTOS authoring.** Outside this application/host split; the size estimates above are not a mathematical impossibility proof.
- **Software TLS rewritten in Almide.** The proposal delegates crypto rather than claiming a new implementation is needed.
- **Custom Almide-authored WASI runtime.** Interesting long-term but requires `no_std` codegen, inline asm, linker section control — all absent today. Borrow WAMR / wasm3 instead.
- **ML inference on MCU.** Lives in `ml-inference.md`. Can share infrastructure later.
- **Graphics on MCU.** Lives in `obsid/docs/strategy.md`. Can share infrastructure later.
- **Ethernet / Wi-Fi drivers.** Host responsibility, not Almide's.

---

## Resume Conditions

Move this item to `active/` when all of:

1. `porta` (existing) has cut a v1.0 release.
2. WASI Preview 2 sockets are stable in at least one host runtime.
3. Almide stdlib has WASI Preview 2 bindings (Phase 0 prerequisite).
4. At least one graphics stack package has frozen to v1.0 (signal that the compiler is stable enough).

---

## Related Roadmap

- **`on-hold/ml-inference.md`** — Same Almide differentiator axes applied to ML inference. Both tracks depend on WASI guest + tiny host architecture, and can share `wasm-simd128` intrinsics and size-optimized codegen work.
- **`obsid/docs/strategy.md`** (external repo) — Graphics stack strategy. Its §C2 "abstract host interface" is the same design principle `porta-embedded` needs.

If all three tracks land, Almide's unified position becomes: **"tiny WASI guests that run the same code across desktop, browser, and embedded — for agents, renderers, and inference alike."**
