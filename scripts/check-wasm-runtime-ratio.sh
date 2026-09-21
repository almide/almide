#!/usr/bin/env bash
# WASM RUNTIME RATIO RATCHET (#1701).
#
# The README said "Wasm runtime numbers are deliberately absent rather than
# estimated" — right while the leg was moving, wrong once the structural leg
# became the default and `almide bench --target wasm` existed. This is the
# number, and the watch on it.
#
# What it checks, per row of docs/benchmarks/wasm-runtime.txt:
#   measured          re-bench BOTH legs. The row must still BENCH — that
#                     is enforced everywhere. The stamped RATIO comparison
#                     is a VERDICT only on the machine class that stamped
#                     the ledger (local runs, or WASM_RUNTIME_RATIO_VERDICT=1):
#                     unlike the native/Rust gate, a CROSS-ENGINE ratio does
#                     NOT cancel the machine — measured 2026-08-31, nbody
#                     wasm/native was 2.09 on the stamping M-series and
#                     19.95 on a 2-core ubuntu runner (the embedded engine's
#                     fixed costs and the fan thread-pool advantage both
#                     scale with hardware) — so on CI the stamped ratio
#                     prints for information. Budget when that verdict is
#                     armed: WASM_RUNTIME_BUDGET_PCT (default 50) percent
#                     above the committed ratio, and BOTH directions (under
#                     40% of baseline = the bench broke or a durable win —
#                     re-stamp in the same change).
#                     THE VERDICT CI ACTS ON is the same-runner A/B below.
#   routed-incumbent  the embedded-host bench must STILL refuse (the program
#                     routes to the incumbent artifact). If it starts
#                     benching, the routing improved: FAIL with the good
#                     news — flip the row to `measured` in the same change.
#   oom-embedded      the run must STILL die with the defined C-197 OOM
#                     (#1729). A workload that starts completing flips its
#                     row to `measured` in the same change.
#
# SAME-RUNNER A/B VERDICT (#2143) — the choice, and why.
#   The header above is right: a cross-engine ratio does not cancel the
#   machine, so a stamped wasm/native ratio can never judge on a runner class
#   that did not stamp it. #2143 listed four ways out (same-run native
#   baseline / a 3x absolute band / a pinned runner class / nightly). The
#   first as written IS the stamped comparison (both legs already run in the
#   same job; the stamped ratio is what fails to transfer), the band is blind
#   on a slow runner, a pinned runner is infrastructure this repo does not
#   have, and nightly is a one-day gap that still needs a reader. The form
#   that cancels the machine is a SAME-ENGINE ratio: this tree's wasm leg
#   against the latest release's wasm leg, both binaries benched on the same
#   runner in the same job — the shape #2028's postmortem named ("gate the
#   relative change between two builds on the same runner").
#   Mechanism, per `measured` row, when WASM_RUNTIME_BASELINE_BIN is set:
#     - two interleaved rounds (release, tree, release, tree), each round one
#       `almide bench --target wasm --runs WASM_RUNTIME_AB_RUNS` (default 5);
#     - the statistic is the MIN over all runs of each binary: a busy
#       neighbour on a shared runner can only ADD time, so the min is the
#       run the load did not touch, and interleaving means a burst must hit
#       every tree run and no release run to fake a regression;
#     - verdict: tree_min / release_min above 1 + band is a FAIL. The band
#       is the ledger's `ab_band` line (percent; fan rows carry
#       `ab_band=NN` on the row because thread scheduling swings them), env
#       WASM_RUNTIME_AB_BAND_PCT overrides for a local probe;
#     - a row the release binary cannot bench (a program newer than the
#       release, or one it walls) is reported and SKIPPED by the A/B — the
#       status taxonomy still gates the tree on it;
#     - a faster tree is good news, printed, never a failure: the A/B has no
#       stamped number to keep honest.
#   What it does not claim: a regression under the band passes (30% by
#   default — #2028's +51% is over it), and drift accumulates against the
#   LAST RELEASE, so two accepted 20% costs since a release trip it together
#   (raise the row's `ab_band=` with the second one, in the same change).
#   Under GITHUB_ACTIONS the baseline binary is REQUIRED (exit 2 without it)
#   unless WASM_RUNTIME_AB=off is set on the step — the gate cannot degrade
#   to informational without the workflow saying so.
#
# Regenerate the ledger (rows + stamp): --measure. Never hand-edit numbers.
set -uo pipefail
cd "$(dirname "$0")/.."

LEDGER="docs/benchmarks/wasm-runtime.txt"
BIN="${ALMIDE_BIN:-target/release/almide}"
BUDGET_PCT="${WASM_RUNTIME_BUDGET_PCT:-50}"
CORPUS=research/benchmark/perf
BASE_BIN="${WASM_RUNTIME_BASELINE_BIN:-}"
AB_RUNS="${WASM_RUNTIME_AB_RUNS:-5}"
AB_ROUNDS=2
AB_BAND_DEFAULT=30

