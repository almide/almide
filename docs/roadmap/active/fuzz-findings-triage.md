<!-- description: Fuzz (nightly) is red — triage the differential findings to zero and re-green the workflow -->
# Fuzz Findings Triage: Re-green the Nightly Differential Gate

Fuzz (nightly) has been red since 2026-07-15. The generative differential
fuzzer (`tools/xtarget-fuzz`) is finding REAL native ⇄ wasm observable
divergences — the exact class the cross-target contract ledger claims cannot
happen silently. Every night it stays red, the byte-identical claim and
reality drift further apart.

## Reproduction

```bash
cargo build --release --bin almide          # the oracle binary
cd tools/xtarget-fuzz && cargo build --release && cd ../..
tools/xtarget-fuzz/target/release/xtarget-fuzz replay --seed 1784352208133210990 --index <i>
tools/xtarget-fuzz/target/release/xtarget-fuzz gen    --seed 1784352208133210990 --index <i>  # source
```

All 12 findings from the 2026-07-18 run REPRODUCE on the current develop
(25585249) — none were incidentally fixed by the #782 wall burn-down.

Note: the same workflow's coverage-ratchet job (#566) is also failing and
needs its own look (job logs via `gh api /repos/almide/almide/actions/jobs/<id>/logs`;
`gh run view --log` returns empty for this repo).

## Findings (seed = 1784352208133210990)

| Class | Index | Symptom | Status |
|---|---|---|---|
| C. String fn returns `""` | 323, 768, 904 | `ok(float.to_fixed(…))` → wasm `ok("")`; `result.map_err` on heap-Ok Result | **FIXED (2026-07-18)** — two root causes: (1) the ok/err ctor's stdlib-call payload fell to the deferred Opaque (binds_p4 Module-call String arms, C-138); (2) the result value combinators linked the len-as-tag scalar impls over the cap-as-tag heap-Ok block (`_h` twins + `_x` walls in result_call_name, C-139) |
| D. Unicode predicate flips | 191 | `none` vs `some("Ǆ")` (titlecase) | **FIXED** — same root as C (the value flowed through a Result/Option ctor payload) |
| E. i32-boundary tuple | 609 | `(true, -2147483648)` vs `(false, 2)` | **FIXED** — same root as C |
| A. Negative-zero display | 67, 655 | native `-0` / wasm `0` | **FIXED (2026-07-18)** — not display: the v1 self-host `float.round` branched on `x >= 0.0` (TRUE for -0.0 under IEEE) and lost the sign; copysign carries it (C-140) |
| G. Build/run failures | 65 (wasm run fails), 96 (wasm build fails) | divergent failure | **FIXED** — 65: `list.zip_with` linked the Int-typed impl for every instantiation; String zips trapped on the scalar closure table type → element-repr routing + `_str` twin (C-141). 96: the v0 emitter's `result.unwrap_or_else` inline lacked the F64 case → invalid module; added, mirroring the option twin (C-142) |
| F. Option flips | 858 | `some("5")` vs `none` | **FIXED (2026-07-18)** — `some/ok/err(<heap if>)` payload fell to the deferred Opaque and read `none`; the ctor piece matches now route If/Match String payloads through the heap-result-if machinery (C-143) |
| B. List collapses to `[]` | 198, 659 | `[1000000, 7, 256]` → `[]`, `[true,true,false,true]` → `[]` | **FIXED** — four mechanisms: (1) a non-literal scalar-list bind now WALLS instead of deferring to the silent-`[]` Opaque (C-144); (2) mono-suffixed stdlib names (`or_else__Int_String_String`) route by base name (C-145); (3) String-err Result captures are admitted to the closure env (the `__drop_list_str`-exact layout family) so the capturing or_else chain runs v1-verified; (4) the v0 lifted closure returning a captured alias now hands out a co-owned +1 (C-146) |

**Seed 1784352208133210990: 12/12 findings CLEAN (2026-07-18).**

