#!/usr/bin/env bash
# NATIVE RUNTIME PERF RATCHET (#917).
#
# The compiler's whole native performance story is "compiles to Rust with no
# runtime tax" — and until this gate, nothing watched it: the perf suite in
# research/benchmark/perf/ had sources but an empty results/ dir, and every
# published number was a stale v0-era code comment. This is the watch.
#
# What it checks: the RATIO of Almide-native runtime to a handwritten-Rust
# reference compiled with the same rustc flags, per benchmark, on the same
# machine in the same run. Absolute times on shared CI runners are noise;
# the ratio cancels the machine. The workload is bench.py's --quick set and
# the gated pairs are the same-shape references (nbody_unrolled, not the
# array-based nbody ref — that one Almide legitimately beats, and a gate on
# a "we're faster" number would fire on the runner's vector unit, not on a
# compiler regression).
#
# Budget: PERF_RATIO_BUDGET_PCT (default 40) percent above the committed
# baseline in scripts/perf-ratio-baseline.txt. Wide on purpose — runner noise
# is real; a genuine codegen regression (a clone in a hot loop, a lost
# in-place list write) shows up as 2-10x, not 1.2x. In the mold of
# check-embedded-size.sh the gate fails in BOTH directions: a ratio below
# 50% of baseline means the benchmark or the reference broke (bench.py's
# output-equivalence check catches wrong answers, not stopped clocks) — a
# real durable win lowers the baseline in the SAME change.
#
# Regressing on purpose (a bounds-check made semantics right, a new safety
# net costs 5%): raise the baseline in the same change, with the reasoning
# in the commit. The number stays a reviewed decision instead of a drift.
set -euo pipefail

# Byte-order collation, pinned: `sort`'s last-resort comparison follows the ambient
# locale, so an unpinned sort produces different output on differently-configured
# machines. #1031 caught docs/roadmap/README.md changing row order with no content change.
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

BASELINE_FILE="scripts/perf-ratio-baseline.txt"
BUDGET_PCT="${PERF_RATIO_BUDGET_PCT:-40}"
RUNS="${PERF_RATIO_RUNS:-3}"
# Gated pairs: bench -> same-shape rust-ref variant.
#
# The three `listbuild` rows are deliberately NOT here. Their almide/rust ratio
# turns out to be strongly ARCHITECTURE-dependent — the ratchet's founding
# assumption, "the ratio cancels the machine", does not hold for a workload
# whose cost is allocation rather than arithmetic. Same commit, same day:
# `listbuild` reads 1.58 on an M4 Pro and 0.91 on the ubuntu-latest CI runner
# (where Almide BEATS the reference, glibc calloc handing back zero pages that
# `Vec::with_capacity` + push has to fault in). A per-row anchor would sit ~16%
# off the two-sided floor on one of the two machines and flake there, and
# re-anchoring per architecture is not something a single committed baseline
# can express. So the rows are MEASURED and REPORTED below (same policy the
# README states for onebrc), and what is gated is the relation between them —
# which is the property #1337 is about and which IS machine-stable: 1.018x on
# the M4 Pro, 1.045x on the CI runner, from the same commit.
PAIRS="nbody=rust:nbody_unrolled spectralnorm=rust:spectralnorm fasta=rust:fasta fft=rust:fft"
# Rows measured for the record and printed, but not anchored (see above), as
# `bench=rust-ref-variant`.
#
# `strchurn` (#1004) joins them under the same rule and for the same reason:
# 75% of its almide/rust delta is malloc/memcpy/free of N owned `String`s, so
# it is an allocator comparison first and a codegen comparison second, and this
# repo has one architecture's reading of it. Its reference is deliberately
# `rust:strchurn`, the SAME-SHAPE/SAME-SEMANTICS one (owned `String`s out of
# split, `chars().count()` for len) — `rust:strchurn_idiomatic` is 1.9x faster
# but every bit of that spread is the stdlib's API contract, and watching it
# would make a language design decision look like a compiler regression.
# Promoting this row to PAIRS wants a second architecture's number first; note
# that unlike `listbuild` both sides here allocate identically, so it may well
# turn out to be anchorable. See research/benchmark/perf/string-gap-1004.md.
REPORTED="listbuild=rust:listbuild listbuild-append=rust:listbuild listbuild-comb=rust:listbuild strchurn=rust:strchurn"
# IDIOM GATE (#1337). The three listbuild rows build the SAME result three
# ways, so beyond each row's own ratio there is a relation between them that
# the mission depends on: CLAUDE.md and docs/CHEATSHEET.md tell authors (and
# the models we measure MSR on) to prefer `list.range |> list.flat_map` over
# `var` + `for`, and a recommended idiom that is slower than the loop it
# replaces teaches generated code onto the slow path. It WAS slower — 1.67x,
# all of it one heap allocation per element for `flat_map`'s intermediate
# list. The ceiling below is what keeps that closed.
#
# Measured as (comb / its own rust ref) / (append / its own rust ref), on the
# MIN of each variant's runs rather than the median. Both rows time the SAME
# reference binary, so dividing each side by its own reference cancels
# whatever load hit that row's window; the min is the least-contaminated
# observation. Both corrections are needed: a run where an unrelated build
# landed on the box during the comb row read 1.30x on raw medians and 1.07x
# this way, against a clean-run 1.02x. The tighter-than-40% ceiling is
# affordable BECAUSE of that normalization — this is a same-machine,
# same-run, same-reference relation, not an absolute time.
IDIOM_CEILING=1.15
# Medians under this many seconds are process-spawn noise, not measurement.
MIN_SECONDS=0.08

