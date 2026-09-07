#!/usr/bin/env bash
# OPTIMIZATION PASS ROSTER GATE (#929).
#
# docs/ARCHITECTURE.md carries the per-target optimization pass roster — which
# pass runs on which leg, and whether a target-specific one is so by design or
# merely never ported. A hand-maintained roster drifts the moment a pass is
# added or renamed, so this gate re-derives the enumerable half from the code
# on every run and fails when the section does not name it:
#
#   almide-codegen  every `pass_*.rs` file basename, and every `NanoPass::name()`
#                   string (`fn name(&self) -> &str { "X" }`) anywhere in the
#                   crate's src — wrappers in pass.rs included;
#   almide-optimize every pass module (`src/optimize/*.rs` minus mod.rs,
#                   `src/mono`, `src/mutual_tco.rs`), and every axis name the
#                   `ALMIDE_ONLY_PASS` switch accepts (parsed from the
#                   `matches!(name, "fold" | ...)` line in optimize/mod.rs).
#
# The check is membership inside the roster SECTION only (from its heading to
# the next `## `), so a name mentioned elsewhere in the document cannot stand
# in for a roster row. Blind-scan floors guard the enumeration itself: zero
# pass files, zero pass names, or zero axis names is a hard FAIL (a moved
# directory must not turn the gate decorative). The retiring almide-mir crate
# (#1696) and the structural emitter's routes are documented in the same
# section but NOT enumerated here — their rows leave with the retirement and
# are not pass files by shape.
set -euo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DOC="$ROOT/docs/ARCHITECTURE.md"
CODEGEN="$ROOT/crates/almide-codegen/src"
OPTIMIZE="$ROOT/crates/almide-optimize/src"
HEADING="## Optimization pass roster per target"

[ -f "$DOC" ] || { echo "PASS ROSTER FAIL — $DOC missing" >&2; exit 1; }
[ -d "$CODEGEN" ] || { echo "PASS ROSTER FAIL — $CODEGEN missing" >&2; exit 1; }
[ -d "$OPTIMIZE" ] || { echo "PASS ROSTER FAIL — $OPTIMIZE missing" >&2; exit 1; }

# ── the roster section, heading to the next top-level heading ──
section="$(awk -v h="$HEADING" '
  $0 == h { on = 1; next }
  on && /^## / { exit }
  on { print }
' "$DOC")"
if [ -z "$section" ]; then
  echo "PASS ROSTER FAIL — section \"$HEADING\" not found in docs/ARCHITECTURE.md" >&2
  exit 1
fi

fail=0
checked=0
# has NEEDLE: fixed-string membership in the section (here-string, no pipe to
# race — the check-contracts SIGPIPE lesson).
has() { grep -qF -- "$1" <<<"$section"; }
require() { # $1 = what, $2 = needle
  checked=$((checked + 1))
  if ! has "$2"; then
    echo "  $1 $2 is missing from the roster section" >&2
    fail=1
  fi
}

# ── almide-codegen: pass files ──
files=0
while IFS= read -r f; do
  files=$((files + 1))
  require "codegen pass file" "$(basename "$f")"
done < <(find "$CODEGEN" -maxdepth 1 -name 'pass_*.rs' | sort)
if [ "$files" -eq 0 ]; then
  echo "PASS ROSTER FAIL — no pass_*.rs files found under crates/almide-codegen/src (moved?)" >&2
  exit 1
fi

# ── almide-codegen: NanoPass names ──
names=0
while IFS= read -r n; do
  names=$((names + 1))
  require "codegen NanoPass name" "\`$n\`"
done < <(grep -rhoE 'fn name\(&self\) -> &str \{ *"[^"]+"' "$CODEGEN" | sed -E 's/.*"([^"]+)"/\1/' | sort -u)
if [ "$names" -eq 0 ]; then
  echo "PASS ROSTER FAIL — no NanoPass::name() strings found under crates/almide-codegen/src (signature changed?)" >&2
  exit 1
fi

# ── almide-optimize: pass modules ──
mods=0
while IFS= read -r f; do
  mods=$((mods + 1))
  require "optimize pass module" "$(basename "$f")"
done < <(find "$OPTIMIZE/optimize" -maxdepth 1 -name '*.rs' ! -name 'mod.rs' | sort)
for extra in "$OPTIMIZE/mono" "$OPTIMIZE/mutual_tco.rs"; do
  if [ -e "$extra" ]; then
    mods=$((mods + 1))
    require "optimize pass module" "$(basename "$extra")"
  fi
done
if [ "$mods" -eq 0 ]; then
  echo "PASS ROSTER FAIL — no pass modules found under crates/almide-optimize/src (moved?)" >&2
  exit 1
fi

# ── almide-optimize: ALMIDE_ONLY_PASS axis names ──
axis=0
while IFS= read -r a; do
  axis=$((axis + 1))
  require "ALMIDE_ONLY_PASS axis name" "\`$a\`"
done < <(grep -hoE 'matches!\(name, *"[^)]*\)' "$OPTIMIZE/optimize/mod.rs" | grep -oE '"[a-z_]+"' | tr -d '"' | sort -u)
if [ "$axis" -eq 0 ]; then
  echo "PASS ROSTER FAIL — no ALMIDE_ONLY_PASS axis names parsed from optimize/mod.rs (the matches! line moved?)" >&2
  exit 1
fi

if [ "$fail" -ne 0 ]; then
  echo "PASS ROSTER FAIL — a pass exists in the code that docs/ARCHITECTURE.md's roster does not name (#929). Add its row to the section \"$HEADING\"." >&2
  exit 1
fi
echo "pass-roster OK: $checked name(s) all rowed ($files codegen pass files, $names NanoPass names, $mods optimize modules, $axis axis names)"
