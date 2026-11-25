#!/usr/bin/env python3
"""Probe only: have the kernel judge each DISTINCT fresh-block proposal once.

  probe-held-fresh-uniq.py fold <dump> <uniq-dump> <uniq-index.tsv>
  probe-held-fresh-uniq.py spread <uniq-verdicts.tsv> <uniq-index.tsv> <verdicts.tsv>

`fold` reads a dump (`MODULE <mutant id | base> <program name>`, then that
module's `FRESHHEX <function> <access> <allocator> <write> <located> | <pre> |
<mid> | <tail>` lines) and writes the distinct proposals (allocator and the
three byte strings) under one made-up module, numbered in the function
column, together with an index: one line per proposal of the dump, ending in
the number of its distinct proposal.

`spread` reads the kernel's verdicts on the distinct proposals (module,
program, number, ..., true | false) and writes one line per proposal of the
dump: mutant id | base, program name, function, access, write, located,
bytes of mid, bytes of tail, true | false.
"""
import sys

mode = sys.argv[1]

if mode == "fold":
    dump, uniq, index = sys.argv[2:5]
    ids, cur = {}, None
    with open(uniq, "w", encoding="utf-8") as u, open(index, "w", encoding="utf-8") as ix:
        u.write("MODULE uniq u\n")
        for line in open(dump, encoding="utf-8", errors="replace"):
            if line.startswith("MODULE "):
                t = line.split()
                cur = (t[1], t[2])
            elif line.startswith("FRESHHEX ") and cur is not None:
                head, pre, mid, tail = [p.strip() for p in line[len("FRESHHEX "):].split("|")]
                func, acc, alloc, write, located = head.split()
                key = (alloc, pre, mid, tail)
                if key not in ids:
                    ids[key] = len(ids)
                    u.write(f"FRESHHEX {ids[key]} 0 {alloc} {write} {located} | {pre} | {mid} | {tail}\n")
                ix.write("\t".join([cur[0], cur[1], func, acc, write, located, str(len(mid.split())), str(len(tail.split())), str(ids[key])]) + "\n")
    print(f"{len(ids)} distinct proposal(s)")
    sys.exit(0)

if mode == "spread":
    uv, index, out = sys.argv[2:5]
    verdict = {}
    for l in open(uv, encoding="utf-8"):
        t = l.rstrip("\n").split("\t")
        verdict[t[2]] = t[-1]
    n = 0
    with open(out, "w", encoding="utf-8") as f:
        for l in open(index, encoding="utf-8"):
            t = l.rstrip("\n").split("\t")
            f.write("\t".join(t[:8] + [verdict[t[8]]]) + "\n")
            n += 1
    print(f"{n} proposal(s) carry a verdict")
    sys.exit(0)

sys.exit("unknown mode")
