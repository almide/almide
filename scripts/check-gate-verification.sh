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
#
# #3032 adds a FIFTH axis: what the gate does NOT see. The four axes above say
# a gate can go red, is run, and measures something that moves — and the
# release-blocker count still printed 0 over an unlabelled miscompile (#2293),
# because its domain is labels, not defects. `blind` names the defect classes
# outside the gate's domain, each routed to the gate that covers it or to
# UNCOMPENSATED; `UNMAPPED` is the honest-debt spelling, shrink-only.
#
# #3032 also closes the ENUMERATION. The set used to be a filename glob, so a
# script a workflow runs as a verdict under another name (count-release-
# blockers.sh gates the final tag) had no row. Now every script a workflow or
# lefthook INVOKES is either a `[[gate]]` or a `[[not_a_gate]]` with a reason;
# the glob stays as the floor, the invocation scan is what makes it closed.
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

# Every script a workflow or lefthook invokes (#3032). A mention on a comment
# line, or as a trigger pattern (`glob:`, a cache `key:`), is not an
# invocation. What is invoked and not a gate is declared in `[[not_a_gate]]`.
CONSUMERS = sorted(
    os.path.join(".github/workflows", f)
    for f in os.listdir(os.path.join(root, ".github/workflows")) if f.endswith(".yml")
) + ["lefthook.yml"]
NOT_INVOCATION = re.compile(r'^\s*(?:-\s*)?(?:glob|key|restore-keys|paths|paths-ignore)\s*:')
SCRIPT_REF = re.compile(r'(?<![\w/.-])(?:[\w.-]+/)?((?:scripts|proofs|tools)/[\w./-]+\.(?:sh|py))')
invoked = {}
for c in CONSUMERS:
    for line in open(os.path.join(root, c), encoding="utf-8"):
        if line.lstrip().startswith("#") or NOT_INVOCATION.match(line):
            continue
        for m in SCRIPT_REF.finditer(line):
            invoked.setdefault(m.group(1), set()).add(c)

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
unmapped_ceiling = None
uncompensated_ceiling = None
rows, not_gates, cur = [], [], None
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
    m = re.match(r'#\s*unmapped_blind_ceiling\s*=\s*"(\d+)"', line)
    if m:
        unmapped_ceiling = int(m.group(1))
    m = re.match(r'#\s*uncompensated_blind_ceiling\s*=\s*"(\d+)"', line)
    if m:
        uncompensated_ceiling = int(m.group(1))
    if line in ("[[gate]]", "[[not_a_gate]]"):
        if cur:
            (not_gates if cur.pop("_kind") == "not_a_gate" else rows).append(cur)
        cur = {"_kind": line.strip("[]")}
        continue
    m = re.match(r'([a-z_]+)\s*=\s*"(.*)"$', line)
    if m and cur is not None:
        cur[m.group(1)] = m.group(2)
if cur:
    (not_gates if cur.pop("_kind") == "not_a_gate" else rows).append(cur)

# The fifth axis (#3032): `blind` is `NONE: <why>`, `UNMAPPED`, or `;`-separated
# entries `<defect class> -> <compensator>`, where the compensator is
# UNCOMPENSATED or a path (optionally followed by prose) that exists. The
# compensator is a file, not a gate row, because the gate that covers a
# blind spot is often a test or a workflow.
UNMAPPED = "UNMAPPED"
UNCOMPENSATED = "UNCOMPENSATED"

def blind_entries(p, text, errs):
    if text.startswith("NONE:"):
        if not text[5:].strip():
            errs.append(f"{p}: `blind = \"NONE:\"` needs its reason — a gate that sees everything is a claim")
        return []
    out = []
    for e in [x.strip() for x in text.split(";") if x.strip()]:
        if "->" not in e:
            errs.append(f"{p}: blind entry {e!r} has no `->` — route each defect class to the gate "
                        f"that covers it, or to {UNCOMPENSATED}")
            continue
        what, comp = (s.strip() for s in e.split("->", 1))
        if not what or not comp:
            errs.append(f"{p}: blind entry {e!r} is missing its defect class or its compensator")
            continue
        if not comp.startswith(UNCOMPENSATED):
            target = comp.split()[0]
            if not os.path.exists(os.path.join(root, target.split(":", 1)[0])):
                errs.append(f"{p}: blind entry routes {what!r} to {target!r}, which does not exist")
        out.append((what, comp))
    return out

