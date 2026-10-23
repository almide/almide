#!/usr/bin/env bash
# #4009 item 3: the checked-allocator gate must catch the memory bugs the wasm
# leg actually shipped. Each proofs/fixtures/heap-check/reinject/<name>.patch
# puts one fixed bug back into the emitter; <name>.almd is its reproduction.
# For each, in a throwaway worktree: apply the patch, build, and run
# scripts/wasm-checked-alloc.sh on the reproduction — it must FAIL (the
# ordinary and checked builds disagree, or the check exits 70). A patch that
# no longer applies fails this script too: regenerate it against the fix's
# current code rather than letting the control go stale.
#
#   scripts/wasm-checked-alloc-reinject.sh [name ...]   (default: every patch)
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
DIR="$ROOT/proofs/fixtures/heap-check/reinject"
TARGET="${REINJECT_TARGET_DIR:-$ROOT/target/reinject}"

if [ "$#" -gt 0 ]; then names=("$@"); else
  names=(); for p in "$DIR"/*.patch; do names+=("$(basename "$p" .patch)"); done
fi
[ "${#names[@]}" -gt 0 ] || { echo "reinject: no patches in $DIR" >&2; exit 1; }

wt=$(mktemp -d)
trap 'git -C "$ROOT" worktree remove --force "$wt" >/dev/null 2>&1; rm -rf "$wt"' EXIT
git -C "$ROOT" worktree add --detach -q "$wt" HEAD || exit 2

fail=0
for name in "${names[@]}"; do
  git -C "$wt" checkout -q -- .
  if ! git -C "$wt" apply "$DIR/$name.patch"; then
    echo "FAIL $name: the patch no longer applies — regenerate it from the fix's current code"; fail=1; continue
  fi
  if ! (cd "$wt" && CARGO_TARGET_DIR="$TARGET" cargo build -q --bin almide); then
    echo "FAIL $name: the re-injected compiler does not build"; fail=1; continue
  fi
  if ALMIDE="$TARGET/debug/almide" bash "$wt/scripts/wasm-checked-alloc.sh" "$DIR/$name.almd" >"$wt/out.log" 2>&1; then
    echo "FAIL $name: the checked-allocator gate PASSED with the bug re-injected"
    sed 's/^/    /' "$wt/out.log"; fail=1
  else
    echo "ok   $name: caught — $(grep -m1 '^DIFF' "$wt/out.log")"
  fi
done
git -C "$wt" checkout -q -- .
[ "$fail" -eq 0 ] && echo "reinject: every re-injected bug is caught by the checked-allocator gate"
exit "$fail"
