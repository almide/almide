#!/usr/bin/env bash
# Probe only, not a gate: at a few sites of the adjacency campaign, what the
# byte-level check reads when the emitting call is deleted and the recording
# call kept, and the reverse. For each site and one program: the witness
# bundle, the anchor file and the check's own account of its matching, under
# the unmutated compiler and under each of the two mutants.
#
#   scripts/probe-adjacency-explain.sh <out-dir>
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
OUT="${1:?usage: scripts/probe-adjacency-explain.sh <out-dir>}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
TD="${CARGO_TARGET_DIR:-$ROOT/target}/release"
ALMIDE="$TD/almide"
PRODUCER="$TD/examples/emit_structural_bundle"

# site, program: four sites where the check accepts the emitting call's
# deletion, and one where it rejects it.
PAIRS="
006 spec/wasm_cross/map_fold_heap_acc_skv.almd
025 spec/wasm_cross/list_zip_with_typed.almd
028 spec/wasm_cross/effect_mut_generic_port.almd
037 spec/wasm_cross/args_surface.almd
002 spec/wasm_cross/alias_combinator_rc.almd
"

python3 -I scripts/probe-adjacency-pick.py "$OUT/pick" 0 1 || exit 1
cargo build -q --release --locked -p almide-anchor-check || exit 1
mkdir -p "$OUT/bin"
cp "$TD/almide-anchor-check" "$OUT/bin/almide-anchor-check"
CHECK="$OUT/bin/almide-anchor-check"

build() {
  cargo build -q --release --locked --bin almide && cargo build -q --release --locked -p almide-wasm --example emit_structural_bundle
  # The driver must not find a verifier beside itself: it would refuse to
  # write a module whose witness the verifier rejects.
  rm -f "$TD/almide-verify"
}

# one <label> <program> <dir>
one() {
  local label="$1" f="$2" d="$3" rc=0
  mkdir -p "$d"
  if ! "$ALMIDE" build "$f" --target wasm -o "$d/m.wasm" > "$d/build.err" 2>&1 < /dev/null; then
    echo "   [$label] the build refuses: $(head -n 2 "$d/build.err" | tr '\n' ' ' | cut -c1-200)"; return
  fi
  if ! "$PRODUCER" "$f" --artifact "$d/m.wasm" --anchors "$d/m.anchors" > "$d/m.bundle" 2> "$d/producer.err" < /dev/null; then
    echo "   [$label] the producer refuses: $(head -n 2 "$d/producer.err" | tr '\n' ' ' | cut -c1-200)"; return
  fi
  "$CHECK" --explain "$d/m.wasm" "$d/m.bundle" "$d/m.anchors" > "$d/check.out" 2>&1 || rc=$?
  echo "   [$label] module $(sha256sum < "$d/m.wasm" | cut -c1-12), bundle $(sha256sum < "$d/m.bundle" | cut -c1-12) ($(wc -l < "$d/m.bundle" | tr -d ' ') lines), anchors $(sha256sum < "$d/m.anchors" | cut -c1-12) ($(wc -l < "$d/m.anchors" | tr -d ' ') lines); the check exits $rc"
  grep -E '^(RC |ANCHORS |.*REJECT)' "$d/check.out" | cut -c1-400 | head -n 6 | sed 's/^/      /'
}

build || { echo "the unmutated compiler does not build"; exit 1; }
echo "$PAIRS" | while read -r site f; do
  [ -n "$site" ] || continue
  n="$(printf '%s' "$f" | tr '/' '_')"
  one base "$f" "$OUT/$site/base" > "$OUT/$site.base.txt" 2>&1
done

for v in 1 2; do
  echo "$PAIRS" | while read -r site f; do
    [ -n "$site" ] || continue
    id="m${site}${v}"
    kind="the emitting call deleted"; [ "$v" = 2 ] && kind="the recording call deleted"
    echo "=================== site $site, $kind ($id): $f"
    grep -P "^$id\t" "$OUT/pick/sample.tsv" | cut -f5 | sed 's/^/   /'
    if ! git apply "$OUT/pick/$id.diff"; then echo "   the diff does not apply"; continue; fi
    if build; then
      cat "$OUT/$site.base.txt"
      one mutant "$f" "$OUT/$site/v$v"
      for x in bundle anchors; do
        if [ -f "$OUT/$site/v$v/m.$x" ]; then
          echo "   --- $x, unmutated against mutant (first 40 changed lines):"
          diff "$OUT/$site/base/m.$x" "$OUT/$site/v$v/m.$x" | grep -E '^[<>]' | cut -c1-260 | head -n 40 | sed 's/^/      /'
        fi
      done
      if [ -f "$OUT/$site/v$v/check.out" ]; then
        echo "   --- the check's account, unmutated against mutant (first 60 changed lines):"
        diff "$OUT/$site/base/check.out" "$OUT/$site/v$v/check.out" | grep -E '^[<>]' | cut -c1-300 | head -n 60 | sed 's/^/      /'
      fi
    else
      echo "   the mutated compiler does not build"
    fi
    git apply -R "$OUT/pick/$id.diff"
  done
done
