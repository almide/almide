#!/bin/bash
# One bisect step for #2398. Exit 0 = good (legs agree), 1 = bad (wasm wrong), 125 = skip.
# Every path that cannot answer the question exits 125 rather than guessing — a build
# failure or a missing r5 line is "unknown", not "good".
set -u
export PATH=/opt/homebrew/bin:$PATH
W=/Users/o6lvl4/workspace/github.com/almide/almide/.claude/worktrees/wt-2310
S=/private/tmp/claude-501/-Users-o6lvl4-workspace-github-com-almide-almide/ad1bfb1f-617a-476e-996f-c0f2c4629087/scratchpad/ab2398
cd "$W" || exit 125
# The build regenerates `src/generated/*` from runtime/rs at whatever commit is
# checked out, which dirties the tree and makes the NEXT checkout refuse. Restore
# only paths under src/generated/, and say which — a silent reset here would be a
# script quietly discarding changes it did not make.
restore_generated() {
  local dirty
  dirty=$(git status --porcelain -- '*/src/generated/*' | awk '{print $2}')
  if [ -n "$dirty" ]; then
    echo "  (restoring regenerated: $(echo $dirty | tr '\n' ' '))"
    echo "$dirty" | while IFS= read -r f; do [ -n "$f" ] && git checkout -- "$f"; done
  fi
  local left
  left=$(git status --porcelain | wc -l | tr -d ' ')
  if [ "$left" != "0" ]; then echo "  WARNING: tree still dirty ($left paths) — bisect will stall"; fi
}
SHA=$(git rev-parse --short HEAD)
FREE=$(df -g /Users | awk 'NR==2{print $4}')
if [ "$FREE" -lt 5 ]; then echo "[$SHA] ABORT: only ${FREE}G free"; exit 128; fi
if ! cargo build --release --bin almide >/tmp/bisect-build.log 2>&1; then
  echo "[$SHA] SKIP: build failed ($(tail -3 /tmp/bisect-build.log | tr '\n' ' ' | cut -c1-120))"; restore_generated; exit 125
fi
B=$W/target/release/almide
NAT=$("$B" run "$S/repro.almd" 2>&1 | grep '^r5 = ')
WAS=$("$B" run "$S/repro.almd" --target wasm 2>&1 | grep '^r5 = ')
if [ -z "$NAT" ] || [ -z "$WAS" ]; then
  echo "[$SHA] SKIP: no r5 line (native='${NAT}' wasm='${WAS}')"; restore_generated; exit 125
fi
NQ=$(printf '%s' "$NAT" | od -An -c | tr -s ' '); WQ=$(printf '%s' "$WAS" | od -An -c | tr -s ' ')
if [ "$NAT" = "$WAS" ]; then
  echo "[$SHA] GOOD  native=$NQ wasm=$WQ"; restore_generated; exit 0
else
  echo "[$SHA] BAD   native=$NQ wasm=$WQ"; restore_generated; exit 1
fi
