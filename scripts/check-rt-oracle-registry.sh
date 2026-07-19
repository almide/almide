#!/usr/bin/env bash
# NEW-ROUTINE ORACLE-PAIRING GATE — Stage 1c of the completeness roadmap.
#
# Every WASM runtime routine (`fn compile_*` in crates/almide-codegen/src/emit_wasm/)
# must be REGISTERED in crates/almide-codegen/rt-oracle-registry.toml with the status
# of its differential verification against the native runtime/rs oracle. ~72% of all
# cross-target (native ⇄ WASM) bugs were this WASM runtime drifting from the
# std-backed native runtime; the forward kill is to never let a runtime routine merge
# without a registered oracle pairing. New routines should ship a differential test
# and be "verified"; "grandfathered" is the pre-existing drain backlog.
#
# This gate is pure grep/awk — no cargo build, no network — and runs in well under 5s.
# It FAILS when:
#   (a) a compile_* routine in emit_wasm/ is ABSENT from the registry,
#   (b) a registry entry names a routine that no longer exists (stale entry),
#   (c) a "verified" entry's cited test path does not exist, or the test name is not
#       found in that file.
# The composite key is file::routine (two routines share the bare names compile_cmp /
# compile_helpers across files, so the bare name alone is not unique).
set -uo pipefail

# #782: the v0 wasm emitter (crates/almide-codegen/src/emit_wasm/) is RETIRED —
# there are no hand-written wasm runtime routines left to pair against the
# native oracle. The v1 trust-spine's stdlib self-hosts are differential-gated
# by spec/wasm_cross (the cross-target byte gate) and the interp 3-way oracle
# instead. This gate is kept as a tombstone so CI/lefthook wiring stays intact;
# it re-arms automatically if emit_wasm/ ever reappears.
if [ ! -d "crates/almide-codegen/src/emit_wasm" ]; then
  echo "rt-oracle-registry: RETIRED with the v0 wasm emitter (#782) — v1 self-hosts are gated by spec/wasm_cross + the interp oracle."
  exit 0
fi
cd "$(dirname "$0")/.." || { echo "::error::cannot cd to repo root"; exit 2; }

EMIT_DIR="crates/almide-codegen/src/emit_wasm"
REGISTRY="crates/almide-codegen/rt-oracle-registry.toml"
# Shared, single-source-of-truth evidence-class vocabulary. A verified routine MAY
# carry an OPTIONAL `class = "..."` (mirroring docs/contracts/contracts.toml); if
# present it must be one of these. Sourcing the SAME file the contract gate uses
# means the two enums provably cannot drift.
CLASS_FILE="scripts/lib/contract-classes.txt"

[ -d "$EMIT_DIR" ] || { echo "::error::$EMIT_DIR not found (run from repo root)"; exit 2; }
[ -f "$REGISTRY" ] || { echo "::error::$REGISTRY not found"; exit 2; }
[ -f "$CLASS_FILE" ] || { echo "::error::$CLASS_FILE not found"; exit 2; }

# ── Structural / orchestration emitters that are NOT runtime functions ──
# These compile user IrFunctions, lambda bodies, the _start / test harness, or are
# emit-order dispatchers that merely CALL the real runtime bodies. They have no native
# runtime/rs counterpart whose semantics they mirror, so they are intentionally not
# registered. Keys are file::routine to stay precise. KEEP THIS IN SYNC with the
# "EXCLUDED" block documented at the top of the registry.
STRUCTURAL_EXCLUDE="
functions.rs::compile_function
functions.rs::compile_module_function
functions.rs::compile_function_with_init
functions.rs::compile_function_inner
closures.rs::compile_lambda_bodies
mod_p4.rs::compile_init_globals
mod_p4.rs::compile_test_runner
mod_p4.rs::compile_main_runner
runtime.rs::compile_runtime
runtime_p2.rs::compile_alloc_pinned
rt_dragon.rs::compile_driver
rt_dragon.rs::compile_helpers
rt_dec2flt.rs::compile_helpers
rt_libm.rs::compile_helpers
"

# The exclude set as a clean newline-delimited list (blank lines stripped, so a
# `grep -f` pattern is never the empty string = "match everything").
EXCLUDE_LIST="$(printf '%s\n' "$STRUCTURAL_EXCLUDE" | grep . || true)"

# ── (1) The CURRENT set of runtime routines in emit_wasm/, as file::routine keys ──
# grep -o prints "<path>:fn compile_x"; reduce to "<basename>::compile_x", then drop
# the structural-exclude set. Sorted + de-duped for deterministic output.
actual_keys() {
  grep -rnoE 'fn compile_[A-Za-z0-9_]+' "$EMIT_DIR" \
    | sed -E 's#^.*/([^/:]+):[0-9]+:fn (compile_[A-Za-z0-9_]+)$#\1::\2#' \
    | grep -vxF -f <(printf '%s\n' "$EXCLUDE_LIST") \
    | sort -u
}

