#!/usr/bin/env bash
# Auto-generate docs/specs/als/README.md — the ALS section index.
#   bash docs/specs/als/generate-readme.sh > docs/specs/als/README.md
# One row per `## ALS-<id>` heading across the normative chapters, in chapter
# order: id, title, chapter, citing contracts (from the ledger). The
# conformance report (docs/contracts/conformance.md) is the fixture-level
# join; this page is the table of contents. Freshness is gated in CI.
set -euo pipefail
export LC_ALL=C
cd "$(dirname "$0")/../../.." || exit 2
python3 - <<'PY'
import re, glob, collections
cites = collections.defaultdict(list)
for b in re.split(r'\[\[contract\]\]', open("docs/contracts/contracts.toml", encoding="utf-8").read())[1:]:
    i = re.search(r'id\s*=\s*"(C-\d+)"', b); s = re.search(r'^spec\s*=\s*"([^"]+)"', b, re.M)
    if i and s: cites[s.group(1)].append(i.group(1))
print("# ALS — Section Index\n")
print("> Auto-generated from the chapter files and [the contract ledger](../../contracts/contracts.toml).")
print("> Run `bash docs/specs/als/generate-readme.sh > docs/specs/als/README.md` to update.\n")
files = sorted(glob.glob("docs/specs/als/*.md"))
files = [f for f in files if not f.endswith("README.md")]
total = 0
for f in files:
    rows = []
    for line in open(f, encoding="utf-8"):
        m = re.match(r'##\s+(ALS-[A-Z]+\d+[a-z]?)\s+(.*)', line)
        if m: rows.append((m.group(1), m.group(2).strip()))
    if not rows: continue
    base = f.split("/")[-1]
    print(f"## {base} — {len(rows)} section(s)\n")
    print("| ID | Section | Contracts |")
    print("|----|---------|-----------|")
    for sid, title in rows:
        total += 1
        # The SAME slugger as scripts/check-links.sh — one anchor law.
        h = re.sub(r'[!"#$%&\'()*+,./:;<=>?@\[\]^{|}~`]', '', f"{sid} {title}".strip().lower())
        anchor = re.sub(r'\s', '-', h)
        c = ", ".join(cites.get(sid, [])) or "—"
        print(f"| [{sid}](./{base}#{anchor}) | {title} | {c} |")
    print()
print(f"{total} sections across {len(files)} chapters.")
PY
