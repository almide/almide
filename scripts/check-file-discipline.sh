#!/usr/bin/env bash
# The 800-line file discipline over the greenfield wasm backend — the
# measured primary driver of codopsy slippage (stages 39 and 51 both).
# codopsy itself stays a manual full-measure; THIS is the cheap proxy a
# hook can afford on every push.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2
fail=0
while read -r n f; do
  if [ "$n" -gt 800 ] && [ "$f" != "total" ]; then
    echo "::error::$f has $n lines (max 800) — split before pushing"
    fail=1
  fi
done < <(wc -l crates/almide-wasm/src/*.rs crates/almide-wasm/tests/*.rs | awk '{print $1, $2}')
[ "$fail" = 0 ] && echo "file-discipline OK (all under 800)"
exit $fail
