# v0 wasm codegen: Try/Unwrap/Fan early-return heap leak

**Status**: IMPLEMENTED + verified (emitter-side `emit_early_return_decs`); v1 wall LIFTED, -59 recovered (in-profile 4083). Leak-free (100k-err-loop completes) + double-free-free (260-file wasm corpus green).
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

## VERIFIED algorithm (workflow wf_dbf7590c, 6 agents)

The adversarial-verify workflow RESOLVED the catastrophic risk and pinned the exact
algorithm:

- **NO double-free from moves.** The Perceus model is *alias-by-RcInc,
  move-nothing-user*: a heap local handed into a list/record/tuple/concat/call is
  SHARED via an `RcInc` on the donated pointer while the source KEEPS its own
  terminal Dec (emit-side "SHARE dup", expressions.rs:1265-1308). No user heap
  VDecl is ever consumed mid-function without a matching `FnBody::Dec`
  (`collect_heap_vdecls` lists every heap VDecl; `insert_decs_before_ret` gives
  each one terminal Dec). So `vdecld-minus-decd` keeps a *still-owned* local in
  `live` — which is exactly the set to dec on the err path. The `collect_moved_out_vars`
  verifier (pass_perceus.rs:880-910) recognizes only bare-Var block-tail and
  for-in-iterable as "moved", and BOTH are leak-DIAGNOSTIC exemptions, not Dec
  removals. ⟹ the worst-case failure of this whole fix is a *missed* dec (LEAK),
  never a double-free — a SAFE direction.

- **The live set MUST union across the enclosing chain-stack.** Every Block,
  `while`/`for-in` body, and `if`/`match` arm body is its OWN `FnBody` chain
  (`block_to_fnbody`), invisible to an outer chain's `collect_heap_vdecls`. An early
  `return_` exits the WHOLE function, so the live set at a node = the UNION, over
  every enclosing chain from the node outward to the function root, of
  (heap VDecls bound BEFORE the node in that chain) − (heap `Dec`s before it).
  A single FLAT walk LEAKS outer-scope and per-iteration locals (two HIGH-severity
  refute holes confirmed this — both LEAKs, not corruption). Compute it INSIDE
  pass_perceus during the same recursion that processes nested chains, threading the
  enclosing live set into each nested-chain walk.

- **Exclusions (mirror the existing predicates EXACTLY, or risk double-free):**
  (1) `__tco_`/`__br_`/`__perceus_*` temps — same prefix test as
  insert_decs_before_ret:414-418 and the Assign move-exempt:309-315 (they donate
  their ref and never get a Dec, so decing them = use-after-free).
  (2) `EnvLoad`-bound borrow locals — same `!matches!(expr.kind, EnvLoad{..})` guard
  as collect_heap_vdecls:358 (the closure env owns them).
  (3) the IN-FLIGHT bind — a Try/Unwrap inside `let n = …!`'s initializer must NOT
  include `n` (it is bound only after its expr evaluates).
  Dedup by VarId.

- **The returned Err value is a `ScratchAllocator` temp (raw wasm local, no VarId),
  never a member of the live set** → the result ptr is never double-dec'd.

- **Delivery**: field-on-node (`early_return_decs: Vec<VarId>` on Unwrap/Try/Fan)
  costs ~13 OR-pattern arms + 15 constructors but the live set travels WITH the node
  (no keying). Side-table keyed by per-function pre-order node index is 1 struct but
  is sound ONLY if NO pass between Perceus and emit reorders/adds/removes a
  Try/Unwrap/Fan node (a keying mismatch could grab a wrong set → double-free). Pick
  per Perceus's pass position: if Perceus is the LAST IR pass before emit, the
  side-table index is stable and cheaper; otherwise prefer the field for safety.

- **Emit** (expressions.rs ~706/770/798, the three early-`return_` sites): for each
  (VarId v) in the live set, resolve v → (local L, ty T) via var_map + var_table
  (exactly the RcDec handler statements.rs:358-392), emit `local_get(L);
  emit_typed_rc_dec(&T, L)` BEFORE `local_get(scratch); return_`. Must not clobber
  `scratch`.

- **Test the nesting explicitly** (the flat-walk holes prove fixture #1 is
  insufficient): an Unwrap inside a `while`/`for` body AND inside an `if` arm, each
  with an OUTER-scope heap local live across it, asserting the outer local IS dec'd
  on the Err path (alloc/free balance). The flat function-top-level fixture passes
  even with the buggy per-chain set.

## Why not now

RC-critical change in the hand-written / drift-prone wasm emit layer; a wrong
live set is a double-free (memory corruption) in EVERY shipped wasm program, not
just the trust spine. Deserves a fresh, rested, dedicated session with the full
test gauntlet above — as flagged when the goal was set ("専用セッション向き").
The v1 interim wall already makes the trust spine HONEST in the meantime.
