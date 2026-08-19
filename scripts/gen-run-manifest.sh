#!/usr/bin/env bash
# Run-parity golden generator (unit 3). Executes every spec/wasm_cross
# fixture on the ORACLE binary's WASM leg (`almide run <f> --target wasm`) —
# legitimate as the reference because wasm_cross fixtures are BY DEFINITION
# cross-target byte-identical under the incumbent's own CI — and records
# sha256(stdout) + exit code. The greenfield interpreter must reproduce both.
#
#   ORACLE=/path/to/almide bash scripts/gen-run-manifest.sh
#
# Requires wasmtime (memory: /opt/homebrew/bin off the sandbox PATH).
set -uo pipefail
export LC_ALL=C
export PATH="/opt/homebrew/bin:$PATH"
cd "$(dirname "$0")/.." || exit 2

ORACLE="${ORACLE:?set ORACLE to the almide binary built from the port SHA}"
OUT_DIR="crates/almide-spine/tests/golden"
mkdir -p "$OUT_DIR"
MANIFEST="$OUT_DIR/spec-run-manifest.txt"
EXCLUDED="$OUT_DIR/spec-run-exclusions.txt"
: > "$MANIFEST"; : > "$EXCLUDED"

run_one() {
  f="$1"
  out="$("$ORACLE" run "$f" --target wasm 2>/tmp/run-err.$$.$RANDOM)"; rc=$?
  if [ "$rc" -ge 2 ]; then
    printf '%s\tEXCLUDED\toracle exit %s\n' "$f" "$rc"
  else
    if [ -n "$out" ]; then h="$(printf '%s\n' "$out" | shasum -a 256 | cut -d' ' -f1)"
    else h="$(printf '' | shasum -a 256 | cut -d' ' -f1)"; fi
    printf '%s\t%s\t%s\n' "$h" "$rc" "$f"
  fi
}
export -f run_one; export ORACLE

find spec/wasm_cross spec/wasm_fail -name '*.almd' | sort \
  | xargs -P 8 -I{} bash -c 'run_one "$@"' _ {} > /tmp/run-manifest-raw.$$

grep -v $'\tEXCLUDED\t' /tmp/run-manifest-raw.$$ | sort -t$'\t' -k3 > "$MANIFEST"
grep $'\tEXCLUDED\t' /tmp/run-manifest-raw.$$ | cut -f1,3 | sort > "$EXCLUDED" || true
rm -f /tmp/run-manifest-raw.$$ /tmp/run-err.$$.* 2>/dev/null

echo "manifest: $(wc -l < "$MANIFEST" | tr -d ' ') files, exclusions: $(wc -l < "$EXCLUDED" | tr -d ' ')"