[ -x "$BIN" ] || { echo "::error::$BIN not built — cargo build --release first"; exit 2; }

src_of() { # benchmark name -> source path (listbuild variants share a dir)
  case "$1" in
    listbuild_*) printf '%s/listbuild/%s.almd' "$CORPUS" "$1" ;;
    *)           printf '%s/%s/%s.almd' "$CORPUS" "$1" "$1" ;;
  esac
}

median_of() { # "<bench output tail>" -> median ms or empty
  grep -oE 'median [0-9.]+ ms' <<<"$1" | grep -oE '[0-9.]+' | head -1
}
min_of() { # "<bench output tail>" -> min ms or empty
  grep -oE 'min [0-9.]+' <<<"$1" | grep -oE '[0-9.]+' | head -1
}

bench_native() { "$BIN" bench "$(src_of "$1")" 2>&1 | tail -1; }
bench_wasm()   { "$BIN" bench "$(src_of "$1")" --target wasm 2>&1 | tail -2; }
bench_wasm_with() { "$1" bench "$(src_of "$2")" --target wasm --runs "$AB_RUNS" </dev/null 2>&1 | tail -2; }

if [ "${1:-}" = "--measure" ]; then
  ver=$("$BIN" --version)
  ab_band_keep=$(grep -E '^ab_band' "$LEDGER" 2>/dev/null | head -1 | grep -oE '[0-9]+' | head -1)
  {
    cat <<'HDR'
# Wasm runtime ledger (#1701) — the SOURCE for the README's wasm-runtime block.
# Regenerate: bash scripts/check-wasm-runtime-ratio.sh --measure
# Gate:       bash scripts/check-wasm-runtime-ratio.sh — the STATUS taxonomy
#             is enforced everywhere (a hole that starts benching, a measured
#             row that stops, a changed refusal class all fail); the stamped
#             RATIO verdict runs only on the stamping machine class (cross-
#             engine ratios do not cancel hardware — see the script header);
#             CI's verdict is the SAME-RUNNER A/B (#2143): this tree's wasm
#             leg against the latest release's, interleaved, min-of-runs,
#             tree/release above 1 + ab_band fails.
#
# Rows: name | status | native_ms | wasm_ms | ratio (wasm/native) [ab_band=NN]
#   measured          — `almide bench --target wasm` (embedded host, verify-
#                       then-time, median of 5 + warmup) and the native twin
#   routed-incumbent  — the program routes to the incumbent artifact; the
#                       embedded-host bench honestly refuses
#   walled            — the wasm build path walls the program (neither leg)
#   oom-embedded      — the workload exceeds the embedded host's memory
#                       service today (#1729)
# Every non-measured row is RE-MEASURED by the gate: a row that starts
# benching fails the run with the good news (flip it to measured in the
# same change); a measured row that stops benching fails as a regression.
# ab_band is the A/B tolerance in percent (a policy line, not a measurement);
# a fan row overrides it with `ab_band=NN` at the end of its row.
HDR
    echo "version = $ver"
    echo "date    = $(date +%F)"
    echo "machine = $(uname -m) $(uname -s) (local stamp; ratios are the gated quantity)"
    echo "ab_band = ${ab_band_keep:-$AB_BAND_DEFAULT}"
    echo
    for name in nbody spectralnorm binarytrees treealloc fasta fannkuchredux mandelbrot onebrc fft strchurn listbuild_append listbuild_combinator listbuild_prealloc mapbuild; do
      n=$(median_of "$(bench_native "$name")")
      w_out=$(bench_wasm "$name")
      w=$(median_of "$w_out")
      fan_suffix=""
      case "$name" in binarytrees|mandelbrot) fan_suffix=" ab_band=100" ;; esac
      if [ -n "$w" ] && [ -n "$n" ]; then
        ratio=$(python3 -c "print(f'{$w/$n:.2f}')")
        printf '%-21s | measured         | %-5s | %-4s | %s%s\n' "$name" "$n" "$w" "$ratio" "$fan_suffix"
      elif grep -q "incumbent artifact" <<<"$w_out"; then
        printf '%-21s | routed-incumbent | %-5s | -    | -\n' "$name" "${n:--}"
      elif grep -q "out of memory" <<<"$w_out"; then
        printf '%-21s | oom-embedded     | %-5s | -    | -\n' "$name" "${n:--}"
      elif grep -q "wall (structural" <<<"$w_out"; then
        printf '%-21s | walled           | %-5s | -    | -\n' "$name" "${n:--}"
      else
        printf '%-21s | UNCLASSIFIED     | %-5s | -    | -\n' "$name" "${n:--}"
      fi
    done
  } > "$LEDGER.tmp" && mv "$LEDGER.tmp" "$LEDGER"
  echo "wasm-runtime: ledger re-measured -> $LEDGER"
  exit 0
