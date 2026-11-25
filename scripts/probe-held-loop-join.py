#!/usr/bin/env python3
"""Probe only: per mutant of a held-out sample, what the loop rule says of the
modules the mutant changed, against the unmutated modules.

  probe-held-loop-join.py <dump> <verdicts.tsv> <shards-dir> <out-dir>

<dump> names every module judged (`MODULE <mutant id | base> <program name>`)
and whether its increment routine was found (`LOOPINC <index | none>`);
<verdicts.tsv> is the kernel's class per proposal (mutant id | base, program
name, function, access, write, located, bytes scanned, class: 0 refused,
4 the core holds with nothing behind the length word, 5 pure, 6 held,
7 both). <shards-dir> holds `emut/windows/<id>.tsv`, one line per program
whose module the mutant changed.

Writes <out-dir>/held-loop-programs.tsv (mutant, program, increment routine,
then accepted / core only / refused in the unmutated module and in the
mutant's) and <out-dir>/held-loop.tsv (one line per mutant).
"""
import collections
import glob
import os
import sys

dump, verdicts, shards, out = sys.argv[1:5]

inc, cur = {}, None
for line in open(dump, encoding="utf-8", errors="replace"):
    if line.startswith("MODULE "):
        t = line.split()
        cur = (t[1], t[2])
        inc[cur] = "?"
    elif line.startswith("LOOPINC ") and cur is not None:
        inc[cur] = line.split()[1]

ok, core, bad = collections.Counter(), collections.Counter(), collections.Counter()
for line in open(verdicts, encoding="utf-8"):
    t = line.rstrip("\n").split("\t")
    k = (t[0], t[1])
    if t[-1] in ("5", "6", "7"):
        ok[k] += 1
    elif t[-1] == "4":
        core[k] += 1
    else:
        bad[k] += 1

changed = {}
for p in glob.glob(os.path.join(shards, "**", "emut", "windows", "*.tsv"), recursive=True):
    name = os.path.basename(p)[:-4]
    if name.startswith("base") or name.endswith(".wins"):
        continue
    changed[name] = sum(1 for l in open(p, encoding="utf-8", errors="replace") if l.strip())

per = collections.defaultdict(list)
with open(os.path.join(out, "held-loop-programs.tsv"), "w", encoding="utf-8") as fh:
    fh.write("id\tprogram\tincrement\tbase_accepted\tbase_core_only\tbase_refused\taccepted\tcore_only\trefused\n")
    for (mid, prog), a in sorted(inc.items()):
        if mid == "base":
            continue
        b = ("base", prog)
        k = (mid, prog)
        row = (prog, a, ok[b], core[b], bad[b], ok[k], core[k], bad[k])
        fh.write(mid + "\t" + "\t".join(map(str, row)) + "\n")
        per[mid].append(row)

tot = lambda c: sum(v for (m, _), v in c.items() if m == "base")
print(f"unmutated modules: {sum(1 for k in inc if k[0] == 'base')} judged; loop proposals accepted {tot(ok)}, core only {tot(core)}, refused {tot(bad)}")
rows = []
with open(os.path.join(out, "held-loop.tsv"), "w", encoding="utf-8") as fh:
    fh.write("id\tchanged_programs\tkept_modules\tincrement_not_found\tbase_accepted\taccepted\tbase_core_only\tcore_only\tbase_refused\trefused\tprograms_with_fewer_accepted\tprograms_with_more_accepted\tprograms_with_more_refused\tprograms_with_more_core_only\n")
    for mid in sorted(set(per) | set(changed)):
        ps = per.get(mid, [])
        row = (mid, changed.get(mid, "-"), len(ps), sum(1 for p in ps if not p[1].isdigit()),
               sum(p[2] for p in ps), sum(p[5] for p in ps), sum(p[3] for p in ps), sum(p[6] for p in ps),
               sum(p[4] for p in ps), sum(p[7] for p in ps),
               sum(1 for p in ps if p[5] < p[2]), sum(1 for p in ps if p[5] > p[2]),
               sum(1 for p in ps if p[7] > p[4]), sum(1 for p in ps if p[6] > p[3]))
        rows.append(row)
        fh.write("\t".join(map(str, row)) + "\n")

kept = [r for r in rows if r[2] > 0]
print(f"{len(rows)} mutant(s) change a module; modules kept for {len(kept)}")
print(f"   fewer accepted in some program: {sum(1 for r in kept if r[10] > 0)} mutant(s)")
print(f"   more refused by the core in some program: {sum(1 for r in kept if r[12] > 0)} mutant(s)")
print(f"   more with nothing behind the length word in some program: {sum(1 for r in kept if r[13] > 0)} mutant(s)")
print(f"   the increment routine is not found in some module: {sum(1 for r in kept if r[3] > 0)} mutant(s)")
print(f"   none of these: {sum(1 for r in kept if r[10] == 0 and r[12] == 0 and r[13] == 0 and r[3] == 0)} mutant(s)")
hit = [r for r in kept if r[10] > 0 or r[12] > 0 or r[13] > 0]
for r in hit[:60]:
    print(f"   {r[0]}: {r[2]} module(s); accepted {r[4]} -> {r[5]}, core only {r[6]} -> {r[7]}, refused {r[8]} -> {r[9]}")
