#!/usr/bin/env python3
"""Turn cut_paths.py output into one Rocq file per distinct statement.

  path_goals.py <work dir> <label>=<cut file> [<label>=<cut file>...]

A statement is a path's bytes, the bytes after its access, the allocator's index, a range
(the bytes written or the bytes read) and one guessed block. Writes <work>/P_<n>.v
(`Example c : path_vc <alloc> [bytes] [rest] <true: written | false: read> (WLen <local> <v>
| WFresh <k>). Proof. path_auto. Qed.`) and <work>/index.tsv, one line per guess:

  <goal, or `-` for a range with no guess, or `h` for a header word> <label> <function>
  <instruction> <kind> <signature> <range: written | read> <guess> <events, as the cutter
  wrote them>

<work>/nopath.tsv lists the accesses that have no path.
"""
import re
import sys

GUESS = re.compile(r"^(?:L(\d+)\.(\d+)|F(\d+))$")
EVENT = re.compile(r"^\d+:[awx]\d+$")


def why(g):
    m = GUESS.match(g)
    if not m:
        sys.exit(f"not a guess: {g}")
    return f"(WLen {m.group(1)} {m.group(2)})" if m.group(1) is not None else f"(WFresh {m.group(3)})"


def numbers(xs, where):
    if not all(x.isdigit() for x in xs):
        sys.exit(f"{where}: not numbers")
    return "; ".join(xs)


def main():
    work = sys.argv[1]
    names, rows, none = {}, [], []
    for arg in sys.argv[2:]:
        label, path = arg.split("=", 1)
        for line in open(path, encoding="utf-8"):
            t = line.split()
            if not t or t[0] == "UNREAD":
                continue
            if t[0] == "NOPATH":
                none.append((label, t[1], t[2], t[3]))
                continue
            if t[0] != "ACCESS" or line.count("|") != 5:
                sys.exit(f"{path}: not a cutter line: {line.strip()[:80]}")
            head, wr, rd, raw, rest, evs = line.split("|")
            evs = " ".join(evs.split())
            if evs != "-" and not all(EVENT.match(e) for e in evs.split()):
                sys.exit(f"{path}: not a cutter line: {line.strip()[:80]}")
            _, func, at, kind, alloc, sig, _first = head.split()
            if kind not in ("load", "store", "copy", "fill") or not re.fullmatch(r"[0-9a-f]{12}", sig):
                sys.exit(f"{path}: not a cutter line: {line.strip()[:80]}")
            numbers([func, at, alloc], path)
            bs = numbers(raw.split(), path)
            rs = "" if rest.split() == ["-"] else numbers(rest.split(), path)
            for side, gs in (("written", wr.split()), ("read", rd.split())):
                if gs == ["."]:
                    continue
                if gs == ["h"]:
                    rows.append(("h", label, func, at, kind, sig, side, "-", evs))
                    continue
                gs = [g for g in gs if g != "-"]
                if not gs:
                    rows.append(("-", label, func, at, kind, sig, side, "-", evs))
                for g in gs:
                    key = (alloc, side, g, bs, rs)
                    if key not in names:
                        names[key] = f"P_{len(names):05d}"
                        with open(f"{work}/{names[key]}.v", "w", encoding="utf-8") as f:
                            f.write("From AlmideTrust Require Import GuardWindow GuardFresh GuardPath.\n"
                                    "From Stdlib Require Import ZArith List Lia.\n"
                                    "Import ListNotations.\nOpen Scope Z_scope.\n\n"
                                    f"Example c : path_vc {alloc}\n  [{bs}]\n  [{rs}]\n  "
                                    f"{'true' if side == 'written' else 'false'} {why(g)}.\n"
                                    "Proof. path_auto. Qed.\n")
                    rows.append((names[key], label, func, at, kind, sig, side, g, evs))
    with open(f"{work}/index.tsv", "w", encoding="utf-8") as f:
        for r in rows:
            f.write("\t".join(r) + "\n")
    with open(f"{work}/nopath.tsv", "w", encoding="utf-8") as f:
        for r in none:
            f.write("\t".join(r) + "\n")
    ranges = {(r[1], r[2], r[3], r[6]) for r in rows}
    header = {(r[1], r[2], r[3], r[6]) for r in rows if r[0] == "h"}
    print(f"{len(ranges)} range(s) of {len({x[:3] for x in ranges})} access(es): {len(header)} header word(s), "
          f"the rest as {len(names)} distinct statement(s); {len(none)} access(es) with no path")


if __name__ == "__main__":
    main()
