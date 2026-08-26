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
# wasm_cross and wasm_fail are judge-owned: they live under the almide/als
# mount `als/`, and the oracle runs there so the corpus-relative path is the
# one the run-parity test hands the interpreter.
set -uo pipefail
export LC_ALL=C
export PATH="/opt/homebrew/bin:$PATH"
cd "$(dirname "$0")/.." || exit 2

ORACLE="${ORACLE:?set ORACLE to the almide binary built from the port SHA}"
case "$ORACLE" in /*) ;; *) ORACLE="$PWD/$ORACLE" ;; esac
OUT_DIR="$PWD/crates/almide-spine/tests/golden"
mkdir -p "$OUT_DIR"
MANIFEST="$OUT_DIR/spec-run-manifest.txt"
EXCLUDED="$OUT_DIR/spec-run-exclusions.txt"
: > "$MANIFEST"; : > "$EXCLUDED"
REPO_ROOT="$PWD"
# Judge-mounted form sweeps the als/ corpus; the main repo sweeps in-tree.
if [ -d als/spec/wasm_cross ]; then cd als || exit 2
else [ -d spec/wasm_cross ] || { echo "::error::spec/wasm_cross missing"; exit 2; }
fi

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

# The hand-maintained register of fixtures the oracle cannot referee (see its
# header): dropped from the manifest, carried into the exclusions with reason.
REGISTER="$REPO_ROOT/scripts/lib/run-oracle-exclusions.txt"
reg_file="/tmp/run-oracle-reg.$$"
grep -vE '^[[:space:]]*(#|$)' "$REGISTER" | cut -f1 > "$reg_file"
{ grep -v $'\tEXCLUDED\t' /tmp/run-manifest-raw.$$ \
    | awk -F'\t' -v rf="$reg_file" 'BEGIN{while ((getline l < rf) > 0) drop[l]=1} !($3 in drop)' \
    | sort -t$'\t' -k3; } > "$MANIFEST"
rm -f "$reg_file"
{ grep $'\tEXCLUDED\t' /tmp/run-manifest-raw.$$ | cut -f1,3 || true
  grep -vE '^[[:space:]]*(#|$)' "$REGISTER" | sed 's/\t/\tregister: /'; } | sort > "$EXCLUDED"
rm -f /tmp/run-manifest-raw.$$ /tmp/run-err.$$.* 2>/dev/null

echo "manifest: $(wc -l < "$MANIFEST" | tr -d ' ') files, exclusions: $(wc -l < "$EXCLUDED" | tr -d ' ')"
