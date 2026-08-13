#!/usr/bin/env bash
# EDIT-LOOP SCALE-INVARIANCE RATCHET (#1334).
#
# The README publishes `almide check` at 22.7ms on a 268-line file, and the
# roadmap's axis A promises "the edit loop is scale-independent". Those are two
# different claims: a fast check on a small file is ALSO what a quadratic
# compiler produces. Nothing watched the second claim until this gate.
#
# What it measures: `almide check` over a ladder of nested prefixes of this
# repo's own stdlib (300 hand-written modules, ~30k lines, byte-sorted, copied
# into a scratch project that imports them all — see research/benchmark/editloop/scale.py
# for why the stdlib and not spec/, and why nothing is synthesized). Rung 0 is
# the entry alone: the constant floor every check pays regardless of project
# size. The rungs reach 2k / 5k / 10k / 20k / 30k lines; 10k is the roadmap's
# headline size.
#
# WHAT IS GATED, and why those two quantities and not the milliseconds.
#
# The obvious gate — "p95 of `almide check` at 10k lines must stay under 100ms" —
# is the one measurement on this ladder that CANNOT be gated on a shared runner.
# Same commit, same corpus, same M4 Pro, one hour apart (2026-08-13):
#
#     load average  6.0   ->  10k-line rung: p50  51.4ms  p95   52.9ms
#     load average 42.4   ->  10k-line rung: p50 184.3ms  p95  332.4ms
#
# A 6x swing in the published statistic with the compiler untouched. That is the
# same trap #1337 documented for the `listbuild` perf row (1.58 on the dev box,
# 0.91 on the CI runner): an absolute anchor would sit far off its band on one
# machine and flake there. So the p50/p95 are MEASURED, PRINTED and RECORDED in
# the baseline file as dated observations — they are the number the roadmap
# publishes — and what is ANCHORED is two dimensionless same-run ratios:
#
#   slope   log-log least-squares slope of MARGINAL check time against project
#           lines across the ladder. 1.0 = linear (doubling the project doubles
#           the marginal check time), 2.0 = quadratic. This is THE scale-
#           invariance number: it is what tells a compiler that is fast because
#           it is linear apart from one that is fast because the file was small.
#           Computed from the MIN of the interleaved runs — the least-
#           contaminated observation, the same correction check-perf-ratio.sh
#           applies to its idiom relation. Measured stability across the load
#           swing above — eight 40-run measurements, load average 3.4 to 42.4,
#           read 1.113 to 1.159 (median 1.132): a 4.1% spread, while p95 on the
#           same ladder moved 6x over the same swing.
#
#   ratio   min(10k-line rung) / min(floor rung): what a 10,151-line project
#           costs in units of what a 4-line project costs. Dimensionless, same
#           binary, same machine, same run, so machine speed cancels. The slope
#           alone would NOT catch a uniform slowdown of the per-line work (every
#           rung 40% slower leaves the slope where it was); this ratio does.
#           Measured 4.298 to 4.454 over load 3.4-7.6, and 4.690 at load 42.4 —
#           an 8.9% total spread.
#
# Both move only on purpose: a regression fails, and a real durable win lowers
# the baseline in the SAME change with the reasoning in the commit — the same
# policy as scripts/perf-ratio-baseline.txt. Both bands are two-sided: a slope
# or ratio far BELOW baseline means the ladder stopped measuring (the corpus
# collapsed, the check became a no-op) rather than that the compiler got good.
#
# NOT covered here: `almide run` at scale. `run` shells out to rustc and the
# linker, so its cost at 10k lines is rustc's scale story, not the frontend's,
# and the composed-stdlib corpus does not survive codegen (duplicate intrinsic
# symbols) even though it type-checks clean. The cold/warm build rows in
# docs/benchmarks/build-speed.txt are the watch for that path; removing rustc
# from the debug path is roadmap 0.66-0.67.
set -euo pipefail

# Byte-order collation, pinned: `sort`'s last-resort comparison follows the ambient
# locale, so an unpinned sort produces different output on differently-configured
# machines. #1031 caught docs/roadmap/README.md changing row order with no content change.
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

BASELINE_FILE="scripts/edit-loop-scale-baseline.txt"
RUNS="${EDIT_LOOP_RUNS:-40}"
# Percent above baseline each anchored quantity may drift before this fails.
# The slope band is the tighter one because the slope is the more stable
# statistic (0.6% observed spread vs 3.6% for the ratio).
SLOPE_BUDGET_PCT="${EDIT_LOOP_SLOPE_BUDGET_PCT:-15}"
RATIO_BUDGET_PCT="${EDIT_LOOP_RATIO_BUDGET_PCT:-25}"

# ── blindness floors (#976: a gate that measures nothing reads green forever) ──
# The corpus is discovered by glob, so it can shrink without anyone noticing.
# These are properties of the INPUT, independent of the compiler under test.
CORPUS_FILE_FLOOR=250      # 303 stdlib modules composed 2026-08-13
CORPUS_LINE_FLOOR=25000    # 30,624 lines in the top rung
TEN_K_LINE_FLOOR=9000      # 10,151 lines in the headline rung
# And one property of the MEASUREMENT: under this, wall-clock is process-spawn
# noise rather than compilation. A durable 3x win re-anchors this on purpose.
TOP_MIN_MS_FLOOR=40

