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

# Byte-order collation, pinned: `sort`'s last-resort comparison follows the ambient
# locale, so an unpinned sort produces different output on differently-configured
# machines. #1031 caught docs/roadmap/README.md changing row order with no content change.
export LC_ALL=C
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

# The gate must not pass VACUOUSLY (#985): `n` counts only fixtures that
# reached the byte-compare, so a renderer regression that walled everything
# printed "0/0 byte-identical" and exited 0. On a green run every corpus file
# is either compared or walled — enforce that identity, and ratchet `walled`
# at its real value: 0 as of 2026-07-30 (324 fixtures); 16 as of 2026-08-26 —
# the commissioning switchover GRADUATED the entire legacy wall corpus into
# spec/wasm_cross (the structural leg renders them; the incumbent still
# walls them, so they are tracked skips of the INCUMBENT'S determinism
# domain, locally re-measured at exactly 16 over 634 fixtures). A NEW wall
# is a conscious ceiling bump, never silent shrinkage of coverage.
# 18 as of 2026-08-30 (second bump): env_sleep_pause.almd (C-327) walls on
# the incumbent brick (env.sleep_ms has no capability seat there) — same
# structural-only division, prunes with #1696 steps 4-5.
# 17 as of 2026-08-30: gzip_inflate_members.almd (C-326) is STRUCTURAL-ONLY
# — the incumbent's brick walls its loop-level tuple write-back
# (WhileHeapAccumulator), which is the intended division of labor until
# #1696 steps 4-5 move the certificate and retire the incumbent.
# 19 as of 2026-09-01: env_set_overlay.almd (C-329) — the determinism
# harnesses build the almide.* module without the env host surface, so the
# env.set/get fixture walls here; it executes on the embedded + stock-p1
# sweeps (#1716), which is where its promise lives.
# 20 as of 2026-09-01: zlib_selfhost.almd (C-331/#1700) — the incumbent
# walls the promoted C-326 decoder's tuple write-back loops, the same
# division the gzip_inflate_members rows record; prunes with #1696 4-5.
# 22 as of 2026-09-02: list_rest_pattern.almd (C-332) and
# as_pattern.almd (C-333) — the incumbent brick walls both #1461 forms
# honestly (the structural leg is the default route and lowers them);
# prunes with #1696 4-5.
# 23 as of 2026-09-02: generic_record_fn_field.almd (C-092/#1676) — the
# incumbent brick refuses the return-position computed call on the
# instantiated closure field (the structural leg, the default route,
# lowers it); prunes with #1696 4-5.
# 24 as of 2026-09-02: list_unique_by_nonscalar_key.almd (C-053/#1797) —
# the incumbent routes a non-scalar unique_by key to its unlinked `_x`
# render wall (C-147; a render-phase refusal the walled-real ratchet
# already classes "(b) acceptable", so no baseline row); the structural
# leg lowers every equatable key. Prunes with #1696 4-5.
# 25 as of 2026-09-03: bytes_temp_receiver.almd (C-213/#1849) — a TEMPORARY
# receiver of a Unit-returning bytes mutator; the incumbent brick walls it
# honestly (the receiver discipline names the call-result receiver, and the
# `let _ =` Unit binding is outside its value subset) where the structural
# leg, the default route, releases the mutated block. Prunes with #1696 4-5.
# 26 as of 2026-09-03: mut_param_effect_can_err.almd (C-132/#1576) — the
# can-err effect fn with a `mut` param takes the move-mode tuple rewrite
# (`(T, Buf)` on the ok payload); the incumbent brick walls the synthesized
# `let (r, b) = call!` destructure-unwrap honestly (`unwrap `!` in a
# call-argument position`) where the structural leg, the default route,
# lowers every cell byte-identical to native. Prunes with #1696 4-5.
MAX_WALLED=27
corpus=$(ls "$FIXTURE_DIR"/*.almd 2>/dev/null | wc -l | tr -d ' ')
if [ "$corpus" -eq 0 ] || [ $((n + walled)) -ne "$corpus" ]; then
  echo "::error::host-determinism: compared $n + walled $walled != corpus $corpus in $FIXTURE_DIR — the scan went blind (#985)"
  exit 1
fi
if [ "$walled" -gt "$MAX_WALLED" ]; then
  echo "::error::host-determinism: $walled fixtures walled (ceiling $MAX_WALLED) — coverage shrank; fix the wall or raise MAX_WALLED consciously in the same change (#985)"
  exit 1
fi
echo "host-architecture codegen determinism: $n/$corpus emitted fixtures byte-identical across x86-64 and wasm32 ($walled walled, ceiling $MAX_WALLED)"
