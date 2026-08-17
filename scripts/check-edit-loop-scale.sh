#!/usr/bin/env bash
# EDIT-LOOP SCALE-INVARIANCE RATCHET (#1334) + FRONT-END PER-PHASE RATCHET (#1311).
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
# ── PER-PHASE RESOLUTION (#1311) ─────────────────────────────────────────────
#
# `slope` and `ratio` are ONE number for the whole front end, and one number
# cannot see a checker that got 40% slower while the lexer got 40% faster. #1311
# asked for lines/sec budgets per phase; this is that, with the units changed for
# the reason the whole file exists. Each rung is now checked with `--timings`, so
# the same interleaved samples also carry the front end split into lex / parse /
# check (plus a named `other`: file I/O, import resolution, canonicalization, and
# the lowering behind the unused-var warnings). Measured on this ladder,
# 2026-08-14, top rung (30,657 project lines / 34,795 lines actually lexed):
#
#     lex 11.8ms (2.9M lines/s)   parse 12.1ms (2.9M)   check 66.0ms (0.53M)
#
# A RAW lines/sec budget is not gateable for exactly the #1334 reason, and this
# ladder re-measured it: across a 16x load swing on one M4 Pro (load 1.7 -> 31.2,
# eight 40-run measurements) the top rung's check throughput fell 524k -> 472k
# lines/s (1.11x) and its p95 wall clock moved 3.0x, with the compiler untouched.
# Across MACHINE CLASSES it would be worse still — #1337's perf row read 1.58 on
# the dev box and 0.91 on the CI runner. So what is anchored is again
# dimensionless and same-run:
#
#   share_lex / share_parse / share_check
#           each phase's top-rung `min` as a fraction of the three summed. The
#           denominator is the accounted phases, not the process total, because
#           `other` is dominated by filesystem cost that is not the front end's.
#           Zero-sum by construction, which is the point: a check regression
#           shows up as the SMALL phases' shares falling, and those move the most
#           in relative terms. Sensitivity, from the measured split: a check
#           slowdown of ~15% pushes share_lex through a 10% floor; a lex slowdown
#           of ~12% pushes share_lex through a 10% ceiling. Observed spread over
#           the same 16x load swing: lex 4.2%, parse 3.2%, check 1.2% — against
#           3.0x for the p95 the naive gate would have held.
#
#   slope_check
#           the log-log slope of MARGINAL check-phase time against project lines.
#           This is `slope` localized: the aggregate reads 1.15, and the split
#           shows why — lex 1.00 and parse 1.03 are flat, and the whole of the
#           superlinearity lives in the checker at 1.14. Anchoring it separately
#           means a checker that goes quadratic cannot be masked by a lexer that
#           got faster in the same release. It is also the most machine-portable
#           number here: a slope is an exponent, so any uniform per-phase speed
#           factor cancels out of it entirely. Observed spread over the load
#           swing: 1.9%.
#
# REPORTED but NOT anchored: lines/sec per phase (a machine speed), and
# slope_lex / slope_parse. The last two are honest debt — lex and parse are ~12ms
# against check's 66ms, so subtracting the floor rung leaves a small difference of
# two small numbers, and they read a 10.3% / 10.4% spread over the load swing
# against slope_check's 1.9%. A band wide enough not to flake there would not
# catch anything the share bands miss, since a lexer going quadratic blows up
# share_lex at the top rung long before its slope band would fire.
#
# BAND WIDTH AND THE MACHINE IT WAS ANCHORED ON. The share bands are +/-20%,
# wider than the 4.2% worst-case load spread justifies, because the baseline is
# anchored on an M4 Pro and CI runs ubuntu x86-64: the phase SPLIT (unlike the
# slope) can legitimately differ between microarchitectures, and #1337 is the
# standing lesson about discovering that in production. Tighten them once the
# runner's own numbers are on record — that direction of the ratchet is free.
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
# Per-phase (#1311). The share band is two-sided and SYMMETRIC — unlike slope and
# ratio, a share falling is not "the gate went blind", it is another phase having
# grown, which is exactly the regression this resolution exists to catch.
SHARE_BUDGET_PCT="${EDIT_LOOP_SHARE_BUDGET_PCT:-20}"

