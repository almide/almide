#!/usr/bin/env python3
"""Read the kernel's answers for the acquire rule, frame by frame.

  acquire_verdicts.py <goals dir> <frames.tsv> [--timeout-code N]
  acquire_verdicts.py <goals dir> <frames.tsv> --replay <name> <label>=<sha> <label>=<sha> <summary.tsv>

<goals dir> holds index.tsv (goal or `-`, label, position, function, acquires, placed, not
placed) from acquire_goals.py and per goal `<goal>.rc` (`<exit code> <seconds>`: 0 the
statement's file compiled, so the kernel evaluated the statement to true). A frame is

  accepted        every named acquire has its own rc_inc call, and the kernel says so
                  (`A_` statement: `acquire_ok ... = true`)
  too few calls   the kernel counted fewer rc_inc calls in the body than the frame has
                  named acquires (`S_` statement: `acquire_short ... = true`)
  refused         a statement was written and the kernel does not accept it
  out of time     the kernel did not answer in time
  not placed      the cutter found no call for some acquire and the count does not settle
                  it (nothing to ask the kernel)

Writes one line per frame to <frames.tsv> and prints totals per label. With --replay, the
two labels must be `unpatched` and `defect`: the rule tells the modules apart when both have
as many frames with a named acquire, none ran out of time, and fewer are accepted in the
defect module; the verdict says for how many of the defect module's other frames the kernel
itself counted too few calls.
"""
import collections
import sys


def main():
    goals, out_path = sys.argv[1], sys.argv[2]
    rest = sys.argv[3:]
    timeout_code = "124"
    if "--timeout-code" in rest:
        timeout_code = rest[rest.index("--timeout-code") + 1]
    out, totals, acq = [], collections.defaultdict(collections.Counter), collections.Counter()
    for line in open(f"{goals}/index.tsv", encoding="utf-8"):
        goal, label, pos, func, total, placed, left = line.rstrip("\n").split("\t")
        secs = "0"
        if goal == "-":
            v = "not placed"
        else:
            rc, secs = open(f"{goals}/{goal}.rc", encoding="utf-8").read().split()
            good = "too few calls" if goal.startswith("S_") else "accepted"
            v = good if rc == "0" else "out of time" if rc == timeout_code else "refused"
        totals[label][v] += 1
        acq[label] += int(total)
        out.append((label, pos, func, total, placed, v, left, secs))
    with open(out_path, "w", encoding="utf-8") as f:
        for r in out:
            f.write("\t".join(r) + "\n")
    for label in totals:
        t = totals[label]
        print(f"{label}: {sum(t.values())} frame(s) with a named acquire ({acq[label]} acquire(s)): {t['accepted']} accepted, "
              f"{t['too few calls']} with too few calls by the kernel's count, {t['refused']} refused, "
              f"{t['not placed']} with an acquire not placed, {t['out of time']} out of time")
    if "--replay" not in rest:
        return
    i = rest.index("--replay")
    replay = rest[i + 1]
    shas = dict(a.split("=", 1) for a in rest[i + 2:i + 4])
    with open(rest[i + 4], "a", encoding="utf-8") as f:
        for label in shas:
            t = totals[label]
            f.write(f"{replay}\t{label}\t{shas[label]}\t{sum(t.values())}\t{acq[label]}\t{t['accepted']}\t"
                    f"{t['too few calls']}\t{t['refused']}\t{t['not placed']}\t{t['out of time']}\n")
    for r in out:
        if r[5] != "accepted":
            print(f"  {r[0]}: function {r[2]}: {r[5]} ({r[4]} of {r[3]} acquire(s) placed; not placed: {r[6]})")
    b, d = totals["unpatched"], totals["defect"]
    if b["out of time"] + d["out of time"]:
        print(f"VERDICT {replay}: no verdict: a frame ran out of time")
    elif sum(b.values()) == sum(d.values()) and d["accepted"] < b["accepted"]:
        print(f"VERDICT {replay}: the acquire rule tells them apart: {b['accepted']} frame(s) accepted in the unpatched "
              f"module, {d['accepted']} in the defect module, of {sum(b.values())} with a named acquire; in "
              f"{d['too few calls']} of the defect module's frames the kernel counted too few rc_inc calls")
    else:
        print(f"VERDICT {replay}: the acquire rule does not tell them apart (frames accepted: unpatched {b['accepted']} of "
              f"{sum(b.values())}, defect {d['accepted']} of {sum(d.values())})")


if __name__ == "__main__":
    main()
