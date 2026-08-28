#!/usr/bin/env bash
# #1423 stage 2 — the target-availability gate: the declared single-leg
# surface (proofs/target-availability.toml) diffed BIDIRECTIONALLY against
# the measured reality (tools/target_availability_probe.py — the renderer's
# own verdict per public stdlib fn).
#
#   measured wall, no row      → FAIL (declare it, with a reason)
#   declared row, measures ok  → FAIL (stale row — delete it; the shrink
#                                 direction, a fn that started lowering)
#   row without a reason       → FAIL (the rustc mandatory-stability rule)
#   pending-self-host count    → shrink-only ratchet vs the committed ceiling
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
ALMIDE="$ALMIDE" python3 tools/target_availability_probe.py > "$tmp/measured.tsv"

python3 - "$tmp/measured.tsv" proofs/target-availability.toml <<'PY'
import re
import sys

measured_path, toml_path = sys.argv[1], sys.argv[2]
walls, oks = set(), set()
for line in open(measured_path):
    status, fn, _ = line.rstrip("\n").split("\t", 2)
    if status == "wall":
        walls.add(fn)
    elif status == "ok":
        oks.add(fn)

toml = open(toml_path).read()
ceiling = int(re.search(r"^pending_self_host_ceiling = (\d+)$", toml, re.M).group(1))
declared, reasons = set(), {}
for block in re.findall(r"\[\[native-only\]\]\n(?:[a-z]+ = .*\n)+", toml):
    fn = re.search(r'fn = "([^"]+)"', block).group(1)
    r = re.search(r'reason = "([^"]*)"', block)
    declared.add(fn)
    reasons[fn] = r.group(1) if r else ""

fail = 0
for fn in sorted(walls - declared):
    print(f"::error::measured native-only but UNDECLARED: {fn} — add its row (with a reason) to proofs/target-availability.toml")
    fail = 1
for fn in sorted(declared & oks):
    print(f"::error::declared native-only but it LOWERS now: {fn} — delete the stale row (the shrink direction)")
    fail = 1
for fn in sorted(declared):
    if not reasons[fn]:
        print(f"::error::reasonless declaration: {fn}")
        fail = 1
pending = sum(1 for fn in declared if reasons.get(fn) == "pending-self-host")
if pending > ceiling:
    print(f"::error::pending-self-host grew: {pending} > ceiling {ceiling} (the ratchet only shrinks)")
    fail = 1
if pending < ceiling:
    print(f"::error::pending-self-host shrank to {pending} — lower pending_self_host_ceiling to match (ratchet bookkeeping)")
    fail = 1

if not fail:
    print(f"target-availability OK: {len(oks)} lower, {len(declared)} declared native-only "
          f"(pending-self-host {pending}/{ceiling}), both directions agree.")
sys.exit(fail)
PY
