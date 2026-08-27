#!/usr/bin/env bash
# check-docs-only.sh — can the compiler's test matrix observe this change set?
#
# A README-only PR used to spend the full CI budget: four Test Rust shards
# (14–20 min), two determinism sweeps, the gates job, the perf ratchet — none
# of which read the files that changed. The `changes` job in ci.yml runs this
# script first and the heavy jobs carry `if: needs.changes.outputs.docs_only
# != 'true'`; a job skipped by its own `if` reports SUCCESS to branch
# protection ("Jobs that are skipped will report their status as Success.
# They will not prevent a pull request from merging, even if it is a required
# check" — GitHub docs, "Handling skipped but required checks"), unlike a
# workflow filtered out by `paths`, which stays Expected forever.
#
# The verdict is the printed `docs_only=true|false` (also appended to
# $GITHUB_OUTPUT when set); the exit code is 0 either way — an unreadable
# range is an ERROR (exit 2), never a silent `true`.
#
#   check-docs-only.sh <base> <head>   # a git range; the change set is `git diff --name-only base head`
#   check-docs-only.sh --stdin         # newline-separated paths on stdin (the negative tests use this)
#
# A path is DOCS only when it matches the ALLOW-LIST below. Everything else —
# every .rs/.almd/.toml/.txt/.sh/.yml, Cargo files, spec/, stdlib/, scripts/,
# workflows — is CODE and buys the full run. The list is deliberately short:
# misclassifying a code path as docs skips twenty minutes of tests, the
# reverse costs twenty minutes, so the bias is toward CODE.
#
# Markdown the compiler or its tests CONSUME is code, not docs:
#   docs/diagnostics/**            — tests/explain_docs_test.rs and
#                                    tests/diagnostic_coverage_test.rs read them
#   docs/project/CLAUDE_TEMPLATE.md — include_str! into the binary (src/cli/commands.rs)
# The README-reading checks that must still run on a docs-only change live in
# the always-on `checks` job: `almide docs-gen --check` (the stdlib-count claim
# in README/SPEC/WASM-OUTPUT and llms.txt, tests/docs_gen_test.rs's subject),
# the LLM-surface fences, the stdlib doc indexes, the README stats and the
# build-speed block render. Widening the allow-list means auditing what reads
# the new path first — grep for it under tests/, src/, crates/ and scripts/.
set -euo pipefail

is_docs() {
  case "$1" in
    docs/diagnostics/*)                 return 1 ;;
    docs/project/CLAUDE_TEMPLATE.md)    return 1 ;;
    *.md)                               return 0 ;;
    docs/assets/*|docs/figures/*)       return 0 ;;
    LICENSE|LICENSE-*|LICENSE.*|CITATION*) return 0 ;;
    .github/ISSUE_TEMPLATE/*|.github/PULL_REQUEST_TEMPLATE*) return 0 ;;
    *)                                  return 1 ;;
  esac
}

if [ "${1:-}" = "--stdin" ]; then
  files="$(cat)"
elif [ $# -eq 2 ]; then
  git rev-parse --verify -q "$1^{commit}" >/dev/null || { echo "::error::check-docs-only: unknown base '$1'"; exit 2; }
  git rev-parse --verify -q "$2^{commit}" >/dev/null || { echo "::error::check-docs-only: unknown head '$2'"; exit 2; }
  files="$(git diff --name-only "$1" "$2")"
else
  echo "usage: $0 <base> <head> | --stdin" >&2
  exit 2
fi

verdict=true
reason="every changed path is documentation"
n="$(printf '%s\n' "$files" | grep -c . || true)"
code_hit=""
while IFS= read -r f; do
  [ -n "$f" ] || continue
  if ! is_docs "$f"; then verdict=false; code_hit="$f"; break; fi
done <<< "$files"

if [ "$n" -eq 0 ]; then
  # An empty change set has nothing to classify; the safe reading is "run it all".
  verdict=false
  reason="empty change set — nothing to classify, full run"
elif [ "$verdict" = false ]; then
  reason="code path: $code_hit"
fi

echo "docs_only=$verdict"
echo "check-docs-only: $reason ($n changed path(s))"
if [ -n "${GITHUB_OUTPUT:-}" ]; then echo "docs_only=$verdict" >> "$GITHUB_OUTPUT"; fi
