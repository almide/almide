# v1 heap-loop-carried ownership — option C (cert-spine extension), the COMPLETENESS fix

**CEO chose C ("C一択", 2026-06-20): close the proof spine's completeness gap at the ROOT — teach the
proven checker to reason about a loop-carried heap accumulator — rather than route around it (A: in-place
push) or hack the rep (B). C lets the user write the NATURAL recursive `acc + [x]` and have it PROVEN.**

## The gap (why the 11 remaining yaml walls need this)

The base ownership cert (OwnershipChecker.v) is a FLAT per-object event stream — no loop notion. A
loop-carried heap accumulator (`acc = acc + [x]` per iteration: drop old object, alloc new, rebind the
slot) is unrepresentable: an object's `i` is in iteration K, its `d` in K+1 — different objects sharing
one SLOT. `verify_ownership` (flat, one pass) sees an unbalanced `d`/`i` and FALSE-REJECTS safe code.
This is a **completeness** hole (soundness was never at risk). The 11 walls (collect_*/parse_*/seq_item/
map_entry, flow_rec↔flow_step, block_*) all hinge on it (append accumulator + mutual recursion).

## ✅ LANDED 2026-06-20 (commit 7f673b4c): the SOUNDNESS PROOF — the ②-critical core

`proofs/OwnershipLoop.v` (in the proof gate: `_CoqProject` + `check.sh` coqc **and** coqchk + claim-drift;
"PROOF SPINE OK", axiom-clean "Closed under the global context"). It adds a `Loop : list FlatOp -> Op`
construct and PROVES:
- `exec_list` (the checker fold) Loop rule: accept a loop iff its body PRESERVES rc (and doesn't fault)
  from the entry count.
- `Unrolls` : the abstract cert unrolls to a concrete flat run (each `Loop body` → n copies of body).
- **`check_unroll_sound`**: `check ops = true → ∀ unrolling, no_double_free ∧ no_leak`. I.e., a rc-preserving
  loop body is leak/double-free-free for ANY iteration count (induction via `exec_flat_repeat_preserve`).
The accumulator slot cert is `[Inc; Loop [FDec; FInc]; MoveOut]` (acquire once; each iter release-old +
acquire-new = net 0; move out the final). Loop bodies are FLAT (no nested loop) — sufficient for the v1
parser walls (one drop+alloc per iteration); nested loops are a future compose-able extension.
The hard, irreducible part of C (the Coq re-proof — "C needs Coq, not corpus-wall-verifiable") is DONE
and kernel-verified. The rest is gate-verifiable engineering.

## Remaining C integration (each gate-verifiable — corpus-wall + byte-match + the proof gate)

1. **✅ DONE 2026-06-20 (commit c05fc209): Production checker + cert format.** `OwnershipChecker.v` gains
   `CertItem` (`COp`/`CLoop`), `exec_line`, `check_line`, and the soundness re-derivation over the full
   Inc/Alias/Dec/MoveOut/Reuse alphabet (`exec_app`, `exec_repeat_preserve`, `UnrollsL`, `exec_line_unroll`,
   **`check_line_unroll_sound`**, **`check_cert_lc_sound`**) — axiom-clean, in the proof gate. Cert format
   v2: loop delimiters `(`…`)` (`parse_lc`), backward-compatible (no-paren certs fold exactly like flat
   `check`). `Extract.v` extracts `check_cert_lc`; `driver.ml` dispatches ownership to it. `build-checker.sh`
   round-trips real bytes: `I(DI)M` ACCEPT (accumulator slot), `I(I)M`/`I(D)M` REJECT (leak/drain). The
   corpus-wall (14564 heap objs) still ACCEPTs via `check_cert_lc` — zero regression.
2. **✅ DONE 2026-06-20 (commit 291a1f35): Rust loop-aware cert emission + verify.** `lib.rs
   verify_ownership` — `Op::SetLocal { local, src }` now REBINDS a heap slot (`object_of[local] ←
   object_of[src]`, slot live again); the OLD object was released by the body's preceding `Drop`, so the
   per-iteration invariant holds (scalar SetLocal is still a no-op). `certificate.rs ownership_certificate`
   — `loop_carried_slots()` pre-scans `SetLocal` feeders inside `LoopStart`…`LoopEnd`; the slot folds to
   ONE stream `i(id)m` (Alloc/Call feeder `i` routed to the slot, `(`/`)` around the loop body). Unit
   tests: `loop_carried_accumulator_folds_to_one_slot_stream` (`i(id)m`, verify_ownership Ok), leaky body
   `i(i)m` rejected. The PROVEN extracted checker ACCEPTs the emitted `i(id)m` (verified via `./checker
   ownership`). corpus-wall (14564 objs) green — backward-compatible. (2 pre-existing render_wasm json
   wasm-exec failures are unrelated — confirmed by stashing only these two files; another agent's list-cap work.)
3. **Lowering** (`lower/mod.rs`): emit the heap-loop-carried accumulator MIR — the append-accumulator TCO.
   Touch points: (a) `try_tco_rewrite` line ~2184 — drop the `carried[i] && is_heap_ty` bail WHEN the
   carried heap arg's every self-call value is `acc + [x]` (`BinOp::ConcatList` with `left = Var(acc)`);
   (b) `tco_rewrite` already emits that carried arg as `Assign { acc, acc + [x] }` (no change); (c) the
   in-loop `Assign` lowering (mod.rs ~690, currently `Err` on heap reassign) — admit `acc = acc + [x]` →
   `new = __list_concat(acc, [x]); Drop acc; SetLocal acc, new` (`try_lower_concat_list`; the cert-backed
   slot now accepted by step 2). Plus mutual-recursion inlining (flow_step→flow_rec).

   **DESIGN RESOLVED — approach 3 (fresh-slot upfront-copy), the cleanest, NO convention change.** The
   crux: a clean `i(id)m` needs the slot OWNED with `of[slot] = slot` (the cert keys the slot stream by its
   ValueId). A borrowed `acc` param has no `i` (cert `(id)m` → correctly REJECTED; dropping it iter-1
   double-frees the caller). Rebinding the param via Dup/owned-param makes `of[slot] ≠ slot` (the param
   object diverges from the slot key) → messy cert. THE FIX: introduce a FRESH slot var `acc_slot` and init
   it `acc_slot = __list_concat(acc, [])` (an owned copy). A Call heap-result sets `of[acc_slot] = acc_slot`
   AUTOMATICALLY (cert `i`), so the slot key == its object — clean. Then substitute `acc → acc_slot`
   throughout the loop body + bases; the loop carries `acc_slot` (drop-old/alloc-new), bases return it
   (move out). cert = `i(id)m`, EXACTLY what step 2 accepts. The borrowed param `acc` stays borrowed
   (caller owns it) — read only for the upfront copy. byte-match holds (the copy + per-iter append builds
   the identical final list as v0's recursion). Implementation pieces: (i) detect heap append accumulator
   (carried[ai] heap + every self-call value `ConcatList{left:Var(acc)}`); (ii) an IR var-substitution
   helper (Var(acc)→Var(acc_slot)); (iii) emit the upfront `let acc_slot = acc + []` bind; (iv) the in-loop
   Assign wiring (c). `try_lower_concat_list` is SCALAR-element only → a synthetic `List[Int]` append
   validates the MECHANISM first; yaml's `List[Value]`/`List[(k,v)]` then need heap-element concat (+
   value.object/stringify).
4. **✅ DONE 2026-06-20 (commit f3ce5401): the append-accumulator TCO PRODUCER.** `try_tco_rewrite` now
   detects a heap carried param whose every self-call value is `acc + [x]`, introduces a fresh OWNED slot
   (`let slot = acc + []`, substitutes `acc → slot`), and the in-loop `Assign` lowers `slot = slot + [x]`
   to `new = __list_concat(slot,[x]); Drop slot; SetLocal slot,new`. End-to-end VERIFIED on
   `spec/wasm_cross/append_accumulator.almd` (List[Int]): in-profile (was walled), ownership cert `i(id)m`
   ×2, **byte-matches v0** (output-parity baseline, match 69→70), corpus-wall green, cargo-test clean
   (the 2 json wasm fails are another agent's pre-existing). The rendered loop emits the per-iteration
   `rc_dec(old)` (frees confirmed in the wat). MEMORY NOTE: `__list_concat` COPIES (O(n²) like v0 deep
   recursion); large n OOBs on wasm's 64KB at n≈110 (sum(1..n)·8B) — an allocator-reclamation/efficiency
   limit, NOT an rc-leak (the cert PROVES rc-balance; the frees are emitted). A future in-place push makes
   it O(n). Fixture n kept small.

**🎯 THE ENTIRE OPTION-C CHAIN NOW WORKS END-TO-END** (proof → extracted checker → cert serializer →
verify_ownership → producer lowering), all verified: commits 7f673b4c, c05fc209, 291a1f35, f3ce5401.
A heap-loop-carried append accumulator compiles from `.almd`, lowers on the v1 trust spine, carries the
PROVEN `i(id)m` cert, and byte-matches v0. The completeness hole is closed AT THE ROOT for scalar-element
append accumulators.

## Remaining toward yaml=0 (the producer EXTENSIONS — the chain is proven, these widen its element domain)

- **✅ DONE 2026-06-20 (commit 7074579d): heap-element concat.** `__list_concat_rc` (self-host, rc_inc per
  element via the whitelisted `__lc_copy_rc`) + `try_lower_concat_list` admits String/Value elements +
  marks `heap_elem_lists`/`value_elem_lists` (so `drop_op_for` = DropListStr/DropListValue) + the gate
  `count_ir_calls` counts the heap-element ConcatList (mir≤ir holds). VERIFIED on
  `spec/wasm_cross/append_accumulator_heap.almd` (`List[String]` build_s + extend_s): byte-matches v0,
  corpus-wall green (cleared 2 spec walls 866→864), output-parity 70→71, cargo-test clean. So `acc + [x]`
  now lowers for SCALAR (Int/…) AND HEAP (String/Value) element accumulators on the proven `i(id)m` slot.
- **MUTUAL-RECURSION INLINING — PROTOTYPED + a KEY FINDING (2026-06-20, reverted, not committed).** All 11
  yaml walls are "heap-result if/match" because every append fn is MUTUAL-recursive (`flow_rec↔flow_step`,
  `collect_seq↔seq_item`, `collect_map↔map_entry`, `collect_block↔block_line↔block_nonblank`), so
  `try_tco_rewrite` (self-call detector) never fires. A prototype `inline_mutual_tail_recursion` (inline the
  single-call sibling G into caller F via `substitute_var_in_expr` per param + drop G; an `IrMutVisitor`
  rebuild) + the detection relaxation (a self-call passes `acc` OR `acc+[x]`) + the `tco_rewrite`
  identity-assign skip — VERIFIED on a synthetic `frec⇄fstep` (List[String], byte-matches v0). On yaml it
  took 11→9 BUT **regressed `esc_rec` + `collect_block` (in-profile → walled)**: inlining makes F
  self-recursive → the TCO FIRES → and TCO then WALLS a fn that lowered fine WITHOUT the TCO. ② forbids that
  incompleteness regression, so it was reverted.
  **✅ DONE 2026-06-20 (commit 8c9a5c07): the GUARDED mutual-recursion inline.** `inline_mutual_tail_recursion`
  (lower/mod.rs, threaded `globals`+`record_layouts`): inlines a single-call mutual sibling G into caller F
  (`IrMutVisitor` + `substitute_var_in_expr` per param) + drops G, **ONLY when F currently WALLS and the
  inlined F then LOWERS** (try-lower both) — no regression by construction. + detection relax (a self-call
  passes `acc` OR `acc+[x]`) + `tco_rewrite` identity-assign skip. Wired into render_program + classify_corpus.
  VERIFIED: `spec/wasm_cross/mutual_append.almd` (`frec⇄fstep`, List[String]) byte-matches v0; **cleared 6
  spec corpus walls (in-profile 3712→3718)**; corpus-wall green; cargo-test clean; yaml UNCHANGED at 11 (no
  regression — esc_rec/collect_block stay in-profile, the guard refused to touch them).

  **✅ DONE 2026-06-20 (commit f6199af9): `[call_result]` element materialization + the off-by-one guard →
  yaml 11→9.** `try_lower_str_list_literal` now admits a STRING-returning Module/Named CALL element
  (`[string.slice(s,0,1)]`) for `elem_str` (not just Value-call for elem_value): it lowers the call to a
  fresh owned String (the registered `string.slice` runtime — `lower_pure_module_value_call` already
  handles general module calls, not value-only) MOVED into the slot. Byte-verified:
  `spec/wasm_cross/list_call_element.almd` (`xs + [string.slice(s,0,1)]`) matches v0.
  **🚨 + a SILENT-MISCOMPILE found & fixed:** a `[string.slice]` element revealed that the TCO assigns
  carried params SEQUENTIALLY, so `acc + [string.slice(s, i, …)]` reading the loop index `i` (reassigned
  `i=i+1`) saw the NEW `i` → off-by-one (`chars("abc")` → `b-c-` not `a-b-c`). FIXED by WALLING
  cross-dependent TCO (a self-call arg reading another carried param) in `try_tco_rewrite` — ②-safe (walls,
  never miscompiles); the common case (each arg reads only its own param) is unaffected. yaml 11→9:
  flow_step + one more now lower correctly; the cross-dep fns (flow_rec, chars) wall instead of miscompiling.

  **✅ DONE 2026-06-20 (commit 89664c68): SIMULTANEOUS-UPDATE TCO.** `tco_rewrite` stages each carried
  SCALAR's new value in a fresh temp (reading OLD params), runs the HEAP append assigns (still-old scalars),
  then commits the temps — so a cross-dependent append (`acc + [string.slice(s, i, …)]` reading the loop
  index, `flow_rec`'s `acc + [slice(s, start, pos)]`) is no longer off-by-one. Byte-verified:
  `spec/wasm_cross/cross_dep_accumulator.almd` (chars `a-b-c`, win `ab|bc|cd`). The cross-dep wall is now
  narrowed to (a) heap-acc reads another heap-acc, (b) a PURE-VAR alias arg (`start = pos`, which a copy
  temp can't stage) — both rare, walled ②-safely. +3 corpus walls (in-profile 3719→3722); output-parity OK.

  - **✅ flow_rec base DONE (commit af2a5695): ConcatList arm in `lower_heap_result_arm`.** The
    heap-result-if return `if string.is_empty(last) then acc else acc + [last]` (a Var move-out arm + a
    ConcatList arm) now lowers (the `"im"` per-arm balance). flow_rec lowers END-TO-END → yaml 9→8.
    Byte-verified `spec/wasm_cross/heap_result_if_append.almd`; +3 corpus walls (3722→3725).

  **⚠ REMAINING (yaml 8 — the Value-PARSER core; each fn stacks MULTIPLE gaps, not one lever):**
  `collect_seq`/`seq_item`, `collect_map`/`map_entry`, `block_*`, `parse_lines`/`parse_nested`. Analysis of
  `collect_seq` (representative) — it returns `(Value, Int)` and:
  - **tuple-return**: the base is `(value.array(items), pos)` — a HEAP-result TUPLE return (Value + Int),
    not a bare List. Needs heap-result tuple-return lowering (the Value built from the accumulator at the base).
  - **value.array-at-base** (DONE as an op) folded into the tuple.
  - **mutual + extra callee**: `seq_item` is the mutual sibling BUT also calls `dash_item` — the guarded
    inline still applies (seq_item called only by collect_seq), but the inlined body keeps the dash_item call.
  - **tuple-destructure of an effect call**: `let (val, next) = dash_item(...)!` — bind a `(Value, Int)`
    from an effect-fn Result, then append `items + [val]`.
  - **effect fn** (`!` Result propagation) returning a tuple.
  So `collect_seq` needs heap-result-tuple-return + effect-tuple-destructure (+ the append/inline/TCO that
  are DONE). `collect_map` adds **value.object** + `List[(String,Value)]` (tuple-element) append; `block_*`
  add **tuple-heap drop**; `parse_*` are heap-result match roots. These are several substantial bricks
  (the Value-parser machinery), not a single lever — the append-accumulator foundation is complete; the
  remainder is value-aggregate construction + tuple plumbing.

  **🎯 ROOT-BLOCKER IDENTIFIED (2026-06-20, by synthetic probe): the effect-fn `!` early-return propagation.**
  The remaining 8 split into TWO sub-clusters by their KEYSTONE:
  - **6 effect fns** (collect_seq, seq_item, collect_map, map_entry, parse_lines, parse_nested) — ALL are
    `effect fn` and bind `let (val,next) = dash_item(...)!` / tail `parse_nested(...)!`. A synthetic
    `let (v,next) = make(n)!` (make an `effect fn -> (Value,Int)`) walls with **"unwrap `!` in a
    call-argument position cannot be faithfully computed (needs EARLY-RETURN propagation)"**. So the
    keystone for ALL 6 is `!` propagation = the v1 MIR Result/error machinery: lower `f()!` as
    `match f() { Ok(v) => <continue>, Err(e) => return Err(e) }` (an early-return on Err). This is a
    FUNDAMENTAL feature (Result repr + mid-function early-return / its desugar), not a per-fn brick —
    once it lands, the 6 effect fns' tuple-destructure + tuple-return (both already supported) compose.
  - **block_scalar/block_line + collect_block** (non-effect `local fn`, NO `!`) — keystone is the 3-CYCLE
    iterative inline (collect_block↔block_line↔block_nonblank, not a pair) + `(List[String],Int)`
    tuple-return + tuple-heap drop. Independent of the effect `!` work.
  THE next lever = effect-fn `!` early-return propagation (unblocks 6 of 8). Soundness-critical (a wrong
  Err-propagation = a silent miscompile), so it must land with the early-return desugar + byte-match, not
  a Const/Opaque shortcut (which the wall explicitly rejects today).

  **CORRECTION (2026-06-20, deeper probe): the cluster MOSTLY LOWERS — only 8 of ~74 wall, and their
  blockers are HETEROGENEOUS (no single keystone).** The dispatchers (dash_item, dash_after,
  nested_dispatch, …) are in-profile: a TAIL `f()!` in an effect fn is a PASS-THROUGH (the Result is
  returned as-is → just `f()`), already handled. Only these 8 wall, each on a DIFFERENT feature:
  - **collect_seq, seq_item** — a LET-BIND `!`: `let (val,next) = dash_item(...)!; <rest>` needs the
    EARLY-RETURN desugar `match dash_item(...) { ok((val,next)) => <rest>, err(e) => err(e) }` (Ok-arm
    continuation + tuple-payload destructure + Err-propagation). [the closest 2 — append/inline/TCO/tuple
    all done; ONLY the let-bind `!` remains]
  - **collect_map, map_entry** — let-bind `!` + **value.object** (build a Value object from `List[(String,Value)]`).
  - **parse_lines, parse_nested** — `lines |> … |> list.find((e) => not is_blank(e.1))` (list.find + a
    LAMBDA + pipeline) + `match next { some((offset,line)) => … }` (Option-of-TUPLE match payload).
  - **block_scalar, block_line** (+ collect_block) — `(List[String],Int)` **tuple-heap drop** + the
    3-CYCLE inline (collect_block↔block_line↔block_nonblank).
  So the path is several DISTINCT bricks (let-bind-`!` early-return ⇒ collect_seq/seq_item first; then
  value.object, list.find+lambda+Option-tuple-match, tuple-heap+3-cycle) — each soundness-sensitive,
  each its own byte-match. NOT one lever. The append-accumulator + option-C foundation is complete.

  **✅ block_scalar DONE 2026-06-20 (commit b31096e8): yaml 8→7 — and the blocker was NOT what was
  scoped above.** block_scalar's actual wall was the RETURN `(value.str(if string.ends_with(ind,"-") then
  joined else joined+"\n"), end)`: a heap-result `if` nested inside `value.str(..)` inside a TUPLE element,
  preceded by `let joined = if…` (two heap let-bound ifs) — NOT tuple-heap-drop, NOT the 3-cycle. Three
  composing fixes: (1) `extract_first_callarg_branch` recurses into TUPLE elements (ANF-lifts the
  `value.str(if…)` arg); (2) the bounded-duplication gate allows ≤2 remaining branch binds (was: refuse
  ANY); (3) `desugar_heap_branches` recurses INTO if/match arms + block tails (`desugar_nested_branch_arms`)
  so a duplicated arm's nested let-bound if resolves — all in the SHARED desugar (lower == count, no
  mir>ir breach). **Two PRE-EXISTING silent miscompiles this exposed in control_flow (C-044) were also
  fixed:** (a) `x |> (n) => body` was desugared to a Computed-callee call v1 MIR mis-lowered to 0 — the
  frontend now INLINES it to `{let n=x; body}` (`lower_pipe`); (b) a BLOCK-valued scalar bind
  `let a = {…; tail}` also mis-lowered to 0 — `lower_bind` now runs the block's stmts then binds the tail.
  Verified: corpus-wall in-profile 3725→3733 (+8), ownership 14984 ACCEPT, cargo-test 466, output-parity
  no baseline regression + control_flow NEWLY wasm-byte-matches (fixtures heap_result_tuple_return,
  pipe_lambda_block_value). (A pipe-lambda in a CALL-ARG position now WALLS, not mis-lowers — safe; ANF-lift
  it later.) **REMAINING yaml 7: block_line (block_scalar's sibling, likely closest), collect_map,
  collect_seq, map_entry, parse_lines, parse_nested, seq_item** — the 6 effect fns still need the
  effect-monad let-bind `!` (⛔ note above) + value.object.

  **✅✅ block_line DONE (commit 5518fff3): yaml 7→6, byte-matches v0.** After SIX turns of ②-disciplined
  bisection (the long note below — substitution / borrowed-param / fresh-let-inline / naive-id-reuse all
  TESTED and DISPROVEN), the wat showed the else-arm's `string.drop(line, 0)` reading `$v19` (the THEN
  arm's `__list_concat` result) instead of `line`. ROOT: `desugar_callarg_heap_if`'s bare-call-arg lift
  sized its fresh `tmp` with `max_var_id(THIS arm)` — but the arm omits `line` (used only in the SIBLING
  else arm), so `tmp` aliased `line` and the renderer's global VarId→local map collided them. FIX: thread
  a FUNCTION-WIDE `next_var` counter through `desugar_heap_branches`/`desugar_callarg_heap_if`/
  `desugar_nested_branch_arms` (a `desugar_heap_branches_inner(body, &mut u32)`; the public wrapper seeds
  `max_var_id(whole_body)+1`). Verified: spec/wasm_cross/block_line_collect.almd byte-matches (`a||bb|c`),
  corpus-wall in-profile 3733→3734 + ownership 14988 ACCEPT, cargo-test 466, output-parity no regression +
  NEW match, full worktree scan = only the 3-4 PRE-EXISTING mismatches (string_ops/fan_map/nested_named/
  list_string, all confirmed at HEAD~1). **The diagnosis discipline mattered: corpus-wall (ownership) AND
  the wall-count BOTH accepted the buggy lower; only byte-match caught it — kept ② across 6 turns.**

  **(historical diagnosis — kept for the method)**
  **⚠ block_line — a CAUGHT ②-trap (2026-06-20): metric-lowerable but RUNTIME-MISCOMPILES, kept WALLED.**
  block_line's body is `if is_blank then collect_block(.., if list.is_empty(acc) then acc else acc+[""])
  else block_nonblank(..)` — a heap-result `if` whose then-arm CALL carries a call-arg heap `if`. A
  `desugar_callarg_heap_if` extension lifting a BARE call/tuple body (so `desugar_nested_branch_arms`
  reaches the per-arm call) DID drop block_line's wall (yaml 7→6) — BUT it then let the guarded
  mutual-inline fold collect_block↔block_line into a TCO whose append-accumulator silently mis-lowered
  EVERY element to "" (`["a","","bb","c"]` → `["","","",""]`, a byte-MISMATCH, NOT a wall). The
  mutual-inline guard only checks that inlined-F LOWERS, not that it byte-matches, so a lowerable-but-
  wrong inline slips through. REVERTED (mod.rs note) per ②: a fake wall-count drop that ships a
  miscompile is worse than an honest wall. The REAL blocker is the collect_block↔block_line TCO append
  reading every element as "" — reproduced by the 2-cycle `collect_block`/`block_line` synthetic; fix
  THAT first, THEN the call-arg lift is safe.

  **🎯 ROOT PINNED (2026-06-20, bisection with the lift temporarily re-enabled): it is the mutual-inline's
  `substitute_var_in_expr` producing a LIST ELEMENT that fails to materialize → "".** On the 2-cycle
  `collect_block`/`block_line` synthetic: a CONSTANT element directly in block_line's body (`acc + ["Z"]`)
  byte-MATCHES (`Z|E|Z`) — TCO + slot + inline machinery are correct. But `acc + [line]` where `line` is
  block_line's PARAM, with collect_block passing even a constant `"X"`, gives "" for EVERY element
  (`X|X|X` → `||`). And `["a"] + [line]` / `[x,y]` with `line`/`x`/`y` as ordinary LOCALS byte-MATCH.
  **⚠ CORRECTION (2026-06-20, the substitution hypothesis was TESTED and DISPROVEN).** Replacing
  `inline_sibling_calls`'s `substitute_var_in_expr` with a `let fresh = arg;` + rename-param-to-`fresh`
  inline (so list elements become LOCAL-var refs `[Var(fresh)]`) did NOT fix it — block_line still emitted
  every element as "" (and flow_rec/chars stayed byte-correct, so the let-inline is regression-free but
  not the cure). Reverted. Deeper bisection: with `acc + [string.drop(line, 0)]`, even the call-element
  is "" — so it is NOT element materialization. The fresh `let line = list.get(lines, pos) ?? ""` itself
  reads EMPTY *inside the TCO loop*: `list.get(lines, pos)` on the BORROWED LIST param `lines` returns
  nothing. chars works because its loop reads a borrowed STRING param (`string.slice(s, …)`); a borrowed
  LIST param read in a mutual-inline→TCO loop comes back empty — the param is dropped/zeroed before the
  loop body reads it, or the loop fails to carry it.

  **🎯🎯 TRUE ROOT, wat-CONFIRMED (2026-06-20): a VarId COLLISION in the bare-call-arg lift + desugar
  duplication.** Dumped the wat for the yaml-faithful `line_at`-helper synthetic (lift re-enabled):
  ```
  (local.set $v15 (call $line_at  (local.get $v0) (local.get $v1)))   ;; line = line_at(lines,pos)
  (local.set $v19 (call $__list_concat_rc (local.get $v2) (local.get $v13)))   ;; THEN arm: acc + [""]
  (local.set $v28 (call $string.drop (local.get $v19) (i64.const 0)))   ;; ELSE arm: string.drop(line,0)
  ```
  The ELSE arm's `string.drop(line, 0)` reads **`$v19` — the THEN arm's `__list_concat` result (a LIST)** —
  instead of `$v15` (line_at's String). So `line`'s VarId aliases the then-arm's concat slot: the lift
  (`tmp = max_var_id(body)+1`) + the tail-duplication (`desugar_let_bound_heap_branch` clones the
  continuation into BOTH arms) + the bounded-dup relaxation reuse a VarId across the two arms, and the
  global VarId→wasm-local map collides them — string.drop runs on a list pointer → garbage/"". NOT
  substitution, NOT borrowed-param (both DISPROVEN above); both earlier theories were red herrings.
  **THE FIX:** thread a single monotonic FRESH-VarId counter through `desugar_heap_branches` /
  `desugar_callarg_heap_if` / `desugar_let_bound_heap_branch` instead of recomputing `max_var_id(body)+1`
  per call (which collides once a prior rewrite has already consumed ids), so every lifted `tmp` and every
  duplicated continuation gets globally-unique ids. THEN the bare-call-arg lift is sound → block_line
  lowers correctly → yaml 7→6. Do NOT re-add the lift before the fresh-id threading.

  **⚠ UPDATE: simple id-reuse is also RULED OUT.** `max_var_id` (mod.rs:2065) DOES count `IrStmtKind::Bind`
  vars (visit_stmt, line 2107) + Match pattern binds, so the lift's `let tmp` IS counted and the next
  `max_var_id+1` is higher — no naive id reuse. So the v19-not-v15 disconnect is NOT an IR-VarId clash; it
  is a `value_of` / materialization disconnect at the MIR-lowering layer: after the inline substitutes the
  CALL `line_at(lines,pos)` into BOTH block_line's cond (`string.is_empty(line)`) and its element
  (`string.drop(line, 0)`), the lowering materializes line_at once (`v15`) but the element's `string.drop`
  binds to `v19` (the sibling arm's concat). FOUR hypotheses now disproven (substitution, borrowed-param,
  fresh-let inline, naive id-reuse). NEXT: dump the MIR Op stream (not just wat) for the `line_at` synthetic
  and trace which Op sets the `string.drop` arg to v19 — the bug is in how the lift/desugar threads
  `value_of` for a call substituted into multiple positions. Needs a focused MIR-op-level session.

  **⚠ BLANKET let-bind-`!` STRIP is UNSOUND — a CAUGHT ②-trap (2026-06-20).** Stripping `let (a,b)=f()!`
  → `let (a,b)=f()` (the tail-`!` pass-through) in `lower_destructure`, plus seeing through `!` in
  `tco_collect`/`tco_rewrite`, DID clear seq_item→collect_seq→collect_map (yaml 6→3) — BUT the full
  v0/v1 spec scan caught it MISCOMPILING erroring fns: `safe_div_chain`, `grade_classify`, `sum_of_squares`,
  `closure_env_churn`, `map_entry_churn` all byte-MISMATCH (the strip drops a real `err(…)` those fns
  propagate). corpus-wall (ownership) PASSED — only byte-match caught it. REVERTED per ②.
  KEY DISTINCTION that makes a SOUND version possible: the **yaml parser cluster never returns `err(…)`**
  (grep-verified: its only `err("…")` are in the PURE int parsers `oct_rec`/`bin_rec`, handled by `match`,
  not by the effect `!`). So a NEVER-ERRS-SCOPED strip — strip the let-bind `!` ONLY when the callee
  provably never errs (a call-graph fixpoint: a fn can-err iff it has `err(…)` or `!`-calls a can-err fn;
  the yaml cluster has none) — would be SOUND and reach yaml=0, while leaving `safe_div` & co. walled.
  That per-callee analysis (threaded into the lowering) OR the full effect-monad (return-wrap) is the path;
  the BLANKET strip is permanently OUT.

  **✅✅ NEVER-ERRS-SCOPED STRIP DONE (commit b154a270): yaml 6→3.** Implemented the per-callee can-err
  analysis in `mod.rs`: `compute_can_err(fns)` seeds with `has_result_err` (body contains `IrExprKind::
  ResultErr`) and runs a `!`-propagation fixpoint (`unwrap_named_callees` = the `g` in `Unwrap{Call{Named
  g}}`; a fn can-err if it `!`-propagates a can-err fn). `strip_never_err_unwraps` then replaces
  `Unwrap{Call{Named g}}` → `Call{Named g}` ONLY for never-err `g` — run as the FIRST step of
  `inline_mutual_tail_recursion` (before the inline guard's try-lower, so inlined-F sees the bare calls and
  the append-TCO fires; `tco_collect` then needs no `!`-awareness). The yaml cluster is entirely never-err
  (no `ResultErr`; the only `err("…")` are PURE `oct_rec`/`bin_rec` reached by `match`, not `!`), so
  seq_item/collect_seq/collect_map all TCO and clear. VERIFIED ②: corpus-wall in-profile 3741→3758 (+17),
  ownership 15068 ACCEPT; full v0/v1 spec scan = only the PRE-EXISTING mismatches — `safe_div_chain`,
  `grade_classify`, `sum_of_squares`, `closure_env_churn`, `map_entry_churn` (the ones the BLANKET strip
  byte-mismatched) now MATCH because their `!` is can-err and is LEFT. cargo-test 466.

  **⚠ REMAINING yaml 3: map_entry, parse_lines, parse_nested** — distinct value-aggregate / match features
  (the strip+TCO foundation is done):
  - **parse_lines, parse_nested**: `… |> list.find((e) => not is_blank(e.1))` (list.find + a LAMBDA with a
    tuple-index `.1`) THEN `match opt { none => …, some((idx, line)) => … }` — an **Option match with a
    TUPLE payload** (`some((idx,line))`). try_lower_variant_value_match handles scalar/single-heap payloads;
    the tuple-payload destructure is the gap.
  - **map_entry**: `match find_colon(t) { none => (value.object(pairs), pos), some(cp) => { … pairs +
    [(key, val)] … } }` — Option-SCALAR match (cp) is fine, but the accumulator append `pairs + [(key,
    val)]` is a **List[(String,Value)] TUPLE-ELEMENT append** (value.object itself is proven — collect_map's
    base lowers). The tuple-element list append is the gap.

  **🔧 RECIPE for the Option-tuple-payload match (parse_lines/parse_nested).** Layout confirmed: a tuple is
  a `DynList`, element `i` at `layout::slot_offset(i)` (so `(idx,line)` = scalar @ slot_offset(0), String
  handle @ slot_offset(1)); the Option `Some` payload sits at the variant block's `@12` as the TUPLE handle.
  Extend `try_lower_variant_value_match` (control.rs:822): when a `Some`/`Ok` inner pattern is a `Tuple`,
  bind `@12` (the tuple handle) as a BORROW to a fresh `$p` (like `str_heap_bind`), then lower the arm with
  `let (idx,line) = $p` prepended (a `BindDestructure` over the tracked container — `lower_destructure`'s
  "tracked heap var aliases the container" path), and DROP the subject AFTER the arms (the `str_heap_bind`
  branch at control.rs:1037), because `parse_lines`/`parse_nested` only BORROW `line` (pass it to
  `dispatch`/`nested_dispatch`/`indent_of`/`string.trim`), never move it out — so the subject's
  drop-after frees the tuple + its String exactly once. Cleanest impl: a top-of-function desugar
  `some((idx,line)) => B` → `some($p) => { let (idx,line)=$p; B }` (fresh `$p` from `max_var_id+1`), then
  extend `heap_or_scalar_bind` to admit a heap TUPLE payload over an Option subject. Then yaml 3→1; map_entry's
  tuple-element append → 1→0.

  **✅ Option-tuple-match DONE (commit a89bda41): yaml 3→1** — parse_lines/parse_nested cleared via the
  variant-match tuple-payload bind + drop-after (corpus-wall ACCEPT, full-scan no new mismatch).

  **🎯 LAST WALL = map_entry (yaml 1), fully diagnosed (up to commit 55343d53).** Three nested blockers,
  found by bisecting a synthetic down to map_entry's exact shape (`match find_colon(t) { none =>
  (value.object(pairs), pos), some(cp) => { … cmap(…, pairs + [(key,val)]) } }`):
  1. ✅ **user-fn-Option subject** (`find_colon` is a `Named` fn, not self-host) — was untracked so the
     variant-match never fired. FIXED: track a `Named` call returning Option/Result as a materialized
     subject (same DynListStr len-as-tag repr). Verified by `ufo.almd` byte-match.
  2. ✅ **borrowed `pairs` used in BOTH arms** (`value.object(pairs)` in none, `pairs + [(k,v)]` in some) —
     the THEN arm's consume leaked into the ELSE arm's lowering view → ELSE walled. FIXED: snapshot/restore
     param_values+live+materialized_aggregates between the alternate arms (branch ownership isolation).
  3. ✅ **DONE (commit 95accd80): `pairs + [(key,val)]` (List[(String,Value)] tuple-element append). yaml
     1→0.** A new self-hosted recursive drop `$__drop_list_str_value` (value_core.almd: per tuple — rc_dec
     the String slot @12, `$__drop_value` the Value slot @20, then the tuple, then the list) behind a new
     `Op::DropListStrValue` (single cert `d`, trusted recursion like `DropListValue`); `try_lower_concat_list`
     + `try_lower_str_list_literal` admit the heap-field `(String,Value)` tuple element (via `try_lower_tuple_
     construct` + `__list_concat_rc`), tracked in `str_value_elem_lists`; the rc_dec allowlist + the
     example-side linker pull `$__drop_list_str_value`/`$__svdrop_list` in. corpus-wall ownership ACCEPT
     (in-profile 3758→3822, +64 — it also cleared 63 other corpus fns), byte-matches v0, cargo-test 466,
     full-scan no new mismatch. **⚠ FINDING: a pre-existing TCO-heap-loop leak — `let xs = [heap]; loop(...)`
     in a tail loop traps (freelist not reused) at ~N/objects-per-iter for List[Value] (proven) AND
     List[String] AND this new drop EQUALLY (List[Value] 1-elem traps ~2000, 3-elem ~1500, this 4-obj ~1000 —
     same per-object rate), so it is NOT this drop's bug but a separate freelist/TCO issue affecting every
     heap-allocating tail loop; worth a dedicated brick.**

  ~~3-OLD. REMAINING: `pairs + [(key,val)]` … CALL-ARG position.~~ (superseded — DONE above; kept for the
  diagnosis trail): `try_lower_concat_list` (calls.rs:534) admits only String/Value elements (line 548-552); a
     **heap-FIELD aggregate element (tuple/record with inner heap) DEFERS** — the call-arg path then WALLS
     (calls.rs:887, correct ②; the let-bind path silently defers it to an Opaque EMPTY list = a latent
     miscompile, NOT a real lowering — so this is genuinely unsolved, not a gating quirk). THE fix = the
     true "Camp-4 frontier": a **tuple-aware recursive list drop** (a `DropList` of `(String,Value)` tuples
     — iterate, masked-drop each tuple, freeing its inner String+Value), so `__list_concat_rc` can rc-own a
     tuple element and the result reclaims correctly. A NEW runtime drop (List[tuple]), distinct from
     DropListStr/DropListValue. With it, map_entry lowers → yaml=0. (Isolated repro: `/tmp/eff2.almd`,
     `/tmp/eff5.almd`; the let-bind defer is `/tmp/sv.almd`.)

  **⚠ CORRECTION (the destructure-desugar route is BLOCKED — tested):** `let (idx, line) = pair` over a tuple
  VAR/param byte-WALLS on its own (`/tmp/td.almd`: v0 `7:hi`, v1 WALLS) — `lower_destructure`'s shapes are a
  tuple LITERAL value or a tracked container, neither covers a plain tuple var → scalar+heap split. (cs's
  `let (v,n) = cs(...)` lowered because the RHS is a fresh call-result tuple, a different shape.) So the
  Option-tuple-match canNOT desugar to `some($p) => { let (idx,line)=$p; … }`. It must bind idx/line DIRECTLY
  inside the variant-match's `bind_payload`: load `@12` (the tuple handle), then `idx = load(handle +
  slot_offset(0))` (scalar copy) and `line = load(handle + slot_offset(1))` (a heap-handle BORROW), and drop
  the subject AFTER the arms. That means restructuring the per-arm bind from `Option<(VarId,bool)>` to a
  multi-bind (single OR tuple) — an intricate, ownership-critical change (a wrong drop-after = UAF), but the
  cert + byte-match gates catch any error. THE remaining work: this multi-bind restructure (parse_lines/
  parse_nested) + the List[(String,Value)] tuple-element append (map_entry).
  (Also this turn: commit 75c9100e had accidentally dropped the block_line fresh-VarId fix from mod.rs via
  a stale working tree — recovered via `git checkout 5518fff3 -- mod.rs`, re-verified block_line byte-match;
  yaml back to 6. corpus-wall green, in-profile 3741, ownership 15035 ACCEPT.)

  **🔧 CONCRETE RECIPE for the let-bind `!` (2026-06-20, the Result repr is now confirmed).** v1 MIR
  represents an effect-fn `Result[T,String]` as a DynListStr with a LEN-AS-TAG (see
  `materialize_result_ok`, control.rs:2030): `len @ handle+4` is `0` for Ok / `≠0` for Err; the Ok payload
  (a scalar, or a TUPLE/heap HANDLE) sits at `handle+12`. The tail `f()!` already passes the Result through
  (`lower_tail(expr)`, tail.rs:256/624) — sound because the tail value IS the fn's return. The LET-BIND
  `let pat = f()!; rest` (binds.rs:235, walled) desugars to a heap-result `if` — NO new variant-match
  extension needed:
  ```
  let r = f()                                  // Result (DynListStr); track in materialized_results_str
  if <load(r+4) != 0> then r                   // Err: move the Result out as-is (Dup+Consume, the Var arm)
  else { let pat = <load(r+12)>; rest }         // Ok: extract the payload @ +12, then the continuation
  ```
  The Ok payload extraction + ownership is EXACTLY the existing `value.as_array` str-result path
  (control.rs:907-916: bind the @12 handle as a BORROW, drop the Result wrapper after) — for a TUPLE
  payload, follow the bind with a tuple-destructure of the @12 handle (read .0/.1). Both arms produce the
  fn's `Result`, so the existing heap-result-`if` machinery (incl. the ConcatList/Call/Block arms just
  added) lowers it. HARDEST integration = collect_seq, where this `!` sits INSIDE the TCO loop body, so the
  Err early-return becomes a loop-carried `if` (the then-arm `return r` is a break-with-value) — do the
  ISOLATED non-TCO `let x = mk(n)!` synthetic FIRST (byte-match), then the TCO integration. Start there.

  **⛔ DEFINITIVE (2026-06-20, the recipe above has an unmet PREREQUISITE — code-confirmed).** v1 MIR
  does NOT wrap a user effect fn's return in the DynListStr Result repr: `lower_body_with_globals`
  (mod.rs:180) returns `lower_body_into(body)` verbatim as `ret` — no Ok-wrap. So a user effect fn returns
  a BARE value (`(Value,Int)`), with NO tag. The tail `f()!` strips soundly ONLY because g ≡ f() at the
  Result level (tail.rs:253, an IDENTITY — g returns exactly f()'s value, Ok or Err). But the LET-BIND
  `let x = f()!; rest` is g ≠ f() (f() THEN rest), so a naive strip runs `rest` with a garbage x on the
  Err path = a SILENT MISCOMPILE — which is why binds.rs:235 deliberately WALLS it (NOT strips it). A
  correct let-bind `!` needs a real early-return, which needs a runtime Result TAG to branch on — but the
  bare-value model has none. So the prerequisite is to BUILD the v1 effect-Result discipline: wrap every
  effect-fn return in the Result repr (materialize_result_ok / an Err ctor) AND make `!`/`?` tag-aware at
  EVERY call site (the tail `!` would change from identity-strip to a tag-check pass-through). That is a
  MAJOR cross-cutting subsystem (every effect fn + every effect call), not the single desugar the recipe
  assumed. ②: a naive strip is OUT (Err-path miscompile). So the 6 effect-fn walls hinge on building the
  effect-monad first; the 2 block_* walls (non-effect) are independent (3-cycle inline + tuple-heap) and
  are the more tractable next target if avoiding the effect-monad subsystem.
  (Append concat — scalar + String/Value heap — guarded mutual-inline, call-element materialization,
  simultaneous-update TCO, and the heap-result-if append base are DONE + verified; off-by-one classes GUARDED.)

After C lands end-to-end: the 11 walls fall (with value.object/stringify + tuple-heap for the Value-parser
subset), driving yaml → 0 — on a PROVEN spine, the v1 completeness ideal.

## ORG wall=0 — the remaining 6 non-native walls, precisely scoped (2026-06-27)

After the cross-module + str-acc/defunc + ReadDir + correctness-sweep campaigns, the org wall surface is
**12 repos at wall=0; 6 non-native walls + porta 29/sqlite 20 native-only**. The 6 split into TWO mechanisms,
both verified by reading the actual `.almd` + the wall site:

### Mechanism 1 — aes (2 walls: `cfb8_encrypt`, `cfb8_decrypt`) — REUSES the PROVEN Loop cert, NO new Coq
Shape (`aes/src/mod.almd:168-193`): `var iv = state.iv` (a `var` bound to a BORROWED heap record FIELD),
then in `for i in 0..len { … iv = bytes.concat(bytes.slice(iv,1,16), …) }` (reassigned to a FRESH OWNED
`bytes` each iter), then moved out into the result record (`iv: iv`). This is EXACTLY the proven
loop-carried slot `[Inc; Loop[FDec;FInc]; MoveOut]` (OwnershipLoop.v) — net-0 per iteration (drop-old +
acquire-new), move out the final.
- **Wall site**: `lower/tail.rs:48-64` deliberately WALLS a loop-reassigned (`loop_reassigned_vars`)
  mutable heap-FIELD var. The non-loop sibling (`:65-78`) already owned-`Dup`s a mutable field var.
- **The cert machinery ALREADY handles it**: `certificate.rs loop_carried_slots` registers ANY heap-result
  Call SetLocal'd inside `LoopStart…LoopEnd` as a slot — `bytes.concat` (heap-result) → `iv` slot, loop
  `(id)`, move-out `m`. The blocker is ONLY that the wall stops lowering before the slot machinery runs.
- **The fix (approach-3, mirrors the append-accumulator)**: the slot's INIT must be a clean `i`
  (Alloc/heap-result-Call), NOT a `Dup` (which emits cert `a` with `of[slot]≠slot`). So replace the wall
  with: emit `var iv`'s init as an OWNED heap-result COPY of the field (a `bytes` clone call →
  `loop_carried_slots` sees its `i` → routes into the slot), then the loop SetLocal + the move-out fold to
  `i(id)m` — the PROVEN cert. **Gate: aes ships NIST FIPS-197 test vectors** (`mod.almd:200+`) — a
  byte-match oracle for free; corpus-wall ownership ACCEPT catches any cert error. aes 2→0, wall 6→4.

### Mechanism 2 — filter/filter_map (4 walls: wasm-bindgen generate_wit/esm/dts, dojo backfill_dir) — NEEDS a NEW Coq construct
Shape: `types |> list.filter((t) => list.contains(used_names, get_str(t,"name")))` (a CAPTURING closure —
captures `used_names`); dojo's `list.filter_map((f) => match fs.read_text(dir+"/"+f) {…})` ALSO captures
`dir` AND is EFFECTFUL. Walled by the campaign's value-position HOF honesty guard (`calls.rs`,
`last_call_had_unlifted_closure`).
- **Why it's the genuine Coq frontier** (empirically confirmed: an agent's C1-inline made it byte-match but
  corpus-wall REJECTed): filter's per-element acquire is **CONDITIONAL** — `if pred then {Inc x; append to
  out}`. The output list `out` accumulates a RUNTIME-VARIABLE number of clones (k = #trues), balanced not
  per-iteration but by `out`'s bulk DropList at the end (k Decs). The current OwnershipLoop Loop rule
  requires the body PRESERVE rc EXACTLY (net-0) — a conditional +k does not. So it REJECTS a SAFE program:
  a NEW completeness hole, one level beyond the net-0 accumulator.
- **The needed extension**: a Coq construct for a **conditional-acquire-into-accumulator + bulk-drain** — a
  loop body that conditionally raises an accumulator's element-count (monotone, non-faulting), balanced by a
  final `Drain` that releases all. Soundness: for ANY trues-count k, (k conditional Incs into out) =
  (len out) = (k Decs by DropList out) — balanced regardless of k. Then extract to the OCaml checker, emit
  the cert for the C1-inlined capturing filter, and route the lowering. dojo's filter_map ALSO needs the
  effect-monad `!` (the #22 / let-bind-`!` frontier) since its closure is effectful. This is the real,
  irreducible `#31` Coq work — multi-layer (Coq → extraction → Rust cert → lowering), soundness-critical.

**Plan toward wall=0**: (1) aes (proven-spine reuse, NIST-gated, tractable) → 6→4; (2) the conditional-acquire
OwnershipLoop construct (new Coq) → wasm-bindgen 3→0 → 4→1; (3) the effect-monad + conditional-acquire for
dojo's effectful filter_map → 1→0. Then the only walls left are porta/sqlite native-only (reclassify).

### ✅ STEP 1 DONE (commit 6a38227e): aes 2→0 (a routing gap, NOT Coq — see the aes section above).

### ✅ STEP 2a DONE (commit 5d3b642f): the CONDITIONAL-acquire Coq core — OwnershipFilter.v.
`CondLoop thenb elseb` + `ccheck_unroll_sound`: BOTH branches rc-preserving ⇒ any per-iteration predicate
outcome sequence is double-free/leak-free. Kernel-checked + coqchk + axiom-clean, in the proof gate
(PROOF SPINE OK). The filter slot cert `[CInc; CondLoop [FDec;FInc] []; CMove]` ACCEPTS; leaky/draining
branches REJECT; the unconditional Loop is the special case thenb=elseb. This is the irreducible Coq part.

### STEP 2b REMAINING (the integration — precisely diagnosed 2026-06-27): heap-filter inline + conditional-acquire cert/checker.
EMPIRICAL findings (probed each filter shape via render_program vs native):
- a NON-capturing heap filter (`xs |> list.filter((s) => string.len(s) > 1)`, List[String]) WORKS — via
  C2-LIFT (the self-host `(call $list.filter list funcref)` combinator, its own proven cert), NOT the inline.
- a SCALAR filter (`[1,2,3] |> filter((x) => x > 2)`) WORKS — via C1-INLINE (try_lower_defunc_list_hof):
  a write-cursor + conditional store of an i64 VALUE (no ownership ⇒ no element cert events; the existing
  checker already accepts this conditional store).
- a CAPTURING heap filter (ALL captures — scalar, string, list — wall) FAILS: C2-lift is impossible (a
  capture has no FuncRef), and C1-inline `try_lower_defunc_list_hof` DECLINES a heap filter at
  control_p5.rs:116 (`filter` not in the heap-source allowlist) + :138-139 (`filter` result must be
  non-heap). So it falls to the funcref-drop + the value-position guard (calls.rs:163) → WALL.
THE FIX (REFINED 2026-06-27 by probing each element type): `filter(pred) ≡ filter_map((x) => if pred(x)
then some(x) else none)` — and a CAPTURING `filter_map` is ALREADY lowered + corpus-wall-ACCEPTED by the
proven str-acc conditional-append path **for List[String]** (verified: `xs |> filter_map((s) => if
list.contains(names,s) then some(s) else none)` byte-matches `bb,ccc`; the conditional append/skip is the
already-accepted cert OwnershipFilter.v now proves sound). So the List[String] filter wall is a pure
filter→filter_map DESUGAR (a routing gap, no new checker code). BUT the wasm-bindgen walls are NOT
List[String] — they are **List[Value]** (`fns`/`types` from `get_arr` = `json.as_array`). A capturing
List[Value] `filter_map` ALSO WALLS (verified): the str-acc path's `result_str_acc` (control_p5:131-133)
matches ONLY `List[String]`, so a `List[Value]` filter_map declines → C2-lift fails on the capture → wall.

So the precise remaining work splits:
1. **filter→filter_map desugar** (List[String]) — a routing gap, verified-ready, clears any List[String]
   capturing filter (general capability; does NOT itself clear a wasm-bindgen wall).
2. **value-element capturing filter/filter_map inline** (the 3 wasm-bindgen walls) — generalize the
   str-acc conditional-append path from String elements to **Value elements** (`value_elem_lists` +
   `__list_concat_rc` for Values, both already exist for the unconditional value append per the heap-element
   concat brick above; extend to the CONDITIONAL filter_map append). The cert is the SAME conditional
   append the String path emits (OwnershipFilter.v's CondLoop, already corpus-wall-accepted for String) —
   so likely NO new checker code, a value-element routing extension. Gate: byte-match + corpus-wall ACCEPT.
3. **dojo filter_map** (1 wall) — its closure calls fs.read_text (effectful) → needs the effect-monad
   let-bind-`!` (the #22 frontier) ON TOP of the value/record-element conditional append. The hardest.
Gates throughout: byte-match (the capturing filters' real output) + corpus-wall ownership ACCEPT + 0
backend-split. The OwnershipFilter.v Coq core (committed) is the soundness foundation for all three.

### ✅ STEP 2b PARTIAL (commit 5a0a9efb): capturing heap filter LOWERED → wasm-bindgen 3→2.
The capturing heap `list.filter` (List[String] + List[Value]) now lowers via the write-cursor + a
LOCALLY-balanced conditional acquire: keep the source element by CLONING it (Dup `a`) + MOVING it into
the output list (Consume `m`) INSIDE the predicate-true then-arm. Because the `a..m` is balanced within
the then-arm (else does nothing; the output list is alloc'd once, NOT a SetLocal-rebound slot), the
EXISTING flat checker accepts it WITHOUT the loop-carried CondLoop — OwnershipFilter.v's CondLoop proves
the more general loop-carried form, this locally-balanced shape needs only the base checker. control_p5
lower_defunc_list_hof_inner: allow heap source+result for filter, body stays Bool, then-arm clones+consumes,
output tracked heap_elem_lists/value_elem_lists. byte-match List[String]+List[Value], corpus-wall 18795
ACCEPT, 0 backend-split. `generate_wit` fully clears.

### REMAINING (org non-native = 3): the deep frontiers.
- **generate_dts / generate_esm (wasm-bindgen 2)** — NOT a simple heap-result-if (every isolated shape —
  flat_map-conditional-call, let-bound big-list-if, MIDDLE-concat heap-result-if `A + throws_line + B`, a
  top-level let-bound-lambda HOF arg — ALL lower + byte-match, the last fixed in commit 9757d56a). The
  residual blocker is the `sigs = supported |> list.flat_map((f) => { … let param_ty = (p: Value) => {…};
  let params_str = params |> list.map((p) => "…${param_ty(p)}…") |> list.join(", "); … })` shape: a
  let-bound lambda DEFINED INSIDE a flat_map body and called by an INNER map's inline lambda (`param_ty(p)`)
  — a NESTED-defunc-body let-lambda. The top-level form now inlines (9757d56a, via lambda_bindings); the
  flat_map Block arm DOES lower inner stmts via lower_stmt (so `let param_ty` registers), yet the synthetic
  `fs |> flat_map((f) => { let tag = (p)=>p+":"+f; ["a","b"] |> map(tag) })` (tag captures the outer
  flat_map param `f`) still walls the OUTER flat_map — a deeper interaction between the outer str-acc body
  walk and an inner map(captured-let-lambda) whose capture is the outer HOF param. NARROWED FURTHER (2026-06-27) to a PRECISE position-dependent bug: `xs |> list.map(tag)` where
  `tag` is a let-bound lambda (`let tag = (p) => …`) LOWERS in TAIL/value position (the 9757d56a fix —
  n2/n1d byte-match) but WALLS in BIND position (`let parts = xs |> list.map(tag); parts` — n1e/n1f/n1h all
  wall "list.map with an unliftable/closure-list higher-order argument"), for BOTH heap (List[String]) and
  scalar (List[Int]) results, capturing or not. BOTH positions call the SAME
  lower_pure_module_value_call → try_lower_defunc_list_hof (calls.rs:89/139), with `tag` registered in
  lambda_bindings (binds_p2:280, unconditional) before the use — yet the bind position's
  try_lower_defunc_list_hof returns None (→ funcref-drop → last_call_had_unlifted_closure=true → the
  binds_p2:642 guard walls), while the tail's returns Some (the 211 clear). An INLINE-lambda bind
  (`let parts = xs |> map((p) => …)`) works in bind position — so it is SPECIFIC to the let-bound-lambda
  (Var) resolution differing by position. The defunc path does NOT read binding_var/binding_is_mutable, so
  the difference is subtler (IR args structure of `tag` in the bind RHS vs the tail, or a state set by
  lower_bind before calls.rs:609). NEXT: dump the MIR ops/args for n1d (works) vs n1e (walls) and diff the
  `tag` arg node + the lambda_bindings state at the try_lower_defunc_list_hof call — a focused IR-level
  session. Fixing blind risks masking a real unfaithful-HOF wall (a silent miscompile), so it must land
  with the ops-diff + byte-match, not a guess. generate_dts/esm's `sigs` hits exactly this (the inner
  `params |> list.map((p) => …${param_ty(p)}…)` where `param_ty` is a let lambda, AND the `let params_str =`
  bind of that map).
  **✅ BIND-vs-TAIL FIXED (commit 1dee4752): the let-bound-lambda HOF arg now lowers in BIND position too**
  (binds_p2's data_arg_has_fn misclassified a let-lambda Var as a fn-typed data arg → walled; now
  recognized as a closure arg). n1e/n1f/n1h byte-match. But generate_dts/esm STILL wall — the let-lambda was
  NOT their first blocker.
  **TRUE generate_esm BLOCKER (fully characterized 2026-06-27): a (List[String], List[String]) TUPLE-of-two-
  heap-lists accumulator fold** — `tuple_helpers`: `all_tuples |> list.fold(([], []), (state, ty) => { let
  (lines, seen) = state; let sig = …; if list.contains(seen, sig) then (lines, seen) else (lines +
  emit_tuple_helpers(ty), seen + [sig]) })` (the dedup pattern). The accumulator is a tuple of TWO heap
  Lists, and the body is a heap-result-`if` returning that tuple. The existing tuple-acc fold (#69) handles
  ONLY (List[T], Int) — one heap list + one scalar; TWO heap-list slots (each its own DropListStr, the
  conditional reusing one or building the other) is a substantial extension. generate_dts's `matrix_iface_block`
  + `sigs` are the analogous deep shapes. dojo's `backfill_dir` is the effect-monad (#22). These are
  genuinely deep, multi-feature lowerings (not single routing-gap levers), each its own fresh brick.
- **dojo backfill_dir (1)** — the capturing `filter_map` whose closure calls fs.read_text → needs the
  effect-monad let-bind-`!` (#22) on top of the value/record-element conditional append.
- **porta 29 + sqlite 20 native-only** — `@extern(rust)` host stubs, no wasm form (physically not
  lowerable; reclassify only — a user accounting decision, not a lowering task).

## The 5 real lowering-walls (user-approved 2026-06-27 "両方: 分離 + 5 実装") — precise diagnoses

The @extern(rust) reclassification (39 native-FFI host-imports → separate category) is DONE in
org-trust-status.md. The 5 real lowering walls to implement:

1. **wasm-bindgen generate_dts/esm (2)** — capturing list.filter/filter_map over List[Value]: needs the
   OwnershipFilter.v CondLoop integration (the conditional-acquire). OwnershipFilter.v (the Coq core) is
   proven + in the gate (5d3b642f); the capturing String/Value filter write-cursor clone+consume already
   lowers (5a0a9efb) — the residual is the (List,List) tuple-fold dedup + the nested-defunc shapes.
2. **dojo backfill_dir (1)** — capturing filter_map whose closure calls fs.read_text: the effect-monad
   let-bind-`!` (#22) + the value/record-element conditional append.
3. **porta proxy.start (1)** — heap-result `match` + `validate(c)!` (the effect-monad let-bind-`!`, #22).
4. **porta observability.format_tool_log (1)** — `"…${if success then "ok" else "error"}"`: a StringInterp
   with an embedded heap-result-`if` part. PRECISE DIAGNOSIS: the manual ConcatStr `("" + "a=") + (if c
   then "x" else "y")` LOWERS + byte-matches (the desugared form is fine), but the StringInterp PATH walls
   — try_lower_string_interp (calls_p2.rs:188-189) desugars to that exact tree then lowers it via
   `try_lower_concat_str` (→ lower_call_args, which declines the heap-result-if operand), whereas a manual
   ConcatStr return goes through lower_tail's ConcatStr handling (which materializes the if). SAME IR tree,
   DIFFERENT lowering entry. FIX: route try_lower_string_interp's desugared tree through the same
   if-materializing path lower_tail uses for a ConcatStr (not try_lower_concat_str), OR teach
   try_lower_concat_str / lower_call_args to materialize a heap-result-if operand. Needs a MIR-op dump of
   nest (works) vs sc2 (walls) to pin the exact entry difference. #60 (StringInterp desugar) territory.

#22 (effect-monad let-bind-`!`) is the keystone for walls 2 + 3. The OwnershipLoop conditional-acquire +
the (List,List) tuple-fold is the keystone for wall 1. Wall 4 is the StringInterp-if path fix above.

## porta.start / dojo effect-monad #22 — PRECISE statement-`!` finding (2026-06-27)

Traced the porta.start keystone to the STATEMENT-position effect-`!` over a CAN-ERR real-Result callee
(`validate(c)!` where `fn validate(c) -> Result[Unit,String]` constructs ok/err):
- `lower_effect_call` (calls.rs:438, the statement path from lower_stmt:576) STRIPS Try/Unwrap (line 10)
  and lowers the inner call. This strip is a TAIL-only shortcut (sound when the call IS the fn's return).
- For a MID-BODY statement `validate(c)!`: the stripped `validate(c)` produces a heap Result handle that is
  (a) left on the stack undropped → INVALID-WAT (verified: `validate(n)!; ok(42)` → "type mismatch at end
  of function, expected [] but got [i32]"), or (b) if dropped, LOSES the err-propagation → silent
  miscompile. So a sound lowering NEEDS the real early-return: `match validate(c) { ok(_) => <drop result,
  continue>, err(e) => return err(e) }`. (A let-bind `!` over a SCALAR-Ok real-Result callee — `let x =
  mk(n)!` — ALREADY lowers, lb1; the statement-`!` and heap-Ok let-bind-`!` are the gaps.)
- TWO sub-deliverables: (1) the statement-`!` early-return (tag-check + return-err + drop-ok-handle), and
  (2) make the strip WALL (not invalid-wat) for a mid-body can-err heap-Result statement as a stop-gap
  "never emit invalid wasm" fix. The full fix = the effect-`!` early-return machinery (the v0 side already
  supports it via emit_early_return_decs; the v1 statement/let-bind path needs the tag-branch + ok-drop).
  This is the #22 keystone, localized to real-Result callees (NOT the full bare-value-effect-fn subsystem —
  validate/json.parse return real Results, so the tag exists to branch on). dojo.backfill_dir's
  `dash_item(...)!` let-bind is the same family + the capturing filter_map.

## ⭐ CORRECTION (2026-06-27): porta.start/dojo need NESTED-MATCH desugar, NOT a v1 MIR Return op

My earlier "v1 MIR has no mid-function early-return (Return op), so porta.start + dojo need a MAJOR
structural subsystem" was WRONG (verify, don't assert — corrected by testing the target form). The effect-`!`
does NOT need an early-return Op: it desugars to a NESTED-MATCH continuation (the standard monadic do-desugar):

    { before; let x = f()!; after }   →   { before; match f() { ok(x) => { after }, err(e) => err(e) } }
    { before;     f()!    ; after }   →   { before; match f() { ok(_) => { after }, err(e) => err(e) } }

The continuation (`after`) nests in the Ok-arm; the Err-arm reconstructs `err(e)` (same `Result[_, String]`
error type as the enclosing fn) — NO early return, NO Return op. The function's tail becomes the (nested)
match, which the EXISTING heap-result-match tail lowering already handles.

VERIFIED — the explicit nested-match TARGET byte-matches for EVERY Ok type porta.start/dojo use:
- scalar Ok: `match validate(n) { ok(_) => ok(42), err(e) => err(e) }` → `42`/`bad` ✓
- nested (two `!`): `match validate(n) { err(e)=>err(e), ok(_)=> match parse(n) {err(e)=>err(e), ok(p)=>ok(p+1)} }` → `1`/`bad` ✓
- String Ok: `match parse(s) { err(e)=>err(e), ok(p)=>ok(p+"?") }` → `a!?`/`E:empty` ✓
- record Ok (porta.start's `Result[ProxyHandle, String]`): `match validate(n){err(e)=>err(e),ok(_)=>if … then err … else ok({handle,port})}` → `5:10`/`E:bad` ✓

So the REMAINING WORK for porta.start (and the effect part of dojo) is a FRONTEND/IR `desugar_effect_unwrap`
Block pass: scan an effect-fn body Block for the first `Unwrap`/`Try` (a `let x = f()!` Bind value OR a
bare `f()!` Expr stmt), split before/after, rewrite to the nested match above (Ok-pattern = the let var or
`_`; Err-pattern binds a fresh `e`; recurse on `after` + the arms). Entry: alongside `desugar_heap_branches`
(mod.rs:1003), threading a function-wide `next_var` (like the block_line fresh-VarId fix). Structures:
`IrExprKind::Match{subject,arms}`, `IrMatchArm{pattern,guard:None,body}`, `IrPattern::{Ok,Err,Bind,Wildcard}`,
`IrExprKind::ResultErr{expr:Var(e)}`. This is TRACTABLE (verified target) — NOT the major Return-op
subsystem I wrongly claimed. dojo.backfill_dir ADDS the capturing-effectful-`filter_map` (the closure body
calls fs.read_text per element + yields Option[record]) on top — that's the closure-as-effectful-loop-body
frontier, the genuinely harder remaining piece.

## ⭐ The remaining 3 lowering-walls ALL reduce to ONE frontier: the UNLIFTABLE HOF CLOSURE (2026-06-27)

After the effect-monad `!` desugar (9af74ed5) + porta.start's native-FFI re-classification, the 3 remaining
org lowering-walls (wasm-bindgen generate_dts/esm, dojo backfill_dir) ALL hit the SAME decline:
`list.{map,filter,flat_map,filter_map,fold} with an unliftable/closure-list higher-order argument`
(calls.rs:166 / binds_p2:651 — `try_lower_defunc_list_hof` returned None AND
`last_call_had_unlifted_closure`). Three isolated sub-cases (each verified — the SIMPLER shapes all lower,
only these stacked forms wall, so the defunctionalizer is the single lever):

1. **NESTED LAMBDA + INNER HOF in the closure body** (generate_dts `sigs`, generate_esm `record_helpers`).
   MINIMAL REPRO (walls): `xs |> list.flat_map((f) => { let tag = (p) => if … then "BIG" else p; ["a","Mb"]
   |> list.map((p) => "x:" + tag(p)) |> list.join(", "); [f + "(" + parts + ")"] })`. A flat_map closure
   whose body defines a let-bound lambda (`tag`/`param_ty`) AND runs an INNER `list.map` calling it.
   `try_lower_defunc_list_hof` inlines the OUTER closure body but cannot defunctionalize the INNER HOF
   nested inside it (nested defunctionalization). A block-bodied closure with let-bound LISTS lowers fine
   (sig1/sig2 verified) — ONLY a nested lambda + inner HOF declines. NOTE: a flat_map closure with a plain
   heap-result-`if`-CHAIN body (no inner HOF) ALSO lowers (fmc/fm2 verified) — so the if-chain is NOT the
   gap, the inner HOF is.
2. **EFFECT CALL inside the closure body** (dojo `backfill_dir`): `task_files |> list.filter_map((f) =>
   match fs.read_text(dir+"/"+f) { ok(c) => some(parse_task_md(f,c)), err(_) => none })` — the closure
   captures `dir`, runs an EFFECT call per element, yields `Option[record]`. The block-level effect-`!`
   desugar does NOT reach an effect call buried in a HOF closure; the closure-as-effectful-loop-body needs
   the effect threaded through the defunctionalized per-element loop.
3. **(List,List) CONDITIONAL tuple-fold** (generate_esm `tuple_helpers`): `all_tuples |> list.fold(([],[]),
   (st, ty) => { let (lines, seen) = st; if list.contains(seen, sig(ty)) then (lines, seen) else (lines +
   emit(ty), seen + [sig]) })` — TWO heap-list loop-carried slots with a CONDITIONAL acquire (both branches
   rc-preserving — the OwnershipFilter.v CondLoop pattern, already PROVEN). `try_lower_defunc_tuple_acc_fold`
   (control_p5:75) handles (List[T], Int) only; extend to (List[heap], List[heap]) + a conditional body.

THE KEYSTONE = extend `try_lower_defunc_list_hof` to handle a closure body containing (a) an inner HOF +
nested let-bound lambda [#1], (b) an effect call [#2], and `try_lower_defunc_tuple_acc_fold` for the
(List,List) conditional fold [#3]. Cracking #1 (nested defunctionalization) likely clears generate_dts +
generate_esm record_helpers; #3 clears generate_esm tuple_helpers; #2 clears dojo. All gate-verifiable
(byte-match + corpus-wall + the proof gate). This is the LAST org lowering frontier.

### CRITICAL refinement: sub-case #1 (nested HOF) needs the NESTED-LOOP cert (a Coq frontier)

Tracing the nl1 decline deeper: a flat_map closure containing an INNER `list.map` means the inner map's
per-element LOOP nests INSIDE the outer flat_map's loop. But OwnershipLoop.v's proven `Loop` is FLAT —
"Loop bodies are FLAT (no nested loop) … nested loops are a future compose-able extension" (the 2026-06-20
LANDED note). So `try_lower_defunc_list_hof` declining a closure body that contains another HOF is SOUND
(it correctly refuses to emit a nested loop the cert can't verify), not a mere engineering gap. generate_dts
`sigs` / generate_esm `record_helpers` (inner `params |> list.map(… param_ty …)` over a variable-length
list — NOT unrollable) therefore need the **nested-loop ownership cert** (extend OwnershipLoop.v: a Loop
whose body contains a Loop, the rc-preservation composed) — a #31-class Coq frontier, NOT just a
defunctionalizer tweak. Ranking the 3 remaining by tractability:
- **#3 (List,List) conditional tuple-fold** (generate_esm `tuple_helpers`) — MOST tractable: the CondLoop is
  ALREADY proven (OwnershipFilter.v), two FLAT loop-carried slots, no nesting. Extend
  `try_lower_defunc_tuple_acc_fold` to (List[heap], List[heap]) + a conditional body. Provable today.
- **#2 effect-in-closure** (dojo `backfill_dir`) — medium: the filter_map closure's per-element body runs an
  effect call; thread the effect (Stdout/FsRead cap) through the defunc per-element loop. FLAT loop, but the
  effect-`!`-inside-a-closure (vs the block-level desugar already shipped) is new machinery.
- **#1 nested HOF** (generate_dts/esm) — DEEPEST: needs the nested-loop Coq cert first.
So generate_esm's two walls split: `tuple_helpers` (#3, tractable) vs `record_helpers` (#1, Coq). dojo = #2.

## ⭐ CORRECTION + WIN (2026-06-27): generate_esm was the BOUNDED-DUPLICATION GATE (not Coq), now CLEARED

Full-module bisection (the ONLY reliable method — render_program standalone doesn't link string.starts_with/
list.contains, so isolated repros mislead) of the REAL wasm-bindgen mod.almd corrected my repeated
mis-diagnosis:
- **generate_esm**: NOT nested-HOF, NOT Coq. Its wall was `desugar_let_bound_heap_branch`'s BOUNDED-
  DUPLICATION gate (mod_p6.rs:1238, was `rest_branch_binds > 2`). generate_esm stacks **4 top-level
  optional-list `if`s** (matrix_helpers/bytes_helpers/import_shim/shim_noop, each `if cond then [LITERALS]
  else []`); the first sees rest=3 > 2 → the gate bailed → the merged let-bound `if` walled "no sound
  scope-end drop". Raising the gate to `> 3` (≤ 2^4 = 16 leaves; the tail-duplication is PROVEN per-arm
  balanced) CLEARS generate_esm. Verified: corpus-wall ownership **18854→19338 ACCEPT** (+484 corpus
  let-bound-ifs now duplicate-and-lower, all kernel-re-verified), output-parity 124/124, corpus-wall fast
  (1:52, no blowup). **wasm-bindgen 2→1.**
- **generate_dts**: GENUINELY the nested-loop frontier. Bisection pinned its wall to the `sigs` flat_map
  closure's `param_ty` — `params |> list.map((p) => "${name}: ${param_ty(p)}") |> join` is an INNER map
  nested inside the outer flat_map loop (removing param_ty clears generate_dts). A loop-inside-a-loop-body
  is unrepresentable in OwnershipLoop.v (flat Loop body) → needs the **nested-loop ownership cert (Coq)**.
- **dojo backfill_dir**: effect-call-in-closure (`filter_map` whose body calls fs.read_text + yields
  Option[record]) — the effect-in-defunc-loop machinery.

NET: lowering-walls **3→2** (generate_esm cleared). Remaining 2 = generate_dts (nested-loop Coq) + dojo
(effect-in-closure). The "deep/Coq" meta-pattern held HALF the time: generate_esm was a 1-line gate (the
meta-lesson "deep was repeatedly an empirical mischaracterization — verify per-wall" applied AGAIN), but
generate_dts's nested loop is a real Coq frontier.