# ── (2) The REGISTERED set — parse [[routine]] tables for file::routine keys ──
# Pure awk: within each [[routine]] block, capture `file = "..."` and `routine = "..."`,
# emit file::routine when the block ends (next [[routine]] or EOF).
registry_keys() {
  awk '
    /^\[\[routine\]\]/ { if (f != "" && r != "") print f "::" r; f=""; r=""; next }
    /^file[ \t]*=/     { gsub(/^file[ \t]*=[ \t]*"/, ""); gsub(/".*$/, ""); f=$0; next }
    /^routine[ \t]*=/  { gsub(/^routine[ \t]*=[ \t]*"/, ""); gsub(/".*$/, ""); r=$0; next }
    END { if (f != "" && r != "") print f "::" r }
  ' "$REGISTRY" | sort -u
}

# Emit one line per SCHEMA violation: a status outside the two-value enum, or a
# "verified" entry with no test= field. The gate must enforce its own schema —
# otherwise a typo'd status or a test-less "verified" silently passes (both were
# demonstrated escape hatches before this check existed).
schema_violations() {
  awk '
    function flush() {
      if (f == "" && r == "") return
      key = f "::" r
      if (st != "verified" && st != "grandfathered")
        print key ": status \"" st "\" is not one of {verified, grandfathered}"
      else if (st == "verified" && t == "")
        print key ": status is \"verified\" but no test= is cited (verified REQUIRES a differential test)"
    }
    /^\[\[routine\]\]/ { flush(); f=""; r=""; st=""; t=""; next }
    /^file[ \t]*=/     { v=$0; gsub(/^file[ \t]*=[ \t]*"/, "", v); gsub(/".*$/, "", v); f=v; next }
    /^routine[ \t]*=/  { v=$0; gsub(/^routine[ \t]*=[ \t]*"/, "", v); gsub(/".*$/, "", v); r=v; next }
    /^status[ \t]*=/   { v=$0; gsub(/^status[ \t]*=[ \t]*"/, "", v); gsub(/".*$/, "", v); st=v; next }
    /^test[ \t]*=/     { v=$0; gsub(/^test[ \t]*=[ \t]*"/, "", v); gsub(/".*$/, "", v); t=v; next }
    END { flush() }
  ' "$REGISTRY"
}

# Emit, for each verified entry, "file::routine<TAB>test-path::test-name".
verified_tests() {
  awk '
    /^\[\[routine\]\]/ { if (f!="" && r!="" && st=="verified" && t!="") print f"::"r"\t"t; f=""; r=""; st=""; t=""; next }
    /^file[ \t]*=/     { v=$0; gsub(/^file[ \t]*=[ \t]*"/, "", v); gsub(/".*$/, "", v); f=v; next }
    /^routine[ \t]*=/  { v=$0; gsub(/^routine[ \t]*=[ \t]*"/, "", v); gsub(/".*$/, "", v); r=v; next }
    /^status[ \t]*=/   { v=$0; gsub(/^status[ \t]*=[ \t]*"/, "", v); gsub(/".*$/, "", v); st=v; next }
    /^test[ \t]*=/     { v=$0; gsub(/^test[ \t]*=[ \t]*"/, "", v); gsub(/".*$/, "", v); t=v; next }
    END { if (f!="" && r!="" && st=="verified" && t!="") print f"::"r"\t"t }
  ' "$REGISTRY"
}

ACTUAL="$(actual_keys)"
REGISTERED="$(registry_keys)"

fail=0

# ── (a) Unregistered routines (present in source, absent from registry) ──
missing="$(comm -23 <(printf '%s\n' "$ACTUAL") <(printf '%s\n' "$REGISTERED"))"
if [ -n "$missing" ]; then
  fail=1
  echo "::error::WASM runtime routine(s) NOT registered in $REGISTRY:"
  while IFS= read -r key; do
    [ -z "$key" ] && continue
    echo "  - $key"
  done <<< "$missing"
  echo ""
  echo "Every 'fn compile_*' in $EMIT_DIR is a WASM runtime routine that must be paired"
  echo "with the native runtime/rs oracle it mirrors. Add an entry like:"
  echo ""
  echo '    [[routine]]'
  echo '    file = "<basename>.rs"        # e.g. rt_string.rs'
  echo '    routine = "compile_<name>"'
  echo '    status = "verified"           # NEW routines should ship a differential test'
  echo '    oracle = "runtime/rs/src/<module>.rs::<fn>  (what it must match)"'
  echo '    test = "<path>::<test-name>"  # required when status = "verified"'
  echo ""
  echo 'Do NOT add to the "grandfathered" backlog — that is the pre-existing drain set.'
  echo 'If the routine is a structural/orchestration emitter (not a runtime function),'
  echo "add it to STRUCTURAL_EXCLUDE in this script and the EXCLUDED block in the registry."
