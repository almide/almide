#!/usr/bin/env bash
# GATE-VERIFICATION LEDGER GATE (Stage 5:「検証ツール自身の検証」).
#
# Every verdict-bearing gate script must carry a row in
# proofs/gate-verification.toml classifying HOW we know the gate can fail
# correctly (see that file's class vocabulary). A gate nobody has ever seen
# fail is possibly decorative — this ledger makes that debt visible and
# shrink-only, the same 直視-then-ratchet pattern as coverage and the
# abstain classes.
set -euo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LEDGER="$ROOT/proofs/gate-verification.toml"

# ── enumerate the verdict-bearing gates (same set the ledger was built from:
# every check-* script plus the named verdict/verify scripts; lib/ helpers and
# generated-doc helpers are not verdict-bearing) ──
enumerate() {
  (
    cd "$ROOT"
    ls scripts/check-*.sh
    ls scripts/release-seal.sh scripts/fuzz-night-verdict.sh
    ls proofs/*.sh | grep -v '^proofs/lib/'
  ) | sort -u
}

python3 - "$ROOT" "$LEDGER" <<'EOF'
import re
import subprocess
import sys

root, ledger_path = sys.argv[1], sys.argv[2]
enumerated = set(
    subprocess.run(
        ["bash", "-c",
         "cd \"$1\" && { ls scripts/check-*.sh; ls scripts/release-seal.sh "
         "scripts/fuzz-night-verdict.sh; ls proofs/*.sh | grep -v '^proofs/lib/'; } | sort -u",
         "_", root],
        capture_output=True, text=True, check=True).stdout.split())

VOCAB = {"KERNEL_PROVEN", "MUTATION_TESTED", "NEGATIVE_TESTED", "EXERCISED", "UNVERIFIED"}

ceiling = None
rows, cur = [], None
for raw in open(ledger_path, encoding="utf-8"):
    line = raw.strip()
    m = re.match(r'#\s*unverified_ceiling\s*=\s*"(\d+)"', line)
    if m:
        ceiling = int(m.group(1))
    if line == "[[gate]]":
        if cur:
            rows.append(cur)
        cur = {}
        continue
    m = re.match(r'([a-z_]+)\s*=\s*"(.*)"$', line)
    if m and cur is not None:
        cur[m.group(1)] = m.group(2)
if cur:
    rows.append(cur)

errs = []
if ceiling is None:
    errs.append("ledger header is missing `# unverified_ceiling = \"N\"`")
seen = set()
unverified = 0
for r in rows:
    p = r.get("path")
    if not p:
        errs.append(f"row without a path: {r}")
        continue
    if p in seen:
        errs.append(f"{p}: duplicate row")
    seen.add(p)
    if p not in enumerated:
        errs.append(f"{p}: STALE row — the script no longer exists (or left the enumerated set)")
    cls = r.get("class", "")
    if cls not in VOCAB:
        errs.append(f"{p}: unknown class {cls!r} (expected one of {sorted(VOCAB)})")
    if cls == "UNVERIFIED":
        unverified += 1
        if r.get("evidence"):
            errs.append(f"{p}: UNVERIFIED must not carry `evidence` — either it has evidence "
                        f"(then classify it) or it does not (then the row is bare)")
    elif not r.get("evidence"):
        errs.append(f"{p}: {cls} needs a durable `evidence` pointer — an unevidenced "
                    f"classification is exactly the drift this ledger exists to prevent")

for p in sorted(enumerated - seen):
    errs.append(f"{p}: UNCLASSIFIED — a verdict-bearing gate with no row. How would we "
                f"know if this gate can no longer fail?")

if ceiling is not None:
    if unverified > ceiling:
        errs.append(f"UNVERIFIED count {unverified} exceeds the ceiling {ceiling} — new gates "
                    f"must land with their self-verification evidence")
    elif unverified < ceiling:
        errs.append(f"UNVERIFIED count {unverified} is BELOW the ceiling {ceiling} — ratchet "
                    f"it down in the ledger header (the debt may only shrink, and the "
                    f"ledger must say so)")

if errs:
    print("GATE VERIFICATION LEDGER FAIL —", file=sys.stderr)
    for e in errs:
        print(f"  {e}", file=sys.stderr)
    sys.exit(1)

by = {}
for r in rows:
    by[r["class"]] = by.get(r["class"], 0) + 1
print("gate-verification OK: " + str(len(rows)) + " gate(s) — "
      + " / ".join(f"{k} {v}" for k, v in sorted(by.items()))
      + f" (unverified ceiling {ceiling})")
EOF
