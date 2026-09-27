#!/bin/bash
# Validate the law-spec tasks: the reference passes visible + laws + hidden,
# and the unmodified source fails the hidden tests (a task it passes measures nothing).
set -uo pipefail
cd "$(dirname "$0")"
RUNTIME=../../../spike/law-blocks/law.almd
WORK=$(mktemp -d)
runtime() { sed '/^\/\/ ── demo/,$d' "$RUNTIME"; }
run() { perl -e 'alarm shift; exec @ARGV' 120 "${ALMIDE_BIN:-almide}" test "$1" >"$1.log" 2>&1; }
status=0
for cfg in tasks/t*.json; do
  t=$(basename "$cfg" .json)
  { cat reference/$t.almd; echo; runtime; echo; cat tasks/${t}_test.almd tasks/${t}_laws_compiled.almd tasks/${t}_hidden.almd; } > $WORK/${t}_ref.almd
  { cat source/$t.almd; echo; cat tasks/${t}_hidden.almd; } > $WORK/${t}_src_hidden.almd
  { cat source/$t.almd; echo; runtime; echo; cat tasks/${t}_laws_compiled.almd; } > $WORK/${t}_src_laws.almd
  run $WORK/${t}_ref.almd && ref=PASS || { ref=FAIL; status=1; }
  run $WORK/${t}_src_hidden.almd && { src=PASS; status=1; } || src=fail
  run $WORK/${t}_src_laws.almd && law=pass || law=FAILS
  printf '%-14s reference=%s  source-vs-hidden=%s  source-vs-laws=%s\n' "$t" "$ref" "$src" "$law"
  [ "$ref" = FAIL ] && grep -E 'error|test:' $WORK/${t}_ref.almd.log | head -6
done
echo "work: $WORK"
exit $status
