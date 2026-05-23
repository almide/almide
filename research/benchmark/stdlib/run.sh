#!/bin/bash
set -e
DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DIR"
VERSION="${1:-$(almide --version 2>&1 | awk '{print $2}')}"
echo "=== Almide stdlib benchmark v${VERSION} ==="
echo ""
RESULTS=""
for f in bench_*.almd; do
  NAME="${f%.almd}"
  almide build "$f" -o "$NAME" --release 2>/dev/null
  echo "--- ${NAME#bench_} ---"
  OUTPUT=$(./"$NAME" 2>&1)
  echo "$OUTPUT"
  RESULTS="${RESULTS}${OUTPUT}\n"
  echo ""
done
echo "$RESULTS" > "results/${VERSION}.txt"
echo "Results saved to results/${VERSION}.txt"
