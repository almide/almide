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
#
# #2234 adds a FOURTH axis: what the gate MEASURES, and whether that quantity
# has ever been seen to VARY. The perf ratchet's four `ablation/*` rows passed
# every negative control for three months while measuring a constant (the
# optimizer-ablated binary was byte-identical to the optimized one), because
# the ledger recorded only that the gate could fail on a forged input, never
# that its measurand exists. `measures` names the quantity and the artifact
# it is read from (required on every row); `varies` records the observation
# that the quantity took two different values (a commit, a PR, a run). A
# RATCHET — a gate whose script compares against a committed `*-baseline.txt`,
# an in-source `MAX_*=` ceiling or a `*_ceiling` — must carry `varies`; any
# other row may leave it empty under the shrink-only `# unvaried_ceiling`.
set -euo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LEDGER="$ROOT/proofs/gate-verification.toml"

python3 - "$ROOT" "$LEDGER" <<'EOF'
import os
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

# THE THIRD AXIS (#2207): who RUNS the gate. A NEGATIVE_TESTED row on a script
# nobody invokes is the decorative gate this ledger exists to expose, and three
# of them sat here for a month (check-cap-effect-consistency, check-pass-isolated,
# check-domain-edges: rows with evidence, no consumer). `wired_by` names the
# consumer(s) — a workflow, the hook file, the Makefile, another script, a test
# — and each named file must exist AND actually mention the gate (the script's
# basename; for `tools/almide-gates:<cmd>`, an almide-gates invocation of that
# subcommand). `UNWIRED` is the honest-debt spelling, counted under the
# shrink-only `# unwired_ceiling` like the other two debts.
UNWIRED = "UNWIRED"

def names_gate(consumer_text, gate_path):
    if gate_path.startswith("tools/almide-gates:"):
        cmd = gate_path.split(":", 1)[1]
        return re.search(r"almide-gates\S*\s+(?:--\s+)?" + re.escape(cmd) + r"\b", consumer_text) is not None
    return os.path.basename(gate_path) in consumer_text

# THE FOURTH AXIS (#2234): a ratchet is a gate that compares a measurement
# against a committed anchor. Detected from the gate's own text so the rule
# cannot be opted out of by omission: a `*-baseline.txt` reference, an
# in-source `MAX_<NAME>=<digits>` ceiling, or a `<name>_ceiling`. Almide gates
# are not scanned (none of them is a ratchet today; output-parity's baseline
# is read by its .sh original, which is).
RATCHET_RE = re.compile(r'-baseline\.txt|\bMAX_[A-Z_]+=[0-9]|_ceiling\b')

def is_ratchet(gate_path):
    fp = os.path.join(root, gate_path)
    if not gate_path.endswith(".sh") or not os.path.isfile(fp):
        return False
    return bool(RATCHET_RE.search(open(fp, encoding="utf-8", errors="replace").read()))

ceiling = None
unclassified_ceiling = None
unwired_ceiling = None
unvaried_ceiling = None
rows, cur = [], None
for raw in open(ledger_path, encoding="utf-8"):
    line = raw.strip()
    m = re.match(r'#\s*unverified_ceiling\s*=\s*"(\d+)"', line)
    if m:
        ceiling = int(m.group(1))
    m = re.match(r'#\s*unclassified_reads_ceiling\s*=\s*"(\d+)"', line)
    if m:
        unclassified_ceiling = int(m.group(1))
    m = re.match(r'#\s*unwired_ceiling\s*=\s*"(\d+)"', line)
    if m:
        unwired_ceiling = int(m.group(1))
    m = re.match(r'#\s*unvaried_ceiling\s*=\s*"(\d+)"', line)
    if m:
        unvaried_ceiling = int(m.group(1))
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
if unwired_ceiling is None:
    errs.append("ledger header is missing `# unwired_ceiling = \"N\"`")
if unvaried_ceiling is None:
    errs.append("ledger header is missing `# unvaried_ceiling = \"N\"`")
seen = set()
unverified = 0
unclassified = 0
unwired = 0
unvaried = 0
ratchets = 0
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
    wired = r.get("wired_by", "")
    if not wired:
        errs.append(f"{p}: no `wired_by` — name the workflow / hook / Makefile / script / test "
                    f"that runs this gate, or declare it UNWIRED under the ceiling. A gate "
                    f"nobody invokes cannot fail anyone (#2207)")
    elif wired == UNWIRED:
        unwired += 1
    else:
        for c in [x.strip() for x in wired.split(",") if x.strip()]:
            cp = os.path.join(root, c)
            if not os.path.isfile(cp):
                errs.append(f"{p}: wired_by names {c!r}, which does not exist — the consumer "
                            f"moved or was deleted; the gate runs nowhere now")
            elif not names_gate(open(cp, encoding="utf-8", errors="replace").read(), p):
                errs.append(f"{p}: wired_by names {c!r}, but that file never invokes the gate "
                            f"(no {os.path.basename(p)!r} in it) — a consumer in name only")

    # The fourth axis (#2234): what is measured, and has it ever varied.
    if "measures" not in r:
        errs.append(f"{p}: no `measures` — say what quantity this gate reads and from which "
                    f"artifact; a gate whose measurand nobody can name may be measuring a constant")
    elif not r["measures"].strip():
        errs.append(f"{p}: `measures` is empty — name the quantity and the artifact it is read from")
    if "varies" not in r:
        errs.append(f"{p}: no `varies` field — record where the measured quantity was observed to "
                    f"take two values, or leave it empty under the unvaried ceiling")
    elif not r["varies"].strip():
        if is_ratchet(p):
            errs.append(f"{p}: a RATCHET (compares against a committed baseline or ceiling) with "
                        f"empty `varies` — the ablation/* rows measured a constant for three months "
                        f"this way (#2234); record the commit, PR or run where the quantity moved")
        else:
            unvaried += 1
    if is_ratchet(p):
        ratchets += 1

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

if unwired_ceiling is not None:
    if unwired > unwired_ceiling:
        errs.append(f"UNWIRED count {unwired} exceeds the ceiling {unwired_ceiling} — a new gate "
                    f"lands wired into the job that runs it (#2207)")
    elif unwired < unwired_ceiling:
        errs.append(f"UNWIRED count {unwired} is BELOW the ceiling {unwired_ceiling} — ratchet it "
                    f"down in the ledger header (the debt may only shrink, and the ledger must "
                    f"say so)")

if unvaried_ceiling is not None:
    if unvaried > unvaried_ceiling:
        errs.append(f"unvaried count {unvaried} exceeds the ceiling {unvaried_ceiling} — a new gate "
                    f"lands with the observation that its measurand varies (#2234)")
    elif unvaried < unvaried_ceiling:
        errs.append(f"unvaried count {unvaried} is BELOW the ceiling {unvaried_ceiling} — ratchet it "
                    f"down in the ledger header (the debt may only shrink, and the ledger must "
                    f"say so)")

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
print(f"  measures: {len(rows)} named / ratchets {ratchets}, every one with `varies` / "
      f"unvaried {unvaried} (ceiling {unvaried_ceiling})")
print(f"  wired: {len(rows) - unwired} consumer-verified / UNWIRED {unwired} (ceiling {unwired_ceiling})")
EOF
