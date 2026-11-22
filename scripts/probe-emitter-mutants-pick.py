#!/usr/bin/env python3
"""Probe only: draw a seeded sample from the mutation tool's listing and write
this shard's share as one diff per mutant.

  probe-emitter-mutants-pick.py <list.json> <out-dir> <sample-size> <seed> <shard> <shards>

<list.json> is the tool's `--list --json` output, used as it is: the sample is
drawn over every mutant it lists, in a fixed order, so which mutants are judged
is decided by the seed and by nothing else. The diff's two header lines are
rewritten into the form `git apply` reads (the tool puts a description where
the new file's name goes); the hunk is the tool's own.

Writes <out-dir>/index.tsv (id, file, line, genre, name) and <out-dir>/<id>.diff,
and <out-dir>/sample.tsv (the whole sample, with each mutant's shard).
"""
import json
import os
import random
import sys

src, out, n, seed, shard, shards = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4]), int(sys.argv[5]), int(sys.argv[6])
mutants = json.load(open(src, encoding="utf-8"))
mutants.sort(key=lambda m: (m["file"], m["span"]["start"]["line"], m["span"]["start"]["column"], m["name"]))
picked = sorted(random.Random(seed).sample(range(len(mutants)), min(n, len(mutants))))
os.makedirs(out, exist_ok=True)
with open(os.path.join(out, "sample.tsv"), "w", encoding="utf-8") as allf, \
        open(os.path.join(out, "index.tsv"), "w", encoding="utf-8") as idx:
    for k, i in enumerate(picked):
        m = mutants[i]
        mid = f"m{k:04d}"
        row = [mid, m["file"], str(m["span"]["start"]["line"]), m["genre"], m["name"].replace("\t", " ")]
        allf.write("\t".join(row + [str(k % shards), str(i)]) + "\n")
        if k % shards != shard:
            continue
        lines = m["diff"].split("\n")
        assert lines[0].startswith("--- ") and lines[1].startswith("+++ "), lines[:2]
        lines[0] = "--- a/" + m["file"]
        lines[1] = "+++ b/" + m["file"]
        text = "\n".join(lines)
        if not text.endswith("\n"):
            text += "\n"
        with open(os.path.join(out, mid + ".diff"), "w", encoding="utf-8") as d:
            d.write(text)
        idx.write("\t".join(row) + "\n")
print(f"listed {len(mutants)}, sampled {len(picked)} with seed {seed}, this shard ({shard}/{shards}) takes {sum(1 for k in range(len(picked)) if k % shards == shard)}")
