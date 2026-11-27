#!/usr/bin/env bash
# Probe only, not a gate: what share of the accesses in the structural wall's
# certified fixtures does the kernel's path rule (proofs/GuardPath.v) put in
# bounds, and for how many recorded frames does the kernel accept the acquire
# rule (proofs/GuardAcquire.v)? The cutters only propose
# (scripts/probe-path-rule/, untrusted); every statement is a Rocq file the
# kernel compiles or does not.
#
#   scripts/probe-corpus-kernel-rules.sh cut   <dir>
#       Build every fixture of crates/almide-wasm/tests/golden/witness-fixtures.txt
#       as the wall does (`almide build --target wasm`, then the bundle
#       producer for its anchor file), cut each module, and write the distinct
#       statements: <dir>/goals (path rule) and <dir>/acquire (acquire rule),
#       <dir>/modules.tsv (label, fixture, status, sha256, bytes), and
#       <dir>/corpus-goals.tgz holding all of it.
#   scripts/probe-corpus-kernel-rules.sh judge <dir> <shard> <of>
#       Compile the proofs, then ask the kernel every statement whose number
#       is <shard> modulo <of>, PATH_GOAL_SECONDS (45) each, for at most
#       JUDGE_BUDGET_SECONDS (18000) in all; a statement not reached is
#       recorded as such (code 97), never as a refusal. Writes
#       <dir>/rc-<shard>.tgz (the `.rc` files: `<exit code> <seconds>`).
#   scripts/probe-corpus-kernel-rules.sh sum   <dir>
#       With every shard's `.rc` files in place: the totals over the corpus.
#       A statement with no `.rc` (its shard was lost) is recorded as code 98.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
CMD="${1:?usage: scripts/probe-corpus-kernel-rules.sh cut|judge|sum <dir> ...}"
OUT="${2:?usage: scripts/probe-corpus-kernel-rules.sh cut|judge|sum <dir> ...}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
TOOLS="$ROOT/scripts/probe-path-rule"
PROOFS="$ROOT/proofs"
export OUT TOOLS PROOFS

