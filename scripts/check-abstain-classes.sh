#!/usr/bin/env bash
# Aviation-quality Stage 2: the abstain-classification gate + the 3-way
# coverage meter.
#
# Every abstain reason in crates/almide-interp/interp-abstain-ledger.txt must
# match exactly one class pattern in interp-abstain-classes.toml (first match
# wins) — a NEW abstain reason without a class fails, so the executable-spec
# boundary can never grow unclassified. Prints the coverage arithmetic an
# auditor reads: voting fixtures / corpus, and the near-horizon target
# (voting + BRIDGEABLE). With --update, appends a dated row to
# research/benchmark/three-way/README.md.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

python3 - "$ROOT" "${1:-}" <<'EOF'
import glob
import re
import sys
from datetime import datetime, timezone

root, flag = sys.argv[1], sys.argv[2]
ledger = f"{root}/crates/almide-interp/interp-abstain-ledger.txt"
classes_p = f"{root}/crates/almide-interp/interp-abstain-classes.toml"

classes = []  # (name, [patterns]) in file order
name = None
for raw in open(classes_p, encoding="utf-8"):
    line = raw.strip()
    m = re.match(r'name\s*=\s*"([A-Z_]+)"', line)
    if m:
        name = m.group(1)
        classes.append((name, []))
    m = re.match(r'"(.+)",?\s*$', line)
    if m and classes:
        classes[-1][1].append(m.group(1))

# The class vocabulary is PINNED (#1244 burn-down: the first negative probe
# renamed HEAP_BOUNDARY to a bogus class and the gate stayed green — the
# breakdown printed the bogus name and the near-horizon arithmetic, which
# reads BRIDGEABLE BY NAME, would silently miscount on any canonical-name
# drift). A new class is a deliberate taxonomy change: add it HERE and in
# the toml in the same PR.
VOCAB = {"HEAP_BOUNDARY", "BRIDGEABLE"}
bad = [n for n, _ in classes if n not in VOCAB]
dupes = {n for n, _ in classes if [x for x, _ in classes].count(n) > 1}
if bad or dupes or not classes:
    for n in bad:
        print(f"::error::unknown abstain class {n!r} (vocabulary: {sorted(VOCAB)})")
    for n in sorted(dupes):
        print(f"::error::duplicate abstain class {n!r}")
    if not classes:
        print("::error::no classes parsed from interp-abstain-classes.toml (anchor drift?)")
    print("abstain-classes FAILED — class table invalid.")
    sys.exit(1)

counts = {n: 0 for n, _ in classes}
unclassified = []
entries = 0
for raw in open(ledger, encoding="utf-8"):
    line = raw.strip()
    if not line or line.startswith("#"):
        continue
    entries += 1
    reason = line.split(None, 1)[1] if len(line.split(None, 1)) > 1 else ""
    for n, pats in classes:
        if any(p in reason for p in pats):
            counts[n] += 1
            break
    else:
        unclassified.append(line)

corpus = len(glob.glob(f"{root}/spec/wasm_cross/*.almd"))
voting = corpus - entries
near = voting + counts.get("BRIDGEABLE", 0)

if unclassified:
    for u in unclassified:
        print(f"::error::unclassified abstain: {u} — add a pattern to interp-abstain-classes.toml")
    print(f"abstain-classes FAILED — {len(unclassified)} unclassified entr(ies).")
    sys.exit(1)

pct = lambda n: f"{100.0 * n / corpus:.1f}%"
breakdown = ", ".join(f"{n}={c}" for n, c in counts.items())
print(
    f"abstain-classes OK — corpus {corpus}, voting {voting} ({pct(voting)}), "
    f"abstaining {entries} ({breakdown}); near-horizon target {near} ({pct(near)})."
)

if flag == "--update":
    import os
    d = f"{root}/research/benchmark/three-way"
    os.makedirs(d, exist_ok=True)
    p = f"{d}/README.md"
    today = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    rows = []
    if os.path.exists(p):
        rows = [l for l in open(p, encoding="utf-8") if re.match(r"^\| \d{4}-", l)]
    with open(p, "w", encoding="utf-8") as f:
        f.write(
            "# 3-way oracle coverage (aviation-quality Stage 2)\n\n"
            "Translation validation the auditor can count: of the spec/wasm_cross\n"
            "corpus, how many fixtures cast a REAL native/wasm/interp 3-way vote\n"
            "(`voting`), and what the near-horizon target is once the BRIDGEABLE\n"
            "abstains are burned down (the HEAP_BOUNDARY remainder is the\n"
            "interp-heap arc). Classes: crates/almide-interp/interp-abstain-classes.toml;\n"
            "gate + meter: `scripts/check-abstain-classes.sh` (append a row with `--update`).\n\n"
            "| measured (UTC) | corpus | voting | HEAP_BOUNDARY | BRIDGEABLE | near-horizon |\n"
            "|---|---|---|---|---|---|\n"
        )
        for r in rows:
            f.write(r)
        f.write(
            f"| {today} | {corpus} | {voting} ({pct(voting)}) | "
            f"{counts.get('HEAP_BOUNDARY', 0)} | {counts.get('BRIDGEABLE', 0)} | "
            f"{near} ({pct(near)}) |\n"
        )
    print(f"ledger updated: {p}")
EOF
