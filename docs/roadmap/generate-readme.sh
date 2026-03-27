#!/bin/bash
# Auto-generate roadmap/README.md from directory structure.
# Each .md file should have a comment at line 1: <!-- description: ... -->
# Falls back to the first H1 title if no description is found.
# Run: bash docs/roadmap/generate-readme.sh > docs/roadmap/README.md

set -euo pipefail
cd "$(dirname "$0")"

extract_title() {
  head -10 "$1" 2>/dev/null | grep '^# ' | head -1 | sed 's/^# //'
}

extract_desc() {
  head -3 "$1" 2>/dev/null | grep '<!-- description:' | sed 's/.*<!-- description: //; s/ -->//' || echo ""
}

section() {
  local dir="$1" label="$2"
  local files=("$dir"/*.md)
  [ ! -f "${files[0]}" ] && return

  local count=${#files[@]}
  echo "## $label"
  echo ""
  echo "$count items"
  echo ""

  if [ "$dir" = "done" ]; then
    echo "<details>"
    echo "<summary>Show all $count completed items</summary>"
    echo ""
  fi

  echo "| Item | Description |"
  echo "|------|-------------|"
  for f in "${files[@]}"; do
    local title desc
    title=$(extract_title "$f")
    [ -z "$title" ] && title=$(basename "$f" .md)
    desc=$(extract_desc "$f")
    echo "| [$title]($f) | $desc |"
  done
  echo ""

  if [ "$dir" = "done" ]; then
    echo "</details>"
    echo ""
  fi
}

cat << 'HEADER'
# Almide Roadmap

> Auto-generated from directory structure. Run `bash docs/roadmap/generate-readme.sh > docs/roadmap/README.md` to update.
>
> [GRAND_PLAN.md](GRAND_PLAN.md) — 5-phase strategy

HEADER

section "active" "Active"
section "on-hold" "On Hold"
section "done" "Done"