out=$(mktemp -t perf-ratio.XXXXXX.json)
trap 'rm -f "$out"' EXIT

python3 research/benchmark/perf/bench.py \
  --quick --runs "$RUNS" --legs native,rust \
  --bench nbody,spectralnorm,fasta,fft,listbuild,listbuild-append,listbuild-comb,strchurn \
  --label ratchet --out "$out"

# ABLATION LEG (#1466): the same anchored benchmarks with the IR optimizer's
# perf passes disabled (ALMIDE_DISABLE_OPT skips fold/DCE/propagate; the
# lowering enablers and the #872 correctness re-fold stay). The gated number
# is the DELTA ablated/optimized per bench — a same-machine, same-run ratio,
# so it anchors where absolute times cannot. MEASURED FINDING at introduction
# (M4 Pro + CI runner agree): the deltas sit at ~1.00 on every anchored
# bench — on the rustc-backed native leg these passes are SUBSUMED by LLVM,
# which runs the same folds downstream. The gate therefore holds the honest
# band: ablation must never SLOW the build's output past noise (floor —
# the optimizer must not COST), and a delta leaving the band upward is a new
# real earning that gets re-anchored on purpose, exactly like the main rows.
abl_out=$(mktemp -t perf-ratio-abl.XXXXXX.json)
trap 'rm -f "$out" "$abl_out"' EXIT
ALMIDE_DISABLE_OPT=1 python3 research/benchmark/perf/bench.py \
  --quick --runs "$RUNS" --legs native \
  --bench nbody,spectralnorm,fasta,fft \
  --label ratchet-ablated --out "$abl_out"

python3 - "$out" "$BASELINE_FILE" "$BUDGET_PCT" "$PAIRS" "$MIN_SECONDS" "$IDIOM_CEILING" "$REPORTED" "$abl_out" <<'PY'
import json, sys

out_path, baseline_path, budget_pct, pairs_arg, min_s, idiom_ceiling, reported_arg, abl_path = sys.argv[1:9]
budget = float(budget_pct)
min_s = float(min_s)
idiom_ceiling = float(idiom_ceiling)
pairs = dict(p.split("=", 1) for p in pairs_arg.split())
reported = dict(p.split("=", 1) for p in reported_arg.split())

data = json.load(open(out_path))["results"]
ratios = {}
for bench, ref in pairs.items():
    variants = data[bench]["variants"]
    native = variants[f"{bench}/native"]["median"]
    rust = variants[f"{bench}/{ref}"]["median"]
    for name, sec in ((f"{bench}/native", native), (f"{bench}/{ref}", rust)):
        if sec < min_s:
            sys.exit(f"::error::perf-ratio: {name} median {sec}s is under the {min_s}s floor — "
                     "the workload shrank below what wall-clock can measure; grow the "
                     "QUICK_ARGS entry instead of gating on spawn noise.")
    ratios[bench] = native / rust

try:
    baseline = {}
    for line in open(baseline_path):
        line = line.strip()
        if line and not line.startswith("#"):
            k, v = line.split()
            baseline[k] = float(v)
except FileNotFoundError:
    with open(baseline_path, "w") as f:
        f.write("# almide-native / handwritten-rust runtime ratio per benchmark\n")
        f.write("# (bench.py --quick medians; see scripts/check-perf-ratio.sh)\n")
        for k, v in sorted(ratios.items()):
            f.write(f"{k} {v:.3f}\n")
    print(f"perf-ratio: no baseline; wrote {baseline_path}")
    sys.exit(0)

