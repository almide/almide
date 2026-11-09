#!/usr/bin/env bash
# A structural witness must catch an emitter that loses what it certifies:
#   bounds (#4005) — a bounds check;   init (#4013) — a slot store;
#   fields (#4279) — a record / tuple / variant field store;
#   native (#4006) — a release in the verified native render, with the
#   renderer's own Rust-mirror check bypassed (the witness is main's
#   ownership witness in the native bundle, judged by check_xc);
#   native-calls (#4006 class P) — a call-site move of the verified native
#   render (a move param claimed while its callers keep borrowing, a caller
#   that never hands the argument over), the Rust mirror bypassed (the
#   witness is the bundle's program `native-calls` witness, judged by
#   check_native_calls_cert);
#   native-bounds (#4006, security row 4) — an element access of the
#   verified native render (a check recorded on another slot, ElemAddr's
#   check dropped, a loop exit test taken against another list's length);
#   the witness is main's `bounds` witness in the native bundle, judged by
#   check_bounds_cert (the render has no Rust mirror of the bounds check to
#   bypass);
#   rc (#4277) — an RC instruction of a certified frame changed (a release
#   dropped, doubled, made on the wrong local, inside an arm window, on the
#   wrong slot holder, or a consuming routine that no longer consumes) while
#   its recorded event stays: almide-verify still accepts the bundle, and
#   almide-anchor-check's RC check must reject;
#   routines (#4005 A4) — a check a runtime routine or a self-host body
#   leans on, dropped from the emitter (or the stdlib body, in the throwaway
#   worktree only). Its accesses are certified by the LINEAR certificates
#   (almide-lin-prove, judged by the proven checker), not by a recorded
#   event, so a lost check shows twice: the control's certificate held to
#   the re-injected bytes is LIN REJECT (the bytes changed under it), and
#   the prover over the re-injected bytes leaves the access uncovered — the
#   <name>.routine label's covered count falls, which the heap-access ledger
#   (proofs/lib/heap_ledger.py) fails on every wall run;
#   anchors (#4011 (d)) — an instruction the bounds, init or fields witness names,
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
# An rc drill with a <name>.sig file is a SIGNATURE drill (#4278 B1): the
# patch makes a runtime helper declare an RC signature its body does not
# keep, and the rejection is almide-verify's verdict on that helper's
# `rc-sig` record (pinned as its REJECT line).
# An rc drill with a <name>.count file is a COUNT drill instead: re-injected,
# the frame must still be ACCEPTED with exactly the named class moved by the
# named delta (e.g. `before an abort +1`) and every other count unchanged —
# a lost event that degrades to a counted class, never to an accepted frame
# credit. (#4278 B1 retired every window class, so no rc drill is a count
# drill today.)
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
#   scripts/witness-reinject.sh <bounds|init|fields|native|native-calls|native-bounds|anchors|rc|routines> [name ...]   (default: every patch)
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
PROP="${1:-}"
case "$PROP" in
  bounds) ISSUE=4005 ;;
  init) ISSUE=4013 ;;
  fields) ISSUE=4279 ;;
  native) ISSUE=4006 ;;
  native-calls) ISSUE=4006 ;;
  native-bounds) ISSUE=4006 ;;
  anchors) ISSUE=4011 ;;
  rc) ISSUE=4277 ;;
  routines) ISSUE=4005 ;;
  *) echo "usage: scripts/witness-reinject.sh <bounds|init|fields|native|native-calls|native-bounds|anchors|rc|routines> [name ...]" >&2; exit 2 ;;
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

if [ "$PROP" = native ] || [ "$PROP" = native-calls ] || [ "$PROP" = native-bounds ]; then
  build() { (cd "$wt" && CARGO_TARGET_DIR="$TARGET" cargo build -q -p almide-mir --example emit_native_bundle && CARGO_TARGET_DIR="$TARGET" cargo build -q -p almide-verify); }
elif [ "$PROP" = routines ]; then
  build() { (cd "$wt" && CARGO_TARGET_DIR="$TARGET" cargo build -q -p almide-wasm --example emit_structural_bundle && CARGO_TARGET_DIR="$TARGET" cargo build -q -p almide-verify -p almide-anchor-check && CARGO_TARGET_DIR="$TARGET" cargo build -q --release -p almide-lin-prove); }
elif [ "$PROP" = anchors ] || [ "$PROP" = rc ]; then
  build() { (cd "$wt" && CARGO_TARGET_DIR="$TARGET" cargo build -q -p almide-wasm --example emit_structural_bundle && CARGO_TARGET_DIR="$TARGET" cargo build -q -p almide-verify -p almide-anchor-check); }
else
  build() { (cd "$wt" && CARGO_TARGET_DIR="$TARGET" cargo build -q -p almide-wasm --example emit_structural_bundle && CARGO_TARGET_DIR="$TARGET" cargo build -q -p almide-verify); }
