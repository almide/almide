#!/usr/bin/env bash
# A measurement, not a gate: the Rocq kernel's verdict on every access a
# loop's exit test guards that the checker cut out of a shipped module
# (proofs/GuardLoop.v, `loop_class`).
#
#   scripts/probe-loop-kernel.sh <dump> <work-dir>
#
# <dump> is, per module, a line `MODULE <leg> <file>` and one line
#   LOOPHEX <function> <access> <inc> <write> <located> | <hold> | <pre> | <mid> | <ht> | <rest>
# per proposal; the byte strings are the module's own, consecutive in one
# function body (the hold may be empty). The kernel is asked once per
# DISTINCT proposal. Writes <work-dir>/verdicts.tsv (leg, file, function,
# access, write, located, bytes from the capture to the end of the scan,
# class) and <work-dir>/summary.txt. The class is `loop_class`'s: 0 refused,
# 4 the core holds and nothing stands behind the length word, 5 no
# instruction in between writes memory or calls, 6 the loop holds the block,
# 7 both.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
DUMP="${1:?usage: scripts/probe-loop-kernel.sh <dump> <work-dir>}"
WORK="${2:?usage: scripts/probe-loop-kernel.sh <dump> <work-dir>}"
mkdir -p "$WORK"
WORK="$(cd "$WORK" && pwd)"
COQC="${COQC:-$(command -v coqc)}"

if ! (cd "$ROOT/proofs" \
  && "$COQC" -Q . AlmideTrust GuardWindow.v > "$WORK/GuardWindow.log" 2>&1 \
  && "$COQC" -Q . AlmideTrust GuardFresh.v > "$WORK/GuardFresh.log" 2>&1 \
  && "$COQC" -Q . AlmideTrust GuardLoop.v > "$WORK/GuardLoop.log" 2>&1); then
  echo "loop kernel check: the proof files do not compile"
  head -20 "$WORK/GuardLoop.log" "$WORK/GuardFresh.log" "$WORK/GuardWindow.log" 2> /dev/null
  exit 1
fi
grep -m1 'Closed under the global context' "$WORK/GuardLoop.log" || { echo "loop kernel check: the theorem is not closed"; head -20 "$WORK/GuardLoop.log"; }

python3 -I - "$DUMP" "$WORK" "${LOOP_CHUNK:-500}" <<'PY'
import sys
dump, work, chunk = sys.argv[1], sys.argv[2], int(sys.argv[3])
rows, cur, ids = [], None, {}
for line in open(dump, encoding="utf-8", errors="replace"):
    if line.startswith("MODULE "):
        t = line.split()
        cur = (t[1], " ".join(t[2:]))
    elif line.startswith("LOOPHEX ") and cur is not None:
        parts = [p.strip() for p in line[len("LOOPHEX "):].split("|")]
        if len(parts) != 6:
            sys.exit(f"a LOOPHEX line does not have six parts: {line[:120]}")
        head, hold, pre, mid, ht, rest = parts
        func, acc, inc, write, located = head.split()
        key = (inc, hold, pre, mid, ht, rest)
        if key not in ids:
            ids[key] = len(ids)
        span = len(mid.split()) + len(ht.split()) + len(rest.split())
        rows.append((cur[0], cur[1], func, acc, write, located, str(span), ids[key]))
HEAD = """From AlmideTrust Require Import GuardWindow GuardFresh GuardLoop.
From Stdlib Require Import ZArith.
From Stdlib Require Import List.
Import ListNotations.
Open Scope Z_scope.
"""
semi = lambda s: ";".join(s.split())
for key, i in ids.items():
    inc, hold, pre, mid, ht, rest = key
    k = i // chunk
    with open(f"{work}/Loop_{k:04d}.v", "a" if i % chunk else "w", encoding="utf-8") as f:
        if i % chunk == 0:
            f.write(HEAD)
        f.write(f"Eval vm_compute in (loop_class {inc} [{semi(hold)}] [{semi(pre)}] [{semi(mid)}] [{semi(ht)}] [{semi(rest)}]).\n")
with open(f"{work}/index.tsv", "w", encoding="utf-8") as idx:
    for r in rows:
        idx.write("\t".join(str(x) for x in r) + "\n")
print(f"{len(rows)} proposal(s) in the dump, {len(ids)} distinct")
PY

export COQC ROOT WORK
t0=$(date +%s)
find "$WORK" -maxdepth 1 -name 'Loop_*.v' | LC_ALL=C sort \
  | xargs -P "${LOOP_JOBS:-4}" -I{} bash -c 'c="${1%.v}"; "$COQC" -Q "$ROOT/proofs" AlmideTrust "$1" > "$c.out" 2> "$c.err" || echo "FAIL $1: $(head -3 "$c.err")"' _ {}
echo "kernel time: $(( $(date +%s) - t0 )) s (4 at a time)"

python3 -I - "$WORK" <<'PY' | tee "$WORK/summary.txt"
import collections, glob, sys
work = sys.argv[1]
idx = [l.rstrip("\n").split("\t") for l in open(f"{work}/index.tsv", encoding="utf-8")]
vals = []
for p in sorted(glob.glob(f"{work}/Loop_*.out")):
    vals += [l.split("=", 1)[1].strip() for l in open(p, encoding="utf-8") if l.strip().startswith("= ")]
need = (max(int(r[7]) for r in idx) + 1) if idx else 0
if len(vals) != need:
    sys.exit(f"{len(vals)} verdict(s) for {need} distinct proposal(s)")
KIND = {"0": "no event located", "1": "an initialization event", "2": "a bounds event", "4": "a field event"}
CLASS = {"0": "refused", "4": "core only", "5": "pure", "6": "held", "7": "pure and held"}
with open(f"{work}/verdicts.tsv", "w", encoding="utf-8") as out:
    for r in idx:
        out.write("\t".join(r[:7] + [vals[int(r[7])]]) + "\n")
for leg in sorted(set(r[0] for r in idx)):
    R = [(r, vals[int(r[7])]) for r in idx if r[0] == leg]
    ok = [r for r, v in R if v in ("5", "6", "7")]
    print(f"loop kernel check [{leg}]: {len(R)} proposal(s): {len(ok)} accepted in {len(set(r[1] for r in ok))} module(s)")
    by = collections.Counter(CLASS.get(v, v) for r, v in R)
    for k in sorted(by):
        print(f"   {k}: {by[k]}")
    by = collections.Counter((CLASS.get(v, v), "store" if r[4] == "1" else "load", KIND.get(r[5], r[5])) for r, v in R)
    for k in sorted(by):
        print(f"   {k[0]:14s} {k[1]:5s} with {k[2]}: {by[k]}")
    spans = sorted(int(r[6]) for r in ok) or [0]
    print(f"   bytes scanned after the capture: median {spans[len(spans)//2]}, max {spans[-1]}")
PY
