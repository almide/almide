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
- **Open, waiting on an owner ruling:** #2755's last witness bucket, `caps:argv-in-plain-fn` (8 frames). It needs a decision on whether argv is ambient or an effect; #848 closed without one.

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