fi
# main's ownership witness (or, with a second argument, the program's
# native-calls witness) out of a native bundle. A third argument names the
# fn whose ownership witness to take instead of main's: a native drill with a
# <name>.fn file judges that fn (the region window's, #4006 C2).
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
        want = {"native-calls": ("native-calls", "program"), "bounds": ("bounds", "main")}.get(sys.argv[2] if len(sys.argv) > 2 else "", ("ownership", sys.argv[3] if len(sys.argv) > 3 else "main"))
        if (prop, fn) == want:
            sys.stdout.write(b[i:i + int(n)].decode())
        i += int(n) + 1
' "$@"
}
# verdict <name>: almide-verify's exit on the reproduction's $PROP witness.
verdict() {
  if [ "$PROP" = native-bounds ]; then
    if ! "$TARGET/debug/examples/emit_native_bundle" "$DIR/$1.almd" "$wt/native.rs" > "$wt/native.bundle" 2> "$wt/producer.err"; then
      echo "producer-failed"; return
    fi
    main_witness "$wt/native.bundle" bounds > "$wt/witness.w"
    "$TARGET/debug/almide-verify" bounds "$wt/witness.w" > /dev/null 2>&1
    echo "$?"
    return
  fi
  if [ "$PROP" = native-calls ]; then
    if ! "$TARGET/debug/examples/emit_native_bundle" "$DIR/$1.almd" "$wt/native.rs" > "$wt/native.bundle" 2> "$wt/producer.err"; then
      echo "producer-failed"; return
    fi
    main_witness "$wt/native.bundle" native-calls > "$wt/witness.w"
    "$TARGET/debug/almide-verify" native-calls "$wt/witness.w" > /dev/null 2>&1
    echo "$?"
    return
  fi
  if [ "$PROP" = native ]; then
    if ! "$TARGET/debug/examples/emit_native_bundle" "$DIR/$1.almd" "$wt/native.rs" > "$wt/native.bundle" 2> "$wt/producer.err"; then
      echo "producer-failed"; return
    fi
    main_witness "$wt/native.bundle" ownership "$(cat "$DIR/$1.fn" 2>/dev/null || echo main)" > "$wt/witness.w"
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
    "$TARGET/debug/almide-verify" bundle "$wt/a.bundle" > "$wt/verify.out" 2>&1
    case $? in
      0|3) ;;
      *)
        # A SIGNATURE drill (<name>.sig, #4278 B1): a runtime helper's
        # declared RC signature lies about its body. The lie is the helper's
        # own certificate's to catch: almide-verify REJECTS its `rc-sig`
        # record (the proven checker and the Rocq kernel judge the same
        # record on every structural-wall run). Its REJECT line is pinned.
        if [ -f "$DIR/$1.sig" ] && grep -E '^REJECT +rc-sig ' "$wt/verify.out" > "$wt/witness.w"; then
          echo 1; return
        fi
        echo "bundle-rejected"; return ;;
    esac
    "$TARGET/debug/almide-anchor-check" "$wt/a.anchors.wasm" "$wt/a.bundle" "$wt/a.anchors" > "$wt/witness.w" 2>&1
    echo "$?"
    return
  fi
  if [ "$PROP" = anchors ]; then
    if ! "$TARGET/debug/examples/emit_structural_bundle" "$DIR/$1.almd" --anchors "$wt/a.anchors" > "$wt/a.bundle" 2> "$wt/producer.err"; then
      echo "producer-failed"; return
    fi
    # The proven checkers must still ACCEPT the witnesses: the bug is one
    # only the bytes show.
    for p in bounds init fields; do
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
if [ "$PROP" = routines ]; then
  # certify <name> <dir>: the reproduction built, both linear certificates
  # proven, verified against the bytes and judged by almide-verify, and the
  # census per function; prints the covered count of <name>.routine's label.
  certify() {
    local d="$2"
    mkdir -p "$d"
    "$TARGET/debug/examples/emit_structural_bundle" "$DIR/$1.almd" --anchors "$d/a.anchors" > "$d/a.bundle" 2> "$d/producer.err" || { echo producer-failed; return; }
    "$TARGET/release/almide-lin-prove" "$d/a.anchors.wasm" "$d/a.anchors" > "$d/l.cert" 2> "$d/l.prove" || { echo lin-prove-failed; return; }
    "$TARGET/release/almide-lin-prove" --routines "$d/a.anchors.wasm" "$d/a.anchors" "$d/r.sel" > "$d/r.cert" 2> "$d/r.prove" || { echo routine-prove-failed; return; }
    "$TARGET/debug/almide-anchor-check" --lin-verify "$d/a.anchors.wasm" "$d/a.anchors" "$d/l.cert" > /dev/null 2>&1 || { echo lin-verify-failed; return; }
    "$TARGET/debug/almide-anchor-check" --lin-verify-routines "$d/a.anchors.wasm" "$d/a.anchors" "$d/r.cert" "$d/r.sel" > /dev/null 2>&1 || { echo routine-verify-failed; return; }
    for c in l r; do "$TARGET/debug/almide-verify" bounds "$d/$c.cert" > /dev/null 2>&1 || { echo "$c-cert-rejected"; return; }; done
    "$TARGET/debug/almide-anchor-check" --census --census-fns --lin "$d/l.cert" --rlin "$d/r.cert" "$d/r.sel" "$d/a.anchors.wasm" "$d/a.bundle" "$d/a.anchors" > "$d/census" 2>&1 || { echo anchors-rejected; return; }
    python3 -c '
import re, sys
want = open(sys.argv[2]).read().strip()
n = 0
for l in open(sys.argv[1]):
    p = l.split()
    if p and p[0] == "CENSUSFN" and re.sub(r"^__selfhost_\d+\.", "", p[4]) == want:
        n += int(p[6])
print(n)
' "$d/census" "$DIR/$1.routine"
  }
  for name in "${names[@]}"; do
    git -C "$wt" checkout -q -- .
    build >/dev/null 2>&1
    ctl=$(certify "$name" "$wt/ctl")
    case "$ctl" in ''|*[!0-9]*) echo "FAIL $name: CONTROL — unpatched, the certificates do not verify ($ctl)"; fail=1; continue ;; esac
    if [ "$ctl" -eq 0 ]; then echo "FAIL $name: CONTROL — unpatched, no access of $(cat "$DIR/$name.routine") is certified"; fail=1; continue; fi
    if ! git -C "$wt" apply "$DIR/$name.patch"; then
      echo "FAIL $name: the patch no longer applies — regenerate it from the current tree"; fail=1; continue
    fi
    if ! build; then echo "FAIL $name: the re-injected tree does not build"; fail=1; continue; fi
    got=$(certify "$name" "$wt/bug")
    case "$got" in ''|*[!0-9]*) echo "FAIL $name: re-injected, the certificates do not verify ($got) — the prover must leave the access counted, not emit a refused certificate"; fail=1; continue ;; esac
    # The control's certificate, held to the re-injected bytes.
    if [ -s "$wt/ctl/r.sel" ] && grep -q '[0-9] [0-9]' "$wt/ctl/r.sel"; then
      "$TARGET/debug/almide-anchor-check" --lin-verify-routines "$wt/bug/a.anchors.wasm" "$wt/bug/a.anchors" "$wt/ctl/r.cert" "$wt/ctl/r.sel" > "$wt/held" 2>&1; held=$?
    fi
    if [ "${held:-0}" -ne 1 ]; then
      "$TARGET/debug/almide-anchor-check" --lin-verify "$wt/bug/a.anchors.wasm" "$wt/bug/a.anchors" "$wt/ctl/l.cert" > "$wt/held" 2>&1; held=$?
    fi
    pinned="$DIR/$name.reject"
    head -1 "$wt/held" | cut -c1-160 > "$wt/held1"
    if [ "${REINJECT_UPDATE:-0}" = 1 ]; then cp "$wt/held1" "$pinned"; fi
    if [ "$got" -lt "$ctl" ] && [ "$held" -eq 1 ] && cmp -s "$wt/held1" "$pinned"; then
      echo "ok   $name: control covers $ctl access(es) of $(cat "$DIR/$name.routine"); re-injected, $got (the ledger rises by $((ctl - got))); the control's certificate on the new bytes: $(cat "$wt/held1")"
    else
      echo "FAIL $name: covered $ctl -> $got (want fewer), the control's certificate on the new bytes exits $held (want 1 and the pinned ${pinned#"$ROOT"/}): $(cat "$wt/held1")"
      fail=1
    fi
    unset held
  done
  [ "$fail" -eq 0 ] && echo "routines-reinject: every lost check leaves its access uncovered and refuses the old certificate"
  exit "$fail"
