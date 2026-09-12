#!/usr/bin/env bash
# GATE-VERIFICATION LEDGER GATE (Stage 5:「検証ツール自身の検証」).
#
# Every verdict-bearing gate must carry a row in
# proofs/gate-verification.toml classifying HOW we know the gate can fail
# correctly (see that file's class vocabulary). A gate nobody has ever seen
# fail is possibly decorative — this ledger makes that debt visible and
# shrink-only, the same 直視-then-ratchet pattern as coverage and the
# abstain classes.
#
# #2128 adds a SECOND axis to the same rows: what the gate READS. The roadmap
# decision is that a gate reading structured data (TOML / markdown / ledgers /
# generated-output comparison) is written in Almide, and a gate that greps and
# exits stays bash — a cost asymmetry, not taste: the 60-odd check-*.sh each
# re-derive their own grep/awk/sed, while the Almide side already paid for a
# typed TOML and markdown reader that the next structured gate gets free. A
# rule alone did not hold before (the previous "no committed .sh" goal shipped
# with a done-criterion nobody implemented, and committed shell went 50 -> 124
# in six weeks), so the boundary is a check: a row that declares `structured`
# whose path is a .sh FAILS.
#
# The Almide gates are enumerated too — they had no rows at all, so an Almide
# gate could never carry a verification class.
set -euo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LEDGER="$ROOT/proofs/gate-verification.toml"

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

# The Almide gates: every subcommand the tool dispatches, read from the dispatch
# itself so the two cannot drift. They are addressed as `tools/almide-gates:<cmd>`
# — a path plus the argument that selects the gate, which is what a reader needs
# to run it.
GATES_MAIN = "tools/almide-gates/src/main.almd"
main_src = open(f"{root}/{GATES_MAIN}", encoding="utf-8").read()
body = main_src[main_src.index("let body = match cmd {"):]
subcommands = sorted(set(re.findall(r'^\s{4}"([a-z0-9-]+)" =>', body, re.M)))
if len(subcommands) < 5:
    print(f"GATE VERIFICATION LEDGER FAIL — only {len(subcommands)} almide-gates subcommand(s) "
          f"found in {GATES_MAIN}; the dispatch shape changed and the enumeration went blind",
          file=sys.stderr)
    sys.exit(1)
enumerated |= {f"tools/almide-gates:{c}" for c in subcommands}

VOCAB = {"KERNEL_PROVEN", "MUTATION_TESTED", "NEGATIVE_TESTED", "EXERCISED", "UNVERIFIED"}
# What the gate READS, which decides the language it belongs in (#2128).
READS = {"structured", "scalar", "UNCLASSIFIED"}

ceiling = None
unclassified_ceiling = None
rows, cur = [], None
for raw in open(ledger_path, encoding="utf-8"):
    line = raw.strip()
    m = re.match(r'#\s*unverified_ceiling\s*=\s*"(\d+)"', line)
    if m:
        ceiling = int(m.group(1))
    m = re.match(r'#\s*unclassified_reads_ceiling\s*=\s*"(\d+)"', line)
    if m:
        unclassified_ceiling = int(m.group(1))
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
if unclassified_ceiling is None:
    errs.append("ledger header is missing `# unclassified_reads_ceiling = \"N\"`")
seen = set()
unverified = 0
unclassified = 0
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
    reads = r.get("reads", "")
    if reads not in READS:
        errs.append(f"{p}: unknown reads {reads!r} (expected one of {sorted(READS)}) — "
                    f"a new gate must declare what it reads")
    elif reads == "UNCLASSIFIED":
        unclassified += 1
    elif reads == "structured" and p.endswith(".sh"):
        errs.append(f"{p}: declares `reads = \"structured\"` and is a .sh — a gate that reads "
                    f"TOML / markdown / a ledger / generated output belongs in Almide, where the "
                    f"typed readers already exist. Port it, or say what it actually reads.")

for p in sorted(enumerated - seen):
    errs.append(f"{p}: UNCLASSIFIED — a verdict-bearing gate with no row. How would we "
                f"know if this gate can no longer fail?")

if unclassified_ceiling is not None:
    if unclassified > unclassified_ceiling:
        errs.append(f"UNCLASSIFIED reads count {unclassified} exceeds the ceiling "
                    f"{unclassified_ceiling} — a new gate must declare what it reads")
    elif unclassified < unclassified_ceiling:
        errs.append(f"UNCLASSIFIED reads count {unclassified} is BELOW the ceiling "
                    f"{unclassified_ceiling} — ratchet it down in the ledger header (the debt "
                    f"may only shrink, and the ledger must say so)")

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
reads_by = {}
for r in rows:
    reads_by[r.get("reads", "?")] = reads_by.get(r.get("reads", "?"), 0) + 1
print("gate-verification OK: " + str(len(rows)) + " gate(s) — "
      + " / ".join(f"{k} {v}" for k, v in sorted(by.items()))
      + f" (unverified ceiling {ceiling})")
print("  reads: " + " / ".join(f"{k} {v}" for k, v in sorted(reads_by.items()))
      + f" (unclassified ceiling {unclassified_ceiling})")
EOF
