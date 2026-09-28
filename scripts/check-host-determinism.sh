#!/usr/bin/env bash
# Host-architecture WASM codegen determinism gate.
#
# The compiler runs as wasm32 in the browser playground but as x86-64/aarch64 in
# the test suite. A codegen path whose output depends on host pointer width
# (usize) or HashMap iteration order produces a DIFFERENT — but individually
# stack-/RC-valid — WASM module on a 32-bit host, which can trap at runtime
# (`RuntimeError: unreachable`). The stack-effect verifier and Perceus belt check
# a single module's well-formedness, not reproducibility ACROSS hosts, so they
# are blind to this class. This gate closes that gap: it compiles each fixture
# with the compiler built BOTH natively and to wasm32-wasip1, and asserts the
# emitted WASM is byte-identical.
#
# The compiled path is the STRUCTURAL leg (#2753): the harness drives
# `almide::wasm_route::render_wasm_routed` with force_structural — the renderer
# `--target wasm` ships by default — and compares the structural module plus its
# stock-WASI form. The incumbent renderer is never consulted, so a structural
# decline is a WALL here, not an incumbent module standing in for it.
#
# Usage: scripts/check-host-determinism.sh [fixture-dir]   (default: spec/wasm_cross)
set -uo pipefail

# Byte-order collation, pinned: `sort`'s last-resort comparison follows the ambient
# locale, so an unpinned sort produces different output on differently-configured
# machines. #1031 caught docs/roadmap/README.md changing row order with no content change.
export LC_ALL=C
cd "$(dirname "$0")/.."

FIXTURE_DIR="${1:-spec/wasm_cross}"
HARNESS="tools/wasmgen-harness"
LEDGER="${DETERMINISM_WALLED_LEDGER:-proofs/determinism-walled-baseline.txt}"
source scripts/lib/determinism-walled.sh
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

WASMTIME="$(command -v wasmtime || echo "$HOME/.wasmtime/bin/wasmtime")"
[ -x "$WASMTIME" ] || { echo "::error::wasmtime not found"; exit 2; }

echo "==> Building harness (native)"
cargo build --release --manifest-path "$HARNESS/Cargo.toml" -q || { echo "::error::native harness build failed"; exit 2; }
echo "==> Building harness (wasm32-wasip1)"
cargo build --release --target wasm32-wasip1 --manifest-path "$HARNESS/Cargo.toml" -q || { echo "::error::wasm32 harness build failed"; exit 2; }

NATIVE_BIN="$HARNESS/target/release/wasmgen-harness"
WASM_BIN="$HARNESS/target/wasm32-wasip1/release/wasmgen-harness.wasm"

fail=0; n=0
# WALL exit code from the harness: the fixture is not host-nondeterministic, the
# structural leg declines it. A wall on BOTH hosts is a TRACKED SKIP, pinned by
# name in $LEDGER; a wall on only one host is a real host-dependent divergence
# and still FAILS.
WALL_RC=3
walled=0
: > "$WORK/walled.txt"
for fix in "$FIXTURE_DIR"/*.almd; do
  [ -e "$fix" ] || continue
  name="$(basename "$fix")"
  cp "$fix" "$WORK/in.almd"
  # x86-64/aarch64 host
  "$NATIVE_BIN" "$WORK/in.almd" "$WORK/native.wasm" 2>"$WORK/native.err"; nrc=$?
  # wasm32 host (compiler running as 32-bit, under wasmtime)
  "$WASMTIME" run --dir "$WORK::/w" "$WASM_BIN" /w/in.almd /w/wasm32.wasm >/dev/null 2>&1; wrc=$?
  if [ "$nrc" -eq "$WALL_RC" ] && [ "$wrc" -eq "$WALL_RC" ]; then
    echo "skip  $name (structural wall on both hosts — $(grep -m1 -o 'why: "[^"]*"' "$WORK/native.err" | sed 's/why: //'), $LEDGER)"
    echo "$name" >> "$WORK/walled.txt"
    walled=$((walled+1)); continue
  fi
  if [ "$nrc" -ne "$wrc" ]; then
    echo "FAIL  $name — HOST-DEPENDENT wall (native rc=$nrc, wasm32 rc=$wrc)"
    fail=1; continue
  fi
  if [ "$nrc" -ne 0 ]; then echo "FAIL  $name (harness errored rc=$nrc)"; fail=1; continue; fi
  if cmp -s "$WORK/native.wasm" "$WORK/wasm32.wasm"; then
    echo "ok    $name ($(wc -c < "$WORK/native.wasm" | tr -d ' ') bytes, identical)"
  else
    echo "FAIL  $name — host-arch codegen DIVERGENCE (native $(wc -c < "$WORK/native.wasm" | tr -d ' ')B vs wasm32 $(wc -c < "$WORK/wasm32.wasm" | tr -d ' ')B)"
    fail=1
  fi
  n=$((n+1))
done

echo "----"
if [ "$fail" -ne 0 ]; then
  echo "::error::host-architecture codegen determinism FAILED — the compiler emits different WASM on 32-bit vs 64-bit hosts (the playground runs wasm32). Sort any HashMap/HashSet whose iteration order reaches emitted bytes."
  exit 1
fi

# The gate must not pass VACUOUSLY (#985): `n` counts only fixtures that
# reached the byte-compare, so a renderer regression that walled everything
# printed "0/0 byte-identical" and exited 0. On a green run every corpus file
# is either compared or walled — enforce that identity, and pin `walled` BY
# NAME against the ledger, with MAX_WALLED equal to its row count.
#
# 5 as of 2026-09-28 (#2753), measured with the native harness on origin/develop
# 76e48de5b: 797 emitted + 5 walled of 802. The gate moved from the incumbent
# renderer (almide_mir::pipeline::try_render_wasm_source, MAX_WALLED=30 — the 30
# fixtures the INCUMBENT refused; that ceiling's history is in git before this
# change) to the structural leg. The two numbers count different refusals: all
# 30 incumbent walls now emit and are byte-compared, and the five structural
# walls are fixtures the incumbent rendered, so their reproducibility stopped
# being measured in the move. They are named in the ledger with their cause
# issues (#2744, #2746, #2747); each fix deletes a row and lowers this number.
MAX_WALLED=4
corpus=$(ls "$FIXTURE_DIR"/*.almd 2>/dev/null | wc -l | tr -d ' ')
if [ "$corpus" -eq 0 ] || [ $((n + walled)) -ne "$corpus" ]; then
  echo "::error::host-determinism: compared $n + walled $walled != corpus $corpus in $FIXTURE_DIR — the scan went blind (#985)"
  exit 1
fi
if [ "$walled" -gt "$MAX_WALLED" ]; then
  echo "::error::host-determinism: $walled fixtures walled (ceiling $MAX_WALLED) — coverage shrank; fix the wall or raise MAX_WALLED consciously in the same change (#985)"
  exit 1
fi
rows="$(walled_ledger_rows "$LEDGER")"
if [ "$rows" -ne "$MAX_WALLED" ]; then
  echo "::error::host-determinism: MAX_WALLED=$MAX_WALLED but $LEDGER has $rows rows — the ceiling and the named walls must move together"
  exit 1
fi
check_walled_ledger "$LEDGER" "$WORK/walled.txt" host-determinism || exit 1
echo "host-architecture codegen determinism: $n/$corpus emitted fixtures byte-identical across x86-64 and wasm32 ($walled walled, ceiling $MAX_WALLED)"
