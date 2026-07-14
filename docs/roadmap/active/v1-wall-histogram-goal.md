<!-- description: ENDGAME — v1 walled-real to ZERO (currently 11); stage ledger + open-wall diagnoses -->
# GOAL — v1 walled-real → 0 (ENDGAME)

> **Read first**: `proofs/corpus-wall.sh`'s Unsupported histogram, the
> self-host linkage pattern (`stdlib/*.almd` + `render_wasm/registry.rs` +
> `purity.rs` — registry name, PURE_MODULES drift gate, typed routing with
> unlinkable `_x`/`_wall` names), and the memory ledger
> `project_cert_format_ladder.md` (every stage, numbered).
>
> This file was COMPACTED 2026-07-14 (4629 → ~1000 lines): closed stages
> B1–B116 and superseded diagnoses are one-line ledger entries below; their
> full text lives in this file's git history. Open-wall diagnoses and the
> latest stages (B117+) are kept verbatim.

## Non-negotiable invariants

1. **Honest wall over silent miscompile, always** — a shape outside the
   subset fails CLOSED (explicit `Unsupported` / unlinked wall name), never
   wrong bytes. Byte-parity vs v0 per opened function BEFORE commit.
2. **Zero new trusted runtime in v1**: stdlib growth is `.almd` self-host
   (own PCC certs), never a WAT/Rust port into the renderer.
3. **Registry discipline**: PURE_MODULES drift gate, typed routing, public
   sigs only across self-host modules.
4. Tiered testing, stop on first red; classify diff (zero newly-walled) per
   stage; adversarial wasmtime-vs-v0 repro for every closure — classify
   proves lowering success ONLY, never output correctness (test-block fns
   especially).
5. Commit per stage at all-green (English, one line, no prefix).

## Completed campaigns (pre-ENDGAME, 2026-07-10..11)

Regex family self-hosted (381 walls → 0; 8 APIs byte-parity, v0-quirk
faithful). json.root/field/index + bytes.append_u8 tails opened. Double-digit
campaign 306 → 166 via generated variant reprs, List[Option/Result] literals,
Option-`!` polarity + unit-main err protocol, cross-module top-let bridge,
testing.assert self-host, fan.settle/any inline, named/anon record reprs,
Camp-4 opener. Full sub-task text: git history of this file.

## Verification ladder (per stage)

```
almide test spec/stdlib/ && almide test  # parity first (both targets)
cargo test -q -p almide-mir
proofs/gate.sh && proofs/corpus-wall.sh  # PCC + kernel oracle + histogram
cargo test -q
```

## Exit criteria

- [x] Every regex.* corpus call site either EXECUTES v0-byte-identically or
      walls on a RECORDED unsupported feature (regex family opened 2026-07-10).
- [x] Engine edge-case suite green (greediness, empty match, anchors, split
      empties — the scouted list), on BOTH targets.
- [x] json.root/field/index + bytes.append_u8 buckets opened or their real
      blocker recorded (json_path self-hosted at 2z, walls 170→166).
- [ ] Histogram deltas recorded; corpus PCC (binary + kernel oracle) ACCEPT
      throughout; pushed at all-green; Trust Spine green (ongoing per stage).

## ENDGAME: walled-real → 0 (set 2026-07-11 at 166; currently 11)

The target is ZERO — no allowlist, no "permanent wall" netting. Every corpus
function lowers, witnesses, and kernel-ACCEPTs.

**The remaining 11 and their precise blockers** (each has a kept-verbatim
diagnosis below or a ledger pointer):

1. `codegen_effect_fn_test :: find_first_even` — `!` bound to a let/var needs
   early-return propagation out of a while loop (new loop-op subsystem).
2. `codegen_loop_guard_test :: for-in guard continue` — for-in break/continue
   unbuilt (model-one-iteration fallback loses the early exit).
3. `crossmod_variant_payload_test :: #484` — variant ctor arg with a
   heap/recursive field (ADT brick 5 extension).
4. `nested_match_option_string_test :: is_balanced` — heap-accumulator
   `list.fold` (`fold_hacc` wall, mod_p4.rs:~1100) + Option[List[String]]
   subject tracking with a RECURSIVE drop (flat heap_elem_lists sweep would
   leak the inner strings) + the fold lambda's own lowering.
5. `compound_eq :: main` — `(record, Int)` tuple list: anonymous-record
   resolution blocker (kept DIAGNOSIS below, two candidate directions).
6. `compound_repr_interp :: main` — 3-level `List[Map[String,List[Option
   [Int]]]]` nesting (MapHval is hardgated to flat inner lists).
7. `compound_repr_records_interp :: main` — mixed-payload variant list
   construction + `${List[record]}` stringification (two separate gaps).
8. `fan_pure_thunks :: main` — `fan.settle` list construction hits the same
   List[heap] cluster (B115 traced the race-side fix; settle remains).
9. `fan_var_thunk_list :: main` — runtime-sized thunk lists need declared-cap
   concurrency machinery (fan.race over a var list).
