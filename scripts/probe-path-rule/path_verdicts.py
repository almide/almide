#!/usr/bin/env python3
"""Read the kernel's answers for paths, range by range.

  path_verdicts.py <goals dir> <ranges.tsv> [--timeout-code N]
  path_verdicts.py <goals dir> <ranges.tsv> --replay <name> <label>=<sha> <label>=<sha> <summary.tsv>

<goals dir> holds index.tsv (goal or `-` or `h`, label, function, instruction, kind,
signature, range, guess, events) from path_goals.py, nopath.tsv, and per goal `<goal>.rc`
(`<exit code> <seconds>`: 0 the statement's file compiled; the timeout code (124, or the one
given) it ran out of time; anything else it did not compile).

A statement against a block the path allocated (`F<k>`) stands on its own. A statement
against a length word (`L<local>.<v>`) is about the word as memory number v had it, so it
counts only when the word is CARRIED to the access: nothing but the allocator and stores and
bulk writes lies between (the cutter's events with a larger number), and every one of those
writes is itself a range that counts as proved (the largest set of ranges that supports
itself: when a write runs, every write before it lay in a block's payload, and the heap
model says a length word is not in any payload). A range is

  proved        one of its guesses is proved, and carried if it is about a length word
  not carried   a guess is proved, but something between may have written the length word
  out of time   none is proved and one ran out of time
  no guess      the cutter named no block
  header        the access is a word in a block's first twelve bytes (not this rule's)
  not proved    otherwise

Writes one line per range to <ranges.tsv> (label, function, instruction, kind, signature,
range, verdict, the guess that counted or the guesses tried, seconds) and prints totals per
label.

With --replay, the two labels must be `unpatched` and `defect`. The same access is found in
both modules by its function, kind, signature (the access and the eight instructions before
it) and range; a signature may occur several times (one per call site inlined). The rule
tells the modules apart when some signature occurs equally often in both, none of its ranges
ran out of time, and fewer of them are proved in the defect module.
"""
import collections
import re
import sys

LEN = re.compile(r"^L\d+\.(\d+)$")


