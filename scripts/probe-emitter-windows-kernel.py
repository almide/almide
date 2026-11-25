#!/usr/bin/env python3
"""Probe only: the kernel's verdict on every distinct window byte string the
shards recorded, and per mutant the accepted windows before and after.

  probe-emitter-windows-kernel.py strings <shards-dir> <out-dir>
  probe-emitter-windows-kernel.py dump <shards-dir> <out-dir>
  probe-emitter-windows-kernel.py join <shards-dir> <out-dir>

<shards-dir> holds one directory per shard artifact, each with
emut/windows/{base.tsv, base.wins.tsv, <id>.tsv, <id>.wins.tsv}
(probe-emitter-windows.py writes them).

`strings` writes <out-dir>/strings.tsv (a code per distinct byte string) and
<out-dir>/eval/Eval_NNNN.v, which make the proof assistant compute
`window_ok` on each string; the caller compiles them and keeps each one's
output beside it (Eval_NNNN.out).

`dump` reads those outputs and writes <out-dir>/evals.tsv (code, true |
false) and <out-dir>/dump, the input of proofs/check-shipped-windows.sh, for
the strings computed true: one module per string, so that its verdicts.tsv
is a verdict per string, each a goal the kernel closed.

`join` reads <out-dir>/work/verdicts.tsv and writes
  held-windows-programs.tsv  one row per (mutant, program whose module changed)
  held-windows.tsv           one row per mutant that changed a module
A window counts as accepted only if the kernel accepted its byte string.
"""
import glob
import os
import sys

mode, shards, out = sys.argv[1], sys.argv[2], sys.argv[3]
os.makedirs(out, exist_ok=True)
dirs = sorted(d for d in glob.glob(os.path.join(shards, "*", "emut", "windows")) if os.path.isdir(d))
if not dirs:
    sys.exit("no shard recorded windows")


def wins(path):
    """[(program, function, access, bytes)] of a .wins.tsv file."""
    rows = []
    try:
        for line in open(path, encoding="utf-8"):
            t = line.rstrip("\n").split("\t")
            if len(t) == 4:
                rows.append(tuple(t))
    except OSError:
        pass
    return rows


CHUNK = 400
HEAD = """From AlmideTrust Require Import GuardWindow.
From Stdlib Require Import ZArith.
From Stdlib Require Import List.
Import ListNotations.
Open Scope Z_scope.
"""

if mode == "strings":
    seen = {}
    for d in dirs:
        for p in sorted(glob.glob(os.path.join(d, "*.wins.tsv"))):
            for _, _, _, bs in wins(p):
                seen.setdefault(bs, f"u{len(seen):06d}")
    os.makedirs(os.path.join(out, "eval"), exist_ok=True)
    items = list(seen.items())
    with open(os.path.join(out, "strings.tsv"), "w", encoding="utf-8") as sf:
        for bs, uid in items:
            sf.write(f"{uid}\t{bs}\n")
    for k in range(0, len(items), CHUNK):
        with open(os.path.join(out, "eval", f"Eval_{k // CHUNK:04d}.v"), "w", encoding="utf-8") as f:
            f.write(HEAD)
            for bs, uid in items[k:k + CHUNK]:
                f.write(f"(* {uid} *)\nEval vm_compute in (window_ok [{';'.join(bs.split())}]).\n")
    print(f"{len(seen)} distinct window byte string(s) from {len(dirs)} shard(s)")
    sys.exit(0)

if mode == "dump":
    items = [line.rstrip("\n").split("\t") for line in open(os.path.join(out, "strings.tsv"), encoding="utf-8")]
    got = []
    for k in range(0, len(items), CHUNK):
        p = os.path.join(out, "eval", f"Eval_{k // CHUNK:04d}.out")
        vals = [l.split("=", 1)[1].strip() for l in open(p, encoding="utf-8") if l.strip().startswith("= ")]
        want = len(items[k:k + CHUNK])
        if len(vals) != want or any(v not in ("true", "false") for v in vals):
            sys.exit(f"{p}: {len(vals)} value(s) for {want} string(s)")
        got += vals
    with open(os.path.join(out, "evals.tsv"), "w", encoding="utf-8") as ef, \
            open(os.path.join(out, "dump"), "w", encoding="utf-8") as df:
        for (uid, bs), v in zip(items, got):
            ef.write(f"{uid}\t{v}\n")
            if v == "true":
                df.write(f"MODULE {uid}\nWINDOW 0 0 {bs}\nACCESSES 1\n")
    print(f"{len(items)} string(s): {got.count('true')} computed true, {got.count('false')} false")
    sys.exit(0)

