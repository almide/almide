#!/usr/bin/env python3
"""Totals over a corpus for the path rule and the acquire rule.

  corpus_sum.py <dir>

<dir> holds modules.tsv (label, fixture, status, sha256, bytes), goals/index.tsv,
goals/nopath.tsv and goals/P_*.rc, ranges.tsv (path_verdicts.py's output), acquire/index.tsv
and acquire/[AS]_*.rc, frames.tsv (acquire_verdicts.py's output).

An `.rc` file is `<exit code> <seconds>`: 0 the kernel compiled the statement, 124 it ran out
of time, 97 the shard's time budget ended before it was reached, 98 its shard's answers are
missing, anything else the statement did not compile. path_verdicts.py and
acquire_verdicts.py read 97 and 98 as a refusal; here a range or a frame that is not proved
and has such a statement is counted apart, as `not asked`, so that a lost shard never reads
as the kernel's refusal.
"""
import collections
import glob
import os
import sys

NOT_ASKED = ("97", "98")


def pct(n, d):
    return f"{100.0 * n / d:.2f}%" if d else "-"


def codes(pattern):
    out = {}
    for path in glob.glob(pattern):
        rc, secs = open(path, encoding="utf-8").read().split()
        out[os.path.basename(path)[:-3]] = (rc, int(secs))
    return out


def by_code(rcs):
    c = collections.Counter()
    for rc, _ in rcs.values():
        c["compiled" if rc == "0" else "out of time" if rc == "124" else "not reached in the time budget" if rc == "97"
          else "answers missing" if rc == "98" else "did not compile"] += 1
    return c


def main():
    d = sys.argv[1]
    mods = [l.rstrip("\n").split("\t") for l in open(f"{d}/modules.tsv", encoding="utf-8")]
    status = collections.Counter(m[2] for m in mods)
    print(f"== modules: {len(mods)} listed; " + ", ".join(f"{n} {s}" for s, n in sorted(status.items())))

    # ---- the path rule
    prc = codes(f"{d}/goals/P_*.rc")
    tried = collections.defaultdict(list)       # range -> its statements
    for line in open(f"{d}/goals/index.tsv", encoding="utf-8"):
        goal, label, func, at, kind, sig, side, guess, _ = line.rstrip("\n").split("\t")
        if goal not in ("-", "h"):
            tried[(label, func, at, side)].append(goal)
    ranges = collections.Counter()
    by_kind = collections.defaultdict(collections.Counter)
    how_proved = collections.Counter()
    per_access = collections.defaultdict(list)
    kind_of = {}
    for line in open(f"{d}/ranges.tsv", encoding="utf-8"):
        label, func, at, kind, sig, side, v, how, _ = line.rstrip("\n").split("\t")
        if v in ("not proved", "out of time") and any(prc.get(g, ("98", 0))[0] in NOT_ASKED for g in tried[(label, func, at, side)]):
            v = "not asked"
        ranges[v] += 1
        by_kind[f"{kind} {side}"][v] += 1
        if v == "proved":
            how_proved["a block the path allocated" if how.startswith("F") else "a block's length word"] += 1
        per_access[(label, func, at)].append(v)
        kind_of[(label, func, at)] = kind
    nopath = 0
    for line in open(f"{d}/goals/nopath.tsv", encoding="utf-8"):
        label, func, at, kind = line.rstrip("\n").split("\t")
        per_access[(label, func, at)].append("no path")
        kind_of[(label, func, at)] = kind
        nopath += 1

    kinds = collections.Counter(kind_of.values())
    n_acc = len(per_access)
    print(f"== accesses: {n_acc} ({', '.join(f'{k} {kinds[k]}' for k in ('load', 'store', 'copy', 'fill'))}); "
          f"loads and stores together {kinds['load'] + kinds['store']}")
    total = sum(ranges.values())
    order = ("proved", "not carried", "header", "not proved", "no guess", "out of time", "not asked")
    print(f"== path rule, per range ({total} range(s); a bulk copy has two, the bytes written and the bytes read; "
          f"{nopath} access(es) more have no path):")
    for v in order:
        print(f"   {v:<12} {ranges[v]:>8}  {pct(ranges[v], total)}")
    print("   proved ranges, by the block the statement names: " + ", ".join(f"{k} {n}" for k, n in sorted(how_proved.items())))
    for k in sorted(by_kind):
        t = by_kind[k]
        print(f"   {k:<14} " + ", ".join(f"{v} {t[v]}" for v in order if t[v]))

    acc = collections.Counter()
    for key, vs in per_access.items():
        s = set(vs)
        if s == {"proved"}:
            acc["every range proved by the kernel"] += 1
        elif s == {"header"}:
            acc["a header word (not this rule's)"] += 1
        elif "proved" in s:
            acc["one range proved, the other not"] += 1
        elif "no path" in s:
            acc["no path"] += 1
        elif "not asked" in s:
            acc["not asked"] += 1
        elif "out of time" in s:
            acc["out of time"] += 1
        elif "not carried" in s:
            acc["proved but not carried"] += 1
        elif "no guess" in s and "not proved" not in s:
            acc["no guess"] += 1
        else:
            acc["not proved"] += 1
    print(f"== path rule, per access ({n_acc}):")
    for k, n in acc.most_common():
        print(f"   {k:<34} {n:>8}  {pct(n, n_acc)}")
    full = collections.Counter(kind_of[k] for k, vs in per_access.items() if set(vs) == {"proved"})
    print("   every range proved, by kind: " + ", ".join(f"{k} {full[k]} of {kinds[k]} ({pct(full[k], kinds[k])})" for k in ("load", "store", "copy", "fill")))

    pc = by_code(prc)
    print(f"== path rule, the kernel's work: {len(prc)} distinct statement(s): " + ", ".join(f"{n} {k}" for k, n in pc.most_common())
          + f"; {sum(s for _, s in prc.values())} s in all, the slowest {max((s for _, s in prc.values()), default=0)} s")

    # ---- the acquire rule
    arc = codes(f"{d}/acquire/[AS]_*.rc")
    goal_of = {}
    for line in open(f"{d}/acquire/index.tsv", encoding="utf-8"):
        goal, label, pos, func, total_a, placed, left = line.rstrip("\n").split("\t")
        goal_of[(label, pos)] = goal
    frames, acquires = collections.Counter(), 0
    for line in open(f"{d}/frames.tsv", encoding="utf-8"):
        label, pos, func, total_a, placed, v, left, _ = line.rstrip("\n").split("\t")
        g = goal_of.get((label, pos), "-")
        if v == "refused" and arc.get(g, ("98", 0))[0] in NOT_ASKED:
            v = "not asked"
        frames[v] += 1
        acquires += int(total_a)
    nf = sum(frames.values())
    print(f"== acquire rule: {nf} frame(s) with a named acquire ({acquires} acquire(s)):")
    for v in ("accepted", "too few calls", "refused", "not placed", "out of time", "not asked"):
        print(f"   {v:<14} {frames[v]:>8}  {pct(frames[v], nf)}")
    ac = by_code(arc)
    print(f"   the kernel's work: {len(arc)} statement(s): " + ", ".join(f"{n} {k}" for k, n in ac.most_common())
          + f"; {sum(s for _, s in arc.values())} s in all")


if __name__ == "__main__":
    main()
