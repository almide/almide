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
#
# PROFILE LEG (#3604). The "--release is within noise" reading above is an
# Apple-silicon reading (rustc targets apple-m1 there). On Linux, generic
# aarch64, --release is SLOWER than the default profile for every spelling —
# LLVM's loop vectorizer, costed for a generic core, splits `eval_a`'s i64
# multiply across NEON lanes. A ratio between spellings cannot see a tax all
# spellings pay, so this leg compares each spelling with ITSELF across the two
# profiles (`compare.py --profile-pair`), round-robin in one run. Measured
# 2026-10-12 on GCE, n=1200, 9 samples, release/default:
#
#   x86_64  (Xeon 8581C)    imperative 1.00   indexed 0.91   enumerate 1.00
#   aarch64 (Neoverse-V2)   imperative 1.43   indexed 0.81   enumerate 1.22
#
# opt-level 3 + LTO slower than opt-level 1 is a pessimization whatever its
# cause. The aarch64 budget is a RATCHET at today's figure, not the goal: the
# goal is <= 1.05 on both arches (#3604). Lower it as #3604 lands.
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="${ALMIDE_BIN:-target/release/almide}"
BUDGET="${SPELLING_RATIO_BUDGET:-2.0}"
# Which legs run: `spelling` (the #2098 ratio, both targets) and `profile`
# (the #3604 release/default ratio, native only). The aarch64 CI job runs only
# `profile`: it has no wasmtime, and the spelling leg already runs on x86_64.
LEGS="${SPELLING_RATIO_LEGS:-spelling,profile}"
case "$(uname -m)" in
  aarch64|arm64) RELEASE_DEFAULT_BUDGET=1.50 ;;
  *)             RELEASE_DEFAULT_BUDGET=1.10 ;;
esac
RELEASE_BUDGET="${SPELLING_RELEASE_BUDGET:-$RELEASE_DEFAULT_BUDGET}"
N="${SPELLING_RATIO_N:-1200}"
RUNS="${SPELLING_RATIO_RUNS:-5}"

[ -x "$BIN" ] || { echo "::error::spelling-ratio: no compiler at $BIN (build it first)"; exit 1; }

if [[ ",$LEGS," == *",spelling,"* ]]; then
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
fi

if [[ ",$LEGS," == *",profile,"* ]]; then
status=0
python3 research/benchmark/perf/spectralnorm/compare.py --profile-pair \
  --almide "$BIN" --n "$N" --runs "$RUNS" --max-release-ratio "$RELEASE_BUDGET" \
  > /tmp/spelling-profile.json || status=$?
python3 - "$status" "$RELEASE_BUDGET" "$(uname -m)" <<'PY'
import json, sys
status, budget, arch = int(sys.argv[1]), sys.argv[2], sys.argv[3]
try:
    d = json.load(open("/tmp/spelling-profile.json"))
except (OSError, ValueError):
    # An output divergence exits before any JSON is written and says so on stderr.
    print(f"::error::spelling-ratio (#3604): the profile pair disagreed on output or did not build ({arch})")
    raise SystemExit(1)
verdict = "OK" if status == 0 else "FAILED"
print(f"spelling-ratio profile leg: {verdict} — {arch}, release/default budget {budget}x, "
      f"n={d['n']}, {d['runs']} samples, outputs identical")
for name, s in d["spellings"].items():
    print(f"  {name:11} default {s['default_ms']:8.2f} ms  release {s['release_ms']:8.2f} ms  {s['ratio']:.2f}x")
if status:
    print(f"::error::spelling-ratio (#3604): a spelling's --release build ran slower than "
          f"{budget}x its default build on {arch}")
raise SystemExit(1 if status else 0)
PY
fi