10. `map_fold_heap_acc :: main` — `List[(String, <nested Map>)]` (same
    family as compound_eq's tuple blocker).
11. `playground_default :: wrap_lists` — loop-carried accumulator frontier
    (the "(B) mechanism" — kept DIAGNOSIS below; caps-counting invariant
    blocks the naive gate extension).

### Stage ledger (compressed; full text in git history)

- DIAGNOSIS UPDATE (2026-07-12, probes ia1/ia2): the interp-in-arg 7 are NOT an ANF-lift problem — `${g(n)}` (String call part) and `${list.min([ints])}` (Option call part) ALREADY lower in-arg. The 7 d
- B1. Scalar-tuple Some ctor SHIPPED (156 → 154): `try_lower_option_ctor`
- B2. Result wildcard arm in the value match SHIPPED (154 → 151): the
- B3. Option-of-variant ctors SHIPPED (151 → 150): three pieces (probe ov1,
- B4. Record defaults + scalar field ANF lift SHIPPED (150 → 146 — the <150
- B5. Heap-eq unit-if conditions SHIPPED (140 held): `try_lower_unit_if`'s
- B6. Record-destructure match desugar SHIPPED (127 → 126): `match f {
- B7. Free-fn UFCS resolution SHIPPED (119 → 117): `desugar_method_calls`
- B8. Record fn-field call desugar SHIPPED (117 held, an enabler): a Method
- DIAGNOSIS (v1, the sized-int interp walls — si1/si2): even the single-field `"a=${o.a}"` (UInt8 Member part) walls at the interp-in-arg position — the part's narrow-int to_string routing/concat operan
- B9. Sized-int interp display SHIPPED (117 → 114): `interp_to_string_call`
- B10. Depth-2 single-outer ctor patterns SHIPPED (114 → 113):
- B11. Fixed-length list-pattern match desugar SHIPPED (113 → 112):
- B12. Depth-2 multi-outer ctor fallthrough SHIPPED (112 → 107): two composed
- B13. Option[scalar] ctor fields + in-arg tuple-variant matches SHIPPED
- B14. Option-`?` identity desugar SHIPPED (102 → 99 — waypoint <100
- B15. Unit-discard `!` normalization SHIPPED (73 → 72): `let _ =
- B16. Scalar-scalar Result `err(<scalar>)` ctor SHIPPED (71 → 69):
- B17. Option-`!` heap payload SHIPPED (69 → 68): the effect-unwrap
- B18. First-class fn values into pure combinators SHIPPED (67 → 62): a
- B19. Option[List[scalar]] `??` SHIPPED (62 → 58 — waypoint <60 CROSSED):
- B20. Closures in record slots SHIPPED (58 → 54): the B8-diagnosed
- B21. Scalar-key (String-value) tuple lists SHIPPED (49 → 48): the
- B22. Map[Int,String] from_list + display (48 held, an enabler):
- B23. Display tail continued (48 held, enablers): `${List[Option[Int]]}`
- B24. hval from_list/display self-hosts + an ownership lesson (48 held):
- B25. Owned-route (String, List[scalar]) pairs literal SHIPPED (48 held):
- B26. List[Map] nesting SHIPPED (48 → 47): the compound-repr depth-2
- B27. RawPtr / linear-memory bridge SHIPPED (47 → 45): the #440 / C-062
- B28. (String, Int) tuple eq (45 held, an enabler): the first of the two
- B29. Small-variant eq + (String, scalar) tuple literals SHIPPED
- DIAGNOSIS (at 44): value_deep_eq / compound_eq mains — the 5-chain
- B30. The 5-chain ceiling OPENED (44 → 43): branch_lift (the shared
- B31. All-scalar tuple lists via the OWNED route (43 held, an enabler):
- B32. list.unique/dedup over flat-block heap elements + List[List[scalar]]
- B33. Variant ctor `List[String]` field opened — ADT brick 5 extension (41 → 40):
- B34. (String, Int) / (Int, String) tuple list literals (40 held, an enabler):
- B35. Heap-result `match` as a call argument (40 held, an enabler): the
- B36. `List[<Fn>]` literal construction opened (40 held, an enabler):
- B37. (String,Int)/(Int,String) widened to any scalar (40 → 39) + a newly-
- DIAGNOSIS (at 39): two further root causes pinned by direct probing,
- B38. Closure `List[String]` capture ratchet closed — 3rd env-header
- B39. Flat record/variant Map keys opened — tuple-pair classifier
- DIAGNOSIS (at 35): B39's flat-heap generalization incidentally opened the
- B40. `List[Closure]` as a HOF data argument opened (35 → 34):
- B41. `map.find` self-hosted end-to-end — the confirmed near-miss soundness
- B42. Tail-position variant constructor calls opened — a general gap, not
- B43. `Option[<custom variant>] ?? <ctor fallback>` opened — closes
- B44. `unwrap_never_err_call_types` regression fixed for List/Record/Tuple
- B45. `branch_lift.rs`'s dense-region lift widened from `If`-only to
- B46. `unit_main` die-on-error gate narrowed to the VOID convention only —
- B47. All-scalar tuple `Some((x, y))` admitted as a heap-result MATCH/IF ARM
- B48. `(String, <scalar>)` tuple `Some((k, v))` admitted as a heap-result
- B49. `some(<custom variant ctor>)` admitted as a heap-result MATCH/IF ARM
- B50. `auto_try.rs` explicit-`ok(...)`-sugar stripping — the ACTUAL fix for
- DIAGNOSIS (at 26, two REVERTED dead-end attempts, no code shipped): `protocol_edge_test.almd`'s "match over a never-err effect-fn call with
- B51. The THIRD attempt found the real function and closes `protocol_edge_
- B52. Two-layer fix for `codegen_patterns_test.almd`'s "match arms
- B53. Closed B52's own scoped follow-up: the sibling `If` arm in
- DIAGNOSIS — `nested_unwrap` (`result_option_matrix_test.almd`) reverted, NOT fixed, a genuine regression caught before shipping: `{ let r: Result[Option[Int],String] = ok(some(42)); let o = r!; o! }` 
- DIAGNOSIS — the ROOT CAUSE shared by (at least) `unannotated_unwraps` (effect_assign_unwrap_test.almd), `nested_unwrap` (result_option_matrix_ test.almd), and plausibly `is_balanced` (nested_match_opt
- B107. Closed 2 of 3 "heap-result match/if outside the executable subset"
- DIAGNOSIS — the 4-entry "match over an UNTRACKED subject with a call-bearing arm" cluster, per-entry findings (nothing shipped, one near-miss reverted after a correctness check caught it): `json_path_
- B108. Closed a latent dangling-call crash for ANY program using an
- DIAGNOSIS — `option_result_symmetry_test.almd`'s "option.collect_map all some" (part of the 4-entry UNTRACKED-subject cluster) has TWO SEPARATE gaps, only one of which is safe to fix in isolation — th
- B109. Closed `cross_module_toplet_byvalue_test`'s "heap module-level
- DIAGNOSIS — `cross_module_variant_test.almd`'s TWO "heap bind from LitInt" walls (#412 and #631, BOTH `varlib.Circle{radius:..}` record-variant patterns) are a FRONTEND type-checker/lowering gap, NOT 
- B110. Found and fixed the actual root cause of the #412/#631 diagnosis
- B111. Closed `json_path_edges.almd :: p_set`'s "UNTRACKED subject" wall
- B112. Closed HALF of the two-part `bidirectional_type_test` gap the
- DIAGNOSIS — found the missing mechanism the earlier "auto-wrap ABI" DIAGNOSIS (search this file for "DIAGNOSIS — the ROOT CAUSE shared by") explicitly flagged as "unidentified, needs its own investiga
- DIAGNOSIS — pinned the EXACT root cause of the `List[heap]` literal cluster's "variant-ctor-list" sub-shapes (3 of the 7 mapped entries: `compound_ repr_recursive_interp.almd`, plus the shared root be
- DIAGNOSIS — picked up the auto-wrap ABI investigation exactly where the prior fork left off (its own "next step": debug-trace `heap_result_arm. rs`'s bare-scalar-tail handling) and found + FIXED the f
- DIAGNOSIS — built the registry (`AUTO_WRAP_ABI_FNS`, mirroring `NEVER_ERR_ LIFTED_FNS`'s exact architecture) the prior entry called for, wired it through ALL FOUR consumer sites needed, got `unannotat
- B113. Closed `unannotated_unwraps` (18 → 17) by re-implementing the
- DIAGNOSIS — pinned the EXACT remaining blocker for `option_result_symmetry_ test.almd :: "option.collect_map all some"` (the earlier DIAGNOSIS's "unlinked stdlib/runtime call" half) to a concrete, ful
- B114. Closed `bidirectional_type_test`'s "structured error - overflow
- B115. Fixed a real (previously undiscovered) silent-wrong-value RISK in `desugar_fan_race_any`'s
- B116. Closed `option.collect_map` (16 → 15) — the registry.rs lock had cleared; implemented the

### Kept diagnoses (open walls) and latest stages

DIAGNOSIS (at 40): **the remaining 40 were triaged in full** (a fork read
   every fixture at its wall site against the current lower/*.rs code). Full
   per-entry breakdown lives in the triage transcript; the load-bearing
   findings:
   - **UNTRACKED-subject match linearization** (control.rs:304, "cannot take
     the both-arms linearization") is a HARD/DEEP, shared root cause across
     ≥5 entries (bidirectional_type_test, option_result_symmetry_test,
     fan_pure_thunks, json_path_edges, + likely more) — the both-arms
     linearization is unsound for a call-bearing arm; opening this needs REAL
     per-arm branching over an untracked (non-Option/Result/variant) subject,
     not a narrow admission widening. Do not attempt piecemeal.
   - **Cross-module variant registry gaps** (#412/#631/#484,
     crossmod_variant_payload_test) — `VariantLayouts` is never populated for
     a FOREIGN module's ctors; HARD/DEEP, a registry-merge project of its own.
   - **Generics + monomorphization** (generic_fn_in_inferred_lambda's
     `List[Box[Int]]`) — tried widening `try_lower_record_list_literal_as`
     with an `is_flat_variant_ty` arm (binds_p3.rs): WORKED for a concrete
     flat variant (`IBox = IB(Int)`, probe bx2 PARITY) but FAILED for the
     generic case — `VariantLayouts` stores the UNRESOLVED generic field type
     (`T`, not `Int`), so `is_flat_variant_ty`'s `!is_heap_ty(fty)` check
     sees a bare type-variable and returns false regardless of the concrete
     instantiation. REVERTED (zero corpus benefit + incomplete + unexercised
     code is itself a risk this session's `_str`-dispatch bug proved). Fixing
     for real needs a mono-aware registry lookup, not a narrow arm.
   - **`map.find`'s Option[(String,Int)] payload — CONFIRMED HARD, a NEAR-
     MISS SOUNDNESS TRAP (map_insertion_order.almd, branch_lift_synth_0)**:
     traced the FULL admission chain — `is_self_host_option_module_fn`
     (mod_p4.rs) is missing `"map" => "find"` (a one-line whitelist gap), and
     `control.rs`'s `is_self_host_option_call` handler already GENERICALLY
     seeds `materialized_options` + `heap_elem_lists` for ANY `Option[heap]`
     subject via `is_heap_elem_list_ty` — so the WIRING looks trivial. It is
     NOT: `heap_elem_lists` routes to the FLAT (no-mask) `Op::DropListStr`,
     which does a BLIND blind `rc_dec` of the payload slot (Option's `len@4`
     doubles as its 0/1 tag, so the "loop" runs 0-or-1 times — the len-as-tag
     trick, intentional). For a `(String,Int)` TUPLE payload, that blind
     rc_dec only decrements the TUPLE's OWN refcount — if it hits 0 the
     tuple's memory frees WITHOUT recursively freeing the tuple's OWN String
     field = **a LEAK**, the exact class of bug the (Value,scalar)-tuple
     precedent in binds_p4.rs (~L216-229) already had to special-case via
     `variant_drop_handles = "value_tuple"` (swapping the flat mask for a
     recursive `$__drop_value_tuple`). No `Op::DropOption<Tuple>` analogue
     exists yet (only the LIST-of-tuples `DropListStrInt`/`DropListIntStr`
     this session's B34 wired up). **Do NOT just add "find" to the
     whitelist** — it would ship a real (if narrower-than-wrong-bytes) leak.
     The correct fix needs a NEW `Op::DropOptionStrInt` (mirroring
     `DropResultStrInt`'s shape but len-as-tag instead of cap-as-tag) wired
     through the full authority chain (Op def in lib.rs, render_wasm_p2.rs
     emission, mod_p3.rs cascade, certificate.rs, render_rust.rs,
     translation_validation.rs) PLUS the admission site
     (`is_self_host_option_call`'s handler, routing to
     `variant_drop_handles` instead of `heap_elem_lists` for a tuple-with-
     heap-field payload) — a real, careful, multi-file brick, not a
     one-liner. Same likely applies to `pattern_test`'s branch_lift_synth_4
     (Result[String,String] match — the STANDALONE match already works,
     probe rss1 PARITY; the fixture-specific failure needs the DENSE branch_
     lift context reproduced, not yet isolated) and `control_flow_test`'s
     branch_lift_synth_3 — re-diagnose with this SAME lens (check for a
     similar blind-flat-drop trap) before touching either.
   - map_fold_heap_acc's residue (after B34) is the separate, previously-
     diagnosed `map.fold_hacc` self-host gap (LOW yield, deferred).
   **Lesson reinforced**: an admission-chain gap that LOOKS like "just add
   the callee to a whitelist" must be checked against what DROP the
   resulting tracked value gets routed to — a flat/masked drop is only sound
   when the payload owns no further heap children one level down. This is
   the THIRD time this exact class of trap has surfaced this session (the
   `_str`-dispatch wrong-bytes bug, the Map/Set key `_x` wall fixes, and now
   this near-miss) — always trace the drop, not just the tag-read, before
   wiring a new admission.

DIAGNOSIS — `map_fold_heap_acc.almd`'s "List argument cannot be faithfully
   materialized" wall is the compound_repr_* cluster in disguise, NOT an
   independent bug. Bisected (no source edits made — `git status` clean
   throughout) down to a single minimal repro with NO `map.fold` involved
   at all: `let m: Map[String, Map[String, String]] = ["k0": ["k0": "x"]]`
   — a bare bind of a NESTED Map literal (a Map whose VALUE type is itself
   a Map) — walls on its own, used or not. A Map is represented internally
   as a "paired-slot List" (per the existing comment in calls_p2.rs), so
   this is STRUCTURALLY the same shape the already-documented "non-empty
   List[heap] literal with nested-ownership elements (a heap-field record/
   tuple, a list, a call result) cannot be faithfully materialized" wall
   covers (`compound_repr_interp.almd`/`compound_repr_records_interp.almd`/
   `compound_repr_recursive_interp.almd`/`generic_chain_unwrap_or.almd`/
   `generic_fn_in_inferred_lambda.almd` — 5 of the current 24 entries) —
   just reached via Map-literal construction instead of List-literal
   construction. `map_fold_heap_acc.almd`'s ACTUAL fold-with-heap-
   accumulator logic (the file's own stated purpose per its header
   comment) is unaffected — ALL of its `map.fold` calls over flat
   (non-nested) Map/List shapes render fine in isolation (verified: the
   first three `map.fold` lines of the file, extracted alone, lower past
   this specific wall — they hit a SEPARATE, unrelated "unlinked map.fold_
   hacc" self-host-registry gap instead, likely just needing all 5 of the
   file's functions present for correct registry linking, not investigated
   further here). The ONLY line that hits "List argument cannot be
   faithfully materialized" is the `map.get_or(["k0": ["k0": "x"]],
   "missing", y3)` sub-expression's nested map-literal argument. Given
   this is the SAME "generics/monomorphization frontier" gap already
   scoped for the compound_repr_* cluster (not a scoped, safe fix — it
   needs the nested-heap-element container-literal construction work,
   not a decline-point extension), no fix attempted. Recommend: when the
   compound_repr_* cluster is eventually tackled, re-classify_corpus
   afterward — `map_fold_heap_acc` likely closes as a side effect (it may
   even be worth ADDING to that cluster's fixture list, since it's the
   only entry currently exercising the Map-literal path instead of the
   List-literal path — same construction machinery, different literal
   syntax). **Current 24, unchanged** (zero source edits made or
   reverted).

DIAGNOSIS — `wrap_lists` (playground_default.almd, B107's documented
   separate root cause, B108's adjacent drop-scan fix) is a DELIBERATE,
   ALREADY-DOCUMENTED wall — NOT a bug, NOT investigated further, NOT
   fixed this pass. Bisected via temporary debug instrumentation in
   `lower_heap_result_if_inner` (control_p3.rs — added then fully
   reverted, `git checkout --`, confirmed classify matches the B110
   baseline exactly): even the SIMPLEST possible repro (`fn f(flag) -> ...
   = { let result = {out:["a","b"], in_ul:flag}; if result.in_ul then
   result.out+["</ul>"] else result.out }` — a plain record LITERAL
   binding, no `list.fold` needed) walls identically. The COND and the
   THEN arm (`result.out + [...]`, a list-concat) both lower successfully
   — the ELSE arm alone (`result.out`, a BARE record-field access with no
   concat) is what fails. Traced to `heap_result_arm.rs`'s `lower_heap_
   result_arm`: its `IrExprKind::Member{object,field}` case is explicitly
   gated `if self.is_borrowed_param_container(object)` — and its OWN
   comment (lines ~848-856) explicitly discusses and NAMES `wrap_lists`:
   "A LOCAL container (`else result.out` over a `list.fold` result, the
   playground `wrap_lists`) is the LOOP-CARRIED-accumulator frontier (the
   `(B)` mechanism) — admitting it makes the enclosing fold body lower,
   whose defunctionalized elided-call count then outruns the source
   count-gate (a caps WALL BREACH). Defer the local-container case (`None`)
   so it keeps its existing wall — the loop-slot work owns it." So this
   isn't a gap nobody noticed — it's a KNOWN, ALREADY-SCOPED frontier
   (referred to elsewhere as "the `(B)` mechanism" / "loop-slot work"),
   deliberately deferred because a naive extension (admitting a LOCAL
   record-field-access arm the same way a BORROWED-PARAM one already is)
   would violate the `mir_calls <= ir_calls` caps-counting invariant once
   the enclosing `list.fold` body's own elided/defunctionalized call
   accounting is dragged into scope — a substantially different, harder
   problem than a simple gate-widening (every OTHER fix in this session's
   "twin function"/"sibling arm" pattern was safe specifically BECAUSE it
   didn't touch caps-counting; this one does). Genuinely out of scope for
   a quick pass — needs whatever the referenced "(B)"/loop-slot mechanism
   actually is, likely a substantial new piece of infrastructure for
   correctly counting calls through a fold-carried accumulator. **Current
   19, unchanged** (fully reverted, zero diff).

DIAGNOSIS — the "non-empty List[heap] literal with nested-ownership elements"
   cluster (5 entries) plus `map_fold_heap_acc` (entry 106's diagnosis) is
   NOT one gap — it's (at least) FOUR DISTINCT sub-shapes, each needing its
   own new drop-routing/materialization work, confirmed by isolating each
   file's actual trigger line (zero source edits made — `git status` clean
   throughout this investigation). Precise findings, replacing the prior
   vague "generics/monomorphization frontier" label with specifics:
   (1) `compound_repr_interp.almd`: the trigger is `let deep: List[Map[
   String, List[Option[Int]]]] = [["k": [some(1), none]]]` — a THREE-level
   nesting (List → Map[String,·] → List[Option[Int]]). The existing
   `ListElemDrop::MapHval` case (binds_p3.rs `try_lower_record_list_
   literal_as`) is hard-gated to `Map[String, List[Int]]` specifically
   (`matches!(b[0], Ty::Int)` — a flat scalar inner list only); it does not
   generalize to a HEAP inner-list element (`Option[Int]`), which needs its
   OWN recursive drop composed with the map's hval drop AND the outer
   list's drop — a new three-way-composed drop routine, not a decline-
   point widening. (2) `compound_repr_records_interp.almd` has TWO
   SEPARATE gaps entangled in one file: (a) `let shapes: List[Shape] =
   [Circle(1.0), Rect(2,3), Label{text:"box",at:Point{x:0,y:0}}]` — a list
   of a USER VARIANT with heterogeneous ctor payloads (tuple/record/
   nested-record) — FAILS AT CONSTRUCTION, because `try_lower_record_list_
   literal_as`'s dispatch only consults the RECORD registry
   (`record_or_anon_drop_type_name`), which is the WRONG registry for a
   variant type entirely (no code path in this function ever checks
   `variant_layouts` for a "list of variant ctors" shape) — this is what
   the corpus-visible wall actually reports (it's earlier in lowering than
   (b) below, so `render_program`/classify report THIS one). (b) SEPARATELY
   — isolated via a standalone repro after temporarily working around (a)
   — `let pts: List[Point] = [Point{x:1,y:2}, Point{x:5,y:6}]` (Point =
   `{x:Int,y:Int}`, ALL-SCALAR fields) turns out to construct FINE already
   today (confirmed: `list.len(pts)` renders and byte-matches v0) — the
   flat-record-list case I initially suspected as the root cause is
   ALREADY HANDLED, contrary to my first hypothesis. But `println("points=
   ${pts}")` (the file's actual line) fails SEPARATELY with "unlinked
   stdlib/runtime call(s)... list.to_string_x" — a STRINGIFICATION/repr-
   generation gap (not a drop/construction gap, and NOT the same as B108's
   anon-record drop-scan fix from earlier this session) — `list.to_string_
   x` (the `_x` fail-closed convention, per B34-era memory notes) means the
   repr generator for `List[<record>]` doesn't yet emit a working element
   formatter for THIS record shape. (3) `generic_fn_in_inferred_lambda.
   almd`: `let xs: List[Box[Int]] = [B(1), B(2), B(3)]` where `Box[T] =
   B(T)` is a GENERIC single-case tuple-payload VARIANT — same root issue
   as (2)(a): `try_lower_record_list_literal_as` has no "list of variant
   ctors" path at all (only the narrow `lenlist_elem_class`-gated CtorFlat/
   CtorLenLoop cases, themselves scoped to Option/Result ctors specifically
   — not arbitrary user variants). (4) `map_fold_heap_acc.almd`: already
   precisely diagnosed at entry 106 above (`Map[String, Map[String,
   String]]`, a nested-Map-VALUE-is-itself-a-Map literal — the Map-literal
   analogue of shape (1)'s List-of-heap-nesting). NOT individually isolated
   this pass (ran out of scope budget for one investigation):
   `compound_repr_recursive_interp.almd`, `generic_chain_unwrap_or.almd`,
   `compound_eq.almd`'s specific trigger line (likely ALSO shape (2)(a)'s
   "list of variant ctors" gap or a List[Tuple]-with-heap-element variant,
   given the file's `List[(Int,Int)]`/`List[List[Int]]` args tests already
   pass per `ScalarAggregate` — the wall must come from something else in
   the file, e.g. the record/set/map key sections near the bottom). **A
   real fix needs, at minimum, TWO separate new pieces**: (i) a genuine
   "list of variant-ctor elements" materialization path in `try_lower_
   record_list_literal_as` (mirroring the EXISTING record path but against
   `variant_layouts` instead of `record_layouts`, handling per-ctor-arm
   heterogeneous payloads within one list — non-trivial since different
   elements may need different per-element drop shapes if the variant
   itself isn't uniformly flat), and (ii) a composed multi-level drop
   routine generator for arbitrarily-nested Map/List/Option combinations
   (currently every nested-container drop case in this codebase is a
   HAND-WRITTEN, TYPE-SPECIFIC pairing — `MapHval`, `ListStr`, `StrStr`,
   etc. — not a general recursive composition; genuinely new
   infrastructure, not a decline-point extension). Neither is a safe,
   scoped fix for a single investigation pass — correctly left unattempted.
   **Current 18, unchanged** (zero source edits, `git status` clean).

DIAGNOSIS — refined findings for the LAST TWO "match over an UNTRACKED
   subject" cluster entries (B111 closed the third, `json_path_edges::
   p_set`), both traced via temporary debug instrumentation, both fully
   reverted (`git checkout --` on `control.rs`/`desugar_fan.rs`, confirmed
   classify matches baseline exactly, zero diff). No code shipped.

   **`bidirectional_type_test.almd`'s "structured error - overflow variant"**
   (`let e: Result[Int, MathError] = err(Overflow("too big")); match e {
   ok(_)=>.., err(Overflow(msg))=>.., err(_)=>.. }`, `MathError` a custom
   variant `DivideByZero | Overflow(String) | NegativeInput(Int)`): refines
   the EARLIER "would need new Err-payload-is-a-registered-variant drop
   routing" characterization — that drop routing ALREADY EXISTS and works
   (`try_lower_result_err_variant_ctor` in `result_ctors.rs`, explicitly
   built for exactly this `Result[T_scalar, <user variant>]` shape,
   confirmed via a standalone repro that the CONSTRUCTION alone renders
   fine). Found a REAL, separate, more precise bug: that constructor
   builds the Err-variant Result via the SHARED `materialize_opt_str_some`
   builder ("Err IS Some physically" — same len-as-tag layout) — which
   internally does `self.materialized_options.insert(obj)` (correct for
   genuine Option construction, since it's a shared builder) but NEVER
   `self.materialized_results.insert(obj)`. `try_lower_result_match`
   (control_p2.rs, the STATEMENT-position match this test needs) gates
   strictly on `materialized_results`/`materialized_results_str` — with
   NO Option fallback — so a `let`-bound `err(<variant ctor>)` value is
   never recognized as a tracked Result subject, falling straight to the
   untracked-subject wall. (The VALUE-position twin `try_lower_variant_
   value_match` apparently already has conflict-resolution for "both
   `materialized_options` AND `materialized_results` true, Result wins" —
   per an existing code comment — but `try_lower_result_match` has no such
   fallback at all.) **Even fixing this tracking gap would NOT close this
   specific entry alone**: `try_lower_result_match` additionally requires
   `arms.len() == 2` with simple `Ok{bind}`/`Err{bind}` patterns (no nested
   ctor) — this test's 3-arm match with a NESTED ctor pattern inside the
   Err arm (`err(Overflow(msg))`, matching a SPECIFIC variant case) is a
   fundamentally different, unsupported shape for that function regardless
   of the tracking-set bug. A real fix needs BOTH: (1) track `materialize_
   opt_str_some`'s structured-error callers into `materialized_results`
   too (a small, likely-safe addition in `try_lower_result_err_variant_
   ctor`, NOT attempted — ran out of budget verifying it doesn't interact
   with the option/result dual-tracking edge cases B32 previously found),
   AND (2) extending `try_lower_result_match` (or routing this shape
   through the value-position machinery instead) to support nested ctor
   patterns in an Err arm — a real capability gap, not a decline-point
   widening.

   **`fan_pure_thunks.almd::main`** (`fan.race([thunk_a, thunk_b])` /
   `fan.any([try_c, try_d])` / `fan.settle([quiet_a, quiet_b])`, all with
   BARE FUNCTION REFERENCES as thunks, not inline `() => ...` lambdas):
   initially suspected `desugar_fan.rs`'s `fan_bodies` helper (the shared
   `race`/`settle`/`any` literal-thunk-list inliner) only recognizes
   `IrExprKind::Lambda` elements, not bare function references — but
   traced via debug instrumentation and found this hypothesis WRONG: the
   frontend already ETA-EXPANDS a bare function reference used as a list
   element into a zero-param `Lambda` (confirmed — `fan_bodies` DOES fire
   and extract 2 `Lambda` bodies correctly for `fan.race([thunk_a,
   thunk_b])`). The REAL wall traced to a match with subject `Call{Named{
   thunk_a}}` (a bare call to `thunk_a`, `ty=Int`) and 2 arms — a
   STRUCTURALLY INVALID tree `desugar_fan_race_any`'s in-place `walk_expr_
   mut` mutation appears to produce: `fan.race(...)`'s CHECKED type is
   `Result[Int,String]` (per the file's own header comment — "FanLowering
   wraps each non-Result thunk in an Ok adapter"), and the un-annotated
   `let r = fan.race(...)` presumably goes through SOME auto-unwrap/
   reconciliation match (`match fan.race(...) { ok(v)=>v, err(e)=>.. }`,
   mirroring the documented `fan.any` pattern this file's own top comment
   shows — `match fan.any([...]) { ok(pat)=>.., err(epat)=>.. }` IS an
   explicitly pre-existing pattern this desugar's PRE-order visitor
   handles). `desugar_fan_race_any`'s POST-order mutation of the match's
   SUBJECT (rewriting `fan.race(...)` → `thunk_a()`) appears to leave the
   SURROUNDING match's `ok`/`err` ARMS in place — over a now-`Int`-typed,
   non-Result subject that can never satisfy them, producing the
   untracked-subject wall (2 arms, one call-bearing) instead of the
   intended plain-value rewrite. NOT fully root-caused (would need to
   trace the EXACT reconciliation-match construction site, likely in
   almide-frontend's auto-unwrap/Try handling for MODULE calls specifically
   — this session's established auto-wrap-ABI investigation for NAMED
   calls, B105 DIAGNOSIS, may or may not be the same mechanism for `fan.*`
   Module calls) — genuinely deeper than the "just handle FnRef elements"
   fix first attempted (which was a no-op, since eta-expansion already
   produces Lambdas). **Current 18, unchanged for both** (zero source
   edits remain, `git status` clean).

DIAGNOSIS — completed the List[heap]-literal cluster map (an earlier fork
   this session mapped 4 of 7 sub-shapes; this pass isolates the remaining
   3, all via read-only investigation — no debug instrumentation needed,
   no edits made, `git status` never touched by this pass). **All three
   converge on the SAME already-diagnosed root mechanism** — `try_lower_
   record_list_literal_as`'s (binds_p3.rs) `ListElemDrop` classification
   doesn't cover every element shape a `List`/call-arg literal can carry,
   so the wall (binds_p2.rs:529-545 for let-binds, calls_p2.rs:612-619 for
   call-args — BOTH already try this SAME builder first) is reached:
   - `compound_repr_recursive_interp.almd`: bisected (12 independent
     `let`/`println` pairs tested standalone) to `let es: List[Either[Int,
     String]] = [Left(1), Right("y")]` — a bare `List[<variant ctor>]`,
     the EXACT category the earlier fork already named ("`try_lower_
     record_list_literal_as` only consults the RECORD registry, never
     `variant_layouts`"). The file's OTHER recursive/mutual/generic-record
     shapes (self-recursive `Tree`/`RNode`, mutual `A`↔`B`, `Holder[List[
     Int]]`) all construct FINE standalone — only the `List[Either[..]]`
     line is the trigger, confirming this file needs no NEW category.
   - `generic_chain_unwrap_or.almd`: bisected to `let md = [("x", ValInt(
     64)), ("general.alignment", ValInt(16))]` — a `List[(String, V)]`
     where `V` is a variant (`ValInt(Int) | ValStr(String)`). A TUPLE
     wrapping a variant ctor, not a bare variant ctor — `try_lower_record_
     list_literal_as`'s `ListElemDrop` enum already has SPECIFIC flat-tuple
     cases (`StrStr`/`StrInt`/`IntStr` — (String,String)/(String,Int)/
     (Int,String)) but none for "(String, <variant>)"; falls through the
     same way. Same underlying gap, one layer of tuple-wrapping deeper.
   - `compound_eq.almd`: bisected to `map.from_list([({name:"alice",
     age:30}, 1), ({name:"bob",age:25}, 2)])` — a `List[(P, Int)]` (a
     RECORD, not variant, as the tuple's heap slot) passed as a CALL
     ARGUMENT (hence the DIFFERENT wall message, "List argument..." vs
     "List[heap] literal..." — but traced to the SAME builder, `try_lower_
     record_list_literal` at calls_p2.rs:616, tried first for call-args
     exactly like the let-bind path). `list.contains`/`set.from_list`
     (tuple-only, no record) over `List[(Int,Int)]` elements construct
     fine — only the `(Record, Int)` tuple pairing for `map.from_list`
     hits the gap. `map_fold_heap_acc.almd` (already diagnosed at memory
     entry 106 as `Map[String, Map[String, String]]` — a Map literal is
     internally a paired-slot List, so `List[(String, Map[String,
     String])]`) is now CONFIRMED the same family too — the tuple's heap
     slot is a nested Map instead of a Record/Variant, same missing-
     `ListElemDrop`-case shape.

   **Consolidated map, all 7 entries, ONE underlying gap**: `try_lower_
   record_list_literal_as`'s `ListElemDrop` classification (binds_p3.rs)
   needs new cases for tuple/bare elements whose HEAP slot is a variant
   ctor, a nested Map, or (per the earlier fork's finding) needs an
   entirely separate variant-ctor-list construction path since bare
   `List[<variant>]` isn't a tuple shape at all — this is genuinely
   more than "add one match arm": each new element family needs its own
   drop-generation (a `$__drop_list_<family>` analog, mirroring how
   `StrStr`/`StrInt`/`IntStr` each got their own dedicated recursive drop
   when they were added). Real, scoped, INCREMENTAL infrastructure work
   (one element-family case at a time, matching how B33 added `List[
   String]` field support to variant drops earlier in this campaign) —
   NOT a single quick fix, but also not an unbounded "generics frontier"
   either now that the exact shapes are enumerated. Suggested order for
   a future session: (1) bare `List[<variant ctor>]` construction (closes
   `compound_repr_recursive_interp.almd` alone), (2) a `(String|Int,
   <variant>)` tuple `ListElemDrop` case (closes `generic_chain_unwrap_
   or.almd`), (3) a `(<record>, Int)` tuple case (closes `compound_eq.
   almd`), (4) a `(String, <nested Map>)` tuple case (closes `map_fold_
   heap_acc.almd`) — likely shares machinery with (2)/(3) once one non-
   flat-scalar tuple case exists as a template. **Current 18, unchanged**
   (zero source edits this pass).

B117. **Closed the generic-monomorphization gap for `List[<generic variant>]` literals (15 → 13,
   TWO closures from one fix) — implemented the "generate a per-instantiation shadow type + drop
   function" design the prior DIAGNOSIS (search this file for "type substitution" / "VariantLayouts.
   by_type") scoped out as "genuinely new infrastructure, needs its own session"**: confirmed the
   root cause precisely (debug-traced): a generic variant's DECLARED field types (`type Either[L,R]
   = Left(L) | Right(R)`) store `L`/`R` VERBATIM as `Ty::Named("L",[])`/`Ty::Named("R",[])` in
   `VariantLayouts.by_type` (confirmed NOT `Ty::TypeVar` — every consumer (`is_flat_variant_ty`,
   `is_rich_variant_ty`, `needs_recursive_drop`) reads this UNSUBSTITUTED registry entry regardless
   of any specific use site's concrete instantiation (`Either[Int,String]`'s `Ty::Named("Either",
   [Int,String])`, confirmed via debug trace — the args ARE present at the use site, just discarded
   by every consumer's `let n = match ty {...}` extraction). This makes EVERY generic variant look
   entirely scalar/flat (`is_heap_ty` on a bare unresolved typevar ref is never true), so
   `is_flat_variant_ty`/`is_rich_variant_ty` both return the "no admitted category" verdict for
   `List[Either[Int,String]]`'s element type — the list-literal builder's initial gate
   (`!elem_flat_variant && elem_rich_variant.is_none() && ...`) then declines the WHOLE construction.

   **Investigated whether `almide-optimize::mono` (CLAUDE.md: "Mono runs before codegen") already
   provides monomorphized per-instantiation type info to key off, per the parent directive's
   suggestion — it does NOT**: `mono::monomorphize` (crates/almide-optimize/src/mono/mod.rs)
   ONLY specializes FUNCTIONS with STRUCTURAL BOUNDS (`T: { name: String, .. }`), never touches
   `program.type_decls` at all — a generic ADT used directly in a value literal (no structurally-
   bounded function involved) is completely untouched by mono. No monomorphized-instantiation
   registry exists anywhere in the pipeline to key off; a real fix needs its own substitution.

   **Design (scattered substitution + PER-INSTANTIATION shadow generation — not a centralized
   pre-pass, since none existed to extend)**:
   1. `substitute_generic_ty(ty, subst)` (mod_p2.rs) — recursive `Ty::Named(sym,[]) → concrete`
      substitution, given a `{generic-sym → concrete-Ty}` map.
   2. `VariantLayouts::instantiated_cases(name, args)` — zips `layout.generics` against `args`,
      substitutes every case's field types; a NO-OP passthrough (`layout.cases.clone()`) when
      `layout.generics.is_empty()` — a non-generic variant is byte-for-byte unaffected, zero risk
      of regression on the entire existing non-generic corpus.
   3. `cases_need_recursive_drop` — the EXISTING `needs_recursive_drop`'s core loop, factored out
      so it can run against EITHER the raw registry cases (existing callers, unchanged) OR
      instantiated (substituted) cases (`instantiated_needs_recursive_drop`, new).
   4. `is_flat_variant_ty`/`is_rich_variant_ty` (mod_p2.rs) — now extract `(name, args)` via a
      shared `variant_name_and_args` helper (`Ty::Named(n,args)` — confirmed this IS how a
      concrete instantiation arrives, not `Ty::Applied`) and substitute BEFORE classifying, when
      `args` is non-empty. `is_rich_variant_ty` returns an INSTANTIATION-SPECIFIC name
      (`generic_variant_instantiation_name`, e.g. `"Either_Int_String"` — a WASM-identifier-safe
      mangling of concrete scalar arg names) instead of the bare generic name, since a SINGLE
      shared drop function could not correctly serve two DIFFERENT instantiations with different
      per-slot heap-ness (Box[Int] all-scalar vs Box[String] one heap field — both appear in the
      SAME program in `generic_fn_in_inferred_lambda.almd`, a real stress case this design had to
      handle, not a hypothetical). **Critical consistency gate**: before returning `Some(inst_
      name)`, ALSO verifies every substituted field type is in the supported-renderable set
      (a scalar, or an already-declared non-generic variant referenced by its real name) — WITHOUT
      this, admission could say "yes, rich" for a field shape the GENERATOR can't actually emit
      source for, reproducing the exact "admission gate says yes, nothing was generated → dangling
      call → invalid WASM" trap this campaign caught once already this session (entry123/B113's
      near-miss on a DIFFERENT wall). Verified this consistency gate is load-bearing by testing:
      WITHOUT it, the first end-to-end test (`List[Either[Int,String]]`) produced exactly that —
      `wasmtime: unknown func: failed to find name '$__drop_list_Either_Int_String'` — caught
      BEFORE shipping, by design (the whole reason step 6 exists), not by luck.
   5. `discover_generic_variant_list_instantiations(ir, variant_layouts)` (drop_sources.rs) — an
      `IrVisitor` scan over EVERY function body + top-let in the program (main + modules) for a
      `List{..}` literal expression whose `.ty` is `List[<generic variant with concrete args>]`;
      returns deduped `(base_name, inst_name, args)` triples via a `BTreeMap` (host-deterministic
      iteration order — the established `MonoKey`/`BTreeMap` precedent this codebase already uses
      for the SAME reason in `almide-optimize::mono`). DELIBERATELY scoped to List-literal element
      position ONLY (matching the actual corpus shapes), not a general instantiation scan — a bare
      `Left(1): Either[Int,String]` construction OUTSIDE a list stays on the EXISTING (already-
      correct-for-a-leaf-heap-field-only instantiation, verified by reading `try_lower_variant_
      ctor`'s field loop: it uses the OPERAND's OWN concrete `arg.ty`, never the generic
      registry's declared type, for its heap/scalar per-field decision — construction was NEVER
      broken by this bug, only the LIST-ELEMENT ADMISSION gate was) `needs_rec`/`record_masks`
      fallback path.
   6. `generate_generic_variant_instantiation_type_decls(instantiations, variant_layouts)`
      (drop_sources.rs) — for each instantiation, builds a SHADOW `IrTypeDecl` (Rust struct,
      `generics: None`, UNIQUE synthetic ctor names `__<inst_name>_c<tag>` so it never collides
      with the real type's own ctors — `Left`/`Right` stay registered to `Either`, not the shadow;
      the v1 runtime repr is driven purely by TAG NUMBER + FIELD ORDER, never ctor NAME, so a
      value built by the REAL `Either[Int,String]` construction and one built against the shadow
      are byte-identical) PLUS its Almide SOURCE TEXT `type <inst_name> = ...` declaration.
      Returns `(source_text, Vec<IrTypeDecl>)` rather than calling `generate_variant_drop_sources`
      itself — the FIRST implementation attempt called it internally and would have DOUBLE-
      DEFINED every regular variant's drop function (a compile error), caught immediately on the
      first build and fixed before proceeding; the shadow decls are instead spliced into
      `all_type_decls` ONCE in pipeline.rs, so the EXISTING single `generate_variant_drop_sources
      (&all_type_decls)` call covers the shadow too.
   7. `pipeline.rs` wiring: builds a PRE-relower `VariantLayouts` from `all_type_decls` (before the
      drops-append), runs discovery + shadow generation, splices the shadow `IrTypeDecl`s into
      `all_type_decls` and prepends the shadow `type` declaration text to `drops` (so the
      two-pass re-lower's `source_to_ir_with(&format!("{source}\n{drops}"), ..)` sees a real,
      type-checkable name for `$__drop_<inst_name>`'s parameter type to reference).

   **Verified**: `Either[Int,String]` (the exact `compound_repr_recursive_interp.almd` shape) —
   wasmtime `2` (list length), v0 native `2`, byte-identical; WAT confirmed `$__drop_Either_Int_
   String`/`$__drop_list_Either_Int_String` are REAL (non-dangling) generated functions. A
   200,000-iteration leak-loop (fresh `[Left(1),Right("y"),Left(2)]` construction every iteration)
   under a TIGHT 2MB memory cap (not the usual 16MB — a looser cap wasn't sensitive enough to
   guarantee catching a per-iteration String leak at this element size) produced the identical
   accumulated value (600000) on wasmtime and v0 native — no leak, no OOM. The `generic_fn_in_
   inferred_lambda.almd` shape (`Box[T]=B(T)`, used as BOTH `List[Box[Int]]` all-scalar AND
   `List[Box[String]]` one-heap-field in the SAME program — a genuine dual-instantiation stress
   case, not synthesized) — hand-written repro wasmtime `"3 a,b"`, v0 native `"3 a,b"`,
   byte-identical; a 200,000-iteration leak-loop under the same 2MB cap produced the identical
   accumulated value (1000000) on both — no leak, confirming the two DIFFERENT instantiations'
   distinct shadow drop functions (`Box_Int` all-scalar, needing no drop at all; `Box_String`
   needing the recursive path) never collide or interfere. `cargo test -q -p almide-mir`: 583/583.
   `classify_corpus`: 15 → 13, TWO closures (`compound_repr_recursive_interp.almd`,
   `generic_fn_in_inferred_lambda.almd`), zero newly-walled (diffed the full 13-name WALLED-REAL
   list against the prior 15-entry baseline). `almide test`: 283/283, 0 failed. GATE OK. CORPUS
   WALL OK (30820 heap objects, 5133 name witnesses, 4185 caps witnesses, 269 caps-transitive,
   Rocq kernel-certified in 266s, FORBIDDEN=0).

   **What remains in the List[heap]-literal cluster (NOT closed by this fix — different
   sub-shapes, per the earlier consolidated map)**: `generic_chain_unwrap_or.almd` (a TUPLE
   wrapping a variant ctor — `List[(String, V)]` — `try_lower_record_list_literal_as`'s
   `ListElemDrop` enum has no case for a tuple whose heap slot is a variant, a DIFFERENT gap
   than the bare-list-of-variant-ctors this fix closes), `compound_eq.almd`/`map_fold_heap_acc.
   almd` (a tuple wrapping a RECORD/nested-Map — same missing-`ListElemDrop`-case family, not a
   generics issue at all), `compound_repr_interp.almd`/`compound_repr_records_interp.almd`
   (different triggers per the earlier bisection — a 3-level `List→Map→List[Option]` nesting and
   a variant-payload-shape-mismatch list respectively, neither a bare `List[<generic variant>]`).
   **15 → 13, TWO closed.**

B118. **Closed `generic_chain_unwrap_or` (13 → 12) — the tuple-wrapped-variant sub-shape B117
   scoped out**: `List[(String, V)]` (`type V = ValInt(Int) | ValStr(String)`, `[("x",
   ValInt(64)), ("general.alignment", ValInt(16))]` — the actual metadata-pairs literal in
   `main`) has no `ListElemDrop` case: the existing `StrInt`/`IntStr` cases require the OTHER
   tuple slot to be scalar (`DropListStrInt`'s render only ever rc_decs slot0, NEVER reading
   slot1 — sound ONLY when slot1 is truly scalar); `V` is a RICH variant (`ValStr` owns a
   String), so reusing `DropListStrInt` would silently LEAK every `ValStr` element's String —
   a genuinely different drop shape was needed, not a gate-widening of an existing one.

   Unlike B117's generic-instantiation gap, `V` here is a plain, NON-generic type — already
   correctly registered in `VariantLayouts` with a real, already-generated `$__drop_V` (no
   shadow-type machinery needed at all, much simpler than B117). Added: (1) a new
   `ListElemDrop::StrVariant(String)` case (binds_p3.rs) gated on `Ty::Tuple([String, T]) where
   T is heap AND NOT `is_flat_heap_tuple_slot` (i.e. genuinely needs recursive drop) — extracts
   the variant's bare name via the existing `custom_variant_type_name`; (2) construction reuses
   the EXISTING general `try_lower_tuple_construct` (already handles arbitrary heap/scalar slot
   mixes, including a ctor-call heap slot via `lower_owned_heap_field`'s existing dispatch — no
   new construction path needed, confirmed by extending the SAME dispatch arm `StrInt |
   IntStr` already used, now `| StrVariant(_)`); (3) a new GENERATED drop function
   `$__drop_list_str_<V>` (drop_sources.rs, appended to the SAME per-rich-variant loop that
   already unconditionally generates `$__drop_list_<V>`/`$__drop_res_<V>` for every rich
   variant — B117 already established this "generate liberally, unused fns are harmless"
   pattern) that, per element: rc_decs the String slot (flat), then recurses into the variant
   slot via the variant's own `$__drop_<V>` (mirrors `map_hval.almd`'s `__drop_list_map_hval`,
   which does the analogous per-element-then-typed-recurse walk for a Map-valued list element —
   the closest existing Almide-SOURCE-level precedent, as opposed to B117's raw-WAT-emission
   precedent).

   **Verified**: an ISOLATED repro (list construction + `list.len` only, bypassing the corpus
   file's OTHER, unrelated function) — wasmtime `3`, v0 native `3`, byte-identical; WAT
   confirmed `$__drop_list_str_V`/`$__drop_V` are REAL non-dangling generated functions (not
   the "admission says yes, nothing generated" trap B117 caught once). A 100,000-iteration
   leak-loop (fresh 3-element list construction every iteration) under a TIGHT 2MB memory cap
   produced the identical accumulated value (300000) on wasmtime and v0 native — no leak, no
   OOM. The ACTUAL corpus file (`generic_chain_unwrap_or.almd`, run via `almide run --target
   wasm --verified` vs plain `almide run`) — both printed `16\n32`, byte-identical — confirming
   end-to-end correctness on the real file, not just the isolated repro. (Note: the corpus
   file's OTHER function, `get_alignment`, still independently walls for an UNRELATED,
   pre-existing reason — "scalar tail outside the value subset… STRICT value mode" from its
   `list.find |> option.map |> option.unwrap_or` chain — so `render_program` on the whole file
   still reports an unlinked-call failure; classify_corpus's per-function tracking does not flag
   this as a NEW walled-real entry since `get_alignment` isn't a tracked test-block/main
   function, and `--verified`'s whole-program fallback to v0 when v1 can't fully link is
   exactly the honest, byte-identical-or-fully-deferred behavior this campaign requires — no
   silent wrong value at any layer, confirmed by the direct comparison above.) `cargo test -q
   -p almide-mir`: 583/583. `classify_corpus`: 13 → 12, exactly ONE closure
   (`generic_chain_unwrap_or`), zero newly-walled (diffed the full 12-name WALLED-REAL list).
   `almide test`: 283/283. GATE OK. CORPUS WALL OK, FORBIDDEN=0. **12, was 13.**

   **Still remaining in this cluster**: `compound_eq.almd`/`map_fold_heap_acc.almd` (tuple
   wrapping a RECORD/nested-Map — the analogous gap for a Record/Map second slot instead of a
   variant, same missing-`ListElemDrop`-case family), `compound_repr_interp.almd`/
   `compound_repr_records_interp.almd` (different triggers — 3-level Map/List nesting and a
   variant-payload-shape mismatch, neither a bare tuple-wrapped case).

DIAGNOSIS — attempted `compound_eq`'s `(<RECURSIVE record>, Int)` tuple case as the RecordInt
   mirror of B118's `StrVariant`, REVERTED (`git checkout --`, zero diff): built the exact
   analogous machinery — a new `ListElemDrop::RecordInt(String)` case (binds_p3.rs, gated on
   `Ty::Tuple([R, scalar])` where `record_or_anon_drop_type_name(R).is_some()`), reusing
   `try_lower_tuple_construct` for construction, and a new generated `$__drop_list_<R>_int`
   (drop_sources.rs, appended to the SAME per-record loop that already generates the bare
   `$__drop_list_<R>`). Built clean. The isolated repro matching `compound_eq`'s ACTUAL literal
   shape (`[({name:"alice",age:30}, 1), …]` — a STRUCTURAL record literal, not `P{name:…,
   age:…}` constructor syntax, even under an explicit `let pairs: List[(P,Int)] = [...]`
   annotation) reproduced EXACTLY the "admission says yes, nothing generated" dangling-call
   trap B117 caught once already this session: `wasmtime: unknown func: $__drop_list_
   anonrec_9fdb9233ebbcebd3_int`. Root cause: the classification gate's `record_or_anon_drop_
   type_name(&tys[0])` call DOES correctly return a name — but for the STRUCTURAL literal's
   own ANONYMOUS record type (an `anonrec_<hash>` synthetic name), not the NAMED type `P` the
   list's declared type carries — `elem_ty` (from `value.ty`) says `Named("P",...)`, matching
   what the drop-registration path used, but the ACTUAL heap value `try_lower_tuple_construct`
   builds for slot0 (via `lower_owned_heap_field` on the bare `{name:…,age:…}` AST literal,
   which the type checker leaves structural per the established "structural record literal"
   precedent — see B108/entry116) is the DIFFERENT anonymous-record-shaped block. My generator
   loop only iterates DECLARED (`rec_names`) record types, never anonymous ones, so
   `$__drop_list_anonrec_<hash>_int` was never emitted. Fixing this needs EITHER (a) a
   B117-style "shadow type" step — synthesize the anon record's fields into a form the `_int`
   generator can ALSO iterate (the existing `collect_recursive_anon_records`/`anon_record_drop_
   name` machinery already generates the BASE `$__drop_anonrec_<hash>`, so extending the SAME
   discovery pass to also emit the `_int`-tuple-wrapped variant is plausible but not attempted),
   or (b) forcing the tuple's record slot to construct AS the named type `P` (mirroring the
   existing `forced_elem` mechanism the bare-record-list case already uses at binds_p3.rs:778-
   789 — but that mechanism currently only threads through a DIRECT record-typed list element,
   not a tuple's inner slot; extending it into `try_lower_tuple_construct` is itself new
   plumbing). Neither was attempted (this fork's remaining budget favored a clean, documented
   stop over a rushed second attempt at a shape that already once required real new
   infrastructure). **`git status` clean, zero diff, 12 unchanged.** `map_fold_heap_acc.almd`
   (structurally the SAME gap — a Map value is internally a paired-slot List, per entry106's
   prior diagnosis — `List[(String, <nested Map>)]`) very likely shares this exact blocker,
   not independently re-verified.

B119. **Fixed two confirmed live wrong-bytes bugs in the bare tail-position Option-`!` path and
   closed `nested_unwrap` (12 → 11).** The pass-through bug B113 scoped out was not confined to
   auto-wrap: (1) a declared-Result fn with a bare tail Option-`!` (`= { let o = some(42); o! }`)
   rendered without walling and printed `Error: ` where v0 prints `42` — the pass-through returns
   the raw Option handle as the "Result" (wrong repr); (2) the scalar-lifted variant
   (`unwrap_option_some()!` from main) emits invalid wasm today (i64/i32 mismatch,
   stash-bisect-confirmed pre-existing). Zero corpus exposure — why parity and classify both
   missed it. Fix: thread an explicit `ret_is_result: bool` through the effect-unwrap desugar
   (the `unit_main` pattern — tree-local `.ty` gating is untrustworthy mid-fixpoint, which killed
   two prior attempts); a new `desugar_tail_effect_unwrap` arm rewrites a bare Option-operand
   `Unwrap` to `match o { none => err("none"), some(v) => ok(v) }` at the synthesized
   `Result[T, String]` (Result operands keep the correct pass-through); `LowerCtx.
   ret_is_result_abi` = strictly-Result declared ∨ AUTO_WRAP; B113's tail-unwrap EXCLUSION
   replaced by an Option-specific INCLUSION (`body_has_tail_position_option_unwrap` — yaml TCO's
   Result-typed `self()!` untouched); and an expression-form gap (a `= <operand>!` body is BARE,
   no Block — the Block gate returned None before any tail machinery ran; also the root cause of
   an `??`-consumer rc_dec trap found during verification). Verified: nine probes byte-identical
   wasmtime-vs-v0; 50k-iteration 3-shape leak-loop under 8MB (2549999 both, no leak); mir
   583/583; classify 12 → 11 zero newly-walled; spec 283/283; GATE OK; CORPUS WALL OK
   (FORBIDDEN=0). **11, was 12.**

B120. **Shipped the `(record, scalar)` tuple-list piece (`ListElemDrop::RecordInt`) — classify
   11 → 10 (`compound_eq` off the lowering-wall list), but NOT a true v1 closure: the file still
   fails to LINK (see below), `--verified` falls back to v0 (md5-verified identical).** The
   B118-DIAGNOSIS blocker resolved by keying all three stages on ONE name: classification via
   `record_or_anon_drop_type_name` of the list's elem `tys[0]`, construction FORCING a structural
   record slot to that same classified type (the `forced_elem` precedent extended into the tuple
   slot), and unconditional `$__drop_list_<R>_int` twins generated in BOTH the named-record and
   anon-record loops (drop_sources.rs) — per element: recurse slot0 via `$__drop_<R>`, slot1
   scalar, free the tuple. Verified: repro byte-identical (2/1), generated
   `$__drop_list_anonrec_<hash>_int` REAL in WAT (the dangling-call trap explicitly checked),
   100k-iteration leak-loop under a 2MB cap (200000 both targets), mir 583/583, classify 11 → 10
   zero newly-walled, spec 283/283, GATE OK, CORPUS WALL OK FORBIDDEN=0.

   **Why neither directive entry truly closes**: `compound_eq::main` now LOWERS (hence off
   classify's list — a metric/reality divergence to keep honest: the collect_map precedent
   refused to count exactly this state) but rendering still walls on NINE missing self-hosts —
   the record-key deep-eq map/set family (`map.from_list/get/len/contains_key_wall`,
   `map.insert`, `map.from_list_hval_wall`, `set.from_list/insert/contains_x`) — record keys
   with a String field need generated per-record deep eq + a keyed map/set family (real new
   infrastructure, the "Map key family" item). `map_fold_heap_acc` still walls at List-arg:
   `List[(String, Map[String,String])]` needs BOTH a StrMapSS drop arm AND — probed standalone —
   `map.from_list` for `Map[String,String]` is itself not self-hosted (`from_list_hval_wall`),
   plus `get_or`/`fold` over the `_str` family; its whole map-literal stack is the missing
   piece, not the tuple case alone. **10 by classify, was 11; v1-true count unchanged.**

B121. **Enabler: self-hosted the first heap-accumulator `list.fold` (`list.fold_ols`,
   Option[List[String]] acc over List[String]) — is_balanced's fold now links; 10 held.**
   `stdlib/list_fold_ols.almd` ((heap,heap)->heap CallIndirect, `list_reduce_str`'s closure
   shape; acc MOVES through f, elements borrowed) + typed routing carved out of the
   `fold_hacc` wall (mod_p4.rs), `"fold"` added to `is_self_host_option_module_fn` (scalar
   folds never reach the variant-gated tracking sites), and a `list.fold`-as-match-subject
   hoist (desugar_match_subject.rs, the fan.map/regex.find precedent — a HOF subject can't
   materialize inline). Verified: identity-lambda fold byte-identical both targets; 50k
   leak-loop under 8MB (fresh Some + fold per iteration) 50000 both, no leak; mir 583/583;
   classify 10 held zero newly-walled; spec 283/283; GATE OK; CORPUS WALL OK FORBIDDEN=0.
   **is_balanced itself remains walled** on its fold LAMBDA's body — a heap-result match
   over an Option[heap] param (`some(stack+["("])` / borrowed-bind move-out / nested
   literal-in-some shapes) — delegated as the next brick. **10 unchanged.**

B121. **Closed `is_balanced` (10 → 9) — the heap-accumulator fold, end to end.** Three composed
   pieces. (1) The fold enabler (parent-built): `stdlib/list_fold_ols.almd` (`(Option[List[
   String]], String) -> Option[List[String]]` via the (heap,heap)->heap CallIndirect), `fold_ols`
   routing, match-subject hoist. (2) The Option[heap] VALUE-match opener
   (`try_lower_option_match_value`, control_p2.rs — the merge-based twin of the Camp-4 Result
   opener, len@4 tag with Option polarity, Some payload = borrowed @12 handle), wired into
   tail.rs + heap_result_arm's Match case; plus two new `OptionSome` payload arms
   (heap-returning Module calls beyond String; ConcatList — both per-arm-bracketed after a
   caught uninitialized-local rc_dec trap from an arm-escaping concat temp). (3) The drop/co-own
   layer: `is_opt_list_str_ty` routes an Option[List[String]] bind/closure-result to the nested
   `DropListListStr` sweep (the flat DropListStr leaked every stack String — probe-confirmed
   OOM under a 4MB cap), admission gates widened to accept the new set (value + statement
   positions), and a REAL latent double-free fixed: `list.drop_end` raw-copied String handles
   un-owned (`__copy_slots`) — sound only while flat drops leaked instead of freeing; under the
   correct nested drop it double-freed. Added `list_drop_end_str` (`__copy_slots_rc` co-own,
   the whitelisted producer) + typed routing. A manual-free attempt inside the fold .almd was
   REVERTED (the lowering already frees each frame's acc at scope end — manual rc_dec
   double-freed; the leak was the closure-result's FLAT drop, fixed by (3) instead). Verified:
   11 probes byte-identical wasmtime-vs-v0 (incl. the corpus fn against all 8 original test
   cases); 100k-iteration leak-loop under 4MB (200000 both targets); mir 583/583; classify
   10 → 9 zero newly-walled; spec 283/283; GATE OK; CORPUS WALL OK FORBIDDEN=0. **9, was 10.**

B122. **Closed `fan_pure_thunks` (9 → 8) — the settle ok-wrap, the race/any sides' B115+B119
   groundwork already in place.** `rewrite_settle_any` (desugar_fan.rs) inlined a PLAIN thunk's
   body into the settle result list RAW while the list's checked element type is
   `Result[Int, String]` — the same phantom-Result contract break B115 fixed on the race side;
   wrapped each non-Result body in `ResultOk` at the element type. Display needed
   `list.to_string_lr` (List[Result[Int,String]] — `stdlib/list_to_string_lr.almd`, registered +
   routed + PURE_MODULES). Verified: corpus file byte-identical wasmtime-vs-v0 (all 6 lines,
   side-effect print order included); settle-shape 50k leak-loop under 8MB (700000 both);
   ladder green on the combined tree (see B123's runs). **8, was 9.**

B123. **Closed `map_fold_heap_acc` (8 → 7) — the nested-map stack: heap-acc `map.fold` pair,
   `map.to_string_ss`, `map.from_list_str`, and the new `map_msv` family.** Six pieces:
   (1) `stdlib/map_fold_hacc.almd` — `map.fold` with a `Map[String, Int]` acc over both
   String-keyed subject families (`fold_str_msi`/`fold_skv_msi`, 3-arity (heap,heap,·)->heap
   CallIndirect, `list.fold_ols`'s acc-moves-through discipline), carved out of the `fold_hacc`
   wall (mod_p4.rs). (2) `map.to_string_ss` (map_to_string.almd) + the (String,String) display
   route. (3) `map.from_list_str` (map_str.almd) + a result-keyed routing arm. (4)
   `stdlib/map_msv.almd` — `Map[String, Map[String, String]]` (new/set/from_list/get_or,
   hval's rc-share discipline; `__drop_map_msv` sweeps each last-ref inner map) + `ListElemDrop::
   StrMapStr` for the pairs literal + `is_map_msv_ty` bind/arg routing. (5) THREE leak/trap
   fixes the 100k-loop discipline caught: map closure results and `Map[String, <scalar>]`
   binds/arg-temps flat-rc_dec'd their key Strings (a LATENT pre-existing leak class — routed to
   the DropListStr key sweep at binds_p2/calls_p4); the from_list recursions leaked every
   intermediate map (rewritten prim-style with owned `let nm` frames); `__drop_msv_inner`
   double-counted map_str's @4 (already the raw slot count) and swept past the block (an
   immediate rc_dec trap). (6) rc-whitelist: `__msv_set_copy`/`__msv_set_append` in
   COOWN_PRODUCERS; drop helpers ride the `__drop_` prefix. Verified: corpus file byte-identical
   (all 5 lines incl. r7 and the init-survival check); 100k combined leak-loop under 4MB
   (500000 both targets) + per-shape loops; mir 583/583; classify 9 → 7 zero newly-walled
   (fan_pure_thunks' closure included — the two shipped together); spec 283/283; GATE OK;
   CORPUS WALL OK FORBIDDEN=0. **7, was 8.**

B124. **Closed `fan_var_thunk_list` (7 → 6)** — the #599 var-bound thunk-list form:
   `fan_bodies` (desugar_fan.rs) now resolves a `Var` arg through a pre-collected map of
   LET-BOUND, never-reassigned `List[() -> _]` literals (all elements no-param lambdas),
   so the SAME race/any/settle inliners run as for the inline form. Sound: VarIds are
   shadowing-free, reassigned vars are dropped from the map, the list's construction stays
   in place (lambdas unevaluated at construction — no duplicated effect), and the desugar
   runs desugar-before-both so the count gate holds. Verified: corpus file byte-identical
   both targets (race=1/settle=2); settle-over-var 50k leak-loop under 8MB via a helper fn
   (100000 both, no leak — a race-in-loop leak-loop is structurally walled by the
   let-`!`-in-while early-return frontier, honest); mir 583/583; classify 7 → 6 zero
   newly-walled; spec 283/283; GATE OK; CORPUS WALL OK FORBIDDEN=0. **6, was 7.**

B124. **Closed `compound_repr_interp` (its `deep` line was the last blocker) — the mlo map
   family.** `stdlib/map_mlo.almd`: `Map[String, List[Option[Int]]]` (split layout, msv's
   value-sweep discipline with Option-block list slots instead of inner-map strings) — new/
   set/from_list + `__drop_map_mlo`/`__drop_list_str_mlo` (pairs literal)/`__drop_list_map_mlo`
   (outer list) + to_string_mlo / list_to_string_lmlo displays composed over list.to_string_lo.
   Wiring: `is_map_mlo_ty` (mod_p2), `ListElemDrop::StrListOpt`/`MapMlo` (binds_p3), bind/
   arg-temp routes (binds_p2 ×2, calls_p4), from_list `_mlo` + two interp cases + a
   `to_string_mlo` pass-through guard (mod_p4), registry + purity. TRAP FOUND: byte-identical
   set fns walled ("unresolvable condition") until `__mlo_set_copy`/`__mlo_set_append` joined
   `COOWN_PRODUCERS` (coown_names.rs) — the name-keyed rc_inc whitelist B123's msv twins are
   in; bisected by rendering the bundle standalone. Verified: `deep` + full corpus file
   byte-identical wasmtime-vs-v0; 100k leak-loop under 4MB (2400000 both); every `$__drop_*`
   confirmed called in WAT; mir 583/583; spec 283/283; GATE OK. Counted with B125 below.

B125. **Closed `crossmod_variant_payload_test` (#484) — ctor `List[<flat variant>]` fields**
   (parent's closure, landed in this combined commit): the VARIANT drop generator's field
   loop gained a `List[<flat variant>]` case (the `__drop_list_str` per-element sweep with the
   List[String] binding-type reinterpretation, mirroring the RECORD generator's precedent),
   and `ctor_list_field_drop_freeable` (binds_p3) now admits `is_flat_variant_ty` element
   types — `Wrapped(List[Policy])`-class ctor args construct exactly when the generated drop
   frees them. Verified (parent): repro byte-identical both targets (`wrap:2`/`tag`);
   `$__drop_Tag` + sweep confirmed real+called in WAT; 100k leak-loop under 4MB (600000 both);
   mir 583/583; the corpus file has no per-function walls. Combined counting for B124+B125:
   6 → 4 by these two closures (`a1c25f0e`'s fan_var_thunk_list closure took 7 → 6 just
   before); zero newly-walled (final list: codegen_effect_fn_test / codegen_loop_guard_test /
   compound_repr_records_interp / playground_default — classify-verified exactly). Spec
   283/283, GATE OK and CORPUS WALL OK (FORBIDDEN=0) all ran on the combined tree.

DIAGNOSIS — `compound_repr_records_interp` decomposes into FOUR independent families (probed
   standalone on the B124-era tree; no code shipped for it this pass): (a) `List[Shape]` with
   MIXED ctor payloads incl. a record-payload variant (`[Circle(1.0), Rect(2,3), Label{text,
   at: Point{..}}]`) — still the List[heap]-literal wall (needs a variant-list ListElemDrop
   case admitting mixed payload shapes + a record-payload ctor materializer); (b) `List[Point]`
   CONSTRUCTS but `${pts}` routes to the unlinked `list.to_string_x` (needs a per-record list
   display — likely a generated or `__repr_rec_<R>`-composed `list.to_string_lrec` family);
   (c)/(d) `Map[String, Point]` and `Map[String, Shape]` both hit `map.from_list_hval_wall` +
   display — each needs a map family (the mlo/msv pattern with record/variant value sweeps).
   Each family is a B-stage-sized piece; (b) is likely smallest (display-only). The file also
   exercises bare record/variant displays (`${Point{..}}`, `${Click(10,20)}`) that the repr
   generators already cover — re-probe before assuming any sub-shape still walls.

B126. **Closed `compound_repr_records_interp` — all four diagnosed families, full-file
   byte parity (25 lines).** (b) container displays: `collect_interp_repr_containers` +
   generated `__repr_list_rec_<R>` / `__repr_opt_rec_<R>` / `__repr_list_<V>` /
   `__repr_map_*` (split layout, quoted keys, `[:]` empty) + `container_repr_name`
   routing with its count-gate mirror; the variant repr generator now admits Float
   fields (`__repr_float` = float.to_string minus trailing `.0`, gated to
   Float-field-bearing programs after unconditional emission linked Dragon4's certs
   into two unit tests) and scalar-record fields (`__repr_rec_<R>` composition). A
   deferred-empty `some(<scalar-only record>)` bind was materialized
   (`materialize_opt_str_some`) after the container display turned the silent empty
   into a visible wrong `none` — construction before display, always. (a) mixed-payload
   variant lists: `is_rich_variant_ty` now takes the caller's record predicate (its
   `|_| false` narrowing made admission lag the generator, which already frees record
   fields). (c)/(d) `Map[String, <record/variant>]`: the desugared literal's
   `from_list` routes to a new value-agnostic `map.from_list_hobj` (msv's split-layout
   co-own family over raw handles; `__hobj_set_copy/append` whitelisted); type-driven
   drops via `map_named_value_drop` → generated `$__drop_map_<V>` (every variant,
   flat→rc_dec / rich→`$__drop_<V>`) and `$__drop_map_rec_<R>` (scalar-only records).
   Verified: all-family probes + the FULL corpus file byte-identical; 100k
   five-shape leak-loop under 4MB (20200000 both targets); every generated fn real
   AND called in the WAT; mir 583/583; spec 283/283; GATE OK; classify on the
   mixed tree shows the file off the list, zero newly-walled. **One caution for the
   record**: reusing `map.from_list_str` for record values was tried first and
   produced GARBAGE field reads (its String deep-copy reinterprets value handles) —
   caught by the byte-parity probe before any ladder step; the hobj family exists
   precisely because construction must be handle-level for opaque values.

B127. **Closed `codegen_loop_guard_test` ("for with guard continue filtering", classify 3 → 2
   on the combined tree with B126)** — the loop early-exit frontier, first slice. Three pieces:
   (1) `desugar_loop_break` (desugar_loop.rs, chained into the shared fixpoint): a `break`
   admitted ONLY as a whole `if`-arm (the `guard c else break` normal form — the iteration's
   remainder is nested in the opposite arm) becomes `{ __bk = true }`; a ForIn guards its body
   on `not __bk` (finite iterable), a While injects the flag into its condition. `continue`
   was already eliminated by desugar_guard's loop rule. (2) `desugar_loop_unwrap` extended to
   WHILE loops (the original for-only gate): flags are injected into the CONDITION via a
   branch-free 0/1 `MulInt` product — the short-circuit `and` lowers to nested IfThen merges
   whose certificate grouping poisons (`flush_branch`'s `{i|}`), and a body-guard alone would
   spin (the induction update lives in the skipped body). (3) mod_p3's in-loop `ResultOk/Err`
   slot reassign now dispatches by repr: a SCALAR-Ok Result built len-as-tag
   (`materialize_result_ok`/`materialize_opt_str_some`) — the cap-tag str builder emitted a
   scalar payload into a handle slot (probe-confirmed invalid wasm that ESCAPED the wall).
   Verified: loop_guard's exact corpus list byte-identical; a pure-`!` while
   (`let d = step(i)!` + err propagation) byte-identical both outcomes; 50k combined
   leak-loop under 8MB (650000 both targets); mir 583/583; classify zero newly-walled,
   zero unbacked; spec 283/283; GATE OK; CORPUS WALL OK.
   **`find_first_even` stays walled BY DESIGN** — a loop VALUE-exit (`guard n % 2 != 0 else
   ok(n)`) is detected (both raw-Guard and desugared-if forms) and DECLINES the pass: every
   delivery shape tried ships one of two pre-existing lower-layer gaps — (a) a heap Result
   slot conditionally reassigned OUTSIDE a loop is silently DROPPED (probe `pick(true)`:
   v0 `ok:42`, v1 `err:normal` — a LIVE wrong-value class, no wall, newly discovered and
   recorded here), and (b) a two-level terminal dispatch makes each nested arm re-release
   the fn-scope `__ev` slot per-path, which the v4 CBranch cannot express (`flush_branch`
   poison → the corpus-wall unbacked breach). Fixing (a) is the honest prerequisite for
   both this and the wrap_lists-class work. **2 remaining: find_first_even, wrap_lists.**

B128. **Closed `wrap_lists` — the "(B) mechanism / loop-slot" frontier (classify: 1 remaining,
   `find_first_even` only).** Four pieces: (1) `try_lower_defunc_record_acc_fold` (defunc_fold.rs)
   — the RECORD-accumulator sibling of the tuple-acc fold: `{ out: List[String], in_ul: Bool }`
   acc, state read via MEMBER projections substituted to slot vars (`substitute_state_members` —
   any other state use declines), interior heap-`if` lets (`opened`) tracked
   materialized+recursive-drop, bare-Record lambda bodies admitted. (2) The old caps objection
   is MOOT on this path — the lambda is C1-inlined (real ops, no elided calls); corpus caps gate
   green. (3) `is_materialized_local_container` widens the heap-result Member/TupleIndex arms to
   MATERIALIZED locals (the borrowed-param Dup discipline, owner = this frame). (4) Variant-CTOR
   arms (`else Para(line)`) now build blocks via `try_lower_variant_ctor` instead of emitting a
   dangling `CallFn $Para`. **A leak the 100k/4MB loop caught before ship**: the result block's
   flat masked drop rc_dec'd the list slot only — element Strings leaked whenever the field was
   BORROWED post-fold (the tuple path's fixture always moved out, hiding it); fixed by routing
   the block's drop through the GENERATED anon/named record drop (last-ref-gated `__drop_list_str`
   sweep, so a moved-out copy stays alive). Verified: playground_default FULL FILE byte-identical;
   6 leak probes 100k×4MB all matching v0; mir 583/583 (a `branch_arm_heap_reassign` failure
   bisected via selective stash to the CONCURRENT loop-fork's WIP, not this work); classify
   1 remaining, zero newly-walled; spec 283/283; GATE OK; FORBIDDEN=0. corpus-wall's UNBACKED
   breach on `find_first_even` reproduces on HEAD+loop-fork-WIP with ZERO of this entry's files
   present — attribution: the concurrent fork's in-progress value-exit work (its own final
   combined ladder gates it).

B128. **Closed `find_first_even` — walled-real (lowering) is ZERO (1 → 0). The ENDGAME
   condition is met: `classify_corpus` reports `walled real (lowering) : 0`.** Two fixes:
   (1) the lp5 LIVE WRONG-VALUE bug (B127's prerequisite (a)): mod_p3's in-frame heap-assign
   elision silently dropped a conditional reassign — `desugar_unit_if_heap_reassign`
   (desugar_branch.rs, in the shared fixpoint) SSA-ifies a Unit `if` reassigning ONE heap var
   into a fresh let-bound value-`if` (`let r' = if c then ok(42) else r`) + substitutes later
   reads, so the value merges through the proven heap-result-`if` machinery (probe `pick`:
   ok:42/err:normal byte-identical, was err:normal/err:normal). (2) the value-exit delivery
   (desugar_loop.rs) is ENABLED — B127's designed machinery unchanged — plus two gaps found
   live: the pre-TCO chain sees RAW `guard c else ok(n)` STATEMENTS (before desugar_guard), so
   `loop_uw_rewrite` gained a Guard-stmt value-exit arm and `expr_has_value_exit` a Guard scan
   (the invisible-Guard fast-path pass-through emitted the machinery but left the exit arm to
   be ELIDED at lowering — an empty-else infinite spin); and the tail-duplicated nested
   dispatch double-released `__ev` on the (vf=0, ef=1) path (rc_dec fault on the error
   string's bytes) — the vx err payload is now an OWNED COPY (`__ev ++ ""`, loop_uw_err_arm's
   trick), removing `__ev` from every arm's parity set. One unit test updated: `branch_arm_
   heap_reassign_is_deferred_and_safe` asserted the OLD elision (the lp5 bug enshrined) — now
   `branch_arm_heap_reassign_ssa_merges_by_value` (both allocs real, ownership verified).
   Verified: all THREE find_first_even outcomes byte-identical (ok:4 / err:no even number
   found / err:not a number: x); 100k combined leak-loop under 4MB (find_first_even ×3
   outcomes + pick per iteration — 2950000 both targets, no leak, no trap); mir 583/583;
   classify `walled real (lowering) : 0`, zero newly-walled; spec 283/283; GATE OK;
   CORPUS WALL OK, KERNEL OK (286s), FORBIDDEN=0. **0. The wall histogram is closed.**

## What NOT to do

- No WAT/Rust regex port into the v1 renderer (invariant 2).
- No "close enough" match semantics — v0 is the oracle, byte-for-byte.
- No opening the untracked-match / interp-in-call-arg buckets here (separate
  lowering work, different skill set — keep this goal stdlib-shaped).
- Do not weaken the purity/drift gates to force a link.
