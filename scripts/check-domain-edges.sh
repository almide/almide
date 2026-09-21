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
# THE SAME TWO DIRECTIONS FOR THE SKIPPED POPULATION (#2402)
# ---------------------------------------------------------
# A slot the matrix cannot build is coverage the matrix does not have, and the
# count of those was printed for a month and never read back — it held
# `json.index(path: JsonPath, i: Int)`, the signature that carried #2396. So a
# skip is now a ledger row with a name and a reason, and the diff runs on it:
#
#   skipped-but-undeclared   -> coverage LOST (a type left the tables, a probe
#                                stopped compiling). Fail.
#   declared-but-now-built   -> coverage regained. Fail, and delete the row, for
#                                the same reason a stale divergence row fails.
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
# could be. This gate covers what the lottery structurally cannot.
#
# Usage:
#   scripts/check-domain-edges.sh                 # gate against proofs/domain-edges.toml
#   scripts/check-domain-edges.sh --update        # re-measure and rewrite the ledger
#   scripts/check-domain-edges.sh --only bytes    # one module (fast local loop)
#   scripts/check-domain-edges.sh --skips-only    # the population and its skips, no edge run
#   scripts/check-domain-edges.sh --measured F    # diff an existing tool JSON (tests)
#
# `--update --only M` rewrites only M's rows; the other modules' rows and the
# hand-written header are kept.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LEDGER="$REPO/proofs/domain-edges.toml"
ALMIDE="${ALMIDE_BIN:-$REPO/target/release/almide}"
MEASURED=""
OWN_MEASURED=""

UPDATE=0
ONLY=""
SKIPS_ONLY=""
while [ $# -gt 0 ]; do
  case "$1" in
    --update)     UPDATE=1; shift ;;
    --only)       ONLY="$2"; shift 2 ;;
    --skips-only) SKIPS_ONLY=1; shift ;;
    --measured)   MEASURED="$2"; shift 2 ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
done

if [ -z "$MEASURED" ]; then
  if [ ! -x "$ALMIDE" ]; then
    echo "::error::$ALMIDE not built — run 'cargo build --release' first"
    exit 1
  fi
  OWN_MEASURED="$(mktemp -t domain-edges-XXXXXX.json)"
  MEASURED="$OWN_MEASURED"
  trap 'rm -f "$OWN_MEASURED"' EXIT
  echo "domain-edges: measuring${ONLY:+ (module $ONLY)}${SKIPS_ONLY:+ (skips only)} …"
  python3 "$REPO/tools/domain_edge_matrix.py" \
    --json "$MEASURED" --almide "$ALMIDE" ${ONLY:+--only "$ONLY"} ${SKIPS_ONLY:+--skips-only} \
    | grep -E "^  (coverage|outside the public surface):" || true
fi

python3 - "$MEASURED" "$LEDGER" "$UPDATE" "$ONLY" <<'PY'
import json, sys, pathlib, re

measured_path, ledger_path, update, only = sys.argv[1], sys.argv[2], sys.argv[3] == "1", sys.argv[4]
data = json.load(open(measured_path))
skips_only = bool(data.get("skips_only"))
cells = data["cells"]
key = lambda c: f'{c["module"]}.{c["fn"]}:{c["param"]}:{c["edge"]}'
measured = {key(c) for c in cells if c["verdict"] == "DIVERGE"}
measured_skips = {s["slot"]: s["reason"] for s in data.get("skipped", [])}

ledger = pathlib.Path(ledger_path)
declared, reasons, declared_skips, header = set(), {}, {}, ""
if ledger.exists():
    text = ledger.read_text()
    header = text.split("\ncount =", 1)[0]
    for m in re.finditer(r'\[\[divergence\]\]\ncell\s*=\s*"([^"]+)"\s*\nreason\s*=\s*"([^"]*)"', text):
        declared.add(m.group(1))
        reasons[m.group(1)] = m.group(2)
    for m in re.finditer(r'\[\[skip\]\]\nslot\s*=\s*"([^"]+)"\s*\nreason\s*=\s*"([^"]*)"', text):
        declared_skips[m.group(1)] = m.group(2)

