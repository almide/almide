#!/usr/bin/env bash
# ALS SYNTAX-ELEMENT COVERAGE GATE (Stage 3, the element→section direction).
# See proofs/als-element-coverage.toml's header for the model. The freeze
# precondition is UNWRITTEN = 0; until then the ceiling is shrink-only.
set -uo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LEDGER="$ROOT/proofs/als-element-coverage.toml"

python3 - "$ROOT" "$LEDGER" <<'EOF'
import glob
import importlib.util
import re
import sys

root, ledger_path = sys.argv[1], sys.argv[2]
spec = importlib.util.spec_from_file_location(
    "enum", f"{root}/scripts/lib/als-element-enumerate.py")
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
elements = set(mod.enumerate_elements(root))

# The resolvable ALS section ids: any `ALS-<id>` token in a heading line of
# the normative docs (the same resolution the contract spec-keying uses).
sections = set()
for p in glob.glob(f"{root}/docs/specs/als/*.md"):
    for line in open(p, encoding="utf-8"):
        if line.startswith("#"):
            sections.update(re.findall(r"ALS-[A-Z]*\d+[a-z]?", line))

ceiling = None
rows, cur = [], None
for raw in open(ledger_path, encoding="utf-8"):
    line = raw.strip()
    m = re.match(r'#\s*unwritten_ceiling\s*=\s*"(\d+)"', line)
    if m:
        ceiling = int(m.group(1))
    if line == "[[element]]":
        if cur:
            rows.append(cur)
        cur = {}
        continue
    m = re.match(r'(\w+)\s*=\s*"(.*)"$', line)
    if m and cur is not None:
        cur[m.group(1)] = m.group(2)
if cur:
    rows.append(cur)

errs = []
if ceiling is None:
    errs.append("ledger header is missing `# unwritten_ceiling = \"N\"`")
seen = set()
unwritten = 0
for r in rows:
    n = r.get("name")
    if not n:
        errs.append(f"row without a name: {r}")
        continue
    if n in seen:
        errs.append(f"{n}: duplicate row")
    seen.add(n)
    if n not in elements:
        errs.append(f"{n}: STALE row — no such variant in the surface AST any more")
    s = r.get("section", "")
    if s == "UNWRITTEN":
        unwritten += 1
    elif s not in sections:
        errs.append(f"{n}: section {s!r} does not resolve to any ALS heading in docs/specs/als/")

for n in sorted(elements - seen):
    errs.append(f"{n}: UNCLASSIFIED — a syntax element with no coverage row. A new "
                f"element lands with its ALS row (UNWRITTEN is honest) in the same PR.")

if ceiling is not None:
    if unwritten > ceiling:
        errs.append(f"UNWRITTEN count {unwritten} exceeds the ceiling {ceiling}")
    elif unwritten < ceiling:
        errs.append(f"UNWRITTEN count {unwritten} is BELOW the ceiling {ceiling} — ratchet it down")

if errs:
    print("ALS ELEMENT COVERAGE FAIL —", file=sys.stderr)
    for e in errs:
        print(f"  {e}", file=sys.stderr)
    sys.exit(1)
written = len(rows) - unwritten
print(f"als-element-coverage OK: {len(rows)} element(s) — {written} sectioned, "
      f"{unwritten} UNWRITTEN (ceiling {ceiling}; freeze precondition: 0)")
EOF
