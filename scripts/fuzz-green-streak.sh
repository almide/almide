#!/usr/bin/env bash
# Aviation-quality Stage 4: the fuzz true-green streak meter.
#
# An auditor of a mission-critical toolchain reads "how long has it stayed
# unbroken", not "how fast do they fix it". This script computes the current
# CONSECUTIVE-CLEAN-DAY streak of the Fuzz (nightly) workflow — a calendar day
# counts as clean only when EVERY run that day concluded `success`; any
# failure/cancellation breaks the streak; a day with no run is skipped (the
# streak neither grows nor resets — scheduler gaps are not evidence either
# way). With --update, the dated ledger at
# research/benchmark/fuzz-green/README.md is refreshed (BENCHMARKS.md
# discipline: measurements are dated, never overwritten silently).
#
# First milestone: 90 consecutive clean days.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LEDGER_DIR="$ROOT/research/benchmark/fuzz-green"
LEDGER="$LEDGER_DIR/README.md"

json=$(gh run list --repo almide/almide --workflow fuzz-nightly.yml --limit 200 \
    --json conclusion,createdAt \
    --jq '[.[] | {c: .conclusion, d: (.createdAt | split("T")[0])}]')

read -r STREAK FIRST_GREEN LAST_DAY <<EOF
$(python3 - "$json" <<'PYEOF'
import json
import sys
from collections import OrderedDict

runs = json.loads(sys.argv[1])
by_day = OrderedDict()
for r in runs:  # newest first
    by_day.setdefault(r["d"], []).append(r["c"])

streak = 0
first_green = "-"
last_day = next(iter(by_day), "-")
for day, cs in by_day.items():  # newest -> oldest
    if all(c == "success" for c in cs):
        streak += 1
        first_green = day
    else:
        break
print(streak, first_green, last_day)
PYEOF
)
EOF

echo "fuzz true-green streak: ${STREAK} consecutive clean day(s) (latest run day: ${LAST_DAY}; streak start: ${FIRST_GREEN}; milestone: 90)"

if [ "${1:-}" = "--update" ]; then
    mkdir -p "$LEDGER_DIR"
    TODAY=$(date -u +%Y-%m-%d)
    {
        echo "# Fuzz true-green streak (aviation-quality Stage 4)"
        echo
        echo "The metric a mission-critical auditor reads: not \"how fast do they fix"
        echo "it\" but \"how long has it stayed unbroken\". A calendar day is CLEAN only"
        echo "when every Fuzz (nightly) run that day concluded success; any failure"
        echo "breaks the streak; a day without a run neither grows nor resets it."
        echo "First milestone: **90 consecutive clean days**."
        echo
        echo "Meter: \`scripts/fuzz-green-streak.sh\` (append a dated row with \`--update\`)."
        echo
        echo "| measured (UTC) | streak (days) | streak start | latest run day |"
        echo "|---|---|---|---|"
        if [ -f "$LEDGER" ]; then
            grep -E '^\| [0-9]{4}-' "$LEDGER" || true
        fi
        echo "| ${TODAY} | ${STREAK} | ${FIRST_GREEN} | ${LAST_DAY} |"
    } > "$LEDGER.tmp"
    mv "$LEDGER.tmp" "$LEDGER"
    echo "ledger updated: $LEDGER"
fi
