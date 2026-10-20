#!/usr/bin/env bash
# Check the hand-written release notes for a tag, and optionally split them
# into the title and body the release workflow publishes.
#
#   bash scripts/check-release-notes.sh vX.Y.Z           # check only
#   bash scripts/check-release-notes.sh vX.Y.Z OUT_DIR   # check, then write
#                                                        # OUT_DIR/title, OUT_DIR/body.md
#
# The notes live in docs/releases/<tag>.md (format: docs/releases/README.md).
# The release workflow refuses to build a tag whose notes are missing or are
# the auto-generated "What's Changed" PR list.
set -euo pipefail

tag="${1:?usage: check-release-notes.sh <tag> [out-dir]}"
out="${2:-}"
root="$(cd "$(dirname "$0")/.." && pwd)"
file="$root/docs/releases/$tag.md"

fail() {
  echo "release notes for $tag: $*" >&2
  echo "  file: docs/releases/$tag.md — format: docs/releases/README.md" >&2
  exit 1
}

[ -f "$file" ] || fail "missing"

first="$(head -n 1 "$file")"
prefix="# $tag — "
case "$first" in
  "$prefix"*) ;;
  *) fail "line 1 must be '# $tag — <headline>' (got: $first)" ;;
esac
headline="${first#"$prefix"}"
len="${#headline}"
{ [ "$len" -ge 10 ] && [ "$len" -le 120 ]; } \
  || fail "headline must be 10–120 characters (got $len)"

[ -z "$(sed -n 2p "$file")" ] || fail "line 2 must be blank"
body="$(tail -n +3 "$file")"
[ -n "$(printf '%s' "$body" | tr -d '[:space:]')" ] || fail "body is empty"

if grep -q "^## What's Changed" <<<"$body"; then
  fail "body is the auto-generated 'What's Changed' list — write the notes by theme"
fi
if grep -qE '^\* .* by @[^ ]+ in https://github\.com/' <<<"$body"; then
  fail "body has auto-generated '* <PR title> by @user in <url>' lines"
fi
grep -qE "^\*\*Full Changelog\*\*: https://github\.com/almide/almide/compare/v[^ ]+\.\.\.$tag\$" <<<"$body" \
  || fail "body must end with '**Full Changelog**: https://github.com/almide/almide/compare/<prev>...$tag'"

if [ -n "$out" ]; then
  mkdir -p "$out"
  printf '%s — %s\n' "$tag" "$headline" > "$out/title"
  printf '%s\n' "$body" > "$out/body.md"
fi
echo "release notes for $tag: ok"