fi
for name in "${names[@]}"; do
  v=$(verdict "$name")
  cp "$wt/witness.w" "$wt/control.w" 2>/dev/null
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
  if [ "$PROP" = rc ] && [ -f "$DIR/$name.count" ]; then
    # The count drill: accepted, the one class moved by its delta, the rest equal.
    read -r cls delta < "$DIR/$name.count"
    got=$(python3 -c '
import re, sys
keys = ["rebind", "judge", "signed"]
def counts(p):
    t = open(p).read()
    m = re.search(r"(\d+) rebind\(s\) with no instruction, (\d+) judge\(s\) with no rebind, (\d+) signed", t)
    return dict(zip(keys, map(int, m.groups()))) if m else None
a, b = counts(sys.argv[1]), counts(sys.argv[2])
if a is None or b is None:
    print("no-rc-ok"); sys.exit()
d = {k: b[k] - a[k] for k in keys}
want = dict.fromkeys(keys, 0); want[sys.argv[3]] = int(sys.argv[4])
print("ok" if d == want else "moved " + " ".join(f"{k}{v:+d}" for k, v in d.items() if v))
' "$wt/control.w" "$wt/witness.w" "$cls" "$delta")
    if [ "$v" = 0 ] && [ "$got" = ok ]; then
      echo "ok   $name: control accepted; re-injected, still accepted with $cls $delta and every other count unchanged"
    else
      echo "FAIL $name: a count drill wants acceptance with $cls $delta only (verdict $v, $got): $(head -c 200 "$wt/witness.w")"
      fail=1
    fi
    git -C "$wt" checkout -q -- .
    build >/dev/null 2>&1
    continue
  fi
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
