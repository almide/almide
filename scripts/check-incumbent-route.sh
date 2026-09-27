#!/usr/bin/env bash
# INCUMBENT-ROUTE CENSUS (#2741, part of #2739: retiring the incumbent wasm
# emitter). Every program the wasm router hands to the incumbent renderer is a
# row in proofs/incumbent-route-baseline.txt, with the structural leg's decline
# reason. The set may only SHRINK, to 0: the incumbent is being deleted, so a
# program newly served by it is a regression onto the leg that is going away.
#
# The other wasm ratchets count something else: proofs/wasm-fallback-baseline.txt
# is files NEITHER leg passes, and walled-real / MAX_WALLED count what the
# incumbent REFUSES. Nothing counted what it SERVES — the largest group (the
# stock-WASI builds rerouted because an fs op has no p1 shim) was in no ledger.
#
# Two lanes, each read from the router's own narration:
#
#   build — `almide check --target wasm` (the build's routing decision minus the
#           write, #1922) under ALMIDE_VERIFIED_DEBUG=1, over every tracked
#           program with a `main` in spec/, examples/, research/benchmark/ and
#           demo/. A row is a program whose narration says the incumbent emitted
#           the module: `structural leg declined (<why>) — incumbent renderer`
#           (reason = <why>), or a direct route with no decline (reason
#           `route:direct` — the @export / main-less routes of #2752).
#   test  — `almide test` at the repo root under ALMIDE_WALL_REASON=1, the run
#           proofs/check-wasm-fallback.sh already makes. A row is a file with a
#           `[wall] <file>: structural <stage>: …` line and NO incumbent wall
#           line (a file both legs wall is the fallback baseline's, not ours).
#           The lane names only the FIRST structural decline per file.
#
# Reasons are normalised to one spelling across the lanes: an emit wall
# `Unsupported("x")` is `x`; a host op the stock-WASI p1 shim does not serve is
# `host-op:<n> (p1)` on both lanes; any other stage is `<stage>: <detail>`.
#
# The verdict, keyed on `lane :: file`:
#   - an observed key NOT in the ledger       -> FAIL (NEW: a regression onto the incumbent);
#   - a ledger key no longer observed         -> FAIL (STALE: it lowers structurally now —
#                                                prune the row in the same change);
#   - a key in both with a different reason   -> PASS with a notice. A fix to the first
#     decline can expose a second one in the same file; the row keeps its file and the
#     reason is refreshed (`--prune` does both).
#
#   ALMIDE_BIN=target/release/almide bash scripts/check-incumbent-route.sh
#   ... --prune     rewrite the ledger: drop STALE rows, refresh changed reasons.
#                   It never ADDS a row — a new one is a regression, not a re-anchor.
#   TEST_LOG=<file> reuse the stderr of an `ALMIDE_WALL_REASON=1 almide test` run at
#                   the root (CI: written by check-wasm-fallback.sh via FALLBACK_TEST_LOG)
#                   instead of running the suite again.
#   LANES="build"   measure one lane only (the other lane's rows are left alone).
set -uo pipefail
export LC_ALL=C
export PATH="/opt/homebrew/bin:$PATH"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT" || exit 2