# ── blindness floors (#976: a gate that measures nothing reads green forever) ──
# The corpus is discovered by glob, so it can shrink without anyone noticing.
# These are properties of the INPUT, independent of the compiler under test.
CORPUS_FILE_FLOOR=250      # 303 stdlib modules composed 2026-08-13
CORPUS_LINE_FLOOR=25000    # 30,624 lines in the top rung
TEN_K_LINE_FLOOR=9000      # 10,151 lines in the headline rung
# And one property of the MEASUREMENT: under this, wall-clock is process-spawn
# noise rather than compilation. A durable 3x win re-anchors this on purpose.
TOP_MIN_MS_FLOOR=40
# Per-phase blindness floors (#1311). The phase numbers come from INSIDE the
# binary under test, so they can go blind in ways the wall clock cannot: a
# deleted `phase_scope` reads 0.0ms, and 0.0 satisfies no band but silently
# poisons every share it appears in. These say the instrumentation was alive.
FE_LINES_FLOOR=28000       # 34,795 lines actually lexed at the top rung 2026-08-14
FE_SOURCES_FLOOR=300       # 362 source texts (303 modules + entry + bundled stdlib)
PHASE_MIN_MS_FLOOR=1.0     # smallest measured top-rung phase is lex at 11.8ms

# Which binary the ladder times — `ALMIDE_BIN`, the same hook bench.py and
# check-semantics-manifest.sh take, so CI can point it at the downloaded build
# artifact. It doubles as this gate's self-verification hook: point it at a shim
# that adds a delay proportional to lines^2 and the `slope` anchor fires, which
# is the only way to demonstrate that the gate detects real superlinearity
# rather than baseline arithmetic. Recorded in proofs/gate-verification.toml.
#
# The PER-PHASE anchors cannot be forged through a shim, because their numbers
# come from inside the binary. Forge them in the compiler and rebuild — a spin
# loop proportional to `src.len()` inside `Lexer::tokenize` (a uniformly slower
# lexer), or one proportional to the module count so far inside `infer_module`
# (a quadratic checker). Both were run for the ledger row, and both are the
# reason this resolution exists: with the lexer forge every aggregate anchor
# read GREEN and only `share_lex` fired; with the checker forge `slope` and
# `ratio` BOTH read green and only `slope_check` and the lex/parse share floors
# fired. One number for the whole front end cannot see either regression.
ALMIDE_BIN="${ALMIDE_BIN:-$PWD/target/release/almide}"

OUT=$(mktemp -t editloop-scale.XXXXXX.json)
trap 'rm -f "$OUT"' EXIT

python3 research/benchmark/editloop/scale.py --runs "$RUNS" --almide "$ALMIDE_BIN" --out "$OUT"

python3 - "$OUT" "$BASELINE_FILE" "$SLOPE_BUDGET_PCT" "$RATIO_BUDGET_PCT" \
    "$CORPUS_FILE_FLOOR" "$CORPUS_LINE_FLOOR" "$TEN_K_LINE_FLOOR" "$TOP_MIN_MS_FLOOR" \
    "$SHARE_BUDGET_PCT" "$FE_LINES_FLOOR" "$FE_SOURCES_FLOOR" "$PHASE_MIN_MS_FLOOR" <<'PY'
import json, sys

out_path, baseline_path = sys.argv[1:3]
slope_budget, ratio_budget = float(sys.argv[3]), float(sys.argv[4])
file_floor, line_floor = int(sys.argv[5]), int(sys.argv[6])
ten_k_floor, top_min_floor = int(sys.argv[7]), float(sys.argv[8])
share_budget = float(sys.argv[9])
fe_lines_floor, fe_sources_floor = int(sys.argv[10]), int(sys.argv[11])
phase_min_floor = float(sys.argv[12])

PHASES = ["lex", "parse", "check"]

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

# ── per-phase blindness floors (#1311) ──────────────────────────────────────
# These run BEFORE the shares are read. A share computed from a phase that
# reported nothing is not a lenient measurement, it is a fabricated one.
if not d.get("timings"):
    fatal.append("the ladder ran WITHOUT --timings, so there are no phase numbers at all. "
                 "The per-phase anchors cannot be evaluated and this gate must not report "
                 "green on the wall-clock half alone.")
elif not d.get("phase_shares") or not d.get("phase_slopes"):
    fatal.append("--timings was requested but no phase split came back — every rung must "
                 "carry an `almide-timings` line. Either the flag stopped working or the "
                 "phase accounting was removed from the compiler.")
else:
    if top.get("fe_lines", 0) < fe_lines_floor:
        fatal.append(f"the front end reported lexing {top.get('fe_lines')} lines at the top "
                     f"rung (floor {fe_lines_floor}) — the corpus shrank, or the compiler "
                     "stopped reading the modules it was handed")
    if top.get("fe_sources", 0) < fe_sources_floor:
        fatal.append(f"the front end reported {top.get('fe_sources')} source texts at the top "
                     f"rung (floor {fe_sources_floor}) — same")
    for name in PHASES:
        got = top["phase"][name]["min"]
        if got < phase_min_floor:
            fatal.append(f"top-rung `{name}` phase measured {got:.3f}ms, under the "
                         f"{phase_min_floor:.1f}ms floor — a phase that reports ~0 is an "
                         "instrumentation hole (a deleted phase_scope), and it would poison "
                         "every share it appears in rather than fail one band")
    if d["phase_slopes"].get("check") is None:
        fatal.append("slope_check could not be computed — fewer than two rungs showed the "
                     "check phase growing with project size at all. That is a broken ladder, "
                     "not a linear compiler, and it must not skip the anchor silently")
    if top["phase"]["other"]["min"] < 0:
        fatal.append(f"the residual `other` phase measured {top['phase']['other']['min']:.3f}ms "
                     "— negative means the accounted phases outran the process total, so the "
                     "phase spans are nesting and double-counting")

