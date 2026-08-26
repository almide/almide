#!/usr/bin/env bash
# The DDD gauntlet: every shape the 2026-08-22..26 ports-and-adapters verification
# surfaced on the incumbent, as one runnable matrix.
#
#   ./run.sh [path-to-almide]      (default: `almide` on PATH)
#
# Each row prints: check / native / wasm. `wasm` needs wasmtime on PATH.
set -u
A="${1:-almide}"
HERE="$(cd "$(dirname "$0")" && pwd)"
pass=0; fail=0

row() { # label, dir, entry
  local label="$1" dir="$2" entry="$3" chk nat was
  chk=$(cd "$dir" && "$A" check "$entry" 2>&1 | head -1)
  case "$chk" in "No errors found") chk="ok";; *) chk="REJECT";; esac
  # a cell with no `main` is a CHECK-ONLY cell (it asserts the surface parses)
  if ! grep -qs 'fn main()' "$dir/$entry"; then
    printf '%-34s check=%-7s native=%-5s wasm=%s\n' "$label" "$chk" "-" "-"
    if [ "$chk" = ok ]; then pass=$((pass+1)); else fail=$((fail+1)); fi
    return
  fi
  nat=$(cd "$dir" && "$A" run "$entry" 2>&1)
  if grep -q '^error' <<<"$nat"; then nat="FAIL"; else nat="ok"; fi
  was=$(cd "$dir" && "$A" run "$entry" --target wasm 2>&1)
  if grep -q '^error' <<<"$was"; then
    if grep -q 'not yet supported by the verified wasm renderer' <<<"$was"; then was="WALL"; else was="FAIL"; fi
  else was="ok"; fi
  printf '%-34s check=%-7s native=%-5s wasm=%s\n' "$label" "$chk" "$nat" "$was"
  if [ "$chk" = ok ] && [ "$nat" = ok ] && [ "$was" = ok ]; then pass=$((pass+1)); else fail=$((fail+1)); fi
}

echo "== cells =="
for p in "$HERE"/cells/*.almd; do
  row "$(basename "$p" .almd)" "$(dirname "$p")" "$(basename "$p")"
done
for d in "$HERE"/cells/*/; do
  [ -f "$d/almide.toml" ] || continue
  row "$(basename "$d")" "$d" src/main.almd
done
echo
echo "== the layered package =="
for d in "$HERE"/pkg/*/; do
  row "pkg:$(basename "$d")" "$d" src/main.almd
done
echo
echo "clean rows: $pass   rows with a check/native/wasm gap: $fail"
