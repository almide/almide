#!/usr/bin/env bash
# usage: run.sh <commit> <listfile> <targets...>
set -u
c=$1; list=$2; shift 2
p=$(git rev-parse $c^)
git checkout -q $c
build() { cargo build -q -p almide-mir --example classify_corpus 2>&1 | grep -E '^error' | head -3; return ${PIPESTATUS[0]}; }
classify() {
  for f in "$@"; do d=$(mktemp -d); cp "$f" "$d/"; mkdir -p /tmp/out
    cargo run -q -p almide-mir --example classify_corpus -- --out /tmp/out "$d" 2>&1 >/dev/null | grep -c '' >/dev/null
    WALL_NAMES=1 cargo run -q -p almide-mir --example classify_corpus -- --out /tmp/out "$d" 2>&1 | grep '^WALLED REAL' | sed -E 's|^WALLED REAL [^ ]*/([^/ ]*)\.almd :: (.{0,40}).*|\1::\2|'
  done | sort | tr '\n' ';'
}
build >/dev/null; echo "BASE $c walls: $(classify "$@")"
while read -r f; do
  [ -z "$f" ] && continue
  git checkout -q $p -- "$f"
  if build >/tmp/err; then echo "REVERT $f -> $(classify "$@")"; else echo "REVERT $f -> NOBUILD $(head -1 /tmp/err)"; fi
  git checkout -q $c -- "$f"
done < "$list"
