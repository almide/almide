#!/usr/bin/env python3
"""#4005 A4 lab summary (probe only): the per-function census and the linear
prover's refusals, aggregated by label."""
import collections, glob, os, re, sys

out = sys.argv[1]
for leg in ("fx", "st"):
    per = collections.defaultdict(lambda: [0, 0, 0, 0, 0, 0, 0])  # artifacts, max acc, max unloc, sum unloc, max hdr, max bulk, max linear
    tot = collections.Counter()
    for c in glob.glob(os.path.join(out, leg, "*.anchor")):
        for line in open(c, errors="replace"):
            if not line.startswith("CENSUSFN "):
                continue
            _, func, cls, src, label, acc, loc, lin, hdr, bulk = line.split()
            cls = src
            acc, loc, lin, hdr, bulk = map(int, (acc, loc, lin, hdr, bulk))
            key = (cls, label if label != "-" else "<user>")
            p = per[key]
            un = acc - loc
            p[0] += 1
            p[1] = max(p[1], acc)
            p[2] = max(p[2], un)
            p[3] += un
            p[4] = max(p[4], hdr)
            p[5] = max(p[5], bulk)
            p[6] = max(p[6], lin)
            tot[cls + ".acc"] += acc
            tot[cls + ".unloc"] += un
            tot[cls + ".hdr"] += hdr
            tot[cls + ".bulk"] += bulk
            tot[cls + ".lin"] += lin
    print(f"== {leg}: totals {dict(tot)}")
    for cls in ("routine", "selfhost", "code"):
        rows = sorted(((k[1], v) for k, v in per.items() if k[0] == cls), key=lambda r: -r[1][2])
        print(f"-- {leg} {cls}: {len(rows)} label(s); sum over labels of max unlocated = {sum(v[2] for _, v in rows)}")
        print("   label artifacts max_acc max_unloc sum_unloc max_hdr max_bulk max_lin")
        for name, v in rows[:400]:
            print(f"   {name} {v[0]} {v[1]} {v[2]} {v[3]} {v[4]} {v[5]} {v[6]}")
    # routine prover
    rcert, rcount, unrouted, rtime = collections.Counter(), collections.Counter(), collections.Counter(), []
    for p in glob.glob(os.path.join(out, leg, "*.rprove")):
        for line in open(p, errors="replace"):
            parts = line.rstrip("\n").split("\t")
            if parts[0] == "routine" and len(parts) == 5:
                rcert[parts[2]] = max(rcert[parts[2]], int(parts[3]))
                rcount[parts[2]] = max(rcount[parts[2]], int(parts[4]))
            elif parts[0] == "unrouted" and len(parts) >= 4:
                unrouted[(parts[2], re.sub(r"\d+", "N", parts[3])[:100])] += 1
            elif line.startswith("rtime "):
                rtime.append(int(line.split()[1]))
    print(f"-- {leg} routines: {len(rcert)} labels with a procedure; prove time ms total {sum(rtime)} max {max(rtime) if rtime else 0}")
    for name in sorted(rcert, key=lambda n: -rcert[n]):
        print(f"   routine {name} certified {rcert[name]} counted {rcount[name]}")
    for (name, why), k in unrouted.most_common(60):
        print(f"   unrouted {k} {name} {why}")
    # linear prover refusals
    why = collections.Counter()
    bodies = {}
    cert = {}
    for p in glob.glob(os.path.join(out, leg, "*.lprove")):
        for line in open(p, errors="replace"):
            parts = line.rstrip("\n").split("\t")
            if parts[0] == "certified" and len(parts) >= 4:
                cert[parts[2]] = max(cert.get(parts[2], 0), int(parts[3]))
            if parts[0] == "uncertified" and len(parts) >= 5:
                r = re.sub(r"\d+", "N", parts[4])[:90]
                why[r] += 1
                bodies.setdefault(parts[2], (int(parts[3]), parts[4][:160]))
    never = {k: v for k, v in bodies.items() if k not in cert}
    print(f"-- {leg} linear: {len(cert)} bodies certified somewhere ({sum(cert.values())} sites), {len(never)} never ({sum(v[0] for v in never.values())} sites)")
    for r, k in why.most_common(40):
        print(f"   why {k} {r}")
    for name, (n, r) in sorted(never.items(), key=lambda x: -x[1][0])[:300]:
        print(f"   never {name} {n} {r}")
