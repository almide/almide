#!/usr/bin/env bash
# The per-FUNCTION wasm reachability ratchet.
#
# Runs tools/wasm_reachability_sweep.py and diffs the observed verdicts against
# proofs/wasm-reachability-baseline.txt:
#   - a fn that WALLS on wasm and is NOT in the baseline -> FAIL (parity
#     regression: a stdlib fn stopped existing on one of the two first-class
#     targets, or a new intrinsic landed native-only);
#   - a baseline fn that now builds on wasm -> FAIL as STALE (prune it in the
#     same change — the set only shrinks);
#   - a fn that became UNPROBEABLE without being listed -> FAIL. The sweep's own
#     coverage is ratcheted too: a synthesizer that quietly stops being able to
#     build a call would otherwise shrink what the gate can see while still
#     printing green.
#
# This gate is CI-only, not a pre-commit hook: it builds ~550 programs twice and
# takes minutes. `check-wasm-fallback.sh` stays the fast per-file signal.
set -uo pipefail
# Pin collation (#1031): the ratchet diff must not depend on the machine's sort.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BASELINE="$ROOT/proofs/wasm-reachability-baseline.txt"
BIN="${ALMIDE_BIN:-$ROOT/target/release/almide}"

if [ ! -x "$BIN" ]; then
  if [ "${CI:-}" = "true" ]; then
    echo "::error::wasm-reachability: no almide at $BIN — in CI a missing oracle is a failure (#978)" >&2
    exit 1
  fi
  echo "wasm-reachability: no almide at $BIN — SKIP"; exit 0
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
ALMIDE_BIN="$BIN" python3 "$ROOT/tools/wasm_reachability_sweep.py" --json "$TMP/sweep.json" \
  >"$TMP/out.txt" 2>"$TMP/progress.txt" || { echo "::error::sweep failed" >&2; cat "$TMP/progress.txt" >&2; exit 1; }

python3 - "$TMP/sweep.json" "$BASELINE" <<'PYEOF'
import json, sys
rows = json.load(open(sys.argv[1]))
listed = {}
for line in open(sys.argv[2]):
    line = line.strip()
    if not line or line.startswith("#"):
        continue
    fn, cls = (p.strip() for p in line.split("::")[:2])
    listed[fn] = cls

observed = {f"{r['module']}.{r['name']}": r["verdict"]
            for r in rows if r["verdict"] in ("GAP", "UNPROBEABLE")}
fail = 0
new = sorted(k for k in observed if k not in listed)
if new:
    print("WASM PARITY REGRESSION — fn(s) that do not reach wasm and are NOT in the baseline:", file=sys.stderr)
    for k in new:
        print(f"  + {k} ({observed[k]})", file=sys.stderr)
    fail = 1
stale = sorted(k for k in listed if k not in observed)
if stale:
    print("STALE baseline entr(ies) — these now build on wasm; prune them (the set only shrinks):", file=sys.stderr)
    for k in stale:
        print(f"  - {k}", file=sys.stderr)
    fail = 1
if fail:
    sys.exit(1)
parity = sum(1 for r in rows if r["verdict"] == "PARITY")
frontier = sum(1 for v in listed.values() if v == "COMPILER_FRONTIER")
host = sum(1 for v in listed.values() if v == "HOST_CAPABILITY")
unprobe = sum(1 for v in listed.values() if v == "UNPROBEABLE")
print(f"WASM REACHABILITY RATCHET OK: {parity} fn(s) at parity; "
      f"{frontier} frontier debt, {host} host-capability, {unprobe} unprobeable "
      f"(targets: frontier 0, unprobeable 0).")
PYEOF
