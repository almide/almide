#!/usr/bin/env bash
# AST-parity golden generator (unit 2). Runs the ORACLE almide binary — the
# CLI built from THIS tree (target/release/almide; #2183) — over every
# spec/**/*.almd and records the sha256 of its `--emit-ast` stdout. The
# greenfield parser must reproduce every hash byte-for-byte
# (crates/almide-syntax/tests/ast_parity.rs). CI regenerates and diffs the
# outputs (scripts/check-parity-goldens.sh), so a committed row can only
# change by the CLI's own output changing.
#
#   ORACLE=target/release/almide bash scripts/gen-ast-manifest.sh
#
# Outputs (committed; the manifest starts with one `# oracle:` header line —
# see scripts/lib/oracle-header.sh):
#   crates/almide-syntax/tests/golden/spec-ast-manifest.txt    sha256<TAB>path
#   crates/almide-syntax/tests/golden/spec-ast-exclusions.txt  path<TAB>reason
# No silent gaps: every spec/**/*.almd lands in exactly one of the two files.
#
# TWO ROOTS (ARCHITECTURE.md §4, "the judge is external"): the corpus is a
# PARTITION of this tree (implementation-resident spec/churn, spec/pass_isolated)
# and the almide/als submodule mount `als/` (everything the judge owns). The
# oracle runs with cwd = the fixture's root, so the RELATIVE path it sees — and
# embeds in its output — is the corpus path, identical to what the parity test
# hands the ported parser (almide-corpus resolves the same way).
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.." || exit 2

ORACLE="${ORACLE:?set ORACLE to the almide binary built from this tree (target/release/almide)}"
case "$ORACLE" in /*) ;; *) ORACLE="$PWD/$ORACLE" ;; esac
"$ORACLE" --version >/dev/null || exit 2
. scripts/lib/oracle-header.sh

OUT_DIR="crates/almide-syntax/tests/golden"
mkdir -p "$OUT_DIR"
MANIFEST="$OUT_DIR/spec-ast-manifest.txt"
EXCLUDED="$OUT_DIR/spec-ast-exclusions.txt"
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
emit_ast() { (cd "$1" && "$ORACLE" "$2" --emit-ast); }

n_ok=0; n_skip=0
for r in $ROOTS; do
  while IFS= read -r f; do
    if out="$(emit_ast "$r" "$f" 2>/tmp/ast-err.$$)"; then
      printf '%s\t%s\n' "$(printf '%s\n' "$out" | shasum -a 256 | cut -d' ' -f1)" "$f" >> "$MANIFEST"
      n_ok=$((n_ok + 1))
    else
      reason="$(head -1 /tmp/ast-err.$$ | tr '\t' ' ' | cut -c1-160)"
      printf '%s\t%s\n' "$f" "${reason:-nonzero exit}" >> "$EXCLUDED"
      n_skip=$((n_skip + 1))
    fi
  done <<< "$(cd "$r" && find spec -name '*.almd' | sort)"
done
rm -f /tmp/ast-err.$$
# One corpus order regardless of which root a file lives in (stable diffs).
sort -t$'\t' -k2 -o "$MANIFEST" "$MANIFEST"
sort -o "$EXCLUDED" "$EXCLUDED"

# Determinism spot-check: re-run the first 25 manifest entries and compare.
# Normalization on BOTH passes (and in the Rust gate): command substitution
# strips trailing newlines, printf '%s\n' re-adds exactly one.
ndet=0
while IFS=$'\t' read -r want f; do
  got="$(printf '%s\n' "$(emit_ast "$(root_of "$f")" "$f" 2>/dev/null)" | shasum -a 256 | cut -d' ' -f1)"
  [ "$got" = "$want" ] || { echo "::error::nondeterministic --emit-ast for $f"; ndet=$((ndet + 1)); }
done <<< "$(head -25 "$MANIFEST")"
[ "$ndet" -eq 0 ] || exit 1
stamp_oracle_header "$MANIFEST"

echo "manifest: $n_ok files, exclusions: $n_skip (see $EXCLUDED)"
