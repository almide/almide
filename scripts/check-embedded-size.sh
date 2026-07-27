#!/usr/bin/env bash
# EMBEDDED-PAYLOAD SIZE RATCHET (#878).
#
# The compiler embeds the self-hosted stdlib verbatim: every `include_str!` in
# `crates/almide-mir/src/render_wasm/registry.rs` (the wasm self-host registry)
# and in `crates/almide-types/src/stdlib_info.rs` (the bundled-module sources)
# lands in the binary's DATA section. That section was 2.04MB of the 5.8MB
# size-first playground build — the half of the footprint this repo owns and
# can watch. The 6x growth over four months went unnoticed because nothing
# watched it; this is the watch.
#
# What it checks: the total bytes of the DISTINCT stdlib sources reachable
# through those two embed sites, against `scripts/embedded-size-baseline.txt`.
# Growth beyond the budget fails. Adding stdlib is normal and expected — the
# gate is not "never grow", it is "grow on purpose": raise the baseline in the
# SAME change, so the number is a reviewed decision instead of a drift.
#
# The other half (the 3.76MB code section) is a twiggy audit + feature gating,
# tracked in #878; it needs the wasm build, which lives in the playground repo.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

BASELINE_FILE="scripts/embedded-size-baseline.txt"
# Percent the total may exceed the baseline before this fails.
BUDGET_PCT="${EMBEDDED_SIZE_BUDGET_PCT:-10}"

sources() {
  # Distinct stdlib stems named by either embed site (`SRC_<STEM>`, the consts
  # `crates/almide-types/build.rs` generates). `sort -u` makes the total a real
  # footprint rather than a sum with duplicates.
  {
    grep -o 'SRC_[A-Z0-9_]*' crates/almide-mir/src/render_wasm/registry.rs
    grep -o 'SRC_[A-Z0-9_]*' crates/almide-types/src/stdlib_info.rs
  } | sort -u | sed 's/^SRC_//' | tr 'A-Z' 'a-z' | sed 's|^|stdlib/|; s|$|.almd|'
}

# Count what SHIPS, not what the repo holds: the embed blanks whole-line
# comments (#878), so a comment costs zero embedded bytes even though it stays
# in the file. Measuring the repo bytes instead would make the gate fire on
# documentation, which is exactly the growth we want to keep encouraging.
total=0
count=0
while read -r f; do
  [ -f "$f" ] || continue
  n=$(awk '{ if ($0 ~ /^[[:space:]]*\/\//) print ""; else print }' "$f" | wc -c | tr -d ' ')
  total=$((total + n))
  count=$((count + 1))
done < <(sources)

if [ ! -f "$BASELINE_FILE" ]; then
  echo "embedded-size: no baseline; writing $BASELINE_FILE ($total bytes over $count sources)"
  printf '%s\n' "$total" > "$BASELINE_FILE"
  exit 0
fi

baseline=$(tr -dc '0-9' < "$BASELINE_FILE")
ceiling=$(( baseline + baseline * BUDGET_PCT / 100 ))

printf 'embedded-size: %s bytes over %s stdlib sources (baseline %s, ceiling %s, +%s%%)\n' \
  "$total" "$count" "$baseline" "$ceiling" "$BUDGET_PCT"

# The five biggest sources, so a failure names where to look.
if [ "$total" -gt "$ceiling" ]; then
  echo "  largest embedded sources:"
  while read -r f; do
    [ -f "$f" ] || continue
    printf '%8s  %s\n' "$(wc -c < "$f" | tr -d ' ')" "$f"
  done < <(sources) | sort -rn | head -5
  echo "::error::embedded stdlib payload $total bytes exceeds the $ceiling ceiling."
  echo "Every embedded byte ships to every embedder of the compiler (the playground"
  echo "downloads it on first visit). Growing it is fine — do it on purpose:"
  echo "  1. confirm the new sources belong in the BUNDLED set (a module only the"
  echo "     CLI needs does not have to be embedded), then"
  echo "  2. raise $BASELINE_FILE to $total in the SAME change."
  exit 1
fi