## Wave 2 (seed 20260718, 1000-program campaign): 8/8 resolved

| Index | Class | Resolution |
|---|---|---|
| 0 | NativeBuildFailure | Generated run projects self-isolate with an empty `[workspace]` table — running almide inside any cargo workspace dir (the fuzzer's .scratch) made cargo resolve the parent workspace and refuse the build |
| 9 | RunFailureDivergence | `list.unique_by` routed by KEY repr: `_sk` twin (prim byte-compare content equality) for String keys, `_x` otherwise (C-147) |
| 198 | Hang | Harness fix: a native hang is a finding only if wasm CLEANLY SUCCEEDS — a wasm OOM-trap at the 4GB ceiling is not termination evidence (`pos + 0` mutation hangs both). Pure classifier + unit tests in ladder.rs |
| 248 | ok("") | Ctor Var payloads Dup instead of move — `let a = ok(r0); let b = ok(r0)` both real, r0 stays live (C-150) |
| 259 | WasmBuildFailure | v0 `list.scan` acc store was i64-fixed → valtype三分岐; v1 `list.scan_str` twin + ACC-repr routing (C-148) |
| 590 | float garbage | v0 result.unwrap_or_else heap-Ok payload use-after-free → share +1 (C-149) |
| 647 | some(garbage) | `result.map/map_err/flat_map` with heap-Ok RESULT (scalar input) → deterministic `_x` wall (C-151) |
| 888 | err→ok(0) | Ctor over an un-admitted heap call payload WALLS (bind-net extension, C-152; nested-Result drop admission = F2 follow-up) |

Passing harness/emitter fixes: v0 emit_result_call gained real `or_else`/`flatten`
arms (the named-dispatch fallback ICE'd on pipelines without the lowered runtime
fn — found by the host-determinism gate, which is now 262/262 byte-identical).
walled-real baseline gained a DESIGNED-PROBE section (result_wall_escalations::
main pins C-152's wall). v1 sweep baseline: PASS 288 / WALL 12 (9 permanent + 3
by-design fixture probes) / INVALID·TRAP 0.

## Wave 3 (seed 20260718 re-campaign after the classifier fixes): 7 unique / 1000

The intended-abort classifier fix (compile failures stay findings; runtime
aborts flow to the 3-point comparison) UNMASKED the abort-form divergence
class the old "any native non-zero = NativeBuildFailure" rule hid:

| Index | Shape | Class |
|---|---|---|
| 10 | corpus mutant: `assert_eq(sql, "hello")` in main | **FIXED (2026-07-18)** — ALS-T18: non-test assert failures now desugar ONCE in frontend lowering (if + eprintln + process.exit(1)) so all four consumers inherit `Error: assertion failed…` + exit 1, operands once-evaluated (C-153, 3 fixtures). In passing: the bare `eprintln` builtin was unlinked on v1 / an ICE on v0 — now a registered self-host (fd-2 print_str twin) + a shared parametrized v0 runtime fn |
| 49 | C-138 fixture mutant | native 101 vs wasm 134 — BOTH legs leak raw abort forms |
| 119 | C-062 RawPtr fixture mutant | native 1 vs wasm 134 (trap) — the unsafe-bridge OOB form needs adjudication |
| 5 | `int.clamp(4, 3, 1)` (min > max) | **FIXED** — ALS-T6 adjudication: `Error: clamp requires min <= max` + exit 1 on all four consumers; float's `!(lo <= hi)` folds NaN bounds into the same line (C-154, 2 fixtures) |
| 145 | `or_else(ok(..), (a) => ok(..))` | **FIXED** — the E025 undecidable-slot validator now sees INTERMEDIATE call results (every call-result ty enqueued at inference; the post-solve check fires only on genuinely unpinned slots). Both targets now reject at check with span + hint — acceptance parity restored; the fuzzer classifies GeneratorReject |
| 149 | `ok(result.unwrap_or(.., none))` over `Result[Option[Float], ..]` | **FIXED** — option/result `unwrap_or`'s heap arms hand out CO-OWNED (+1) refs on BOTH branches (kept payload + Var default), the #727 share family's unwrap_or edition (C-149 extended, new fixture) |
| 98/92 | C-002 Int8-overflow mutant (`neg_one_i8(128)`) | **FIXED** — E024's call-arg edition: a bare int literal flowing into a SIZED param is range-checked at check time (the context-recording hook now CREATES sites for i64-fitting literals under sized contexts and is wired at the call-arg inference point). check now rejects what native rustc rejected — the check-vs-build gap closed; the fuzzer classifies GeneratorReject |

Also fixed in passing: the **coverage-ratchet job's red** (the other #795 job) was
`find -perm +111` — BSD syntax GNU find rejects; now the portable `/111`
(proofs/coverage.sh). Wave-3 tail (campaign vs the batch-5 binary, 1000
programs): index 92 (check-vs-build), 995 (a JSON nbsp/emsp key DISPLAY
divergence — new shape), 998 (native exit 1 vs wasm trap 134 — an abort-form
leak). Interp gained Flow::Exit (process.exit) so the assert desugar evaluates
in the 3-way oracle.

Wave 3 is a different arc from waves 1–2 (instance lowering bugs): it is
mostly ABORT-FORM NORMALIZATION (raw 101 panics and 134 traps leaking where
ALS-T6 promises `Error: …` + exit 1) and ACCEPTANCE PARITY (unresolved type
vars reaching codegen). The clamp/RawPtr edges need normative adjudications
in the ALS before fixes.

Loop-until-dry status: wave-3 triage open. Remaining DoD: findings-free
1000-run + coverage-ratchet job diagnosis + two consecutive green nightlies.

Lesson feeding #777/F3: BOTH C-class roots were "a deferred/mis-linked value
reaching observed output without a wall" — (1) the deferred-Opaque ctor payload
printed as `ok("")`, (2) a name-keyed registry link ignored the layout the type
implies. The F3 gate should make each structural: an Opaque that flows into a
display/eq/observed op must wall the fn, and a self-host link must carry a
repr-compatibility check (the `_h`/`_x` suffix discipline, mechanically).

## Wave 4 (2026-07-28..30 nightly findings — the first post-revival campaigns, Unit 0.42)

The 0.41 instrument revival made the campaign complete every night; these are the findings
the completed nights recorded. Replayed and classified 2026-07-31 on develop e610331d
(macOS; check/build-stage verdicts are host-independent).

| # | Night / seed | Kind | Symptom | Status |
|---|---|---|---|---|
| 861 | 7/28, 1785217538023450905 | OutputDivergence | native `r15 = 100`, wasm `r15 = 0` — `result.unwrap_or_else(err, closure returning captured Float)` | **RESOLVED — attributed (B2)**: `020021df` ("Let a closure capture any scalar by reading is_heap_ty… so Float and every sized int width stop walling the lift"). The finding's closure `(s14) => r12` captures a FLOAT; before the commit the lift walled and the HOF ran with a missing closure producing the zero-filled result (wasm `r15 = 0`), after it the capture lifts and both targets read 100. Mechanism-exact match; earlier 7021f11f suspicion retracted |
| 535 | 7/28, same seed | NativeBuildFailure | check accepted `let m: Int8 = --9223372036854775808`; rustc then rejected | **RESOLVED — attributed (B2)**: `6ac44503` ("Reach the int literal through its whole paren and unary-minus chain so a narrow annotation ranges it and the net sign decides"). C-173 (18f96604) was ALREADY in the 7/28 night's compiler but could not see through the DOUBLE unary minus; the chain walk with the net sign is exactly what rejects `--9223372036854775808` at E024. Mechanism-exact match |
| 57 | 7/29, 1785304212462799529 | RunFailureDivergence | mutated C-044 fixture: `factorial(n - -2147483648)` — native terminates (LLVM's accumulator tail-recursion elimination turns the ~4-billion-deep non-tail recursion into a loop that wraps to ≤ 1 and unwinds); wasm recurses faithfully and traps `call stack exhausted` at ~16k frames | **CONTRACTED (2026-07-31, M2 #1017 option 1)** — C-196: call-stack exhaustion is a resource limit outside the observable-behavior promise. Convergent-boundary fixture `recursion_depth_within_limits.almd` (byte-identical at depth 1000); the oracle's `one_sided_stack_exhaustion` rule (unit-tested ×4) classifies the class as a skip, mirroring the both-legs rule; replay now reads SKIPPED naming C-196. The depth-guard normalization (same T6 abort depth on both targets) is the recorded strengthening follow-up |
| 12 | 7/29, same seed | WasmBuildFailure | The wall named the outermost symptom ("If of Bool with a call-bearing arm"); minimization (m1–m9) proved the ROOT is elsewhere: the nested-map literal `Map[String, Map[String, Bool]]` never materializes. The msv route (`map_heap_val_nested_route`) admits only inner `Map[String, String]`, and for good reason — `$__drop_map_msv` sweeps ALL 2m inner slots as Strings, so an inner scalar-value map would be freed as garbage handles (accept-but-unsafe). The if-condition chain above it then declines all the way up to the statement wall | **FIXED (2026-07-31)** — the designed msb extension landed: `__drop_map_msb` / `__drop_list_str_msb` key-sweep drops in `map_msv.almd`, `is_map_msb_ty` type routing at both drop-registration sites + the pairs-literal `StrMapSkv` classifier arm, and the msv from_list/get_or gates widened to inner scalar-value maps (the impls are handle-generic; only the drop is type-specific). Replay CLEAN; churn spec test exercises the drop on the wasm leg. Known honest residual: `len`/`contains`/`set` on the OUTER nested map still wall (not needed by this finding) |
| 29 | 7/30, 1785389912282950207 | WasmBuildFailure | unlinked stdlib call `map.get_or_str_wall` — the String-valued `map.get_or` variant was not in the self-host registry | **FIXED (2026-07-31)** — `map_get_or_str` twin in `stdlib/map_str.almd` (hit arm deep-copies per the str family's copy discipline, miss arm returns default per the hval branch shape), registry entry, and `get_or` admitted to the `_str` routing list in `mod_p4_e.rs`; replay CLEAN, spec test added |

### Wave 4 additions (the loop-until-dry tail)

Round-2 local campaign (2026-07-31, 1,283 programs / 1,093 clean / 165 raw finding events
deduped to 2 unique classes):

| # | Seed | Kind | Symptom | Status |
|---|---|---|---|---|
| L2 | 1785460667454423000 idx 5 | WasmBuildFailure | The wall message ("if over an unresolvable condition (Call of Bool)") is a MISREPORT — the second instance of the #904 arm-decline-named-as-condition trap. Delta-debugging (p1–p8, g5a–g5f) proved a THRESHOLD interaction: the full program walls, but EVERY single simplification passes — drop r6, simplify r1's unwrap-over-nested-if chain, replace the r4 alias with r1, shorten the 5-element list, or replace the 1e-300 denormal default with 0.5. No single construct is the culprit; some lowering budget/interaction (suspected: the elided-call/caps class — the #848 lane) declines only on the combined shape | **FIXED (2026-07-31)** — the decline chain was named by two new instruments (the cond-side trace and the discarded module-call reason trace), and the root turned out to be an ABSENT MATCH ARM: `str_list_literal_elems_lowerable` had no arm for an `If` element, so `["a", (if r2 then "b" else "d")]` fell to `_ => false` → the whole literal declined → Opaque → the honest reject. The "threshold" mystery was CONST-FOLD: every simplified variant's `if` collapsed at optimization, so only the full program ever reached the missing arm. Fix: admit the String-result literal-arm `if` element (cond = tracked scalar Var or Bool literal — the shape the proven `try_lower_heap_result_if` always lowers, honoring the no-mid-build-decline guard) in both the pre-check and the element builder. Replay CLEAN; `spec/lang/list_literal_if_element_test.almd` (runtime conds so fold cannot mask the path) on the wasm leg; mir 601/0 |
| L3 | 1785460667454423000 idx 54 | OutputDivergence vs the REFERENCE INTERPRETER | A mutated C-182 fixture (Float32/Float64 negated-literal context typing): native and wasm agree EXACTLY (7 lines, `-2.5 … 123456792.0 -1.5`) but BOTH disagree with almide-interp — either a shared-lowering bug (the class the third judge exists to catch) or an interp Float32-model gap. The replay does not print the interp's expected output — a fuzzer reporting gap to fix in passing | **FIXED (2026-07-31)** — the INTERP was the wrong judge: `let p: Float32 = 123456789.12345679` kept its f64 spelling because the interp never narrowed Float32-typed literals at birth (the widened-carrier convention narrows at the float32.* bridge, but a literal never crosses it). Both backends correctly fold to f32 (`123456792.0`). Fixed in eval.rs literal evaluation (`as f32 as f64` when `ty == Float32`); the reporting gap fixed in passing — the finding summary now names the first differing line (`first_line_diff`, unit-tested ×2); fixture `float32_literal_excess_precision.almd` added under C-182 so the 3-way gate holds the class forever. Replay CLEAN, interp tests 56/0 |

| # | Source / seed | Kind | Symptom | Status |
|---|---|---|---|---|
| L0 | 2026-07-31 local B4 campaign (707 programs, 607 clean), seed 1785458401504935000 index 0 | WasmBuildFailure | wall: "map.fold with an unliftable/closure-list higher-order argument cannot execute faithfully in this brick (walled, not mis-valued)". Minimized (t3): `map.fold` with a HEAP-map accumulator and a map-literal-returning closure | **FIXED (2026-07-31)** — the key insight: `Some(s)` for Option[String] IS a 1-element DynListStr (`materialize_opt_str_some`), so Option[String] values follow the same "len@4-counted String slots" discipline as the skv inner maps — the msb drops serve them VERBATIM. The fix is pure type-gate widening: `is_map_msb_ty`, the `StrMapSkv` pairs classifier, and the two msv routes now admit inner `Option[String]`. The fold needed nothing — `map.fold_skv_hacc` (any-heap-acc over a skv subject) already existed, and the closure lifted once its literal body lowered. Replay CLEAN; spec tests (get_or some/none/miss, fold-into-mso-acc, churn) on the wasm leg. Known residual cell: `??` over an mso `get_or` result lacks read-shape seeding (falls back honestly; recorded, not needed by L0) |
| L1 | found while minimizing L0 (probe t1) | Checker hole (ICE, both targets) | `map.fold`'s closure argument is under-checked for a SCALAR accumulator: an ill-typed body (`Int + String` via the (k,v,acc) order mistake) passes `almide check` and dies at IR verify. list.fold and the concrete-heap-acc map.fold case both reject correctly | **FILED** — [#1018](https://github.com/almide/almide/issues/1018), assigned to the 0.52 hole-hunt row (loud, not silent; not a nightly-red risk — the generator only emits well-typed closures) |

Round-3 campaign (2026-07-31, 1,304 programs / 1,108 clean / 163 raw → 1 unique):

| # | Seed | Kind | Symptom | Status |
|---|---|---|---|---|
| L4 | 1785464182375979000 idx 0 | WasmBuildFailure | `Map[String, Result[Int, String]]` values (get_or / remove) feeding `result.unwrap_or` — the STRICT-mode "scalar binding outside the value subset" refusal | **FIXED (2026-07-31)** — the recorded edit list landed verbatim: the family inner set centralized as `is_msv_family_inner` (inner String-keyed maps, Option[String], and Result[T,String] with T scalar or String — every "len@4-counted low-32 String handles" block, tag-safe under the `$rc_dec (param i32)` wrap), the three routes (get_or / remove / from_list) unified on it, `is_map_msb_ty` + the pairs classifier widened, and the NEW `map_remove_msv` (set_copy's discipline with one slot skipped) registered. One extra trap re-confirmed from the issue-sweep ledger: an rc_inc-using helper must ALSO join the `coown_names.rs` whitelist or it renders unlinked. Replay CLEAN; spec tests (ok/err get_or, remove, churn) on the wasm leg; mir 601/0 |

Round-4 campaign (2026-07-31, 1,271 programs / 1,072 clean / 3 unique):

| # | Seed 1785466162321453000 | Kind | Symptom | Status |
|---|---|---|---|---|
| L5 | idx 985 | RunFailureDivergence | Mutated C-045: `for i in 0..<4294967295 { list.push(ys, i*2) }` (~34 GB of pushes). Native completes in its 64-bit address space; wasm32 hits the linear-memory ceiling and dies with an OOB memory fault at exactly the memory-size boundary (0x80010000) — the allocator runs PAST the end after a failed grow instead of aborting cleanly | **FIXED + CONTRACTED (2026-07-31, M2 #1019 option 1 complete)** — three layers: (1) `$oom` fires on a refused `memory.grow`; (2) the LAYER-2 ROOT was an **i32 frontier overflow** — a single ~2.1 GB request made `$bump = p + n` wrap PAST 2^32, the wrapped bump skipped the grow check, and the block's writes ran off the memory end at exactly the boundary (the observed 0x80010000 fault). Fixed with the unsigned wrap guard (`new < old → $oom`) in `$alloc`/`$alloc8` plus the cap×8 mul-wrap guard in `$list_new`; the probe now prints the defined `Error: out of memory` + exit 1; (3) C-197 landed (197 active / 0 flagged, convergent fixture `allocation_within_limits.almd` byte-identical) with the oracle's `one_sided_memory_exhaustion` skip rule (unit-tested ×3, fuzzer 26/0), mirroring C-196's stack rule |
| L6 | idx 33 | WasmBuildFailure | The same "List argument cannot be faithfully materialized" class — the next list-element cell outside the admitted set (element kind TBD from the source) | **LIVE — classified**: a `List[Option[Map[String, Bool]]]` literal (`[some(["k0": true, …]), some(n1), …]`) chained into `list.get_or` with an Option-map default. The composition BREAKS the "len-counted String slots" discipline — a `some(map)` block's slot 0 is a MAP handle, so the flat rc_dec leaks the map's keys at last-ref; the drop needs payload ROUTING. The rails exist: `materialize_opt_aggregate_some` + the `optrec:<drop_fn>` route (`Op::DropWrapperRec`) already do exactly this for `Option[record]` — the fix is an opt-of-map sibling (payload drop_fn = the msb/skv map sweep) + the list-element admission + `list.get_or` on the element type. IMPLEMENTATION DESIGN (probed q1/q2 — the wall is the explicit nested-ownership gate in `try_lower_bind_heap_fresh_scalar_list`, binds_p2_b.rs): (1) a static 3-level drop in map_msv.almd — `__drop_list_omb` / `__drop_list_omb_loop`: per element, if the option block (1-slot DynListStr) is last-ref and `len@4 == 1`, route the slot-0 MAP payload through `__drop_msb_inner`, then rc_dec the block; (2) construction — a new `ListElemDrop::OptMapSkv` classifier arm (Tuple→Option[Map[String,scalar]] elements) + an element lowering that materializes `some(<map literal|tracked var>)` via the opt-aggregate block (map handle moved/Dup'd into slot 0) and `none` as the 0-len block, registered as drop `"list_omb"`; (3) `list.get_or` admission for the element type (the heapelem hshare route); (4) fmt'd spec tests incl. a churn. REFINED (second pass): `List[Option[String]]` / `List[Option[Int]]` literals ALREADY build — the machinery is the CtorFlat/CtorLenLoop path, gated by `lenlist_elem_class` (repr_sources_d.rs), which admits only "one-level-exact" payloads (scalar → Flat; String / flat scalar list → LenLoop via the shared generated `$__drop_list_lenlist`). Map payloads are excluded exactly because they need an interior sweep — and the msb insight COMPOSES: the len-loop discipline is recursive (option block len-slots → map len-slots → Strings), which is what the static `__drop_list_omb` expresses. Do NOT widen `lenlist_elem_class`/`$__drop_list_lenlist` in place (it would change the shared drop for existing users); add the separate `OptMapSkv` class → drop `"list_omb"`, decided BEFORE the lenlist arm in binds_p3_b's classifier (~line 243). The element payload materialization (some(map-literal)) must route the map through its from_list machinery — verify `try_lower_option_ctor`'s aggregate-payload piece reaches it, else extend the piece. **FIXED (2026-07-31, fourth pass)** — the brick turned out SMALLER than spec'd because two pieces already existed: map literals desugar to `map.from_list` calls, and the opt-ctor's computed-map piece arm (`is_str_int_map_ty` = any heap-key/scalar-value map) already materializes them (probe q6). What landed: (1) the static 3-level `__drop_list_omb` (list slots → option len-slot → the msb key sweep — the len-loop discipline COMPOSES); (2) `ListElemDrop::OptMapSkv` decided before the lenlist arm, routing to the ctor element path with drop `"list_omb"`; (3) a NEW link-trap discovered and fixed: static drops render from DropVariant ops which the demand-linker's CallFn scan cannot see — a program whose only demand on map_msv.almd is a DROP left `$__drop_list_omb` dangling; the linker now forces the module when any msv-family static-drop op is present (the value_core precedent). i33 replay CLEAN, spec tests (literal + get_or + churn) on the wasm leg, mir 601/0. [#1020](https://github.com/almide/almide/issues/1020)'s STANDALONE `let o = some(map_var)` bind exposure (flat drop registration, order-dependent) remains open — list-held options are now covered, the bare bind is not | 

Nightly 2026-07-31 (run 30608326062, head at 05:59 UTC — includes every fix through the B2
attribution): the campaign COMPLETED (full-budget streak 4/14) and recorded 2 findings —
| # | Seed 1785477905242041784 | Kind | Status |
|---|---|---|---|
| N1 | idx 93 | NativeBuildFailure: `literal out of range for i32` | **FIXED (2026-07-31)** — the declared tuple-variant payload type is now pinned onto literal ctor args in the call-arg loop (ctor calls carry no call_sig, so no hook ever saw the position). One trap pinned in the regression test: a capitalized ctor callee parses as TypeName, not Ident — the first fix matched Ident only and silently never fired. Both repros now E024; in-range payloads stay accepted; frontend + literal-domain (8/8) + spec/lang (159) green |
| N2 | idx 9 | WasmBuildFailure (the condition-wall misreport shape again) | **FIXED (2026-07-31)** — the earlier "discipline does NOT hold" call was WRONG: `try_lower_tuple_construct` builds tuples as `DynList {{ len = slot count }}` with heap slots stored as handles, so an all-String tuple IS the len-counted discipline verbatim (len=2, both slots String handles) and `__drop_msb_inner` frees it exactly. N2 reduced to pure gate widening: `is_msv_family_inner` + `is_map_msb_ty` + the pairs classifier admit all-String tuple values. Replay CLEAN, both-target probe identical, spec tests (get_or hit/miss + churn) on the wasm leg, mir 601/0. **Wave 4 live count: 0** |

Strategic note (rounds 1–4): each ~1,300-program campaign surfaces 1–3 NEW cells of ever-deeper type composition (map values → option values → result values → option-of-map elements). The hand-listed family/drop registry grows a cell per finding; the mechanical end-state is the completeness-by-construction arc (type-derived drop routing). Until then the grind is the design — and the streak-based DoD absorbs it.
| L7 | idx 757 | NativeBuildFailure | Check accepted a Float32 literal that rustc rejects: `error: literal out of range for f32` — the float sibling of the 535 integer-literal-domain class (C-173 was integer-only) | **FIXED (2026-07-31)** — E024's float twin: a `FloatOverflowSite` queue (pre-filtered to literals finite as f64 but infinite as f32) + the annotated-binding context pin (`float_literal_chain` through paren/unary, mirroring the int hook — a bare literal's own solved type stays `Float`, so the Float32 context lives on the binding) + the post-solve validator. Replay now GENERATOR REJECT with E024; regression test `float32_range_is_checked_and_precision_is_not` (range rejected both signs, excess-precision and plain-Float accepted); frontend + literal-domain + spec/lang all green. Amusing provenance: the fuzzer found this by mutating the C-182 fixture added THIS session |

Round-5 campaign (2026-07-31, 1,385 programs / 1,198 clean — every Wave 4 + nightly fix aboard):
2 unique, BOTH new classes (no fixed cell regressed).

| # | Seed 1785481464003870000 | Kind | Symptom | Status |
|---|---|---|---|---|
| P1 | idx 961 | **OutputDivergence — the silent-wrong-value class** | `bytes.read_f32_le(verts, dst + 9223372036854775807)` (an i64::MAX offset that overflows the index math): native reads `z=0.0`, wasm reads `z=2.0` — i.e. an OUT-OF-BOUNDS byte read is not equal-by-construction across targets. HIGHEST priority of the two: the bounds discipline for `bytes.read_*` is the memory-safety surface, and the wasm leg returning a neighbouring value where native returns 0.0 means the read is unchecked (or checked differently) on one leg | **FIXED (2026-07-31)** — no contract decision needed: the documented behavior ("out of range → +0.0") was already normative, and BOTH legs simply failed to enforce it at an overflowing offset. Root: the guard `pos + k > n` — at `pos = i64::MAX` the add wraps NEGATIVE, the check passes, and the read proceeds (wasm returned a neighbouring value; native's address math happened to land on zeros). Ordinary out-of-range offsets were always correct — only the overflow band leaked. Fixed FAMILY-WIDE by the API-completeness rule: all **14** sites in `bytes_core.almd` rewritten to the overflow-safe `pos > n - k` (a length is non-negative and ≤ 2^31, so `n - k` cannot overflow). Replay CLEAN; regression test `bytes reads are +0.0 at an overflowing offset` covers f32/f64/u32/u16/i32 plus the unchanged in-range and ordinary-OOR answers; spec/stdlib 112 files green |
| P2 | idx 27 | WasmBuildFailure (scalar-element list literal, element outside the subset) | The element is `map.get_or(list.get_or([<map literals>], 1, ["k0": true]), "ΑΒΓ", false)` — a `List[Map[String, Bool]]` LITERAL fed to `list.get_or`. The map-VALUE matrix is now broad, but a list whose ELEMENTS are maps is the next composition frontier (the L6 shape one level out: option-of-map → bare map elements) | **LIVE** — same rails as L6: an element-drop class routing each element through the msb key sweep |

## Definition of done

1. Every finding minimized (`gen` → delta), root-caused, and either FIXED
   (with a `spec/wasm_cross/` fixture + contract entry per the ledger rules)
   or converted to an HONEST WALL (never a silently wrong value).
2. Class C's wall-coverage hole closed structurally — feed the mechanism
   into the #777 tracking-set/wall-consistency gate design.
3. The coverage-ratchet job failure diagnosed and fixed (or the floor
   re-justified in its own commit, per the #566 discipline).
4. A local `xtarget-fuzz run --count 1000` campaign is findings-free, then
   Fuzz (nightly) is green two consecutive nights.

## Ownership boundary

The fuzzer itself (generator, oracle ladder, delta-debugger) lives in
`tools/xtarget-fuzz` and is NOT the subject of this stream — only its
findings are. A fuzzer bug discovered during triage (e.g. a misclassified
verdict) gets fixed in passing with its own test.
