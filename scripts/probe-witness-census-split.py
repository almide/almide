#!/usr/bin/env python3
"""Probe only: class each function of one artifact and append its census row.

  probe-witness-census-split.py <leg> <file> <bundle> <anchors> <fn-rows> <out.tsv>

A function is `user` (a named frame of the program's own source: no `.` in
its name), `stdlib` (a named function with a module prefix), `helper` (a
certified frame with no ownership witness of its own) or `runtime` (every
other defined function). Names come from the bundle's ownership witnesses,
which follow function position order; the anchor file's `selfhost` line pins
where that run of positions starts.
"""
import sys

leg, file, bundle_p, anchors_p, rows_p, out_p = sys.argv[1:7]

names = []
b = open(bundle_p, "rb").read()
i = b.index(b"\n") + 1
while i < len(b):
    j = b.index(b"\n", i)
    line = b[i:j].decode(errors="replace")
    i = j + 1
    if line.startswith("witness "):
        _, prop, n, fn = line.split(" ", 3)
        if prop == "ownership":
            names.append(fn)
        i += int(n) + 1

lines = {}
for line in open(anchors_p, encoding="utf-8", errors="replace"):
    k, _, v = line.rstrip("\n").partition(" ")
    lines[k] = v
frames = [int(f.split("|", 1)[0]) for f in lines.get("rc", "").split(";") if f.split("|", 1)[0].strip().isdigit()]
helpers = {int(t.split(":", 1)[0]) for t in lines.get("rchelpers", "").split() if t.split(":", 1)[0].isdigit()}
pins = []
for t in lines.get("selfhost", "").split():
    parts = t.split(":", 2)
    if len(parts) == 3 and parts[0].isdigit() and parts[2] in names:
        pins.append(int(parts[0]) - names.index(parts[2]))
own = sorted(f for f in frames if f not in helpers)
how = "none"
base = None
if pins:
    base = max(set(pins), key=pins.count)
    how = "pinned" if len(set(pins)) == 1 else "pinned-majority"
elif own:
    base = own[0]
    how = "first-frame"
if base is not None and own and not (base <= own[0] and own[-1] < base + len(names)):
    how = "mismatch"
    base = None

def klass(pos):
    if base is not None and base <= pos < base + len(names):
        return "stdlib" if "." in names[pos - base] else "user"
    if pos in frames:
        return "helper"
    return "runtime"

with open(out_p, "a") as out:
    for line in open(rows_p):
        p = line.split()
        if len(p) != 11 or p[0] != "CENSUSFN":
            continue
        pos = int(p[1])
        name = names[pos - base] if base is not None and base <= pos < base + len(names) else "-"
        out.write("\t".join([leg, file, how, klass(pos), name] + p[1:]) + "\n")
