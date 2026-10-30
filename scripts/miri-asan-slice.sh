#!/usr/bin/env bash
# MIRI AND ADDRESSSANITIZER ON EVERY PR AND PUSH, AS A ROTATING SLICE (#4009)
# ==========================================================================
#
# .github/workflows/miri-nightly.yml runs every spec/wasm_cross program's
# generated crate under Miri and under ASan once a night, ~80 and ~25 runner
# minutes. A change that brings undefined behaviour in is then found the
# morning after, by whoever reads the issue. This runs a 1-in-k slice of the
# same corpus in CI (owner ruling 2026-10-30: a slice on every PR and push, the
# full run stays nightly, ~10 minutes of PR time):
#
#   bash scripts/miri-asan-slice.sh miri|asan
#
# The slice is scripts/lib/ci-slice.sh's — a fixture is in when
# (hash(path) + offset) % k == 0, plus every fixture the change touches — with
# the offset taken from CI's run counter instead of the commit hash, so the
# rotation is a schedule and not a lottery: consecutive runs take offsets
# n, n+1, ..., and every program is judged at least once in any k consecutive
# runs of the workflow (PRs and pushes share the counter).
#
# MIRI_ASAN_ROTATION  the run counter (CI: github.run_number; default 0)
# MIRI_SLICE_K        Miri's k (default 12)
# ASAN_SLICE_K        ASan's k (default 4)
# ALMIDE_CI_BASE      the base the change is diffed against (its touched fixtures
#                     join the slice; under CI a slice without it is refused)
#
# Every slice also runs the runtime-unsafe boundary program
# (scripts/lib/unsafe-boundary-asan.almd, the edges of each listed `unsafe` in
# proofs/runtime-unsafe-allowlist.toml), and ASan's the sys.rs foreign-call
# program beside it, so a change to the runtime's unsafe is judged on the PR
# that makes it, whatever the rotation. The verdicts are the full scripts'
# (scripts/miri-generated.sh, scripts/asan-generated.sh), controls included:
# a UB / ASAN verdict fails the job, as it fails the nightly.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
tool="${1:-}"
case "$tool" in
  miri) k="${MIRI_SLICE_K:-12}"; runner=scripts/miri-generated.sh
        always=(scripts/lib/unsafe-boundary-asan.almd) ;;
  asan) k="${ASAN_SLICE_K:-4}"; runner=scripts/asan-generated.sh
        always=(scripts/lib/unsafe-boundary-asan.almd scripts/lib/sys-ffi-asan.almd) ;;
  *) echo "usage: $0 miri|asan" >&2; exit 2 ;;
esac
rot="${MIRI_ASAN_ROTATION:-0}"
case "$rot" in ''|*[!0-9]*) echo "::error::miri-asan-slice: MIRI_ASAN_ROTATION=$rot is not a run counter" >&2; exit 2 ;; esac

. scripts/lib/ci-slice.sh
# ci_slice_offset reads the seed's leading hex digits, so the counter in hex
# makes the offset rot % k (a counter below 16^7; reduce it first so it stays one).
seed="$(printf '%x' $(( rot % (k * 1000000) )))"
list="$(mktemp)"; trap 'rm -f "$list"' EXIT
ls spec/wasm_cross/*.almd | ALMIDE_CI_SLICE="$k" ALMIDE_CI_SEED="$seed" ci_slice_filter "$tool-slice" > "$list" || {
  echo "::error::miri-asan-slice: the slice could not be selected (see above)"; exit 2; }
for f in "${always[@]}"; do
  [ -f "$f" ] || { echo "::error::miri-asan-slice: $f is gone — the slice's fixed program must exist"; exit 2; }
  echo "$f" >> "$list"
done
echo "$tool-slice: rotation $rot, offset $(( rot % k )) of $k; $(grep -c . "$list") program(s), the fixed ones: ${always[*]}"
xargs bash "$runner" < "$list"
