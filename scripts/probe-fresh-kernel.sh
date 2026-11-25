#!/usr/bin/env bash
# A measurement, not a gate: the Rocq kernel's verdict on every access into a
# block just allocated with a constant size that the checker cut out of a
# shipped module (proofs/GuardFresh.v, `fresh_ok`).
#
#   scripts/probe-fresh-kernel.sh <dump> <work-dir>
#
# <dump> is, per module, a line `MODULE <leg> <file>` and one line
#   FRESHHEX <function> <access> <alloc> <write> <located> | <pre> | <mid> | <tail>
# per proposal; the three byte strings are the module's own, consecutive in
# one function body. Writes <work-dir>/verdicts.tsv (leg, file, function,
# access, write, located, header | payload, bytes of mid, true | false) and
# <work-dir>/summary.txt.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
DUMP="${1:?usage: scripts/probe-fresh-kernel.sh <dump> <work-dir>}"
WORK="${2:?usage: scripts/probe-fresh-kernel.sh <dump> <work-dir>}"
mkdir -p "$WORK"
WORK="$(cd "$WORK" && pwd)"
COQC="${COQC:-$(command -v coqc)}"

if ! (cd "$ROOT/proofs" && "$COQC" -Q . AlmideTrust GuardWindow.v > "$WORK/GuardWindow.log" 2>&1 && "$COQC" -Q . AlmideTrust GuardFresh.v > "$WORK/GuardFresh.log" 2>&1); then
  echo "fresh kernel check: the proof files do not compile"
  head -20 "$WORK/GuardFresh.log" "$WORK/GuardWindow.log" 2> /dev/null
  exit 1
fi
grep -m1 'Closed under the global context' "$WORK/GuardFresh.log" || { echo "fresh kernel check: the theorem is not closed"; head -20 "$WORK/GuardFresh.log"; }

python3 -I - "$DUMP" "$WORK" "${FRESH_CHUNK:-1000}" <<'PY'
import sys
dump, work, chunk = sys.argv[1], sys.argv[2], int(sys.argv[3])
rows, cur = [], None
for line in open(dump, encoding="utf-8", errors="replace"):
    if line.startswith("MODULE "):
        t = line.split()
        cur = (t[1], " ".join(t[2:]))
    elif line.startswith("FRESHHEX ") and cur is not None:
        head, pre, mid, tail = [p.strip() for p in line[len("FRESHHEX "):].split("|")]
        func, acc, alloc, write, located = head.split()
        rows.append((cur[0], cur[1], func, acc, alloc, write, located, pre.split(), mid.split(), tail.split()))
HEAD = """From AlmideTrust Require Import GuardWindow GuardFresh.
From Stdlib Require Import ZArith.
From Stdlib Require Import List.
Import ListNotations.
Open Scope Z_scope.
"""
with open(f"{work}/index.tsv", "w", encoding="utf-8") as idx:
    for i, (leg, label, func, acc, alloc, write, located, pre, mid, tail) in enumerate(rows):
        k = i // chunk
        with open(f"{work}/Fresh_{k:04d}.v", "a" if i % chunk else "w", encoding="utf-8") as f:
            if i % chunk == 0:
                f.write(HEAD)
            f.write(f"Eval vm_compute in (fresh_ok {alloc} [{';'.join(pre)}] [{';'.join(mid)}] [{';'.join(tail)}]).\n")
        idx.write(f"{leg}\t{label}\t{func}\t{acc}\t{write}\t{located}\t{len(mid)}\t{len(tail)}\n")
print(f"{len(rows)} proposal(s) in the dump")
PY

export COQC ROOT WORK
t0=$(date +%s)
find "$WORK" -maxdepth 1 -name 'Fresh_*.v' | LC_ALL=C sort \
  | xargs -P "${FRESH_JOBS:-4}" -I{} bash -c 'c="${1%.v}"; "$COQC" -Q "$ROOT/proofs" AlmideTrust "$1" > "$c.out" 2> "$c.err" || echo "FAIL $1: $(head -3 "$c.err")"' _ {}
echo "kernel time: $(( $(date +%s) - t0 )) s (4 at a time)"

python3 -I - "$WORK" <<'PY' | tee "$WORK/summary.txt"
import collections, glob, sys
work = sys.argv[1]
idx = [l.rstrip("\n").split("\t") for l in open(f"{work}/index.tsv", encoding="utf-8")]
vals = []
for p in sorted(glob.glob(f"{work}/Fresh_*.out")):
    vals += [l.split("=", 1)[1].strip() for l in open(p, encoding="utf-8") if l.strip().startswith("= ")]
if len(vals) != len(idx):
    sys.exit(f"{len(vals)} verdict(s) for {len(idx)} proposal(s)")
KIND = {"0": "no event located", "1": "an initialization event", "2": "a bounds event", "4": "a field event"}
with open(f"{work}/verdicts.tsv", "w", encoding="utf-8") as out:
    for r, v in zip(idx, vals):
        out.write("\t".join(r + [v]) + "\n")
for leg in sorted(set(r[0] for r in idx)):
    R = [(r, v) for r, v in zip(idx, vals) if r[0] == leg]
    ok = [r for r, v in R if v == "true"]
    print(f"fresh kernel check [{leg}]: {len(R)} proposal(s): {len(ok)} accepted, {len(R) - len(ok)} refused; accepted in {len(set(r[1] for r in ok))} module(s)")
    by = collections.Counter(("store" if r[4] == "1" else "load", KIND.get(r[5], r[5])) for r in ok)
    for k in sorted(by):
        print(f"   accepted {k[0]:5s} with {k[1]}: {by[k]}")
    bad = collections.Counter(("store" if r[4] == "1" else "load", KIND.get(r[5], r[5])) for r, v in R if v != "true")
    for k in sorted(bad):
        print(f"   REFUSED  {k[0]:5s} with {k[1]}: {bad[k]}")
    mids = sorted(int(r[6]) for r in ok) or [0]
    print(f"   bytes between the allocation and the tail: median {mids[len(mids)//2]}, max {mids[-1]}")
PY
