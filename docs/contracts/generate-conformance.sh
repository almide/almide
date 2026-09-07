#!/usr/bin/env bash
# Auto-generate docs/contracts/conformance.md — the ALS conformance report (F1,
# #811): every normative ALS section, the contracts that cite it, and the
# EXECUTABLE fixtures that exercise each, with how CI executes them.
#
#   bash docs/contracts/generate-conformance.sh > docs/contracts/conformance.md
#   bash docs/contracts/generate-conformance.sh --counts > docs/contracts/conformance.md
#                          # also restamp proofs/ledger-counts.toml first
#   bash docs/contracts/generate-conformance.sh --measure   # print "sections fixtures"
#
# The totals line ("N normative sections; N distinct executable fixtures.") is
# NOT re-derived on a default run: it is rendered from the stamped record in
# proofs/ledger-counts.toml inside a dated `counts:generated` block, so a
# fixture PR regenerates the rows and leaves the totals alone (two fixture PRs
# conflicted on that one line at every merge). `--measure` is the one code path
# that computes them — scripts/lib/ledger-counts.sh calls it when stamping.
#
# The report is DERIVED — the ledger (contracts.toml) is the single source of
# truth, and `scripts/check-contracts.sh` already enforces that every section is
# cited, every active contract carries >= fixture-class evidence, and every
# fixture link is bidirectional. This report joins those facts per section so an
# auditor reads one page instead of re-deriving the join. A freshness check in
# check-contracts.sh keeps it from drifting, the same discipline as README.md.
#
# The "How CI runs it" column is derived from the fixture's PATH:
#   spec/wasm_cross/*.almd    cross-target byte-compare (wasm_runtime_cross_target:
#                             native stdout/exit == wasm stdout/exit)
#   spec/**_test.almd, spec/* `almide test` on both targets (Test Rust / Test
#                             WASM CI jobs)
#   tests/diagnostics/*       checker-reject harness (broken.almd must produce
#                             the pinned code+hint; fixed.almd must compile)
#   tests/*.rs                a Rust gate (cargo test)
set -euo pipefail

# Byte-order collation, pinned: `sort`'s last-resort comparison follows the ambient
# locale, so an unpinned sort produces different output on differently-configured
# machines. #1031 caught docs/roadmap/README.md changing row order with no content change.
export LC_ALL=C
cd "$(dirname "$0")/../.." || exit 2

LEDGER="docs/contracts/contracts.toml"
MODE="${1:-}"
. scripts/lib/ledger-counts.sh

# The join, in one place. `measure` prints the two totals; `table` prints the rows.
report() {
python3 - "$LEDGER" "$1" << 'PYEOF'
import re, sys, collections

src = open(sys.argv[1]).read()
mode = sys.argv[2]
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

if mode == 'measure':
    n_sections = len(sections)
    n_fixtures = len({p for rows in sections.values() for _, ps in rows for p in ps})
    print(f'{n_sections} {n_fixtures}')
    sys.exit(0)

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
PYEOF
}

case "$MODE" in
  --measure) report measure; exit 0 ;;
  --counts)  counts_stamp ;;
  "") ;;
  *) echo "::error::unknown flag $MODE (expected --counts or --measure)" >&2; exit 2 ;;
esac

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
> The totals line below is STAMPED (`proofs/ledger-counts.toml`, dated in its
> block) and refreshed only by `bash scripts/gen-ledger-counts.sh` — a fixture
> PR regenerates the rows and leaves it alone.

HEADER

counts_render_conformance
echo
report table
