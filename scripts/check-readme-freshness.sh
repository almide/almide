#!/usr/bin/env bash
# check-readme-freshness.sh — the README's MEASURED claims must be young.
#
# check-readme-numbers.sh makes every hand-written count carry the date it was
# measured. A date is honest, but a reader still has to do the arithmetic; the
# LLM-writability scorecard sat at 2026-04-12 for five months while being the
# page's strongest claim (#1617, #2146). This gate does the arithmetic: each
# dated measurement line in the sections listed below must be at most MAX_DAYS
# old, or the gate is red and names the section, its date and its age.
#
# It is NOT a PR gate. A number that goes stale does so on a calendar, not in
# a diff; failing every PR for it would make people edit the date, not re-run
# the measurement. It runs nightly (.github/workflows/readme-freshness.yml)
# and the red is delivered to the issue that owns the re-measurement (#1617),
# which is where someone acts on it (see #2391 for why delivery matters).
#
#   bash scripts/check-readme-freshness.sh            # gate (exit 1 when stale)
#   MAX_DAYS=120 bash scripts/check-readme-freshness.sh
set -euo pipefail
cd "$(dirname "$0")/.." || exit 2

MAX_DAYS="${MAX_DAYS:-90}"
# Section heading → the measurement it owns. A section is scanned from its
# heading to the next heading of the same or higher level; the FIRST
# YYYY-MM-DD in it is the measurement date. Add a row when a section starts
# carrying a measured claim; never remove one — retire it by measuring.
SECTIONS=(
  '### LLM writability'
)

today=$(date -u +%Y-%m-%d)
to_epoch() { # YYYY-MM-DD -> seconds (GNU and BSD date)
  date -u -d "$1" +%s 2>/dev/null || date -u -j -f '%Y-%m-%d' "$1" +%s
}
now=$(to_epoch "$today")

fail=0
for heading in "${SECTIONS[@]}"; do
  level=$(printf '%s' "$heading" | sed -E 's/^(#+).*/\1/' | wc -c | tr -d ' ')
  level=$((level - 1))
  d=$(awk -v h="$heading" -v lvl="$level" '
    $0 == h { on = 1; next }
    on && /^#+ / { n = match($0, /^#+/); if (RLENGTH <= lvl) exit }
    on && match($0, /20[0-9][0-9]-[0-9][0-9]-[0-9][0-9]/) { print substr($0, RSTART, RLENGTH); exit }
  ' README.md)
  if [ -z "$d" ]; then
    echo "::error::README.md: section '$heading' carries no YYYY-MM-DD measurement date (check-readme-numbers.sh should have refused this)"
    fail=1
    continue
  fi
  age=$(( (now - $(to_epoch "$d")) / 86400 ))
  if [ "$age" -gt "$MAX_DAYS" ]; then
    echo "::error::README.md: '$heading' was measured on $d — $age days ago, above the $MAX_DAYS-day line. Re-run the measurement (almide-dojo owns it: #1617) and replace the table and its date; do not edit the date alone."
    fail=1
  else
    echo "readme-freshness: '$heading' measured $d ($age days ago, line $MAX_DAYS)"
  fi
done

if [ "$fail" -ne 0 ]; then exit 1; fi
echo "readme-freshness: every measured section is within $MAX_DAYS days."
