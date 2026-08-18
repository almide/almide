#!/usr/bin/env bash
# LLM-SURFACE GATE (#1483, the floor form)
# ========================================
#
# Every code fence LABELED ```almide in the LLM-facing docs must pass
# `almide check`. The LLM surface (llms.txt, docs/CHEATSHEET.md) is the
# de-facto stable surface — models generalize from exactly these snippets —
# and an example that no longer compiles teaches a false move on every
# generation (the `regex.captures` drift class). This gate makes a labeled
# example that rots a CI failure instead of a training corpus bug.
#
# CONTRACT. The ```almide check marker (the fence info-string's second
# word — highlighting still reads the first) is the author's opt-in: it
# asserts "this fence is a complete, checkable program". Plain ```almide
# stays a highlighting label for fragments, deliberate ✗-examples and
# idiom halves — out of scope on purpose; the gate checks the promise, not
# the prose. Growing coverage means completing an example and promoting its
# marker, one fence at a time (#1483 tracks the arc). The allowlist below is
# SHRINK-ONLY: a day-one failure may be listed with its reason, a new
# failure may not be added. First run already paid for the gate: the
# CHEATSHEET's own "Complete example" used process.args() without
# `import process`.
#
# Usage: check-llm-surface.sh          (uses target/release/almide or PATH)
set -euo pipefail
export LC_ALL=C

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${ALMIDE_BIN:-}"
if [ -z "$BIN" ]; then
  if [ -x "$ROOT/target/release/almide" ]; then BIN="$ROOT/target/release/almide"; else BIN="almide"; fi
fi

FILES=("$ROOT/llms.txt" "$ROOT/docs/CHEATSHEET.md")

# Shrink-only allowlist: "<file-basename>:<fence-index>  <reason>".
# (Empty at landing — every labeled fence checks clean.)
ALLOW=""

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

fail=0
total=0
for f in "${FILES[@]}"; do
  base=$(basename "$f")
  # Extract each ```almide fence into its own numbered file.
  awk -v out="$TMP/$base" '
    /^```almide check[ \t]*$/ { on=1; n++; next }
    /^```/              { on=0; next }
    on                  { print > (out "." n ".almd") }
  ' "$f"
  for snip in "$TMP/$base".*.almd; do
    [ -e "$snip" ] || continue
    total=$((total + 1))
    idx="${snip##*.almd}"; idx="${snip%.almd}"; idx="${idx##*.}"
    key="$base:$idx"
    if ! "$BIN" check "$snip" > "$TMP/out.txt" 2>&1; then
      if printf '%s\n' "$ALLOW" | grep -q "^$key "; then
        echo "allowlisted (day-one): $key"
        continue
      fi
      echo "::error::llm-surface: $key — a \`\`\`almide fence no longer compiles:"
      sed 's/^/    /' "$TMP/out.txt" | head -8
      echo "    (fence source: $snip)"
      fail=1
    fi
  done
done

echo "llm-surface: $total labeled fence(s) checked across ${#FILES[@]} file(s)"
[ "$total" -gt 0 ] || { echo "::error::llm-surface: zero labeled fences found — the extractor or the labels moved (#976 class)"; exit 1; }
exit $fail