BIN="${ALMIDE_BIN:-almide}"
case "$BIN" in */*) case "$BIN" in /*) ;; *) BIN="$ROOT/$BIN" ;; esac ;; esac
LEDGER="${LEDGER:-proofs/incumbent-route-baseline.txt}"
JOBS="${JOBS:-$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)}"
LANES="${LANES:-build test}"
prune=0
[ "${1:-}" = "--prune" ] && prune=1

"$BIN" --version >/dev/null 2>&1 || { echo "::error::incumbent-route: ALMIDE_BIN is not runnable: $BIN"; exit 2; }
[ -f "$LEDGER" ] || { echo "::error::incumbent-route: $LEDGER missing"; exit 2; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
: > "$tmp/observed"

# One normaliser for both lanes' reasons.
norm_reason() {
  sed -E \
    -e 's/^host op ([0-9]+) has no stock-WASI service.*$/host-op:\1 (p1)/' \
    -e 's/^emit: Unsupported\("(.*)"\)$/\1/' \
    -e 's/^host audit: op ([0-9]+) is not served by the p1 shim$/host-op:\1 (p1)/'
}

# ── build lane ───────────────────────────────────────────────────────────────
if [[ " $LANES " == *" build "* ]]; then
  git ls-files 'spec/*.almd' 'examples/*.almd' 'research/benchmark/*.almd' 'demo/*.almd' \
    | while IFS= read -r f; do
        grep -qE '^[[:space:]]*(pub[[:space:]]+)?(effect[[:space:]]+)?fn[[:space:]]+main[[:space:]]*\(' "$f" && printf '%s\n' "$f"
      done > "$tmp/build-corpus"
  n_build=$(wc -l < "$tmp/build-corpus" | tr -d ' ')
  [ "$n_build" -ge 500 ] || { echo "::error::incumbent-route: only $n_build build-lane program(s) found — the corpus moved and the gate went blind"; exit 1; }
  mkdir -p "$tmp/b"
  export BIN tmp
  # A project entry (`<dir>/src/main.almd` beside `<dir>/almide.toml`) is checked
  # from its project root, as `almide build` would build it. The 120 s ceiling
  # turns a hang into a named failure instead of a stalled job.
  # shellcheck disable=SC2016
  xargs -P "$JOBS" -I{} bash -c '
    f="$1"; key="$(printf "%s" "$f" | tr "/" "_")"; d="$(dirname "$f")"
    if [ "$(basename "$f")" = main.almd ] && [ -f "$d/../almide.toml" ]; then
      (cd "$d/.." && ALMIDE_VERIFIED_DEBUG=1 perl -e "alarm 120; exec @ARGV" "$BIN" check --target wasm) > /dev/null 2> "$tmp/b/$key" < /dev/null
    else
      (cd "$d" && ALMIDE_VERIFIED_DEBUG=1 perl -e "alarm 120; exec @ARGV" "$BIN" check --target wasm "$(basename "$f")") > /dev/null 2> "$tmp/b/$key" < /dev/null
    fi
    echo "$?" > "$tmp/b/$key.rc"
  ' _ {} < "$tmp/build-corpus"
  hung=""
  while IFS= read -r f; do
    key="$(printf "%s" "$f" | tr "/" "_")"
    e="$tmp/b/$key"
    [ "$(cat "$e.rc" 2>/dev/null)" = 142 ] && hung="$hung $f"
    grep -q 'v1 trust-spine emitted the module' "$e" || continue
    why=$(grep -m1 -o 'structural leg declined (.*) — incumbent renderer' "$e" \
      | sed -e 's/^structural leg declined (//' -e 's/) — incumbent renderer$//')
    [ -n "$why" ] || why="route:direct"
    printf 'build :: %s :: %s\n' "$f" "$(printf '%s' "$why" | norm_reason)" >> "$tmp/observed"
  done < "$tmp/build-corpus"
  if [ -n "$hung" ]; then
    echo "::error::incumbent-route: \`almide check --target wasm\` hung (120 s) on:$hung"
    exit 1
  fi
fi

# ── test lane ────────────────────────────────────────────────────────────────
if [[ " $LANES " == *" test "* ]]; then
  if [ -n "${TEST_LOG:-}" ]; then
    [ -s "$TEST_LOG" ] || { echo "::error::incumbent-route: TEST_LOG=$TEST_LOG is empty or missing"; exit 2; }
    cp "$TEST_LOG" "$tmp/test.log"
  else
    ALMIDE_WALL_REASON=1 "$BIN" test > /dev/null 2> "$tmp/test.log" < /dev/null || true
  fi
  # The lane must have run at all: a log with no route narration would read as
  # "nothing served by the incumbent".
  grep -q '^\[route\] ' "$tmp/test.log" \
    || { echo "::error::incumbent-route: the test log has no [route] narration — was it run with ALMIDE_WALL_REASON=1?"; exit 2; }
  grep '^\[wall\] ' "$tmp/test.log" | sed 's/^\[wall\] //' > "$tmp/walls"
  # file -> first structural decline; files with an incumbent wall are dropped.
  grep -v ': structural ' "$tmp/walls" | sed 's/: .*//' | sort -u > "$tmp/incumbent-walled"
  grep ': structural ' "$tmp/walls" | awk -F': structural ' '!seen[$1]++ { print $1 "\t" $2 }' \
    | while IFS=$'\t' read -r f why; do
        grep -qxF "$f" "$tmp/incumbent-walled" && continue
        printf 'test :: %s :: %s\n' "$f" "$(printf '%s' "$why" | norm_reason)"
      done >> "$tmp/observed"
