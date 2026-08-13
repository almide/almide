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
PAIRS="nbody=rust:nbody_unrolled spectralnorm=rust:spectralnorm fasta=rust:fasta fft=rust:fft listbuild=rust:listbuild listbuild-append=rust:listbuild listbuild-comb=rust:listbuild"
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
  --bench nbody,spectralnorm,fasta,fft,listbuild,listbuild-append,listbuild-comb \
  --label ratchet --out "$out"

python3 - "$out" "$BASELINE_FILE" "$BUDGET_PCT" "$PAIRS" "$MIN_SECONDS" "$IDIOM_CEILING" <<'PY'
import json, sys

out_path, baseline_path, budget_pct, pairs_arg, min_s, idiom_ceiling = sys.argv[1:7]
budget = float(budget_pct)
min_s = float(min_s)
idiom_ceiling = float(idiom_ceiling)
pairs = dict(p.split("=", 1) for p in pairs_arg.split())

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

if failed:
    print("::error::perf-ratio: a gated almide/rust runtime ratio left its band. A real")
    print("regression is usually a lost optimization in the generated Rust (clone in a")
    print("hot loop, in-place list write turned into a copy). Reproduce locally:")
    print("  python3 research/benchmark/perf/bench.py --quick --legs native,rust")
    print("If the change is intentional, move scripts/perf-ratio-baseline.txt in the")
    print("SAME change with the reasoning in the commit message.")
    sys.exit(1)
PY
