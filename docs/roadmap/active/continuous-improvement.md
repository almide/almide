# Continuous improvement ledger

The running record of the standing goal: make Almide the language in which an
AI completes a correct change in few attempts and little time. Correct code
generation first, then change success rate, repair count, diagnostic clarity,
edit→build→test latency, and the speed and memory of generated code. Issue
counts and feature counts are not the measure.

Each session appends a dated entry: what was fixed (written / tested / merged /
released are kept apart), what was measured and how, what is still open, and
the next move. A resuming session reconciles this file against the remote
before acting.

## Quality bar: codopsy grade A

Measured exactly as CI's `commissioned-aviation-quality` job does: codopsy
built from `O6lvl4/codopsy` HEAD, the repo-root `.codopsyrc.json` copied into
the measured directory, `codopsy analyze <dir>`, the `Quality Score:` line read
from stderr. Grades: A ≥ 90, B ≥ 75, C ≥ 60, D ≥ 40. CI gates every workspace
member crate at A; the root package and `crates/almide-edit-belt` are outside
the gate (#3152).

### 2026-10-07 — develop 974c00ea0, codopsy 85ac6cd, macOS 26.3 arm64, rustc 1.96.1

| Target | Score | Gated |
|---|---|---|
| almide-base | A 97 | yes |
| almide-syntax | A 94 | yes |
| almide-types | A 95 | yes |
| almide-lang | A 100 | yes |
| almide-ir | A 93 | yes |
| almide-mir | A 90 | yes |
| almide-dialect | A 99 | yes |
| almide-codegen | A 90 | yes |
| almide-optimize | A 96 | yes |
| almide-frontend | A 90 | yes |
| almide-driver | A 100 | yes |
| almide-interp | A 96 | yes |
| almide-tools | A 92 | yes |
| almide-egg-lab | A 94 | yes |
| almide-layout | A 100 | yes |
| almide-corpus | A 98 | yes |
| almide-rt-core | A 97 | yes |
| almide-wasm | A 90 | yes |
| almide-wasm-run | A 94 | yes |
| almide-wasi | A 93 | yes |
| almide-wasm-vm | A 97 | yes |
| almide-spine | A 94 | yes |
| almide-verify | A 97 | yes |
| root `src/` | **B 86** | no (#3152) |
| root `tests/` | **B 82** | no (#3152) |
| crates/almide-edit-belt | **B 88** | no (#3152) |

Four gated crates sit at exactly 90 with no headroom: almide-mir,
almide-codegen, almide-frontend, almide-wasm. A change to them is judged by its
warning count against develop, not by the rounded grade.

## Log

### 2026-10-07

- **State verified against the remote.**
  - develop is at 974c00ea0. main is at 0bec251be, the merge of PR #3367 "Release v0.67.0-rc1".
  - No v0.67 tag exists. The latest release is v0.66.0 (2026-10-03). So 0.67.0-rc1 is merged to main but neither tagged nor released; the tag is the owner's call.
  - Open bugs: 0. Release blockers: 0.
- **Landed on develop on 2026-10-06** (merged, not released):
  - #3436: #3431, #3433, #3434, #2698
  - #3443: #3435, #3398, #3437, #2659, C-196, #3438, #3439, #3440, #3441
  - #3445: #3442, #3444 items 1 and 3
  - #3447: #2755 buckets, #3446
- **In progress:**
  - root `src/` codopsy to A (#3152), on branch `codopsy-root-src`.
  - Reproduction of five codegen observations from Gramide on 0.62, on the latest build, on branch `gramide-probe`:
    1. a loop-state read hoisted out of the loop
    2. a deep copy into a read-only helper
    3. a list-head read that copies a big structure
    4. quadratic string concatenation
    5. JSON escaping of control characters
- **Ruled 2026-10-07 (argv is an effect), branch `args-effect`:** the `args` readers are `effect fn`s (dialect epoch 13), so #2755's last witness bucket, `caps:argv-in-plain-fn` (8 frames), is gone: the frames are certified as effect frames and the counted exception `ARGV_PLAIN_FNS` is deleted. `process.env` / `process.pid` are the same class of gap for environ and still need their own ruling.

### 2026-10-07 (later)

- **Merged to develop** (not released):
  - **#3458** (batch 15):
    - #3452: LICM miscompile
    - #3456: E096 and dialect epoch 12
    - #3454: in-place string accumulation
    - root `src/` codopsy B 86 → A 93
    - epoch ledger repair: the stray merge marker; the gate now parses the TOML
    - this ledger
  - **#3461** (batch 16):
    - #3453: head reads borrow
    - #3455: scoped let-bound closures
    - the Trust Spine caps fix
- **Trust Spine had been red on develop since batch 14** (974c00ea0). The workflow is not required, so nothing blocked on it.
  - **Cause:** the fan offer host op 74 had no row in `op_caps`. It read as the unknown-op sentinel, so three fan fixtures certified by #3447 failed [caps]. `proofs/structural-wall.sh` is the only script that checks caps over certified fixtures, and it was not in the batch-14 checks.
  - **Fix:** op 74 maps to no capability, because the call graph already counts the chunk fn it runs. `proofs/gate.sh` adds a drill that puts the sentinel back and must be rejected.
  - **Result:** `make verify-trust` went from 926/929 to 929/929.
  - **Lesson:** for any change to certification, also run `make verify-trust`.
- **Batch 17** (this PR): #3459 + #3460, #3462, #3463.
  - **#3462:** in a `fan { }` arm, `!` ends that arm with its Err. Every arm still runs and the lowest-index Err wins.
    - Before: native failed rustc, and wasm left the function at the failing arm.
    - The `spec/wasm_cross` fixture was withdrawn rather than seeding `walled-real-baseline.txt` (MIR declines it). The evidence is `tests/fan_arm_bang_test.rs`, on both targets.
  - **#3463:** on wasm, a fan Err inside a non-main effect fn is now returned instead of aborting. A tail fan or `let r = fan` also runs every arm now, on wasm as on native.
    - Evidence: `tests/fan_block_propagate_test.rs`, 13 cases on both targets, including one alloc-balance case.
- **Follow-ups filed:**
  - #3464: `fan.settle { }` arm `!` gives a misleading E022. Decision taken: accept it scoped to the arm, as the `fan.settle(xs, f)` mapper form already does. Flagged for the owner.
  - #3465: latent MIR `!` scope and arm-skipping.
  - #3467: I-divergence. A typed-error arm `!` skips later arms on wasm and gives invalid Rust on native.
  - #3468: native invalid Rust for a block arm in a tail fan.
  - #3464, #3467 and #3468 are in progress on branch `fix-fan-arms`.
- **`almide check` speed (#3466):** develop is about 10% slower than v0.66.0 on gramide's `src/cli.almd` (83.5 vs 75.5 ms, median of 25 interleaved runs).
  - Build profile ruled out: a non-incremental build like release.yml times the same.
  - About 2.2 ms is the E008 concurrent-reach analysis (first bad commit be61ee66f).
  - The remaining ~5% is creep that a threshold bisect cannot pin down. It needs a sampling profiler; xctrace hung here.
- **codopsy:** every crate these batches touched stays A with an unchanged warning count: almide-wasm 90/47, almide-codegen 90/32, almide-frontend 90/36.

## Edit loop on a real project: O6lvl4/gramide 0.2.11

gramide has 19 files and 8,106 lines of Almide. It is measured on a copy (`git archive HEAD`) with one line added: `import self.lex` in `src/keystrokes.almd`.
- That line is required since dialect epoch 10 ("names-resolve-where-declared", 0.67.0). Without it, develop refuses `lex.LexError` with E029 by design; the downstream canary named gramide.
- The fix in gramide itself belongs to its owner.

Harness:
- Every run uses a fresh `TMPDIR` and no project `.almide/`, so "clean" means the compiler's own caches are empty. The Cargo registry and the toolchain are warm.
- Measured with `/usr/bin/time -l`: wall-clock time and max RSS of the whole command.
- macOS 26.3 arm64. develop is 974c00ea0; the comparison is the v0.66.0 release binary (819bbc74f).

`almide test`: all 10 files run on the wasm lane ("9 via WASM, 0 via native fallback"), so this does not measure native builds. 3 runs each:

| Step | develop | v0.66.0 |
|---|---|---|
| clean | 0.66–0.68 s | 0.63–0.68 s |
| cached | 0.67–0.71 s | 0.64 s |
| comment-only edit | 0.66–0.70 s | 0.61–0.71 s |
| one-line code edit | 0.67–0.71 s | 0.64–0.69 s |
| max RSS | 434–474 MiB | 423–449 MiB |

`almide build src/cli.almd` (native, through cargo), 2 runs each:

| Step | develop | v0.66.0 |
|---|---|---|
| clean | 3.89–5.08 s | 3.58–4.27 s |
| cached | 0.36–0.39 s | 0.33–0.34 s |
| one-line code edit | 0.90–1.09 s | 0.85–0.86 s |
| max RSS (clean) | 388–392 MiB | 367–373 MiB |

`almide check src/cli.almd`: 0.09 s on develop, 0.07 s on v0.66.0.

Reading:
- develop is not faster than 0.66.0 on this loop.
- It is about 3–10% slower in every row, and its RSS is about 5% higher.
- The spread is close to the run-to-run noise at 2–3 runs, so the gap is recorded as a lead to investigate, not a confirmed regression.
- The first single run showed 1.24 s for an edit; repeated runs did not reproduce it.

## Gramide observations from 0.62, re-checked on 2026-10-07

Builds compared: 0.62.0, 0.66.0 (release 819bbc74f) and develop 974c00ea0.
- Native probes went through the standard codegen path, because the verified render walls on record types.
- Wasm probes on develop went through the structural leg; none hit an E082 wall or fell back.

| # | Observation | develop native | develop wasm | Action |
|---|---|---|---|---|
| 1 | loop-state read hoisted out of the loop | **wrong** (`0,0,0` vs `1,2,3`) | correct | #3452 (I-miscompile, release blocker); fix in progress on `fix-3452` |
| 2 | read-only helper deep-copies its argument | the cross-module and `list.slice` shapes are fixed (#2164, #3397); a let-bound closure capturing a parameter still copies | no copy | #3455 |
| 3 | list-head read copies the element | `match list.get` and `xs[0]` are fixed (#2070); `??`, `list.first`, `[h, ..]` and `option.map` still clone | no copy | #3453 |
| 4 | `s = s + piece` quadratic | var accumulator fixed (#3404); field assign and interpolation still quadratic | linear | #3454; fix in progress on `perf-3454` |
| 5 | JSON control characters unescaped | fixed (f461733d3, 0.65.1) | fixed | pinned by `json_stringify_control_chars.almd` (C-095) and `json_quote_matrix_test.rs` |

#3404's var-accumulator move now has its own String pin: the `a_string_accumulator_moves_into_its_own_concat` test, on branch `gramide-probe` (c5569a604).

`almide check` on gramide's `src/cli.almd`, 20 interleaved runs each: develop 80 ms median, v0.66.0 74 ms. That is about 8% slower, and outside the noise. It is a lead to profile; `ALMIDE_TIME_PHASES` covers `almide run` only.

## Root package codopsy (#3152), 2026-10-07

- **`src/`: B 86 → A 93.** Branch `codopsy-root-src` (fa947c150), 12 structural refactors. Warnings went from 52 to 11; the info count is unchanged at 48.
  - No rc change, no exclusion, no suppression.
  - Output is identical to the develop binary across the emitted Rust, the wasm bytes, the test/fmt/check transcripts and all `--help` text.
  - **Side effect:** a one-time native cache miss, because the split cargo_build files now join the hashed build recipe.
- **`tests/`: B 82, unchanged.** The weighted base is 97.4, but the issue-density penalty is at its cap of 15 with 2010 issues.
  - 1536 of those issues are `no-unwrap` in integration test files, which codopsy does not recognise as test code: it exempts only a module named `tests`.
  - **Correction (2026-10-07, measured):** removing the unwraps alone does not reach A, because the density penalty stays capped once total issues exceed about 330.
    - Measured on a copy of develop's `tests/` with variants of the repo rc: as is **B 82**; `no-unwrap` off **B 85**; `no-unwrap` and `no-println` off **A 90**, exactly at the boundary.
    - So rewriting about 1500 unwraps (to `expect`, which codopsy does not match) tops out near 85.
  - **Prior art** (almide-references):
    - clippy's `is_in_test` is a `#[test]` fn or a `#[cfg(test)]` item, never a path (`clippy_utils/src/lib.rs`).
    - Its `allow-unwrap-in-tests` and `allow-print-in-tests` are opt-in and default to false.
    - Gleam and wasmi lint non-test targets only.
    - codopsy's own CI scores `./src` only.
    - Recognising `#[test]`/`#[cfg(test)]` in codopsy would correct a misclassification. Exempting `tests/` by path, or exempting println, goes beyond clippy and changes the criterion. Both are the owner's call.
- **crates/almide-edit-belt: B 88** (a Lean 4 project). Not started.

