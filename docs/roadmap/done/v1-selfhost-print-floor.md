<!-- description: v1 self-host print floor — the observability keystone for the first self-host slice -->
<!-- done: 2026-06-15 -->
> **NOTE**: the `print_str` keystone itself is DONE (commits 74cc1fff, 7d82f628, 915fe2ee,
> b2896c87, 2026-06-14/15 — scalar-value foundation + prim floor + self-hosted `print_str`
> byte-matching v0). This doc's own "NEXT" section below (`print_int` / general control-flow
> execution via `Op::If`) describes separate, likely-still-open follow-on work — if it isn't
> already tracked by a newer active/ doc, a fresh one should be opened for it (not created here).

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

## Settled implementation design (2026-06-14)

**FOUNDATION DONE** (commits 74cc1fff `Op::ConstInt`, 7d82f628 `IntBinOp` emission):
int literals materialize (`fn f() -> Int = 42` → `(i64.const 42)`) and int
arithmetic computes (`a + b` → `i64.add`). `lower_scalar_value` (lower/calls.rs)
recursively lowers Var/LitInt/Int-Add·Sub·Mul, rollback→defer outside the subset.
corpus-wall cert-neutral (scalars carry no ownership). This is exactly what
print_str's `h + 4` / `h + 12` address math + the literal addresses need.

**SOURCE MECHANISM = PATH A** (a `prim` bundled stdlib module — chosen over reusing
`@intrinsic`, which lowers to `RuntimeCall` that v1 DEFERS to Opaque). Stdlib
modules are bundled `.almd` whose signatures `bundled_sigs.rs` extracts; a
`prim.X(..)` call type-checks and lowers to `IrExprKind::Call{Module{"prim", X}}`
reaching v1 lowering UNCHANGED. Steps: (a) add `stdlib/prim.almd` declaring the
prim fns (body `= _`); (b) register `prim` in the bundled-module list
(almide-lang `stdlib_info`); (c) in `lower/calls.rs` intercept `module == "prim"`
(before the purity gate in `lower_pure_module_value_call` / a sibling) and map each
`func` to a MIR prim op.

**MIR PRIM OPS** (new, inline-rendered → preamble `(func $` count unchanged →
discipline PASSES; a CLOSED set accounted as the floor, like RC_PRIMITIVE_FNS). The
MIR is i64-uniform; the i32 wasm memory boundary wraps/extends at the op:

| prim fn (Almide) | MIR op | wasm render |
|---|---|---|
| `prim.handle(s: String) -> Int` | `PrimHandle{dst,src}` | `(i64.extend_i32_u (local.get $src))` — heap handle (i32) → i64 address |
| `prim.load32(addr: Int) -> Int` | `PrimLoad{dst,addr,width:4}` | `(i64.extend_i32_u (i32.load (i32.wrap_i64 (local.get $addr))))` |
| `prim.store32(addr: Int, val: Int)` | `PrimStore{addr,val,width:4}` | `(i32.store (i32.wrap_i64 …addr) (i32.wrap_i64 …val))` |
| `prim.store8(addr: Int, val: Int)` | `PrimStore{…,width:1}` | `(i32.store8 …)` |
| `prim.fd_write(fd,iov,count,nw: Int) -> Int` | `PrimFdWrite{dst,…}` | `(i64.extend_i32_u (call $fd_write (i32.wrap_i64 …)×4))` — carries `Capability::Stdout` |

Cert: PrimHandle/Load/Store are no-ops for ownership (scalars), define their `dst` /
use operands for the name witness. **PrimFdWrite carries `Capability::Stdout`** — the
caps fold must map it to Stdout (like `RtFn::PrintStr`) so print_str (and its callers)
are correctly required to declare Stdout. The ops use i64 ValueIds throughout (the
scalar model); `prim.handle` is the String→Int bridge so all address math is `Int`
`IntBinOp` (no String+Int type error).

**print_str.almd** (uses the foundation `h + 4` / `h + 12` + literal addresses;
addresses match the existing preamble layout: NWRITTEN=0, IOVEC=8, SCRATCH=512):
```almide
fn print_str(s: String) -> Unit = {
  let h = prim.handle(s)            // i64 address of the [rc][len][cap][bytes] block
  let len = prim.load32(h + 4)      // header len
  let data = h + 12                 // byte start
  prim.store32(8, data)             // iovec[0] = { data, len }
  prim.store32(12, len)
  prim.store8(512, 10)              // "\n" at scratch
  prim.store32(16, 512)             // iovec[1] = { 512, 1 }
  prim.store32(20, 1)
  prim.fd_write(1, 8, 2, 0)         // fd_write(stdout, iovec@8, count 2, nwritten@0)
}
```

