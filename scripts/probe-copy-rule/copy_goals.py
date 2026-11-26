#!/usr/bin/env python3
"""Turn cut_copies.py output into one Rocq file per distinct statement.

  copy_goals.py <work dir> <label>=<cut file> [<label>=<cut file>...]

A statement is a window's bytes, the allocator's index, a side (the bytes written or the
bytes read) and one guessed block. Writes <work>/C_<n>.v (`Example c : copy_vc <alloc>
[bytes] <true: written | false: read> (WLen <local> <v> | WFresh <k>). Proof. copy_auto.
Qed.`) and <work>/index.tsv (goal or `-`, label, function, instruction, side, guess): one
line per guess, and one line with `-` for a side that has no guess. <work>/nowindow.tsv
lists the copies that have no window.
"""
import re
import sys

GUESS = re.compile(r"^(?:L(\d+)\.(\d+)|F(\d+))$")


def why(g):
    m = GUESS.match(g)
    if not m:
        sys.exit(f"not a guess: {g}")
    return f"(WLen {m.group(1)} {m.group(2)})" if m.group(1) is not None else f"(WFresh {m.group(3)})"


def main():
    work = sys.argv[1]
    names, rows, none = {}, [], []
    for arg in sys.argv[2:]:
        label, path = arg.split("=", 1)
        for line in open(path, encoding="utf-8"):
            t = line.split()
            if not t:
                continue
            if t[0] == "NOWINDOW":
                none.append((label, t[1], t[2]))
                continue
            if t[0] != "COPY" or line.count("|") != 3:
                sys.exit(f"{path}: not a cutter line: {line.strip()[:80]}")
            head, wr, rd, raw = line.split("|")
            _, func, at, alloc = head.split()
            bs = raw.split()
            if not all(x.isdigit() for x in [func, at, alloc] + bs):
                sys.exit(f"{path}: a COPY line is not numbers")
            for side, gs in (("written", wr.split()), ("read", rd.split())):
                gs = [g for g in gs if g != "-"]
                if not gs:
                    rows.append(("-", label, func, at, side, "-"))
                for g in gs:
                    key = (alloc, side, g, ";".join(bs))
                    if key not in names:
                        names[key] = f"C_{len(names):05d}"
                        with open(f"{work}/{names[key]}.v", "w", encoding="utf-8") as f:
                            f.write("From AlmideTrust Require Import GuardWindow GuardCopy.\n"
                                    "From Stdlib Require Import ZArith List Lia.\n"
                                    "Import ListNotations.\nOpen Scope Z_scope.\n\n"
                                    f"Example c : copy_vc {alloc} [{key[3]}] "
                                    f"{'true' if side == 'written' else 'false'} {why(g)}.\n"
                                    "Proof. copy_auto. Qed.\n")
                    rows.append((names[key], label, func, at, side, g))
    with open(f"{work}/index.tsv", "w", encoding="utf-8") as f:
        for r in rows:
            f.write("\t".join(r) + "\n")
    with open(f"{work}/nowindow.tsv", "w", encoding="utf-8") as f:
        for r in none:
            f.write("\t".join(r) + "\n")
    copies = {(r[1], r[2], r[3]) for r in rows}
    print(f"{len(copies)} copy window(s), two sides each, as {len(names)} distinct statement(s); "
          f"{len(none)} copy(ies) with no window")


if __name__ == "__main__":
    main()
