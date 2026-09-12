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
# Measured 2026-09-12 (local, n=1200, 5 samples after warmup, outputs verified
# identical across all six artifacts):
#
#   native  imperative 1.00   indexed 1.42   enumerate 1.72
#   wasm    imperative 1.00   indexed 0.89   enumerate 0.72
#
# The wasm leg is where the documented spellings now WIN; the native leg still
# pays for the outer capture copy (#2098's mechanism 1, partially fixed). The
# budget sits above the native figure with room for runner noise — it exists to
# catch the return of a 4x, not to hold a decimal place. Lower it when the
# native side lands; never raise it to make a red build green.
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="${ALMIDE_BIN:-target/release/almide}"
BUDGET="${SPELLING_RATIO_BUDGET:-2.5}"
N="${SPELLING_RATIO_N:-1200}"
RUNS="${SPELLING_RATIO_RUNS:-5}"

[ -x "$BIN" ] || { echo "::error::spelling-ratio: no compiler at $BIN (build it first)"; exit 1; }

python3 research/benchmark/perf/spectralnorm/compare.py \
  --almide "$BIN" --n "$N" --runs "$RUNS" --max-ratio "$BUDGET" > /tmp/spelling-ratio.json || {
    echo "::error::spelling-ratio: a spelling exceeded ${BUDGET}x its imperative twin, or the six artifacts disagreed on output"
    python3 - <<'PY'
import json
d = json.load(open("/tmp/spelling-ratio.json"))
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
