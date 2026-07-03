#!/usr/bin/env bash
# OUTPUT-PARITY GATE (the 4th dimension the corpus-wall does NOT check).
#
# corpus-wall.sh proves the 3 SOUNDNESS properties (ownership/names/caps) + lower
# totality over the corpus — statically, via the Coq checker. It does NOT execute
# programs or compare stdout. So "v1 output == v0 output" is a SEPARATE, unproven
# dimension. This gate closes that blind spot: it RUNS every spec/ program with a
# `fn main` through both pipelines and byte-diffs stdout.
#
#   v0 oracle : `almide run <f>`                                  (native)
#   v1        : examples/render_program <f> -> wat -> `wasmtime`  (the trust-spine path)
#
# Categories: MATCH / WALL (clean Unsupported — expected for unlinked stdlib) /
# MISMATCH (renders but wrong bytes = silent miscompile) / RUNERR (renders but
# wasmtime rejects the wasm = invalid wasm) / v0fail (v0 can't run = effect/input).
#
# RATCHET: proofs/output-parity-baseline.txt lists the files that MUST byte-match.
# The gate FAILS if any baseline file stops matching (a regression). As fixes land,
# re-run with `--update` to ADD newly-matching files (the baseline only grows).
#
#   bash proofs/output-parity.sh            # gate: fail on regression vs baseline
#   bash proofs/output-parity.sh --update   # ratchet: regenerate the baseline
#
# Requires: a built `almide` on PATH (v0 oracle) and `wasmtime`. Skips gracefully
# if either is absent (so it never blocks an environment that lacks them).
set -uo pipefail
# Determinism: sort/comm collation is LOCALE-DEPENDENT (`.` vs `_` invert between
# C and UTF-8 collation), which made the SAME files appear as both "new match"
# and "regression" (2026-07-03). Evidence comparison must be byte-ordered.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# F6-2: identity of the evidence — stamp + verify the toolchain (see proofs/lib/stamp.sh).
source "$ROOT/proofs/lib/stamp.sh"
stamp_toolchain "$ROOT" || exit 1

BASELINE="$ROOT/proofs/output-parity-baseline.txt"
TMP="${TMPDIR:-/tmp}/almide-output-parity.$$"
mkdir -p "$TMP"
to() { perl -e 'alarm shift @ARGV; exec @ARGV' "$@"; }   # macOS has no `timeout`

command -v wasmtime >/dev/null || { echo "output-parity: wasmtime not found — SKIP"; exit 0; }
ALM="$(command -v almide || true)"
[ -n "$ALM" ] || { echo "output-parity: almide (v0 oracle) not found — SKIP"; exit 0; }

cd "$ROOT"
cargo build -q -p almide-mir --example render_program 2>/dev/null || { echo "output-parity: render_program build failed"; exit 1; }
RP="$ROOT/target/debug/examples/render_program"

: > "$TMP/matches.txt"
match=0; wall=0; mismatch=0; runerr=0; v0fail=0; skip=0
# F4 (flight-evidence-gaps): a NON-DETERMINISTIC verification result is not a
# result. Under full-gate machine load the 20s alarm occasionally fires on files
# that byte-match solo (append_accumulator/list_eq/string_codepoint — recorded
# 2026-07-03), so any per-file failure is RETRIED ONCE after the sweep, alone,
# with a generous timeout. Only the solo re-run's verdict counts — a genuine
# failure fails twice; a load artifact never reaches the report.
run_one() { # $1=file -> sets VERDICT to match|mismatch|wall|runerr|v0fail
  local f="$1" t="$2"
  to "$t" "$ALM" run "$f" > "$TMP/v0" 2>/dev/null || { VERDICT=v0fail; return; }
  "$RP" "$f" > "$TMP/wat" 2>/dev/null || { VERDICT=wall; return; }
  to "$t" wasmtime "$TMP/wat" > "$TMP/v1" 2>/dev/null || { VERDICT=runerr; return; }
  if diff -q "$TMP/v0" "$TMP/v1" >/dev/null 2>&1; then VERDICT=match; else VERDICT=mismatch; fi
}
declare -a suspects=()
while IFS= read -r f; do
  grep -q 'fn main' "$f" || { skip=$((skip+1)); continue; }
  run_one "$f" 20
  case "$VERDICT" in
    match) match=$((match+1)); echo "$f" >> "$TMP/matches.txt" ;;
    # EVERY non-match goes to the solo retry — the load artifact shows up as any
    # verdict (a v0 `almide run` past the alarm counts as v0fail, a starved
    # render as wall), not just as runerr. Only the quiet re-run classifies.
    *)     suspects+=("$f:$VERDICT") ;;
  esac
done < <(find spec -name '*.almd' | sort)
# Solo retry pass — the machine is quiet now (the sweep is over).
for sv in "${suspects[@]:-}"; do
  [ -n "$sv" ] || continue
  f="${sv%%:*}"
  run_one "$f" 60
  case "$VERDICT" in
    match)    match=$((match+1)); echo "$f" >> "$TMP/matches.txt" ;;
    v0fail)   v0fail=$((v0fail+1)) ;;
    wall)     wall=$((wall+1)) ;;
    runerr)   runerr=$((runerr+1)) ;;
    mismatch) mismatch=$((mismatch+1)) ;;
  esac
done
sort -o "$TMP/matches.txt" "$TMP/matches.txt"  # (re-sorted below after the retry appends)
echo "output-parity: match=$match wall=$wall MISMATCH=$mismatch RUNERR=$runerr v0fail=$v0fail skip=$skip"

# The retry loop appends AFTER the first sort — comm(1) requires sorted input,
# so re-sort before any baseline comparison (the unsorted tail made comm report
# three phantom regressions, 2026-07-03).
sort -o "$TMP/matches.txt" "$TMP/matches.txt"

if [ "${1:-}" = "--update" ]; then
  cp "$TMP/matches.txt" "$BASELINE"
  echo "output-parity: baseline updated -> $BASELINE ($match files)"
  rm -rf "$TMP"; exit 0
fi

[ -f "$BASELINE" ] || { echo "output-parity: no baseline ($BASELINE) — run with --update first"; rm -rf "$TMP"; exit 0; }
# REGRESSION = a baseline must-match file that is no longer matching.
regressions="$(comm -23 "$BASELINE" "$TMP/matches.txt")"
gained="$(comm -13 "$BASELINE" "$TMP/matches.txt")"
[ -n "$gained" ] && { echo "output-parity: NEW matches not yet in baseline (run --update to ratchet):"; echo "$gained" | sed 's/^/  + /'; }
if [ -n "$regressions" ]; then
  echo "output-parity: REGRESSION — these baseline files stopped byte-matching v0:" >&2
  echo "$regressions" | sed 's/^/  - /' >&2
  rm -rf "$TMP"; exit 1
fi
echo "output-parity: OK — all $(wc -l < "$BASELINE" | tr -d ' ') baseline files still byte-match v0."
rm -rf "$TMP"; exit 0
