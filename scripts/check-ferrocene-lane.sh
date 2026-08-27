#!/usr/bin/env bash
# The qualified-toolchain lane (#573): Almide-generated Rust for the spec
# corpus must build green under the FERROCENE-PINNED rustc, and must never
# smuggle a nightly feature into generated code.
#
# The split of qualification responsibility this lane evidences:
#   - Almide  = the code GENERATOR (what a tool-qualification package
#     covers — #574);
#   - the Rust compiler below it = Ferrocene's qualification (ISO 26262
#     ASIL-D / IEC 61508 SIL-4).
# Riding a qualified backend shrinks "qualify a whole new compiler" to
# "qualify a code generator".
#
# THE PIN (mirrors als ADR-0015 clause 4, als/ref/rust-toolchain.toml):
#   Ferrocene 26.05.0 (released 2026-07-28) == upstream Rust 1.95.0.
# Until a Ferrocene subscription exists, CI builds with STOCK rustc of the
# same version — chosen so the day `criticalup` arrives the lane rebuilds
# under the qualified binary WITHOUT a code or version change. Bump the
# pin here and in .github/workflows/ferrocene-lane.yml together when
# Ferrocene moves.
#
# Usage:
#   ALMIDE_BIN=path/to/almide bash scripts/check-ferrocene-lane.sh [N]
# N = fixture cap (default: whole run-manifest corpus). The CI lane sets
# RUSTUP_TOOLCHAIN=1.95.0 after installing that toolchain; locally the
# script runs with whatever `cargo` dispatches to (the MECHANICS smoke —
# the pin itself lives in CI, and the header records why).

set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."

BIN="${ALMIDE_BIN:-target/release/almide}"
"$BIN" --version >/dev/null || { echo "FAIL: almide binary not runnable: $BIN" >&2; exit 2; }
CAP="${1:-0}"

MANIFEST="crates/almide-spine/tests/golden/spec-run-manifest.txt"
[ -f "$MANIFEST" ] || { echo "FAIL: $MANIFEST missing" >&2; exit 2; }

command -v cargo >/dev/null || { echo "FAIL: cargo not on PATH" >&2; exit 2; }
echo "ferrocene-lane: generated-Rust compile under $(rustc --version)"
if [ -n "${RUSTUP_TOOLCHAIN:-}" ]; then
  echo "ferrocene-lane: RUSTUP_TOOLCHAIN=$RUSTUP_TOOLCHAIN (the qualified pin's upstream)"
fi

tmp_rs=$(mktemp)
trap 'rm -f "$tmp_rs"' EXIT

total=0
nightly=0
build_fail=0
while IFS=$'\t' read -r _hash _exit path; do
  [ -n "$path" ] || continue
  [ -f "$path" ] || continue
  total=$((total + 1))
  if [ "$CAP" -gt 0 ] && [ "$total" -gt "$CAP" ]; then
    total=$((total - 1))
    break
  fi
  # 1. The nightly-feature gate is STATIC and toolchain-independent:
  #    generated code must never contain a feature gate.
  if ! "$BIN" "$path" --target rust > "$tmp_rs" 2>/dev/null; then
    # A fixture the Rust emitter refuses is not this lane's failure —
    # the lane judges what IS generated. Count and continue.
    continue
  fi
  if grep -q '^#!\[feature(' "$tmp_rs"; then
    echo "FAIL: $path — generated Rust carries a nightly feature gate" >&2
    nightly=$((nightly + 1))
    continue
  fi
  # 2. The generated program must BUILD under the lane's toolchain (the
  #    shared build dir keeps this incremental across the corpus).
  if ! out=$("$BIN" build "$path" -o /tmp/ferrocene-lane-out 2>&1); then
    echo "FAIL: $path — generated Rust did not build under the lane toolchain" >&2
    # The compiler's own words, or the finding is not actionable from a
    # machine without this toolchain (the gen-claims diagnosability rule).
    printf '%s
' "$out" | grep -B2 -A8 "^error" | head -40 >&2
    build_fail=$((build_fail + 1))
  fi
done < "$MANIFEST"

echo "ferrocene-lane: $total fixture(s) — nightly-feature offences $nightly, build failures $build_fail"
if [ "$nightly" -ne 0 ] || [ "$build_fail" -ne 0 ]; then
  exit 1
fi
echo "ferrocene-lane OK"
