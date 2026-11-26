#!/usr/bin/env python3
"""Read the kernel's answers for one replay's copy windows and say whether the copy rule
tells the two modules apart.

  copy_verdicts.py <goals dir> <replay> <label>=<sha> <label>=<sha> <sides.tsv> <summary.tsv>

<goals dir> holds index.tsv (goal, label, function, instruction, side) from copy_goals.py,
nowindow.tsv, and per goal `<goal>.rc` (`<exit code> <seconds>`: 0 the statement's file
compiled, 124 it ran out of time, anything else it did not compile). Writes one line per
side to <sides.tsv> and appends one line per module to <summary.tsv>.
"""
import sys


def main():
    goals, replay = sys.argv[1], sys.argv[2]
    shas = dict(a.split("=", 1) for a in sys.argv[3:5])
    sides_path, summary_path = sys.argv[5], sys.argv[6]
    answer = {}
    count = {label: {"copies": set(), "sides": 0, "proved": 0, "not proved": 0, "out of time": 0} for label in shas}
    lines = []
    for line in open(f"{goals}/index.tsv", encoding="utf-8"):
        goal, label, func, at, side = line.rstrip("\n").split("\t")
        if goal not in answer:
            rc, secs = open(f"{goals}/{goal}.rc", encoding="utf-8").read().split()
            answer[goal] = ("proved" if rc == "0" else "out of time" if rc == "124" else "not proved", secs)
        v, secs = answer[goal]
        c = count[label]
        c["copies"].add((func, at))
        c["sides"] += 1
        c[v] += 1
        lines.append(f"{replay}\t{label}\t{func}\t{at}\t{side}\t{goal}\t{v}\t{secs}")
    nowin = {label: 0 for label in shas}
    for line in open(f"{goals}/nowindow.tsv", encoding="utf-8"):
        label = line.split("\t")[0]
        nowin[label] += 1
    with open(sides_path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + ("\n" if lines else ""))
    with open(summary_path, "a", encoding="utf-8") as f:
        for label in shas:
            c = count[label]
            f.write(f"{replay}\t{label}\t{shas[label]}\t{len(c['copies'])}\t{c['sides']}\t{c['proved']}\t"
                    f"{c['not proved']}\t{c['out of time']}\n")
    for label in shas:
        c = count[label]
        print(f"{label} (sha {shas[label]}): {len(c['copies'])} copy window(s), {c['sides']} side(s): "
              f"{c['proved']} proved, {c['not proved']} not proved, {c['out of time']} out of time; "
              f"{nowin[label]} copy(ies) with no window")
    for l in lines:
        t = l.split("\t")
        if t[6] != "proved":
            print(f"  {t[1]}: function {t[2]}, instruction {t[3]}, bytes {t[4]}: {t[6]} ({t[5]}, {t[7]} s)")
    base, bug = count["unpatched"], count["defect"]
    timed = base["out of time"] + bug["out of time"]
    if bug["not proved"] > base["not proved"] and len(bug["copies"]) == len(base["copies"]) and not timed:
        print(f"VERDICT {replay}: the copy rule tells them apart: {bug['not proved'] - base['not proved']} "
              f"side(s) proved in the unpatched module are not proved in the defect module")
    elif timed:
        print(f"VERDICT {replay}: no verdict: {timed} statement(s) ran out of time")
    else:
        print(f"VERDICT {replay}: the copy rule does not tell them apart "
              f"(not proved: unpatched {base['not proved']}, defect {bug['not proved']}; "
              f"copies: {len(base['copies'])} and {len(bug['copies'])})")


if __name__ == "__main__":
    main()
