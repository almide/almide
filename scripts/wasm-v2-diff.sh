#!/usr/bin/env bash
#
# WASM engine v2 differential correctness check.
#
# For every `.almd` program with a `fn main`, build it twice — once with the v2
# engine (ALMIDE_WASM_V2=1) and once with the legacy emitter — run both under
# wasmtime, and compare output + exit code. A mismatch on a program the v2
# engine *fully lowered* (did not fall back) is a silent miscompilation: v2
# produced a valid-but-wrong binary. Those are reported as V2-BUG and fail the
# script. Programs where v2 falls back to legacy are byte-identical by
# construction and are reported only in the summary.
#
# "Builds under v2" is not the same as "correct under v2" — this is the gate
# that makes it so. Requires `wasmtime` on PATH and a release `almide` build.
#
# Usage: scripts/wasm-v2-diff.sh [dir ...]   (default: spec examples)
set -u

ALMIDE="${ALMIDE:-./target/release/almide}"
dirs=("$@"); [ ${#dirs[@]} -eq 0 ] && dirs=(spec examples)

command -v wasmtime >/dev/null 2>&1 || { echo "wasmtime not found on PATH — skipping"; exit 0; }
[ -x "$ALMIDE" ] || { echo "almide binary not found at $ALMIDE (run: cargo build --release)"; exit 1; }

leg=$(mktemp); v2=$(mktemp)
trap 'rm -f "$leg" "$v2"' EXIT

# Collect the file list up front: commands in the loop (almide/wasmtime) read
# stdin, so a `while read` over a pipe would have its list eaten.
mapfile -t files < <(find "${dirs[@]}" -name '*.almd' 2>/dev/null | xargs grep -l 'fn main' 2>/dev/null | sort)

ran=0 fb=0 bugs=0 skipped=0
for f in "${files[@]}"; do
  # Skip programs the legacy path can't build standalone (packages, extern, …).
  "$ALMIDE" build "$f" --target wasm -o "$leg" </dev/null >/dev/null 2>&1 || { skipped=$((skipped+1)); continue; }
  v2err=$(ALMIDE_WASM_V2=1 "$ALMIDE" build "$f" --target wasm -o "$v2" </dev/null 2>&1)
  [ -s "$v2" ] || { skipped=$((skipped+1)); continue; }

  legout=$(wasmtime "$leg" </dev/null 2>&1); legrc=$?
  v2out=$(wasmtime "$v2"  </dev/null 2>&1); v2rc=$?

  if echo "$v2err" | grep -q '\[wasm-v2\]'; then
    fb=$((fb+1))
  else
    ran=$((ran+1))
    if [ "$legout" != "$v2out" ] || [ "$legrc" != "$v2rc" ]; then
      bugs=$((bugs+1))
      echo "V2-BUG: $f  (exit legacy=$legrc v2=$v2rc)"
      diff <(printf '%s\n' "$legout") <(printf '%s\n' "$v2out") | sed 's/^/    /' | head -20
    fi
  fi
done

echo "── v2-diff: ran-under-v2=$ran  fell-back=$fb  skipped=$skipped  V2-BUGS=$bugs ──"
[ "$bugs" -eq 0 ]
