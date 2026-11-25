#!/usr/bin/env bash
# Probe (not a gate): the three rules the kernel judges, on the modules of one
# source mutant of the emitter, built without it and with it.
#
#   scripts/probe-m0217-rules.sh <data-dir> <out-dir>
#
# <data-dir>/{base,mutant}/<name>.wasm with <name>.anchors beside each. The
# mutant lets a loop read a list's element count once, before the loop, even
# when the loop hands the list to a call that can shrink it. The question is
# what each rule says about the reads such a loop then makes: a window that is
# lost, a loop read whose core holds with nothing behind the length word
# (class 4), or -- the answer that would be a defect of the rule -- a loop
# read the kernel accepts.
#
# Needs target/release/almide-anchor-check, coqc, and GuardWindow.vo,
# GuardFresh.vo and GuardLoop.vo in proofs/. Writes <out-dir>/rules.txt
# (per module and side: loads and stores, windows accepted, fresh-block
# accesses accepted, loop reads accepted / with nothing behind / refused) and
# <out-dir>/loops.txt (every loop proposal with its class, side by side).
set -uo pipefail
export LC_ALL=C
D="${1:?usage: scripts/probe-m0217-rules.sh <data-dir> <out-dir>}"
O="${2:?usage: scripts/probe-m0217-rules.sh <data-dir> <out-dir>}"
mkdir -p "$O"
B=./target/release/almide-anchor-check
printf 'fixtures 0\ntests 0\n' > "$O/floor1.txt"
printf 'fixtures 0 1000000\ntests 0 1000000\n' > "$O/floor2.txt"
for side in base mutant; do
  : > "$O/win-dump-$side"; : > "$O/fresh-dump-$side"; : > "$O/loop-dump-$side"
  for w in "$D/$side"/*.wasm; do
    n="$(basename "$w" .wasm)"
    a="$D/$side/$n.anchors"
    printf 'MODULE %s\n' "$n" | tee -a "$O/win-dump-$side" "$O/fresh-dump-$side" >> "$O/loop-dump-$side"
    "$B" --windows "$w" >> "$O/win-dump-$side" || echo "the window cutter failed on $side/$n"
    "$B" --fresh "$w" "$a" >> "$O/fresh-dump-$side" || echo "the fresh cutter failed on $side/$n"
    "$B" --loops "$w" "$a" >> "$O/loop-dump-$side" || echo "the loop cutter failed on $side/$n"
  done
  SHIPPED_WINDOWS_FLOOR="$O/floor1.txt" bash proofs/check-shipped-windows.sh "$O/win-dump-$side" "$O/win-$side" fixtures \
    || echo "note: the window gate's own verdict on the $side side is a failure (floor or refusal)"
  SHIPPED_FRESH_FLOOR="$O/floor1.txt" bash proofs/check-shipped-fresh.sh "$O/fresh-dump-$side" "$O/fresh-$side" fixtures \
    || echo "note: the fresh-block gate's own verdict on the $side side is a failure"
  SHIPPED_LOOPS_FLOOR="$O/floor2.txt" bash proofs/check-shipped-loops.sh "$O/loop-dump-$side" "$O/loop-$side" fixtures \
    || echo "note: the loop gate's own verdict on the $side side is a failure (a refused core)"
done
python3 -I - "$O" <<'PY'
import collections
import sys

o = sys.argv[1]


def rows(p):
    try:
        return [l.rstrip("\n").split("\t") for l in open(p, encoding="utf-8")]
    except OSError:
        print(f"MISSING {p}")
        return []


def accesses(side):
    out, cur = {}, None
    for l in open(f"{o}/win-dump-{side}", encoding="utf-8"):
        t = l.split()
        if t[:1] == ["MODULE"]:
            cur = t[1]
        elif t[:1] == ["ACCESSES"] and cur:
            out[cur] = t[1]
    return out


tab = {}
names = set()
for side in ("base", "mutant"):
    acc = accesses(side)
    win = collections.Counter(r[0] for r in rows(f"{o}/win-{side}/verdicts.tsv") if "ok" in r[3:6])
    fresh = collections.Counter(r[0] for r in rows(f"{o}/fresh-{side}/verdicts.tsv") if r[3] == "ok")
    loops = rows(f"{o}/loop-{side}/verdicts.tsv")
    la = collections.Counter(r[0] for r in loops if int(r[3]) > 4)
    lb = collections.Counter(r[0] for r in loops if int(r[3]) == 4)
    lr = collections.Counter(r[0] for r in loops if int(r[3]) == 0)
    for n in acc:
        names.add(n)
        tab[(n, side)] = (acc[n], win[n], fresh[n], la[n], lb[n], lr[n])
    tab[("loops", side)] = loops
with open(f"{o}/rules.txt", "w", encoding="utf-8") as out:
    head = "module | side | loads+stores | windows | fresh | loop accepted | loop nothing-behind | loop refused"
    print(head)
    out.write(head + "\n")
    for n in sorted(names):
        for side in ("base", "mutant"):
            line = f"{n} | {side} | " + " | ".join(str(x) for x in tab.get((n, side), ("-",) * 6))
            print(line)
            out.write(line + "\n")
    for side in ("base", "mutant"):
        tot = [sum(int(tab[(n, side)][i]) for n in names if (n, side) in tab) for i in range(6)]
        line = f"TOTAL | {side} | " + " | ".join(str(x) for x in tot)
        print(line)
        out.write(line + "\n")
with open(f"{o}/loops.txt", "w", encoding="utf-8") as out:
    for side in ("base", "mutant"):
        for r in tab[("loops", side)]:
            c = int(r[3])
            what = "refused" if c == 0 else "nothing-behind" if c == 4 else "accepted:" + "+".join(
                k for k, b in (("pure", 1), ("held", 2), ("kept", 8)) if c & b)
            line = f"{side}\t{r[0]}\t{r[1]}\t{r[2]}\t{c}\t{what}"
            print(line)
            out.write(line + "\n")
PY
