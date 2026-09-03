#!/usr/bin/env bash
# Negative controls for the interface differ (#1860): prove that
# check-interface-diff.sh FIRES on a removed fn — an EFFECT fn as much as a
# pure one — and COUNTS an added effect fn. Before #1860 the matcher anchored
# at the module head, so an `effect module.fn(` line was never part of the
# surface: six added effect twins reported as `added=3`, and a removed effect
# fn classified `identical`. The real tag history is the positive control; the
# forged inputs below are what the history cannot supply on demand.
#
# The gate reads its surface from GIT REFS (build-free, forever-derivable), so
# a forged index has to be a commit: this script copies docs/stdlib/*.md at
# the working tree into a scratch repository, commits it as `base`, commits
# each mutation on top as its own tag, and points the gate at the scratch
# repo through INTERFACE_DIFF_ROOT.
set -euo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."

# INTERFACE_DIFF_GATE lets an A/B run point these controls at another copy of
# the gate (the pre-#1860 matcher must FAIL the removed-effect control).
GATE="bash ${INTERFACE_DIFF_GATE:-scripts/check-interface-diff.sh}"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
scratch="$tmp/repo"
pristine="$tmp/pristine"
mkdir -p "$scratch/docs/stdlib" "$pristine"
cp docs/stdlib/*.md "$pristine/"
cp "$pristine"/*.md "$scratch/docs/stdlib/"
export INTERFACE_DIFF_ROOT="$scratch"

g() { git -C "$scratch" -c user.name=negative -c user.email=negative@localhost -c commit.gpgsign=false "$@"; }

# Snapshot the current scratch tree as a tag, then restore the pristine copy so
# every mutation starts from `base` (the scratch repo is throwaway; the real
# tree is never touched).
snapshot() { # tag
  g add -A >/dev/null
  g commit -qm "$1" >/dev/null || { echo "FAIL: snapshot '$1' did not commit (empty tree?)" >&2; exit 1; }
  g tag "$1"
  cp "$pristine"/*.md "$scratch/docs/stdlib/"
}

# Signature lines are only the ones INSIDE the generated blocks — the prose
# above the block quotes signatures in tables, and deleting one of those must
# not count as evidence of anything.
in_block() { # file -> generated-block lines
  awk '/BEGIN GENERATED SIGNATURE INDEX/{on=1} /END GENERATED SIGNATURE INDEX/{on=0} on' "$1"
}
# Pick the first (file, line) whose line matches a pattern, over the pristine index.
pick() { # regex -> "file<TAB>line"
  local f
  for f in "$pristine"/*.md; do
    local l
    l=$(in_block "$f" | grep -E -m1 "$1" || true)
    if [ -n "$l" ]; then printf '%s\t%s\n' "$(basename "$f")" "$l"; return 0; fi
  done
  return 1
}
drop_line() { # file line — delete that exact line from the scratch copy
  grep -vFx -- "$2" "$scratch/docs/stdlib/$1" >"$tmp/edit" && mv "$tmp/edit" "$scratch/docs/stdlib/$1"
}
add_line() { # file line — insert a signature line at the top of the generated block
  awk -v ins="$2" '
    /BEGIN GENERATED SIGNATURE INDEX/ { on=1 }
    on && /^```$/ && !done { print; print ins; done=1; next }
    { print }' "$scratch/docs/stdlib/$1" >"$tmp/edit" && mv "$tmp/edit" "$scratch/docs/stdlib/$1"
}

effect_pick=$(pick '^effect [a-z_0-9]+\.[a-z_0-9]+\(') \
  || { echo "FAIL: no effect fn line in the committed index — this control has nothing to delete" >&2; exit 1; }
pure_pick=$(pick '^[a-z_0-9]+\.[a-z_0-9]+\(') \
  || { echo "FAIL: no pure fn line in the committed index — this control has nothing to delete" >&2; exit 1; }
effect_file=${effect_pick%%	*}; effect_line=${effect_pick#*	}
pure_file=${pure_pick%%	*};     pure_line=${pure_pick#*	}
forged_effect='effect negative_control.forged(x: Int) -> Result[Int, String]'
forged_pure='negative_control.forged_pure(x: Int) -> Int'

g init -q
snapshot base

drop_line "$effect_file" "$effect_line"; snapshot rm-effect
drop_line "$pure_file"   "$pure_line";   snapshot rm-pure
add_line  "$effect_file" "$forged_effect"; snapshot add-effect
add_line  "$pure_file"   "$forged_pure";   snapshot add-pure
# The changed-signature direction: the SAME effect fn with a different return
# type is a removal plus an addition, which must read as breaking.
drop_line "$effect_file" "$effect_line"
add_line  "$effect_file" "${effect_line%% -> *} -> Never"; snapshot chg-effect
# Blind-gate guard (#976 find-nothing-exit-0): a tree whose generated blocks
# carry no signature at all must go loud, not classify anything.
for f in "$scratch"/docs/stdlib/*.md; do
  grep -vE '^(effect )?[a-z_0-9]+\.[a-z_0-9]+\(' "$f" >"$tmp/edit" && mv "$tmp/edit" "$f"
done
snapshot blank

run() { # prev cur [flag] -> stdout+stderr, exit code in $rc
  rc=0; out=$($GATE "$@" 2>&1) || rc=$?
}
expect() { # description rc-expected verdict-substring [must-contain...]
  local desc="$1" want_rc="$2" want="$3"; shift 3
  if [ "$rc" != "$want_rc" ]; then
    echo "FAIL: $desc — exit $rc, expected $want_rc" >&2; printf '%s\n' "$out" >&2; exit 1
  fi
  case "$out" in *"$want"*) ;; *) echo "FAIL: $desc — verdict line missing '$want'" >&2; printf '%s\n' "$out" >&2; exit 1;; esac
  local needle
  for needle in "$@"; do
    case "$out" in *"$needle"*) ;; *) echo "FAIL: $desc — output does not name '$needle'" >&2; printf '%s\n' "$out" >&2; exit 1;; esac
  done
}

# Positive control: the unmodified copy against itself is identical; a harness
# that cannot see the index at all would fail here rather than pass vacuously.
run base base
expect "identical positive control" 0 "= identical (added=0 removed=0)"

# The #1860 direction: a removed EFFECT fn is breaking, named, and refused.
run base rm-effect
expect "removed effect fn" 1 "= breaking (added=0 removed=1)" "  - $effect_line" "BREAKING surface change without --allow-breaking"

# The direction that always worked, kept as the pure-fn twin of the above.
run base rm-pure
expect "removed pure fn" 1 "= breaking (added=0 removed=1)" "  - $pure_line"

# A changed effect signature is a break too (the http.serve v0.56->v0.57 shape).
run base chg-effect
expect "changed effect fn return type" 1 "= breaking (added=1 removed=1)" "  - $effect_line"

# The declared-break path still opens with the flag.
run base rm-effect --allow-breaking
expect "removed effect fn, declared" 0 "= breaking (added=0 removed=1)"

# Additions are counted for effect fns exactly like pure ones.
run base add-effect
expect "added effect fn" 0 "= additive (added=1 removed=0)" "  + $forged_effect"
run base add-pure
expect "added pure fn" 0 "= additive (added=1 removed=0)" "  + $forged_pure"

# Reversal turns the removal into the addition of the same effect line.
run rm-effect base
expect "reversed effect removal" 0 "= additive (added=1 removed=0)" "  + $effect_line"

# Blind-gate guard: an index with no signatures is an error, never a verdict.
run base blank
expect "blank index goes loud" 2 "no generated signature index"

echo "interface-diff negative controls: 1 positive + 8 forged inputs all behaved" \
  "(effect: $effect_line | pure: $pure_line)"
