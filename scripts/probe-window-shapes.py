#!/usr/bin/env python3
"""Probe only: the windows of a census dump, one file per shape.

  probe-window-shapes.py <window-dump> <window-kernel/verdicts.tsv> <out-dir>

<window-dump> holds, per module, `MODULE <leg> <file>` and one line
`WINDOWALL <function> <access> <shape> <bytes...>` per window the cutter
proposes; verdicts.tsv has the kernel's verdict on each (leg, file, function,
access, bytes, ok | REFUSED, shape, located). Writes
<out-dir>/windows-<shape>.txt, one window the kernel accepts per line as
decimal bytes, and <out-dir>/shapes.txt with the counts. A window the kernel
refused is left out and counted.
"""
import collections
import os
import sys

dump, verdicts, out = sys.argv[1], sys.argv[2], sys.argv[3]
ok = {}
for l in open(verdicts, encoding="utf-8"):
    t = l.rstrip("\n").split("\t")
    ok[(t[0], t[1], t[2], t[3])] = t[5]
by, refused, cur = collections.defaultdict(list), collections.Counter(), None
for line in open(dump, encoding="utf-8", errors="replace"):
    t = line.split()
    if not t:
        continue
    if t[0] == "MODULE":
        cur = (t[1], " ".join(t[2:]))
    elif t[0] == "WINDOWALL" and cur is not None:
        v = ok.get((cur[0], cur[1], t[1], t[2]))
        if v == "ok":
            by[t[3]].append((cur[0], t[4:]))
        else:
            refused[(t[3], v)] += 1
os.makedirs(out, exist_ok=True)
with open(os.path.join(out, "shapes.txt"), "w", encoding="utf-8") as s:
    for shape in sorted(by):
        with open(os.path.join(out, f"windows-{shape}.txt"), "w", encoding="utf-8") as f:
            for _, bs in by[shape]:
                f.write(" ".join(bs) + "\n")
        legs = collections.Counter(leg for leg, _ in by[shape])
        line = f"{shape}: {len(by[shape])} accepted window(s) {dict(legs)}; with a conditional branch (opcode 13) among the bytes: {sum(1 for _, bs in by[shape] if '13' in bs)}"
        print(line)
        s.write(line + "\n")
    line = f"left out (not accepted by the kernel, or without a verdict): {dict(refused)}"
    print(line)
    s.write(line + "\n")
