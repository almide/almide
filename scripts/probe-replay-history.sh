#!/usr/bin/env bash
# Probe only: put each fixed wasm memory bug back into the emitter and ask
# every static checker for its verdict on the module the buggy compiler
# emits for the bug's own reproduction. Not a gate.
#
#   scripts/probe-replay-history.sh <out-dir>
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
OUT="${1:?usage: scripts/probe-replay-history.sh <out-dir>}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
T="${CARGO_TARGET_DIR:-$PWD/target}/debug"
build() {
  cargo build -q -p almide-wasm --example emit_structural_bundle &&
    cargo build -q -p almide-verify -p almide-anchor-check &&
    cargo build -q --bin almide
}
# verdicts <almd> <dir>: the producer, almide-verify, the anchor check, and
# the program's own behaviour on the wasm target.
verdicts() {
  local f="$1" W="$2"
  mkdir -p "$W"
  if "$T/examples/emit_structural_bundle" "$f" --anchors "$W/a.anchors" > "$W/a.bundle" 2> "$W/producer.err"; then
    echo "   producer: emitted"
    "$T/almide-verify" bundle "$W/a.bundle" > "$W/verify.out" 2>&1
    echo "   almide-verify: exit $? ; $(grep -cE '^REJECT' "$W/verify.out") REJECT line(s) ; $(grep -m3 -E '^REJECT' "$W/verify.out" | cut -c1-220 | tr '\n' '|')"
    tail -2 "$W/verify.out" | cut -c1-220 | sed 's/^/      /'
    "$T/almide-anchor-check" "$W/a.anchors.wasm" "$W/a.bundle" "$W/a.anchors" > "$W/anchor.out" 2>&1
    echo "   anchor check: exit $? ; $(head -c 400 "$W/anchor.out" | tr '\n' ' ')"
  else
    echo "   producer: REFUSED ; $(head -2 "$W/producer.err" | cut -c1-300 | tr '\n' ' ')"
  fi
  timeout 60 "$T/almide" run "$f" --target wasm > "$W/run.out" 2> "$W/run.err" < /dev/null
  echo "   run on wasm: exit $? ; stdout sha $(sha256sum < "$W/run.out" | cut -c1-12) ; $(head -c 300 "$W/run.out" | tr '\n' '|') ; stderr: $(head -1 "$W/run.err" | cut -c1-160)"
}
build || { echo "the unpatched tree does not build"; exit 1; }
for p in proofs/fixtures/heap-check/reinject/*.patch proofs/fixtures/history-replay/*.patch; do
  name="$(basename "$p" .patch)"
  repro="proofs/fixtures/heap-check/reinject/$name.almd"
  [ -f "$repro" ] || { echo "== $name: no reproduction"; continue; }
  if ! git apply --check "$p" 2>/dev/null; then
    echo "== $name ($p): the patch no longer applies"
    continue
  fi
  echo "== $name ($p)"
  echo "  unpatched:"
  verdicts "$repro" "$OUT/$name.base"
  git apply "$p"
  if build 2> "$OUT/$name.build.err"; then
    echo "  bug replayed:"
    verdicts "$repro" "$OUT/$name.bug"
    if cmp -s "$OUT/$name.base/run.out" "$OUT/$name.bug/run.out"; then echo "   behaviour: same stdout as unpatched"; else echo "   behaviour: stdout DIFFERS from unpatched"; fi
  else
    echo "  bug replayed: does not build ; $(head -3 "$OUT/$name.build.err" | cut -c1-300 | tr '\n' ' ')"
  fi
  git apply -R "$p"
  build
done
