#!/usr/bin/env bash
# Shared input to all three native perf gates. Migration candidates are
# measured before promotion to the verified floor; listing them here does
# not certify them or change any loss/coverage threshold.
native_v1_perf_programs() {
  {
    if [ "${1:-}" != "--shapes-only" ]; then
      grep -E '^spec/' proofs/native-verified-floor.txt
      grep -E '^spec/' proofs/native-v1-perf-candidates.txt
    fi
    ls research/benchmark/perf/v1-shapes/*.almd
  } | sort -u
}
