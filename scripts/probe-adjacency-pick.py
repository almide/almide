#!/usr/bin/env python3
"""Probe only: mutants of the adjacency between an emitted instruction and its
record, second campaign: releases, and the records of field accesses.

  probe-adjacency-pick.py <out-dir> <shard> <shards>

Two kinds of SITE in the emitter crate, outside the recorder's own files.

RELEASE: a line that calls a release hook (`self.witness_dec(..);`,
`self.witness_discard(..);`, `self.witness_owned_released(..);`) and whose
preceding line of code, at the same indentation, emits the release it records
(`self.f.instructions().local_get(<local>).call(<dec…>);`). Three mutants:
the emitting line deleted, the recording line deleted, both deleted.

FIELD: a one-line statement that records a field or tag access and emits
nothing (`self.fw_field(..);`, `self.fw_store(..);`, `self.fw_tag(..);`,
`self.fw_res_tag(..);`, `self.fw_res_tag_back(..);`). One mutant: the
recording line deleted. The access it records cannot be deleted alone: the
module would no longer validate.

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
CALL = re.compile(r"^(\s*)self\.([a-z_]+)\((.*)\);\s*$")
EMIT_DEC = re.compile(r"^(\s*)self\.f\.instructions\(\)\.local_get\([a-z_0-9]+\)\.call\(dec[a-z_0-9]*\);\s*$")
RELEASE_HOOKS = {"witness_dec", "witness_discard", "witness_owned_released"}
FIELD_HOOKS = {"fw_field", "fw_store", "fw_tag", "fw_res_tag", "fw_res_tag_back"}
releases, fields = [], []
for name in sorted(os.listdir(ROOT)):
    if not name.endswith(".rs") or "witness" in name:
        continue
    path = f"{ROOT}/{name}"
    src = open(path, encoding="utf-8").read().split("\n")
    for i, line in enumerate(src):
        m = CALL.match(line)
        if not m:
            continue
        if m.group(2) in FIELD_HOOKS:
            fields.append((path, i, m.group(2)))
            continue
        if m.group(2) not in RELEASE_HOOKS:
            continue
        j = i - 1
        while j >= 0 and (src[j].strip() == "" or src[j].strip().startswith("//")):
            j -= 1
        e = EMIT_DEC.match(src[j]) if j >= 0 else None
        if e and e.group(1) == m.group(1):
            releases.append((path, j, i, m.group(2)))
os.makedirs(out, exist_ok=True)


def diff(path, drop):
    old = open(path, encoding="utf-8").read().split("\n")
    new = [l for k, l in enumerate(old) if k not in drop]
    d = list(difflib.unified_diff(old, new, "a/" + path, "b/" + path, lineterm="", n=3))
    return "\n".join(d) + "\n"


mutants = []
for s, (path, j, i, hook) in enumerate(releases):
    for v, (genre, drop, what) in enumerate((("ReleaseEmit", {j}, "delete the emitted release"),
                                             ("ReleaseRecord", {i}, f"delete self.{hook}(..)"),
                                             ("ReleaseBoth", {j, i}, f"delete the emitted release and self.{hook}(..)"))):
        mutants.append((f"m{s:03d}{v + 1}", path, j + 1, genre, f"{path}:{j + 1}: {what}", drop))
for s, (path, i, hook) in enumerate(fields):
    mutants.append((f"m{500 + s:03d}2", path, i + 1, "FieldRecord", f"{path}:{i + 1}: delete self.{hook}(..)", {i}))

with open(os.path.join(out, "sample.tsv"), "w", encoding="utf-8") as allf, \
        open(os.path.join(out, "index.tsv"), "w", encoding="utf-8") as idx:
    for n, (mid, path, line, genre, what, drop) in enumerate(mutants):
        row = [mid, path, str(line), genre, what]
        mine = n % shards
        allf.write("\t".join(row + [str(mine), str(n)]) + "\n")
        if mine == shard:
            with open(os.path.join(out, mid + ".diff"), "w", encoding="utf-8") as d:
                d.write(diff(path, drop))
            idx.write("\t".join(row) + "\n")
print(f"{len(releases)} release sites, {len(fields)} field-record sites, {len(mutants)} mutants; this shard ({shard}/{shards}) takes {sum(1 for _ in open(os.path.join(out, 'index.tsv'), encoding='utf-8'))}")
