#!/usr/bin/env bash
# Confluence gate for docs/roadmap/active/logical-time-proofs.md.
#
# Re-verifies, from scratch, that over the exhaustive small scope the four
# artifacts agree exactly (outcome + consumed fuel + occurred charge stream):
#   REF  the merge-order decisive-event semantics
#   SEQ  the sequential list-order scan with shrinking caps + deferred traps
#   ADV  EVERY physical schedule, with pruning at the cap rule AND lazy-cap
#        overruns (confluence: the reachable-outcome set must be a singleton)
#   and the nested-bounded streaming arithmetic on the occurred stream.
#
# Usage: research/spike/logical-time-race/run-gate.sh
set -euo pipefail
cd "$(dirname "$0")"

cargo run --release --quiet
