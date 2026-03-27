#!/bin/bash
# Auto-generate roadmap/README.md from directory structure.
# Run: bash docs/roadmap/generate-readme.sh > docs/roadmap/README.md

set -euo pipefail
cd "$(dirname "$0")"

extract_title() {
  head -5 "$1" 2>/dev/null | grep '^# ' | head -1 | sed 's/^# //'
}

# First non-empty, non-heading, non-blockquote line (truncated)
extract_desc() {
  python3 -c "
import sys
for line in open('$1'):
    line = line.strip()
    if not line or line.startswith('#') or line.startswith('>') or line.startswith('|') or line.startswith('-') or line.startswith('\`\`\`'):
        continue
    print(line[:100])
    break
" 2>/dev/null || echo ""
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
    local title
    title=$(extract_title "$f")
    [ -z "$title" ] && title=$(basename "$f" .md)
    local desc
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
section "stdlib" "Stdlib"
