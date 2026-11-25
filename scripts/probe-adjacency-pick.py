#!/usr/bin/env python3
"""Probe only: mutants of the adjacency between an emitted share and its record.

  probe-adjacency-pick.py <out-dir> <shard> <shards>

A SITE is a line of the emitter crate, outside the recorder's own files, that
calls a recorder hook (`self.witness_*(..);`) and whose preceding line of code,
at the same indentation, is a call that emits a share (`self.rc_share_guard(..);`,
`self.rc_arg_guard(..);`, `self.share_handle_top(..);`, `self.rc_inc_top();`,
`self.rc_own(..);`), paired with the hook that mirrors it. Each site gives three
mutants: the emitting call deleted, the recording call deleted, both deleted.

Writes <out-dir>/index.tsv (id, file, line, genre, name) and <out-dir>/<id>.diff
for this shard's mutants, and <out-dir>/sample.tsv (every mutant, with its
shard), in the form probe-emitter-mutants.sh reads.
"""
import difflib
import os
import re
import sys

out, shard, shards = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
ROOT = "crates/almide-wasm/src"
PAIRS = {("rc_share_guard", "witness_store"), ("rc_arg_guard", "witness_arg"), ("share_handle_top", "witness_payload_share"),
         ("rc_inc_top", "witness_arm_share_out"), ("rc_own", "witness_cell_bind")}
CALL = re.compile(r"^(\s*)self\.([a-z_]+)\((.*)\);\s*$")
sites = []
for name in sorted(os.listdir(ROOT)):
    if not name.endswith(".rs") or name.startswith("witness"):
        continue
    path = f"{ROOT}/{name}"
    src = open(path, encoding="utf-8").read().split("\n")
    for i, line in enumerate(src):
        m = CALL.match(line)
        if not m or not m.group(2).startswith("witness_"):
            continue
        j = i - 1
        while j >= 0 and (src[j].strip() == "" or src[j].strip().startswith("//")):
            j -= 1
        e = CALL.match(src[j]) if j >= 0 else None
        if e and e.group(1) == m.group(1) and (e.group(2), m.group(2)) in PAIRS:
            sites.append((path, j, i, e.group(2), m.group(2)))
os.makedirs(out, exist_ok=True)


def diff(path, drop):
    old = open(path, encoding="utf-8").read().split("\n")
    new = [l for k, l in enumerate(old) if k not in drop]
    d = list(difflib.unified_diff(old, new, "a/" + path, "b/" + path, lineterm="", n=3))
    return "\n".join(d) + "\n"


n = 0
with open(os.path.join(out, "sample.tsv"), "w", encoding="utf-8") as allf, \
        open(os.path.join(out, "index.tsv"), "w", encoding="utf-8") as idx:
    for s, (path, j, i, emit, hook) in enumerate(sites):
        for v, (genre, drop, what) in enumerate((("AdjacentEmit", {j}, f"delete self.{emit}(..)"),
                                                 ("AdjacentRecord", {i}, f"delete self.{hook}(..)"),
                                                 ("AdjacentBoth", {j, i}, f"delete self.{emit}(..) and self.{hook}(..)"))):
            mid = f"m{s:03d}{v + 1}"
            row = [mid, path, str(j + 1), genre, f"{path}:{j + 1}: {what}"]
            mine = n % shards
            allf.write("\t".join(row + [str(mine), str(n)]) + "\n")
            if mine == shard:
                with open(os.path.join(out, mid + ".diff"), "w", encoding="utf-8") as d:
                    d.write(diff(path, drop))
                idx.write("\t".join(row) + "\n")
            n += 1
print(f"{len(sites)} sites, {n} mutants; this shard ({shard}/{shards}) takes {sum(1 for _ in open(os.path.join(out, 'index.tsv'), encoding='utf-8'))}")
