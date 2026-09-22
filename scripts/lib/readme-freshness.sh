#!/usr/bin/env bash
# scripts/lib/readme-freshness.sh — how old a MEASURED README claim is.
#
# Sourced, never run. Two gates read the same answer from here so they cannot
# disagree about which sections are measured or where the line is:
#
#   scripts/check-readme-numbers.sh   the PR gate — the MSR row (#2146 item 4):
#                                     "a stale number is a red gate, not a
#                                     footnote"
#   scripts/check-readme-freshness.sh the nightly — the same reading delivered
#                                     to the issue that owns the
#                                     re-measurement (#1617), because a number
#                                     goes stale on a calendar, not in a diff,
#                                     and a gate is finished when it reaches
#                                     someone who acts on it (#2391)
#
# Neither needs a key or the network: the measurement date is in README.md.
# Keeping the constant and the section list in one file is the point — the
# 90-day line written down twice is the "second place to remember" bug class.

# The line. Override for a one-off reading, never in a gate.
README_FRESHNESS_MAX_DAYS="${MAX_DAYS:-90}"

# Section heading → the measurement it owns. A section is scanned from its
# heading to the next heading of the same or higher level; the FIRST
# YYYY-MM-DD in it is the measurement date, so the dated sentence that states
# the measurement goes first and any older comparison date goes after it.
# Add a row when a section starts carrying a measured claim; never remove one
# — retire it by measuring.
README_MEASURED_SECTIONS=(
  '### LLM writability'
)

# YYYY-MM-DD -> seconds since the epoch (GNU date and BSD date).
readme_to_epoch() {
  date -u -d "$1" +%s 2>/dev/null || date -u -j -f '%Y-%m-%d' "$1" +%s
}

# The first YYYY-MM-DD inside a section, or the empty string.
readme_section_date() {
  awk -v h="$1" -v lvl="$2" '
    $0 == h { on = 1; next }
    on && /^#+ / { n = match($0, /^#+/); if (RLENGTH <= lvl) exit }
    on && match($0, /20[0-9][0-9]-[0-9][0-9]-[0-9][0-9]/) { print substr($0, RSTART, RLENGTH); exit }
  ' README.md
}

# Whole days between a YYYY-MM-DD and today (UTC).
readme_days_since() {
  local now
  now=$(readme_to_epoch "$(date -u +%Y-%m-%d)")
  echo $(( (now - $(readme_to_epoch "$1")) / 86400 ))
}

# The MSR scorecard's reading, anchored to the CITATION rather than to the
# section.
#
# Every line that cites an almide-dojo run must carry that run's own
# YYYY-MM-DD, and every cited run must be inside the line. Anchoring on the
# section instead would let a DIFFERENT measurement standing in the same
# section supply the date: drop the MSR date from `### LLM writability` and
# the section still holds the MiniGit bench's date, so the gate would go on
# reading a number that is no longer the one it is aging. Tying the date to
# the sha also means a re-measurement cannot land as a date edit — the sha
# has to move with it.
#
# Every cited run is aged, not just the newest, so a fresh row cannot carry a
# fossil alongside it.
readme_msr_report() {
  local fail=0 n=0 hit lineno text d age
  while IFS= read -r hit; do
    n=$((n + 1))
    lineno=${hit%%:*}
    text=${hit#*:}
    d=$(printf '%s' "$text" | grep -oE '20[0-9][0-9]-[0-9][0-9]-[0-9][0-9]' | head -1 || true)
    if [ -z "$d" ]; then
      echo "::error::README.md:$lineno: this line cites an almide-dojo run but carries no YYYY-MM-DD — the run and the date it was measured belong on the same line, or the scorecard's age gets read off some other measurement's date"
      fail=1
      continue
    fi
    age=$(readme_days_since "$d")
    if [ "$age" -gt "$README_FRESHNESS_MAX_DAYS" ]; then
      echo "::error::README.md:$lineno: the MSR scorecard cites a run measured $d — $age days ago, above the $README_FRESHNESS_MAX_DAYS-day line. Re-measure (almide-dojo owns it, #1617: 'make msr' or its Cross-language MSR Run workflow, neither of which needs an Anthropic key) and replace the table, the date and the sha together; do not edit the date alone."
      fail=1
    else
      echo "readme-numbers: MSR scorecard (README.md:$lineno) measured $d, $age days ago, line $README_FRESHNESS_MAX_DAYS"
    fi
  done < <(grep -nE 'almide-dojo@[0-9a-f]{7,40}' README.md || true)
  if [ "$n" -eq 0 ]; then
    echo "::error::README.md: no line cites an almide-dojo@<sha> run — the LLM-writability scorecard must name the committed run it came from (#1617, #2146 item 4), so a reader can recompute it from that repo's runs/ instead of believing it"
    fail=1
  fi
  return "$fail"
}

# Age every measured section. Prints one ::error:: per offender and one
# informational line per fresh section; returns 1 when anything is stale or
# undated. The caller decides what a non-zero return means.
readme_freshness_report() {
  local fail=0 heading level d age
  for heading in "${README_MEASURED_SECTIONS[@]}"; do
    level=$(printf '%s' "$heading" | sed -E 's/^(#+).*/\1/' | wc -c | tr -d ' ')
    level=$((level - 1))
    d=$(readme_section_date "$heading" "$level")
    if [ -z "$d" ]; then
      echo "::error::README.md: section '$heading' carries no YYYY-MM-DD measurement date (check-readme-numbers.sh should have refused this)"
      fail=1
      continue
    fi
    age=$(readme_days_since "$d")
    if [ "$age" -gt "$README_FRESHNESS_MAX_DAYS" ]; then
      echo "::error::README.md: '$heading' was measured on $d — $age days ago, above the $README_FRESHNESS_MAX_DAYS-day line. Re-run the measurement (almide-dojo owns it: #1617) and replace the table and its date; do not edit the date alone."
      fail=1
    else
      echo "readme-freshness: '$heading' measured $d ($age days ago, line $README_FRESHNESS_MAX_DAYS)"
    fi
  done
  return "$fail"
}
