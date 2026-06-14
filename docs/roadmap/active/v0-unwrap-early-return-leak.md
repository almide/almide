# v0 wasm codegen: Try/Unwrap/Fan early-return heap leak

**Status**: designed, NOT implemented (dedicated-session work — RC-critical).
**Discovered**: 2026-06-14, by the v1 trust-spine adversarial-verify workflow
(`wf_54ac302a`), while sweeping the `(あ)` small-lowering-gap tier.
**Severity**: MEDIUM — a LEAK, never a use-after-free / double-free. wasm linear
memory is reclaimed wholesale at instance teardown, so the cost is bounded for a
short program and unbounded only for a long-running process that takes an error
early-return path repeatedly. The normal (Ok) path is unaffected.

## The bug

For an `effect fn`, `x!` (`IrExprKind::Unwrap`), the effect-fn auto-`?`
(`IrExprKind::Try`), and `Fan` auto-unwrap all PROPAGATE `Err` by an EARLY
RETURN. The v0 **wasm** emitter renders that Err path as a bare early return:

```
// crates/almide-codegen/src/emit_wasm/expressions.rs
//   Unwrap/Result : 846   local_get(scratch); return_;
//   Unwrap/Option : 826   ... return_;
//   Try           : 786   ... local_get(res); return_;
//   Fan single    : 715   ... return_;
//   Fan multi     : 749   ... return_;
```

The function's per-heap-local `rc_dec` frees are inserted by the Perceus pass
ONLY at the terminal `Ret`/`Nop`
(`crates/almide-codegen/src/pass_perceus.rs:389-441 insert_decs_before_ret`).
The early `return_` is an unconditional control transfer to the function end that
jumps PAST all of those decs. So any heap local already bound and still live at
the early-return point is LEAKED on the Err path.

**Rust is leak-free**: `x!` renders as Rust `?`
(`crates/almide-codegen/src/walker/expressions.rs:710-753`), whose early
`return Err(..)` runs scope-exit `Drop` for every in-scope local automatically.
So this is a **wasm-only** codegen leak.

**Why `break`/`continue` are already fixed but Try/Unwrap are not**:
`flatten_exit_tail_blocks` (pass_perceus.rs:661-704) hoists a `continue`/`break`
that sits in TAIL position so `insert_decs_before_ret` treats it as the terminal
and emits the decs BEFORE it. It only matches `Continue`/`Break`
(`tail_block_ends_in_exit`), and only in tail position. `Try`/`Unwrap`/`Fan` are
MID-EXPRESSION values, never a block tail, so the tail-promotion never applies —
and there is **no `IrExprKind::Return` node** in the language to desugar them to
(Almide has no `return` keyword), so the early-return cannot be turned into an
explicit IR control-flow node that Perceus's existing return-dec logic would see.

## v1 trust-spine interim (DONE, commit 4fbbefe7)

The v1 MIR lowering now WALLS a `Try`/`Unwrap` whenever an owned heap local is
LIVE (`expr_has_early_return` + `!live_heap_handles.is_empty()`, guard at
`lower_stmt`/`lower_tail`). This keeps the proven checker from certifying the
leaky shape (it would otherwise be **accept-but-unsafe** for the `no_leak`
clause). Cost: in-profile 4081→4022 (−59). **This v0 fix RECOVERS those 59** —
once the wasm render is leak-free, the wall can be lifted (the `live_heap_handles`
guard removed), because the deferred-continue cert becomes faithful again.

## The core constraint: per-exit liveness

Each early-return point has a DIFFERENT set of live heap locals (the locals bound
BEFORE it and not yet moved/dec'd). A single function-epilogue that decs ALL heap
locals would DOUBLE-FREE the ones not yet bound on the early path, or dec a local
already moved out. So the fix MUST dec exactly the live-at-this-exit set. The only
authoritative source of that set is the **Perceus liveness analysis** — the
emitter must NOT re-derive it (the emit layer's ownership is hand-written and
drifts from the analysis; an independent emitter live-set is the classic
double-free generator).

## Recommended approach: Perceus annotates, emitter emits

1. **Perceus** (pass_perceus.rs), during its existing liveness pass, computes for
   each `Unwrap`/`Try`/`Fan` node the set of heap-local `VarId`s LIVE at that node
   (alloc'd before, not yet dec'd — exactly what it already tracks for terminal
   dec placement). Attach it via a side-table keyed by a stable node id, or a new
   `Vec<VarId>` field on the Unwrap/Try/Fan IR node (an IR-shape change — prefer
   the side-table to avoid touching every IR consumer).
2. **wasm emitter** (emit_wasm/expressions.rs), at each `return_` it emits for an
   Unwrap/Try/Fan Err path, first emits `local_get(v); call(rc_dec)` for each
   `VarId v` in that node's live set (looked up from the side-table). The value
   being returned (the Err Result ptr) is NOT in the live set (it is the result,
   not a local), so it is never double-dec'd.
3. **native emitter**: NO CHANGE (Rust `?` already runs Drop — adding decs would
   double-free).

This is drift-FREE (the live set is Perceus's, not the emitter's) and surgical
(the emitter only emits a provided set).

### Alternatives considered (rejected)

- **Emitter-owned live-local tracking** (a set updated on each Alloc/Dec the
  emitter emits): drift-prone — the emit layer already drifts from the analysis,
  and a wrong set double-frees. Rejected.
- **Add an `IrExprKind::Return` node + desugar Unwrap→`match{Ok=>v,Err=>return}`
  pre-Perceus**: principled, but a large language/IR change (new node, every
  visitor/consumer, both backends) for a medium-severity wasm leak. Out of
  proportion. Rejected for now.
- **Single cleanup-epilogue via `br`**: still needs per-exit liveness (else
  double-free), so it does not avoid the core constraint. No simpler.

## Test strategy (mandatory before shipping — RC-critical)

1. **Fixture** `effect fn f(s) { let big = make_heap(); let n = parse(s)!; use(big,n) }`
   compiled `--target wasm`; assert the Err path decs `big` (wat2wasm trace shows
   `rc_dec` of the `big` local before the `return_`).
2. **No double-free**: a fixture where the unwrapped value IS the only heap thing
   (no other live local) must dec NOTHING extra on the Err path (the result ptr is
   returned, not freed).
3. **Cross-target byte-stability**: native unaffected; the wasm-vs-native
   differential gate (`spec/wasm_cross/`) stays green (native output unchanged).
4. **Full `almide test` on `--target wasm`** + the wasm determinism harness — a
   double-free traps; a leak is silent, so add a leak-count assertion (alloc vs
   free balance) to the fixture's harness.
5. **Re-run the v1 trust-spine** and LIFT the `live_heap_handles` Unwrap/Try wall
   (`expr_has_early_return` guard in lower_stmt/lower_tail) — corpus-wall should
   return to in-profile ≈ 4081 with all 3 properties still ACCEPT.

## Why not now

RC-critical change in the hand-written / drift-prone wasm emit layer; a wrong
live set is a double-free (memory corruption) in EVERY shipped wasm program, not
just the trust spine. Deserves a fresh, rested, dedicated session with the full
test gauntlet above — as flagged when the goal was set ("専用セッション向き").
The v1 interim wall already makes the trust spine HONEST in the meantime.
