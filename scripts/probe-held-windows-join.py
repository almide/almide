#!/usr/bin/env python3
"""Probe only: the guarded windows of a sample's kept modules.

  probe-held-windows-join.py fold <jobs.tsv> <wins-dir> <uniq-dump> <index.tsv>
  probe-held-windows-join.py spread <verdicts.tsv> <index.tsv> <out-dir>

<jobs.tsv> is probe-held-fresh.sh's (mutant id | base, module path, program
name); <wins-dir>/<id>/<name>.txt is the cutter's `--windows-all` output for
that module. `fold` writes one `WINDOWALL` line per DISTINCT byte string (the
kernel is asked once for each) and an index of every proposal; `spread` puts
the kernel's verdicts back and writes <out-dir>/held-windows.tsv (per mutant,
over the programs whose module it changed: proposals, accepted and refused
windows of the unmutated modules and of the mutant's) and
held-windows-programs.tsv.
"""
import collections
import os
import sys

mode = sys.argv[1]
if mode == "fold":
    jobs, wins, dump, index = sys.argv[2:6]
    keys = {}
    with open(index, "w", encoding="utf-8") as idx:
        for line in open(jobs, encoding="utf-8"):
            mid, _path, name = line.rstrip("\n").split("\t")
            p = os.path.join(wins, mid, name + ".txt")
            swept = False
            if os.path.exists(p):
                for l in open(p, encoding="utf-8", errors="replace"):
                    t = l.split()
                    if t and t[0] == "WINDOWALL":
                        b = " ".join(t[4:])
                        k = keys.setdefault((t[3], b), len(keys))
                        idx.write(f"{mid}\t{name}\t{t[1]}\t{t[2]}\t{k}\n")
                    elif t and t[0] == "WINDOWSWEEP":
                        swept = True
            if not swept:
                idx.write(f"{mid}\t{name}\t-\t-\tunread\n")
    with open(dump, "w", encoding="utf-8") as d:
        d.write("MODULE uniq all\n")
        for (shape, b), k in sorted(keys.items(), key=lambda kv: kv[1]):
            d.write(f"WINDOWALL 0 {k} {shape} {b}\n")
    print(f"{len(keys)} distinct window(s) to ask the kernel about")
else:
    verdicts, index, out = sys.argv[2:5]
    ok = {}
    for l in open(verdicts, encoding="utf-8"):
        t = l.rstrip("\n").split("\t")
        ok[t[3]] = t[5] == "ok"
    per = collections.defaultdict(lambda: [0, 0, 0])  # (id, name) -> proposed, accepted, refused
    unread = set()
    seen = set()
    for l in open(index, encoding="utf-8"):
        mid, name, _f, _a, k = l.rstrip("\n").split("\t")
        seen.add((mid, name))
        if k == "unread":
            unread.add((mid, name))
            continue
        c = per[(mid, name)]
        c[0] += 1
        if k not in ok:
            continue  # the kernel gave no verdict: neither accepted nor refused
        c[1 if ok[k] else 2] += 1
    by = collections.defaultdict(list)
    for (mid, name) in seen:
        if mid != "base":
            by[mid].append(name)
    with open(os.path.join(out, "held-windows.tsv"), "w", encoding="utf-8") as fh, \
            open(os.path.join(out, "held-windows-programs.tsv"), "w", encoding="utf-8") as ph:
        fh.write("id\tkept_modules\tunread\tbase_proposed\tbase_accepted\tbase_refused\tproposed\tmutant_accepted\tproposals_refused\n")
        ph.write("id\tprogram\tbase_proposed\tbase_accepted\tproposed\taccepted\trefused\n")
        for mid in sorted(by):
            tot = [0] * 6
            for name in sorted(by[mid]):
                b, m = per[("base", name)], per[(mid, name)]
                for i, v in enumerate(b + m):
                    tot[i] += v
                if b != m:
                    ph.write(f"{mid}\t{name}\t{b[0]}\t{b[1]}\t{m[0]}\t{m[1]}\t{m[2]}\n")
            u = sum(1 for name in by[mid] if (mid, name) in unread)
            fh.write(f"{mid}\t{len(by[mid])}\t{u}\t" + "\t".join(str(x) for x in tot) + "\n")
    print(f"{len(by)} mutant(s); windows of unmutated modules: {sum(per[k][1] for k in per if k[0] == 'base')} accepted, {sum(per[k][2] for k in per if k[0] == 'base')} refused; unread modules {len(unread)}")
