# Return-op eradication: one desugar for `!`, position zoo deleted

Status: PLANNED (research sealed 2026-08-15; ratification pending)
Law: rot-eradication law 6 — a diverging arm drops the merge continuation.
Prior art: the `??` route-zoo deletion (#1418, +148/−1054) — same disease, one
level up: the recognizer grew a case per fixture; here the desugar table grows
a row per syntactic position.

## The disease

`!` (effect-unwrap-propagate) is desugared **per syntactic position** — ~9
passes in the `BRANCH_PASSES` row table (`desugar_branch.rs`) plus the
stmt/tail pair in `desugar_unwrap.rs`:

| position | pass | what it pays |
|---|---|---|
| stmt / let-bind | `desugar_effect_unwrap` | continuation nested into the ok-arm |
| tail | `desugar_tail_effect_unwrap` | pass-through special case |
| call-arg | `desugar_callarg_unwrap` | ad-hoc ANF hoist |
| if-arm | `desugar_if_arm_unwrap` | own traversal |
| `ok(e!)` | `desugar_unwrap_rewrap_identity` | identity collapse |
| let continuation | `desugar_let_unwrap` | own traversal |
| expr-nested scalar | `desugar_stmt_value_nested_unwrap` (#1183) | second ad-hoc hoist |
| for-loop body | `desugar_loop_unwrap` | loop-carried **error flag** + post-loop dispatch |
| unit if/match stmt + cont | `desugar_stmt_control_unwrap` | **tail duplication** into every arm |

Root cause, stated in `desugar_unwrap.rs`'s own header: *the v1 MIR has no Op
for a mid-function early return*. Each position must therefore restructure its
continuation differently. Every row was added when a fixture walled — the same
growth signature the `??` route zoo had. ~2,800 lines in the dedicated desugar
files alone; `Unwrap`/`Try` handling is scattered across 30+ lowering files.

## Research verdict (5-compiler survey, sealed out-of-repo 2026-08-15)

Surveyed: rustc, Zig, Roc, Lean 4, Koka, Grain. Unanimous on the core:

1. **One desugar site, position-independent** — enabled by making the exit a
   first-class TERMINAL (noreturn) expression. In value position the lowering
   "assigns nothing and continues unreachable" (rustc `lower_expr_try` is the
   only `?` site; Zig `tryExpr` emits a block whose error body ends in
   `ret_node`; Roc desugars `?` once into `match + e_return`).
2. **Exit-path cleanup is derived, never hand-written per sugar** — walk the
   live/scope state at the exit site. rustc: 7-line scope walk into a shared
   DropTree. Zig: `genDefers(inner→outer, mode)` — the exit's *destination*
   is the walk's outer bound. Grain (RC + structured wasm, our exact
   profile): `return` = decref every live managed binding *skipping the
   returned value*, emitted before the return; `break`/`continue` = loop-level
   bindings only, live map untouched.
3. **Tail duplication / error flags are the tax for lacking the primitive.**
   Where the references duplicate a continuation at all it is a *thresholded
   optimization* (Koka: iff ≤1 non-return branch; Lean: join iff >1 regular
   exits), never the strategy.
4. **The no-new-op path is empirically dead.** Lean shipped "sum-type result +
   single dispatch" for ~5 years and replaced it: variant explosion (result
   types × exit-kind combinations), mutated-var tuple threading at every exit,
   code blowup never solved, and they reinvented join points internally anyway.
5. **Frame-targeted beats block-targeted for `!`.** Zig's `ret_node` names the
   function frame, so no enclosing block cooperates at any depth, and the
   error edge is noreturn so no merge/phi negotiation exists. Full join points
   (Roc/Lean) are the general machinery with real scope-invariant and
   ownership-contract costs — overkill for `!`, whose error edge always
   targets the frame. Deferred as a follow-on (see Non-goals).

## The cure: `Op::Return`, one hoist, one rule

### New op

```rust
/// Frame-targeted early exit: return `val` (None = Unit) to the caller.
/// TERMINAL — nothing may follow it in the enclosing arm (mir_wellformed).
Return { val: Option<ValueId> }
```

Render: wat `return` (valid at any block depth), native `return v;`. Both
targets already have the instruction; render cost is zero.

### Lowering rule (the ONE `!` rule)

