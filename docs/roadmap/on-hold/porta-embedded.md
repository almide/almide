<!-- description: Porta-style WASI agent runtime for IoT: <10KB Almide guests on tiny hosts -->
# Porta Embedded — Sub-10KB Almide IoT Agents on WASI Hosts

**Priority:** after `porta` reaches v1.0 and graphics stack stabilizes
**Prerequisites:** WASI Preview 2 sockets binding in Almide stdlib, `wasi-tls` proposal landing, size-optimized WASM emit pass
**Principle:** Write an IoT agent once in Almide. Run it on any WASI-compliant host — desktop wasmtime for prototyping, WAMR / wasm3 on the MCU for deployment.
**Differentiator:** The same Almide guest binary runs on `porta` (desktop) and `porta-embedded` (MCU). No other embedded runtime offers that.

> "Almide agent = 10 KB. Host runtime = ≤ 100 KB. Everything a connected IoT node needs, in ≤ 110 KB total."

---

## Thesis

Almide has a real path to a 10 KB IoT agent, but only if it stops trying to *be* an RTOS and instead *runs inside* a WASI-compliant host that provides TLS, TCP/IP, scheduling, and crypto as imports. The heavy math (TLS ciphers, certificate parsing, TCP state) stays in the host runtime (≈ 64–85 KB with WAMR or wasm3). Almide owns the application layer, MQTT framing, business rules, and capability policy — all the parts where the language's LLM-authoring and compile-time specialization actually matter.

This is the `porta` philosophy extended to MCUs: *capability-controlled WASM guest, service-providing host, portable agent binary.*

---

## Why Not a Bare-Metal RTOS Clone

A 10 KB all-in RTOS that includes TLS, TCP/IP, MQTT, and a shell is physically impossible, regardless of language. The binding constraint is mathematical, not compiler-related.

| Irreducible subsystem | Minimum size | Why |
|---|---|---|
| AES-128-GCM (software) | ~1.5 KB | S-box + Galois field multiply |
| SHA-256 (software) | ~0.8 KB | Round constants + mixing function |
| X25519 | ~2 KB | Multi-precision integer arithmetic |
| X.509 DER parser | ~5 KB | ASN.1 decoder + OID table |
| TCP state machine | ~5 KB | Retransmit + congestion + sequence |
| MQTT 3.1.1 QoS 2 | ~3 KB | 4-way handshake state tracking |

Even with Almide's DCE, monomorphization, and zero-libc advantage, the software-only floor is ~20–25 KB. The only ways under that are:

1. **Hardware offload** of crypto and TCP (STM32 CryptoCell, ESP32 hardware AES, NXP MAC+IP). Possible but ties the agent to a specific MCU family.
2. **Delegation to a WASI host** that already implements the heavy stacks. Portable, architecturally clean, and already proven by `porta` on the desktop side.

Path 2 is the one this roadmap commits to.

---

## Where Almide's Size Advantage Comes From

Obsid ships at 3.8 KB today — a fully-featured 3D scene engine. The Rust + WebGL equivalent is 20–30 KB. The 5×–8× density comes from compiler properties that survive unchanged into the embedded target:

- **No libc.** Almide's WASM emit doesn't pull crt0, printf, or format-parsing machinery.
- **Whole-program DCE.** Function-level dead-code elimination runs by default; nothing stays in the binary because it *might* be called.
- **Monomorphization.** A TLS library written once in Almide specializes to exactly the ciphersuite in use; alternate code paths vanish at compile time.
- **Compile-time const propagation.** Far finer-grained than C's `#ifdef`; configuration choices become direct substitutions.
- **No vtable layer.** `effect fn` rewrites to plain Result early-return, no dynamic dispatch cost.

The one headwind is bytecode density: WASM is 1.5–2× larger than Thumb-2 for equivalent logic. Almide's compile-time advantages more than offset this when the binary is the *guest*, not the whole system.

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

The guest imports named WASI Preview 2 functions via `@extern(wasm, "wasi:sockets/tcp", ...)`. The host runtime resolves those imports against its own TCP/TLS implementation. The same `.wasm` file runs under desktop wasmtime, which pleases developers doing local iteration.

### Runtime options (known tradeoffs)

| Host runtime | Size | Notes |
|---|---|---|
| `wasm3` | ~64 KB | Pure interpreter, smallest known footprint |
| `wasm-micro-runtime` (WAMR) | ~85 KB+ | Interp + AOT, Bytecode Alliance, broader WASI coverage |
| wasmtime | ~10 MB | Desktop-only, what `porta` already uses |
| custom Almide-authored runtime | — | Deferred; see §"Scope Out" |

WAMR is the default recommendation once WASI Preview 2 support lands upstream. wasm3 is the fallback for flash-constrained targets.

---

## Relationship to Existing `porta`

| Axis | `porta` (existing) | `porta-embedded` |
|---|---|---|
| Target | Desktop / server | MCU / IoT device |
| Host runtime | wasmtime (~10 MB) | WAMR / wasm3 (~64–85 KB) |
| OS layer | macOS / Linux + `sandbox-exec` | Bare metal, no RTOS or thin RTOS |
| Use case | AI agent sandbox, Claude Code control | Sensor / actuator / telemetry agent |
| Capabilities | FS / Network / Exec | GPIO / I²C / SPI / Network |
| Manifest | `porta.toml` + `manifest.json` | Same format, shared parser |

The shared manifest and capability model are load-bearing: **one Almide agent, prototyped on desktop `porta`, deployed unchanged to `porta-embedded`.** That cross-host portability is the feature no other embedded runtime offers today.

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
| Binary size | Guest < 10 KB, host + guest < 110 KB. Unmatched at this layer. |
| LLM authoring | IoT business rules written in Almide are far easier to LLM-edit than C. |
| Zero-copy | WASI linear memory carries payloads; no serialization tax. |
| Cross-host | One guest binary runs on desktop and MCU. Nothing else in the market does this. |
| Capability security | `porta`'s capability model reaches the MCU — defense-in-depth for field devices. |

---

## Scope Out

- **Bare-metal RTOS authoring.** Covered by the math floor above. Not a language problem.
- **Software TLS rewritten in Almide.** Crypto is irreducible math; rewriting it wastes effort.
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
