#!/usr/bin/env bash
# A control, not a gate: the instruction lengths the proof script's scans step
# by (`step1` in proofs/GuardFresh.v), against the lengths the checker's
# decoder found for the same bytes.
#
#   scripts/probe-scan-lens.sh <dump> <work-dir>
#
# <dump> holds lines `SCANLENS <bytes> | <lengths>`: a byte string the kernel
# scans for a proposal (what lies between an allocation and its access, or
# between a count's capture and the end of the loop), and the byte length of
# each of its instructions as the decoder library read them. The kernel is
# asked once per DISTINCT string whether its own stepping gives the same
# lengths (proofs/GuardLoop.v, `lens_ok`). Writes <work-dir>/summary.txt and,
# when some string disagrees, <work-dir>/differ.txt.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
DUMP="${1:?usage: scripts/probe-scan-lens.sh <dump> <work-dir>}"
WORK="${2:?usage: scripts/probe-scan-lens.sh <dump> <work-dir>}"
mkdir -p "$WORK"
WORK="$(cd "$WORK" && pwd)"
COQC="${COQC:-$(command -v coqc)}"

if ! (cd "$ROOT/proofs" \
  && "$COQC" -Q . AlmideTrust GuardWindow.v > "$WORK/GuardWindow.log" 2>&1 \
  && "$COQC" -Q . AlmideTrust GuardFresh.v > "$WORK/GuardFresh.log" 2>&1 \
  && "$COQC" -Q . AlmideTrust GuardLoop.v > "$WORK/GuardLoop.log" 2>&1); then
  echo "scan lengths: the proof files do not compile"
  head -20 "$WORK/GuardLoop.log" "$WORK/GuardFresh.log" "$WORK/GuardWindow.log" 2> /dev/null
  exit 1
fi

python3 -I - "$DUMP" "$WORK" "${LENS_CHUNK:-400}" <<'PY'
import sys
dump, work, chunk = sys.argv[1], sys.argv[2], int(sys.argv[3])
ids, lines = {}, 0
for line in open(dump, encoding="utf-8", errors="replace"):
    if not line.startswith("SCANLENS "):
        continue
    lines += 1
    parts = line[len("SCANLENS "):].split("|")
    if len(parts) != 2:
        sys.exit(f"a SCANLENS line does not have two parts: {line[:120]}")
    key = (" ".join(parts[0].split()), " ".join(parts[1].split()))
    if key not in ids:
        ids[key] = len(ids)
HEAD = """From AlmideTrust Require Import GuardWindow GuardFresh GuardLoop.
From Stdlib Require Import ZArith.
From Stdlib Require Import List.
Import ListNotations.
Open Scope Z_scope.
"""
semi = lambda s: ";".join(s.split())
with open(f"{work}/index.tsv", "w", encoding="utf-8") as idx:
    for (bs, ls), i in ids.items():
        k = i // chunk
        with open(f"{work}/Lens_{k:04d}.v", "a" if i % chunk else "w", encoding="utf-8") as f:
            if i % chunk == 0:
                f.write(HEAD)
            f.write(f"Eval vm_compute in (lens_ok [{semi(bs)}] [{semi(ls)}]).\n")
        idx.write(f"{len(bs.split())}\t{len(ls.split())}\t{bs}\n")
print(f"{lines} scanned string(s) in the dump, {len(ids)} distinct")
PY

export COQC ROOT WORK
t0=$(date +%s)
find "$WORK" -maxdepth 1 -name 'Lens_*.v' | LC_ALL=C sort \
  | xargs -P "${LENS_JOBS:-4}" -I{} bash -c 'c="${1%.v}"; "$COQC" -Q "$ROOT/proofs" AlmideTrust "$1" > "$c.out" 2> "$c.err" || echo "FAIL $1: $(head -3 "$c.err")"' _ {}
echo "kernel time: $(( $(date +%s) - t0 )) s (4 at a time)"

python3 -I - "$WORK" <<'PY' | tee "$WORK/summary.txt"
import glob, sys
work = sys.argv[1]
idx = [l.rstrip("\n").split("\t") for l in open(f"{work}/index.tsv", encoding="utf-8")]
vals = []
for p in sorted(glob.glob(f"{work}/Lens_*.out")):
    vals += [l.split("=", 1)[1].strip() for l in open(p, encoding="utf-8") if l.strip().startswith("= ")]
if len(vals) != len(idx):
    sys.exit(f"{len(vals)} verdict(s) for {len(idx)} distinct string(s)")
same = [r for r, v in zip(idx, vals) if v == "true"]
diff = [r for r, v in zip(idx, vals) if v != "true"]
print(f"scan lengths: {len(idx)} distinct string(s), {sum(int(r[0]) for r in idx)} byte(s), {sum(int(r[1]) for r in idx)} instruction(s)")
print(f"   the scan steps exactly as the decoder does: {len(same)} string(s), {sum(int(r[1]) for r in same)} instruction(s)")
print(f"   the scan steps differently or stops: {len(diff)} string(s)")
if diff:
    with open(f"{work}/differ.txt", "w", encoding="utf-8") as f:
        for r in diff:
            f.write(r[2] + "\n")
    for r in diff[:5]:
        print(f"      {r[2][:160]}")
PY
