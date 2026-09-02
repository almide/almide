#!/usr/bin/env bash
# #1423 stages 2+3 / #1710 increment 2 / #1827 — the PER-LEG
# target-availability gate: the declared unavailable surface of each
# service leg (proofs/target-availability.toml, schema 2) diffed
# BIDIRECTIONALLY against the measured reality (tools/target_availability_probe.py
# --leg <leg> — the renderer's / run path's own verdict per public stdlib
# fn, enumerated from the compiler's own module interface so the WHOLE
# surface is swept, #1827). Per leg:
#
#   measured wall, leg not declared → FAIL (declare it, with a reason)
#   declared leg, measures ok       → FAIL (stale — remove the leg; the
#                                     shrink direction, a leg that
#                                     started serving)
#   leg without a reason            → FAIL (mandatory-stability rule)
#   pending-self-host count         → shrink-only ratchet vs the ceiling
#   probe `error` line              → FAIL (a public fn the synthesizer
#                                     could not build a probe for — the
#                                     probe is the thing to fix; a silent
#                                     skip is how #1827 hid 515 fns)
#   probe `unprobed` lines          → printed per leg, held to the leg's
#                                     unprobed_ceiling_<leg> both ways
#                                     (the leg walled on an argument
#                                     CONSTRUCTOR the probe injected, so
#                                     the fn itself is claimed neither
#                                     way — never allowed to grow silently)
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
# The probe exits non-zero when a fn could not be probed; its `error`
# lines are in the TSV and the diff below reports them by name, so the
# sweep continues to the report instead of dying on set -e here.
for leg in $LEGS; do
  ALMIDE="$ALMIDE" python3 tools/target_availability_probe.py --leg "$leg" > "$tmp/$leg.tsv" || true
done

python3 - "$tmp" proofs/target-availability.toml $LEGS <<'PY'
import re
import sys

tmp, toml_path, legs = sys.argv[1], sys.argv[2], sys.argv[3:]
toml = open(toml_path).read()
ceiling = int(re.search(r"^pending_self_host_ceiling = (\d+)$", toml, re.M).group(1))


def unprobed_ceiling(leg):
    key = "unprobed_ceiling_" + leg.replace("-", "_")
    m = re.search(rf"^{key} = (\d+)$", toml, re.M)
    if not m:
        print(f"::error::proofs/target-availability.toml has no `{key}` — every swept leg carries its unprobed ceiling")
        return None
    return int(m.group(1))


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
probed = {}
for leg in legs:
    walls, oks, unprobed, errors = set(), set(), {}, {}
    for line in open(f"{tmp}/{leg}.tsv"):
        status, fn, detail = line.rstrip("\n").split("\t", 2)
        if status == "wall":
            walls.add(fn)
        elif status == "ok":
            oks.add(fn)
        elif status == "unprobed":
            unprobed[fn] = detail
        elif status == "error":
            errors[fn] = detail
    probed[leg] = len(walls) + len(oks) + len(unprobed) + len(errors)
    for fn in sorted(errors):
        print(f"::error::[{leg}] NOT PROBED — the synthesizer has no probe program for {fn}: {errors[fn]} (fix tools/target_availability_probe.py; a public fn is never skipped)")
        fail = 1
    for fn in sorted(walls - declared[leg]):
        print(f"::error::[{leg}] measured wall but UNDECLARED: {fn} — add \"{leg}\" to its row (with a reason) in proofs/target-availability.toml")
        fail = 1
    for fn in sorted(declared[leg] & oks):
        print(f"::error::[{leg}] declared unavailable but it SERVES now: {fn} — remove the leg from its row (the shrink direction)")
        fail = 1
    for fn in sorted(declared[leg] & set(unprobed)):
        print(f"::error::[{leg}] declared unavailable but UNPROBED now: {fn} — {unprobed[fn]}; the row claims what the probe cannot measure, remove the leg or fix the probe")
        fail = 1
    for fn in sorted(declared[leg]):
        if not reasons.get((fn, leg)):
            print(f"::error::[{leg}] reasonless declaration: {fn}")
            fail = 1
    for fn in sorted(unprobed):
        print(f"::notice::[{leg}] unprobed {fn}: {unprobed[fn]}")
    cap = unprobed_ceiling(leg)
    if cap is None:
        fail = 1
    elif len(unprobed) > cap:
        print(f"::error::[{leg}] unprobed grew: {len(unprobed)} > unprobed_ceiling {cap} — a fn the probe cannot reach must be reached (extend the synthesizer) or the ceiling raised consciously with a dated comment")
        fail = 1
    elif len(unprobed) < cap:
        print(f"::error::[{leg}] unprobed shrank to {len(unprobed)} — lower unprobed_ceiling_{leg.replace('-', '_')} to match (ratchet bookkeeping)")
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
    swept = ", ".join(f"{leg}={probed[leg]}" for leg in legs)
    print(f"target-availability OK ({row_count} rows; fns swept per leg: {swept}; declared walls per leg: {per}; "
          f"pending-self-host {pending}/{ceiling}; two directions agree per swept leg).")
sys.exit(fail)
PY
