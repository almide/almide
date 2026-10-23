#!/usr/bin/env bash
# #4009 item 2: run the wasm corpus under the checked allocator
# (crates/almide-wasm/src/checked_alloc.rs) and compare every program with
# its ordinary build.
#
# Under ALMIDE_WASM_CHECKED_ALLOC, `$free` poisons a freed block and
# `$alloc` checks the poison before reusing it. A program that writes into a
# freed block exits 70 with the heap-check message; one that READS a freed
# block sees poison and prints something else. Either shows up here as a
# difference in stdout, stderr or exit status between the two builds.
#
# The negative control proofs/fixtures/heap-check/uaf_write.almd writes
# through a dangling `ffi` address: it must exit 0 in the ordinary build
# (the corruption is silent) and 70 under the switch, or the check has no
# teeth and the gate fails.
#
#   scripts/wasm-checked-alloc.sh [fixture.almd ...]   (default: spec/wasm_cross/*.almd)
#   WASM_CHECKED_SHARD=i/n                              (every n-th fixture from i)
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
ALMIDE="${ALMIDE:-$ROOT/target/debug/almide}"
WASMTIME="${WASMTIME:-wasmtime}"
RUN_TIMEOUT="${RUN_TIMEOUT:-60}"

command -v "$WASMTIME" >/dev/null || { echo "wasm-checked-alloc: $WASMTIME not found" >&2; exit 2; }
[ -x "$ALMIDE" ] || (cargo build -q --bin almide) || exit 2

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

with_timeout() {
  if command -v timeout >/dev/null; then timeout "$RUN_TIMEOUT" "$@"; else "$@"; fi
}

# build <src> <out.wasm> <checked:0|1> → 0 when it built
build() {
  if [ "$3" = 1 ]; then
    ALMIDE_WASM_CHECKED_ALLOC=1 "$ALMIDE" build "$1" --target wasm -o "$2" >/dev/null 2>&1
  else
    env -u ALMIDE_WASM_CHECKED_ALLOC "$ALMIDE" build "$1" --target wasm -o "$2" >/dev/null 2>&1
  fi
}

# run <wasm> <prefix>: writes prefix.out / prefix.err / prefix.rc
run() {
  (cd "$tmp" && with_timeout "$WASMTIME" run --dir=. "$1" </dev/null >"$2.out" 2>"$2.err"; echo $? >"$2.rc")
}

echo "== negative control: a write after free =="
CONTROL="$ROOT/proofs/fixtures/heap-check/uaf_write.almd"
build "$CONTROL" "$tmp/c0.wasm" 0 && build "$CONTROL" "$tmp/c1.wasm" 1 || { echo "FAIL: the control does not build" >&2; exit 1; }
run "$tmp/c0.wasm" "$tmp/c0"; run "$tmp/c1.wasm" "$tmp/c1"
c0=$(cat "$tmp/c0.rc"); c1=$(cat "$tmp/c1.rc")
if [ "$c0" != 0 ] || [ "$c1" != 70 ] || ! grep -q "heap check" "$tmp/c1.err"; then
  echo "FAIL: the control must exit 0 ordinarily and 70 with the heap-check message under the switch (got $c0 and $c1)" >&2
  exit 1
fi
echo "  ok: ordinary build exit 0 (silent corruption); checked build exit 70: $(cat "$tmp/c1.err")"

if [ "$#" -gt 0 ]; then files=("$@"); else files=(spec/wasm_cross/*.almd); fi
shard="${WASM_CHECKED_SHARD:-0/1}"; si="${shard%/*}"; sn="${shard#*/}"

echo "== corpus: ordinary build vs checked build (shard $shard) =="
n=0; built=0; same=0; diff=0; skipped=0
for f in "${files[@]}"; do
  n=$((n + 1))
  [ $(( (n - 1) % sn )) -eq "$si" ] || continue
  if ! build "$f" "$tmp/a.wasm" 0; then skipped=$((skipped + 1)); continue; fi
  if ! build "$f" "$tmp/b.wasm" 1; then
    echo "DIFF $f: builds ordinarily but not under the switch"; diff=$((diff + 1)); continue
  fi
  built=$((built + 1))
  run "$tmp/a.wasm" "$tmp/a"; run "$tmp/b.wasm" "$tmp/b"
  if cmp -s "$tmp/a.out" "$tmp/b.out" && cmp -s "$tmp/a.err" "$tmp/b.err" && cmp -s "$tmp/a.rc" "$tmp/b.rc"; then
    same=$((same + 1))
  else
    diff=$((diff + 1))
    echo "DIFF $f: exit $(cat "$tmp/a.rc") -> $(cat "$tmp/b.rc")"
    head -c 300 "$tmp/b.err" | sed 's/^/    /'
  fi
done
echo "wasm-checked-alloc: $built built, $same identical, $diff different, $skipped not built (no wasm build)"
[ "$diff" -eq 0 ]
