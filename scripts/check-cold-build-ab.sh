#!/usr/bin/env bash
# Cold native build: a same-runner A/B against the latest release (#3829).
#
# A cold `almide build` of a program pays for the runtime rlib (`almide_rt`,
# compiled once per compiler + toolchain into `<temp>/almide-rtlib-<key>`) and
# then for the program itself. The build-speed ledger
# (docs/benchmarks/build-speed.txt) is stamped by hand on one machine class, so
# nothing re-measured it between 0.59.1 and 0.68: the cold row went from 635 ms
# to 4.8 s unseen. Most of that was `almide clean` starting to empty the rlib
# dirs (8cddfe4d6, so the bench's "cold" began to include the runtime build);
# the rest was the runtime build itself growing 41% with the runtime source.
#
# The verdict here does not depend on the machine: both binaries build the
# same program on this runner, interleaved, min of runs, each run in a fresh
# empty TMPDIR — so every run is cold by construction (no almide-run cache, no
# rlib) and the user's own caches are never touched. Fails when this tree's
# cold build is slower than the baseline's by more than the band.
#
# Also checked: a cold build compiles exactly ONE runtime rlib (one
# `almide-rtlib-*` dir in the fresh TMPDIR) — a key that drifts within a build
# would compile it twice and still pass a loose band.
#
#   ALMIDE_BIN=target/release/almide COLD_BUILD_BASELINE_BIN=/path/to/release/almide \
#     bash scripts/check-cold-build-ab.sh
#
#   env: COLD_BUILD_TARGET (examples/lisp.almd)  COLD_BUILD_ROUNDS (3)
#        COLD_BUILD_BAND_PCT (30)
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.." || exit 2
BIN="${ALMIDE_BIN:-target/release/almide}"
BASE="${COLD_BUILD_BASELINE_BIN:-}"
TARGET="${COLD_BUILD_TARGET:-examples/lisp.almd}"
ROUNDS="${COLD_BUILD_ROUNDS:-3}"
BAND="${COLD_BUILD_BAND_PCT:-30}"
[ -x "$BIN" ] || { echo "::error::cold-build: ALMIDE_BIN not executable: $BIN"; exit 2; }
# No baseline, no verdict — and a gate that quietly turns informational is the
# hole #2143 closed, so a missing baseline is a failure, not a skip.
[ -n "$BASE" ] && [ -x "$BASE" ] || { echo "::error::cold-build: COLD_BUILD_BASELINE_BIN not executable: '${BASE}' (the latest release binary)"; exit 2; }
[ -f "$TARGET" ] || { echo "::error::cold-build: target not found: $TARGET"; exit 2; }
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

# One cold build in a fresh TMPDIR; prints "<ms> <rlib dirs>", or FAIL.
cold_build() {
  local bin="$1" tmp
  tmp="$(mktemp -d "$SCRATCH/run.XXXXXX")"
  TMPDIR="$tmp" python3 - "$bin" "$TARGET" "$tmp/out" "$tmp" <<'PY'
import glob, os, subprocess, sys, time
bin_, target, out, tmp = sys.argv[1:5]
t = time.perf_counter()
r = subprocess.run([bin_, "build", target, "-o", out], capture_output=True)
ms = (time.perf_counter() - t) * 1000
if r.returncode != 0:
    sys.stderr.write(r.stderr.decode(errors="replace")[-2000:])
    print("FAIL")
else:
    print(f"{ms:.0f} {len(glob.glob(os.path.join(tmp, 'almide-rtlib-*')))}")
PY
  rm -rf "$tmp"
}

echo "cold-build: tree $("$BIN" --version) vs baseline $("$BASE" --version), $TARGET, $ROUNDS interleaved rounds, band ${BAND}%"
t_min=""; b_min=""; fail=0
for round in $(seq "$ROUNDS"); do
  b=$(cold_build "$BASE"); t=$(cold_build "$BIN")
  echo "  round $round: baseline ${b% *} ms, tree ${t% *} ms"
  if [ "$b" = "FAIL" ]; then
    echo "::error::cold-build: the baseline cannot build $TARGET — point COLD_BUILD_TARGET at a program both binaries accept"
    exit 1
  fi
  if [ "$t" = "FAIL" ]; then echo "::error::cold-build: this tree cannot build $TARGET"; exit 1; fi
  if [ "${t#* }" != "1" ]; then
    echo "::error::cold-build: one cold build left ${t#* } runtime rlib dirs (expected exactly 1: the runtime is compiled once per build)"
    fail=1
  fi
  b_min=$(( ${b_min:-${b% *}} < ${b% *} ? ${b_min:-${b% *}} : ${b% *} ))
  t_min=$(( ${t_min:-${t% *}} < ${t% *} ? ${t_min:-${t% *}} : ${t% *} ))
done
verdict=$(python3 -c "r = $t_min / $b_min; print('HIGH' if r > 1 + $BAND / 100 else 'OK', f'{r:.2f}')")
if [ "${verdict%% *}" = "HIGH" ]; then
  echo "::error::cold-build REGRESSION: ${t_min} ms against the latest release's ${b_min} ms (${verdict##* }x, band ${BAND}%) — time the runtime rlib build (rustc -Z time-passes on <temp>/almide-rtlib-*/almide_rt.rs) and the slim main separately (#3829)"
  fail=1
else
  echo "cold-build: ${t_min} ms vs ${b_min} ms (${verdict##* }x, band ${BAND}%) OK"
fi
exit "$fail"
