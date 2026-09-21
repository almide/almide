#!/usr/bin/env bash
# Run-parity golden generator (unit 3). Executes every spec/wasm_cross
# fixture on the ORACLE binary's WASM leg (`almide run <f> --target wasm`) —
# legitimate as the reference because wasm_cross fixtures are BY DEFINITION
# cross-target byte-identical under the incumbent's own CI — and records
# sha256(stdout) + exit code. The greenfield interpreter must reproduce both.
# The oracle is the CLI built from THIS tree (target/release/almide; #2183):
# CI regenerates and diffs the outputs (scripts/check-parity-goldens.sh), so
# a committed row can only change by the CLI's own output changing. A fixture
# the CLI itself cannot run (exit >= 2: a build wall, a trap) lands in the
# exclusions with that exit — there is no hand-maintained register.
#
#   ORACLE=target/release/almide bash scripts/gen-run-manifest.sh
#
# Outputs (committed; the manifest starts with one `# oracle:` header line —
# see scripts/lib/oracle-header.sh):
#   crates/almide-spine/tests/golden/spec-run-manifest.txt     sha256<TAB>exit<TAB>path
#   crates/almide-spine/tests/golden/spec-run-exclusions.txt   path<TAB>reason
#
# Requires wasmtime (memory: /opt/homebrew/bin off the sandbox PATH).
# wasm_cross and wasm_fail are judge-owned: they live under the almide/als
# mount `als/`, and the oracle runs there so the corpus-relative path is the
# one the run-parity test hands the interpreter.
set -uo pipefail
export LC_ALL=C
export PATH="/opt/homebrew/bin:$PATH"
cd "$(dirname "$0")/.." || exit 2

ORACLE="${ORACLE:?set ORACLE to the almide binary built from this tree (target/release/almide)}"
case "$ORACLE" in /*) ;; *) ORACLE="$PWD/$ORACLE" ;; esac
"$ORACLE" --version >/dev/null || exit 2
. scripts/lib/oracle-header.sh
# #2405: a stale oracle, a foreign binary, a worktree behind upstream or an
# untracked fixture is refused HERE, before anything is truncated.
refuse_stale_tree || exit $?
OUT_DIR="$PWD/crates/almide-spine/tests/golden"
mkdir -p "$OUT_DIR"
MANIFEST="$OUT_DIR/spec-run-manifest.txt"
EXCLUDED="$OUT_DIR/spec-run-exclusions.txt"
: > "$MANIFEST"; : > "$EXCLUDED"
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

grep -v $'\tEXCLUDED\t' /tmp/run-manifest-raw.$$ | sort -t$'\t' -k3 > "$MANIFEST"
{ grep $'\tEXCLUDED\t' /tmp/run-manifest-raw.$$ | cut -f1,3 || true; } | sort > "$EXCLUDED"
rm -f /tmp/run-manifest-raw.$$ /tmp/run-err.$$.* 2>/dev/null
stamp_oracle_header "$MANIFEST"

echo "manifest: $(wc -l < "$MANIFEST" | tr -d ' ') files, exclusions: $(wc -l < "$EXCLUDED" | tr -d ' ')"