fi

# Stamped-ratio verdict: on by default locally (the stamping machine class),
# off on CI unless explicitly armed — see the header for the measured reason.
if [ -n "${WASM_RUNTIME_RATIO_VERDICT:-}" ]; then RATIO_VERDICT=1
elif [ -n "${GITHUB_ACTIONS:-}" ]; then RATIO_VERDICT=0
else RATIO_VERDICT=1; fi

# Same-runner A/B verdict (#2143): armed whenever a baseline binary is given;
# on CI its absence is an error unless the workflow disarms it by name.
if [ -n "$BASE_BIN" ]; then
  [ -x "$BASE_BIN" ] || { echo "::error::WASM_RUNTIME_BASELINE_BIN=$BASE_BIN is not executable — the A/B verdict cannot run"; exit 2; }
  AB_VERDICT=1
  base_ver=$("$BASE_BIN" --version 2>&1 | head -1)
elif [ -n "${GITHUB_ACTIONS:-}" ] && [ "${WASM_RUNTIME_AB:-}" != "off" ]; then
  echo "::error::wasm-runtime: no WASM_RUNTIME_BASELINE_BIN on CI — the same-runner A/B (#2143) is the only verdict that acts here; supply the latest release binary, or set WASM_RUNTIME_AB=off on the step to disarm it on record"
  exit 2
else
  AB_VERDICT=0
fi
ab_band_ledger=$(grep -E '^ab_band' "$LEDGER" | head -1 | grep -oE '[0-9]+' | head -1)
AB_BAND="${WASM_RUNTIME_AB_BAND_PCT:-${ab_band_ledger:-$AB_BAND_DEFAULT}}"
if [ "$AB_VERDICT" = "1" ]; then
  echo "wasm-runtime: A/B verdict armed — tree $("$BIN" --version 2>&1 | head -1) vs baseline $base_ver, $AB_ROUNDS interleaved rounds x $AB_RUNS runs, min-of-runs, band +${AB_BAND}%"
fi

