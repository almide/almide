#!/usr/bin/env bash
# Diagnostics-parity golden generator (unit 4). Runs the ORACLE almide binary
# (clean a877d2138 build) as `almide check <file> --json` over every
# spec/**/*.almd and records the sha256 of its stdout. The greenfield
# `check_file_json` query must reproduce every hash byte-for-byte
# (crates/almide-spine/tests/check_parity.rs).
#
#   ORACLE=/path/to/almide bash scripts/gen-check-manifest.sh
#
# Outputs (committed):
#   crates/almide-spine/tests/golden/spec-check-manifest.txt   sha256<TAB>exit<TAB>path
#   crates/almide-spine/tests/golden/spec-check-exclusions.txt path<TAB>reason
# A file lands in exclusions only when the oracle DIED without producing
# parseable stdout semantics (e.g. resolve failure exits before printing).
# Empty stdout is legitimate (no diagnostics) and hashes as the empty string.
#
# TWO ROOTS: see scripts/gen-ast-manifest.sh — the oracle runs with cwd = the
# fixture's root (this tree or the almide/als mount) so the relative path it
# embeds in diagnostics is the corpus path the parity test uses.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.." || exit 2

ORACLE="${ORACLE:?set ORACLE to the almide binary built from the port SHA}"
case "$ORACLE" in /*) ;; *) ORACLE="$PWD/$ORACLE" ;; esac
"$ORACLE" --version >/dev/null || exit 2

OUT_DIR="crates/almide-spine/tests/golden"
mkdir -p "$OUT_DIR"
MANIFEST="$OUT_DIR/spec-check-manifest.txt"
EXCLUDED="$OUT_DIR/spec-check-exclusions.txt"
: > "$MANIFEST"; : > "$EXCLUDED"

# Two forms of the tree: the greenfield form mounts the judge corpus as the
# als/ submodule (two roots); the main repo holds the whole corpus in-tree
# (one root). Same partition semantics either way (almide-corpus).
ROOTS="."
[ -d als/spec ] && ROOTS=". als"
for r in $ROOTS; do
  [ -d "$r/spec" ] || { echo "::error::$r/spec missing"; exit 2; }
done
root_of() { if [ -e "$1" ]; then printf '.'; else printf 'als'; fi; }
check_json() { (cd "$1" && "$ORACLE" check "$2" --json); }

hash_out() { # normalized: empty stays empty; else exactly one trailing \n
  if [ -n "$1" ]; then printf '%s\n' "$1" | shasum -a 256 | cut -d' ' -f1
  else printf '' | shasum -a 256 | cut -d' ' -f1; fi
}

n_ok=0; n_skip=0
for r in $ROOTS; do
  while IFS= read -r f; do
    out="$(check_json "$r" "$f" 2>/tmp/chk-err.$$)"; rc=$?
    # Resolve/module failures exit via stderr with no JSON stdout — those
    # stdouts are not check results; exclude with the reason. rc=1 WITH stdout
    # is a normal "has errors" result and stays in the manifest.
    if [ "$rc" -ne 0 ] && [ -z "$out" ]; then
      reason="$(head -1 /tmp/chk-err.$$ | tr '\t' ' ' | cut -c1-160)"
      printf '%s\t%s\n' "$f" "${reason:-exit $rc, empty stdout}" >> "$EXCLUDED"
      n_skip=$((n_skip + 1))
    else
      printf '%s\t%s\t%s\n' "$(hash_out "$out")" "$rc" "$f" >> "$MANIFEST"
      n_ok=$((n_ok + 1))
    fi
  done <<< "$(cd "$r" && find spec -name '*.almd' | sort)"
done
rm -f /tmp/chk-err.$$
sort -t$'\t' -k3 -o "$MANIFEST" "$MANIFEST"
sort -o "$EXCLUDED" "$EXCLUDED"

# Determinism spot-check on the first 25 manifest rows.
ndet=0
while IFS=$'\t' read -r want _rc f; do
  got="$(hash_out "$(check_json "$(root_of "$f")" "$f" 2>/dev/null)")"
  [ "$got" = "$want" ] || { echo "::error::nondeterministic check output for $f"; ndet=$((ndet + 1)); }
done <<< "$(head -25 "$MANIFEST")"
[ "$ndet" -eq 0 ] || exit 1

echo "manifest: $n_ok files, exclusions: $n_skip (see $EXCLUDED)"
