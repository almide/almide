#!/usr/bin/env bash
# A structural witness must catch an emitter that loses what it certifies:
#   bounds (#4005) — a bounds check;   init (#4013) — a slot store;
#   native (#4006) — a release in the verified native render, with the
#   renderer's own Rust-mirror check bypassed (the witness is main's
#   ownership witness in the native bundle, judged by check_xc);
#   rc (#4277) — an RC instruction of a certified frame changed (a release
#   dropped, doubled, made on the wrong local, inside an arm window, on the
#   wrong slot holder, or a consuming routine that no longer consumes) while
#   its recorded event stays: almide-verify still accepts the bundle, and
#   almide-anchor-check's RC check (ALMIDE_RC_ANCHORS=1) must reject;
#   anchors (#4011 (d)) — an instruction the bounds or init witness names,
#   changed while its recording call stays: the proven checkers still
#   accept both witnesses (step 2 requires it), and almide-anchor-check,
#   which holds each event to the shipped bytes, must reject.
# Each proofs/fixtures/<property>-witness/reinject/<name>.patch puts one such
# bug into the emitter; <name>.almd is its reproduction. In a throwaway
# worktree, for each:
#   1. CONTROL — unpatched, the reproduction's <property> witness is ACCEPTED
#      by almide-verify (exit 0), so a rejection below is the patch's doing;
#   2. apply the patch, rebuild the bundle producer, and require the
#      witness to be REJECTED (exit 1).
#   3. the rejected witness is byte-identical to
#      proofs/poisoned-certs/<property>/<issue>-<name>.cert, the copy
#      proofs/structural-wall.sh has all three verdicts reject on every
#      trust-spine run (REINJECT_UPDATE=1 rewrites the copy).
# A patch that no longer applies fails this script too: regenerate it
# against the current emitter rather than letting the drill go stale.
#
# Each witness records an event where its instruction is emitted (a check
# beside its compare, witness_bounds.rs; a slot beside its store,
# witness_init.rs), so an emitter that drops the instruction drops the event
# and the checker rejects. What this cannot see: a patch that drops the
# instructions but keeps the recording call beside them. That is the
# anchors drill's subject: the event stays, the instruction changes, and the
# byte-level check (#4011 (d)) is what rejects.
#
#   scripts/witness-reinject.sh <bounds|init|native|anchors|rc> [name ...]   (default: every patch)
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
PROP="${1:-}"
case "$PROP" in
  bounds) ISSUE=4005 ;;
  init) ISSUE=4013 ;;
  native) ISSUE=4006 ;;
  anchors) ISSUE=4011 ;;
  rc) ISSUE=4277; export ALMIDE_RC_ANCHORS=1 ;;
  *) echo "usage: scripts/witness-reinject.sh <bounds|init|native|anchors|rc> [name ...]" >&2; exit 2 ;;
esac
shift
DIR="$ROOT/proofs/fixtures/$PROP-witness/reinject"
TARGET="${REINJECT_TARGET_DIR:-$ROOT/target/witness-reinject}"

