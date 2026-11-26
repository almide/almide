#!/usr/bin/env bash
# Probe only, not a gate: keep what the structural wall reads of each contract
# program, so that a rule can be tried against the shipped bytes without a
# build: the module, its witness bundle, its anchor file, the cutter's three
# sets of proposals (windows, fresh blocks, loops) and the bytes of the
# runtime routines the anchor file names.
#
#   scripts/probe-keep-corpus.sh <out-dir>
#
# Writes <out-dir>/m/<name>.{wasm,bundle,anchors,windows,fresh,loops,proved},
# <out-dir>/list.tsv (program, name), not-emitted.txt and cutter-failed.txt.
# Needs the release binaries built.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
OUT="${1:?usage: scripts/probe-keep-corpus.sh <out-dir>}"
mkdir -p "$OUT/m" "$OUT/bin"
OUT="$(cd "$OUT" && pwd)"
W="$OUT/m"
TD="${CARGO_TARGET_DIR:-$ROOT/target}/release"
ANCHORCHECK="$TD/almide-anchor-check"
PRODUCER="$TD/examples/emit_structural_bundle"
for b in "$TD/almide" "$PRODUCER" "$ANCHORCHECK"; do
  [ -x "$b" ] || { echo "not built: $b"; exit 1; }
done
# The driver refuses to write a module when it finds a verifier beside itself
# and that verifier rejects the certificate. A copy in a directory of its own
# has none beside it.
cp "$TD/almide" "$OUT/bin/almide"
ALMIDE="$OUT/bin/almide"
"$ALMIDE" --version
: > "$OUT/not-emitted.txt"; : > "$OUT/cutter-failed.txt"; : > "$OUT/list.tsv"
LIST="$OUT/programs.txt"
grep -v -E '^(#|$)' crates/almide-wasm/tests/golden/witness-fixtures.txt > "$LIST"
[ -z "${ONLY_FIRST:-}" ] || { head -n "$ONLY_FIRST" "$LIST" > "$LIST.cut"; mv "$LIST.cut" "$LIST"; }
echo "contract programs: $(wc -l < "$LIST" | tr -d ' ')"

one() {
  local f="$1" n
  n="$(printf '%s' "$f" | tr '/' '_')"
  if ! timeout 300 "$ALMIDE" build "$f" --target wasm -o "$W/$n.wasm" > "$W/$n.cerr" 2>&1 < /dev/null; then
    printf 'build\t%s\n' "$f" >> "$OUT/not-emitted.txt"; return 0
  fi
  if ! timeout 300 "$PRODUCER" "$f" --artifact "$W/$n.wasm" --anchors "$W/$n.anchors" > "$W/$n.bundle" 2> "$W/$n.perr" < /dev/null; then
    printf 'producer\t%s\n' "$f" >> "$OUT/not-emitted.txt"; return 0
  fi
  timeout 300 "$ANCHORCHECK" --windows "$W/$n.wasm" > "$W/$n.windows" 2> "$W/$n.werr" || printf 'windows\t%s\n' "$f" >> "$OUT/cutter-failed.txt"
  timeout 300 "$ANCHORCHECK" --fresh "$W/$n.wasm" "$W/$n.anchors" > "$W/$n.fresh" 2> "$W/$n.ferr" || printf 'fresh\t%s\n' "$f" >> "$OUT/cutter-failed.txt"
  timeout 300 "$ANCHORCHECK" --loops "$W/$n.wasm" "$W/$n.anchors" > "$W/$n.loops" 2> "$W/$n.lerr" || printf 'loops\t%s\n' "$f" >> "$OUT/cutter-failed.txt"
  timeout 300 "$ANCHORCHECK" --proved-bytes "$W/$n.wasm" "$W/$n.anchors" > "$W/$n.proved" 2> "$W/$n.rerr" || printf 'proved\t%s\n' "$f" >> "$OUT/cutter-failed.txt"
  printf '%s\t%s\n' "$f" "$n" >> "$OUT/list.tsv"
}
export -f one
export W OUT ALMIDE PRODUCER ANCHORCHECK
t0=$(date +%s)
xargs -P "${JOBS:-4}" -I{} bash -c 'one "$1"' _ {} < "$LIST"
sort -o "$OUT/list.tsv" "$OUT/list.tsv"
echo "kept: $(wc -l < "$OUT/list.tsv" | tr -d ' ') program(s) in $(( $(date +%s) - t0 )) s"
echo "not emitted: $(wc -l < "$OUT/not-emitted.txt" | tr -d ' ')   cutter failures: $(wc -l < "$OUT/cutter-failed.txt" | tr -d ' ')"
echo "modules: $(find "$W" -name '*.wasm' | wc -l | tr -d ' ')  bytes: $(find "$W" -name '*.wasm' -exec cat {} + | wc -c | tr -d ' ')"
echo "error files with text: $(find "$W" -name '*err' -size +0 | wc -l | tr -d ' ')"
git -C "$ROOT" rev-parse HEAD > "$OUT/revision.txt"
# An error file that holds nothing says nothing: only the ones with text go in.
( cd "$OUT" && { find m -type f ! -name '*err'; find m -type f -name '*err' -size +0; } | sort > "$OUT/packed.txt" )
printf '%s\n' list.tsv not-emitted.txt cutter-failed.txt programs.txt revision.txt >> "$OUT/packed.txt"
tar -C "$OUT" -czf "$OUT/corpus.tar.gz" -T "$OUT/packed.txt"
ls -l "$OUT/corpus.tar.gz"
[ "$(wc -l < "$OUT/list.tsv" | tr -d ' ')" -gt 0 ] || { echo "nothing was kept"; exit 1; }
