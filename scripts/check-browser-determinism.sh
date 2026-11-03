#!/usr/bin/env bash
# Browser-ABI codegen gate. The playground runs the compiler compiled to
# wasm32-unknown-unknown (no WASI, no std::time, JS-shimmed). This builds the
# SAME compile path to that target via wasm-bindgen + node and asserts, for each
# fixture, that compilation (1) does NOT panic and (2) emits byte-identical bytes
# to the native compiler. Catches both wasm32-unknown-unknown-only failures
# (e.g. an unconditional std::time::Instant::now()) and host-width codegen
# divergence that the wasip1 gate can mask.
#
# The compiled path is the STRUCTURAL leg (#2753): both harnesses drive
# `almide::wasm_route::render_wasm_routed` with skip_stock_audit — the renderer
# `--target wasm` ships by default — and emit the structural module plus its
# stock-WASI form. The incumbent renderer is never consulted.
#
# Requires: wasm-pack, node. Skips with a warning if either is missing.
set -uo pipefail
cd "$(dirname "$0")/.."

FIXTURE_DIR="${1:-spec/wasm_cross}"
NATIVE_HARNESS="tools/wasmgen-harness"
UU_HARNESS="tools/wasmgen-harness-uu"
LEDGER="${DETERMINISM_WALLED_LEDGER:-proofs/determinism-walled-baseline.txt}"
source scripts/lib/determinism-walled.sh
source scripts/lib/determinism-select.sh
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

# One node process compiles a whole list of fixtures (`<index>\t<path>` per
# line), each on a FRESH instance of the browser compiler: the package is
# dropped from the require cache and loaded again before every fixture, so no
# state carries from one compile to the next, as when each fixture had its
# own `node` process. V8 keeps the compiled code of identical wasm bytes for
# the life of the process, so only the first load pays the module compile —
# the cost that a node start per fixture paid 1012 times (~15 min of the job,
# and V8 compiles on every core, so running those starts side by side bought
# nothing: develop run 41604028581). Per fixture it writes what the old
# per-process run left behind: `uu.wasm` on success, the thrown value in `err`
# (a structured `wall: …` refusal or a panic's RuntimeError) and the exit
# status in `brc`.
cat > "$WORK/batch.js" <<'JS'
const fs = require('fs');
const path = require('path');
const pkgDir = path.resolve(process.argv[2]);
const lines = fs.readFileSync(process.argv[3], 'utf8').split('\n').filter(Boolean);
const work = process.argv[4];
function fresh() {
  for (const k of Object.keys(require.cache)) if (k.startsWith(pkgDir)) delete require.cache[k];
  return require(pkgDir);
}
let errText = '';
console.error = (...a) => { errText += a.map(String).join(' ') + '\n'; };
for (const line of lines) {
  const tab = line.indexOf('\t');
  const dir = path.join(work, 'fix', line.slice(0, tab));
  const fixture = line.slice(tab + 1);
  errText = '';
  let rc = 0;
  try {
    const pkg = fresh();
    const bytes = pkg.compile_source_to_wasm(fs.readFileSync(fixture, 'utf8'));
    fs.writeFileSync(path.join(dir, 'uu.wasm'), Buffer.from(bytes));
  } catch (e) {
    errText += (e && e.stack ? e.stack : String(e)) + '\n';
    rc = 1;
  }
  fs.writeFileSync(path.join(dir, 'err'), errText);
  fs.writeFileSync(path.join(dir, 'brc'), rc + '\n');
}
JS

NATIVE_BIN="$NATIVE_HARNESS/target/release/wasmgen-harness"
fail=0; n=0
# Tracked-skip ceiling, shared with check-host-determinism.sh and pinned BY
# NAME in the same ledger ($LEDGER), with MAX_WALLED equal to its row count.
#
# 5 as of 2026-09-28 (#2753), measured with the native harness on origin/develop
# 76e48de5b: 797 emitted + 5 walled of 802. The gate moved from the incumbent
# renderer (MAX_WALLED=30 — the fixtures the INCUMBENT refused; that history is
# in git before this change) to the structural leg. All 30 incumbent walls now
# emit and are compared; the five structural walls are fixtures the incumbent
# rendered, so their browser-ABI reproducibility stopped being measured in the
# move. The ledger names them with their cause issues (#2744, #2746, #2747).
MAX_WALLED=0
walled=0
: > "$WORK/walled.txt"
determinism_select "$FIXTURE_DIR" "$WORK/selected.txt"
# Both compiles of every fixture run before the judging loop, each fixture in
# its own directory; the verdicts below are read back in corpus order, so the
# log and the judgement are what the one-at-a-time loop produced. Native: one
# harness run per fixture on parallel workers. Browser: DETERMINISM_JOBS node
# processes (default: cores), each compiling every JOBS-th fixture (batch.js).
JOBS="${DETERMINISM_JOBS:-$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)}"
compile_native() { # index fixture
  d="$WORK/fix/$1"; mkdir -p "$d"
  [ -e "$2" ] || return 0
  "$NATIVE_BIN" "$2" "$d/native.wasm" 2>"$d/native.err"; echo $? > "$d/nrc"
}
export -f compile_native
export WORK NATIVE_BIN
awk '{ print NR; print $0 }' "$WORK/selected.txt" | tr '\n' '\0' \
  | xargs -0 -n 2 -P "$JOBS" bash -c 'compile_native "$1" "$2"' _