missing = set(pairs) - set(baseline)
if missing:
    sys.exit(f"::error::perf-ratio: baseline has no entry for {sorted(missing)} — "
             "a gated benchmark was added without anchoring it; add the line on purpose.")

failed = False
for bench, ratio in sorted(ratios.items()):
    base = baseline[bench]
    ceiling = base * (1 + budget / 100)
    floor = base * 0.5
    verdict = "ok"
    if ratio > ceiling:
        verdict = f"OVER ceiling {ceiling:.3f}"
        failed = True
    elif ratio < floor:
        verdict = f"UNDER floor {floor:.3f} (broken bench or durable win — re-anchor on purpose)"
        failed = True
    print(f"perf-ratio: {bench:16s} {ratio:.3f} (baseline {base:.3f}, +{budget:.0f}% budget) {verdict}")

for bench, ref_name in sorted(reported.items()):
    v = data[bench]["variants"]
    nat = v[f"{bench}/native"]["median"]
    ref = v[f"{bench}/{ref_name}"]["median"]
    print(f"perf-ratio: {bench:16s} {nat / ref:.3f} (reported, not anchored — machine-dependent)")

# The listbuild idiom relation (#1337): the RECOMMENDED combinator shape
# against the `var` + `for` append loop it is documented to replace, from this
# same run. This is the property the idiom docs assert, so it is checked
# directly rather than inferred from two absolute ratios drifting apart. See
# IDIOM_CEILING above for why it is min-of-runs and reference-normalized.
def own_ratio(bench):
    v = data[bench]["variants"]
    return v[f"{bench}/native"]["min"] / v[f"{bench}/rust:listbuild"]["min"]

penalty = own_ratio("listbuild-comb") / own_ratio("listbuild-append")
if penalty > idiom_ceiling:
    failed = True
    print(f"::error::perf-ratio: the RECOMMENDED list idiom costs {penalty:.3f}x the append "
          f"loop it replaces (ceiling {idiom_ceiling:.2f}x). CLAUDE.md and docs/CHEATSHEET.md "
          "tell authors to write `list.range |> list.flat_map` instead of `var` + `for`; that "
          "guidance is only honest while this holds. The usual cause is the flat_map lambda "
          "losing its array return (RustLoweringPass::lower_flat_map_arrays) and going back to "
          "a heap Vec per element. Either restore the lowering or change the guidance — not "
          "the ceiling.")
else:
    print(f"perf-ratio: {'listbuild-idiom':16s} {penalty:.3f}x the append loop "
          f"(ceiling {idiom_ceiling:.2f}x) ok")

# ABLATION deltas (#1466): ablated/optimized per anchored bench, gated on
# baseline rows keyed `ablation/<bench>`. Floor 0.90 is the honest direction
# the measurement supports — the optimizer must never COST more than noise;
# the +budget ceiling flags a NEW earning so it gets re-anchored on purpose.
abl = json.load(open(abl_path))["results"]
for bench in sorted(pairs):
    o = data[bench]["variants"][f"{bench}/native"]["median"]
    a = abl[bench]["variants"][f"{bench}/native"]["median"]
    delta = a / o
    key = f"ablation/{bench}"
    base = baseline.get(key)
    if base is None:
        sys.exit(f"::error::perf-ratio: baseline has no `{key}` row — the ablation leg was "
                 "added without anchoring it; add the line on purpose.")
    ceiling = base * (1 + budget / 100)
    floor = 0.90
    verdict = "ok"
    if delta < floor:
        verdict = f"UNDER floor {floor:.2f} — the optimizer is COSTING runtime; find the pass and fix or retire it"
        failed = True
    elif delta > ceiling:
        verdict = f"OVER ceiling {ceiling:.3f} — a new real earning; re-anchor the row in the same change"
        failed = True
    print(f"perf-ratio: {key:16s} {delta:.3f} (ablated/optimized, baseline {base:.3f}) {verdict}")

if failed:
    print("::error::perf-ratio: a gated almide/rust runtime ratio left its band. A real")
    print("regression is usually a lost optimization in the generated Rust (clone in a")
    print("hot loop, in-place list write turned into a copy). Reproduce locally:")
    print("  python3 research/benchmark/perf/bench.py --quick --legs native,rust")
    print("If the change is intentional, move scripts/perf-ratio-baseline.txt in the")
    print("SAME change with the reasoning in the commit message.")
    sys.exit(1)
PY