if fatal:
    for f in fatal:
        print(f"::error::editloop-scale: {f}")
    sys.exit(1)

slope = d["slope_min"]
ratio = ten["min"] / floor["min"]
shares = d["phase_shares"]
phase_slopes = d["phase_slopes"]

print(f"editloop-scale: headline rung {ten['lines']} lines — "
      f"p50 {ten['p50']:.1f}ms / p95 {ten['p95']:.1f}ms "
      f"(min {ten['min']:.1f}ms, {d['runs']} interleaved runs, load {d['loadavg'][0]:.1f}) "
      "— REPORTED, not anchored: this statistic tracks the machine, not the compiler")
print("editloop-phase: top rung %d project lines / %d lines lexed — "
      % (top["lines"], top["fe_lines"])
      + "  ".join("%s %.1fms (%.0fk lines/s)"
                  % (n, top["phase"][n]["min"], top["k_lines_per_sec"][n]) for n in PHASES)
      + "  other %.1fms" % top["phase"]["other"]["min"]
      + " — throughput REPORTED, not anchored (it is a machine speed)")
print("editloop-phase: per-phase slope — "
      + "  ".join("%s %s" % (n, "n/a" if phase_slopes[n] is None else "%.3f" % phase_slopes[n])
                  for n in PHASES)
      + "   (only slope_check is anchored; lex/parse are small differences of small "
        "numbers — see the header)")

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
        for n in PHASES:
            f.write(f"share_{n} {shares[n]:.4f}\n")
        f.write(f"slope_check {phase_slopes['check']:.3f}\n")
    print(f"editloop-scale: no baseline; wrote {baseline_path}")
    sys.exit(0)

missing = {"slope", "ratio", "slope_check"} | {f"share_{n}" for n in PHASES}
missing -= set(baseline)
if missing:
    sys.exit(f"::error::editloop-scale: baseline has no entry for {sorted(missing)} — "
             "an anchored quantity was added without anchoring it; add the line on purpose.")

# (name, measured, budget %, floor as a fraction of baseline).
# The share floors are SYMMETRIC with their ceilings, unlike slope/ratio: a share
# FALLING is not "the ladder went blind", it is another phase having grown, and
# that is precisely the regression per-phase resolution was added to see. The
# asymmetric 0.75/0.85 treatment would blind that direction.
checks = [("slope", slope, slope_budget, 0.85),
          ("ratio", ratio, ratio_budget, 0.75),
          ("slope_check", phase_slopes["check"], slope_budget, 0.85)]
checks += [(f"share_{n}", shares[n], share_budget, 1 - share_budget / 100) for n in PHASES]

failed = False
for name, value, budget, floor_frac in checks:
    base = baseline[name]
    ceiling = base * (1 + budget / 100)
    band_floor = base * floor_frac
    verdict = "ok"
    if value > ceiling:
        verdict = f"OVER ceiling {ceiling:.4f}"
        failed = True
    elif value < band_floor:
        verdict = f"UNDER floor {band_floor:.4f} (ladder broke, or a durable win — re-anchor on purpose)"
        failed = True
    print(f"editloop-scale: {name:12s} {value:.4f} (baseline {base:.4f}, +/-{budget:.0f}% budget) {verdict}")

if failed:
    print("::error::editloop-scale: the edit loop left its scale band. `slope` over 1.3 means")
    print("`almide check` is going superlinear in project size — the usual cause is a pass that")
    print("rescans an accumulating table per module (import resolution, UFCS dispatch, the")
    print("checker's module env) instead of indexing it. `ratio` over its ceiling with `slope`")
    print("flat means the per-line work got uniformly more expensive. `slope_check` over its")
    print("ceiling with `slope` still inside it means the checker went superlinear and a")
    print("cheaper lexer is hiding it in the aggregate. A `share_*` out of band names the")
    print("phase directly: the share that ROSE is the phase that got slower, and the shares")
    print("that fell are the ones that merely paid for it.")
    print("Reproduce locally:")
    print("  python3 research/benchmark/editloop/scale.py --runs 40")
    print("  almide check --timings <entry>      # the same split for one project")
    print("If the change is intentional, move scripts/edit-loop-scale-baseline.txt in the SAME")
    print("change with the reasoning in the commit message.")
    sys.exit(1)
PY
