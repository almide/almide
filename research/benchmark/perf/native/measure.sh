#!/bin/bash
# Measure native performance metrics for README
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

TMPDIR=$(mktemp -d)
trap "rm -rf '$TMPDIR'" EXIT

# Build a representative CLI app (minigit v2 from MiniGit bench if available,
# otherwise a simple CLI app)
SRC="research/benchmark/perf/native/cli_app.almd"
if [ ! -f "$SRC" ]; then
  echo "Error: $SRC not found"
  exit 1
fi

echo "Building native binary..."
almide build "$SRC" -o "$TMPDIR/app"
cp "$TMPDIR/app" "$TMPDIR/app_stripped"
strip "$TMPDIR/app_stripped"

STRIPPED_SIZE=$(wc -c < "$TMPDIR/app_stripped" | tr -d ' ')
STRIPPED_KB=$(echo "scale=0; $STRIPPED_SIZE / 1024" | bc)

echo ""
echo "Metric|Value"
echo "---|---"
echo "Binary size (CLI app)|**${STRIPPED_KB} KB** (stripped)"

# Runtime: 100 init+add+commit operations
cd "$TMPDIR"
START=$(python3 -c "import time; print(time.time())")
for i in $(seq 1 100); do
  ./app init > /dev/null 2>&1 || true
  echo "file $i" > "f${i}.txt"
  ./app add "f${i}.txt" > /dev/null 2>&1 || true
  ./app commit -m "commit $i" > /dev/null 2>&1 || true
done
END=$(python3 -c "import time; print(time.time())")
RUNTIME=$(python3 -c "print(f'{$END - $START:.1f}')")

echo "Runtime (100 ops)|**${RUNTIME}s**"
echo "Dependencies|**0** (single static binary)"
echo "WASM target|\`almide build app.almd --target wasm\`"
