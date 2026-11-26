#!/usr/bin/env python3
"""Turn cut_acquires.py output into one Rocq file per frame whose acquires are all placed.

  acquire_goals.py <work dir> <label>=<cut file> [<label>=<cut file>...]

Writes <work>/A_<n>.v (`Example c : acquire_ok <alloc> <rc_inc> [body] [certificates] = true.
Proof. vm_compute. reflexivity. Qed.`) and <work>/index.tsv, one line per frame that has a
named acquire:

  <goal, or `-` when an acquire is not placed> <label> <position> <function> <acquires>
  <placed> <the acquires not placed, or `-`>
"""
import re
import sys

CERT = re.compile(r"^([LCG])(\d+):(\d+):(\d+)$")
KIND = {"L": "HLocal", "C": "HCell", "G": "HGlobal"}


def main():
    work = sys.argv[1]
    rows, n = [], 0
    for arg in sys.argv[2:]:
        label, path = arg.split("=", 1)
        for line in open(path, encoding="utf-8"):
            t = line.split()
            if not t or t[0] == "UNREAD":
                continue
            if t[0] != "FRAME" or line.count("|") != 3:
                sys.exit(f"{path}: not a cutter line: {line.strip()[:80]}")
            head, certs, left, body = line.split("|")
            _, pos, func, alloc, inc, frac = head.split()
            placed, total = frac.split("/")
            left = " ".join(left.split())
            if not all(x.isdigit() for x in (pos, func, alloc, placed, total)) or not all(b.isdigit() for b in body.split()):
                sys.exit(f"{path}: a FRAME line is not numbers")
            if placed != total or not inc.isdigit():
                rows.append(("-", label, pos, func, total, placed, left))
                continue
            cs = []
            for c in certs.split():
                m = CERT.match(c)
                if not m:
                    sys.exit(f"{path}: not a certificate: {c}")
                cs.append(f"({KIND[m.group(1)]} {m.group(2)}, {m.group(3)}%nat, {m.group(4)}%nat)")
            name = f"A_{n:05d}"
            n += 1
            with open(f"{work}/{name}.v", "w", encoding="utf-8") as f:
                f.write("From AlmideTrust Require Import GuardWindow GuardFresh GuardPath GuardAcquire.\n"
                        "From Stdlib Require Import ZArith List.\nImport ListNotations.\nOpen Scope Z_scope.\n\n"
                        f"Example c : acquire_ok {alloc} {inc}\n  [{'; '.join(body.split())}]\n  [{'; '.join(cs)}] = true.\n"
                        "Proof. vm_compute. reflexivity. Qed.\n")
            rows.append((name, label, pos, func, total, placed, left))
    with open(f"{work}/index.tsv", "w", encoding="utf-8") as f:
        for r in rows:
            f.write("\t".join(r) + "\n")
    print(f"{len(rows)} frame(s) with a named acquire; {n} with every acquire placed, written as statements")


if __name__ == "__main__":
    main()
