#!/usr/bin/env bash
# TOR reference gate: every `enforced-by:` instrument named in proofs/TOR.md
# must exist. A TOR row pointing at a deleted or renamed instrument is an
# operational requirement with no enforcement — exactly the silent rot this
# file exists to prevent (the #1176 stale-pointer class, applied to the
# operational contract).
set -euo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TOR="$ROOT/proofs/TOR.md"

fail=0
count=0
while IFS= read -r ref; do
  count=$((count + 1))
  if [ ! -e "$ROOT/$ref" ]; then
    echo "  TOR names enforced-by: $ref — which does not exist" >&2
    fail=1
  fi
done < <(sed -n 's/^enforced-by: *//p' "$TOR")

if [ "$count" -eq 0 ]; then
  echo "TOR REFS FAIL — no enforced-by rows found (the parse anchor moved?)" >&2
  exit 1
fi
if [ $fail -ne 0 ]; then
  echo "TOR REFS FAIL — an operational requirement lost its enforcing instrument." >&2
  exit 1
fi
echo "tor-refs OK: $count enforced-by instrument(s) all resolve"
