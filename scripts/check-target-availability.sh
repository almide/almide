#!/usr/bin/env bash
# #1423 stages 2+3 / #1710 increment 2 — the PER-LEG target-availability
# gate: the declared unavailable surface of each service leg
# (proofs/target-availability.toml, schema 2) diffed BIDIRECTIONALLY
# against the measured reality (tools/target_availability_probe.py
# --leg <leg> — the renderer's / run path's own verdict per public
# stdlib fn). Per leg:
#
#   measured wall, leg not declared → FAIL (declare it, with a reason)
#   declared leg, measures ok       → FAIL (stale — remove the leg; the
#                                     shrink direction, a leg that
#                                     started serving)
#   leg without a reason            → FAIL (mandatory-stability rule)
#   pending-self-host count         → shrink-only ratchet vs the ceiling
#
# Legs swept here: structural, stock-p1, embedded. The p3-component leg
# joins with the wasi:http@0.3 port (#1710) — no sweep, no rows yet.
# The EMBEDDED sweep EXECUTES probes (slower); CI-only by default is
# deliberate: locally set AVAIL_EMBEDDED=1 to include it, or =0 in CI to
# skip it while iterating.
#
# Tool policy (#921): locally a missing binary is an honest skip; in CI a
# failure. The probe needs the built almide binary (ALMIDE env or
# target/release/almide).
set -euo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ALMIDE="${ALMIDE:-$PWD/target/release/almide}"
if ! "$ALMIDE" --version >/dev/null 2>&1; then
  if [ "${CI:-}" = "true" ]; then
    echo "::error::check-target-availability: almide binary not found"
    exit 1
  fi
  echo "check-target-availability: almide not built — SKIP"
  exit 0
fi
command -v python3 >/dev/null 2>&1 || { echo "python3 missing"; exit 1; }
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT

if [ "${AVAIL_EMBEDDED:-}" = "1" ] || [ "${CI:-}" = "true" ] && [ "${AVAIL_EMBEDDED:-}" != "0" ]; then
  LEGS="structural stock-p1 embedded"
else
  LEGS="structural stock-p1"
fi
for leg in $LEGS; do
  ALMIDE="$ALMIDE" python3 tools/target_availability_probe.py --leg "$leg" > "$tmp/$leg.tsv"
done

python3 - "$tmp" proofs/target-availability.toml $LEGS <<'PY'
import re
import sys

tmp, toml_path, legs = sys.argv[1], sys.argv[2], sys.argv[3:]
toml = open(toml_path).read()
ceiling = int(re.search(r"^pending_self_host_ceiling = (\d+)$", toml, re.M).group(1))

# declared[leg] = {fn}; reasons[(fn, leg)] = reason
declared = {leg: set() for leg in legs}
reasons = {}
row_count = 0
for block in re.findall(r"\[\[unavailable\]\]\n(?:[a-z0-9_-]+ = .*\n)+", toml):
    fn = re.search(r'fn = "([^"]+)"', block).group(1)
    row_count += 1
    row_legs = re.findall(r'"([a-z0-9-]+)"', re.search(r"legs = \[(.*)\]", block).group(1))
    shared = re.search(r'^reason = "([^"]*)"$', block, re.M)
    for leg in row_legs:
        per = re.search(rf'^reason-{leg} = "([^"]*)"$', block, re.M)
        reasons[(fn, leg)] = (per or shared).group(1) if (per or shared) else ""
        if leg in declared:
            declared[leg].add(fn)

fail = 0
for leg in legs:
    walls, oks = set(), set()
    for line in open(f"{tmp}/{leg}.tsv"):
        status, fn, _ = line.rstrip("\n").split("\t", 2)
        if status == "wall":
            walls.add(fn)
        elif status == "ok":
            oks.add(fn)
    for fn in sorted(walls - declared[leg]):
        print(f"::error::[{leg}] measured wall but UNDECLARED: {fn} — add \"{leg}\" to its row (with a reason) in proofs/target-availability.toml")
        fail = 1
    for fn in sorted(declared[leg] & oks):
        print(f"::error::[{leg}] declared unavailable but it SERVES now: {fn} — remove the leg from its row (the shrink direction)")
        fail = 1
    for fn in sorted(declared[leg]):
        if not reasons.get((fn, leg)):
            print(f"::error::[{leg}] reasonless declaration: {fn}")
            fail = 1

pending = sum(1 for (fn, leg), r in reasons.items() if r == "pending-self-host" and leg == "structural")
if pending > ceiling:
    print(f"::error::pending-self-host grew: {pending} > ceiling {ceiling} (the ratchet only shrinks)")
    fail = 1
if pending < ceiling:
    print(f"::error::pending-self-host shrank to {pending} — lower pending_self_host_ceiling to match (ratchet bookkeeping)")
    fail = 1

if not fail:
    per = ", ".join(f"{leg}={len(declared[leg])}" for leg in legs)
    print(f"target-availability OK ({row_count} rows; declared walls per leg: {per}; "
          f"pending-self-host {pending}/{ceiling}; two directions agree per swept leg).")
sys.exit(fail)
PY
