#!/usr/bin/env python3
"""#4005 A4 lab (probe only): why the routine prover dropped each access,
aggregated by routine label and site."""
import collections, glob, os, sys

out = sys.argv[1]
for leg in ("fx", "st"):
    count = collections.Counter()
    sample = {}
    for p in glob.glob(os.path.join(out, leg, "*.rprove")):
        lines = open(p, errors="replace").read().splitlines()
        label = {}
        for line in lines:
            parts = line.split("\t")
            if parts[0] in ("routine", "unrouted") and len(parts) >= 3:
                label[parts[1]] = parts[2]
        cur, seen = None, set()
        for line in lines:
            parts = line.split("\t")
            if parts[0] == "whyproc":
                cur = parts[1]
            elif parts[0] == "why" and len(parts) >= 6:
                key = (label.get(cur, "?" + str(cur)), int(parts[1]), parts[2], parts[3], parts[4])
                if (cur, parts[1]) in seen:
                    continue
                seen.add((cur, parts[1]))
                count[key] += 1
                sample.setdefault(key, parts[5])
    per = collections.Counter()
    for k, n in count.items():
        per[k[0]] += n
    print(f"== {leg}: {sum(count.values())} dropped access(es) over {len(count)} (label, site) key(s)")
    for name, n in per.most_common():
        print(f"-- {name}: {n}")
        for k in sorted(x for x in count if x[0] == name):
            print(f"   site {k[1]} {k[2]} w{k[3]} at {k[4]} x{count[k]}")
            print(f"      facts: {sample[k][:1500]}")