fi

sort -u "$tmp/observed" -o "$tmp/observed"
# The ledger rows of the lanes measured this run; the others are carried as-is.
grep -v '^#' "$LEDGER" | sed '/^[[:space:]]*$/d' > "$tmp/ledger-all"
: > "$tmp/ledger"; : > "$tmp/ledger-other"
while IFS= read -r row; do
  lane="${row%% :: *}"
  if [[ " $LANES " == *" $lane "* ]]; then echo "$row" >> "$tmp/ledger"; else echo "$row" >> "$tmp/ledger-other"; fi
done < "$tmp/ledger-all"
sort -u "$tmp/ledger" -o "$tmp/ledger"

key() { awk -F' :: ' '{ print $1 " :: " $2 }' "$1" | sort -u; }
row_of() { awk -F' :: ' -v k="$1" '$1 " :: " $2 == k { print; exit }' "$2"; }
key "$tmp/observed" > "$tmp/k-observed"
key "$tmp/ledger" > "$tmp/k-ledger"
new=$(comm -23 "$tmp/k-observed" "$tmp/k-ledger")
stale=$(comm -13 "$tmp/k-observed" "$tmp/k-ledger")
changed=$(comm -13 "$tmp/observed" "$tmp/ledger" | awk -F' :: ' '{ print $1 " :: " $2 }' | sort -u | comm -12 - "$tmp/k-observed")

if [ -n "$changed" ]; then
  echo "incumbent-route: the decline reason changed for (allowed — the lane reports the first decline only):"
  while IFS= read -r k; do
    was=$(row_of "$k" "$tmp/ledger" | sed 's/^[^:]* :: [^:]* :: //')
    now=$(row_of "$k" "$tmp/observed" | sed 's/^[^:]* :: [^:]* :: //')
    echo "  ~ $k: $was -> $now"
  done <<< "$changed"
  echo "  (\`bash scripts/check-incumbent-route.sh --prune\` refreshes the reasons)"
fi

if [ "$prune" = 1 ]; then
  {
    grep '^#' "$LEDGER"
    { cat "$tmp/ledger-other"; comm -12 "$tmp/k-observed" "$tmp/k-ledger" | while IFS= read -r k; do
        row_of "$k" "$tmp/observed"
      done; } | sort -u
  } > "$tmp/new-ledger"
  mv "$tmp/new-ledger" "$LEDGER"
  echo "incumbent-route: ledger rewritten — $(echo "$stale" | sed '/^$/d' | wc -l | tr -d ' ') stale row(s) pruned, reasons refreshed; no row added"
fi

fail=0
if [ -n "$new" ]; then
  echo "::error::INCUMBENT-ROUTE REGRESSION — program(s) the router now hands to the incumbent that are NOT in $LEDGER:"
  while IFS= read -r k; do row_of "$k" "$tmp/observed" | sed 's/^/  + /'; done <<< "$new"
  echo "  The incumbent is being retired (#2739): make the structural leg lower the shape instead."
  fail=1
fi
if [ -n "$stale" ] && [ "$prune" = 0 ]; then
  echo "::error::STALE incumbent-route row(s) — these now lower structurally; prune them in this change (the set only shrinks):"
  echo "$stale" | sed 's/^/  - /'
  echo "  (\`bash scripts/check-incumbent-route.sh --prune\` drops them)"
  fail=1
fi
[ "$fail" -eq 0 ] || exit 1

total=$(grep -v '^#' "$LEDGER" | sed '/^[[:space:]]*$/d' | wc -l | tr -d ' ')
echo "INCUMBENT-ROUTE CENSUS OK: $total program(s) served by the incumbent, all in the ledger (lanes measured: $LANES; target: 0)."
