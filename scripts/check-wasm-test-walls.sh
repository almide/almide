#!/usr/bin/env bash
# WASM TEST-LANE WALL GATE (#2121).
#
# `almide test spec/ --target wasm` reports two different things as a benign
# skip: the `// wasm:skip` markers an author DECLARED (gated by
# tests/wasm_skip_ledger_test.rs) and the files a RENDERER declined. Nothing
# watched the second kind — a wall carries no marker, so it appeared in no
# ledger — and the gate stayed green while five files' tests had not run on
# wasm.
#
# This holds the observed wall set equal to proofs/wasm-test-walls.txt, both
# directions. A new wall cannot join silently, and a row whose file now runs
# must be deleted: the register only shrinks.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

ALMIDE="${ALMIDE:-$REPO_ROOT/target/release/almide}"
REGISTER="$REPO_ROOT/proofs/wasm-test-walls.txt"
SPEC_DIR="${1:-spec/}"

if [ ! -x "$ALMIDE" ]; then
  echo "::error::check-wasm-test-walls: no almide binary at $ALMIDE (set ALMIDE=...)"
  exit 1
fi
if [ ! -f "$REGISTER" ]; then
  echo "::error::check-wasm-test-walls: missing register $REGISTER"
  exit 1
fi

OUT="$(mktemp)"
trap 'rm -f "$OUT" "$OUT.observed" "$OUT.registered"' EXIT

# The lane prints one `WALL <path> (...)` line per declined file. Its own exit
# code is about test results, not about walls, so it is deliberately not the
# verdict here.
"$ALMIDE" test "$SPEC_DIR" --target wasm > "$OUT" 2>&1
grep '^WALL ' "$OUT" | awk '{print $2}' | sort -u > "$OUT.observed"
grep -v '^#' "$REGISTER" | grep -v '^[[:space:]]*$' | cut -f1 | sort -u > "$OUT.registered"

new="$(comm -23 "$OUT.observed" "$OUT.registered")"
stale="$(comm -13 "$OUT.observed" "$OUT.registered")"
fail=0

if [ -n "$new" ]; then
  echo "::error::check-wasm-test-walls: these files' tests did not run on wasm and are not in the register:"
  echo "$new" | sed 's/^/  /'
  echo "  Fix the wall, or add a row to proofs/wasm-test-walls.txt naming the issue that removes it."
  echo "  A \`// wasm:skip\` marker is NOT the place: that says wasm CANNOT run the file, and its"
  echo "  ledger (tests/wasm_skip_ledger_test.rs) refuses subset debt (#812)."
  fail=1
fi

if [ -n "$stale" ]; then
  echo "::error::check-wasm-test-walls: these register rows name files that now run on wasm:"
  echo "$stale" | sed 's/^/  /'
  echo "  Delete the row — the register only shrinks."
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi

n="$(wc -l < "$OUT.registered" | tr -d ' ')"
echo "wasm test-lane walls: $n registered, none new, none stale ($SPEC_DIR)"
