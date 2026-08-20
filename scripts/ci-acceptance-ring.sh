#!/usr/bin/env bash
# One leg of the acceptance ring (#1533, attack-list A2-2): build and test a
# REAL downstream project against the compiler under test. The E0004 and
# #1501 classes were found by real code, #1526 by a real downstream running
# real input — generated corpora keep missing what production shapes hit, so
# real projects gate every develop push the way the spec suites do.
#
#   scripts/ci-acceptance-ring.sh <project-dir> <min-test-files> [max-native-fallback]
#
# Expects `almide` (the binary under test) on PATH; `almide test` additionally
# needs `wasmtime` for its wasm leg. Asserts, beyond the exit codes:
#
#   1. `almide test` saw at least <min-test-files> files. Without a floor, a
#      truncated checkout (or a test-discovery regression) goes green by
#      running NOTHING — the same skip-as-pass failure mode as #983.
#   2. At most [max-native-fallback] files (default 0) fell back to native.
#      `almide test` runs each file on the wasm target and falls back on a
#      lowering wall — a wall regression therefore PASSES the plain exit
#      code, silently retreating the verified leg. All current ring projects
#      run all-wasm, so fallback is pinned at zero; raise a project's
#      allowance deliberately, in the workflow matrix, never here.
set -euo pipefail

dir=${1:?usage: ci-acceptance-ring.sh <project-dir> <min-test-files> [max-native-fallback]}
min=${2:?usage: ci-acceptance-ring.sh <project-dir> <min-test-files> [max-native-fallback]}
max_fallback=${3:-0}

cd "$dir"
name=$(basename "$PWD")

almide build

# The shipped-artifact leg: `almide test` renders test FILES, not the main
# entry, so a wall in the project's own entry point is invisible to it.
wasm_out=$(mktemp -t ring-XXXXXX.wasm)
trap 'rm -f "$wasm_out"' EXIT
almide build --target wasm -o "$wasm_out"
echo "$name: wasm artifact $(wc -c < "$wasm_out" | tr -d ' ') bytes"

test_log=$(mktemp -t ring-test-XXXXXX.log)
trap 'rm -f "$wasm_out" "$test_log"' EXIT
almide test 2>&1 | tee "$test_log"

# Summary shape: "3 via WASM, 1 via native fallback, 0 failed (of 4 files)"
summary=$(grep -E 'via WASM.*via native fallback.*failed' "$test_log" | tail -1)
if [ -z "$summary" ]; then
  echo "$name: RING FAIL — no test summary line in the output above" >&2
  exit 1
fi
total=$(sed -E 's/.*\(of ([0-9]+) files?\).*/\1/' <<<"$summary")
fallback=$(sed -E 's/.*, ([0-9]+) via native fallback.*/\1/' <<<"$summary")
if [ "$total" -lt "$min" ]; then
  echo "$name: RING FAIL — ran $total test file(s), floor is $min (a shrunken run is not a green run)" >&2
  exit 1
fi
if [ "$fallback" -gt "$max_fallback" ]; then
  echo "$name: RING FAIL — $fallback file(s) fell back to native (allowance $max_fallback); the wasm leg retreated" >&2
  exit 1
fi
echo "$name: ring leg OK — $total file(s), fallback $fallback/$max_fallback"