**LINKAGE** (open detail to settle in-slice): render_program lowers the single
source's `ir.functions`; a bundled `prim`/print_str module is auto-imported for
TYPING but its BODY may not be lowered into `ir.functions`. So print_str's body
must be included — either by concatenating `stdlib/print_str.almd` into the
compiled source, or by having the linker include the bundled print_str fn body
when `PrintStr` is reached. The PrintStr CallFn → `(call $print_str)` wiring exists.

**SUB-SLICES**: (1) the MIR prim ops + render + a HAND-BUILT-MIR `print_str` test
that actually prints "hi" on wasmtime (proves the ops+render in isolation, no
frontend); (2) the `prim` module + the lower/calls.rs intercept (source → ops);
(3) print_str.almd + linkage → `println("hello")` byte-matches v0; discipline test
PASSES (self-hosted, no preamble growth); corpus-wall/gate/cargo test green.

**GOAL (set 2026-06-14): "v1 が v0 と同じように動くまで"** — this floor + print is the
keystone; after it, scalar/control-flow/data all become e2e v0-verifiable, then the
stdlib self-hosts via the same floor.

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

## DONE so far (2026-06-14)

Keystone realized: a PLAIN `println("…")` (v0-identical source) byte-matches v0 via
an auto-linked Almide-written `print_str` over the prim floor (commits 915fe2ee →
b2896c87). Scalar-value foundation complete: `ConstInt`, `IntBinOp` Add/Sub/Mul/Div/Mod.
prim floor: `Op::Prim{Handle/Load/Store/FdWrite}`. print_str writes string+"\n" via
TWO single-iovec fd_writes (a 2-element iovec is not gathered by wasmtime).

## NEXT: control-flow execution → print_int (the campaign's next architectural slice)

This is SOUNDNESS-CRITICAL (it touches verify_ownership / the cert) and the corpus
does NOT exercise it (only the runtime will), so it needs DEDICATED adversarial
verification (like the scalar-call slice), best done with fresh focus.

**Why**: `if/match` are currently LINEARIZED (both arms lowered into the flat op list,
value deferred to `Const`/`Opaque`) — sound for the cert but it does NOT EXECUTE (both
arms' effects would run). `print_int` (recursive itoa) needs a real branch: only the
taken arm runs.

**Design** (scalar first; heap-result if is a later step):
- `Op::If { cond: ValueId, then_body: Vec<Op>, then_val: Option<ValueId>, else_body:
  Vec<Op>, else_val: Option<ValueId>, dst: Option<ValueId> }` — NESTED op bodies.
- Comparison `IntOp`s `Lt/Le/Gt/Ge/Eq/Ne` for the cond (gate on `left.ty == Ty::Int`),
  rendered `(i64.extend_i32_u (i64.lt_s …))` (i64 cmp → i32 0/1 → i64).
- Lowering: adapt `lower_branch` to EMIT `Op::If` with PER-ARM-BALANCED arms (keep the
  existing linearization's per-arm drop discipline — that invariant is what makes
  "run one arm" sound) + the arm values.
- Cert: `verify_ownership` / `name_witness` / `cap_witness` recurse into both arms
  SEQUENTIALLY — the SAME logic the corpus already proves for linearization, now on
  structured arms (soundness by reuse; the per-arm-balance invariant from the lowering
  is the load-bearing premise — adversarially verify it).
- Render: `(local.set $dst (if (result i64) (i32.wrap_i64? — cond is i64 0/1, use
  (i32.eqz (i32.eqz (i32.wrap_i64 cond))) or (i64.ne cond 0)) (then <then_body>
  (local.get $then_val)) (else <else_body> (local.get $else_val))))`.
- `defined_value`/`value_reprs`: `Op::If` dst → its repr. V: recurse or skip.
- THEN `stdlib/print_int.almd`: recursive `put_int(n, pos)` — `if n < 10 then {store8;
  pos+1} else { let p = put_int(n/10, pos); store8(p, '0'+n%10); p+1 }` — over prim +
  Div/Mod[done] + If[this] + recursion[done]. Wire `int.to_string`/PrintInt → it.

After print_int: control-flow + numbers are observable → verify the whole scalar/branch
surface e2e vs v0; then string/list/map/json self-host on the same floor.
