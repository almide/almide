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

command -v wasm-pack >/dev/null || { echo "::warning::wasm-pack not found — skipping browser-ABI determinism gate"; exit 0; }
command -v node      >/dev/null || { echo "::warning::node not found — skipping browser-ABI determinism gate"; exit 0; }

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
for fix in "$FIXTURE_DIR"/*.almd; do
  [ -e "$fix" ] || continue
  name="$(basename "$fix")"
  "$NATIVE_BIN" "$fix" "$WORK/native.wasm" 2>/dev/null || { echo "FAIL  $name (native errored)"; fail=1; continue; }
  if ! node "$WORK/run.js" "$WORK/pkg" "$fix" "$WORK/uu.wasm" 2>"$WORK/err"; then
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
echo "browser-ABI determinism: $n/$n fixtures compile without panic and byte-identical to native"
