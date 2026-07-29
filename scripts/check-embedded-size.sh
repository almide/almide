#!/usr/bin/env bash
# EMBEDDED-PAYLOAD SIZE RATCHET (#878).
#
# The compiler embeds the self-hosted stdlib verbatim: every `SRC_<STEM>`
# const `crates/almide-types/build.rs` generates (referenced from
# `stdlib_info.rs`'s bundled sources and `self_host_registry.rs`'s wasm
# self-host registry) plus every direct `include_str!(".../stdlib/*.almd")`
# lands in the binary's DATA section. That section was 2.04MB of the 5.8MB
# size-first playground build — the half of the footprint this repo owns and
# can watch. The 6x growth over four months went unnoticed because nothing
# watched it; this is the watch.
#
# What it checks: the total bytes of the DISTINCT stdlib sources reachable
# through those embed sites, against `scripts/embedded-size-baseline.txt`.
# Growth beyond the budget fails. Adding stdlib is normal and expected — the
# gate is not "never grow", it is "grow on purpose": raise the baseline in the
# SAME change, so the number is a reviewed decision instead of a drift.
#
# The embed sites are DISCOVERED by pattern over crates/, not named by path:
# the previous version grepped a hard-coded file that was later deleted in a
# reorganization, its grep error was swallowed, and the gate spent its life
# measuring "0 bytes over 0 sources" — green forever (#976). Discovery plus
# the hard floor below make that failure mode loud instead of silent.
#
# The other half (the 3.76MB code section) is a twiggy audit + feature gating,
# tracked in #878; it needs the wasm build, which lives in the playground repo.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

BASELINE_FILE="scripts/embedded-size-baseline.txt"
# Percent the total may exceed the baseline before this fails.
BUDGET_PCT="${EMBEDDED_SIZE_BUDGET_PCT:-10}"
# The scan must find at least this many sources or it has gone blind (#976):
# the bundled list alone names ~40 modules, so a result under the floor means
# the discovery pattern no longer matches the embed sites, not a small stdlib.
SOURCE_FLOOR=30

sources() {
  # Distinct stdlib stems named by any embed site: `SRC_<STEM>` references
  # anywhere in almide-types (build.rs generates one per stdlib/<stem>.almd),
  # plus direct `include_str!` of a stdlib file anywhere in crates/. `sort -u`
  # makes the total a real footprint rather than a sum with duplicates.
  {
    grep -rho 'SRC_[A-Z0-9_]*' crates/almide-types/src/ \
      | sed 's/^SRC_//' | tr 'A-Z' 'a-z' | sed 's|^|stdlib/|; s|$|.almd|'
    grep -rho 'include_str!("[^"]*stdlib/[^"]*\.almd")' crates/ \
      | sed 's|.*stdlib/|stdlib/|; s|")$||'
  } | sort -u
}

# Count what SHIPS, not what the repo holds: the embed blanks whole-line
# comments (#878), so a comment costs zero embedded bytes even though it stays
# in the file. Measuring the repo bytes instead would make the gate fire on
# documentation, which is exactly the growth we want to keep encouraging.
total=0
count=0
while read -r f; do
  if [ ! -f "$f" ]; then
    echo "::error::embedded-size: $f is named by an embed site but does not exist —"
    echo "a SRC_ const or include_str! references a deleted stdlib source. Fix the"
    echo "reference (or restore the file); the gate does not skip what it cannot see."
    exit 1
  fi
  n=$(awk '{ if ($0 ~ /^[[:space:]]*\/\//) print ""; else print }' "$f" | wc -c | tr -d ' ')
  total=$((total + n))
  count=$((count + 1))
done < <(sources)

if [ "$count" -lt "$SOURCE_FLOOR" ]; then
  echo "::error::embedded-size: only $count embedded sources discovered (floor $SOURCE_FLOOR)"
  echo "— the scan went blind (#976): the embed sites moved and the discovery pattern"
  echo "in sources() no longer matches them. Point it at the current registry; do not"
  echo "lower the floor to make this pass."
  exit 1
fi

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
