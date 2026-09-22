#!/usr/bin/env bash
# check-readme-freshness.sh — the README's MEASURED claims must be young.
#
# check-readme-numbers.sh makes every hand-written count carry the date it was
# measured. A date is honest, but a reader still has to do the arithmetic; the
# LLM-writability scorecard sat at 2026-04-12 for five months while being the
# page's strongest claim (#1617, #2146). The arithmetic lives in
# scripts/lib/readme-freshness.sh, which check-readme-numbers.sh reads too, so
# the two gates cannot disagree about the line or the section list.
#
# This script is the DELIVERY half. check-readme-numbers.sh fails the PR that
# is open when a section is stale; that is the gate #2146 item 4 asks for, but
# it only fires when somebody happens to open a PR. A number goes stale on a
# calendar, so this runs nightly (.github/workflows/readme-freshness.yml) and
# the red is delivered to the issue that owns the re-measurement (#1617),
# which is where someone acts on it (see #2391 for why delivery matters).
#
#   bash scripts/check-readme-freshness.sh            # gate (exit 1 when stale)
#   MAX_DAYS=120 bash scripts/check-readme-freshness.sh
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
cd "$here/.." || exit 2

# shellcheck source=lib/readme-freshness.sh
. "$here/lib/readme-freshness.sh"

if readme_freshness_report; then
  echo "readme-freshness: every measured section is within $README_FRESHNESS_MAX_DAYS days."
else
  exit 1
fi
