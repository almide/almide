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
| 10 | corpus mutant: `assert_eq(sql, "hello")` in main | **native raw panic (exit 101) vs wasm exit 1** — the assert abort form is un-normalized outside test blocks (ALS-T6 family) |
| 49 | C-138 fixture mutant | native 101 vs wasm 134 — BOTH legs leak raw abort forms |
| 119 | C-062 RawPtr fixture mutant | native 1 vs wasm 134 (trap) — the unsafe-bridge OOB form needs adjudication |
| 5 | `int.clamp(4, 3, 1)` (min > max) | native panics (Rust clamp assert), wasm returns a value — the clamp domain edge needs a T6 adjudication |
| 98 | C-002 Int8-overflow mutant | native build failed AFTER check accepted (check-vs-build gap) |
| 145 | `or_else(ok(..), (a) => ok(..))` | an UNCONSTRAINED err-type var (`Result[String, Unknown]`) reaches codegen: native tolerates, wasm refuses — an ACCEPTANCE-PARITY gap (check should adjudicate, not the emitters) |
| 149 | `ok(result.unwrap_or(.., none))` over `Result[Option[Float], ..]` | wasm run fails while native succeeds — nested Option-in-Result payload |

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
