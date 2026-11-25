#!/usr/bin/env python3
"""Probe only: per mutant of a held-out sample, what the fresh-block rule says
of the modules the mutant changed, against the unmutated modules.

  probe-held-fresh-join.py <dump> <verdicts.tsv> <shards-dir> <out-dir>

<dump> names every module judged (`MODULE <mutant id | base> <program name>`)
and whether its allocator was found (`FRESHALLOC ...`); <verdicts.tsv> is the
kernel's answer per proposal (mutant id | base, program name, function,
access, write, located, bytes of mid, bytes of tail, true | false).
<shards-dir> holds `emut/windows/<id>.tsv`, one line per program whose module
the mutant changed: a mutant that changes more than a hundred has no modules
kept and is listed as not judged.

Writes <out-dir>/held-fresh-programs.tsv (mutant, program, allocator, accepted
in the unmutated module, proposed, accepted and refused in the mutant's) and
<out-dir>/held-fresh.tsv (one line per mutant).
"""
import collections
import glob
import os
import sys

dump, verdicts, shards, out = sys.argv[1:5]

alloc, cur = {}, None
for line in open(dump, encoding="utf-8", errors="replace"):
    if line.startswith("MODULE "):
        t = line.split()
        cur = (t[1], t[2])
        alloc[cur] = "?"
    elif line.startswith("FRESHALLOC ") and cur is not None:
        alloc[cur] = line.split()[1]

# `none`: the kernel gave no verdict under the time and memory limits; that is
# neither an acceptance nor a refusal.
ok, bad, none = collections.Counter(), collections.Counter(), collections.Counter()
for line in open(verdicts, encoding="utf-8"):
    t = line.rstrip("\n").split("\t")
    (ok if t[-1] == "true" else none if t[-1] == "none" else bad)[(t[0], t[1])] += 1

changed = {}
for p in glob.glob(os.path.join(shards, "**", "emut", "windows", "*.tsv"), recursive=True):
    name = os.path.basename(p)[:-4]
    if name.startswith("base") or name.endswith(".wins"):
        continue
    changed[name] = sum(1 for l in open(p, encoding="utf-8", errors="replace") if l.strip())

per = collections.defaultdict(list)
with open(os.path.join(out, "held-fresh-programs.tsv"), "w", encoding="utf-8") as fh:
    fh.write("id\tprogram\tallocator\tbase_accepted\tproposed\taccepted\trefused\tno_verdict\n")
    for (mid, prog), a in sorted(alloc.items()):
        if mid == "base":
            continue
        b, o, r, n = ok[("base", prog)], ok[(mid, prog)], bad[(mid, prog)], none[(mid, prog)]
        fh.write(f"{mid}\t{prog}\t{a}\t{b}\t{o + r + n}\t{o}\t{r}\t{n}\n")
        per[mid].append((prog, a, b, o, r))

base_total = sum(v for (m, _), v in ok.items() if m == "base")
base_bad = sum(v for (m, _), v in bad.items() if m == "base")
print(f"unmutated modules: {sum(1 for k in alloc if k[0] == 'base')} judged, {base_total} proposal(s) accepted, {base_bad} refused")
print(f"proposals without a verdict under the limits: {sum(none.values())} in {len(none)} module(s) of {len({k[0] for k in none})} mutant(s)")
rows = []
with open(os.path.join(out, "held-fresh.tsv"), "w", encoding="utf-8") as fh:
    fh.write("id\tchanged_programs\tkept_modules\tallocator_not_found\tbase_accepted\taccepted\trefused\tprograms_with_fewer\tprograms_with_more\tprograms_with_a_refusal\n")
    for mid in sorted(set(per) | set(changed)):
        ps = per.get(mid, [])
        row = (mid, changed.get(mid, "-"), len(ps), sum(1 for p in ps if not p[1].isdigit()),
               sum(p[2] for p in ps), sum(p[3] for p in ps), sum(p[4] for p in ps),
               sum(1 for p in ps if p[3] < p[2]), sum(1 for p in ps if p[3] > p[2]), sum(1 for p in ps if p[4] > 0))
        rows.append(row)
        fh.write("\t".join(map(str, row)) + "\n")

kept = [r for r in rows if r[2] > 0]
print(f"{len(rows)} mutant(s) change a module; modules kept for {len(kept)} (the others change more than a hundred programs or kept none)")
print(f"   the kernel refuses a proposal of a mutant's module: {sum(1 for r in kept if r[6] > 0)} mutant(s)")
print(f"   fewer accepted in total than the unmutated modules hold: {sum(1 for r in kept if r[5] < r[4])} mutant(s)")
print(f"   fewer accepted in some program: {sum(1 for r in kept if r[7] > 0)} mutant(s)")
print(f"   the allocator is not found in some module: {sum(1 for r in kept if r[3] > 0)} mutant(s)")
print(f"   none of these: {sum(1 for r in kept if r[6] == 0 and r[7] == 0 and r[3] == 0)} mutant(s)")
for title, pick in (("a refusal", lambda r: r[6] > 0), ("a lower total", lambda r: r[5] < r[4])):
    hit = [r for r in kept if pick(r)]
    if hit:
        print(f"mutants with {title}: " + " ".join(f"{r[0]}({r[4]}->{r[5]},refused {r[6]},{r[2]} module(s))" for r in hit[:80]))
