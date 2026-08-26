#!/usr/bin/env bash
# The fmt gate (#919): spec/ + examples/ stay formatted. The DDD gauntlet's
# REJECT cells are deliberately-invalid programs (their front rejection is
# pinned by the gauntlet manifest), so `almide fmt --check` cannot parse
# them and would fail the gate on files whose brokenness is the point. The
# manifest's reject rows are the SINGLE exclusion source — the same ruling
# the fmt-corpus test applies (crates/almide-tools/tests/fmt_corpus_test.rs).
set -euo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."

BIN="${ALMIDE_BIN:-}"
if [ -z "$BIN" ]; then
  if [ -x "target/release/almide" ]; then BIN="target/release/almide"; else BIN="almide"; fi
fi

MANIFEST="crates/almide-wasm/tests/golden/gauntlet-manifest.txt"
rejects=$(grep $'^reject\t' "$MANIFEST" | awk -F'\t' '{print $3}')

# Every .almd under the gate's roots, minus the manifest's reject cells
# (a dir cell's row names the cell dir; all of its files skip).
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
list="$work/files"
find spec examples -name '*.almd' > "$work/all"
sort "$work/all" -o "$work/all"
: > "$list"
while IFS= read -r f; do
  skip=0
  for r in $rejects; do
    case "$f" in
      "spec/gauntlet/$r"|"spec/gauntlet/$r"/*) skip=1; break ;;
    esac
  done
  [ "$skip" = 0 ] && echo "$f" >> "$list"
done < "$work/all"

# xargs batches keep the arg list within limits; any drift fails the gate.
xargs "$BIN" fmt --check < "$list"
