#!/bin/bash
# Measure WASM binary sizes for standard programs
set -euo pipefail
cd "$(dirname "$0")"

TMPDIR=$(mktemp -d)
trap "rm -rf '$TMPDIR'" EXIT

echo "Program|Binary Size"
echo "---|---:"

for src in hello.almd fizzbuzz.almd fibonacci.almd closure.almd variant.almd; do
  name=$(basename "$src" .almd)
  label=$(echo "$name" | sed 's/^./\U&/' | sed 's/_/ /g')
  # Map filenames to display names
  case "$name" in
    hello) label="Hello World" ;;
    fizzbuzz) label="FizzBuzz" ;;
    fibonacci) label="Fibonacci" ;;
    closure) label="Closure" ;;
    variant) label="Variant" ;;
  esac

  out="$TMPDIR/${name}.wasm"
  if almide build "$src" --target wasm -o "$out" > /dev/null 2>&1; then
    size=$(wc -c < "$out" | tr -d ' ')
    # Format with comma separator
    formatted=$(printf "%'d" "$size")
    echo "$label|**${formatted} B**"
  else
    echo "$label|BUILD FAILED"
  fi
done
