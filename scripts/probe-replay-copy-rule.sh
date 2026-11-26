#!/usr/bin/env bash
# Probe only, not a gate: does the kernel's copy rule (proofs/GuardCopy.v)
# tell the module a fixed compiler emits from the module the same compiler
# emits with a historical memory bug put back?
#
# For each replay patch (the ones scripts/probe-replay-history.sh runs), the
# bug's own reproduction is emitted twice, by the unpatched compiler and by
# the compiler with the patch applied. Every `memory.copy` of both modules is
# cut with the longest straight-line run before it
# (scripts/probe-copy-rule/cut_copies.py, untrusted: it only proposes), and
# the kernel is asked, per copy and per side (the bytes written, the bytes
# read), for a proof that the side lies in the payload of the block a named
# local holds. A side is `proved` when the file with its statement compiles.
#
#   scripts/probe-replay-copy-rule.sh <out-dir>      (ONLY="<patch name> ..." to select)
#
# Writes <out-dir>/summary.tsv (replay, module, sha, copies, sides, proved,
# not proved, out of time) and <out-dir>/<replay>.sides.tsv. Needs coqc.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
OUT="${1:?usage: scripts/probe-replay-copy-rule.sh <out-dir>}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
T="${CARGO_TARGET_DIR:-$PWD/target}/debug"
TOOLS="$ROOT/scripts/probe-copy-rule"
PROOFS="$ROOT/proofs"
export PROOFS
export LIMIT="${COPY_GOAL_SECONDS:-600}"

t0=$(date +%s)
if ! (cd proofs && coqc -Q . AlmideTrust GuardWindow.v && coqc -Q . AlmideTrust GuardCopy.v) > "$OUT/coqc.log" 2>&1; then
  echo "the proofs do not compile:"; tail -n 60 "$OUT/coqc.log"; exit 1
fi
echo "== proofs/GuardCopy.v compiles in $(( $(date +%s) - t0 )) s; $(coqc --version | head -1)"
grep -n -B1 -A2 'Closed under the global context\|Axioms:' "$OUT/coqc.log" | tail -n 12

build() { cargo build -q -p almide-wasm --example emit_structural_bundle; }
emit() {
  mkdir -p "$2"
  "$T/examples/emit_structural_bundle" "$1" --anchors "$2/a.anchors" > "$2/a.bundle" 2> "$2/producer.err"
}
one() {
  local v="$1" c t1 rc
  c="${v%.v}"
  t1=$(date +%s)
  timeout "$LIMIT" coqc -Q "$PROOFS" AlmideTrust "$v" > "$c.log" 2>&1
  rc=$?
  echo "$rc $(( $(date +%s) - t1 ))" > "$c.rc"
}
export -f one

: > "$OUT/summary.tsv"
build || { echo "the unpatched tree does not build"; exit 1; }
for p in proofs/fixtures/heap-check/reinject/*.patch proofs/fixtures/history-replay/*.patch; do
  name="$(basename "$p" .patch)"
  case " ${ONLY:-$name} " in *" $name "*) ;; *) continue ;; esac
  issue="${name%%-*}"
  repro="$(ls proofs/fixtures/heap-check/reinject/"$issue"-*.almd 2>/dev/null | head -1)"
  [ -f "$repro" ] || { echo "== $name: no reproduction"; continue; }
  if ! git apply --check "$p" 2>/dev/null; then
    echo "== $name ($p): the patch no longer applies"
    continue
  fi
  echo "== $name ($p) on $repro"
  if ! emit "$repro" "$OUT/$name.base"; then
    echo "   unpatched: the producer refused: $(head -2 "$OUT/$name.base/producer.err" | cut -c1-300 | tr '\n' ' ')"
    continue
  fi
  git apply "$p"
  if build 2> "$OUT/$name.build.err"; then
    if ! emit "$repro" "$OUT/$name.bug"; then
      echo "   defect put back: the producer refused: $(head -2 "$OUT/$name.bug/producer.err" | cut -c1-300 | tr '\n' ' ')"
      git apply -R "$p"
      continue
    fi
  else
    echo "   defect put back: does not build ; $(grep -m3 -E '^error' "$OUT/$name.build.err" | cut -c1-300 | tr '\n' ' ')"
    git apply -R "$p"
    continue
  fi
  git apply -R "$p"
  if cmp -s "$OUT/$name.base/a.anchors.wasm" "$OUT/$name.bug/a.anchors.wasm"; then echo "   module: same bytes as unpatched"; else echo "   module: bytes DIFFER from unpatched"; fi
  G="$OUT/$name.goals"
  mkdir -p "$G"
  python3 -I "$TOOLS/cut_copies.py" "$OUT/$name.base/a.anchors.wasm" > "$OUT/$name.base.cut" || { echo "   the cutter failed on the unpatched module"; continue; }
  python3 -I "$TOOLS/cut_copies.py" "$OUT/$name.bug/a.anchors.wasm" > "$OUT/$name.bug.cut" || { echo "   the cutter failed on the defect module"; continue; }
  python3 -I "$TOOLS/copy_goals.py" "$G" "unpatched=$OUT/$name.base.cut" "defect=$OUT/$name.bug.cut" | sed 's/^/   /'
  t1=$(date +%s)
  find "$G" -maxdepth 1 -name 'C_*.v' | sort | xargs -P "${JOBS:-4}" -I{} bash -c 'one "$1"' _ {}
  echo "   the kernel answered in $(( $(date +%s) - t1 )) s"
  python3 -I "$TOOLS/copy_verdicts.py" "$G" "$name" \
    "unpatched=$(sha256sum < "$OUT/$name.base/a.anchors.wasm" | cut -c1-12)" \
    "defect=$(sha256sum < "$OUT/$name.bug/a.anchors.wasm" | cut -c1-12)" \
    "$OUT/$name.sides.tsv" "$OUT/summary.tsv" | sed 's/^/   /'
done
echo "== summary (replay, module, sha, copies, sides, proved, not proved, out of time)"
cat "$OUT/summary.tsv"
