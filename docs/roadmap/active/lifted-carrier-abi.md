# #1437: the lifted carrier's ABI — declaration-driven, one readable layout

Status: DESIGN (scouted 2026-08-24; direction per the issue's recorded candidate —
producer-side agreement by construction). Unblocks the return-op R2 matrix's
fn-family class (40 fn-level declines; ~30 of the remaining fallback files).
Parent arcs: return-op-eradication R2/R3, result-family-from-type Phase 3,
rot-eradication R5.

## The disease, measured precisely

#1437 is two entangled defects, and the scout resolved what "the lift
materializes len-as-tag" actually is:

1. **The hybrid ABI (#841), not a tag-offset choice.** For a can-err lifted
   heap-ret fn, `auto_wrap_abi_body` retypes the ROOT body to
   `Result[T, String]` but `wrap_return_positions_in_ok` is gated
   `opt_ret || !has_propagation_site(body)` (mod_c.rs:599) — a `!`-bearing
   body keeps its ok tails RAW while the err path materializes a real
   wrapped block. No consumer can discriminate a raw String block from a
   carrier; the recorded crash (fs_fold_lines_range, `0x2f2f`) is a raw
   payload's bytes read as a tag. Probe-off, the position desugars later
   rewrite the body into explicit ok()/err() arms, hiding the hybrid; under
   R2 (desugars off) nothing wraps, so the class walls at
   `decl_ret_family = None` (mod_c.rs:552 — the containment).
2. **The ABI decision is a fixpoint over trial lowerings.**
   `AUTO_WRAP_ABI_FNS` membership = body predicates + the mutual-recursion
   trial inlining (`lowers = |f| lower_function_all_with_types(f, ..).is_ok()`,
   mod_p2_b.rs:264), iterated with the never-err strips to a snapshot
   fixpoint (pipeline_b.rs:406-447, #485). Acceptance changes bodies, bodies
   change membership: which fns HAVE a Result ABI depends on what the
   current lowering accepts. The frontend's own lift is already
   signature-driven (`should_lift_effect_fn_ret`,
   pass_result_propagation.rs:78) — the MIR registry is a second, narrower
   opinion keyed by name (rot-eradication R5's residual class).

## The measured fact that shrinks the fix

The @16 tag slot is ALREADY valid on every block the lifted-heap class can
produce, once every exit is actually wrapped:

- err path: `materialize_result_err_str` writes the SUPERSET block
  (len@4 = 1 AND tag@16 = 1; result_materialize.rs:59-93).
- heap ok path: `ok(x)` with heap `x` routes to `materialize_result_str` /
  `_aggregate` — cap-as-tag natively (@16 = 0 on Ok).
- scalar-materializer blocks zero @16 by construction (`Init::ResOkScalar`'s
  8-byte payload store; lib.rs:196).

So for `Result[<heap T>, String]` — which `result_family` types HeapOk — the
producer and the type AGREE at @16 by construction **as soon as the hybrid
dies**. No new layout, no stdlib reader changes, no generated-drop changes.
(The full Scalar-family @16 convergence — RESEARCH Stage A, ~70 files, 14
stdlib readers, 5 generated drops — is explicitly NOT needed for this arc
and stays its own roadmap.)

## Stages

### L1 — kill the hybrid under the probe; lift the containment (the R2 unblocker)

- `auto_wrap_abi_body`: under `bang_return_probe()`, wrap return positions
  unconditionally (the desugars that justified the skip do not run there).
  Probe-off behavior byte-identical.
- `decl_ret_family` arm 2 widens to heap rets:
  `Some(result_family(Ty::result(T, String)))` — the 2026-08-16 experiment's
  one-liner, now justified: under the probe every exit is a wrapped block
  whose @16 read is correct by the producer inventory above. (The experiment
  failed for lack of a discriminating repro BECAUSE the containment made the
  class wall; under R2 the discriminating programs are exactly the walling
  matrix files — walls today, runs after, 3-way byte-equal.)
- The value-position `e!` reader (calls_p4.rs:463-500) hard-codes tag@4 /
  payload@12: family-select the offset if reachable with a heap callee under
  the probe (the generic hoist should move those to bind position first —
  measure, then fix or record why unreachable).
- **Gates**: red→green probe family (lifted heap-ret can-err producer +
  `!` consumer; String / List[String] / record payloads; ok and err paths);
  census fn-family collapse (40 → residue); `spec/wasm_cross/effect_tco`
  stays green probe-ON (the TCO carve-out — wrapping retypes the if spine
  and the loop-conversion must still fire; if it regresses, the wrap keeps a
  tail-self-recursion exception and the doc records it); coverage ratchet +
  battery probe-off unchanged; heap-cap churn; causal A/B.

### L2 — the ABI decision becomes a declaration fact (kills the fixpoint)

- Membership computed ONCE from the ORIGINAL declarations + syntactic
  can-err (compute_can_err is acceptance-independent), before any rewrite;
  the populate→rewrite fixpoint (pipeline_b.rs:406-447) and the trial
  lowering's influence on membership are deleted. The #485 drift class is
  closed in the opposite direction: a rewrite may not CHANGE a fn's ABI —
  a strip that would remove the last propagating `!` keeps the wrapped ABI
  and the wrap materializes trivially.
- The never-err raw-T ABI (load-bearing for the yaml/parser TCO clusters)
  stays: NEVER_ERR ∧ ¬can-err is itself a declaration-level fact.
- Alignment target: the frontend's `should_lift_effect_fn_ret` — one
  decision, one place, folded into the signature at the boundary
  (rot-eradication R5's cure, verbatim).
- **Gates**: `ALMIDE_ABI_PROBE` snapshot diff (membership before vs after
  must be explainable fn-by-fn); whole-corpus 3-way; census; the #485/#786
  regression fixtures.

### L3 — R3 folds the probe default; the hybrid dies everywhere

When the R2 matrix is empty and the probe flips default (return-op R3), the
L1 wrap becomes the only path and `has_propagation_site` + the hybrid are
deleted with the position desugars. No separate work — recorded here so L1
is not mistaken for the end state.

## Non-goals

- No Scalar-family layout change (Result[scalar, String] stays len-as-tag;
  its producers/readers agree today).
- No stdlib .almd reader changes, no generated-drop changes (the @16
  inventory above is why).
- The containment does NOT lift probe-off before L2+L3: probe-off consumers
  of the hybrid are the position desugars, which handle it today.

## Exit criteria

1. R2 census: fn-family declines reduced to the void-fn residue; the
   ~30-file class runs on the wasm leg probe-on, 3-way byte-equal.
2. `AUTO_WRAP_ABI_FNS` population reads no lowering verdict (L2).
3. The #1437 issue closes with the discriminating fixture family committed
   under contract — walls on the pre-L1 compiler (causal A/B), runs after.
