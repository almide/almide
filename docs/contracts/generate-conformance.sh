#!/usr/bin/env bash
# Auto-generate docs/contracts/conformance.md — the ALS conformance report (F1,
# #811): every normative ALS section, the contracts that cite it, and the
# EXECUTABLE fixtures that exercise each, with how CI executes them.
#
#   bash docs/contracts/generate-conformance.sh > docs/contracts/conformance.md
#
# The report is DERIVED — the ledger (contracts.toml) is the single source of
# truth, and `scripts/check-contracts.sh` already enforces that every section is
# cited, every active contract carries >= fixture-class evidence, and every
# fixture link is bidirectional. This report joins those facts per section so an
# auditor reads one page instead of re-deriving the join. A freshness check in
# check-contracts.sh keeps it from drifting, the same discipline as README.md.
#
# The "How CI runs it" column is derived from the fixture's PATH:
#   spec/wasm_cross/*.almd    cross-target byte-compare (wasm_runtime_test:
#                             native stdout/exit == wasm stdout/exit)
#   spec/**_test.almd, spec/* `almide test` on both targets (Test Rust / Test
#                             WASM CI jobs)
#   tests/diagnostics/*       checker-reject harness (broken.almd must produce
#                             the pinned code+hint; fixed.almd must compile)
#   tests/*.rs                a Rust gate (cargo test)
set -euo pipefail
cd "$(dirname "$0")/../.." || exit 2

LEDGER="docs/contracts/contracts.toml"

cat << 'HEADER'
# ALS Conformance Report

> Auto-generated from [contracts.toml](contracts.toml).
> Run `bash docs/contracts/generate-conformance.sh > docs/contracts/conformance.md` to update.
>
> One row per normative ALS section: the contracts citing it and the executable
> fixtures exercising it. Every fixture below runs in CI — `spec/wasm_cross`
> fixtures as a native↔wasm byte-compare, `spec/` test files on both targets,
> `tests/diagnostics` through the checker harness, `tests/*.rs` under cargo.
> A section with no executable fixture would fail `scripts/check-contracts.sh`
> (spec-coverage + evidence-class >= fixture for every active contract), so this
> page cannot legitimately contain an empty Fixtures cell.

HEADER

python3 - "$LEDGER" << 'PY'
import re, sys, collections

src = open(sys.argv[1]).read()
blocks = re.split(r'\[\[contract\]\]', src)[1:]

def how(path):
    if path.startswith('spec/wasm_cross/'):
        return 'byte-compare'
    if path.startswith('tests/diagnostics/'):
        return 'checker'
    if path.startswith('spec/'):
        return 'both-target test'
    if path.startswith('tests/') and path.endswith('.rs'):
        return 'cargo gate'
    return 'other'

sections = collections.defaultdict(list)
for b in blocks:
    m = re.search(r'id\s*=\s*"(C-\d+)"', b)
    if not m:
        continue
    cid = m.group(1)
    spec = re.search(r'spec\s*=\s*"([^"]+)"', b)
    spec = spec.group(1) if spec else '?'
    paths = re.findall(r'path\s*=\s*"([^"]+)"[^}]*class\s*=\s*"(?:fixture|exhaustive)"', b)
    sections[spec].append((cid, paths))

def sort_key(s):
    m = re.match(r'ALS-([A-Z]+)(\d+)', s)
    return (m.group(1), int(m.group(2))) if m else (s, 0)

n_sections = len(sections)
n_fixtures = len({p for rows in sections.values() for _, ps in rows for p in ps})
print(f'{n_sections} normative sections; {n_fixtures} distinct executable fixtures.')
print()
print('| Section | Contracts | Fixtures (how CI runs each) |')
print('|---------|-----------|------------------------------|')
for spec in sorted(sections, key=sort_key):
    rows = sections[spec]
    cids = ', '.join(cid for cid, _ in rows)
    cells = []
    for _, ps in rows:
        for p in ps:
            cells.append(f'`{p}` ({how(p)})')
    # de-dup fixtures shared between contracts of one section, keep order
    seen, fixture_cell = set(), []
    for c in cells:
        if c not in seen:
            seen.add(c)
            fixture_cell.append(c)
    print(f'| {spec} | {cids} | {"<br>".join(fixture_cell)} |')
PY
