#!/usr/bin/env bash
# SPELLING RATIO GATE (#2098).
#
# The docs tell writers to prefer combinators over `var` + `for` and recursion
# over `var` + `while`. When that advice costs 4.25x (spectralnorm's
# `enumerate |> map |> fold` on wasm, measured 2026-09-11), nothing sees it:
# the contract ledger gates OUTPUT equivalence and the perf ratchets gate a
# program against ITSELF across targets, so a tax that every spelling of a
# program pays equally is invisible to both.
#
# This gate compares SPELLINGS of one program on ONE leg, in one round-robin
# run. That ratio cancels the machine the way the native/Rust ratio does, so a
# slow shared runner changes the numbers and not the verdict.
#
# Measured 2026-09-13 (local, n=1200, 5 samples after warmup, outputs verified
# identical across all six artifacts):
#
#   native  imperative 1.00   indexed 1.43   enumerate 1.01
#   wasm    imperative 1.00   indexed 1.14   enumerate 0.91
#
# It builds with the DEFAULT profile, which is what a plain `almide build`
# ships (opt-level 1). At `--release` every ratio is within noise of 1.00 —
# `compare.py --release` prints that reading — so the native indexed figure
# above is what LLVM declines to do to an iterator chain at opt-level 1, not a
# tax the lowering emits. The default profile is the one gated because it is
# the one a user gets without asking, and because a LOWERING regression shows
# at both levels anyway; `--release` is not the faster build here (it is slower
# for the two spellings opt-level 1 already handles well), so neither column is
# a recommendation — see the corpus README.
#
# The budget sits above the worst figure with room for runner noise — it exists
# to catch the return of a 4x, not to hold a decimal place. Lower it as the
# measurements improve; never raise it to make a red build green.
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="${ALMIDE_BIN:-target/release/almide}"
BUDGET="${SPELLING_RATIO_BUDGET:-2.0}"
N="${SPELLING_RATIO_N:-1200}"
RUNS="${SPELLING_RATIO_RUNS:-5}"

[ -x "$BIN" ] || { echo "::error::spelling-ratio: no compiler at $BIN (build it first)"; exit 1; }

python3 research/benchmark/perf/spectralnorm/compare.py \
  --almide "$BIN" --n "$N" --runs "$RUNS" --max-ratio "$BUDGET" > /tmp/spelling-ratio.json || {
    echo "::error::spelling-ratio: a spelling exceeded ${BUDGET}x its imperative twin, or the six artifacts disagreed on output"
    # An output divergence exits before any JSON is written and says so on
    # stderr; only the ratio verdict leaves a table to print.
    python3 - <<'PY'
import json
try:
    d = json.load(open("/tmp/spelling-ratio.json"))
except (OSError, ValueError):
    raise SystemExit(0)
for target, spellings in d.get("targets", {}).items():
    for name, s in spellings.items():
        print(f"  {target:7} {name:11} {s['median_ms']:8.2f} ms  {s['ratio']:.2f}x")
PY
    exit 1
  }

python3 - <<'PY'
import json
d = json.load(open("/tmp/spelling-ratio.json"))
print(f"spelling-ratio: OK — n={d['n']}, {d['runs']} samples, outputs identical")
for target, spellings in d.get("targets", {}).items():
    row = "  ".join(f"{n} {s['ratio']:.2f}x" for n, s in spellings.items())
    print(f"  {target:7} {row}")
PY