in_scope = (lambda k: k.startswith(only + ".")) if only else (lambda k: True)

if update:
    # Rows outside the measured scope are carried over untouched; rows inside
    # it are replaced by what was measured. A --skips-only run replaces only
    # the skip rows.
    if not skips_only:
        kept = {k: v for k, v in reasons.items() if not in_scope(k)}
        for k in sorted(measured):
            kept[k] = reasons.get(k, "UNTRIAGED — measured divergent, no cause recorded yet")
    else:
        kept = reasons
    kept_skips = {k: v for k, v in declared_skips.items() if not in_scope(k)}
    kept_skips.update(measured_skips)
    rows = "".join(f'\n[[divergence]]\ncell   = "{k}"\nreason = "{kept[k]}"\n' for k in sorted(kept))
    skip_rows = "".join(f'\n[[skip]]\nslot   = "{k}"\nreason = "{kept_skips[k]}"\n' for k in sorted(kept_skips))
    if not header:
        header = (
            "# Integer-domain edge divergences — MEASURED, not asserted.\n"
            "#\n"
            "# Regenerate with `scripts/check-domain-edges.sh --update`. Every row must earn a\n"
            "# `reason`; an UNTRIAGED row is a bug nobody has looked at yet, and the count is a\n"
            "# shrink-only ceiling. A row that stops diverging must be DELETED in the same PR\n"
            "# that fixes it — the gate fails on a stale row exactly as it fails on a new one,\n"
            "# because a ledger of things that used to be true measures nothing.\n")
    ledger.parent.mkdir(parents=True, exist_ok=True)
    # Both counts precede the first table: a top-level key after `[[divergence]]`
    # rows would parse as a key of the last row.
    ledger.write_text(
        f"{header}\ncount = {len(kept)}\nskip_count = {len(kept_skips)}\n" + rows +
        "\n# ── Skipped slots: coverage the matrix does NOT have, by name (#2402) ──────\n"
        "# One row per public Int-parameter slot the tool could not build, render, or\n"
        "# type-check, with the measured reason (`skip_count` above). The gate diffs\n"
        "# these both ways too: a slot that BECOMES unbuildable is coverage lost and\n"
        "# fails; a slot that becomes buildable must have its row deleted.\n"
        + skip_rows)
    print(f"domain-edges: ledger rewritten — {len(kept)} divergent cells, {len(kept_skips)} named skips")
    sys.exit(0)

# Scoping: a --only run can only speak for the module it measured.
declared = {k for k in declared if in_scope(k)}
declared_skips = {k: v for k, v in declared_skips.items() if in_scope(k)}

fail = False

if not skips_only:
    new = sorted(measured - declared)
    stale = sorted(declared - measured)
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

new_skips = sorted(set(measured_skips) - set(declared_skips))
regained = sorted(set(declared_skips) - set(measured_skips))
if new_skips:
    fail = True
    print(f"::error::{len(new_skips)} UNDECLARED skipped slot(s) — coverage the matrix had (or should have) and now does not:")
    for k in new_skips[:25]:
        print(f"    {k}: {measured_skips[k]}")
    if len(new_skips) > 25:
        print(f"    … and {len(new_skips) - 25} more")
    print("  Make the slot buildable (VALUES / RENDER in tools/domain_edge_matrix.py), or declare it by name: scripts/check-domain-edges.sh --update --skips-only")
if regained:
    fail = True
    print(f"::error::{len(regained)} declared skip row(s) are now buildable — delete them (coverage regained must be recorded):")
    for k in regained[:25]:
        print(f"    {k}")

exercised = len(data.get("exercised", []))
total = exercised + len(measured_skips)
scope = f" (module {only})" if only else ""
print(f"domain-edges: coverage {exercised} of {total} signatures exercised{scope}, "
      f"{len(measured_skips)} skipped" + ("" if new_skips or regained else " — every skip declared by name"))
if not fail:
    if skips_only:
        print(f"domain-edges: OK — skip ledger matches (no edge was run).")
    else:
        print(f"domain-edges: OK — {len(measured)} divergent cells, all declared.")
sys.exit(1 if fail else 0)
PY
