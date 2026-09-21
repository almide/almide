#!/usr/bin/env bash
# Aviation-quality Stage 4: the fuzz true-green streak meter.
#
# An auditor of a mission-critical toolchain reads "how long has it stayed
# unbroken", not "how fast do they fix it". This script computes the current
# CONSECUTIVE-CLEAN-DAY streak of the Fuzz (nightly) workflow — a calendar day
# counts as clean only when EVERY run that day concluded `success`; any
# failure/cancellation breaks the streak; a day with no run is skipped (the
# streak neither grows nor resets — scheduler gaps are not evidence either
# way).
#
# WHAT "success" MEANS HERE (corrected 2026-08-11, and again 2026-08-17): the
# night's truth is the NIGHT VERDICT job — it fails on correctness findings
# (and on a night with no evidence at all), and tolerates a shard killed by a
# runner shutdown (exit 143, the documented ~1-in-6 event) as reduced
# coverage. The 2026-08-11 fix made the verdict RUN unconditionally; this
# meter, however, kept scoring the RUN conclusion — which a killed shard leg
# still paints red — so infra kills went on breaking the streak the verdict
# had already absorbed (shard 5 on 2026-08-17: SIGTERM at 295s, findings=0,
# run red). The meter now scores each run's VERDICT JOB conclusion, exactly
# like scripts/fuzz-track-record.sh; a run whose verdict job never concluded
# scores as failure (the aggregation itself died — that IS evidence).
# A streak only measures correctness if the thing it counts is correctness.
#
# COVERAGE BAR (#2390, under #924's 2026-09-21 ruling): a green verdict over a
# quarter of the planned fuzz-minutes is a weaker assurance than one over the
# whole campaign, and the two used to be indistinguishable here. A run now
# scores `success` only when its verdict concluded success AND the night's
# `fuzz-night:` record line (read from the verdict job's log) shows >= 75% of
# the planned fuzz-minutes delivered. Below that it is `partial`; a verdict
# job whose log carries no record line is `no-record` — unknown is not clean.
# Shards that did not report are named in the record line; their findings
# are unknown, not zero, and at >= 75% delivered the night still counts.
# The rule is scripts/lib/fuzz-night-line.sh (`fuzz_night_score`), shared
# with scripts/fuzz-track-record.sh.
#
# With --update, the dated ledger at
# research/benchmark/fuzz-green/README.md is refreshed (BENCHMARKS.md
# discipline: measurements are dated, never overwritten silently).
#
# First milestone: 90 consecutive clean days.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LEDGER_DIR="$ROOT/research/benchmark/fuzz-green"
LEDGER="$LEDGER_DIR/README.md"

# shellcheck source=scripts/lib/fuzz-night-line.sh
. "$ROOT/scripts/lib/fuzz-night-line.sh"

REPO="almide/almide"
VERDICT_JOB="Night verdict"
LEGACY_JOB="Generative differential fuzz"

runs=$(gh api "repos/$REPO/actions/workflows/fuzz-nightly.yml/runs?per_page=60" \
  --jq '.workflow_runs[] | [.id, (.conclusion // .status // "unknown"), .created_at] | @tsv')

# One JSON row per run: the verdict-job conclusion qualified by the night's
# delivered coverage (sharded nights), the single campaign job (legacy
# nights), else the run conclusion (no verdict ever ran — a whole-run failure,
# scored as such).
json="["
sep=""
while IFS=$'\t' read -r id run_conc created; do
  [ -n "$id" ] || continue
  day="${created%%T*}"
  jobs=$(gh api "repos/$REPO/actions/runs/$id/jobs?per_page=100" \
    --jq '[.jobs[] | {id, name, conclusion}]' 2>/dev/null || echo '[]')
  vjob=$(jq -r "[.[] | select(.name == \"$VERDICT_JOB\")][0] // empty | \"\(.id)\t\(.conclusion // \"unknown\")\"" <<<"$jobs")
  if [ -n "$vjob" ]; then
    IFS=$'\t' read -r vid vconc <<<"$vjob"
    log=$(gh api "repos/$REPO/actions/jobs/$vid/logs" 2>/dev/null || true)
    line=$(fuzz_night_line <(printf '%s\n' "$log"))
    IFS=$'\t' read -r _full green _coverage _text <<<"$(fuzz_night_score "$vconc" "$line")"
    if [ "$green" -eq 1 ]; then v="success"
    elif [ "$vconc" != "success" ]; then v="$vconc"
    elif [ -z "$line" ]; then v="no-record"
    else v="partial"; fi
  else
    v=$(jq -r "[.[] | select(.name == \"$LEGACY_JOB\") | (.conclusion // \"unknown\")][0] // empty" <<<"$jobs")
    [ -n "$v" ] || v="$run_conc"
  fi
  json="$json$sep{\"c\": \"$v\", \"d\": \"$day\"}"
  sep=","
done <<< "$runs"
json="$json]"

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
        echo "when every Fuzz (nightly) run that day delivered a NIGHT VERDICT that"
        echo "concluded success over at least 75% of the planned fuzz-minutes (findings"
        echo "fail it; a reclaimed shard does not, until the night falls under the 75%"
        echo "line — #924's ruling, read from the \`fuzz-night:\` record line); any"
        echo "failure breaks the streak; a day without a run neither grows nor resets it."
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
