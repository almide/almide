#!/usr/bin/env bash
# STDLIB TIER GATE (SD2, STDLIB-EXCELLENCE.md — ratified 2026-08-19).
# ============================================================================
# Two machine-enforced tiers (plus the raw floor), Rust-tidy style:
#   core   — pure+total: may not contain `effect fn`, may import core/floor
#   effect — capability-gated modules and their parts
#   floor  — the raw primitive tier (prim, mem): may import nothing
# Membership is DECLARED, one row per stdlib file, in
# scripts/lib/stdlib-tiers.txt (stem<TAB>tier). Every stdlib/*.almd must be
# declared exactly once; stale rows fail. A new module without a declaration
# fails — tier assignment is a reviewed decision, never an accident.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.." || { echo "::error::cannot cd to repo root"; exit 2; }

DECL="scripts/lib/stdlib-tiers.txt"
[ -f "$DECL" ] || { echo "::error::$DECL not found"; exit 2; }
fail=0
err() { echo "::error::$1"; fail=1; }

# ── (a) declaration coverage: files ↔ rows, exactly ─────────────────────────
for f in stdlib/*.almd; do
  stem="$(basename "$f" .almd)"
  n="$(awk -F'\t' -v s="$stem" '$1==s' "$DECL" | wc -l | tr -d ' ')"
  [ "$n" = "1" ] || err "$stem: declared $n times in $DECL (want exactly 1)"
done
while IFS=$'\t' read -r stem tier; do
  [ -z "$stem" ] && continue
  case "$stem" in \#*) continue ;; esac
  [ -f "stdlib/$stem.almd" ] || err "stale declaration: stdlib/$stem.almd does not exist"
  case "$tier" in core|effect|floor) ;; *) err "$stem: unknown tier '$tier'" ;; esac
done < "$DECL"

tier_of() { awk -F'\t' -v s="$1" '$1==s{print $2}' "$DECL"; }

# ── (b) core purity: no effect fn in a core-tier file ───────────────────────
while IFS=$'\t' read -r stem tier; do
  [ "$tier" = "core" ] || continue
  if grep -q "effect fn " "stdlib/$stem.almd"; then
    err "core-tier file stdlib/$stem.almd contains 'effect fn' — reclassify or purify"
  fi
done < "$DECL"

# ── (c)(d) import discipline ────────────────────────────────────────────────
while IFS=$'\t' read -r stem tier; do
  [ -z "$stem" ] && continue
  while IFS= read -r target; do
    [ -z "$target" ] && continue
    ttier="$(tier_of "$target")"
    if [ -z "$ttier" ]; then
      err "stdlib/$stem.almd imports '$target', which has no tier declaration"
      continue
    fi
    if [ "$tier" = "floor" ]; then
      err "floor-tier file stdlib/$stem.almd imports '$target' — the floor imports nothing"
    elif [ "$tier" = "core" ] && [ "$ttier" = "effect" ]; then
      err "core-tier file stdlib/$stem.almd imports effect-tier module '$target'"
    fi
  done <<< "$(grep -E '^import [a-z_0-9]+' "stdlib/$stem.almd" | awk '{print $2}')"
done < "$DECL"

n_core="$(awk -F'\t' '$2=="core"' "$DECL" | wc -l | tr -d ' ')"
n_eff="$(awk -F'\t' '$2=="effect"' "$DECL" | wc -l | tr -d ' ')"
echo "----"
echo "stdlib-tiers: $n_core core, $n_eff effect, floor 2."
if [ "$fail" -eq 0 ]; then
  echo "stdlib-tiers: GREEN — tier boundary holds."
  exit 0
fi
echo "::error::stdlib-tiers gate FAILED — see messages above."
exit 1
