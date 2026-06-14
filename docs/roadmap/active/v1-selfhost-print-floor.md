# v1 self-host print floor — the ③ observability keystone

**Status**: design (2026-06-14). Synthesized from a 5-agent design workflow
(prior-art + codebase inventory + 2 candidate floors; the judge step was
interrupted, so this is the hand-synthesized decision). Supersedes nothing; it
*concretizes* `v1-mir-architecture.md` §4 for the FIRST self-host slice.

## Why print is the keystone

The v1 EXECUTION path (lower→MIR→render_wasm→wasm→wasmtime) byte-matches v0 today
for heap-value programs and scalar-call programs (commits 915fe2ee, 8cb5a093).
But the v0 oracle compares program **stdout/exit**, and v1 has **no print**, so
NO computation is observable end-to-end — scalar values, control flow, data all
run "blind" (unit-testable only). **Print is the observation channel** that makes
every other feature end-to-end verifiable against v0, and real programs need
output. So `println` is the single highest-leverage unblock toward "all v0
features run through v1".

The wiring already exists: `println(s)` lowers to `Op::Call{RtFn::PrintStr}`
(lower/calls.rs) and render emits `(call $print_str ...)` — but `$print_str` has
**no body**. The discipline test `handwritten_wasm_runtime_does_not_grow`
(render_wasm.rs, baseline 11) forbids adding `$print_str` to the hand-written
preamble (that is the v0 trap). So print_str must be **self-hosted in Almide**,
compiled through v1 to `(func $print_str ...)` — then the preamble func count is
unchanged and the discipline test PASSES.

## The decision: the THIN primitive floor (candidate A), scoped to print_str

Two candidates were designed:

- **A — thin floor**: new MIR primitive ops `Load{width}` / `Store{width}` /
  `HostCall{cap}` (raw memory + the fd_write host import), inline-rendered;
  print_str (and ultimately all stdlib) written in Almide over them. The ideal
  per §4 (smallest trusted/un-provable surface, the "~20-op provable floor").
- **B — fat `host.write_line(s)` primitive**: one op that bakes the byte-copy +
  iovec + newline + fd_write inline. Print works in one brick, but it **grows the
  hand-written WAT body** (it only games the `(func $` *count*; the trusted WAT
  *surface* grows) and bundles stdlib into the floor — the v0 trap, "staged to
  dissolve later". Rejected: "ship the trap now, thin it later" is the patchwork
  the project forbids.

