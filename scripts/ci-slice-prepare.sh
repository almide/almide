#!/usr/bin/env bash
# Resolve the CI slice's "change's own files" once per job (scripts/lib/ci-slice.sh).
#
# Under ALMIDE_CI_SLICE=k > 1: fetch ALMIDE_CI_BASE when the shallow checkout
# lacks it, write `git diff --name-only $ALMIDE_CI_BASE HEAD` to a file and
# export ALMIDE_CI_TOUCHED (through $GITHUB_ENV) so every later step — the
# Rust test binaries included — reads the same list without touching git.
# A full run does nothing. A slice whose base cannot be resolved fails here,
# loudly, rather than judging a slice without the change's own fixtures.
set -euo pipefail
cd "$(dirname "$0")/.."
. scripts/lib/ci-slice.sh
if ! ci_slice_active; then
  echo "CI slice: full run (ALMIDE_CI_SLICE=${ALMIDE_CI_SLICE:-unset}) — every gate judges every fixture"
  exit 0
fi
out="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/almide-ci-touched.txt"
[ -n "${ALMIDE_CI_BASE:-}" ] || { echo "::error::ALMIDE_CI_SLICE=$ALMIDE_CI_SLICE without ALMIDE_CI_BASE"; exit 1; }
unset ALMIDE_CI_TOUCHED
ci_slice_touched "$out"
echo "CI slice 1/$(ci_slice_k) (offset $(ci_slice_offset), seed ${ALMIDE_CI_SEED:-none}); the change touches $(grep -c . "$out" || true) file(s):"
sed 's/^/  /' "$out" | head -200
[ -z "${GITHUB_ENV:-}" ] || echo "ALMIDE_CI_TOUCHED=$out" >> "$GITHUB_ENV"
