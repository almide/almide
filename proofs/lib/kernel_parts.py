#!/usr/bin/env python3
"""The structural wall's kernel oracle, split into parts (#4005 A4).

The kernel keeps every goal's statement, so one coqc file holding every
witness grows its memory with the corpus. The wall writes the goals into
parts of bounded literal size instead, each its own coqc run, and this
module keeps the split from dropping a goal:

  write(base, header, goals, part_bytes)   goals: (text, literal bytes,
      witnesses) triples; writes <base>_p<i>.v per part and the manifest
      <base>.lst — `goals G`, `witnesses W`, then `path<TAB>goals` per
      part — after asserting every goal landed in exactly one part.

  kernel_parts.py check <manifest> <witnesses>   exit 1 unless the
      manifest names at least one part, every part file exists and holds
      exactly its listed number of goals, the parts' goals sum to G, and
      W is the number of witnesses the caller fed in.
"""
import os
import re
import sys

GOAL = re.compile(r"^Goal ", re.M)


def write(base, header, goals, part_bytes):
    parts, part, size = [], [], 0
    for g in goals:
        if part and size + g[1] > part_bytes:
            parts.append(part)
            part, size = [], 0
        part.append(g)
        size += g[1]
    if part:
        parts.append(part)
    if not parts:
        parts.append([])
    if sum(len(p) for p in parts) != len(goals):
        sys.exit("kernel parts: the split lost a goal")
    lines = [f"goals {len(goals)}", f"witnesses {sum(g[2] for g in goals)}"]
    for i, p in enumerate(parts):
        path = f"{base}_p{i}.v"
        with open(path, "w") as f:
            f.write(header + "".join(g[0] for g in p))
        lines.append(f"{path}\t{len(p)}")
    with open(base + ".lst", "w") as f:
        f.write("\n".join(lines) + "\n")
    return base + ".lst"


def check(manifest, witnesses):
    rows = open(manifest).read().splitlines()
    if len(rows) < 3 or not rows[0].startswith("goals ") or not rows[1].startswith("witnesses "):
        return "the manifest names no part"
    goals, wit = int(rows[0].split()[1]), int(rows[1].split()[1])
    if wit != witnesses:
        return f"the parts hold {wit} witness(es), the wall fed in {witnesses}"
    total = 0
    for row in rows[2:]:
        path, n = row.split("\t")
        if not os.path.exists(path):
            return f"part {path} is missing"
        got = len(GOAL.findall(open(path).read()))
        if got != int(n):
            return f"part {path} holds {got} goal(s), the manifest lists {n}"
        total += got
    if total != goals:
        return f"the parts hold {total} goal(s), the manifest lists {goals}"
    return None


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "check":
        why = check(sys.argv[2], int(sys.argv[3]))
        if why:
            print(f"STRUCTURAL WALL FAIL: kernel parts: {why}", file=sys.stderr)
            sys.exit(1)
        sys.exit(0)
    print(__doc__, file=sys.stderr)
    sys.exit(2)
