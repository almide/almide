#!/usr/bin/env bash
# TOOLCHAIN PINS GATE (#4010)
# ===========================
#
# Every proof in the spine sits on tools it does not check: rustc and the LLVM
# inside it build the native leg, and wasmtime runs the wasm leg in CI and in
# `almide run`. A trusted tool is only named honestly with its version, so this
# gate keeps scripts/toolchain-pins.txt equal to what the repository actually
# uses, and proofs/TRUSTED_BASE.md naming every version on it:
#
#   rustc <v>           a toolchain a workflow pins (an env RUST_TOOLCHAIN, a
#                       `toolchain:` input, or a workflow_dispatch default)
#   wasmtime-crate <v>  the embedded engine (crates/almide-wasm-run)
#   wasmtime-cli <v>    a stock runtime a workflow downloads
#   floating <file>:<job> <ref>
#                       a dtolnay/rust-toolchain step with no version: it runs
#                       whatever `stable` / `nightly` is that day. Honest debt,
#                       shrink-only: a new one fails, a pinned one is STALE.
#
# Found but not listed fails; listed but not found fails as STALE.
# TOOLCHAIN_PINS_ROOT points the gate at another tree (the negative control).
set -euo pipefail
export LC_ALL=C

ROOT="${TOOLCHAIN_PINS_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
PINS="$ROOT/scripts/toolchain-pins.txt"
DOC="$ROOT/proofs/TRUSTED_BASE.md"
WF="$ROOT/.github/workflows"
CRATE="$ROOT/crates/almide-wasm-run/Cargo.toml"

found=$(mktemp)
trap 'rm -f "$found"' EXIT

for f in "$WF"/*.yml; do
  base="$(basename "$f")"
  # rustc: the env pin, explicit toolchain inputs, dispatch defaults. A step
  # with no version is a floating site, named by its job.
  awk -v F="$base" '
    function strip(s) { sub(/#.*/, "", s); gsub(/["'"'"' ]/, "", s); return s }
    /^  RUST_TOOLCHAIN:/ { v = $0; sub(/^  RUST_TOOLCHAIN:/, "", v); env = strip(v); print "rustc " env }
    /^  [A-Za-z0-9_-]+:[[:space:]]*$/ && injobs { job = $1; sub(/:$/, "", job) }
    /^jobs:/ { injobs = 1 }
    /uses: dtolnay\/rust-toolchain@/ {
      ref = $0; sub(/.*@/, "", ref); ref = strip(ref)
      pending = 1; pjob = job; pref = ref; left = 4; next
    }
    pending && /^[[:space:]]+toolchain:/ {
      v = $0; sub(/^[[:space:]]+toolchain:/, "", v)
      if (v ~ /env\.RUST_TOOLCHAIN/) { pending = 0; next }
      if (v ~ /inputs\.toolchain/) { d = v; sub(/.*\|\|[[:space:]]*/, "", d); sub(/[[:space:]]*}}.*/, "", d); print "rustc " strip(d); pending = 0; next }
      print "rustc " strip(v); pending = 0; next
    }
    pending && (/^[[:space:]]*- / || --left == 0) {
      if (pref ~ /^(stable|nightly|beta)$/) print "floating " F ":" pjob " " pref
      else print "rustc " pref
      pending = 0
    }
  ' "$f"
  # wasmtime CLI: a release tarball name, or a VERSION= line inside a step
  # whose name says it installs wasmtime (other tools use VERSION= too).
  { grep -oE 'wasmtime-v[0-9]+\.[0-9]+\.[0-9]+' "$f" | sed 's/^wasmtime-v//' || true
    awk '/^[[:space:]]+- name:/ { step = tolower($0) }
         step ~ /wasmtime/ && match($0, /VERSION=v[0-9]+\.[0-9]+\.[0-9]+/) { print substr($0, RSTART + 9, RLENGTH - 9) }' "$f"
  } | sed 's/^/wasmtime-cli /'
done | sort -u > "$found"
grep -E '^wasmtime = "' "$CRATE" | sed -E 's/^wasmtime = "([^"]+)".*/wasmtime-crate \1/' >> "$found"
sort -u -o "$found" "$found"

listed=$({ grep -vE '^(#|[[:space:]]*$)' "$PINS" || true; } | awk '{ print $1 " " $2 ($1 == "floating" ? " " $3 : "") }' | sort -u)

fail=0
while read -r row; do
  [ -n "$row" ] || continue
  printf '%s\n' "$listed" | grep -qxF "$row" || {
    echo "::error::toolchain-pins: the repository uses '$row', which scripts/toolchain-pins.txt does not list — add it with its role (#4010)."
    fail=1
  }
done < "$found"
while read -r row; do
  [ -n "$row" ] || continue
  grep -qxF "$row" "$found" || {
    echo "::error::toolchain-pins: scripts/toolchain-pins.txt lists '$row', which nothing uses any more — STALE, delete the line."
    fail=1
  }
done <<< "$listed"

# The trusted-base ledger names every version (not the floating sites, which
# it names by count).
while read -r kind v; do
  case "$kind" in rustc|wasmtime-crate|wasmtime-cli) ;; *) continue ;; esac
  grep -qF "$v" "$DOC" || {
    echo "::error::toolchain-pins: proofs/TRUSTED_BASE.md does not name $kind $v — the ledger must list every trusted version."
    fail=1
  }
done < <({ grep -vE '^(#|[[:space:]]*$)' "$PINS" || true; } | awk '{ print $1, $2 }')

[ "$fail" = 0 ] || exit 1
n_float=$(grep -c '^floating ' "$found" || true)
echo "toolchain-pins: $(grep -vc '^floating ' "$found") pinned version(s), $n_float floating site(s), all listed and named in TRUSTED_BASE.md"
