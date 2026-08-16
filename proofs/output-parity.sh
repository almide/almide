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
# Requires: a built oracle at $ALMIDE_BIN (default target/release/almide) and
# `wasmtime`. Locally a missing tool SKIPs; under CI=true it is a hard FAILURE
# (#978 — this gate spent its life skipping because CI never had `almide` on
# PATH, and nothing noticed).
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

# In CI a missing tool is a FAILURE, not a skip (#978 — the require_or_skip
# policy of proofs/check-wasm-exec.sh): this gate SKIPped on every CI run it
# ever had, because it looked the oracle up on PATH while the workflow only
# ever built ./target/release/almide. Resolve like diff-fuzz.sh does — never
# a PATH `almide` — with ALMIDE_BIN as the override.
if ! command -v wasmtime >/dev/null; then
  if [ "${CI:-}" = "true" ]; then
    echo "::error::output-parity: wasmtime not found — in CI a missing tool is a failure (#978)"
    exit 1
  fi
  echo "output-parity: wasmtime not found — SKIP"; exit 0
fi
ALM="${ALMIDE_BIN:-$ROOT/target/release/almide}"
if [ ! -x "$ALM" ]; then
  if [ "${CI:-}" = "true" ]; then
    echo "::error::output-parity: almide (v0 oracle) not found at $ALM — in CI a missing oracle is a failure (#978)"
    exit 1
  fi
  echo "output-parity: almide (v0 oracle) not found at $ALM — SKIP"; exit 0
fi

cd "$ROOT"
cargo build -q -p almide-mir --example render_program 2>/dev/null || { echo "output-parity: render_program build failed"; exit 1; }
RP="$ROOT/target/debug/examples/render_program"

