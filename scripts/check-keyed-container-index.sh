#!/usr/bin/env bash
# KEYED-CONTAINER LOOKUP-COST MATRIX (#1219).
#
# A Map/Set repr on the wasm leg either carries the BUCKET INDEX SIDECAR (O(1) keyed
# lookup, the self-host twin of native's fingerprint index in runtime/rs/src/map.rs) or
# it scans linearly (O(n)). That is a COMPLEXITY divergence between the legs, invisible
# to every output-equivalence gate — a program that is fine natively falls over on wasm.
# So the family is gated by MATRIX, never point-wise: every self-hosted map_*/set_*
# module carries an explicit row here, and CI fails if
#
#   * a module exists with no row            → a new repr landed without a cost decision
#   * a row exists with no module            → the table went stale
#   * a row says `indexed` but the module has no index sidecar
#   * a row says `linear:<ref>` but the module DOES have one (the row is now stale)
#
# COMPLETENESS RULE. Every module that resolves a KEY against its own block owns a row of
# `indexed` or `linear:<tracking ref>`. Modules that never do a keyed lookup — pure
# facades, renderers, folds over an already-located entry — are `n/a` and are exempt by
# construction, not by omission.
set -u
export LC_ALL=C
cd "$(dirname "$0")/.."

# module            state          why
TABLE="
map                 n/a            facade: signatures only, no block access
map_core            indexed        Map[Int,Int] — the scalar-KV twin (#1219)
map_fold_hacc       n/a            folds entries in order; never resolves a key
map_hobj            linear:#1219   Map[String,record] — heap-value repr, not yet indexed
map_hval            linear:#1219   Map[String,List] — heap-value repr, not yet indexed
map_if              n/a            Map[Int,Float] composes over map_core's map.set
map_ivh             linear:#1219   Map[Int,String] — heap-value repr, not yet indexed
map_mlo             linear:#1219   Map[String,List] ordered ops — not yet indexed
map_msv             linear:#1219   Map[String,Value] — not yet indexed
map_skv             linear:#1219   Map[String,Int] split-slot repr — not yet indexed
map_str             linear:#1219   Map[String,String] paired-slot repr — not yet indexed
map_to_string       n/a            renderer: walks entries in order
map_typechange      n/a            re-keys through map_core's map.set
map_vkey            n/a            normalizes a variant key to i64, then map_core
set                 n/a            facade: signatures only, no block access
set_core            indexed        Set[Int] — the scalar twin (#1219)
set_str             linear:#1219   Set[String] — heap-element repr, not yet indexed
set_to_string       n/a            renderer: walks elements in order
set_to_string_s     n/a            renderer: walks elements in order
"

fail=0

# 1. every row's module exists, and its claimed state matches the source
while read -r mod state _rest; do
    [ -n "${mod:-}" ] || continue
    src="stdlib/$mod.almd"
    if [ ! -f "$src" ]; then
        echo "::error::keyed-container-index: row '$mod' has no stdlib/$mod.almd — the table is stale"
        fail=1
        continue
    fi
    # The sidecar is identified by its three REQUIRED helpers (a naming convention this
    # gate enforces, so a half-reverted index — buckets read but never rebuilt, say —
    # is a failure, not a silent downgrade to a scan that still calls itself indexed).
    present=""
    missing=""
    for marker in _reindex _probe _idx_put; do
        if grep -q "$marker(" "$src"; then present="$present $marker"; else missing="$missing $marker"; fi
    done
    case "$state" in
        indexed)
            if [ -n "$missing" ]; then
                echo "::error::keyed-container-index: $mod is rowed 'indexed' but is missing sidecar helper(s):$missing"
                echo "  (an indexed repr defines __<x>_reindex / __<x>_probe / __<x>_idx_put — was the index reverted?)"
                fail=1
            fi
            ;;
        linear:*|n/a)
            if [ -n "$present" ]; then
                echo "::error::keyed-container-index: $mod is rowed '$state' but NOW carries sidecar helper(s):$present"
                echo "  — flip its row to 'indexed' in scripts/check-keyed-container-index.sh"
                fail=1
            fi
            ;;
        *)
            echo "::error::keyed-container-index: $mod has unknown state '$state' (want indexed | linear:<ref> | n/a)"
            fail=1
            ;;
    esac
done <<< "$TABLE"

# 2. every stdlib map_*/set_* module has a row
for src in stdlib/map*.almd stdlib/set*.almd; do
    mod="$(basename "$src" .almd)"
    if ! grep -qE "^ *$mod +" <<< "$TABLE"; then
        echo "::error::keyed-container-index: stdlib/$mod.almd has NO row in the lookup-cost matrix."
        echo "  Add one to scripts/check-keyed-container-index.sh: 'indexed' if it carries the"
        echo "  bucket sidecar, 'linear:<tracking ref>' if it still scans, 'n/a' if it never"
        echo "  resolves a key against a block."
        fail=1
    fi
done

if [ "$fail" -ne 0 ]; then
    exit 1
fi

rows=$(grep -cE "^ *[a-z_]+ +" <<< "$TABLE")
indexed=$(grep -cE "^ *[a-z_]+ +indexed" <<< "$TABLE")
linear=$(grep -cE "^ *[a-z_]+ +linear:" <<< "$TABLE")
echo "keyed-container-index: $rows repr module(s) rowed — $indexed indexed, $linear linear, $((rows - indexed - linear)) n/a"
