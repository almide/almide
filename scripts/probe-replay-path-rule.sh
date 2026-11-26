#!/usr/bin/env bash
# Probe only, not a gate: does the kernel's path rule (proofs/GuardPath.v)
# tell the module a fixed compiler emits from the module the same compiler
# emits with a historical memory bug put back?
#
# For each replay patch (the ones scripts/probe-replay-history.sh runs), the
# bug's own reproduction is emitted twice, by the unpatched compiler and by
# the compiler with the patch applied. Every load, store, `memory.copy` and
# `memory.fill` of both modules is cut with the longest run of its function
# before it that the rule reads (scripts/probe-path-rule/cut_paths.py,
# untrusted: it only proposes), and the kernel is asked, per access and per
# range (the bytes written, the bytes read), for a proof that the range lies
# in the payload of a block the cutter guessed: one a local holds, by its
# length word, or one the path allocated, by the size asked for. A range is
# `proved` when the file with one of its statements compiles.
#
#   scripts/probe-replay-path-rule.sh <out-dir>      (ONLY="<patch name> ..." to select)
#
# Writes <out-dir>/summary.tsv (replay, module, sha, ranges, proved, proved
# but not carried, not proved, no guess, header words, out of time, accesses
# with no path) and <out-dir>/<replay>.ranges.tsv. Needs coqc.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
OUT="${1:?usage: scripts/probe-replay-path-rule.sh <out-dir>}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
T="${CARGO_TARGET_DIR:-$PWD/target}/debug"
TOOLS="$ROOT/scripts/probe-path-rule"
PROOFS="$ROOT/proofs"
export PROOFS
export LIMIT="${PATH_GOAL_SECONDS:-300}"

t0=$(date +%s)
if ! (cd proofs && coqc -Q . AlmideTrust GuardWindow.v && coqc -Q . AlmideTrust GuardFresh.v \
      && coqc -Q . AlmideTrust GuardPath.v && coqc -Q . AlmideTrust GuardPathShipped.v \
      && coqc -Q . AlmideTrust GuardAcquire.v) > "$OUT/coqc.log" 2>&1; then
  echo "the proofs do not compile:"; tail -n 60 "$OUT/coqc.log"; exit 1
fi
echo "== proofs/GuardPath.v, GuardPathShipped.v and GuardAcquire.v compile in $(( $(date +%s) - t0 )) s; $(coqc --version | head -1)"
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
  # the one function a path may call: the anchor file's `alloc` line
  abase="$(awk '$1 == "alloc" { print $2 }' "$OUT/$name.base/a.anchors")"
  abug="$(awk '$1 == "alloc" { print $2 }' "$OUT/$name.bug/a.anchors")"
  echo "   allocator index: unpatched ${abase:-none}, defect ${abug:-none}"
  python3 -I "$TOOLS/cut_paths.py" "$OUT/$name.base/a.anchors.wasm" "${abase:--}" > "$OUT/$name.base.cut" || { echo "   the cutter failed on the unpatched module"; continue; }
  python3 -I "$TOOLS/cut_paths.py" "$OUT/$name.bug/a.anchors.wasm" "${abug:--}" > "$OUT/$name.bug.cut" || { echo "   the cutter failed on the defect module"; continue; }
  python3 -I "$TOOLS/path_goals.py" "$G" "unpatched=$OUT/$name.base.cut" "defect=$OUT/$name.bug.cut" | sed 's/^/   /'
  t1=$(date +%s)
  find "$G" -maxdepth 1 -name 'P_*.v' | sort | xargs -P "${JOBS:-4}" -I{} bash -c 'one "$1"' _ {}
  echo "   the kernel answered in $(( $(date +%s) - t1 )) s"
  python3 -I "$TOOLS/path_verdicts.py" "$G" "$OUT/$name.ranges.tsv" --replay "$name" \
    "unpatched=$(sha256sum < "$OUT/$name.base/a.anchors.wasm" | cut -c1-12)" \
    "defect=$(sha256sum < "$OUT/$name.bug/a.anchors.wasm" | cut -c1-12)" \
    "$OUT/summary.tsv" | sed 's/^/   /'
  # The acquire rule (proofs/GuardAcquire.v): per frame of the anchor file's
  # `rc` line, every named acquire (`a<local>`) has its own call of the rc_inc
  # routine (`rcfns`, first index) that is handed the value that local holds.
  # The cutter proposes the calls; the kernel evaluates `acquire_ok`. Where
  # an acquire has no call and the body holds fewer rc_inc calls than the
  # frame has named acquires, the kernel is asked to count them itself
  # (`acquire_short`).
  A="$OUT/$name.acquire"
  mkdir -p "$A"
  python3 -I "$TOOLS/cut_acquires.py" "$OUT/$name.base/a.anchors.wasm" "$OUT/$name.base/a.anchors" > "$A/base.cut" || { echo "   the acquire cutter failed on the unpatched module"; continue; }
  python3 -I "$TOOLS/cut_acquires.py" "$OUT/$name.bug/a.anchors.wasm" "$OUT/$name.bug/a.anchors" > "$A/bug.cut" || { echo "   the acquire cutter failed on the defect module"; continue; }
  python3 -I "$TOOLS/acquire_goals.py" "$A" "unpatched=$A/base.cut" "defect=$A/bug.cut" | sed 's/^/   /'
  find "$A" -maxdepth 1 -name '[AS]_*.v' | sort | xargs -P "${JOBS:-4}" -I{} bash -c 'one "$1"' _ {}
  python3 -I "$TOOLS/acquire_verdicts.py" "$A" "$OUT/$name.frames.tsv" --replay "$name" \
    "unpatched=$(sha256sum < "$OUT/$name.base/a.anchors.wasm" | cut -c1-12)" \
    "defect=$(sha256sum < "$OUT/$name.bug/a.anchors.wasm" | cut -c1-12)" \
    "$OUT/acquire-summary.tsv" | sed 's/^/   /'
done
echo "== acquire rule summary (replay, module, sha, frames with a named acquire, acquires, accepted, too few calls by the kernel's count, refused, with an acquire not placed, out of time)"
cat "$OUT/acquire-summary.tsv" 2>/dev/null
echo "== summary (replay, module, sha, ranges, proved, proved but not carried, not proved, no guess, header words, out of time, accesses with no path)"
cat "$OUT/summary.tsv"