: > "$TMP/matches.txt"
match=0; wall=0; mismatch=0; runerr=0; v0fail=0; skip=0; xfail=0
# F4 (flight-evidence-gaps): a NON-DETERMINISTIC verification result is not a
# result. Under full-gate machine load the 20s alarm occasionally fires on files
# that byte-match solo (append_accumulator/list_eq/string_codepoint — recorded
# 2026-07-03), so any per-file failure is RETRIED ONCE after the sweep, alone,
# with a generous timeout. Only the solo re-run's verdict counts — a genuine
# failure fails twice; a load artifact never reaches the report.
# THREE-POINT observable comparison (contracts.toml's definition: stdout AND
# stderr AND exit code — the stdout-only harness hid every trap fixture in
# v0fail, flight-evidence-gaps item 6). A fixture whose v0 run FAILS is still
# comparable when the render succeeds: the traps (div-by-zero, index-bounds,
# unwrap-none) PROMISE identical stderr + exit 1 cross-target (C-001/C-035).
# v1 stderr is normalized (the wasmtime trap preamble names the tmp wat path).
# STDIN IS CLOSED for every fixture run. The corpus arrives on this script's own
# stdin (`done < <(find spec ...)`), so a fixture that READS stdin consumes the
# rest of the file list and the sweep stops early — silently, because the loop
# simply runs out of input and the summary prints as if it had finished.
#
# It cost a green build to learn: `count_domain_nonbytes.almd` calls
# `io.read_n_bytes`, and the run that introduced it classified 547 of 932 files
# and reported ~300 baseline entries as "stopped byte-matching" — none of which
# had regressed; they were never run. `MISMATCH=0` alongside hundreds of
# regressions is the signature.
#
# The fixture was not even the first to read stdin — it was the first to SUCCEED
# at it. `read_n_bytes(i64::MAX)` used to truncate to i32 and read nothing, so
# the hazard sat here harmless until that truncation was fixed.
run_one() { # $1=file -> sets VERDICT to match|mismatch|wall|runerr|v0fail
  local f="$1" t="$2"
  to "$t" "$ALM" run "$f" > "$TMP/v0" 2>"$TMP/v0e" < /dev/null
  local v0rc=$?
  "$RP" "$f" > "$TMP/wat" 2>/dev/null || {
    if [ "$v0rc" -ne 0 ]; then VERDICT=v0fail; else VERDICT=wall; fi
    return
  }
  to "$t" wasmtime "$TMP/wat" > "$TMP/v1" 2>"$TMP/v1e" < /dev/null
  local v1rc=$?
  if [ "$v0rc" -eq 0 ] && [ "$v1rc" -ne 0 ]; then VERDICT=runerr; return; fi
  diff -q "$TMP/v0" "$TMP/v1" >/dev/null 2>&1 || { VERDICT=mismatch; return; }
  if [ "$v0rc" -eq 0 ]; then VERDICT=match; return; fi
  # v0 FAILED (a trap/abort fixture): the full observable must agree —
  # exit code AND stderr (v1's normalized: strip the wasmtime module preamble).
  sed -e "s|$TMP/wat|<module>|g" -e '/^Error: failed to run main module/d' \
      -e '/^$/d' -e '/^Caused by:/d' -e 's/^ *[0-9]*: *//' "$TMP/v1e" > "$TMP/v1en"
  # v0's stderr is normalized symmetrically: `almide run` interleaves COMPILE
  # notes with the program's runtime stderr, and #931 made the native-fallback
  # notice print by default — compiler infrastructure, not a program
  # observable, same class as wasmtime's preamble above. Without this, every
  # trap fixture that walls the native-verified rung "regressed" the moment
  # the notice went always-on (found re-arming this gate, #978).
  # #1123: E041-era deprecation WARNINGS are compiler infrastructure, not a
  # program observable — the render leg never prints them, so a pinned
  # trap fixture carrying a warning "regressed" the moment the warning
  # shipped (same class as the #931 fallback notice below). Strip each
  # warning block (the warning line through its trailing blank line).
  awk '
    /^warning\[/ { skipw=1; next }
    skipw && /^$/ { skipw=0; next }
    skipw && (/^Error:/ || /^error\[/ || /^error:/) { skipw=0 }
    skipw { next }
    { print }
  ' "$TMP/v0e" \
    | sed -e '/^note: verified native render walled/d' -e '/^  reason: /d' \
    > "$TMP/v0en"
  if [ "$v0rc" -eq "$v1rc" ] && diff -q "$TMP/v0en" "$TMP/v1en" >/dev/null 2>&1; then
    VERDICT=match
  else
    VERDICT=xfail
  fi
}
declare -a suspects=()
# The worklist arrives on fd 3, and every fixture runs with stdin closed to
# /dev/null. Both halves are load-bearing, and neither is a style choice:
#
# A fixture is free to READ stdin — `io.read_n_bytes`, `io.read_all`,
# `io.read_line`. On the old `done < <(find …)` form that read came out of the
# SAME descriptor the loop was reading filenames from, so one such fixture
# swallowed the rest of the worklist and the sweep stopped there. Nothing
# reported an error: the tail simply never ran, the counters stopped, and every
# baseline file past the cut looked like it had "stopped byte-matching" — a
# REGRESSION verdict for work no one had touched (#1473).
#
# `spec/wasm_cross/count_domain_nonbytes.almd` is the fixture that hit it, and
# the fixture is fine. It calls `io.read_n_bytes(i64::MAX)`; before that call
# was brought under the count-domain rule it aborted on a capacity overflow
# BEFORE reaching stdin, so the defect here was masked. Fixing the intrinsic is
# what armed it — which is the shape to remember: this gate must not depend on
# what the corpus chooses to read.
while IFS= read -r f <&3; do
  grep -q 'fn main' "$f" || { skip=$((skip+1)); continue; }
  # `// wasm:skip` — a multi-module / harness-incompatible fixture that cannot run
  # STANDALONE (its imports live in sibling files); comparing a broken standalone
  # invocation proves nothing. Same class as the no-main part files.
  head -1 "$f" | grep -q 'wasm:skip' && { skip=$((skip+1)); continue; }
  run_one "$f" 20 < /dev/null
  case "$VERDICT" in
    match) match=$((match+1)); echo "$f" >> "$TMP/matches.txt" ;;
    # EVERY non-match goes to the solo retry — the load artifact shows up as any
    # verdict (a v0 `almide run` past the alarm counts as v0fail, a starved
    # render as wall), not just as runerr. Only the quiet re-run classifies.
    *)     suspects+=("$f:$VERDICT") ;;
  esac
