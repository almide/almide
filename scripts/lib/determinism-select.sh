# Which fixtures a determinism gate compares on this run — the CI slice
# (scripts/lib/ci-slice.sh) over one fixture directory.
#
# Full corpus when ALMIDE_CI_SLICE is unset or 1: CI sets that for a PR into
# main and every push (develop, main), the releases. Otherwise the commit-
# rotated 1-in-k slice plus every fixture the change itself touches.
#
# A host-dependent codegen path (HashMap order, usize width) changes many
# fixtures at once, so a slice still catches it on the PR that introduces it;
# the release PR into main compares everything.
#
# Usage: determinism_select <fixture-dir> <out-file>; writes one path per line.
. "$(dirname "${BASH_SOURCE[0]}")/ci-slice.sh"
determinism_select() {
  local dir="$1" out="$2"
  ls "$dir"/*.almd 2>/dev/null | LC_ALL=C sort | ci_slice_filter "determinism($dir)" > "$out"
}
