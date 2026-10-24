#!/usr/bin/env bash
# #4005: the bounds witness must catch a structural emitter that loses a
# bounds check. Each proofs/fixtures/bounds-witness/reinject/<name>.patch
# puts one such bug into the emitter; <name>.almd is its reproduction. In a
# throwaway worktree, for each:
#   1. CONTROL — unpatched, the reproduction's `bounds` witness is ACCEPTED
#      by almide-verify (exit 0), so a rejection below is the patch's doing;
#   2. apply the patch, rebuild the bundle producer, and require the
#      witness to be REJECTED (exit 1).
#   3. the rejected witness is byte-identical to
#      proofs/poisoned-certs/bounds/4005-<name>.cert, the copy
#      proofs/structural-wall.sh has all three verdicts reject on every
#      trust-spine run (BOUNDS_REINJECT_UPDATE=1 rewrites the copy).
# A patch that no longer applies fails this script too: regenerate it
# against the current emitter rather than letting the drill go stale.
#
# The witness records a check where the compare is emitted and an access
# where the load or store is emitted (crates/almide-wasm/src/witness_bounds.rs),
# so an emitter that drops a check leaves a bare `a` / `w`. What this cannot
# see: a patch that drops the compare's instructions but keeps the recording
# call beside them — the recording discipline stays trusted.
#
#   scripts/bounds-witness-reinject.sh [name ...]   (default: every patch)
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
DIR="$ROOT/proofs/fixtures/bounds-witness/reinject"
TARGET="${REINJECT_TARGET_DIR:-$ROOT/target/bounds-reinject}"

if [ "$#" -gt 0 ]; then names=("$@"); else
  names=(); for p in "$DIR"/*.patch; do names+=("$(basename "$p" .patch)"); done
fi
[ "${#names[@]}" -gt 0 ] || { echo "bounds-reinject: no patches in $DIR" >&2; exit 1; }

wt=$(mktemp -d)
trap 'git -C "$ROOT" worktree remove --force "$wt" >/dev/null 2>&1; rm -rf "$wt"' EXIT
git -C "$ROOT" worktree add --detach -q "$wt" HEAD || exit 2

build() { (cd "$wt" && CARGO_TARGET_DIR="$TARGET" cargo build -q -p almide-wasm --example emit_structural_bundle && CARGO_TARGET_DIR="$TARGET" cargo build -q -p almide-verify); }
# verdict <name>: almide-verify's exit on the reproduction's bounds witness.
verdict() {
  if ! "$TARGET/debug/examples/emit_structural_bundle" "$DIR/$1.almd" --only bounds '(program)' > "$wt/bounds.w" 2> "$wt/producer.err"; then
    echo "producer-failed"; return
  fi
  "$TARGET/debug/almide-verify" bounds "$wt/bounds.w" > /dev/null 2>&1
  echo "$?"
}

build || { echo "FAIL: the unpatched tree does not build"; exit 2; }
fail=0
for name in "${names[@]}"; do
  v=$(verdict "$name")
  if [ "$v" != 0 ]; then
    echo "FAIL $name: CONTROL — unpatched, the bounds witness is not accepted (verdict $v): $(head -c 200 "$wt/bounds.w" 2>/dev/null) $(head -3 "$wt/producer.err")"
    fail=1; continue
  fi
  git -C "$wt" checkout -q -- .
  if ! git -C "$wt" apply "$DIR/$name.patch"; then
    echo "FAIL $name: the patch no longer applies — regenerate it from the current emitter"; fail=1; continue
  fi
  if ! build; then
    echo "FAIL $name: the re-injected emitter does not build"; fail=1; git -C "$wt" checkout -q -- .; continue
  fi
  v=$(verdict "$name")
  pinned="$ROOT/proofs/poisoned-certs/bounds/4005-$name.cert"
  if [ "$v" = 1 ] && [ "${BOUNDS_REINJECT_UPDATE:-0}" = 1 ]; then cp "$wt/bounds.w" "$pinned"; fi
  if [ "$v" = 1 ] && ! cmp -s "$wt/bounds.w" "$pinned"; then
    echo "FAIL $name: rejected, but the witness is not the pinned poisoned certificate ${pinned#"$ROOT"/} (BOUNDS_REINJECT_UPDATE=1 rewrites it): $(head -c 200 "$wt/bounds.w")"
    fail=1
  elif [ "$v" = 1 ]; then
    echo "ok   $name: control accepted (exit 0); re-injected, rejected (exit 1), pinned: $(head -c 120 "$wt/bounds.w")"
  else
    echo "FAIL $name: with the bug re-injected the bounds witness was not rejected (verdict $v): $(head -c 200 "$wt/bounds.w")"
    fail=1
  fi
  git -C "$wt" checkout -q -- .
  build >/dev/null 2>&1
done
[ "$fail" -eq 0 ] && echo "bounds-reinject: every re-injected lost check is rejected by the bounds witness's checker"
exit "$fail"
