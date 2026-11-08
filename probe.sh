#!/usr/bin/env bash
set -u
OUT=probe-out; mkdir -p $OUT
cargo build -q -p almide-mir --example emit_native_bundle || exit 1
while read -r f; do
  [ -z "$f" ] && continue
  n="$(basename "$f" .almd)"
  target/debug/examples/emit_native_bundle "$f" "$OUT/$n.rs" > "$OUT/$n.bundle" 2> "$OUT/$n.err"
done < "$1"
ls -la $OUT
