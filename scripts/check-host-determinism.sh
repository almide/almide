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
# Usage: scripts/check-host-determinism.sh [fixture-dir]   (default: spec/wasm_cross)
set -uo pipefail
cd "$(dirname "$0")/.."

FIXTURE_DIR="${1:-spec/wasm_cross}"
HARNESS="tools/wasmgen-harness"
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
# WALL exit code from the harness: the fixture is not host-nondeterministic, it
# is simply not renderable by v1 yet (#782 — the v0 emitter that used to render
# it is retired). A wall on BOTH hosts is a TRACKED SKIP; a wall on only one host
# is a real host-dependent divergence and still FAILS.
WALL_RC=3
walled=0
for fix in "$FIXTURE_DIR"/*.almd; do
  [ -e "$fix" ] || continue
  name="$(basename "$fix")"
  cp "$fix" "$WORK/in.almd"
  # x86-64/aarch64 host
  "$NATIVE_BIN" "$WORK/in.almd" "$WORK/native.wasm" 2>/dev/null; nrc=$?
  # wasm32 host (compiler running as 32-bit, under wasmtime)
  "$WASMTIME" run --dir "$WORK::/w" "$WASM_BIN" /w/in.almd /w/wasm32.wasm >/dev/null 2>&1; wrc=$?
  if [ "$nrc" -eq "$WALL_RC" ] && [ "$wrc" -eq "$WALL_RC" ]; then
    echo "skip  $name (v1 wall on both hosts — tracked #782)"
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
echo "host-architecture codegen determinism: $n/$n emitted fixtures byte-identical across x86-64 and wasm32 ($walled walled, tracked #782)"
