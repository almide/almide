#!/usr/bin/env python3
"""Probe only: the guarded-access windows the cutter proposes, per program,
under the unmutated compiler and under one mutant.

  probe-emitter-windows.py <out-dir> <fixtures-list> --baseline
  probe-emitter-windows.py <out-dir> <fixtures-list> <mutant-id>

`<name>.windows` beside each module is the output of `almide-anchor-check
--windows` (a `WINDOW <function> <access> <bytes...>` line per proposal, then
`ACCESSES <n>`). The cutter only proposes: whether a byte string is accepted
is the kernel's verdict, taken once per distinct byte string by the job that
follows the shards. So this script records byte strings, not verdicts.

--baseline writes windows/base.tsv (program, accesses, proposals) and
windows/base.wins.tsv (program, function, access, bytes).

For a mutant it reads the programs whose module differs from the baseline's
(`<name>.cmp` starts with 1) and writes windows/<id>.tsv (program, baseline
accesses and proposals, the mutant's accesses and proposals, or `-` where the
cutter gave no ACCESSES line) and windows/<id>.wins.tsv (the mutant's
proposals for those programs).
"""
import os
import sys

out, lst, what = sys.argv[1], sys.argv[2], sys.argv[3]
os.makedirs(os.path.join(out, "windows"), exist_ok=True)
progs = [l.strip() for l in open(lst, encoding="utf-8") if l.strip()]


def name_of(f):
    return f.replace("/", "_")


def read(d, f):
    """(accesses or None, [(function, access, bytes)]) of one program's dump."""
    p = os.path.join(out, d, name_of(f) + ".windows")
    acc, wins = None, []
    try:
        for line in open(p, encoding="utf-8", errors="replace"):
            t = line.split()
            if not t:
                continue
            if t[0] == "WINDOW" and len(t) > 3:
                wins.append((t[1], t[2], " ".join(t[3:])))
            elif t[0] == "ACCESSES" and len(t) == 2 and t[1].isdigit():
                acc = int(t[1])
    except OSError:
        pass
    return acc, wins


if what == "--baseline":
    blind = 0
    with open(os.path.join(out, "windows", "base.tsv"), "w", encoding="utf-8") as rows, \
            open(os.path.join(out, "windows", "base.wins.tsv"), "w", encoding="utf-8") as wf:
        for f in progs:
            acc, wins = read("base", f)
            if acc is None:
                blind += 1
            rows.write(f"{f}\t{'-' if acc is None else acc}\t{len(wins)}\n")
            for fn, a, bs in wins:
                wf.write(f"{f}\t{fn}\t{a}\t{bs}\n")
    tot = sum(int(l.split("\t")[2]) for l in open(os.path.join(out, "windows", "base.tsv"), encoding="utf-8"))
    print(f"baseline windows: {tot} proposal(s) over {len(progs)} program(s), {blind} without an ACCESSES line")
    sys.exit(1 if blind else 0)

mid = what
changed = 0
with open(os.path.join(out, "windows", mid + ".tsv"), "w", encoding="utf-8") as rows, \
        open(os.path.join(out, "windows", mid + ".wins.tsv"), "w", encoding="utf-8") as wf:
    for f in progs:
        try:
            cmp_ = open(os.path.join(out, "work", name_of(f) + ".cmp"), encoding="utf-8").read().split()
        except OSError:
            continue
        if not cmp_ or cmp_[0] != "1":
            continue
        changed += 1
        b_acc, b_wins = read("base", f)
        m_acc, m_wins = read("work", f)
        rows.write("\t".join([f, "-" if b_acc is None else str(b_acc), str(len(b_wins)),
                              "-" if m_acc is None else str(m_acc), str(len(m_wins))]) + "\n")
        for fn, a, bs in m_wins:
            wf.write(f"{f}\t{fn}\t{a}\t{bs}\n")
print(f"{mid}: windows read for {changed} program(s) whose module changed")