fail=0
ab_rows=0
while IFS= read -r raw; do
  case "$raw" in ''|\#*|version*|date*|machine*|ab_band*) continue ;; esac
  name=$(echo "$raw" | cut -d'|' -f1 | xargs)
  name_hint="$name"
  status=$(echo "$raw" | cut -d'|' -f2 | xargs)
  ratio=$(echo "$raw" | cut -d'|' -f5 | xargs | cut -d' ' -f1)
  row_budget=$(echo "$raw" | grep -oE 'budget=[0-9]+' | cut -d= -f2)
  row_ab_band=$(echo "$raw" | grep -oE 'ab_band=[0-9]+' | cut -d= -f2)
  # The fan-parallel benches ride thread scheduling: their wasm/native ratio
  # legitimately swings 2-3x run to run (binarytrees measured 0.52..0.89 in
  # back-to-back stamps). A tight budget would flake, and a flaking gate
  # teaches people to ignore it — give the fan set the wide default; a real
  # regression (rc leak, lost in-place write) still clears 2-10x.
  fan_default="$BUDGET_PCT"
  case "$name_hint" in binarytrees|mandelbrot) fan_default=200 ;; esac
  budget="${row_budget:-$fan_default}"
  ab_band="${row_ab_band:-$AB_BAND}"
  [ -z "$name" ] && continue
  case "$status" in
    measured)
      n=$(median_of "$(bench_native "$name")")
      w=$(median_of "$(bench_wasm "$name")")
      if [ -z "$n" ] || [ -z "$w" ]; then
        echo "::error::wasm-runtime[$name]: a measured row stopped benching (native='$n' wasm='$w') — a leg or the routing regressed"
        fail=1; continue
      fi
      verdict=$(python3 -c "
now = $w/$n; base = $ratio
hi = base * (1 + $budget/100); lo = base * 0.4
print('HIGH' if now > hi else 'LOW' if now < lo else 'OK', f'{now:.2f}')")
      v=${verdict%% *}; now=${verdict##* }
      if [ "$RATIO_VERDICT" != "1" ]; then
        echo "wasm-runtime[$name]: benches (ratio $now here, $ratio stamped — informational on this machine class)"
      else
        case "$v" in
          OK)  echo "wasm-runtime[$name]: ratio $now (baseline $ratio) OK" ;;
          HIGH) echo "::error::wasm-runtime[$name]: ratio $now regressed past baseline $ratio +${budget}%"; fail=1 ;;
          LOW) echo "::error::wasm-runtime[$name]: ratio $now is under 40% of baseline $ratio — a leg or the bench broke, or a durable win: re-stamp with --measure in this change"; fail=1 ;;
        esac
      fi
      [ "$AB_VERDICT" = "1" ] || continue
      # Same-runner A/B (#2143): interleaved rounds, min over every run.
      b_min=""; t_min=""; base_out=""
      for _round in $(seq "$AB_ROUNDS"); do
        base_out=$(bench_wasm_with "$BASE_BIN" "$name"); bm=$(min_of "$base_out")
        [ -n "$bm" ] || break
        tm=$(min_of "$(bench_wasm_with "$BIN" "$name")")
        [ -n "$tm" ] || break
        b_min=$(python3 -c "print(min($bm, ${b_min:-$bm}))")
        t_min=$(python3 -c "print(min($tm, ${t_min:-$tm}))")
      done
      if [ -z "$b_min" ]; then
        echo "wasm-runtime[$name]: A/B skipped — the baseline ($base_ver) does not bench this row: $(head -1 <<<"$base_out")"
        continue
      fi
      if [ -z "$t_min" ]; then
        echo "::error::wasm-runtime[$name]: A/B — the tree benched once and then stopped (min missing in a later round)"
        fail=1; continue
      fi
      ab=$(python3 -c "
r = $t_min/$b_min
print('HIGH' if r > 1 + $ab_band/100 else 'OK', f'{r:.2f}')")
      abv=${ab%% *}; abr=${ab##* }
      ab_rows=$((ab_rows + 1))
      case "$abv" in
        OK)   echo "wasm-runtime[$name]: A/B tree/release ${abr} (min ${t_min} vs ${b_min} ms, band +${ab_band}%) OK" ;;
        HIGH) echo "::error::wasm-runtime[$name]: A/B tree/release ${abr} (min ${t_min} vs ${b_min} ms) regressed past +${ab_band}% against $base_ver — a wasm-leg slowdown on this runner, same engine, same program"; fail=1 ;;
      esac
      ;;
    routed-incumbent)
      out=$(bench_wasm "$name")
      if median=$(median_of "$out") && [ -n "$median" ]; then
        echo "::error::wasm-runtime[$name]: routed-incumbent row now BENCHES (${median} ms) — the routing improved; flip the row to measured (--measure) in this change"
        fail=1
      elif ! grep -q "incumbent artifact" <<<"$out"; then
        echo "::error::wasm-runtime[$name]: expected the incumbent-routing refusal, got: $(head -1 <<<"$out")"
        fail=1
      else
        echo "wasm-runtime[$name]: still routed-incumbent (honest hole)"
      fi
      ;;
    oom-embedded)
      out=$(bench_wasm "$name")
      if median=$(median_of "$out") && [ -n "$median" ]; then
        echo "::error::wasm-runtime[$name]: oom-embedded row now COMPLETES (${median} ms) — #1729 progressed; flip the row to measured (--measure) in this change"
        fail=1
      elif ! grep -q "out of memory" <<<"$out"; then
        echo "::error::wasm-runtime[$name]: expected the C-197 OOM (#1729), got: $(head -1 <<<"$out")"
        fail=1
      else
        echo "wasm-runtime[$name]: still oom-embedded (#1729)"
      fi
      ;;
    walled)
      out=$(bench_wasm "$name")
      if median=$(median_of "$out") && [ -n "$median" ]; then
        echo "::error::wasm-runtime[$name]: walled row now BENCHES (${median} ms) — flip the row to measured (--measure) in this change"
        fail=1
      elif ! grep -q "wall (structural" <<<"$out"; then
        echo "::error::wasm-runtime[$name]: expected the structural wall, got: $(head -1 <<<"$out")"
        fail=1
      else
        echo "wasm-runtime[$name]: still walled (honest hole)"
      fi
      ;;
    UNCLASSIFIED)
      echo "::error::wasm-runtime[$name]: UNCLASSIFIED row committed — classify it before merging"
      fail=1
      ;;
  esac
done < "$LEDGER"

if [ "$AB_VERDICT" = "1" ] && [ "$ab_rows" -eq 0 ]; then
  echo "::error::wasm-runtime: the A/B verdict was armed but judged ZERO rows — the baseline benched nothing, so nothing watched the wasm leg"
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "::error::wasm-runtime ratchet FAILED — see rows above"
  exit 1
fi
echo "wasm-runtime ratchet OK ($(grep -c '^[a-z].*| measured' "$LEDGER") measured, $(grep -c '^[a-z].*| routed-incumbent' "$LEDGER" || true) routed, $(grep -c '^[a-z].*| walled' "$LEDGER" || true) walled, $(grep -c '^[a-z].*| oom-embedded' "$LEDGER" || true) oom; A/B judged $ab_rows row(s))"
