#!/usr/bin/env bash
# gen-claims.sh — regenerate the machine-derived claims block in README.md
# from docs/contracts/contracts.toml (#766).
#
# The README's "Equivalence Claim" section quotes ledger numbers (contract
# count, active/flagged split) and the exceptions clause (the list of
# flagged-for-revision contracts). Those must be DERIVED, never hand-written:
# this script rewrites everything between the claims markers, and
# scripts/check-contracts.sh runs `--check` in CI so a drifted block is red.
#
#   bash scripts/gen-claims.sh          # rewrite README.md in place
#   bash scripts/gen-claims.sh --check  # exit 1 if the block is stale
#
# Pure shell/awk, no deps — same discipline as docs/contracts/generate-readme.sh.
set -euo pipefail
cd "$(dirname "$0")/.." || exit 2

LEDGER="docs/contracts/contracts.toml"
README="README.md"
START="<!-- claims:generated:start — derived from docs/contracts/contracts.toml by scripts/gen-claims.sh; DO NOT EDIT between the markers -->"
END="<!-- claims:generated:end -->"

[ -f "$LEDGER" ] || { echo "::error::$LEDGER not found (run from repo root)"; exit 2; }
[ -f "$README" ] || { echo "::error::$README not found (run from repo root)"; exit 2; }
grep -qxF "$START" "$README" || { echo "::error::claims start marker missing from $README"; exit 2; }
grep -qxF "$END" "$README"   || { echo "::error::claims end marker missing from $README"; exit 2; }

# Walk the ledger with the same block parser as generate-readme.sh: line-start
# anchors skip schema comments, the ''' toggle skips multi-line statements.
blockfile="$(mktemp)"
trap 'rm -f "$blockfile"' EXIT
awk '
  function flush() {
    if (id == "") return
    total++
    if (status == "active") active++
    else { nflag++; fid[nflag] = id; ftitle[nflag] = title; fdoc[nflag] = doc }
    id = ""; title = ""; status = ""; doc = ""
  }
  /'"'"''"'"''"'"'/ { in_stmt = !in_stmt; next }
  in_stmt { next }
  /^\[\[contract\]\]/ { flush(); next }
  /^id[ \t]*=/     { v = $0; sub(/^id[ \t]*=[ \t]*"/, "", v); sub(/".*$/, "", v); id = v; next }
  /^title[ \t]*=/  { v = $0; sub(/^title[ \t]*=[ \t]*"/, "", v); sub(/".*$/, "", v); title = v; next }
  /^status[ \t]*=/ { v = $0; sub(/^status[ \t]*=[ \t]*"/, "", v); sub(/".*$/, "", v); status = v; next }
  /^doc[ \t]*=/    { v = $0; sub(/^doc[ \t]*=[ \t]*"/, "", v); sub(/".*$/, "", v); doc = v; next }
  END {
    flush()
    printf "> **Ledger: %d contracts — %d active, %d flagged-for-revision.**\n", total, active, nflag
    print ">"
    if (nflag == 0) {
      print "> **Exceptions: none.** Every contract in the ledger is `active`, carrying"
      print "> executable evidence of class ≥ `fixture`."
    } else {
      printf "> **Exceptions (%d)** — contracts flagged for revision; the ratchet says this list may only shrink:\n", nflag
      print ">"
      for (k = 1; k <= nflag; k++) {
        link = (fdoc[k] != "") ? "docs/contracts/" fdoc[k] : "docs/contracts/contracts.toml"
        printf "> - [%s — %s](%s)\n", fid[k], ftitle[k], link
      }
    }
  }
' "$LEDGER" > "$blockfile"

rendered="$(awk -v start="$START" -v end="$END" -v bf="$blockfile" '
  $0 == start { print; while ((getline line < bf) > 0) print line; skip = 1; next }
  $0 == end   { skip = 0; print; next }
  !skip { print }
' "$README")"

if [ "${1:-}" = "--check" ]; then
  if [ "$rendered" != "$(cat "$README")" ]; then
    echo "::error::README.md claims block is stale — run: bash scripts/gen-claims.sh"
    exit 1
  fi
  exit 0
fi

printf '%s\n' "$rendered" > "$README"