After the hoist (below), every `!` is in let/stmt position:

```
let x = f()!   ⇒   v = lower(f())
                   IfThen(err_tag(v)) {
                     drops(live_heap_handles ∖ {err_carrier})   // derived, not authored
                     Return(err_carrier)
                   } EndIf
                   x = ok_payload(v)                            // straight-line, no nesting
```

`live_heap_handles` already exists in the lowerer — it IS the scope walk the
references derive drops from. The continuation stays linear: no nested match,
no tail duplication, no loop flag. `!` in a loop body needs zero loop
cooperation (wat `return` exits everything; drops cover the loop's live state).

### The hoist (kills the position dimension)

ONE generic ANF traversal replaces `desugar_callarg_unwrap` +
`desugar_stmt_value_nested_unwrap` + the if-arm/let variants: hoist every
expression-nested `Unwrap` to its own bind-position stmt. Position knowledge
becomes a small table (forbidden binders, delimiter kinds), not passes.

### Verifier (the part only we must prove)

- `lib_c.rs` branch frame gains `diverged: bool`. An arm ending in `Return`:
  (a) takes no part in `check_branch_agreement` — the join continues from the
  OTHER arm's state alone; (b) must independently satisfy the frame-exit
  obligation at the `Return`: every owned object at rc 0 except the returned
  value, which is accounted moved (`m`) to the caller.
- Certificate: `Return` emits a `ret` event closing the stream segment; the
  incs-before-decs discipline is unchanged (drops precede the `Return`).
- Rocq kernel (`OwnershipChecker.v`): the one-shot-branch acceptance gains the
  mirrored rule — an arm whose run ends in `ret` is accepted iff it reaches
  the exit obligation fault-free; agreement is only required between
  non-diverged arms. Witness corpus gains dedicated fixtures (both-arms-ret,
  ret-with-live-handle ⇒ reject, ret-in-loop-body).

## Stages

- **R1 — the op, end to end, unused.** `Op::Return` + `mir_wellformed`
  (terminal: nothing follows in the arm) + both renderers + certificate event
  + verifier diverged rule + kernel rule + witness fixtures. Gate: full
  battery green with the op never emitted; kernel certifies the new witnesses.
- **R2 — the rule behind a probe.** The hoist + the one lowering rule, gated
  by `ALMIDE_BANG_RETURN=1` (the #1418 readiness-probe recipe). Measure the
  decline matrix over the full battery + 3-way oracle; drive it to zero with
  law-4 fixes (typed seeding, ANF retries), byte-A/B against the old path
  where both accept.
- **R3 — the deletion.** Matrix empty ⇒ flip default, delete the position
  rows (`desugar_effect_unwrap`'s nested-match body, `desugar_loop_unwrap`'s
  error flag, `desugar_stmt_control_unwrap`'s tail duplication, the if-arm /
  let / nested / callarg variants), keep `desugar_unwrap_rewrap_identity`
  only if it still pays as an optimization. Same-PR ledger follow-through:
  scalar-read audit, STAGE-STATUS, TCB claims, walled-real ratchet, caps
  accounting (Return emits no calls — count-invariant).
- **R4 — follow-ons (separate issues, not this arc).** #1421 accepting match
  lowering via the same diverged-arm rule; `guard … else <value>` inside
  loops (currently an honest wall); loop-as-join generalization; render-stage
  drop-chain suffix sharing if wasm size regresses.

## Non-goals

- No general join points / labeled breaks in this arc. `!`'s error edge is
  always frame-targeted; the general machinery costs scope invariants and a
  join-aware ownership contract we don't need yet. The door stays open (R4).
- No v0 changes: the v0 leg already lowers `!` to Rust `?`; only the v1 MIR
  leg gains the op. The 3-way oracle keeps both legs honest.

## Exit criteria

1. `BRANCH_PASSES` contains zero `!`-specific rows; `desugar_unwrap*.rs` is
   reduced to the hoist + the identity (if kept).
2. A new `!` position (future syntax) requires ZERO new desugar code — the
   hoist + one rule absorb it. (The rustc test: `do yeet` reused `?`'s
   machinery for free.)
3. Kernel-certified: the witness corpus covers ret-diverged arms both
   accepting and rejecting; verify-trust green.
4. Deletion dividend ≥ 1,500 lines net negative across `lower/`.
