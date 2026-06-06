# C-006 — fan.timeout: the SOLE documented wall-clock divergence

> Prose companion to contract C-006 in [contracts.toml](contracts.toml).
> Status: **flagged-for-revision** — this is the one deliberate cross-target
> exemption (N=1), not a behaviour we certify as equivalent.

## Why it diverges

`fan.timeout(thunk, ms)` is a **wall-clock effect**. The two targets have
fundamentally different notions of time:

| Target | Mechanism | Result |
|--------|-----------|--------|
| native | a real thread + `recv_timeout(ms)` | the timeout can fire; the slow thunk is abandoned |
| wasm   | no clock, no scheduler, no threads | the thunk runs to completion; the timeout never elapses |

There is no portable meaning for "elapsed wall-clock time" in the wasm sandbox,
so `fan.timeout` is **excluded from the cross-target equivalence guarantee**. It
is the only `fan.*` op that is: `fan.race` / `fan.any` / `fan.map` / `fan.settle`
were all made deterministic by list order (see C-004 / C-005) and DO hold the
equivalence promise.

## Why there is no spec/wasm_cross fixture

A `spec/wasm_cross/*.almd` fixture is asserted byte-identical native == wasm by
the gate. `fan.timeout` would, by design, diverge — so a fixture would either
fail the gate or need an `@xt-allow`, and an allow would imply we are still
*trying* to converge it. We are not: it is the exemption. Pinning it as a fixture
would misrepresent the contract.

## How the divergence is made LOUD, not silent

The CLI emits a build-time warning whenever it emits **wasm** for a program that
uses `fan.timeout`:

- the scan: `almide::codegen::program_uses_fan_timeout` (an `IrVisitor` over the
  program) — `crates/almide-codegen/src/lib.rs`.
- wired at the two wasm-emitting entry points:
  - `src/cli/build.rs` (`almide build --target wasm`)
  - `src/cli/commands.rs` (`almide run` on the wasm path)

The warning text names the divergence explicitly:

> `warning: fan.timeout uses a wall clock, which the WASM target has none of —
> ... can differ from native. fan.timeout is excluded from the cross-target
> equivalence guarantee.`

## How this clears

If a portable timeout semantics is ever defined for the wasm target (e.g. a host
clock import with deterministic semantics), flip C-006 to `status = "active"`,
add a fixture, and lower `MAX_FLAGGED` in `scripts/check-contracts.sh` — all in
the same PR. The flagged count is a ratchet; it may only go down.
