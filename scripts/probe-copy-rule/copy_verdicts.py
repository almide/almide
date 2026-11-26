#!/usr/bin/env python3
"""Read the kernel's answers for copy windows, side by side.

  copy_verdicts.py <goals dir> <sides.tsv> [--timeout-code N]
  copy_verdicts.py <goals dir> <sides.tsv> --replay <name> <label>=<sha> <label>=<sha> <summary.tsv>

<goals dir> holds index.tsv (goal or `-`, label, function, instruction, side, guess) from
copy_goals.py, nowindow.tsv, and per goal `<goal>.rc` (`<exit code> <seconds>`: 0 the
statement's file compiled; the timeout code (124, or the one given) it ran out of time;
anything else it did not compile). A side is `proved` when one of its guesses is; `out of
time` when none is proved and one ran out of time; `no guess` when the cutter named no
block; else `not proved`. Writes one line per side to <sides.tsv> (label, function,
instruction, side, verdict, the guess proved or the guesses tried, seconds) and prints
totals per label. With --replay, the two labels must be `unpatched` and `defect`: a line per
module is appended to <summary.tsv> and a VERDICT line says whether the rule tells the two
modules apart.
"""
import collections
import sys


def main():
    goals, sides_path = sys.argv[1], sys.argv[2]
    rest = sys.argv[3:]
    timeout_code = "124"
    if "--timeout-code" in rest:
        timeout_code = rest[rest.index("--timeout-code") + 1]
    answer = {}

    def ask(goal):
        if goal not in answer:
            rc, secs = open(f"{goals}/{goal}.rc", encoding="utf-8").read().split()
            answer[goal] = ("proved" if rc == "0" else "out of time" if rc == timeout_code else "not proved", int(secs))
        return answer[goal]

    sides = collections.OrderedDict()
    for line in open(f"{goals}/index.tsv", encoding="utf-8"):
        goal, label, func, at, side, guess = line.rstrip("\n").split("\t")
        sides.setdefault((label, func, at, side), []).append((goal, guess))
    out, totals = [], collections.defaultdict(collections.Counter)
    copies = collections.defaultdict(set)
    both = collections.defaultdict(lambda: collections.defaultdict(list))
    for (label, func, at, side), tries in sides.items():
        copies[label].add((func, at))
        if tries == [("-", "-")]:
            v, how, secs = "no guess", "-", 0
        else:
            got = [(g, guess) + ask(g) for g, guess in tries]
            secs = sum(x[3] for x in got)
            proved = [x for x in got if x[2] == "proved"]
            if proved:
                v, how = "proved", proved[0][1]
            elif any(x[2] == "out of time" for x in got):
                v, how = "out of time", " ".join(x[1] for x in got)
            else:
                v, how = "not proved", " ".join(x[1] for x in got)
        totals[label][v] += 1
        both[label][(func, at)].append(v)
        out.append((label, func, at, side, v, how, str(secs)))
    with open(sides_path, "w", encoding="utf-8") as f:
        for r in out:
            f.write("\t".join(r) + "\n")
    nowin = collections.Counter(line.split("\t")[0] for line in open(f"{goals}/nowindow.tsv", encoding="utf-8"))
    labels = list(totals) or sorted(nowin)
    for label in labels:
        t = totals[label]
        whole = sum(1 for vs in both[label].values() if all(v == "proved" for v in vs))
        print(f"{label}: {len(copies[label])} copy window(s), {sum(t.values())} side(s): {t['proved']} proved, "
              f"{t['not proved']} not proved, {t['no guess']} with no guess, {t['out of time']} out of time; "
              f"{whole} copy(ies) with both sides proved; {nowin[label]} copy(ies) with no window")
    if "--replay" not in rest:
        return
    i = rest.index("--replay")
    replay = rest[i + 1]
    shas = dict(a.split("=", 1) for a in rest[i + 2:i + 4])
    summary_path = rest[i + 4]
    with open(summary_path, "a", encoding="utf-8") as f:
        for label in shas:
            t = totals[label]
            f.write(f"{replay}\t{label}\t{shas[label]}\t{len(copies[label])}\t{sum(t.values())}\t{t['proved']}\t"
                    f"{t['not proved'] + t['no guess']}\t{t['out of time']}\n")
    for r in out:
        if r[4] != "proved":
            print(f"  {r[0]}: function {r[1]}, instruction {r[2]}, bytes {r[3]}: {r[4]} ({r[5]}; {r[6]} s)")
    base, bug = totals["unpatched"], totals["defect"]
    lost = (bug["not proved"] + bug["no guess"]) - (base["not proved"] + base["no guess"])
    timed = base["out of time"] + bug["out of time"]
    same = len(copies["unpatched"]) == len(copies["defect"])
    if timed:
        print(f"VERDICT {replay}: no verdict: {timed} side(s) ran out of time")
    elif lost > 0 and same:
        print(f"VERDICT {replay}: the copy rule tells them apart: {lost} side(s) proved in the unpatched module "
              f"are not proved in the defect module")
    else:
        print(f"VERDICT {replay}: the copy rule does not tell them apart (sides proved: unpatched "
              f"{base['proved']}, defect {bug['proved']}; copies: {len(copies['unpatched'])} and "
              f"{len(copies['defect'])})")


if __name__ == "__main__":
    main()
