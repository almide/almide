#!/usr/bin/env bash
# INTRINSIC BOUNDARY GATE (#1468 option b)
# ========================================
#
# The runtime boundary is ~770 `@intrinsic("almide_rt_*")` declarations in
# `stdlib/*.almd` against the `almide_rt_*` symbols `runtime/rs` provides
# (as `pub fn`s AND as macro-minted names — `cursor_read_float!(almide_rt_…)`
# defines a symbol the `pub fn` grep never sees). Nothing verified the join:
# a dangling `@intrinsic` surfaced only at render time, as a wall, far from
# the typo that caused it.
#
# Following `proofs/check-stdlib-purity-registry.sh`'s discipline, the join
# is RE-DERIVED from the sources on every run — no hand-maintained row file
# to drift — and enforced in the direction that bites:
#
#   DANGLING (hard): every `@intrinsic("X")` target must occur in runtime/rs
#     as a token. A target no runtime file mentions is a name that can never
#     link.
#
#   ACCOUNTING (reported): runtime `pub fn almide_rt_*` symbols never named
#     by an `@intrinsic` are classified — template-referenced (named under
#     `crates/` or `src/`, e.g. the Codec `__decode_*` glue codegen emits)
#     or orphaned. Orphans are listed so dead runtime surface is visible,
#     not silently carried.
#
# Usage: check-intrinsic-boundary.sh
set -euo pipefail
export LC_ALL=C

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# Every @intrinsic target in the stdlib.
grep -rhoE '@intrinsic\("[a-z0-9_]+"\)' stdlib/*.almd \
  | sed -E 's/@intrinsic\("([a-z0-9_]+)"\)/\1/' | sort -u > "$TMP/targets"

# Every almide_* token runtime/rs mentions (pub fns + macro mints; the
# prefix conventions vary — rt_, http_, json_ — so match the family root).
grep -rhoE 'almide_[a-z0-9_]+' runtime/rs/src/*.rs | sort -u > "$TMP/provided"

# Every pub-fn symbol (for the accounting leg).
grep -rhoE 'pub fn (almide_[a-z0-9_]+)' runtime/rs/src/*.rs \
  | sed 's/^pub fn //' | sort -u > "$TMP/pubfns"

# The PRIM FLOOR is the second provider class: `prim.load8` etc. are
# COMPILER-LOWERED — almide-mir matches the SHORT op name and emits the
# instruction directly; the mangled `almide_rt_prim_*` string is never
# called anywhere, so its @intrinsic target is satisfied by the lowering
# arm, not a runtime fn.
missing=""
while IFS= read -r tgt; do
  [ -n "$tgt" ] || continue
  case "$tgt" in
    almide_rt_prim_*)
      op="${tgt#almide_rt_prim_}"
      grep -rqF "\"$op\"" crates/almide-mir/src/ || missing="$missing$tgt (prim op \"$op\" not in the mir lowering)\n"
      ;;
    *) missing="$missing$tgt\n" ;;
  esac
done < <(comm -23 "$TMP/targets" "$TMP/provided")

if [ -n "$missing" ]; then
  echo "::error::intrinsic-boundary: @intrinsic target(s) with NO provider — these can never link:"
  printf "$missing" | sed 's/^/  - /'
  exit 1
fi

untargeted=$(comm -23 "$TMP/pubfns" "$TMP/targets")
orphans=""
template_refd=0
while IFS= read -r symfn; do
  [ -n "$symfn" ] || continue
  if grep -rqF "$symfn" crates/ src/ --include='*.rs' 2>/dev/null; then
    template_refd=$((template_refd + 1))
  else
    orphans="$orphans$symfn\n"
  fi
done <<< "$untargeted"

t=$(wc -l < "$TMP/targets" | tr -d ' ')
pv=$(wc -l < "$TMP/provided" | tr -d ' ')
pf=$(wc -l < "$TMP/pubfns" | tr -d ' ')
echo "intrinsic-boundary: targets=$t provided-tokens=$pv pub-fns=$pf dangling=0 template-referenced=$template_refd"
if [ -n "$orphans" ]; then
  ocount=$(printf "$orphans" | grep -c . || true)
  echo "orphaned runtime symbols (no @intrinsic, no crates/src reference) — $ocount:"
  printf "$orphans" | sed 's/^/  - /'
fi