fi

# ── (b) Stale registry entries (registered, but the routine no longer exists) ──
stale="$(comm -13 <(printf '%s\n' "$ACTUAL") <(printf '%s\n' "$REGISTERED"))"
if [ -n "$stale" ]; then
  fail=1
  echo "::error::STALE registry entries in $REGISTRY — the routine no longer exists:"
  while IFS= read -r key; do
    [ -z "$key" ] && continue
    echo "  - $key"
  done <<< "$stale"
  echo "Remove the [[routine]] block(s) above so the registry stays honest."
fi

# ── (c) Schema enforcement: status enum + verified-requires-test ──
violations="$(schema_violations)"
if [ -n "$violations" ]; then
  fail=1
  echo "::error::Registry SCHEMA violations in $REGISTRY:"
  while IFS= read -r v; do
    [ -z "$v" ] && continue
    echo "  - $v"
  done <<< "$violations"
fi

# ── (d) "verified" entries must cite a real test path + name ──
while IFS=$'\t' read -r key spec; do
  [ -z "$key" ] && continue
  path="${spec%%::*}"
  name="${spec#*::}"
  if [ ! -f "$path" ]; then
    fail=1
    echo "::error::$key is 'verified' but its test path does not exist: $path"
    continue
  fi
  # Match the test name as a Rust fn definition (handles `fn name(` and `fn name (`).
  if ! grep -qE "fn[[:space:]]+${name}[[:space:]]*\(" "$path"; then
    fail=1
    echo "::error::$key is 'verified' but test '$name' was not found in $path"
  fi
done < <(verified_tests)

# ── (e) class enum (the contract-ledger unification) ──
# A `class = "..."` line in any [[routine]] block must be a valid evidence class
# from the shared scripts/lib/contract-classes.txt. This keeps the registry's
# optional class= vocabulary identical to the contract ledger's — one list file,
# no drift. Grandfathered entries carry no class= and are unaffected.
VALID_CLASSES="$(grep -vE '^[[:space:]]*(#|$)' "$CLASS_FILE")"
while IFS= read -r cls; do
  [ -z "$cls" ] && continue
  if ! printf '%s\n' "$VALID_CLASSES" | grep -qxF "$cls"; then
    fail=1
    echo "::error::registry has class \"$cls\" which is not a valid evidence class (see $CLASS_FILE: $(printf '%s' "$VALID_CLASSES" | paste -sd, -))"
  fi
done < <(grep -E '^class[ \t]*=' "$REGISTRY" | sed -E 's/^class[ \t]*=[ \t]*"//; s/".*$//' | sort -u)

n_actual="$(printf '%s\n' "$ACTUAL" | grep -c . || true)"
n_reg="$(printf '%s\n' "$REGISTERED" | grep -c . || true)"
n_ver="$(grep -c '^status = "verified"' "$REGISTRY" || true)"
n_grand="$(grep -c '^status = "grandfathered"' "$REGISTRY" || true)"

# Belt-and-suspenders: every entry is exactly one of the two statuses, so the
# counts must tile the registry. A mismatch means a malformed/duplicated status
# line slipped past the per-entry schema check.
if [ "$((n_ver + n_grand))" -ne "$n_reg" ]; then
  fail=1
  echo "::error::status counts do not tile the registry: verified($n_ver) + grandfathered($n_grand) != entries($n_reg)"
fi

echo "----"
# ── Ratchet ceiling: the grandfathered count may only go DOWN ──
# ZERO. The drain is complete (2026-06-06): cow_check was fixed (value
# semantics for aliased mutable collections, locked by spec/wasm_cross/alias_cow.almd)
# and the fs pair was corpus-locked by spec/wasm_cross/fs_preopen_resolve.almd.
# Every wasm runtime routine is verified against its native oracle. New routines
# MUST ship verified — this ceiling never rises.
MAX_GRANDFATHERED=0
if [ "$n_grand" -gt "$MAX_GRANDFATHERED" ]; then
  fail=1
  echo "::error::grandfathered count $n_grand exceeds the ratchet ceiling $MAX_GRANDFATHERED — new routines must ship verified (see crates/almide-codegen/CLAUDE.md)"
fi
if [ "$fail" -ne 0 ]; then
  echo "::error::rt-oracle-registry gate FAILED — see messages above."
  exit 1
fi
echo "rt-oracle-registry: OK — $n_actual runtime routines, all registered ($n_reg entries)."
echo "  verified=$n_ver  grandfathered=$n_grand / ceiling $MAX_GRANDFATHERED  (grandfathered = Stage-2 drain backlog)"