if [ "$#" -gt 0 ]; then names=("$@"); else
  names=(); for p in "$DIR"/*.patch; do names+=("$(basename "$p" .patch)"); done
fi
[ "${#names[@]}" -gt 0 ] || { echo "$PROP-reinject: no patches in $DIR" >&2; exit 1; }

wt=$(mktemp -d)
trap 'git -C "$ROOT" worktree remove --force "$wt" >/dev/null 2>&1; rm -rf "$wt"' EXIT
git -C "$ROOT" worktree add --detach -q "$wt" HEAD || exit 2

if [ "$PROP" = native ]; then
  build() { (cd "$wt" && CARGO_TARGET_DIR="$TARGET" cargo build -q -p almide-mir --example emit_native_bundle && CARGO_TARGET_DIR="$TARGET" cargo build -q -p almide-verify); }
elif [ "$PROP" = anchors ] || [ "$PROP" = rc ]; then
  build() { (cd "$wt" && CARGO_TARGET_DIR="$TARGET" cargo build -q -p almide-wasm --example emit_structural_bundle && CARGO_TARGET_DIR="$TARGET" cargo build -q -p almide-verify -p almide-anchor-check); }
else
  build() { (cd "$wt" && CARGO_TARGET_DIR="$TARGET" cargo build -q -p almide-wasm --example emit_structural_bundle && CARGO_TARGET_DIR="$TARGET" cargo build -q -p almide-verify); }
fi
# main's ownership witness out of a native bundle.
main_witness() {
  python3 -c '
import sys
b = open(sys.argv[1], "rb").read()
i = b.index(b"\n") + 1
while i < len(b):
    j = b.index(b"\n", i)
    line = b[i:j].decode()
    i = j + 1
    if line.startswith("witness "):
        _, prop, n, fn = line.split(" ", 3)
        if prop == "ownership" and fn == "main":
            sys.stdout.write(b[i:i + int(n)].decode())
        i += int(n) + 1
' "$1"
}
# verdict <name>: almide-verify's exit on the reproduction's $PROP witness.
verdict() {
  if [ "$PROP" = native ]; then
    if ! "$TARGET/debug/examples/emit_native_bundle" "$DIR/$1.almd" "$wt/native.rs" > "$wt/native.bundle" 2> "$wt/producer.err"; then
      echo "producer-failed"; return
    fi
    main_witness "$wt/native.bundle" > "$wt/witness.w"
    "$TARGET/debug/almide-verify" ownership "$wt/witness.w" > /dev/null 2>&1
    echo "$?"
    return
  fi
  if [ "$PROP" = rc ]; then
    if ! "$TARGET/debug/examples/emit_structural_bundle" "$DIR/$1.almd" --anchors "$wt/a.anchors" > "$wt/a.bundle" 2> "$wt/producer.err"; then
      echo "producer-failed"; return
    fi
    # The ownership witness must still be ACCEPTED (certified or incomplete):
    # the bug is one only the bytes show.
    "$TARGET/debug/almide-verify" bundle "$wt/a.bundle" > /dev/null 2>&1
    case $? in 0|3) ;; *) echo "bundle-rejected"; return ;; esac
    "$TARGET/debug/almide-anchor-check" "$wt/a.anchors.wasm" "$wt/a.bundle" "$wt/a.anchors" > "$wt/witness.w" 2>&1
    echo "$?"
    return
  fi
  if [ "$PROP" = anchors ]; then
    if ! "$TARGET/debug/examples/emit_structural_bundle" "$DIR/$1.almd" --anchors "$wt/a.anchors" > "$wt/a.bundle" 2> "$wt/producer.err"; then
      echo "producer-failed"; return
    fi
    # The proven checkers must still ACCEPT both witnesses: the bug is one
    # only the bytes show.
    for p in bounds init; do
      "$TARGET/debug/examples/emit_structural_bundle" "$DIR/$1.almd" --only "$p" '(program)' > "$wt/$p.w" 2>/dev/null || { echo "producer-failed"; return; }
      "$TARGET/debug/almide-verify" "$p" "$wt/$p.w" > /dev/null 2>&1 || { echo "$p-witness-rejected"; return; }
    done
    "$TARGET/debug/almide-anchor-check" "$wt/a.anchors.wasm" "$wt/a.bundle" "$wt/a.anchors" > "$wt/witness.w" 2>&1
    echo "$?"
    return
  fi
  if ! "$TARGET/debug/examples/emit_structural_bundle" "$DIR/$1.almd" --only "$PROP" '(program)' > "$wt/witness.w" 2> "$wt/producer.err"; then
    echo "producer-failed"; return
  fi
  "$TARGET/debug/almide-verify" "$PROP" "$wt/witness.w" > /dev/null 2>&1
  echo "$?"
}

build || { echo "FAIL: the unpatched tree does not build"; exit 2; }
fail=0
for name in "${names[@]}"; do
  v=$(verdict "$name")
  if [ "$v" != 0 ]; then
    echo "FAIL $name: CONTROL — unpatched, the $PROP witness is not accepted (verdict $v): $(head -c 200 "$wt/witness.w" 2>/dev/null) $(head -3 "$wt/producer.err")"
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
  pinned="$ROOT/proofs/poisoned-certs/$PROP/$ISSUE-$name.cert"
  # The anchors drill pins the checker's REJECT line (no certificate format
  # of its own: the witnesses it judges are the accepted ones).
  [ "$PROP" = anchors ] && pinned="$ROOT/proofs/fixtures/anchors-witness/reinject/$name.reject"
  [ "$PROP" = rc ] && pinned="$ROOT/proofs/fixtures/rc-witness/reinject/$name.reject"
  if [ "$v" = 1 ] && [ "${REINJECT_UPDATE:-0}" = 1 ]; then cp "$wt/witness.w" "$pinned"; fi
  if [ "$v" = 1 ] && ! cmp -s "$wt/witness.w" "$pinned"; then
    echo "FAIL $name: rejected, but the witness is not the pinned poisoned certificate ${pinned#"$ROOT"/} (REINJECT_UPDATE=1 rewrites it): $(head -c 200 "$wt/witness.w")"
    fail=1
  elif [ "$v" = 1 ]; then
    echo "ok   $name: control accepted (exit 0); re-injected, rejected (exit 1), pinned: $(head -c 120 "$wt/witness.w")"
  else
    echo "FAIL $name: with the bug re-injected the $PROP witness was not rejected (verdict $v): $(head -c 200 "$wt/witness.w")"
    fail=1
  fi
  git -C "$wt" checkout -q -- .
  build >/dev/null 2>&1
done
[ "$fail" -eq 0 ] && echo "$PROP-reinject: every re-injected bug is rejected by the $PROP witness's checker"
exit "$fail"