**Decisive insight that makes A tractable for print_str (refuting the "A needs
multi-brick control-flow" worry):** `print_str` needs **no copy loop, no itoa, no
Div/Rem/compare, no general control flow**. `fd_write` takes an **iovec array**,
so print zero-copy with a **2-element iovec**:

```
iovec[0] = { ptr = s + LIST_HEADER, len = load32(s, LIST_LEN_OFFSET) }  // the string bytes, in place
iovec[1] = { ptr = NEWLINE_ADDR,    len = 1 }                            // "\n"
fd_write(STDOUT_FD, IOVEC_ADDR, /*count*/ 2, NWRITTEN_ADDR)
```

The OS does the gather; we never loop. The control-flow + itoa worry applies to
`print_INT` (number formatting), **not** `print_STR`. So the thin floor needs,
for print_str: `load`/`store` (with immediate offset) + `fd_write` + the single
pointer add `s + LIST_HEADER` — i.e. **scalar value flow + 3 floor ops**, no loop.

## Floor design (the trusted, proof-bounded set)

New MIR ops (a CLOSED set, inline-rendered by a TOTAL match — they add **zero**
preamble `(func $...)`, so the open-stdlib baseline-11 ratchet is untouched; they
are accounted SEPARATELY as the floor, exactly like `RC_PRIMITIVE_FNS`, and are
small/total/decision-free → faithful-to-wasm-spec provable, the §4.1 target):

| MIR op | wasm | irreducible because |
|---|---|---|
| `PrimLoad{width, signed}` | `i32.load8_u`/`i32.load`/`i64.load` | reading raw memory at a computed offset is below every high-level op |
| `PrimStore{width}` | `i32.store8`/`i32.store`/`i64.store` | the dual; needed to pack the iovec |
| `PrimHostCall{cap}` | `(call $fd_write ...)` (the existing WASI import) | wasm has no syscalls; the host import is the only sandbox exit, host-defined, un-expressible in-language. Carries a `Capability` → stays in the exhaustive caps accounting |

Source mechanism: a reserved **`prim` module** of compiler-recognized intrinsics,
dispatched **by name** at MIR-lowering (the exact seam `println`/`RtFn::PrintStr`
already use) — NOT the v0 `@wasm_intrinsic`/`RuntimeCall` route (that is
v0-emitter-only; v1 lowering defers `RuntimeCall` to `Opaque`/`Const`). The
checker registers `prim` with fixed signatures; `prim.load32(ptr, off)` lowers to
`Op::PrimLoad{..}`. Gate `prim.*` to runtime source files (an unsafe sub-language,
like AssemblyScript's `load<T>`/`store<T>`).

```almide
// print_str.almd (the runtime author writes this; compiled through v1 like any fn)
fn print_str(s: String) -> Unit = {
  let len = prim.load32(s, LIST_LEN_OFFSET)        // read header len
  prim.store32(IOVEC_ADDR, 0, prim.add(s, LIST_HEADER))  // iovec[0].ptr = s+12
  prim.store32(IOVEC_ADDR, 4, len)                  // iovec[0].len
  prim.store8(NEWLINE_ADDR, 0, 10)                  // '\n'
  prim.store32(IOVEC_ADDR, 8, NEWLINE_ADDR)         // iovec[1].ptr
  prim.store32(IOVEC_ADDR, 12, 1)                   // iovec[1].len
  prim.fd_write(STDOUT_FD, IOVEC_ADDR, 2, NWRITTEN_ADDR)
}
```

(Exact prim surface — whether offsets are immediate args vs an explicit
`prim.add` — is settled in the implementing slice; the floor is load/store/hostcall.)

## Decomposition (campaign order toward "all v0 features")

1. **Scalar value computation** (FOUNDATION, the prerequisite — and the deferred
   (a') track): `Op::Const` carries a literal value (`Const{dst, value: Option<i64>}`,
   `None`=deferred/0); the lowering EMITS `IntBinOp` for scalar arithmetic instead
   of deferring to `Const`; scalar values thread through (a call/prim result flows
   as a real value, not `Const(0)`). Unit-testable now (function-export+call or the
   PrintInt fixture); becomes e2e-observable once print lands. Invasive: `Op::Const`
   is matched at ~12 sites (cert/V/render/lower) — all keep it a no-op for ownership
   (a scalar carries none), so the ownership cert is byte-unchanged.
2. **`prim` module + the 3 floor ops** (`PrimLoad`/`PrimStore`/`PrimHostCall`): the
   checker module + the by-name lowering + the inline render + the discipline-test
   floor accounting (separate, like RC_PRIMITIVE_FNS).
3. **print_str.almd** over 1+2 → v1 compiles it to `(func $print_str)` → the
   existing `PrintStr` CallFn resolves → **`println("hello")` runs + byte-matches v0**;
   discipline test PASSES (preamble unchanged); corpus-wall/gate/cargo test green.
4. **Observability unlocked** → scalar values, control flow, etc. become e2e
   v0-verifiable. Then: `print_int` (itoa → the Div/Rem/compare + control-flow
   track), then `string`/`list`/`map`/`json` self-hosted via the same floor —
   the §4 "zero hand-written wasm runtime" convergence.

## Invariants (守る系, must hold throughout)

- Floor ops are inline-rendered → preamble `(func $` count unchanged → discipline
  test PASSES; the floor is accounted SEPARATELY (closed, proof-bounded), never
  against the open-stdlib baseline.
- `PrimHostCall` carries a `Capability` → the sandbox/caps accounting stays
  exhaustive (a host effect is never invisible).
- Scalar values carry no ownership → the ownership cert (i/a/d/m) is byte-unchanged
  by the scalar-value + prim work; corpus-wall ownership/names/caps ACCEPT counts
  must not move except as a verified recovery.
- The self-host print_str is a normal `MirFunction` — no new hand-written WAT.
