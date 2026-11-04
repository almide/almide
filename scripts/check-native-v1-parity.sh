#!/usr/bin/env bash
# NATIVE v1 OUTPUT PARITY (#4006): every program on the native verified floor,
# built by the certified native render (`almide build`, v1) and by the
# standard codegen (`--no-verified`, v0), must print the same stdout and exit
# with the same code. The certificates judge what the render builds from
# (ownership, call modes, bounds); they say nothing of what a program prints,
# and a render rule outside the MIR (a fan run on threads, a formatter) can
# change it. This holds every certified program to the shipped pipeline.
#
# Fails on a differing program, a build failure of either leg, or a floor
# program the v1 build no longer certifies. A program with a timed effect or
# a nondeterministic print would need an allowance; there is none, so such a
# program on the floor fails here until it is made deterministic.
set -uo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

ALMIDE="${ALMIDE_BIN:-target/release/almide}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
[ -x "$ALMIDE" ] || { echo "native-v1-parity: no compiler at $ALMIDE"; exit 2; }

check() {
  local f="$1" n out1 out0 c1 c0
  n="$(echo "$f" | tr '/.' '__')"
  if ! log=$(ALMIDE_VERIFIED_DEBUG=1 timeout 600 "$ALMIDE" build "$f" -o "$WORK/$n.v1" 2>&1); then
    echo "BUILD-FAIL v1 $f"; return 1
  fi
  grep -q "native: v1 trust-spine render" <<<"$log" || { echo "UNCERTIFIED $f"; return 1; }
  if ! ALMIDE_NO_VERIFIED_OK=1 timeout 600 "$ALMIDE" build "$f" --no-verified -o "$WORK/$n.v0" >/dev/null 2>&1; then
    echo "v1-only $f"; return 0
  fi
  # Each leg runs in a fresh directory of its own: a program that writes a
  # file sees the same empty tree on both.
  mkdir -p "$WORK/$n.d1" "$WORK/$n.d0"
  out1=$(cd "$WORK/$n.d1" && timeout 120 "../$n.v1" </dev/null 2>/dev/null); c1=$?
  out0=$(cd "$WORK/$n.d0" && timeout 120 "../$n.v0" </dev/null 2>/dev/null); c0=$?
  if [ "$out1" != "$out0" ] || [ "$c1" != "$c0" ]; then
    echo "DIFFERS $f (exit v1=$c1 v0=$c0)"
    diff <(echo "$out0") <(echo "$out1") | head -6
    return 1
  fi
  echo "ok $f"
}
export -f check
export ALMIDE WORK
grep -E '^spec/' proofs/native-verified-floor.txt | xargs -P "${NATIVE_V1_PARITY_JOBS:-8}" -I{} bash -c 'check "$@"' _ {} > "$WORK/report.txt"
grep -v '^ok ' "$WORK/report.txt"
n_ok=$(grep -c '^ok ' "$WORK/report.txt")
if grep -qE '^(DIFFERS|BUILD-FAIL|UNCERTIFIED)' "$WORK/report.txt"; then
  echo "native-v1-parity: FAILED ($n_ok equal)"
  exit 1
fi
echo "native-v1-parity: ok ($n_ok equal)"
