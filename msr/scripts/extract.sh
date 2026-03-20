#!/bin/bash
# msr/scripts/extract.sh — Extract exercise prompts for MSR measurement
#
# For each exercise, creates a prompt file with:
#   - Type definitions (kept as-is)
#   - Function signatures with body replaced by `todo`
#   - All test blocks (kept as-is)
#
# Usage: ./msr/scripts/extract.sh

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

PROMPT_DIR="msr/prompts"
mkdir -p "$PROMPT_DIR"

count=0
for ex_dir in exercises/*/; do
  name=$(basename "$ex_dir")
  src=$(find "$ex_dir" -name '*.almd' | head -1)
  [ -z "$src" ] && continue

  outfile="$PROMPT_DIR/${name}.almd"

  python3 -c "
import re, sys

with open('$src') as f:
    lines = f.readlines()

output = []
i = 0
while i < len(lines):
    line = lines[i]

    # Keep test blocks as-is
    if line.startswith('test '):
        # Collect entire test block
        output.append(line)
        if '{' in line and '}' in line:
            i += 1
            continue
        # Multi-line test
        depth = line.count('{') - line.count('}')
        i += 1
        while i < len(lines) and depth > 0:
            output.append(lines[i])
            depth += lines[i].count('{') - lines[i].count('}')
            i += 1
        continue

    # Keep type declarations as-is
    if line.startswith('type '):
        output.append(line)
        # Multi-line variant/record
        i += 1
        while i < len(lines) and (lines[i].startswith('  |') or lines[i].startswith('  ') and not lines[i].strip().startswith('fn') and not lines[i].strip().startswith('test')):
            output.append(lines[i])
            i += 1
        continue

    # Replace function bodies with todo
    fn_match = re.match(r'^((?:effect )?fn \S+\(.*?\)\s*->\s*.+?)\s*=\s', line)
    if fn_match:
        sig = fn_match.group(1)
        output.append(sig + ' = todo\n')
        # Skip body
        if '= {' in line:
            depth = line.count('{') - line.count('}')
            i += 1
            while i < len(lines) and depth > 0:
                depth += lines[i].count('{') - lines[i].count('}')
                i += 1
            continue
        i += 1
        continue

    # Keep imports, comments, blank lines
    if line.startswith('import ') or line.startswith('//') or line.strip() == '':
        output.append(line)

    i += 1

with open('$outfile', 'w') as f:
    f.writelines(output)
" 2>&1

  count=$((count + 1))
  echo "Extracted: $name"
done

echo ""
echo "Total: $count exercises extracted to $PROMPT_DIR/"