# Which binary the ladder times — `ALMIDE_BIN`, the same hook bench.py and
# check-semantics-manifest.sh take, so CI can point it at the downloaded build
# artifact. It doubles as this gate's self-verification hook: point it at a shim
# that adds a delay proportional to lines^2 and the `slope` anchor fires, which
# is the only way to demonstrate that the gate detects real superlinearity
# rather than baseline arithmetic. Recorded in proofs/gate-verification.toml.
ALMIDE_BIN="${ALMIDE_BIN:-$PWD/target/release/almide}"

OUT=$(mktemp -t editloop-scale.XXXXXX.json)
trap 'rm -f "$OUT"' EXIT

python3 research/benchmark/editloop/scale.py --runs "$RUNS" --almide "$ALMIDE_BIN" --out "$OUT"

python3 - "$OUT" "$BASELINE_FILE" "$SLOPE_BUDGET_PCT" "$RATIO_BUDGET_PCT" \
    "$CORPUS_FILE_FLOOR" "$CORPUS_LINE_FLOOR" "$TEN_K_LINE_FLOOR" "$TOP_MIN_MS_FLOOR" <<'PY'
import json, sys

out_path, baseline_path = sys.argv[1:3]
slope_budget, ratio_budget = float(sys.argv[3]), float(sys.argv[4])
file_floor, line_floor = int(sys.argv[5]), int(sys.argv[6])
ten_k_floor, top_min_floor = int(sys.argv[7]), float(sys.argv[8])

d = json.load(open(out_path))
rungs = d["rungs"]
floor, top = rungs[0], rungs[-1]
ten = next((r for r in rungs if ten_k_floor <= r["lines"] <= 12000), None)

fatal = []
if d["corpus"]["files"] < file_floor:
    fatal.append(f"corpus shrank to {d['corpus']['files']} modules (floor {file_floor}) — the "
                 "ladder is measuring a smaller project than the baseline was anchored on")
if top["lines"] < line_floor:
    fatal.append(f"top rung is {top['lines']} lines (floor {line_floor}) — same")
if ten is None:
    fatal.append(f"no rung landed in the 9k-12k headline band; rungs were "
                 f"{[r['lines'] for r in rungs]}")
if top["min"] < top_min_floor:
    fatal.append(f"top rung min {top['min']:.1f}ms is under the {top_min_floor:.0f}ms floor — "
                 "at that scale wall-clock is process-spawn noise, so either the corpus "
                 "collapsed or `check` stopped doing the work. A real 3x win re-anchors "
                 "this floor on purpose.")
if fatal:
    for f in fatal:
        print(f"::error::editloop-scale: {f}")
    sys.exit(1)

slope = d["slope_min"]
ratio = ten["min"] / floor["min"]

print(f"editloop-scale: headline rung {ten['lines']} lines — "
      f"p50 {ten['p50']:.1f}ms / p95 {ten['p95']:.1f}ms "
      f"(min {ten['min']:.1f}ms, {d['runs']} interleaved runs, load {d['loadavg'][0]:.1f}) "
      "— REPORTED, not anchored: this statistic tracks the machine, not the compiler")

try:
    baseline = {}
    for line in open(baseline_path):
        line = line.strip()
        if line and not line.startswith("#"):
            k, v = line.split()
            baseline[k] = float(v)
except FileNotFoundError:
    with open(baseline_path, "w") as f:
        f.write("# edit-loop scale-invariance anchors (see scripts/check-edit-loop-scale.sh)\n")
        f.write(f"slope {slope:.3f}\nratio {ratio:.3f}\n")
    print(f"editloop-scale: no baseline; wrote {baseline_path}")
    sys.exit(0)

missing = {"slope", "ratio"} - set(baseline)
if missing:
    sys.exit(f"::error::editloop-scale: baseline has no entry for {sorted(missing)} — "
             "an anchored quantity was added without anchoring it; add the line on purpose.")

failed = False
for name, value, budget, floor_frac in (("slope", slope, slope_budget, 0.85),
                                        ("ratio", ratio, ratio_budget, 0.75)):
    base = baseline[name]
    ceiling = base * (1 + budget / 100)
    band_floor = base * floor_frac
    verdict = "ok"
    if value > ceiling:
        verdict = f"OVER ceiling {ceiling:.3f}"
        failed = True
    elif value < band_floor:
        verdict = f"UNDER floor {band_floor:.3f} (ladder broke, or a durable win — re-anchor on purpose)"
        failed = True
    print(f"editloop-scale: {name:6s} {value:.3f} (baseline {base:.3f}, +{budget:.0f}% budget) {verdict}")

if failed:
    print("::error::editloop-scale: the edit loop left its scale band. `slope` over 1.3 means")
    print("`almide check` is going superlinear in project size — the usual cause is a pass that")
    print("rescans an accumulating table per module (import resolution, UFCS dispatch, the")
    print("checker's module env) instead of indexing it. `ratio` over its ceiling with `slope`")
    print("flat means the per-line work got uniformly more expensive. Reproduce locally:")
    print("  python3 research/benchmark/editloop/scale.py --runs 40")
    print("If the change is intentional, move scripts/edit-loop-scale-baseline.txt in the SAME")
    print("change with the reasoning in the commit message.")
    sys.exit(1)
PY
