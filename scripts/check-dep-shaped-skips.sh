#!/usr/bin/env bash
# DEPENDENCY-SHAPED LEG SKIP LIST IS SHRINK-ONLY (#2839)
# ======================================================
#
# tests/dep_shaped_leg_test.rs runs every multi-module project in the spec
# corpus as written AND as a path dependency, and requires the two to agree.
# scripts/lib/dep-shaped-skips.txt lists the cases allowed to diverge. The
# test itself fails on an entry that stopped diverging or names no case (the
# list goes DOWN when a bug is fixed); this gate refuses the other direction:
# an entry the base branch did not have. A new divergence is a bug to fix (or
# at least to file and fix), not a line to add.
#
#   check-dep-shaped-skips.sh <base>     entries at HEAD's working tree vs <base>
#   check-dep-shaped-skips.sh --self-test   negative controls (a grown list is red)
#
# A base without the file (the commit that introduced the leg) is the
# bootstrap: every entry is accepted once.
set -euo pipefail
export LC_ALL=C

LIST=scripts/lib/dep-shaped-skips.txt

ids() { # stdin: a skip list → its case ids, sorted
  grep -vE '^[[:space:]]*(#|$)' | awk '{print $1}' | sort -u
}

check() { # <base> — run from the repository root
  local base="$1" added
  if ! git cat-file -e "$base^{commit}" 2>/dev/null; then
    echo "::error::dep-shaped-skips: base $base is not a commit here (fetch-depth?)"
    return 2
  fi
  if ! git cat-file -e "$base:$LIST" 2>/dev/null; then
    echo "dep-shaped-skips: $LIST is new at this base — bootstrap, $(ids < "$LIST" | wc -l | tr -d ' ') entr(ies) accepted"
    return 0
  fi
  added=$(comm -13 <(git show "$base:$LIST" | ids) <(ids < "$LIST"))
  if [ -n "$added" ]; then
    echo "::error::dep-shaped-skips: the skip list grew — these cases were not skipped at $base:"
    echo "$added" | sed 's/^/  /'
    echo "  The list is shrink-only. A case that diverges between its as-written run and"
    echo "  its dependency-shaped run is a bug: fix it, or keep it red until it is fixed."
    return 1
  fi
  echo "dep-shaped-skips: $(ids < "$LIST" | wc -l | tr -d ' ') entr(ies), none new since $base"
}

self_test() {
  local tmp rc
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' RETURN
  (
    set -e
    cd "$tmp"
    git init -q . && git config user.email t@t && git config user.name t
    mkdir -p scripts/lib
    printf '# header\ncase:a  #1 reason\n' > "$LIST"
    git add . && git commit -qm base
    base=$(git rev-parse HEAD)
    # unchanged: green
    check "$base" > /dev/null || { echo "self-test: an unchanged list was refused"; exit 1; }
    # shrunk: green
    printf '# header\n' > "$LIST"
    check "$base" > /dev/null || { echo "self-test: a shrunk list was refused"; exit 1; }
    # grown: red
    printf '# header\ncase:a  #1 reason\ncase:b  #2 new\n' > "$LIST"
    rc=0; check "$base" > /dev/null || rc=$?
    [ "$rc" = 1 ] || { echo "self-test: a grown list was accepted"; exit 1; }
    # a reworded reason for an existing id is not growth
    printf '# header\ncase:a  #1 other words\n' > "$LIST"
    check "$base" > /dev/null || { echo "self-test: a reworded entry was refused"; exit 1; }
    # a base without the list is the bootstrap: green
    git rm -qf "$LIST"
    git commit -qm drop
    base2=$(git rev-parse HEAD)
    mkdir -p scripts/lib
    printf 'case:z  #9 x\n' > "$LIST"
    check "$base2" > /dev/null || { echo "self-test: the bootstrap was refused"; exit 1; }
  )
  echo "dep-shaped-skips self-test: growth refused, shrink/reword/bootstrap accepted"
}

cd "$(git rev-parse --show-toplevel)"
case "${1:-}" in
  --self-test) self_test ;;
  "") echo "usage: $0 <base> | --self-test" >&2; exit 2 ;;
  *) check "$1" ;;
esac