case "$CMD" in
cut)
  TD="${CARGO_TARGET_DIR:-$ROOT/target}/debug"
  ALMIDE="$TD/almide"
  PRODUCER="$TD/examples/emit_structural_bundle"
  t0=$(date +%s)
  cargo build -q --bin almide && cargo build -q -p almide-wasm --example emit_structural_bundle \
    || { echo "the tree does not build"; exit 1; }
  echo "== the compiler and the bundle producer built in $(( $(date +%s) - t0 )) s"
  mkdir -p "$OUT/mods" "$OUT/cut" "$OUT/acq" "$OUT/goals" "$OUT/acquire"
  grep -v -E '^(#|$)' crates/almide-wasm/tests/golden/witness-fixtures.txt \
    | awk '{ printf "m%04d\t%s\n", NR, $0 }' > "$OUT/listed.tsv"
  echo "== $(wc -l < "$OUT/listed.tsv") fixture(s) listed"
  # One at a time, as the wall builds them.
  t0=$(date +%s)
  while IFS=$'\t' read -r m f; do
    d="$OUT/mods/$m"
    mkdir -p "$d"
    if ! "$ALMIDE" build "$f" --target wasm -o "$d/a.wasm" > "$d/build.err" 2>&1; then
      echo "build-refused" > "$d/status"; continue
    fi
    if ! "$PRODUCER" "$f" --artifact "$d/a.wasm" --anchors "$d/a.anchors" > "$d/a.bundle" 2> "$d/producer.err"; then
      echo "producer-refused" > "$d/status"; continue
    fi
    echo "built" > "$d/status"
  done < "$OUT/listed.tsv"
  echo "== built and anchored in $(( $(date +%s) - t0 )) s"
  # The cutters are independent per module.
  cutone() {
    local m="$1" d="$OUT/mods/$1" alloc
    [ "$(cat "$d/status" 2>/dev/null)" = built ] || return 0
    alloc="$(awk '$1 == "alloc" { print $2 }' "$d/a.anchors")"
    case "$alloc" in ''|*[!0-9]*) alloc=- ;; esac
    if ! python3 -I "$TOOLS/cut_paths.py" "$d/a.wasm" "$alloc" > "$OUT/cut/$m.cut" 2> "$d/cut.err"; then
      echo "path-cutter-failed" > "$d/status"; return 0
    fi
    if ! python3 -I "$TOOLS/cut_acquires.py" "$d/a.wasm" "$d/a.anchors" > "$OUT/acq/$m.cut" 2> "$d/acq.err"; then
      echo "acquire-cutter-failed" > "$d/status"; return 0
    fi
    echo "ok" > "$d/status"
  }
  export -f cutone
  t0=$(date +%s)
  cut -f1 "$OUT/listed.tsv" | xargs -P "${JOBS:-4}" -I{} bash -c 'cutone "$1"' _ {}
  echo "== cut in $(( $(date +%s) - t0 )) s"
  : > "$OUT/modules.tsv"
  PATHS=(); ACQS=()
  while IFS=$'\t' read -r m f; do
    d="$OUT/mods/$m"
    st="$(cat "$d/status" 2>/dev/null || echo missing)"
    if [ -f "$d/a.wasm" ]; then
      printf '%s\t%s\t%s\t%s\t%s\n' "$m" "$f" "$st" "$(sha256sum < "$d/a.wasm" | cut -c1-64)" "$(wc -c < "$d/a.wasm" | tr -d ' ')" >> "$OUT/modules.tsv"
    else
      printf '%s\t%s\t%s\t-\t0\n' "$m" "$f" "$st" >> "$OUT/modules.tsv"
    fi
    if [ "$st" = ok ]; then
      PATHS+=("$m=$OUT/cut/$m.cut"); ACQS+=("$m=$OUT/acq/$m.cut")
    fi
  done < "$OUT/listed.tsv"
  echo "== modules by status: $(cut -f3 "$OUT/modules.tsv" | sort | uniq -c | tr '\n' ' ')"
  awk -F'\t' '$3 != "ok" { print "   not cut: " $1 " " $2 " (" $3 ")" }' "$OUT/modules.tsv" | head -n 40
  [ "${#PATHS[@]}" -gt 0 ] || { echo "no module was cut"; exit 1; }
  echo "== path rule statements"
  python3 -I "$TOOLS/path_goals.py" "$OUT/goals" "${PATHS[@]}" | sed 's/^/   /' || exit 1
  echo "== acquire rule statements"
  python3 -I "$TOOLS/acquire_goals.py" "$OUT/acquire" "${ACQS[@]}" | sed 's/^/   /' || exit 1
  echo "   $(find "$OUT/goals" -maxdepth 1 -name 'P_*.v' | wc -l | tr -d ' ') path statement file(s), $(du -sm "$OUT/goals" | cut -f1) MB; $(find "$OUT/acquire" -maxdepth 1 -name '[AS]_*.v' | wc -l | tr -d ' ') acquire statement file(s), $(du -sm "$OUT/acquire" | cut -f1) MB"
  tar -C "$OUT" -czf "$OUT/corpus-goals.tgz" goals acquire modules.tsv || exit 1
  echo "   corpus-goals.tgz: $(du -m "$OUT/corpus-goals.tgz" | cut -f1) MB"
  ;;

