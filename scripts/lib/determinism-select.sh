# Which fixtures a determinism gate compares on this run.
#
# Full corpus when DETERMINISM_STRIDE is unset or 1: CI sets that for a PR into
# main and a push to main, the releases. Otherwise every DETERMINISM_STRIDE-th
# fixture, with the starting offset taken from DETERMINISM_SEED (the commit
# sha), so consecutive commits compare different slices, plus every fixture
# the change itself touches (DETERMINISM_BASE..HEAD, when git can see it).
#
# A host-dependent codegen path (HashMap order, usize width) changes many
# fixtures at once, so a slice still catches it on the PR that introduces it;
# the release PR into main compares everything.
#
# Usage: determinism_select <fixture-dir> <out-file>; writes one path per line.
determinism_select() {
  local dir="$1" out="$2" stride="${DETERMINISM_STRIDE:-1}" seed="${DETERMINISM_SEED:-0}"
  ls "$dir"/*.almd 2>/dev/null | LC_ALL=C sort > "$out.all"
  if [ "$stride" -le 1 ] 2>/dev/null; then
    mv "$out.all" "$out"
    return
  fi
  # The seed's first 7 hex digits pick the offset.
  local off=$(( 16#$(printf '%s' "$seed" | tr -cd '0-9a-fA-F' | cut -c1-7 | sed 's/^$/0/') % stride ))
  awk -v s="$stride" -v o="$off" '(NR - 1) % s == o' "$out.all" > "$out"
  if [ -n "${DETERMINISM_BASE:-}" ]; then
    git diff --name-only "$DETERMINISM_BASE"...HEAD -- "$dir" 2>/dev/null \
      | /usr/bin/grep -E '\.almd$' | while read -r f; do [ -e "$f" ] && echo "$f"; done >> "$out" || true
  fi
  LC_ALL=C sort -u -o "$out" "$out"
  rm -f "$out.all"
  echo "==> slice: $(wc -l < "$out" | tr -d ' ') of $(ls "$dir"/*.almd | wc -l | tr -d ' ') fixtures (every ${stride}th from offset $off, plus the change's own); the full corpus runs on PRs into main"
}
