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
#                     is enforced everywhere. The RATIO comparison is a
#                     VERDICT only on the machine class that stamped the
#                     ledger (local runs, or WASM_RUNTIME_RATIO_VERDICT=1):
#                     unlike the native/Rust gate, a CROSS-ENGINE ratio does
#                     NOT cancel the machine — measured 2026-08-31, nbody
#                     wasm/native was 2.09 on the stamping M-series and
#                     19.95 on a 2-core ubuntu runner (the embedded engine's
#                     fixed costs and the fan thread-pool advantage both
#                     scale with hardware) — so on CI the ratio prints for
#                     information and the STATUS taxonomy is the gate.
#                     Budget when the verdict is armed:
#                     WASM_RUNTIME_BUDGET_PCT (default 50) percent above
#                     the committed ratio, and BOTH directions (under 40%
#                     of baseline = the bench broke or a durable win —
#                     re-stamp in the same change).
#   routed-incumbent  the embedded-host bench must STILL refuse (the program
#                     routes to the incumbent artifact). If it starts
#                     benching, the routing improved: FAIL with the good
#                     news — flip the row to `measured` in the same change.
#   oom-embedded      the run must STILL die with the defined C-197 OOM
#                     (#1729). A workload that starts completing flips its
#                     row to `measured` in the same change.
#
# Regenerate the ledger (rows + stamp): --measure. Never hand-edit numbers.
set -uo pipefail
cd "$(dirname "$0")/.."

LEDGER="docs/benchmarks/wasm-runtime.txt"
BIN="${ALMIDE_BIN:-target/release/almide}"
BUDGET_PCT="${WASM_RUNTIME_BUDGET_PCT:-50}"
CORPUS=research/benchmark/perf

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

bench_native() { "$BIN" bench "$(src_of "$1")" 2>&1 | tail -1; }
bench_wasm()   { "$BIN" bench "$(src_of "$1")" --target wasm 2>&1 | tail -2; }

if [ "${1:-}" = "--measure" ]; then
  ver=$("$BIN" --version)
  {
    cat <<'HDR'
# Wasm runtime ledger (#1701) — the SOURCE for the README's wasm-runtime block.
# Regenerate: bash scripts/check-wasm-runtime-ratio.sh --measure
# Gate:       bash scripts/check-wasm-runtime-ratio.sh — the STATUS taxonomy
#             is enforced everywhere (a hole that starts benching, a measured
#             row that stops, a changed refusal class all fail); the RATIO
#             verdict runs only on the stamping machine class (cross-engine
#             ratios do not cancel hardware — see the script header).
#
# Rows: name | status | native_ms | wasm_ms | ratio (wasm/native)
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
HDR
    echo "version = $ver"
    echo "date    = $(date +%F)"
    echo "machine = $(uname -m) $(uname -s) (local stamp; ratios are the gated quantity)"
    echo
    for name in nbody spectralnorm binarytrees fasta fannkuchredux mandelbrot onebrc fft strchurn listbuild_append listbuild_combinator listbuild_prealloc mapbuild; do
      n=$(median_of "$(bench_native "$name")")
      w_out=$(bench_wasm "$name")
      w=$(median_of "$w_out")
      if [ -n "$w" ] && [ -n "$n" ]; then
        ratio=$(python3 -c "print(f'{$w/$n:.2f}')")
        printf '%-21s | measured         | %-5s | %-4s | %s\n' "$name" "$n" "$w" "$ratio"
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

# Ratio verdict: on by default locally (the stamping machine class), off on
# CI unless explicitly armed — see the header for the measured reason.
if [ -n "${WASM_RUNTIME_RATIO_VERDICT:-}" ]; then RATIO_VERDICT=1
elif [ -n "${GITHUB_ACTIONS:-}" ]; then RATIO_VERDICT=0
else RATIO_VERDICT=1; fi

fail=0
while IFS= read -r raw; do
  case "$raw" in ''|\#*|version*|date*|machine*) continue ;; esac
  name=$(echo "$raw" | cut -d'|' -f1 | xargs)
  name_hint="$name"
  status=$(echo "$raw" | cut -d'|' -f2 | xargs)
  ratio=$(echo "$raw" | cut -d'|' -f5 | xargs)
  row_budget=$(echo "$raw" | grep -oE 'budget=[0-9]+' | cut -d= -f2)
  # The fan-parallel benches ride thread scheduling: their wasm/native ratio
  # legitimately swings 2-3x run to run (binarytrees measured 0.52..0.89 in
  # back-to-back stamps). A tight budget would flake, and a flaking gate
  # teaches people to ignore it — give the fan set the wide default; a real
  # regression (rc leak, lost in-place write) still clears 2-10x.
  fan_default="$BUDGET_PCT"
  case "$name_hint" in binarytrees|mandelbrot) fan_default=200 ;; esac
  budget="${row_budget:-$fan_default}"
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
        continue
      fi
      case "$v" in
        OK)  echo "wasm-runtime[$name]: ratio $now (baseline $ratio) OK" ;;
        HIGH) echo "::error::wasm-runtime[$name]: ratio $now regressed past baseline $ratio +${budget}%"; fail=1 ;;
        LOW) echo "::error::wasm-runtime[$name]: ratio $now is under 40% of baseline $ratio — a leg or the bench broke, or a durable win: re-stamp with --measure in this change"; fail=1 ;;
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

if [ "$fail" -ne 0 ]; then
  echo "::error::wasm-runtime ratchet FAILED — see rows above"
  exit 1
fi
echo "wasm-runtime ratchet OK ($(grep -c '^[a-z].*| measured' "$LEDGER") measured, $(grep -c '^[a-z].*| routed-incumbent' "$LEDGER" || true) routed, $(grep -c '^[a-z].*| walled' "$LEDGER" || true) walled, $(grep -c '^[a-z].*| oom-embedded' "$LEDGER" || true) oom)"