judge)
  SHARD="${3:?judge <dir> <shard> <of>}"
  OF="${4:?judge <dir> <shard> <of>}"
  export LIMIT="${PATH_GOAL_SECONDS:-45}"
  t0=$(date +%s)
  if ! (cd proofs && coqc -Q . AlmideTrust GuardWindow.v && coqc -Q . AlmideTrust GuardFresh.v \
        && coqc -Q . AlmideTrust GuardPath.v && coqc -Q . AlmideTrust GuardAcquire.v) > "$OUT/coqc.log" 2>&1; then
    echo "the proofs do not compile:"; tail -n 60 "$OUT/coqc.log"; exit 1
  fi
  echo "== proofs/GuardPath.v and GuardAcquire.v compile in $(( $(date +%s) - t0 )) s; $(coqc --version | head -1)"
  grep -n -B1 -A2 'Closed under the global context\|Axioms:' "$OUT/coqc.log" | tail -n 12
  DEADLINE=$(( $(date +%s) + ${JUDGE_BUDGET_SECONDS:-18000} ))
  export DEADLINE
  one() {
    local v="$1" c t1 rc
    c="${v%.v}"
    if [ "$(date +%s)" -ge "$DEADLINE" ]; then echo "97 0" > "$c.rc"; return 0; fi
    t1=$(date +%s)
    timeout "$LIMIT" coqc -Q "$PROOFS" AlmideTrust "$v" > "$c.log" 2>&1
    rc=$?
    echo "$rc $(( $(date +%s) - t1 ))" > "$c.rc"
  }
  export -f one
  # A statement's number is the digits of its file name; this shard takes the
  # ones that are <shard> modulo <of>.
  ( find "$OUT/goals" -maxdepth 1 -name 'P_*.v'; find "$OUT/acquire" -maxdepth 1 -name '[AS]_*.v' ) | sort \
    | awk -v s="$SHARD" -v of="$OF" '{ n = $0; sub(/.*_/, "", n); sub(/\.v$/, "", n); if ((n + 0) % of == s) print }' > "$OUT/mine-$SHARD.txt"
  echo "== shard $SHARD of $OF: $(wc -l < "$OUT/mine-$SHARD.txt" | tr -d ' ') statement(s), $LIMIT s each"
  t0=$(date +%s)
  xargs -P "${JOBS:-4}" -I{} bash -c 'one "$1"' _ {} < "$OUT/mine-$SHARD.txt"
  echo "== the kernel answered in $(( $(date +%s) - t0 )) s"
  sed 's/\.v$/.rc/' "$OUT/mine-$SHARD.txt" | while IFS= read -r r; do [ -f "$r" ] && cat "$r"; done \
    | awk '{ c[$1]++; t += $2 } END { printf "   by exit code:"; for (k in c) printf " %s: %d;", k, c[k]; printf " %d s of kernel time (0 compiled, 124 out of time, 97 not reached, other: did not compile)\n", t }'
  ( cd "$OUT" && sed "s|^$OUT/||; s/\.v$/.rc/" "mine-$SHARD.txt" | tar -czf "rc-$SHARD.tgz" -T - ) || exit 1
  echo "   rc-$SHARD.tgz: $(du -k "$OUT/rc-$SHARD.tgz" | cut -f1) KB"
  ;;

sum)
  missing=0
  for v in "$OUT"/goals/P_*.v "$OUT"/acquire/[AS]_*.v; do
    [ -e "$v" ] || continue
    [ -f "${v%.v}.rc" ] || { echo "98 0" > "${v%.v}.rc"; missing=$((missing + 1)); }
  done
  echo "== statements with no answer file (their shard was lost): $missing"
  python3 -I "$TOOLS/path_verdicts.py" "$OUT/goals" "$OUT/ranges.tsv" > "$OUT/path-by-module.txt" || { echo "path_verdicts.py failed"; tail -n 5 "$OUT/path-by-module.txt"; exit 1; }
  python3 -I "$TOOLS/acquire_verdicts.py" "$OUT/acquire" "$OUT/frames.tsv" > "$OUT/acquire-by-module.txt" || { echo "acquire_verdicts.py failed"; tail -n 5 "$OUT/acquire-by-module.txt"; exit 1; }
  python3 -I "$TOOLS/corpus_sum.py" "$OUT" || exit 1
  ;;

*)
  echo "usage: scripts/probe-corpus-kernel-rules.sh cut|judge|sum <dir> ..."; exit 2 ;;
esac
