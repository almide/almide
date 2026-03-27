#!/bin/bash
# Auto-generate roadmap/README.md from directory structure.
# Each .md file should have:
#   Line 1: <!-- description: ... -->
#   (done/ only) Line 2: <!-- done: YYYY-MM-DD -->
# Run: bash docs/roadmap/generate-readme.sh > docs/roadmap/README.md

set -euo pipefail
cd "$(dirname "$0")"

extract_title() {
  head -10 "$1" 2>/dev/null | grep '^# ' | head -1 | sed 's/^# //'
}

extract_desc() {
  head -3 "$1" 2>/dev/null | grep '<!-- description:' | sed 's/.*<!-- description: //; s/ -->//' || echo ""
}

extract_done_date() {
  head -5 "$1" 2>/dev/null | grep '<!-- done:' | sed 's/.*<!-- done: //; s/ -->//' || echo ""
}

cat << 'HEADER'
# Almide Roadmap

> Auto-generated from directory structure. Run `bash docs/roadmap/generate-readme.sh > docs/roadmap/README.md` to update.
>
> [GRAND_PLAN.md](GRAND_PLAN.md) — 5-phase strategy

HEADER

# Active
files=(active/*.md)
if [ -f "${files[0]}" ]; then
  echo "## Active"
  echo ""
  echo "${#files[@]} items"
  echo ""
  echo "| Item | Description |"
  echo "|------|-------------|"
  for f in "${files[@]}"; do
    title=$(extract_title "$f")
    [ -z "$title" ] && title=$(basename "$f" .md)
    desc=$(extract_desc "$f")
    echo "| [$title]($f) | $desc |"
  done
  echo ""
fi

# On Hold
files=(on-hold/*.md)
if [ -f "${files[0]}" ]; then
  echo "## On Hold"
  echo ""
  echo "${#files[@]} items"
  echo ""
  echo "| Item | Description |"
  echo "|------|-------------|"
  for f in "${files[@]}"; do
    title=$(extract_title "$f")
    [ -z "$title" ] && title=$(basename "$f" .md)
    desc=$(extract_desc "$f")
    echo "| [$title]($f) | $desc |"
  done
  echo ""
fi

# Done — sorted by date (newest first)
files=(done/*.md)
if [ -f "${files[0]}" ]; then
  count=${#files[@]}
  echo "## Done"
  echo ""
  echo "$count items"
  echo ""
  echo "<details>"
  echo "<summary>Show all $count completed items</summary>"
  echo ""
  echo "| Done | Item | Description |"
  echo "|------|------|-------------|"

  # Collect and sort by date
  entries=""
  for f in "${files[@]}"; do
    title=$(extract_title "$f")
    [ -z "$title" ] && title=$(basename "$f" .md)
    desc=$(extract_desc "$f")
    date=$(extract_done_date "$f")
    entries+="${date}|[$title]($f)|$desc"$'\n'
  done

  echo "$entries" | sort -t'|' -k1 -r | while IFS='|' read -r date link desc; do
    [ -z "$date" ] && continue
    echo "| $date | $link | $desc |"
  done

  echo ""
  echo "</details>"
  echo ""
fi
