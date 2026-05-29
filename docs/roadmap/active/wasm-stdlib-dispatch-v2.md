<!-- description: Recreate WASM stdlib dispatch as a declarative registry on verified WasmIR -->
# WASM Stdlib Dispatch v2 — Declarative Recreate

## Problem

The legacy WASM stdlib dispatch is the engine's largest debt, and it is debt
*because it was built first* — organically, before the typed WasmIR / verifier /
LayoutRegistry existed. Today it is ~1 MB of hand-written `wasm_encoder`
emission scattered across 20+ `emit_wasm/calls_*.rs` and `rt_*.rs` files
(`calls_matrix.rs` alone is 202 KB). Each stdlib function's WASM implementation
is bespoke imperative code with:

- raw `f.instruction(&...)` calls — no stack-effect verification,
- hardcoded memory offsets — bypassing `LayoutRegistry`,
- per-file ad-hoc dispatch (`calls_list.rs`, `calls_list_closure.rs`,
  `calls_list_closure2.rs` — the split itself is accreted debt).

Meanwhile the **Rust** target is declarative (`@inline_rust` / TOML templates).
That asymmetry is the smell: WASM should be just as declarative.

## How stdlib reaches the WASM backend

1. Stdlib is Almide source: `stdlib/<module>.almd`, e.g.
   ```almide
   @intrinsic("almide_rt_string_len")
   fn len(s: String) -> Int = _
   ```
2. A call `string.len(s)` resolves to that declaration; lowering turns it into
   `IrExprKind::RuntimeCall { symbol: "almide_rt_string_len", args: [s] }`.
3. Legacy WASM: `expressions.rs` routes `RuntimeCall` into the `calls_*.rs`
   dispatch (or a "bundled stdlib source" fallback that compiles the `.almd`).

So **the dispatch key is the intrinsic symbol** `almide_rt_<module>_<fn>`.

## Design: one registry on the verified layer

A single `engine/intrinsics.rs` registry maps each intrinsic symbol to an
implementation expressed in **WasmIR `Op`** (stack-verified) using
**`LayoutRegistry`** (no hardcoded offsets) and the existing engine runtime
(`__alloc`, `__string_concat`, …). Entry point:

```rust
/// Returns the ops implementing `symbol(args)`, or None if unknown
/// (→ Op::Unsupported → legacy fallback). Args are lowered by this fn so
/// tiers that need special argument ordering (closures) stay in control.
pub fn lower_intrinsic(
    symbol: &str, args: &[IrExpr], ret_ty: &Ty, ctx: &mut LowerCtx,
) -> Option<Vec<Op>>;
```

Wired into `lower.rs`'s `RuntimeCall` arm: try `lower_intrinsic` first; on `None`
emit `Op::Unsupported(symbol)` (clean fallback, as today).

### Three tiers

- **Tier 1 — inline primitives** (~1–3 ops): `string.len`/`list.len` →
  `FieldLoad{STRING|LIST, LEN, I32}` then widen; `list.get`, `list.is_empty`,
  tuple/option tag reads. Emitted inline at the call site.
- **Tier 2 — library functions**: larger algorithms as verified `WasmFunc`s
  registered alongside the runtime (the `__string_concat` pattern). The
  intrinsic becomes a `Call` to that function. e.g. `string.to_upper`,
  `list.sort`, `list.reverse`. Several map to runtime fns we already have
  (`almide_rt_string_concat` → `__string_concat`).
- **Tier 3 — higher-order** (closure-bearing): `list.map`/`filter`/`fold` take a
  closure pair `[table_idx, env_ptr]`. Implemented as runtime fns that iterate
  and invoke the closure per element via `call_indirect` (reusing the closure
  machinery already built). `lower_intrinsic` lowers the closure arg and threads
  it through.

### Why this is not the legacy debt again

- Every impl is **stack-verified** (`verify_func_stack`) — the legacy raw
  emission was not.
- All memory access goes through **`LayoutRegistry`** — single source of truth.
- **One file, one entry per intrinsic** — no `calls_*_closure2.rs` sprawl.
- Tiers 2/3 are ordinary `WasmFunc`s; a later step can *generate* them by
  compiling the stdlib `.almd` bodies through v2 itself (true bootstrap),
  deleting even the hand-written WasmIR.

## Incremental plan

1. **Slice 0 (proving)**: `intrinsics.rs` + `lower_intrinsic`; Tier-1
   `almide_rt_string_len`, `almide_rt_list_len`, `almide_rt_list_get_or`;
   Tier-2 `almide_rt_string_concat` → `__string_concat`. Wire into RuntimeCall.
   Execution tests through the real pipeline (`string.len("hi")`, etc.).
2. Tier-1 sweep: the remaining pure-read primitives across modules.
3. Tier-2 sweep: allocate/transform ops (`list.reverse`, `string.slice`, …).
4. Tier-3: `list.map`/`filter`/`fold` via `call_indirect`.
5. Map/Set (Swiss Table) intrinsics once the table lands.
6. Bootstrap experiment: compile stdlib `.almd` bodies through v2, replacing
   hand-written Tier-2/3 impls where the body is expressible.

## Success metric

`spec/wasm_cross/*` programs that today fall back on `runtime-call` build and
run through v2 (`wasmtime --invoke main`) with results matching the legacy
emitter. Track the V2-OK count as tiers land.