def main():
    goals, out_path = sys.argv[1], sys.argv[2]
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

    ranges, events = collections.OrderedDict(), {}
    for line in open(f"{goals}/index.tsv", encoding="utf-8"):
        goal, label, func, at, kind, sig, side, guess, evs = line.rstrip("\n").split("\t")
        key = (label, func, at, kind, sig, side)
        ranges.setdefault(key, []).append((goal, guess))
        events[key] = [] if evs == "-" else [(int(e.split(":")[0]), e.split(":")[1][0], e.split(":")[1][1:])
                                             for e in evs.split()]
    # what each proved guess needs: None when it stands on its own, else the writes between
    got = {}
    for key, tries in ranges.items():
        if tries[0][0] in ("-", "h"):
            continue
        got[key] = []
        for goal, guess in tries:
            v, secs = ask(goal)
            need = None
            m = LEN.match(guess)
            if v == "proved" and m:
                later = [e for e in events[key] if e[0] > int(m.group(1))]
                need = "x" if any(tag == "x" for _, tag, _ in later) else {i for _, tag, i in later if tag == "w"}
            got[key].append((guess, v, secs, need))
    written = collections.defaultdict(list)     # (label, function, instruction) -> its written ranges
    for key in ranges:
        if key[5] == "written":
            written[key[:3]].append(key)
    # The largest set that supports itself: when a write runs, every write before it lay in a
    # payload, so the length word its own statement read is still there (induction on time).
    cand = {(key, guess): need for key, gs in got.items() for guess, v, _, need in gs
            if v == "proved" and need != "x"}
    changed = True
    while changed:
        changed = False
        ok = {key for key, _ in cand}
        for (key, guess), need in list(cand.items()):
            if need is None:
                continue
            for i in need:
                ws = written[(key[0], key[1], i)]
                if not ws or not all(w in ok for w in ws):
                    del cand[(key, guess)]
                    changed = True
                    break
    solid = {}
    for key, gs in got.items():
        for guess, _, _, _ in gs:
            if (key, guess) in cand:
                solid[key] = guess
                break
    out, totals = [], collections.defaultdict(collections.Counter)
    by_sig = collections.defaultdict(lambda: collections.defaultdict(list))
    for key, tries in ranges.items():
        label, func, at, kind, sig, side = key
        secs = 0
        if tries == [("-", "-")]:
            v, how = "no guess", "-"
        elif tries == [("h", "-")]:
            v, how = "header", "-"
        else:
            gs = got[key]
            secs = sum(x[2] for x in gs)
            if key in solid:
                v, how = "proved", solid[key]
            elif any(x[1] == "proved" for x in gs):
                v, how = "not carried", " ".join(x[0] for x in gs if x[1] == "proved")
            elif any(x[1] == "out of time" for x in gs):
                v, how = "out of time", " ".join(x[0] for x in gs)
            else:
                v, how = "not proved", " ".join(x[0] for x in gs)
        totals[label][v] += 1
        by_sig[label][(func, kind, sig, side)].append((at, v, how))
        out.append((label, func, at, kind, sig, side, v, how, str(secs)))
    with open(out_path, "w", encoding="utf-8") as f:
        for r in out:
            f.write("\t".join(r) + "\n")
    nopath = collections.Counter(line.split("\t")[0] for line in open(f"{goals}/nopath.tsv", encoding="utf-8"))
    for label in list(totals) or sorted(nopath):
        t = totals[label]
        print(f"{label}: {sum(t.values())} range(s): {t['proved']} proved, {t['not carried']} proved but not carried, "
              f"{t['not proved']} not proved, {t['no guess']} with no guess, {t['header']} header word(s), "
              f"{t['out of time']} out of time; {nopath[label]} access(es) with no path")
    if "--replay" not in rest:
        return
    i = rest.index("--replay")
    replay = rest[i + 1]
    shas = dict(a.split("=", 1) for a in rest[i + 2:i + 4])
    summary_path = rest[i + 4]
    with open(summary_path, "a", encoding="utf-8") as f:
        for label in shas:
            t = totals[label]
            f.write(f"{replay}\t{label}\t{shas[label]}\t{sum(t.values())}\t{t['proved']}\t{t['not carried']}\t"
                    f"{t['not proved']}\t{t['no guess']}\t{t['header']}\t{t['out of time']}\t{nopath[label]}\n")
    base, bug = by_sig["unpatched"], by_sig["defect"]
    lost, timed, uneven = [], 0, 0
    for key in sorted(set(base) | set(bug)):
        b, d = base.get(key, []), bug.get(key, [])
        if len(b) != len(d):
            uneven += 1
            continue
        if any(v == "out of time" for _, v, _ in b + d):
            timed += 1
            continue
        nb, nd = sum(v == "proved" for _, v, _ in b), sum(v == "proved" for _, v, _ in d)
        if nd < nb:
            lost.append((key, len(b), nb, nd, [(at, v, how) for at, v, how in d if v != "proved"]))
    for key, n, nb, nd, bad in lost:
        func, kind, sig, side = key
        print(f"  function {func}, {kind} {sig}, bytes {side}: {n} occurrence(s); proved {nb} in the unpatched "
              f"module, {nd} in the defect module")
        for at, v, how in bad:
            print(f"    defect module, instruction {at}: {v} ({how})")
    note = f"{uneven} signature(s) occur unequally often and {timed} have a range out of time (neither counted)"
    if lost:
        print(f"VERDICT {replay}: the path rule tells them apart: {sum(x[2] - x[3] for x in lost)} range(s) of "
              f"{len(lost)} signature(s) proved in the unpatched module are not proved in the defect module; {note}")
    else:
        tb, td = totals["unpatched"], totals["defect"]
        print(f"VERDICT {replay}: the path rule does not tell them apart (ranges proved: unpatched {tb['proved']} "
              f"of {sum(tb.values())}, defect {td['proved']} of {sum(td.values())}); {note}")


if __name__ == "__main__":
    main()