uid_of = {}
for line in open(os.path.join(out, "strings.tsv"), encoding="utf-8"):
    uid, bs = line.rstrip("\n").split("\t")
    uid_of[bs] = uid
evals = dict(line.rstrip("\n").split("\t") for line in open(os.path.join(out, "evals.tsv"), encoding="utf-8"))
verdict = {}
for line in open(os.path.join(out, "work", "verdicts.tsv"), encoding="utf-8"):
    t = line.rstrip("\n").split("\t")
    verdict[t[0]] = t[-1]
# A string computed true must also be a goal the kernel closed.
odd = [u for u, v in evals.items() if v == "true" and verdict.get(u) != "ok"]
if odd:
    sys.exit(f"{len(odd)} string(s) computed true but not closed as goals, e.g. {odd[:3]}")
ok = {bs for bs, uid in uid_of.items() if verdict.get(uid) == "ok"}
print(f"{len(uid_of)} distinct byte string(s): {len(ok)} accepted by the kernel, {len(uid_of) - len(ok)} refused")

# The baseline is the same compiler in every shard: read it once, and say so
# if a shard disagrees.
base_rows = {}
base_ok = {}
for k, d in enumerate(dirs):
    text = open(os.path.join(d, "base.tsv"), encoding="utf-8").read()
    if k == 0:
        first = text
        for line in text.splitlines():
            f, acc, n = line.split("\t")
            base_rows[f] = (acc, int(n))
        for f, _, _, bs in wins(os.path.join(d, "base.wins.tsv")):
            base_ok[f] = base_ok.get(f, 0) + (bs in ok)
    elif text != first:
        print(f"NOTE: the baseline of {d} differs from the first shard's")
corpus_ok = sum(base_ok.values())
corpus_prop = sum(n for _, n in base_rows.values())
print(f"baseline: {len(base_rows)} program(s), {corpus_prop} proposal(s), {corpus_ok} accepted")

per_prog, per_mut = [], []
for d in dirs:
    for p in sorted(glob.glob(os.path.join(d, "m*.tsv"))):
        if p.endswith(".wins.tsv"):
            continue
        mid = os.path.basename(p)[:-4]
        got = {}
        refused = 0
        for f, _, _, bs in wins(p[:-4] + ".wins.tsv"):
            got[f] = got.get(f, 0) + (bs in ok)
            refused += bs not in ok
        changed = blind = fewer = more = b_tot = m_tot = 0
        for line in open(p, encoding="utf-8"):
            f, b_acc, b_prop, m_acc, m_prop = line.rstrip("\n").split("\t")
            changed += 1
            b, m = base_ok.get(f, 0), got.get(f, 0)
            blind += m_acc == "-"
            fewer += m < b
            more += m > b
            b_tot += b
            m_tot += m
            per_prog.append((mid, f, b_acc, b_prop, b, m_acc, m_prop, m))
        if changed:
            per_mut.append((mid, changed, blind, b_tot, m_tot, fewer, more, refused, corpus_ok, corpus_ok - b_tot + m_tot))
per_mut.sort()
per_prog.sort()
with open(os.path.join(out, "held-windows-programs.tsv"), "w", encoding="utf-8") as fh:
    fh.write("id\tprogram\tbase_accesses\tbase_proposed\tbase_accepted\tmutant_accesses\tmutant_proposed\tmutant_accepted\n")
    for r in per_prog:
        fh.write("\t".join(str(x) for x in r) + "\n")
with open(os.path.join(out, "held-windows.tsv"), "w", encoding="utf-8") as fh:
    fh.write("id\tprograms_changed\tprograms_blind\tbase_accepted\tmutant_accepted\tprograms_fewer\tprograms_more\tproposals_refused\tcorpus_base\tcorpus_mutant\n")
    for r in per_mut:
        fh.write("\t".join(str(x) for x in r) + "\n")
below = sum(1 for r in per_mut if r[9] < r[8])
print(f"{len(per_mut)} mutant(s) changed a module; {below} leave the corpus with fewer accepted windows than the baseline's {corpus_ok}; "
      f"{sum(1 for r in per_mut if r[5])} lose a window in some program; {sum(1 for r in per_mut if r[2])} leave a module the cutter cannot read")
