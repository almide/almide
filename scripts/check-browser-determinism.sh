#!/usr/bin/env bash
# Browser-ABI codegen gate. The playground runs the compiler compiled to
# wasm32-unknown-unknown (no WASI, no std::time, JS-shimmed). This builds the
# SAME compile path to that target via wasm-bindgen + node and asserts, for each
# fixture, that compilation (1) does NOT panic and (2) emits byte-identical bytes
# to the native compiler. Catches both wasm32-unknown-unknown-only failures
# (e.g. an unconditional std::time::Instant::now()) and host-width codegen
# divergence that the wasip1 gate can mask.
#
# Requires: wasm-pack, node. Skips with a warning if either is missing.
set -uo pipefail
cd "$(dirname "$0")/.."

FIXTURE_DIR="${1:-spec/wasm_cross}"
NATIVE_HARNESS="tools/wasmgen-harness"
UU_HARNESS="tools/wasmgen-harness-uu"
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT

# In CI a missing tool is a FAILURE, not a skip (#985): the invoking job
# installs node + wasm-pack, so their absence there means the gate silently
# stopped gating. Locally the skip stays.
missing_tool() {
  if [ "${CI:-}" = "true" ]; then
    echo "::error::browser-determinism: $1 not found — in CI a missing tool is a failure (#985)"
    exit 1
  fi
  echo "::warning::$1 not found — skipping browser-ABI determinism gate"
  exit 0
}
command -v wasm-pack >/dev/null || missing_tool wasm-pack
command -v node      >/dev/null || missing_tool node

echo "==> Building native harness"
cargo build --release --manifest-path "$NATIVE_HARNESS/Cargo.toml" -q || { echo "::error::native harness build failed"; exit 2; }
echo "==> Building browser harness (wasm32-unknown-unknown via wasm-pack)"
# Capture the build log and print it on failure — a gate that fails without
# showing WHY violates the project's own diagnostics principle (it cost a
# debugging round-trip when this failed on CI but built fine locally).
( cd "$UU_HARNESS" && wasm-pack --version && wasm-pack build --target nodejs --out-dir "$WORK/pkg" ) > "$WORK/uu-build.log" 2>&1 \
  || { echo "::error::wasm32-unknown-unknown harness build failed — log tail follows"; tail -100 "$WORK/uu-build.log"; exit 2; }

cat > "$WORK/run.js" <<'JS'
const pkg = require(process.argv[2]);
const fs = require('fs');
const src = fs.readFileSync(process.argv[3], 'utf8');
const bytes = pkg.compile_source_to_wasm(src);   // throws (panics) → nonzero exit
fs.writeFileSync(process.argv[4], Buffer.from(bytes));
JS

NATIVE_BIN="$NATIVE_HARNESS/target/release/wasmgen-harness"
fail=0; n=0
# Tracked-skip ceiling, shared with check-host-determinism.sh: 16 as of the
# 2026-08-26 commissioning (the graduated wall corpus — locally re-measured);
# 17 as of 2026-08-30 — gzip_inflate_members.almd (C-326) is structural-only,
# the incumbent walls its loop-level tuple write-back (see the host twin and
# proofs/walled-real-baseline.txt; prunes with #1696 steps 4-5).
# 19 as of 2026-09-01: env_set_overlay.almd (C-329) — the determinism
# harnesses build the almide.* module without the env host surface, so the
# env.set/get fixture walls here; it executes on the embedded + stock-p1
# sweeps (#1716), which is where its promise lives.
# 20 as of 2026-09-01: zlib_selfhost.almd (C-330/#1700) — the incumbent
# walls the promoted C-326 decoder's tuple write-back loops, the same
# division the gzip_inflate_members rows record; prunes with #1696 4-5.
MAX_WALLED=20
walled=0
for fix in "$FIXTURE_DIR"/*.almd; do
  [ -e "$fix" ] || continue
  name="$(basename "$fix")"
  "$NATIVE_BIN" "$fix" "$WORK/native.wasm" 2>/dev/null; nrc=$?
  node "$WORK/run.js" "$WORK/pkg" "$fix" "$WORK/uu.wasm" 2>"$WORK/err"; brc=$?
  # The harness exits 3 on a v1 WALL (a tracked skip — the incumbent does
  # not render the fixture; the commissioned structural leg does). Both
  # legs walling is the SAME renderer agreeing with itself; one leg
  # rendering what the other walls is a real ABI divergence and FAILS.
  if [ "$nrc" -eq 3 ]; then
    if [ "$brc" -ne 0 ]; then
      echo "skip  $name (v1 wall on both ABIs — commissioned-leg fixture)"
      walled=$((walled+1)); continue
    fi
    echo "FAIL  $name — browser ABI rendered what the native harness walls"
    fail=1; continue
  fi
  if [ "$nrc" -ne 0 ]; then echo "FAIL  $name (native errored rc=$nrc)"; fail=1; continue; fi
  if [ "$brc" -ne 0 ]; then
    echo "FAIL  $name — browser compile PANICKED: $(grep -iE 'panic|unreachable|RuntimeError' "$WORK/err" | head -1)"
    fail=1; continue
  fi
  if cmp -s "$WORK/native.wasm" "$WORK/uu.wasm"; then
    echo "ok    $name ($(wc -c < "$WORK/native.wasm" | tr -d ' ') bytes, identical)"
  else
    echo "FAIL  $name — browser vs native codegen DIVERGENCE (native $(wc -c < "$WORK/native.wasm" | tr -d ' ')B vs browser $(wc -c < "$WORK/uu.wasm" | tr -d ' ')B)"
    fail=1
  fi
  n=$((n+1))
done

echo "----"
if [ "$fail" -ne 0 ]; then
  echo "::error::browser-ABI codegen gate FAILED — the compiler panics or diverges when built to wasm32-unknown-unknown (the playground target). Common causes: unconditional std::time/Instant in the compile path, or HashMap iteration reaching emitted bytes."
  exit 1
fi
# No vacuous pass (#985): on a green run every corpus file was compared, so an
# empty/renamed FIXTURE_DIR (n=0) is a broken scan, not a win.
if [ "$walled" -gt "$MAX_WALLED" ]; then
  echo "::error::browser-determinism: $walled fixtures walled (ceiling $MAX_WALLED) — coverage shrank; fix the wall or raise MAX_WALLED consciously in the same change (#985)"
  exit 1
fi
corpus=$(ls "$FIXTURE_DIR"/*.almd 2>/dev/null | wc -l | tr -d ' ')
if [ "$corpus" -eq 0 ] || [ "$((n + walled))" -ne "$corpus" ]; then
  echo "::error::browser-determinism: compared $n + skipped $walled of $corpus fixtures in $FIXTURE_DIR — the scan went blind (#985)"
  exit 1
fi
echo "browser-ABI determinism: $n compared + $walled tracked-skips of $corpus fixtures — byte-identical to native"