errs = []
if ceiling is None:
    errs.append("ledger header is missing `# unverified_ceiling = \"N\"`")
if unclassified_ceiling is None:
    errs.append("ledger header is missing `# unclassified_reads_ceiling = \"N\"`")
if unwired_ceiling is None:
    errs.append("ledger header is missing `# unwired_ceiling = \"N\"`")
if unvaried_ceiling is None:
    errs.append("ledger header is missing `# unvaried_ceiling = \"N\"`")
if unmapped_ceiling is None:
    errs.append("ledger header is missing `# unmapped_blind_ceiling = \"N\"`")
if uncompensated_ceiling is None:
    errs.append("ledger header is missing `# uncompensated_blind_ceiling = \"N\"`")

# The closed enumeration (#3032): an invoked script is a gate unless declared
# otherwise, and a declaration must name something that is still invoked.
declared_not = set()
for r in not_gates:
    p = r.get("path", "")
    if not r.get("reason", "").strip():
        errs.append(f"{p}: [[not_a_gate]] without a `reason` — say why its exit code decides nothing")
    if p not in invoked:
        errs.append(f"{p}: STALE [[not_a_gate]] — no workflow or lefthook invokes it any more")
    if p in declared_not:
        errs.append(f"{p}: duplicate [[not_a_gate]]")
    declared_not.add(p)
enumerated |= set(invoked) - declared_not

seen = set()
unverified = 0
unclassified = 0
unwired = 0
unvaried = 0
ratchets = 0
unmapped = 0
mapped = 0
uncompensated = []
for r in rows:
    p = r.get("path")
    if not p:
        errs.append(f"row without a path: {r}")
        continue
    if p in seen:
        errs.append(f"{p}: duplicate row")
    if p in declared_not:
        errs.append(f"{p}: both a [[gate]] and a [[not_a_gate]] — it is one or the other")
    seen.add(p)

    # The fifth axis (#3032): what the gate does not see.
    blind = r.get("blind", "")
    if not blind.strip():
        errs.append(f"{p}: no `blind` — name the defect classes this gate cannot see and what "
                    f"covers each, or declare {UNMAPPED} under the ceiling (#3032)")
    elif blind == UNMAPPED:
        unmapped += 1
    else:
        mapped += 1
        uncompensated += [(p, w) for w, c in blind_entries(p, blind, errs) if c.startswith(UNCOMPENSATED)]
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

if unmapped_ceiling is not None:
    if unmapped > unmapped_ceiling:
        errs.append(f"UNMAPPED blind count {unmapped} exceeds the ceiling {unmapped_ceiling} — a new "
                    f"gate lands saying what it cannot see (#3032)")
    elif unmapped < unmapped_ceiling:
        errs.append(f"UNMAPPED blind count {unmapped} is BELOW the ceiling {unmapped_ceiling} — ratchet "
                    f"it down in the ledger header (the debt may only shrink, and the ledger must "
                    f"say so)")

# Mapping a gate may ADD uncompensated entries (finding a blind spot is the
# point), so this ceiling moves both ways by declaration; what it refuses is
# the silent change — the header states the number the ledger holds.
if uncompensated_ceiling is not None and len(uncompensated) != uncompensated_ceiling:
    errs.append(f"UNCOMPENSATED blind entries {len(uncompensated)} != the declared "
                f"{uncompensated_ceiling} — update `# uncompensated_blind_ceiling` in the header: "
                f"up when a mapping names a new blind spot, down when a gate comes to cover one")

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
print(f"  blind: {mapped} mapped / UNMAPPED {unmapped} (ceiling {unmapped_ceiling}) / "
      f"{len(uncompensated)} uncompensated blind spot(s)")
print(f"  enumeration: {len(invoked)} invoked script(s) — {len(set(invoked) & seen)} gate row(s), "
      f"{len(declared_not)} declared not-a-gate")
EOF
