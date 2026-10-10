#!/usr/bin/env bash
# NIGHTLY FUZZ FINDINGS AGGREGATE (#3518)
# =======================================
#
# Gathers every shard's findings into one directory and prints the night's
# counts as `key=value` lines for $GITHUB_OUTPUT.
#
# Findings are counted by directory NAME: the fuzzer names each finding after
# its kind + minimized summary, so two shards that trip the same bug from
# different seeds produce the same name, and counting those twice would
# inflate the night and file a duplicate issue. `findings`, `slow`, `leak` and
# `correctness` are those unique-name counts.
#
# A name is not always a bug's identity, though: every Slow finding carries the
# same generic summary, and the v0.67.0-rc2 soak's two Slow findings (shards 2
# and 5) were two different gaps (#3003, #3519). The aggregate used to `cp` the
# second over the first, so one reproduction vanished from the artifact and the
# issue body. Now the first instance of a name is the finding's directory and
# every further instance is kept beside it, in `<name>/instances/<k>/`, and
# `instances` counts them all. The body lists each instance's replay line.
#
# Any depth (#2611): when exactly ONE shard uploaded, its artifact is extracted
# flat (`shards/tools/...`, no per-artifact directory), and the old
# `shards/*/tools/...` glob found nothing there. Run 36051987508 (v0.63.1-rc2
# gate) concluded success with a correctness finding sitting in its only
# artifact.
#
# Usage: fuzz-night-aggregate.sh <shards-dir> <out-dir>

set -uo pipefail
export LC_ALL=C

SHARDS="${1:?shards dir}"
OUT="${2:?output dir}"

mkdir -p "$OUT"
INSTANCES=0
while IFS= read -r d; do
  name=$(basename "$d")
  dest="$OUT/$name"
  if [ -e "$dest" ]; then
    k=2
    while [ -e "$dest/instances/$k" ]; do k=$((k + 1)); done
    mkdir -p "$dest/instances"
    dest="$dest/instances/$k"
  fi
  cp -R "$d" "$dest" 2>/dev/null && INSTANCES=$((INSTANCES + 1))
done < <(find "$SHARDS" -type d -path '*/tools/xtarget-fuzz/findings/*' -prune -print | sort)

count() { find "$OUT" -mindepth 1 -maxdepth 1 -type d "$@" | wc -l | tr -d ' '; }
COUNT=$(count)
SLOW=$(count -name 'Slow__*')
# LeakAtExit (the live-heap rung): wasm heap blocks live at exit. Its own class
# like Slow — tracked, not fatal; the corpus live-at-exit ledger
# (crates/almide-wasm/tests/alloc_ledger.rs) is the red gate for leaks.
LEAK=$(count -name 'LeakAtExit__*')
echo "findings=$COUNT"
echo "slow=$SLOW"
echo "leak=$LEAK"
echo "correctness=$((COUNT - SLOW - LEAK))"
echo "instances=$INSTANCES"