done 3< <(find spec -name '*.almd' | sort)
# Solo retry pass — the machine is quiet now (the sweep is over). This loop
# iterates an ARRAY, so it cannot be truncated the way the sweep was; the
# redirect is here because a stdin-reading fixture would otherwise block on the
# terminal when the script is run by hand.
for sv in "${suspects[@]:-}"; do
  [ -n "$sv" ] || continue
  f="${sv%%:*}"
  run_one "$f" 60 < /dev/null
  case "$VERDICT" in
    match)    match=$((match+1)); echo "$f" >> "$TMP/matches.txt" ;;
    v0fail)   v0fail=$((v0fail+1)) ;;
    wall)     wall=$((wall+1)) ;;
    runerr)   runerr=$((runerr+1)); echo "$f" >> "$TMP/runerr.txt" ;;
    xfail)    xfail=$((xfail+1)); echo "$f" >> "$TMP/xfail.txt" ;;
    mismatch) mismatch=$((mismatch+1)); echo "$f" >> "$TMP/mismatch.txt" ;;
  esac
done
sort -o "$TMP/matches.txt" "$TMP/matches.txt"  # (re-sorted below after the retry appends)
echo "output-parity: match=$match wall=$wall MISMATCH=$mismatch RUNERR=$runerr XFAIL=$xfail v0fail=$v0fail skip=$skip"

# THE COUNTERS MUST ACCOUNT FOR THE WHOLE CORPUS. Every file lands in exactly one
# bucket, so the buckets sum to the corpus size — and if they do not, the sweep
# did not finish and every number above is a partial measurement reported as a
# total. That is worse than a red build: the baseline diff then lists every
# unreached file as a REGRESSION, burying whatever really broke under hundreds of
# files that were simply never run (547 of 932, ~300 phantom regressions,
# MISMATCH=0 — 2026-08-16). The stdin isolation in run_one fixes THAT cause; this
# catches the next one, whatever it is.
seen=$((match + wall + mismatch + runerr + xfail + v0fail + skip))
corpus=$(find spec -name '*.almd' | wc -l | tr -d ' ')
if [ "$seen" -ne "$corpus" ]; then
  echo "::error::output-parity: classified $seen of $corpus files — the sweep did not finish."
  echo "  Every count above is PARTIAL, and the baseline diff would report the"
  echo "  $((corpus - seen)) unreached file(s) as regressions. Do not ratchet from this run."
  echo "  Most likely cause: a fixture consumed this script's stdin (the corpus is piped"
  echo "  into the loop), or the loop exited early. run_one closes stdin for exactly this."
  exit 1
fi
# A failure class NAMES its files (#978: counts alone leave nothing to act on
# — a MISMATCH is the silent-miscompile class, the one this gate exists for).
if [ "$mismatch" -gt 0 ]; then
  echo "  (MISMATCH = renders and runs but the stdout bytes diverge — silent miscompile class):"
  sed 's/^/    ! /' "$TMP/mismatch.txt"
fi
if [ "$runerr" -gt 0 ]; then
  echo "  (RUNERR = renders but wasmtime rejects or traps where v0 succeeds):"
  sed 's/^/    r /' "$TMP/runerr.txt"
fi
if [ "$xfail" -gt 0 ]; then
  echo "  (XFAIL = a trap/abort fixture whose v1 observable [stderr+exit] diverges from v0 —"
  echo "   the trap-semantics contract surface not yet implemented on the MIR render path):"
  sed 's/^/    x /' "$TMP/xfail.txt"
fi

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
# `comm` requires BOTH inputs sorted under the SAME collation. `matches.txt` is
# LC_ALL=C-sorted (above), but a committed baseline may have been generated under
# a different locale, so `.`-vs-`_` ordering pairs (the *_interp_wasm cluster)
# appeared as spurious REGRESSION *and* NEW-match every run. Re-sort the baseline
# under the same C collation into a temp before comparing — the committed file is
# left untouched (regenerated canonically on the next --update).
LC_ALL=C sort "$BASELINE" > "$TMP/baseline_sorted.txt"
# REGRESSION = a baseline must-match file that is no longer matching.
regressions="$(comm -23 "$TMP/baseline_sorted.txt" "$TMP/matches.txt")"
gained="$(comm -13 "$TMP/baseline_sorted.txt" "$TMP/matches.txt")"
[ -n "$gained" ] && { echo "output-parity: NEW matches not yet in baseline (run --update to ratchet):"; echo "$gained" | sed 's/^/  + /'; }
if [ -n "$regressions" ]; then
  echo "output-parity: REGRESSION — these baseline files stopped byte-matching v0:" >&2
  echo "$regressions" | sed 's/^/  - /' >&2
  rm -rf "$TMP"; exit 1
fi
echo "output-parity: OK — all $(wc -l < "$BASELINE" | tr -d ' ') baseline files still byte-match v0."
rm -rf "$TMP"; exit 0
