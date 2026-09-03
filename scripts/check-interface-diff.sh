#!/usr/bin/env bash
# INTERFACE DIFFER (#1488, the roc-shaped release gate in its reduced form)
# =========================================================================
#
# Classifies the PUBLIC STDLIB SURFACE between two git refs as
#
#   identical   not a single signature changed
#   additive    only NEW signatures appeared — a safe minor release
#   breaking    a signature was REMOVED or CHANGED — callers can break
#
# and exits non-zero on `breaking` unless the break is explicitly declared
# (--allow-breaking). The release procedure runs this before a final tag so
# "the version bump matches the interface diff" is checked, not remembered.
#
# WHY THE DOC INDEX, NOT THE COMPILER. The stdlib is EMBEDDED in the almide
# binary, so "run `almide compile --json` at both revisions" means building
# the compiler at the old tag — a full release build per release, forever
# (this is a sealed-forever measurement's worst nightmare). The machine-owned
# signature index in docs/stdlib/*.md is generated from exactly that
# interface JSON (tools/gen-stdlib-doc-index.py), committed, and CI-gated
# fresh on every push ("Stdlib doc signature indexes up to date") — so the
# TREES alone carry the surface, and the diff is deterministic, build-free,
# and derivable at any pair of refs forever.
#
# Usage:
#   check-interface-diff.sh <prev-ref> <ref> [--allow-breaking]
#
# Negative control (must exit 1): run any additive pair REVERSED —
#   check-interface-diff.sh HEAD v0.57.0
# turns every addition into a removal, which must classify `breaking`.
# The forged-input controls live in check-interface-diff-negative.sh (#1860):
# it points INTERFACE_DIFF_ROOT at a scratch repo whose index copy has one
# effect fn and one pure fn deleted, and expects `breaking` for each.
set -euo pipefail
export LC_ALL=C

ROOT="${INTERFACE_DIFF_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
PREV="${1:?usage: check-interface-diff.sh <prev-ref> <ref> [--allow-breaking]}"
CUR="${2:?usage: check-interface-diff.sh <prev-ref> <ref> [--allow-breaking]}"
ALLOW="${3:-}"

# The index writer (tools/gen-stdlib-doc-index.py `signature()`) emits exactly
# one shape: `[effect ]module.fn(params) -> ret[   (deprecated — …)]`. The
# `effect ` prefix is the only modifier that can precede the qualified name,
# and it MUST be admitted here — anchoring at the module head skipped every
# effect fn, so an added effect twin was uncounted and a REMOVED effect fn
# classified `identical` (#1860). Any new modifier the writer grows belongs in
# this one pattern; the negative control (check-interface-diff-negative.sh)
# deletes an effect line and a pure line and expects `breaking` for both.
SIG_HEAD='^(effect )?[a-z_0-9]+\.'

# All signature lines inside the generated blocks of docs/stdlib/*.md at REF.
surface() { # ref -> sorted signature lines on stdout
  local ref="$1"
  git -C "$ROOT" ls-tree -r --name-only "$ref" -- docs/stdlib/ 2>/dev/null \
    | grep -E '\.md$' \
    | while read -r f; do
        git -C "$ROOT" show "$ref:$f" 2>/dev/null \
          | awk '/BEGIN GENERATED SIGNATURE INDEX/{on=1} /END GENERATED SIGNATURE INDEX/{on=0} on'
      done \
    | grep -E "${SIG_HEAD}[a-z_0-9]+\(" \
    | grep -vE "${SIG_HEAD}__" \
    | sed -E 's/[[:space:]]+\(deprecated[^)]*\)[[:space:]]*$//' \
    | sort -u
}
# The `(deprecated — …)` annotation the index renders (#1735/#1758) is
# METADATA, stripped before the diff: gaining or losing the annotation is
# not a signature change — the deprecation window is exactly the
# mechanism that keeps a later REMOVAL classifiable here, and marking a
# fn deprecated must never itself read as the break it exists to prevent.
# The `__`-prefixed names are INTERNAL carriers (the ADR-0006 fallible family,
# self-host helpers) — checker-inserted, not writable surface — so their
# appearance and disappearance is not an interface event.

# An empty surface makes the trailing grep exit 1, which under pipefail would
# let `set -e` kill the script silently with exit 1 — before the loud exit-2
# guard below ever ran (the negative control's blank-index case). Swallow the
# pipeline status so emptiness is judged by the guard, not by set -e.
prev_s=$(surface "$PREV" || true)
cur_s=$(surface "$CUR" || true)

[ -n "$prev_s" ] || { echo "check-interface-diff: no generated signature index at $PREV" >&2; exit 2; }
[ -n "$cur_s" ]  || { echo "check-interface-diff: no generated signature index at $CUR" >&2; exit 2; }

removed=$(comm -23 <(printf '%s\n' "$prev_s") <(printf '%s\n' "$cur_s"))
added=$(comm -13 <(printf '%s\n' "$prev_s") <(printf '%s\n' "$cur_s"))

if [ -z "$removed" ] && [ -z "$added" ]; then
  verdict="identical"
elif [ -z "$removed" ]; then
  verdict="additive"
else
  verdict="breaking"
fi

echo "interface-diff: $PREV -> $CUR = $verdict (added=$(printf '%s' "$added" | grep -c . || true) removed=$(printf '%s' "$removed" | grep -c . || true))"
if [ -n "$removed" ]; then
  echo "removed/changed signatures:"
  printf '%s\n' "$removed" | sed 's/^/  - /'
fi
if [ -n "$added" ]; then
  echo "added signatures:"
  printf '%s\n' "$added" | head -40 | sed 's/^/  + /'
fi

if [ "$verdict" = "breaking" ] && [ "$ALLOW" != "--allow-breaking" ]; then
  echo "interface-diff: BREAKING surface change without --allow-breaking —" >&2
  echo "a removal/rename needs its @deprecated window (E052), a dialect-epoch" >&2
  echo "entry when it can break written code (proofs/dialect-epochs.toml)," >&2
  echo "and an explicit declaration here." >&2
  exit 1
fi
