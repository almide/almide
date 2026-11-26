#!/usr/bin/env python3
"""Turn cut_copies.py output into one Rocq file per distinct copy window.

  copy_goals.py <work dir> <label>=<cut file> [<label>=<cut file>...]

Writes <work>/C_<n>.v (`Example c : copy_vc [bytes] <true: written | false: read> <local>.
Proof. copy_auto. Qed.`), two per distinct window, and <work>/index.tsv (goal, label,
function, instruction, side) with two lines per copy; <work>/nowindow.tsv lists the copies
that have no window.
"""
import sys


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
            if t[0] != "COPY":
                sys.exit(f"{path}: not a cutter line: {line.strip()[:80]}")
            head, raw = line.split("|")
            _, func, at, hd, hs = head.split()
            bs = raw.split()
            if not all(x.isdigit() for x in [func, at, hd, hs] + bs):
                sys.exit(f"{path}: a COPY line is not numbers")
            for side, h in (("written", hd), ("read", hs)):
                key = (side, h, ";".join(bs))
                if key not in names:
                    names[key] = f"C_{len(names):05d}"
                    with open(f"{work}/{names[key]}.v", "w", encoding="utf-8") as f:
                        f.write("From AlmideTrust Require Import GuardWindow GuardCopy.\n"
                                "From Stdlib Require Import ZArith List Lia.\n"
                                "Import ListNotations.\nOpen Scope Z_scope.\n\n"
                                f"Example c : copy_vc [{key[2]}] {'true' if side == 'written' else 'false'} {h}.\n"
                                "Proof. copy_auto. Qed.\n")
                rows.append((names[key], label, func, at, side))
    with open(f"{work}/index.tsv", "w", encoding="utf-8") as f:
        for r in rows:
            f.write("\t".join(r) + "\n")
    with open(f"{work}/nowindow.tsv", "w", encoding="utf-8") as f:
        for r in none:
            f.write("\t".join(r) + "\n")
    print(f"{len(rows) // 2} copy window(s), two sides each, as {len(names)} distinct goal(s); {len(none)} copy(ies) with no window")


if __name__ == "__main__":
    main()
