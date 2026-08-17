#!/usr/bin/env bash
# Integer-domain edge gate — the measured matrix, diffed BIDIRECTIONALLY against
# the declared ledger.
#
# THE LAW THIS ENFORCES (../almide-references/RESEARCH-integer-domain-guards.md)
# -----------------------------------------------------------------------------
# A guard must not be defeatable by its own arithmetic. No compiler of the nine
# surveyed enforces how a guard is PHRASED — clippy's lints for it are
# allow-by-default and not enabled on rustc's own source, and Zig has no lint
# layer at all — so this repo cannot copy a lint. What it can copy is Rust
# `tidy`'s shape (src/tools/tidy/src/target_policy.rs:27-62): discover the family
# by WALKING the implementation, subtract what is covered, fail on the remainder,
# and name every exception so each hole is attributed rather than silent.
#
# BOTH DIRECTIONS, because one direction is how the class kept coming back:
#
#   measured-but-undeclared  -> a NEW divergence. Fail. This is the bug catcher.
#   declared-but-not-measured -> a row that no longer diverges. Fail, and delete
#                                the row. Without this half the ledger becomes a
#                                list of things that used to be true, and the
#                                count stops meaning anything.
#
# The DIVERGE count is a shrink-only ceiling: it may go down, never up.
#
# WHY A SEPARATE INSTRUMENT FROM THE FUZZER
# -----------------------------------------
# The fuzzer already owns the extreme pool (generator/pools.rs:76 has i64::MAX,
# i64::MIN, both i32 rails, u32::MAX) and already derives the function catalogue
# by parsing every bundled module (generator/catalogue.rs:165). It does not cross
# them, on purpose: generator/term.rs:363-365 feeds any parameter whose NAME is
# count-like a value from {0,1,2,3,4,5}, because a `repeat` of u32::MAX
# manufactures an out-of-memory "hang" that is noise rather than a finding.
#
# That decision is right and stays. It also draws the blind spot exactly over the
# parameters the room guards read — `pos` is not in the name list, so
# `bytes.set_f32_le` was found by fuzzing; `size` is, so `bytes.chunks` never
# could be, and it took an inventory to see it. This gate covers what the lottery
# structurally cannot.
#
# Usage:
#   scripts/check-domain-edges.sh              # gate against proofs/domain-edges.toml
#   scripts/check-domain-edges.sh --update     # re-measure and rewrite the ledger
#   scripts/check-domain-edges.sh --only bytes # one module (fast local loop)

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LEDGER="$REPO/proofs/domain-edges.toml"
ALMIDE="${ALMIDE_BIN:-$REPO/target/release/almide}"
MEASURED="$(mktemp -t domain-edges-XXXXXX.json)"
trap 'rm -f "$MEASURED"' EXIT

UPDATE=0
ONLY=""
while [ $# -gt 0 ]; do
  case "$1" in
    --update) UPDATE=1; shift ;;
    --only)   ONLY="$2"; shift 2 ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
done

if [ ! -x "$ALMIDE" ]; then
  echo "::error::$ALMIDE not built — run 'cargo build --release' first"
  exit 1
fi

echo "domain-edges: measuring${ONLY:+ (module $ONLY)} …"
python3 "$REPO/tools/domain_edge_matrix.py" \
  --json "$MEASURED" --almide "$ALMIDE" ${ONLY:+--only "$ONLY"} >/dev/null || true

python3 - "$MEASURED" "$LEDGER" "$UPDATE" "$ONLY" <<'PY'
import json, sys, pathlib, re

measured_path, ledger_path, update, only = sys.argv[1], sys.argv[2], sys.argv[3] == "1", sys.argv[4]
cells = json.load(open(measured_path))["cells"]
key = lambda c: f'{c["module"]}.{c["fn"]}:{c["param"]}:{c["edge"]}'
measured = {key(c) for c in cells if c["verdict"] == "DIVERGE"}

ledger = pathlib.Path(ledger_path)
declared, reasons = set(), {}
if ledger.exists():
    for m in re.finditer(r'cell\s*=\s*"([^"]+)"\s*\nreason\s*=\s*"([^"]*)"', ledger.read_text()):
        declared.add(m.group(1))
        reasons[m.group(1)] = m.group(2)

if update:
    rows = "".join(
        f'\n[[divergence]]\ncell   = "{k}"\nreason = "{reasons.get(k, "UNTRIAGED — measured divergent, no cause recorded yet")}"\n'
        for k in sorted(measured))
    ledger.parent.mkdir(parents=True, exist_ok=True)
    ledger.write_text(
        "# Integer-domain edge divergences — MEASURED, not asserted.\n"
        "#\n"
        "# Regenerate with `scripts/check-domain-edges.sh --update`. Every row must earn a\n"
        "# `reason`; an UNTRIAGED row is a bug nobody has looked at yet, and the count is a\n"
        "# shrink-only ceiling. A row that stops diverging must be DELETED in the same PR\n"
        "# that fixes it — the gate fails on a stale row exactly as it fails on a new one,\n"
        "# because a ledger of things that used to be true measures nothing.\n"
        f"\ncount = {len(measured)}\n" + rows)
    print(f"domain-edges: ledger rewritten — {len(measured)} divergent cells")
    sys.exit(0)

# Scoping: a --only run can only speak for the module it measured.
if only:
    declared = {k for k in declared if k.startswith(only + ".")}

new = sorted(measured - declared)
stale = sorted(declared - measured)
fail = False

if new:
    fail = True
    print(f"::error::{len(new)} UNDECLARED divergent cell(s) — a guard was defeated by its own arithmetic, or a new one was written that way:")
    for k in new[:25]:
        print(f"    {k}")
    if len(new) > 25:
        print(f"    … and {len(new) - 25} more")
    print("  Fix it, or record it with a reason: scripts/check-domain-edges.sh --update")

if stale:
    fail = True
    print(f"::error::{len(stale)} ledger row(s) no longer diverge — delete them (the count is shrink-only):")
    for k in stale[:25]:
        print(f"    {k}")

untriaged = [k for k in sorted(declared & measured) if reasons.get(k, "").startswith("UNTRIAGED")]
if untriaged:
    print(f"domain-edges: {len(untriaged)} declared cell(s) still UNTRIAGED (not a failure, but they are open bugs)")

if not fail:
    print(f"domain-edges: OK — {len(measured)} divergent cells, all declared.")
sys.exit(1 if fail else 0)
PY
