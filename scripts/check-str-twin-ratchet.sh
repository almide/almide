#!/usr/bin/env bash
# _str TWIN RATCHET (#1460 item 2)
# ================================
#
# The stdlib has no `protocol` use, and the bill is the `_str` suffix family:
# every String-element operation is a hand-written twin of its Int sibling.
# Whether the stdlib adopts protocols or ratifies the suffix scheme is a mob
# decision (#1460) — but while it is pending, the NUMBER must not creep: the
# triage measured 119 on 2026-08-16 and 120 one day later. This ratchet
# freezes it, shrink-only.
#
#   count > baseline  -> FAIL (a new twin needs the mob decision first, or a
#                        conscious baseline bump in the same PR with why)
#   count < baseline  -> FAIL as STALE (celebrate: lower the baseline)
set -euo pipefail
export LC_ALL=C

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BASELINE_FILE="$ROOT/scripts/str-twin-baseline.txt"

count=$(grep -hoE '^fn [a-z0-9_]+_str\(' "$ROOT"/stdlib/*.almd | wc -l | tr -d ' ')
baseline=$(grep -E '^[0-9]+$' "$BASELINE_FILE")

if [ "$count" -gt "$baseline" ]; then
  echo "::error::str-twin-ratchet: $count _str twins in stdlib, baseline $baseline — the suffix bill grew. A new twin needs #1460's direction decided (or a justified baseline bump in this PR)."
  exit 1
fi
if [ "$count" -lt "$baseline" ]; then
  echo "::error::str-twin-ratchet: $count _str twins, baseline $baseline — STALE baseline; lower it to $count in this PR (shrink-only, the good direction)."
  exit 1
fi
echo "str-twin-ratchet: $count _str twin(s), baseline held"