# Only fixtures that exist go to the browser batches (the judging loop skips
# the rest, as before).
for w in $(seq 0 $((JOBS - 1))); do
  awk -v w="$w" -v n="$JOBS" '(NR - 1) % n == w { if (system("test -e \"" $0 "\"") == 0) print NR "\t" $0 }' \
    "$WORK/selected.txt" > "$WORK/batch-$w.txt"
done
pids=()
for w in $(seq 0 $((JOBS - 1))); do
  node "$WORK/batch.js" "$WORK/pkg" "$WORK/batch-$w.txt" "$WORK" 2>"$WORK/batch-$w.err" &
  pids+=("$!")
done
for w in $(seq 0 $((JOBS - 1))); do
  if ! wait "${pids[$w]}"; then
    echo "::error::browser batch $w crashed outside a compile — its log follows"; cat "$WORK/batch-$w.err"; exit 2
  fi
done
echo "==> compiled $(wc -l < "$WORK/selected.txt" | tr -d ' ') fixture(s): native on $JOBS worker(s), browser in $JOBS node batch(es)"
i=0
while read -r fix <&3; do
  i=$((i+1))
  [ -e "$fix" ] || continue
  name="$(basename "$fix")"
  d="$WORK/fix/$i"
  # The judgement below reads the files a single-fixture run wrote.
  cp "$d/native.err" "$WORK/native.err"; cp "$d/err" "$WORK/err"
  [ -e "$d/native.wasm" ] && cp "$d/native.wasm" "$WORK/native.wasm" || rm -f "$WORK/native.wasm"
  [ -e "$d/uu.wasm" ] && cp "$d/uu.wasm" "$WORK/uu.wasm" || rm -f "$WORK/uu.wasm"
  nrc=$(cat "$d/nrc"); brc=$(cat "$d/brc")
  # The native harness exits 3 on a structural WALL (a tracked skip, named in
  # $LEDGER). The browser twin must refuse the same fixture with its own
  # structured wall (`wall: …`, a JS throw): both ABIs walling is the SAME
  # renderer agreeing with itself. A browser PANIC where native walls is not
  # agreement and FAILS, and one ABI rendering what the other walls is a real
  # ABI divergence and FAILS.
  if [ "$nrc" -eq 3 ]; then
    if [ "$brc" -ne 0 ] && grep -q 'wall: ' "$WORK/err"; then
      echo "skip  $name (structural wall on both ABIs — $(grep -m1 -o 'why: "[^"]*"' "$WORK/native.err" | sed 's/why: //'), $LEDGER)"
      echo "$name" >> "$WORK/walled.txt"
      walled=$((walled+1)); continue
    fi
    if [ "$brc" -ne 0 ]; then
      echo "FAIL  $name — native walls but the browser compile failed without a wall: $(grep -iE 'panic|unreachable|RuntimeError|Error' "$WORK/err" | head -1)"
      fail=1; continue
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
done 3< "$WORK/selected.txt"

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
rows="$(walled_ledger_rows "$LEDGER")"
if [ "$rows" -ne "$MAX_WALLED" ]; then
  echo "::error::browser-determinism: MAX_WALLED=$MAX_WALLED but $LEDGER has $rows rows — the ceiling and the named walls must move together"
  exit 1
fi
check_walled_ledger "$LEDGER" "$WORK/walled.txt" browser-determinism "$WORK/selected.txt" || exit 1
corpus=$(wc -l < "$WORK/selected.txt" | tr -d ' ')
if [ "$corpus" -eq 0 ] || [ "$((n + walled))" -ne "$corpus" ]; then
  echo "::error::browser-determinism: compared $n + skipped $walled of $corpus fixtures in $FIXTURE_DIR — the scan went blind (#985)"
  exit 1
fi
echo "browser-ABI determinism: $n compared + $walled tracked-skips of $corpus fixtures — byte-identical to native"
