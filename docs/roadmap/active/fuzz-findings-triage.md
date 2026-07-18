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
| B. List collapses to `[]` | 198, 659 | `[1000000, 7, 256]` → `[]`, `[true,true,false,true]` → `[]` | open — next up |
| F. Option flips | 858 | `some("5")` vs `none` | open |
| A. Negative-zero display | 67, 655 | native `-0` / wasm `0` | open — display layer |
| G. Build/run failures | 65 (wasm run fails), 96 (wasm build fails) | divergent failure | open |

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
