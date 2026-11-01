#!/usr/bin/env bash
# STDLIB TEST COVERAGE — every public stdlib fn is called by a test on BOTH
# legs (native Rust and wasm), or is named in proofs/stdlib-test-coverage.txt
# with the leg it lacks. The verdict is tests/stdlib_test_coverage_test.rs; CI
# runs that target in the test-rust shards, and this script runs the same
# target on its own (`make stdlib-test-coverage`).
#
#   bash scripts/check-stdlib-test-coverage.sh            # the gate
#   REPORT=/path/table.md bash scripts/check-stdlib-test-coverage.sh   # + the per-function table
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
if [ -n "${REPORT:-}" ]; then
  export ALMIDE_STDLIB_COVERAGE_REPORT="$REPORT"
fi
exec cargo test --release --locked --test stdlib_test_coverage_test -- --nocapture
