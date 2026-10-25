# Shared by scripts/check-host-determinism.sh and
# scripts/check-browser-determinism.sh (#2753): compare the fixtures a
# determinism run saw WALL on both sides against the equality-pinned ledger
# proofs/determinism-walled-baseline.txt. Sourced, not run.
#
#   check_walled_ledger <ledger> <walled-names-file> <gate-name>
#
# <walled-names-file> holds one fixture basename per line. Prints one line per
# offender and returns 1 on any: a wall without a ledger row (coverage shrank)
# or a ledger row whose fixture no longer walls (STALE — prune it).
# [selected-file] (the run's fixture paths, under an ALMIDE_CI_SLICE run):
# only the rows of fixtures this run compared are judged — a row outside the
# slice is not "no longer walling"; the full run judges every row.
check_walled_ledger() {
  local ledger="$1" seen="$2" gate="$3" selected="${4:-}" rc=0 name
  [ -f "$ledger" ] || { echo "::error::$gate: wall ledger $ledger missing"; return 1; }
  local want; want="$(mktemp)"
  grep -v '^[[:space:]]*#' "$ledger" | grep -v '^[[:space:]]*$' | sed 's/[[:space:]]*::.*$//' | LC_ALL=C sort -u > "$want"
  if [ -n "$selected" ] && [ "${ALMIDE_CI_SLICE:-1}" -gt 1 ] 2>/dev/null; then
    local sel; sel="$(mktemp)"
    sed 's#.*/##' "$selected" | LC_ALL=C sort -u > "$sel"
    LC_ALL=C comm -12 "$want" "$sel" > "$want.in"; mv "$want.in" "$want"; rm -f "$sel"
  fi
  local got; got="$(mktemp)"
  LC_ALL=C sort -u "$seen" > "$got"
  while IFS= read -r name; do
    [ -n "$name" ] || continue
    echo "::error::$gate: $name walls on the structural leg with no row in $ledger — coverage shrank; fix the decline or add the row (and raise MAX_WALLED) consciously in the same change"
    rc=1
  done < <(LC_ALL=C comm -13 "$want" "$got")
  while IFS= read -r name; do
    [ -n "$name" ] || continue
    echo "::error::$gate: $name has a row in $ledger but no longer walls — STALE; delete the row and lower MAX_WALLED in the same change"
    rc=1
  done < <(LC_ALL=C comm -23 "$want" "$got")
  rm -f "$want" "$got"
  return "$rc"
}

# The ledger's row count — MAX_WALLED in each script must equal it, so the
# number in the script and the names in the ledger cannot drift apart.
walled_ledger_rows() {
  # `|| true`: an emptied ledger counts 0 rows, and grep -c exits 1 on 0.
  grep -v '^[[:space:]]*#' "$1" | grep -cv '^[[:space:]]*$' || true
}
