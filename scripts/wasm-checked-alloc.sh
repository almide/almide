#!/usr/bin/env bash
# #4009 item 2: run the wasm corpus under the checked allocator
# (crates/almide-wasm/src/checked_alloc.rs) and compare every program with
# its ordinary build.
#
# Under ALMIDE_WASM_CHECKED_ALLOC, `$free` poisons a freed block and
# `$alloc` checks the poison before reusing it, and every block carries a
# canary past its usable capacity that `$free` checks. A program that writes
# into a freed block, or past the end of a block, exits 70 with a heap-check
# message; one that READS a freed block sees poison and prints something
# else. Either shows up here as a difference in stdout, stderr or exit
# status between the two builds.
#
# Two negative controls must exit 0 in the ordinary build (the corruption is
# silent) and 70 with their message under the switch, or the check has no
# teeth and the gate fails: proofs/fixtures/heap-check/raw/uaf_write.almd
# writes through a dangling address, and overrun_write.almd writes 16 bytes
# into a 4-byte buffer. Since ADR-0031 no user program can spell either (a
# Region holds its owner and every access is bounds-checked), so they are
# written in the primitive floor and built with `--stdlib-authority` on their
# directory. The Region mechanisms themselves are re-injected by
# scripts/wasm-checked-alloc-reinject.sh (ffi-region-*.patch).
#
# The sandbox (#4224): each run gets the cwd preopened as `.` and a FRESH,
# real temp root preopened at /tmp — the guest's `$TMPDIR` is unset, so its
# temp root is /tmp. Without one, `fs.create_temp_dir` is refused in both
# builds and every fs fixture compares two refusals. A sandbox control must
# run proofs/fixtures/heap-check/temp_root.almd to exit 0 under the harness,
# and its negative leg must see the same program refused without the temp
# root, so the control can fail.
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

# build <src> <out.wasm> <checked:0|1> [extra flags…] → 0 when it built
build() {
  local src="$1" out="$2" checked="$3"; shift 3
  if [ "$checked" = 1 ]; then
    ALMIDE_WASM_CHECKED_ALLOC=1 "$ALMIDE" build "$src" --target wasm -o "$out" "$@" >/dev/null 2>&1
  else
    env -u ALMIDE_WASM_CHECKED_ALLOC "$ALMIDE" build "$src" --target wasm -o "$out" "$@" >/dev/null 2>&1
  fi
}

# run <wasm> <prefix>: writes prefix.out / prefix.err / prefix.rc, with a
# fresh temp root at /tmp (each build starts from the same empty one).
run() {
  rm -rf "$tmp/troot" && mkdir "$tmp/troot"
  (cd "$tmp" && with_timeout "$WASMTIME" run --dir=. --dir="$tmp/troot::/tmp" "$1" </dev/null >"$2.out" 2>"$2.err"; echo $? >"$2.rc")
}

# run_bare <wasm> <prefix>: the same without the temp root (the sandbox
# control's negative leg).
run_bare() {
  (cd "$tmp" && with_timeout "$WASMTIME" run --dir=. "$1" </dev/null >"$2.out" 2>"$2.err"; echo $? >"$2.rc")
}

echo "== sandbox control: a program that makes its own temp dir runs under the harness =="
build "$ROOT/proofs/fixtures/heap-check/temp_root.almd" "$tmp/s.wasm" 0 || { echo "FAIL: the sandbox control does not build" >&2; exit 1; }
run "$tmp/s.wasm" "$tmp/s"
if [ "$(cat "$tmp/s.rc")" != 0 ] || [ "$(cat "$tmp/s.out")" != "temp root served" ]; then
  echo "FAIL: under the harness's sandbox the temp-root program exits $(cat "$tmp/s.rc"): $(head -c 300 "$tmp/s.err")" >&2
  exit 1
fi
rm -rf "$tmp/tmp"
run_bare "$tmp/s.wasm" "$tmp/s0"
if [ "$(cat "$tmp/s0.rc")" = 0 ] || ! grep -q "create_temp_dir" "$tmp/s0.err"; then
  echo "FAIL: without a temp root the sandbox control did not fail (exit $(cat "$tmp/s0.rc")) — the control has no teeth" >&2
  exit 1
fi
echo "  ok: with the temp root exit 0 ('temp root served'); without it exit $(cat "$tmp/s0.rc"): $(head -c 160 "$tmp/s0.err")"

# control <fixture> <message fragment> <what it does>
control() {
  echo "== negative control: $3 =="
  local dir="$ROOT/proofs/fixtures/heap-check/raw" f
  f="$dir/$1"
  build "$f" "$tmp/c0.wasm" 0 --stdlib-authority "$dir" && build "$f" "$tmp/c1.wasm" 1 --stdlib-authority "$dir" \
    || { echo "FAIL: the control $1 does not build" >&2; exit 1; }
  run "$tmp/c0.wasm" "$tmp/c0"; run "$tmp/c1.wasm" "$tmp/c1"
  local c0 c1; c0=$(cat "$tmp/c0.rc"); c1=$(cat "$tmp/c1.rc")
  if [ "$c0" != 0 ] || [ "$c1" != 70 ] || ! grep -q "$2" "$tmp/c1.err"; then
    echo "FAIL: the control $1 must exit 0 ordinarily and 70 with '$2' under the switch (got $c0 and $c1)" >&2
    exit 1
  fi
  echo "  ok: ordinary build exit 0 (silent corruption); checked build exit 70: $(cat "$tmp/c1.err")"
}
control uaf_write.almd "a freed block was written" "a write after free"
control overrun_write.almd "ran past the end of a block" "a write past the end of a block"

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
