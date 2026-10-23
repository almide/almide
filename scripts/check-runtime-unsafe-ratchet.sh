#!/usr/bin/env bash
# RUNTIME UNSAFE RATCHET (#4007)
# ==============================
#
# Almide's memory-safety claim stops at every `unsafe` in the native runtime:
# runtime/rs is concatenated into the emitted crate as text, so a crate-level
# `#![forbid(unsafe_code)]` cannot be placed there. This gate does the job
# instead: every file's count of `unsafe` code lines must match
# scripts/runtime-unsafe-allowlist.txt exactly.
#
#   a file over its count, or not listed  -> FAIL (a new unsafe site needs an
#                                            allowlist line with its reason)
#   a file under its count                -> FAIL as STALE (lower the count)
#
# RUNTIME_UNSAFE_ROOT points the gate at another tree (the negative control).
set -euo pipefail
export LC_ALL=C

ROOT="${RUNTIME_UNSAFE_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
ALLOW="$ROOT/scripts/runtime-unsafe-allowlist.txt"

count_file() {
  { grep -E '(^|[^A-Za-z0-9_])unsafe([^A-Za-z0-9_]|$)' "$1" || true; } \
    | { grep -vE '^[[:space:]]*//' || true; } | wc -l | tr -d ' '
}

allowed_for() {
  awk -v f="$1" '!/^#/ && NF >= 2 && $2 == f { print $1; found = 1 } END { if (!found) print 0 }' "$ALLOW"
}

fail=0
total=0
for path in "$ROOT"/runtime/rs/src/*.rs; do
  rel="${path#"$ROOT"/}"
  have=$(count_file "$path")
  want=$(allowed_for "$rel")
  total=$((total + have))
  if [ "$have" -gt "$want" ]; then
    echo "::error::runtime-unsafe-ratchet: $rel has $have unsafe line(s), allowlist says $want — a new unsafe site needs a line in scripts/runtime-unsafe-allowlist.txt with its reason (#4007)."
    fail=1
  elif [ "$have" -lt "$want" ]; then
    echo "::error::runtime-unsafe-ratchet: $rel has $have unsafe line(s), allowlist says $want — STALE; lower it to $have in this PR (shrink-only, the good direction)."
    fail=1
  fi
done

# A listed file that no longer exists is stale too.
while read -r _ rel _; do
  [ -f "$ROOT/$rel" ] || { echo "::error::runtime-unsafe-ratchet: allowlist names $rel, which does not exist — delete the line."; fail=1; }
done < <(grep -vE '^(#|[[:space:]]*$)' "$ALLOW")

[ "$fail" = 0 ] || exit 1
echo "runtime-unsafe-ratchet: $total unsafe line(s) in runtime/rs, allowlist held"
